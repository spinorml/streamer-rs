/*
 * SpinorML Ltd 🚀 AGPL-3.0 License - https://spinorml.com/license
 */

use async_trait::async_trait;

use crate::codec::EncodedPacket;
use crate::error::Result;
use crate::frame::{FrameData, VideoFrame};

#[async_trait]
pub trait VideoEncoder: Send {
    type Frame: FrameData;

    async fn start(&mut self) -> Result<()>;
    async fn encode(&mut self, frame: VideoFrame<Self::Frame>) -> Result<Vec<EncodedPacket>>;
    async fn flush(&mut self) -> Result<Vec<EncodedPacket>>;
    async fn stop(&mut self) -> Result<()>;
}
