/*
 * SpinorML Ltd 🚀 AGPL-3.0 License - https://spinorml.com/license
 */

use async_trait::async_trait;
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use tokio::sync::mpsc;

use crate::error::{Result, StreamError};
use crate::frame::VideoFrame;
use crate::source::VideoSource;

use super::frame::GstFrameData;
use super::utils;

/// Captures video from a V4L2 device, a test source, a local file, or an RTSP stream.
/// Produces `VideoFrame<GstFrameData>` — the GstBuffer is kept alive without a CPU copy.
pub struct GstVideoSource {
    pub pipeline: gst::Pipeline,
    rx: mpsc::Receiver<Result<VideoFrame<GstFrameData>>>,
}

impl GstVideoSource {
    pub fn new(device: &str) -> Result<Self> {
        utils::init();
        let pipeline_str = format!(
            "v4l2src device={device} ! videoconvert ! video/x-raw,format=I420 ! appsink name=sink sync=false"
        );
        Self::from_pipeline_str(&pipeline_str)
    }

    /// Open an RTSP stream. Handles reconnection and buffer management automatically.
    ///
    /// `url` must be a valid `rtsp://` URI, e.g. `rtsp://localhost:8554/live`.
    pub fn from_rtsp(url: &str) -> Result<Self> {
        utils::init();
        let pipeline_str = format!(
            "rtspsrc location={url} latency=0 ! decodebin ! videoconvert ! video/x-raw,format=I420 ! appsink name=sink sync=false"
        );
        Self::from_pipeline_str(&pipeline_str)
    }

    /// Use `videotestsrc` — useful for testing without hardware.
    pub fn test_source() -> Result<Self> {
        utils::init();
        Self::from_pipeline_str(
            "videotestsrc ! videoconvert ! video/x-raw,format=I420 ! appsink name=sink sync=false",
        )
    }

    /// Open a local video file. Supports any format GStreamer's `playbin` can handle.
    ///
    /// Uses `playbin` with `fakesink` for audio so the audio track never blocks
    /// pipeline preroll. Video is decoded and converted to I420 via a custom video-sink bin.
    pub fn from_file(path: impl AsRef<std::path::Path>) -> Result<Self> {
        utils::init();

        let path = path
            .as_ref()
            .canonicalize()
            .map_err(|e| StreamError::Pipeline { message: e.to_string() })?;

        // GStreamer expects a URI; on Unix the absolute path already starts with '/'
        // giving us file:///absolute/path.
        #[cfg(unix)]
        let uri = format!("file://{}", path.display());
        #[cfg(not(unix))]
        let uri = format!("file:///{}", path.display().to_string().replace('\\', "/"));

        // Build a video-sink bin:  [ghost-sink] → videoconvert → capsfilter → appsink
        let video_bin = gst::Bin::new();

        let convert = make_element("videoconvert", None)?;
        let capsfilter = {
            let caps = gst::Caps::builder("video/x-raw")
                .field("format", "I420")
                .build();
            make_element("capsfilter", None)?
                .tap(|e| e.set_property("caps", &caps))
        };
        let appsink_el = make_element("appsink", Some("sink"))?;
        appsink_el.set_property("sync", true);

        video_bin
            .add_many([&convert, &capsfilter, &appsink_el])
            .map_err(|e| StreamError::Pipeline { message: e.to_string() })?;
        gst::Element::link_many([&convert, &capsfilter, &appsink_el])
            .map_err(|e| StreamError::Pipeline { message: e.to_string() })?;

        // Expose a ghost pad so playbin can link into the bin.
        let sink_pad = convert
            .static_pad("sink")
            .ok_or_else(|| StreamError::Pipeline {
                message: "videoconvert has no sink pad".into(),
            })?;
        let ghost = gst::GhostPad::with_target(&sink_pad)
            .map_err(|_| StreamError::Pipeline {
                message: "ghost pad creation failed".into(),
            })?;
        video_bin
            .add_pad(&ghost)
            .map_err(|e| StreamError::Pipeline { message: e.to_string() })?;

        // playbin wires demuxing, decoding, A/V sync, and seeking automatically.
        let fakesink = make_element("fakesink", None)?;
        let playbin = gst::ElementFactory::make("playbin")
            .property("uri", &uri)
            .property("audio-sink", &fakesink)
            .property("video-sink", &video_bin)
            .build()
            .map_err(|e| StreamError::Pipeline { message: e.to_string() })?;

        let pipeline = playbin
            .downcast::<gst::Pipeline>()
            .map_err(|_| StreamError::Pipeline {
                message: "playbin is not a pipeline".into(),
            })?;

        let appsink = video_bin
            .by_name("sink")
            .and_downcast::<gst_app::AppSink>()
            .ok_or_else(|| StreamError::Pipeline {
                message: "appsink 'sink' not found in video bin".into(),
            })?;

        let (tx, rx) = mpsc::channel::<Result<VideoFrame<GstFrameData>>>(32);
        Self::install_callbacks(&appsink, tx);

        Ok(Self { pipeline, rx })
    }

