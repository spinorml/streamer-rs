/*
 * SpinorML Ltd 🚀 AGPL-3.0 License - https://spinorml.com/license
 */

use async_trait::async_trait;
use gstreamer as gst;
use gstreamer::prelude::*;

use crate::error::{Result, StreamError};
use crate::pipeline::{Pipeline, PipelineState};

use super::utils;

/// A GStreamer pipeline driven by a `gst-launch`-style description string.
pub struct GstPipeline {
    pipeline: gst::Pipeline,
    state: PipelineState,
}

#[async_trait]
impl Pipeline for GstPipeline {
    fn build(description: &str) -> Result<Self> {
        utils::init();

        let pipeline = gst::parse::launch(description)
            .map_err(|e| StreamError::Pipeline {
                message: e.to_string(),
            })?
            .downcast::<gst::Pipeline>()
            .map_err(|_| StreamError::Pipeline {
                message: "element is not a pipeline".into(),
            })?;

        Ok(Self {
            pipeline,
            state: PipelineState::Null,
        })
    }

    async fn play(&mut self) -> Result<()> {
        self.pipeline
            .set_state(gst::State::Playing)
            .map_err(|e| StreamError::Pipeline {
                message: e.to_string(),
            })?;
        self.state = PipelineState::Playing;
        Ok(())
    }

    async fn pause(&mut self) -> Result<()> {
        self.pipeline
            .set_state(gst::State::Paused)
            .map_err(|e| StreamError::Pipeline {
                message: e.to_string(),
            })?;
        self.state = PipelineState::Paused;
        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
        self.pipeline
            .set_state(gst::State::Null)
            .map_err(|e| StreamError::Pipeline {
                message: e.to_string(),
            })?;
        self.state = PipelineState::Null;
        Ok(())
    }

    fn state(&self) -> PipelineState {
        self.state
    }
}
