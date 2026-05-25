/*
 * SpinorML Ltd 🚀 AGPL-3.0 License - https://spinorml.com/license
 */

use bytes::Bytes;
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_video as gst_video;
use std::sync::Once;

use crate::codec::{Codec, EncoderConfig};
use crate::error::{Result, StreamError};
use crate::frame::{FrameData, PixelFormat, VideoFrame};

use super::frame::GstFrameData;

static GST_INIT: Once = Once::new();

pub fn init() {
    GST_INIT.call_once(|| {
        gst::init().expect("GStreamer initialisation failed");
    });
}

pub fn pixel_format_to_gst(fmt: PixelFormat) -> &'static str {
    match fmt {
        PixelFormat::I420 => "I420",
        PixelFormat::NV12 => "NV12",
        PixelFormat::Rgb => "RGB",
        PixelFormat::Rgba => "RGBA",
        PixelFormat::Bgr => "BGR",
        PixelFormat::Bgra => "BGRA",
        PixelFormat::Yuyv => "YUY2",
    }
}

pub fn gst_format_to_pixel(fmt: gst_video::VideoFormat) -> Option<PixelFormat> {
    match fmt {
        gst_video::VideoFormat::I420 => Some(PixelFormat::I420),
        gst_video::VideoFormat::Nv12 => Some(PixelFormat::NV12),
        gst_video::VideoFormat::Rgb => Some(PixelFormat::Rgb),
        gst_video::VideoFormat::Rgba => Some(PixelFormat::Rgba),
        gst_video::VideoFormat::Bgr => Some(PixelFormat::Bgr),
        gst_video::VideoFormat::Bgra => Some(PixelFormat::Bgra),
        gst_video::VideoFormat::Yuy2 => Some(PixelFormat::Yuyv),
        _ => None,
    }
}

pub fn codec_to_encoder_element(codec: Codec) -> Result<&'static str> {
    match codec {
        Codec::H264 => Ok("x264enc"),
        Codec::H265 => Ok("x265enc"),
        Codec::Vp8 => Ok("vp8enc"),
        Codec::Vp9 => Ok("vp9enc"),
        Codec::Av1 => Ok("av1enc"),
        Codec::Mjpeg => Ok("jpegenc"),
    }
}

pub fn codec_to_decoder_element(codec: Codec) -> Result<&'static str> {
    match codec {
        Codec::H264 => Ok("avdec_h264"),
        Codec::H265 => Ok("avdec_h265"),
        Codec::Vp8 => Ok("vp8dec"),
        Codec::Vp9 => Ok("vp9dec"),
        // No standard avdec element for AV1; requires av1dec plugin
        Codec::Av1 => Err(StreamError::UnsupportedCodec(codec)),
        Codec::Mjpeg => Ok("jpegdec"),
    }
}

pub fn encoder_config_to_caps(cfg: &EncoderConfig) -> gst::Caps {
    gst::Caps::builder("video/x-raw")
        .field("format", pixel_format_to_gst(PixelFormat::I420))
        .field("width", cfg.resolution.width as i32)
        .field("height", cfg.resolution.height as i32)
        .field(
            "framerate",
            gst::Fraction::new(cfg.framerate.num as i32, cfg.framerate.den as i32),
        )
        .build()
}

/// Wraps a GStreamer `Sample` (from an appsink) into a `VideoFrame<GstFrameData>`.
/// No CPU copy is made — the GstBuffer stays alive via ref-count.
pub fn gst_sample_to_gst_frame(sample: gst::Sample) -> Result<VideoFrame<GstFrameData>> {
    let caps = sample
        .caps()
        .ok_or_else(|| StreamError::Pipeline {
            message: "sample has no caps".into(),
        })?
        .to_owned();

    let info = gst_video::VideoInfo::from_caps(&caps).map_err(|_| StreamError::Pipeline {
        message: "failed to parse video caps".into(),
    })?;

    let buffer = sample
        .buffer()
        .ok_or_else(|| StreamError::Pipeline {
            message: "sample has no buffer".into(),
        })?
        .to_owned();

    let pts = buffer
        .pts()
        .map(|t| std::time::Duration::from_nanos(t.nseconds()))
        .unwrap_or_default();

    let dts = buffer
        .dts()
        .map(|t| std::time::Duration::from_nanos(t.nseconds()));

    let format = gst_format_to_pixel(info.format()).ok_or_else(|| StreamError::Pipeline {
        message: format!("unsupported GStreamer pixel format: {:?}", info.format()),
    })?;

    Ok(VideoFrame {
        width: info.width(),
        height: info.height(),
        format,
        pts,
        dts,
        data: GstFrameData::new(buffer, caps),
    })
}

/// Builds a `gst::Buffer` from a `VideoFrame<D>`.
///
/// If `D` is `GstFrameData`, the native buffer is returned directly (zero-copy).
/// For any other `D`, `to_bytes()` is awaited and a new buffer is allocated.
pub async fn video_frame_to_gst_buffer<D: FrameData>(
    frame: &VideoFrame<D>,
) -> Result<gst::Buffer> {
    if let Some(gst_data) = frame.data.as_any().downcast_ref::<GstFrameData>() {
        return Ok(gst_data.buffer.clone());
    }
    let bytes = frame.data.to_bytes().await?;
    bytes_to_gst_buffer(&bytes, frame.pts, frame.dts)
}

/// Allocates a new `gst::Buffer` and copies `bytes` into it.
pub fn bytes_to_gst_buffer(
    bytes: &Bytes,
    pts: std::time::Duration,
    dts: Option<std::time::Duration>,
) -> Result<gst::Buffer> {
    let mut buffer = gst::Buffer::with_size(bytes.len()).map_err(|_| StreamError::Pipeline {
        message: "failed to allocate GStreamer buffer".into(),
    })?;
    {
        let buf_ref = buffer.get_mut().unwrap();
        buf_ref.set_pts(gst::ClockTime::from_nseconds(pts.as_nanos() as u64));
        if let Some(dts) = dts {
            buf_ref.set_dts(gst::ClockTime::from_nseconds(dts.as_nanos() as u64));
        }
        let mut map = buf_ref
            .map_writable()
            .map_err(|_| StreamError::Pipeline {
                message: "failed to map buffer for writing".into(),
            })?;
        map.as_mut_slice().copy_from_slice(bytes);
    }
    Ok(buffer)
}

/// Blocks on the GStreamer bus until EOS or an error.
pub fn wait_for_eos_or_error(pipeline: &gst::Pipeline) -> Result<()> {
    let bus = pipeline.bus().unwrap();
    for msg in bus.iter_timed(gst::ClockTime::NONE) {
        use gst::MessageView;
        match msg.view() {
            MessageView::Eos(..) => return Ok(()),
            MessageView::Error(err) => {
                return Err(StreamError::Pipeline {
                    message: err.error().to_string(),
                })
            }
            _ => {}
        }
    }
    Ok(())
}
