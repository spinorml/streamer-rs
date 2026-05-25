/*
 * SpinorML Ltd 🚀 AGPL-3.0 License - https://spinorml.com/license
 */

use async_trait::async_trait;

use crate::error::Result;
use crate::frame::{FrameData, VideoFrame};

#[async_trait]
pub trait VideoSource: Send {
    type Frame: FrameData;

    async fn start(&mut self) -> Result<()>;
    /// Returns `None` on end of stream.
    async fn next_frame(&mut self) -> Result<Option<VideoFrame<Self::Frame>>>;
    async fn stop(&mut self) -> Result<()>;
}
