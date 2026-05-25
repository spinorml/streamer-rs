/*
 * SpinorML Ltd 🚀 AGPL-3.0 License - https://spinorml.com/license
 */

use async_trait::async_trait;
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use std::marker::PhantomData;

use crate::error::{Result, StreamError};
use crate::frame::{FrameData, VideoFrame};
use crate::sink::VideoSink;

use super::utils;

/// Renders video to the system's default display via `autovideosink`.
///
/// Generic over `D: FrameData`. When `D` is `GstFrameData`, the native GstBuffer is
/// handed directly to appsrc (zero-copy). Any other `D` triggers a `to_bytes()` readback.
pub struct GstVideoSink<D: FrameData> {
    pipeline: gst::Pipeline,
    appsrc: gst_app::AppSrc,
    _phantom: PhantomData<fn(D)>,
}

impl<D: FrameData> GstVideoSink<D> {
    pub fn new(width: u32, height: u32, framerate_num: u32, framerate_den: u32) -> Result<Self> {
        utils::init();

        let pipeline = gst::parse::launch("appsrc name=src ! videoconvert ! autovideosink")
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
                message: "appsrc element 'src' not found".into(),
            })?;

        let caps = gst::Caps::builder("video/x-raw")
            .field("format", "I420")
            .field("width", width as i32)
            .field("height", height as i32)
            .field(
                "framerate",
                gst::Fraction::new(framerate_num as i32, framerate_den as i32),
            )
            .build();
        appsrc.set_caps(Some(&caps));
        appsrc.set_format(gst::Format::Time);

        Ok(Self { pipeline, appsrc, _phantom: PhantomData })
    }
}

#[async_trait]
impl<D: FrameData + 'static> VideoSink for GstVideoSink<D> {
    type Frame = D;

    async fn start(&mut self) -> Result<()> {
        self.pipeline
            .set_state(gst::State::Playing)
            .map_err(|e| StreamError::Pipeline {
                message: e.to_string(),
            })?;
        Ok(())
    }

    async fn write_frame(&mut self, frame: VideoFrame<D>) -> Result<()> {
        let buffer = utils::video_frame_to_gst_buffer(&frame).await?;
        self.appsrc
            .push_buffer(buffer)
            .map_err(|e| StreamError::Pipeline {
                message: e.to_string(),
            })?;
        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
        let _ = self.appsrc.end_of_stream();
        self.pipeline
            .set_state(gst::State::Null)
            .map_err(|e| StreamError::Pipeline {
                message: e.to_string(),
            })?;
        Ok(())
    }
}
