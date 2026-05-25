# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

`streamer-rs` is an async Rust library for streaming audio and video. It defines a backend-agnostic trait layer with GStreamer as the first implementation. FFmpeg and V4L2/VAAPI are planned follow-on backends.

## Commands

```bash
# Core (no system deps required)
cargo build --no-default-features
cargo test --no-default-features

# With GStreamer backend (requires system libs — see below)
cargo build
cargo build --features gstreamer
cargo test --features gstreamer
cargo test <test_name> --features gstreamer

cargo clippy --features gstreamer -- -D warnings
cargo fmt
```

### GStreamer system dependency

The `gstreamer` feature requires:
```bash
sudo apt install libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev \
  gstreamer1.0-plugins-base gstreamer1.0-plugins-good
```

## Architecture

```
src/
  lib.rs          — re-exports all public types, traits, and backend structs
  error.rs        — StreamError (#[non_exhaustive]), Result<T> alias
  frame.rs        — FrameData trait, BytesFrameData, VideoFrame<D>, PixelFormat, Resolution, Framerate
  codec.rs        — Codec (#[non_exhaustive]), EncoderConfig, DecoderConfig, EncodedPacket
  source.rs       — VideoSource trait (associated type Frame: FrameData)
  sink.rs         — VideoSink trait
  encoder.rs      — VideoEncoder trait
  decoder.rs      — VideoDecoder trait
  transform.rs    — VideoTransform trait (associated types Input, Output)
  pipeline.rs     — Pipeline trait + PipelineState enum
  backends/
    gstreamer/    — #[cfg(feature = "gstreamer")]
      frame.rs      GstFrameData — wraps gst::Buffer + gst::Caps, zero-copy until to_bytes()
      utils.rs      gst::init() (Once), caps builders, gst_sample_to_gst_frame, video_frame_to_gst_buffer
      source.rs     GstVideoSource  — produces VideoFrame<GstFrameData>, no CPU copy in callback
      sink.rs       GstVideoSink<D> — generic; native path for GstFrameData, to_bytes() fallback
      encoder.rs    GstVideoEncoder<D> — generic; native path for GstFrameData, to_bytes() fallback
      decoder.rs    GstVideoDecoder — produces VideoFrame<GstFrameData>
      pipeline.rs   GstPipeline     — gst-launch-style string via gst::parse::launch()
```

### Key design decisions

- **`FrameData` trait**: `VideoFrame<D: FrameData>` — data stays where the backend left it (GPU, DMA-BUF, etc.) until `to_bytes().await` is called. All traits use an associated type `type Frame: FrameData` so each impl declares what it produces/consumes.
- **Zero-copy GPU path**: `FrameData::as_any()` enables runtime downcasting. `GstVideoEncoder<D>` and `GstVideoSink<D>` check `frame.data.as_any().downcast_ref::<GstFrameData>()` — if it matches, the native `gst::Buffer` is handed directly to appsrc; otherwise `to_bytes().await` is called (CPU fallback).
- **`GstFrameData` is Clone**: `gst::Buffer` is internally ref-counted, so clone is a cheap bump. `VideoFrame<D: Clone>` also becomes `Clone`.
- **GStreamer threading**: GStreamer callbacks run on a non-tokio OS thread. All backends use `tokio::sync::mpsc` to bridge the GStreamer callback thread to the async API. Never spawn GStreamer loops as tokio tasks.
- **Backpressure**: All mpsc channels are bounded (capacity 32). Producers block on `blocking_send`; consumers use `.try_recv()` for non-blocking drain or `.recv().await` for flush.
- **EOS signalling**: `next_frame` returns `Ok(None)` on end-of-stream; `flush` drains until `EndOfStream` sentinel arrives on the channel.
- **`#[non_exhaustive]`**: `Codec`, `PixelFormat`, and `StreamError` are all non-exhaustive so downstream doesn't break when new variants are added.
- **Exhaustive codec matching inside the crate**: `codec_to_encoder_element` / `codec_to_decoder_element` in `utils.rs` use exhaustive matches (no `_`) so adding a new `Codec` variant forces a compile error there.

### Adding a new backend (FFmpeg, V4L2/VAAPI, etc.)

1. Add a feature flag in `Cargo.toml`
2. Create `src/backends/<name>/` mirroring the GStreamer layout
3. Implement the same traits (`VideoSource`, `VideoEncoder`, etc.)
4. Gate the module in `src/backends/mod.rs` with `#[cfg(feature = "<name>")]`
5. Re-export from `src/lib.rs` under the same feature gate
6. No changes to core traits required

## Rust Conventions

**Error handling**
- Define library errors with `thiserror`; never expose `anyhow::Error` in public API
- Use `?` for propagation; avoid `unwrap()`/`expect()` outside of tests
- Mark public error enums `#[non_exhaustive]` so downstream callers don't break on new variants

**API design**
- Accept `impl Trait` in function arguments (e.g. `impl AsRef<Path>`, `impl Read`) rather than concrete types
- Return `impl Trait` from free functions when the concrete type is an implementation detail
- Prefer `&str` / `&[T]` over `String` / `Vec<T>` in function parameters
- Use the newtype pattern to make domain concepts distinct at the type level (e.g. `SampleRate(u32)`)

**Ownership & borrowing**
- Minimize `clone()` on hot paths; pass references or use `Arc` for shared ownership
- Prefer `Cow<'_, str>` when a function sometimes needs to own and sometimes can borrow

**Clippy**
- Run `cargo clippy -- -D warnings` to treat all warnings as errors before committing
- Enable `#![warn(clippy::pedantic)]` at the crate root; suppress individual lints with `#[allow(...)]` + a comment explaining why rather than blanket-disabling
