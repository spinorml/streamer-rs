/*
 * SpinorML Ltd 🚀 AGPL-3.0 License - https://spinorml.com/license
 */

use async_trait::async_trait;

use crate::codec::EncodedPacket;
use crate::error::Result;
use crate::frame::{FrameData, VideoFrame};

#[async_trait]
pub trait VideoDecoder: Send {
    type Frame: FrameData;

    async fn start(&mut self) -> Result<()>;
    async fn decode(&mut self, packet: EncodedPacket) -> Result<Vec<VideoFrame<Self::Frame>>>;
    async fn flush(&mut self) -> Result<Vec<VideoFrame<Self::Frame>>>;
    async fn stop(&mut self) -> Result<()>;
}
