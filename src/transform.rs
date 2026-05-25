/*
 * SpinorML Ltd 🚀 AGPL-3.0 License - https://spinorml.com/license
 */

use async_trait::async_trait;

use crate::error::Result;
use crate::frame::{FrameData, VideoFrame};

#[async_trait]
pub trait VideoTransform: Send {
    type Input: FrameData;
    type Output: FrameData;

    async fn process(&mut self, frame: VideoFrame<Self::Input>) -> Result<VideoFrame<Self::Output>>;
}
