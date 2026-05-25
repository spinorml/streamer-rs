/*
 * SpinorML Ltd 🚀 AGPL-3.0 License - https://spinorml.com/license
 */

use async_trait::async_trait;
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use std::marker::PhantomData;
use tokio::sync::mpsc;

use crate::codec::{EncodedPacket, EncoderConfig};
use crate::encoder::VideoEncoder;
use crate::error::{Result, StreamError};
use crate::frame::{FrameData, VideoFrame};

use super::utils;

/// Encodes video frames using a GStreamer encode pipeline.
///
/// Generic over `D: FrameData`. When `D` is `GstFrameData`, the native GstBuffer is
/// pushed directly to appsrc (zero-copy). Any other `D` triggers a `to_bytes()` readback.
pub struct GstVideoEncoder<D: FrameData> {
    pipeline: gst::Pipeline,
    appsrc: gst_app::AppSrc,
    rx: mpsc::Receiver<Result<EncodedPacket>>,
    _config: EncoderConfig,
    _phantom: PhantomData<fn(D)>,
}

impl<D: FrameData> GstVideoEncoder<D> {
    pub fn new(config: EncoderConfig) -> Result<Self> {
        utils::init();

        let encoder_element = utils::codec_to_encoder_element(config.codec)?;
        let pipeline_str = format!(
            "appsrc name=src ! videoconvert ! {encoder_element} ! appsink name=sink sync=false"
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

        let input_caps = utils::encoder_config_to_caps(&config);
        appsrc.set_caps(Some(&input_caps));
        appsrc.set_format(gst::Format::Time);

        let (tx, rx) = mpsc::channel::<Result<EncodedPacket>>(32);
        let codec = config.codec;

        appsink.set_callbacks(
            gst_app::AppSinkCallbacks::builder()
                .new_sample({
                    let tx = tx.clone();
                    move |sink| {
                        let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                        let buffer = sample.buffer().ok_or(gst::FlowError::Error)?;

                        let pts = buffer
                            .pts()
                            .map(|t| std::time::Duration::from_nanos(t.nseconds()))
                            .unwrap_or_default();
                        let dts = buffer
                            .dts()
                            .map(|t| std::time::Duration::from_nanos(t.nseconds()));
                        let is_keyframe =
                            !buffer.flags().contains(gst::BufferFlags::DELTA_UNIT);

                        let map =
                            buffer.map_readable().map_err(|_| gst::FlowError::Error)?;
                        let packet = EncodedPacket {
                            codec,
                            pts,
                            dts,
                            is_keyframe,
                            data: bytes::Bytes::copy_from_slice(map.as_slice()),
                        };
                        let _ = tx.blocking_send(Ok(packet));
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

        Ok(Self { pipeline, appsrc, rx, _config: config, _phantom: PhantomData })
    }
}

#[async_trait]
impl<D: FrameData + 'static> VideoEncoder for GstVideoEncoder<D> {
    type Frame = D;

    async fn start(&mut self) -> Result<()> {
        self.pipeline
            .set_state(gst::State::Playing)
            .map_err(|e| StreamError::Pipeline {
                message: e.to_string(),
            })?;
        Ok(())
    }

    async fn encode(&mut self, frame: VideoFrame<D>) -> Result<Vec<EncodedPacket>> {
        let buffer = utils::video_frame_to_gst_buffer(&frame).await?;
        self.appsrc
            .push_buffer(buffer)
            .map_err(|e| StreamError::Pipeline {
                message: e.to_string(),
            })?;

        let mut packets = Vec::new();
        while let Ok(result) = self.rx.try_recv() {
            match result {
                Ok(packet) => packets.push(packet),
                Err(StreamError::EndOfStream) => break,
                Err(e) => return Err(e),
            }
        }
        Ok(packets)
    }

    async fn flush(&mut self) -> Result<Vec<EncodedPacket>> {
        let _ = self.appsrc.end_of_stream();
        let mut packets = Vec::new();
        while let Some(result) = self.rx.recv().await {
            match result {
                Ok(packet) => packets.push(packet),
                Err(StreamError::EndOfStream) => break,
                Err(e) => return Err(e),
            }
        }
        Ok(packets)
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
