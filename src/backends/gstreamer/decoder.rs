/*
 * SpinorML Ltd 🚀 AGPL-3.0 License - https://spinorml.com/license
 */

use async_trait::async_trait;
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use tokio::sync::mpsc;

use crate::codec::EncodedPacket;
use crate::decoder::VideoDecoder;
use crate::error::{Result, StreamError};
use crate::frame::VideoFrame;

use super::frame::GstFrameData;
use super::utils;

/// Decodes compressed packets into `VideoFrame<GstFrameData>` — no CPU copy until `.to_bytes()`.
pub struct GstVideoDecoder {
    pipeline: gst::Pipeline,
    appsrc: gst_app::AppSrc,
    rx: mpsc::Receiver<Result<VideoFrame<GstFrameData>>>,
}

impl GstVideoDecoder {
    pub fn new(codec: crate::codec::Codec) -> Result<Self> {
        utils::init();

        let decoder_element = utils::codec_to_decoder_element(codec)?;
        let pipeline_str = format!(
            "appsrc name=src ! {decoder_element} ! videoconvert ! video/x-raw,format=I420 ! appsink name=sink sync=false"
        );

        let pipeline = gst::parse::launch(&pipeline_str)
            .map_err(|e| StreamError::Pipeline {
                message: e.to_string(),
            })?
            .downcast::<gst::Pipeline>()
            .map_err(|_| StreamError::Pipeline {
                message: "element is not a pipeline".into(),
            })?;

        let appsrc = pipeline
            .by_name("src")
            .and_downcast::<gst_app::AppSrc>()
            .ok_or_else(|| StreamError::Pipeline {
                message: "appsrc 'src' not found".into(),
            })?;

        let appsink = pipeline
            .by_name("sink")
            .and_downcast::<gst_app::AppSink>()
            .ok_or_else(|| StreamError::Pipeline {
                message: "appsink 'sink' not found".into(),
            })?;

        appsrc.set_format(gst::Format::Time);

        let (tx, rx) = mpsc::channel::<Result<VideoFrame<GstFrameData>>>(32);

        appsink.set_callbacks(
            gst_app::AppSinkCallbacks::builder()
                .new_sample({
                    let tx = tx.clone();
                    move |sink| {
                        let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                        let result = utils::gst_sample_to_gst_frame(sample)
                            .map_err(|_| gst::FlowError::Error);
                        let _ = tx.blocking_send(result.map_err(|_| StreamError::Pipeline {
                            message: "frame conversion error".into(),
                        }));
                        Ok(gst::FlowSuccess::Ok)
                    }
                })
                .eos({
                    move |_| {
                        let _ = tx.blocking_send(Err(StreamError::EndOfStream));
                    }
                })
                .build(),
        );

        Ok(Self { pipeline, appsrc, rx })
    }
}

#[async_trait]
impl VideoDecoder for GstVideoDecoder {
    type Frame = GstFrameData;

    async fn start(&mut self) -> Result<()> {
        self.pipeline
            .set_state(gst::State::Playing)
            .map_err(|e| StreamError::Pipeline {
                message: e.to_string(),
            })?;
        Ok(())
    }

    async fn decode(&mut self, packet: EncodedPacket) -> Result<Vec<VideoFrame<GstFrameData>>> {
        let buffer = utils::bytes_to_gst_buffer(&packet.data, packet.pts, packet.dts)?;
        self.appsrc
            .push_buffer(buffer)
            .map_err(|e| StreamError::Pipeline {
                message: e.to_string(),
            })?;

        let mut frames = Vec::new();
        while let Ok(result) = self.rx.try_recv() {
            match result {
                Ok(frame) => frames.push(frame),
                Err(StreamError::EndOfStream) => break,
                Err(e) => return Err(e),
            }
        }
        Ok(frames)
    }

    async fn flush(&mut self) -> Result<Vec<VideoFrame<GstFrameData>>> {
        let _ = self.appsrc.end_of_stream();
        let mut frames = Vec::new();
        while let Some(result) = self.rx.recv().await {
            match result {
                Ok(frame) => frames.push(frame),
                Err(StreamError::EndOfStream) => break,
                Err(e) => return Err(e),
            }
        }
        Ok(frames)
    }

    async fn stop(&mut self) -> Result<()> {
        self.pipeline
            .set_state(gst::State::Null)
            .map_err(|e| StreamError::Pipeline {
                message: e.to_string(),
            })?;
        Ok(())
    }
}
