/*
 * SpinorML Ltd 🚀 AGPL-3.0 License - https://spinorml.com/license
 */

use crate::frame::{Framerate, Resolution};
use bytes::Bytes;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Codec {
    H264,
    H265,
    Av1,
    Vp8,
    Vp9,
    Mjpeg,
}

#[derive(Debug, Clone)]
pub struct EncoderConfig {
    pub codec: Codec,
    pub resolution: Resolution,
    pub framerate: Framerate,
    pub bitrate_kbps: u32,
}

#[derive(Debug, Clone)]
pub struct DecoderConfig {
    pub codec: Codec,
}

#[derive(Debug, Clone)]
pub struct EncodedPacket {
    pub codec: Codec,
    pub pts: Duration,
    pub dts: Option<Duration>,
    pub is_keyframe: bool,
    pub data: Bytes,
}
