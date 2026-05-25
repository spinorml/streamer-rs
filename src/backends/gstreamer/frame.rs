/*
 * SpinorML Ltd 🚀 AGPL-3.0 License - https://spinorml.com/license
 */

use async_trait::async_trait;
use bytes::Bytes;
use gstreamer as gst;
use std::any::Any;

use crate::error::{Result, StreamError};
use crate::frame::FrameData;

/// GStreamer-native frame data. Holds a ref-counted `gst::Buffer` and its caps,
/// keeping the data where GStreamer left it (GPU or CPU) until `to_bytes()` is called.
///
/// Clone is cheap — GStreamer buffers are internally ref-counted.
#[derive(Clone)]
pub struct GstFrameData {
    pub(super) buffer: gst::Buffer,
    /// Retained for format negotiation in the zero-copy GPU path.
    #[allow(dead_code)]
    pub(super) caps: gst::Caps,
}

impl GstFrameData {
    pub fn new(buffer: gst::Buffer, caps: gst::Caps) -> Self {
        Self { buffer, caps }
    }
}

#[async_trait]
impl FrameData for GstFrameData {
    /// Maps the underlying GstBuffer to CPU-accessible memory and copies it into a `Bytes`.
    async fn to_bytes(&self) -> Result<Bytes> {
        let map = self.buffer.map_readable().map_err(|_| StreamError::Pipeline {
            message: "failed to map GstBuffer for reading".into(),
        })?;
        Ok(Bytes::copy_from_slice(map.as_slice()))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
