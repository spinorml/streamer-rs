/*
 * SpinorML Ltd 🚀 AGPL-3.0 License - https://spinorml.com/license
 */

use async_trait::async_trait;

use crate::error::Result;
use crate::frame::{FrameData, VideoFrame};

#[async_trait]
pub trait VideoSink: Send {
    type Frame: FrameData;

    async fn start(&mut self) -> Result<()>;
    async fn write_frame(&mut self, frame: VideoFrame<Self::Frame>) -> Result<()>;
    async fn stop(&mut self) -> Result<()>;
}
