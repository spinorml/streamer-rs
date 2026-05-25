/*
 * SpinorML Ltd 🚀 AGPL-3.0 License - https://spinorml.com/license
 */

use async_trait::async_trait;
use bytes::Bytes;
use std::any::Any;
use std::time::Duration;

use crate::error::Result;

/// Holds the pixel data for a single video frame.
///
/// Implementations may keep data on the GPU (e.g. `GstFrameData`, `CudaFrameData`)
/// and only copy to CPU memory when `to_bytes()` is awaited.
#[async_trait]
pub trait FrameData: Send + Sync + Any + 'static {
    async fn to_bytes(&self) -> Result<Bytes>;
    fn as_any(&self) -> &dyn Any;
}

/// CPU-resident frame data backed by a `Bytes` buffer.
#[derive(Debug, Clone)]
pub struct BytesFrameData(pub Bytes);

#[async_trait]
impl FrameData for BytesFrameData {
    async fn to_bytes(&self) -> Result<Bytes> {
        Ok(self.0.clone())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

pub struct VideoFrame<D: FrameData> {
    pub width: u32,
    pub height: u32,
    pub format: PixelFormat,
    pub pts: Duration,
    pub dts: Option<Duration>,
    pub data: D,
}

impl<D: FrameData + Clone> Clone for VideoFrame<D> {
    fn clone(&self) -> Self {
        Self {
            width: self.width,
            height: self.height,
            format: self.format,
            pts: self.pts,
            dts: self.dts,
            data: self.data.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PixelFormat {
    I420,
    NV12,
    Rgb,
    Rgba,
    Bgr,
    Bgra,
    Yuyv,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Resolution {
    pub width: u32,
    pub height: u32,
}

impl Resolution {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }
}

/// Rational framerate, e.g. `Framerate { num: 30, den: 1 }` for 30 fps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Framerate {
    pub num: u32,
    pub den: u32,
}

impl Framerate {
    pub fn new(num: u32, den: u32) -> Self {
        Self { num, den }
    }
}
