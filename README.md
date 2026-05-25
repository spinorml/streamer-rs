# streamer-rs

**⚠️ Work in progress — not stable for production use. APIs will change without notice.**

Backend-agnostic async library for streaming audio and video in Rust.

Defines a clean trait layer over media backends so application code stays independent of any particular codec or hardware vendor. [GStreamer](https://gstreamer.freedesktop.org/) is the first implementation; FFmpeg and V4L2/VAAPI are planned.

## Features

| Feature | Description | Status |
|---|---|---|
| `gstreamer` | GStreamer backend (capture, encode, decode, pipeline) | ✅ Available |
| `ffmpeg` | FFmpeg backend | 🗓 Planned |
| `vaapi` | V4L2 / VAAPI hardware acceleration | 🗓 Planned |

The `gstreamer` feature is enabled by default.

## System requirements

The `gstreamer` feature requires GStreamer development libraries:

```bash
# Debian / Ubuntu
sudo apt install \
  libgstreamer1.0-dev \
  libgstreamer-plugins-base1.0-dev \
  gstreamer1.0-plugins-base \
  gstreamer1.0-plugins-good \
  gstreamer1.0-libav \
  gstreamer1.0-plugins-ugly

# macOS
brew install gstreamer gst-plugins-base gst-plugins-good
```

To build without any system dependencies:

```bash
cargo build --no-default-features
```

## Installation

```toml
[dependencies]
streamer-rs = "0.1"

# Without GStreamer (core traits only):
streamer-rs = { version = "0.1", default-features = false }
```

## Usage

### Capture from a camera

```rust
use streamer::{GstVideoSource, VideoSource};

#[tokio::main]
async fn main() -> streamer::Result<()> {
    let mut source = GstVideoSource::new("/dev/video0")?;
    source.start().await?;

    while let Some(frame) = source.next_frame().await? {
        // frame.data is a GstFrameData — no CPU copy until you call:
        let bytes = frame.data.to_bytes().await?;
        println!("frame {}x{} — {} bytes", frame.width, frame.height, bytes.len());
    }

    source.stop().await
}
```

### Encode frames

```rust
use streamer::{
    Codec, EncoderConfig, Framerate, GstVideoEncoder, GstVideoSource, Resolution,
    VideoEncoder, VideoSource,
};

#[tokio::main]
async fn main() -> streamer::Result<()> {
    let mut source = GstVideoSource::new("/dev/video0")?;
    let mut encoder = GstVideoEncoder::new(EncoderConfig {
        codec: Codec::H264,
        resolution: Resolution::new(1920, 1080),
        framerate: Framerate::new(30, 1),
        bitrate_kbps: 4000,
    })?;

    source.start().await?;
    encoder.start().await?;

    while let Some(frame) = source.next_frame().await? {
        // GstFrameData flows into GstVideoEncoder with no CPU copy
        let packets = encoder.encode(frame).await?;
        for pkt in packets {
            println!("keyframe={} size={}", pkt.is_keyframe, pkt.data.len());
        }
    }

    let remaining = encoder.flush().await?;
    encoder.stop().await?;
    source.stop().await
}
```

### Launch a GStreamer pipeline directly

```rust
use streamer::{GstPipeline, Pipeline};

#[tokio::main]
async fn main() -> streamer::Result<()> {
    let mut pipeline = GstPipeline::build(
        "videotestsrc ! videoconvert ! autovideosink"
    )?;

    pipeline.play().await?;
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    pipeline.stop().await
}
```

### CPU frames (`BytesFrameData`)

For pipelines that don't go through GStreamer, use `BytesFrameData`:

```rust
use bytes::Bytes;
use streamer::{BytesFrameData, FrameData, VideoFrame, PixelFormat};

let data = BytesFrameData(Bytes::from(vec![0u8; 1920 * 1080 * 3 / 2]));
let frame = VideoFrame {
    width: 1920,
    height: 1080,
    format: PixelFormat::I420,
    pts: std::time::Duration::ZERO,
    dts: None,
    data,
};
let bytes = frame.data.to_bytes().await?;
```

## Frame data and GPU memory

`VideoFrame<D: FrameData>` is generic over its pixel data. The `FrameData` trait has a single required method:

```rust
async fn to_bytes(&self) -> Result<Bytes>;
```

GStreamer decodes directly on the GPU when hardware acceleration is enabled. `GstFrameData` holds the native `gst::Buffer` without copying it to CPU memory until `to_bytes()` is called. When a `GstFrameData` frame is passed to `GstVideoEncoder` or `GstVideoSink`, the buffer is forwarded to GStreamer's appsrc natively — no roundtrip through CPU memory.

| Type | Where data lives | Copy on `to_bytes()` |
|---|---|---|
| `BytesFrameData` | CPU (`Bytes`) | None (clone is cheap) |
| `GstFrameData` | Where GStreamer left it (GPU / CPU) | Only if not already mapped |

## Implementing a new backend

1. Add a feature flag in `Cargo.toml`
2. Create `src/backends/<name>/` and implement the relevant traits:
   - `VideoSource` — produces frames
   - `VideoSink` — consumes frames
   - `VideoEncoder` — raw frames → compressed packets
   - `VideoDecoder` — compressed packets → raw frames
   - `VideoTransform` — frame-to-frame transform
   - `Pipeline` — full pipeline lifecycle
3. Define your `FrameData` type (or reuse `BytesFrameData`)
4. Gate the module in `src/backends/mod.rs` with `#[cfg(feature = "<name>")]`
5. Re-export from `src/lib.rs` under the same gate

No changes to core traits are required.

## License

Copyright © SpinorML Ltd. Licensed under the [GNU Affero General Public License v3.0](LICENSE).
