/*
 * SpinorML Ltd 🚀 AGPL-3.0 License - https://spinorml.com/license
 */

use std::sync::Arc;

use async_trait::async_trait;
use gstreamer as gst;
use gstreamer::prelude::*;
use tokio::sync::mpsc;

use crate::error::{Result, StreamError};
use crate::frame::VideoFrame;
use crate::source::VideoSource;

use super::frame::GstFrameData;
use super::source::GstVideoSource;

/// Merges multiple `GstVideoSource` instances into a single stream.
///
/// Each frame carries `source_id` identifying which source produced it.
/// `start()` and `stop()` are fanned out to all inner pipelines.
pub struct GstMultiVideoSource {
    pipelines: Vec<(Arc<str>, gst::Pipeline)>,
    rx: mpsc::Receiver<Result<VideoFrame<GstFrameData>>>,
    tx: mpsc::Sender<Result<VideoFrame<GstFrameData>>>,
}

impl GstMultiVideoSource {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel(128);
        Self { pipelines: Vec::new(), rx, tx }
    }

    /// Add a source with the given ID. Frames from this source will have
    /// `frame.source_id == Some(id)`.
    pub fn add(&mut self, id: impl Into<Arc<str>>, source: GstVideoSource) -> &mut Self {
        let id: Arc<str> = id.into();
        let tx = self.tx.clone();

        // Decompose the source into its pipeline and channel receiver, then
        // spawn a forwarder task that re-tags each frame with this source's ID.
        let (pipeline, mut rx) = source.into_parts();

        self.pipelines.push((id.clone(), pipeline));

        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Some(Ok(mut frame)) => {
                        frame.source_id = Some(id.clone());
                        if tx.send(Ok(frame)).await.is_err() {
                            break;
                        }
                    }
                    Some(Err(StreamError::EndOfStream)) => {
                        // One source finished — don't close the whole merged channel.
                        break;
                    }
                    Some(Err(e)) => {
                        let _ = tx.send(Err(e)).await;
                        break;
                    }
                    None => break,
                }
            }
        });

        self
    }
}

impl Default for GstMultiVideoSource {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl VideoSource for GstMultiVideoSource {
    type Frame = GstFrameData;

    async fn start(&mut self) -> Result<()> {
        for (id, pipeline) in &self.pipelines {
            pipeline
                .set_state(gst::State::Playing)
                .map_err(|e| StreamError::Pipeline { message: e.to_string() })?;

            let (res, cur, _) = pipeline.state(Some(gst::ClockTime::from_seconds(10)));
            if res.is_err() {
                return Err(StreamError::Pipeline {
                    message: format!(
                        "source '{id}' failed to reach Playing state (current: {cur:?})"
                    ),
                });
            }
        }
        Ok(())
    }

    async fn next_frame(&mut self) -> Result<Option<VideoFrame<GstFrameData>>> {
        match self.rx.recv().await {
            Some(Ok(frame)) => Ok(Some(frame)),
            Some(Err(StreamError::EndOfStream)) => Ok(None),
            Some(Err(e)) => Err(e),
            None => Ok(None),
        }
    }

    async fn stop(&mut self) -> Result<()> {
        for (_id, pipeline) in &self.pipelines {
            pipeline
                .set_state(gst::State::Null)
                .map_err(|e| StreamError::Pipeline { message: e.to_string() })?;
        }
        Ok(())
    }
}
