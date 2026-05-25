/*
 * SpinorML Ltd 🚀 AGPL-3.0 License - https://spinorml.com/license
 */

pub mod backends;
pub mod codec;
pub mod decoder;
pub mod encoder;
pub mod error;
pub mod frame;
pub mod pipeline;
pub mod sink;
pub mod source;
pub mod transform;

pub use codec::{Codec, DecoderConfig, EncodedPacket, EncoderConfig};
pub use decoder::VideoDecoder;
pub use encoder::VideoEncoder;
pub use error::{Result, StreamError};
pub use frame::{BytesFrameData, FrameData, Framerate, PixelFormat, Resolution, VideoFrame};
pub use pipeline::{Pipeline, PipelineState};
pub use sink::VideoSink;
pub use source::VideoSource;
pub use transform::VideoTransform;

#[cfg(feature = "gstreamer")]
pub use backends::gstreamer::{
    GstFrameData, GstPipeline, GstVideoDecoder, GstVideoEncoder, GstVideoSink, GstVideoSource,
};