    // --- private helpers ---

    fn from_pipeline_str(desc: &str) -> Result<Self> {
        let pipeline = gst::parse::launch(desc)
            .map_err(|e| StreamError::Pipeline { message: e.to_string() })?
            .downcast::<gst::Pipeline>()
            .map_err(|_| StreamError::Pipeline {
                message: "element is not a pipeline".into(),
            })?;

        let appsink = pipeline
            .by_name("sink")
            .and_downcast::<gst_app::AppSink>()
            .ok_or_else(|| StreamError::Pipeline {
                message: "appsink element 'sink' not found".into(),
            })?;

        let (tx, rx) = mpsc::channel::<Result<VideoFrame<GstFrameData>>>(32);
        Self::install_callbacks(&appsink, tx);

        Ok(Self { pipeline, rx })
    }

    fn install_callbacks(
        appsink: &gst_app::AppSink,
        tx: mpsc::Sender<Result<VideoFrame<GstFrameData>>>,
    ) {
        appsink.set_callbacks(
            gst_app::AppSinkCallbacks::builder()
                .new_sample({
                    let tx = tx.clone();
                    move |sink| {
                        let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                        let result = utils::gst_sample_to_gst_frame(sample)
                            .map_err(|_| gst::FlowError::Error);
                        let _ = tx.blocking_send(result.map_err(|_| StreamError::Pipeline {
                            message: "frame conversion error".into(),
                        }));
                        Ok(gst::FlowSuccess::Ok)
                    }
                })
                .eos(move |_| {
                    let _ = tx.blocking_send(Err(StreamError::EndOfStream));
                })
                .build(),
        );
    }
}

#[async_trait]
impl VideoSource for GstVideoSource {
    type Frame = GstFrameData;

    async fn start(&mut self) -> Result<()> {
        self.pipeline
            .set_state(gst::State::Playing)
            .map_err(|e| StreamError::Pipeline { message: e.to_string() })?;

        // playbin returns StateChangeReturn::Async — wait up to 10 s for it to reach Playing.
        let (res, cur, _pending) = self
            .pipeline
            .state(Some(gst::ClockTime::from_seconds(10)));
        if res.is_err() {
            return Err(StreamError::Pipeline {
                message: format!("pipeline failed to reach Playing state (current: {cur:?})"),
            });
        }
        Ok(())
    }

    async fn next_frame(&mut self) -> Result<Option<VideoFrame<GstFrameData>>> {
        match self.rx.recv().await {
            Some(Ok(frame)) => Ok(Some(frame)),
            Some(Err(StreamError::EndOfStream)) => Ok(None),
            Some(Err(e)) => Err(e),
            None => Ok(None),
        }
    }

    async fn stop(&mut self) -> Result<()> {
        self.pipeline
            .set_state(gst::State::Null)
            .map_err(|e| StreamError::Pipeline { message: e.to_string() })?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_element(factory: &str, name: Option<&str>) -> Result<gst::Element> {
    let mut builder = gst::ElementFactory::make(factory);
    if let Some(n) = name {
        builder = builder.name(n);
    }
    builder.build().map_err(|e| StreamError::Pipeline { message: e.to_string() })
}

trait ElementExt2 {
    fn tap(self, f: impl FnOnce(&Self)) -> Self;
}

impl ElementExt2 for gst::Element {
    fn tap(self, f: impl FnOnce(&Self)) -> Self {
        f(&self);
        self
    }
}
