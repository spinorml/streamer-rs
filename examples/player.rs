/*
 * SpinorML Ltd 🚀 AGPL-3.0 License - https://spinorml.com/license
 *
 * Minimal MP4 player demo.
 *
 * Usage:
 *   cargo run --example player --features gstreamer              # downloads a test clip
 *   cargo run --example player --features gstreamer -- video.mp4 # use your own file
 *
 * Decodes via GstVideoSource (GStreamer backend) and renders each frame
 * as an egui texture, demonstrating the full VideoSource → FrameData::to_bytes() path.
 */

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use eframe::egui;
use gstreamer::prelude::{ElementExt, GstObjectExt};
use streamer_rs::{FrameData, GstVideoSource, VideoSource};

fn main() {
    let path = match std::env::args().nth(1) {
        Some(p) => PathBuf::from(p),
        None => download_test_video(),
    };

    let (tx, rx) = mpsc::sync_channel::<egui::ColorImage>(4);

    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
        rt.block_on(decode_loop(path, tx));
    });

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("streamer-rs player")
            .with_inner_size([960.0, 560.0]),
        ..Default::default()
    };

    eframe::run_native(
        "streamer-rs player",
        options,
        Box::new(move |_cc| Ok(Box::new(PlayerApp::new(rx)))),
    )
    .expect("eframe failed");
}

// ---------------------------------------------------------------------------
// Decode loop — runs in a background thread on its own tokio runtime
// ---------------------------------------------------------------------------

async fn decode_loop(path: PathBuf, tx: mpsc::SyncSender<egui::ColorImage>) {
    eprintln!("[decode] opening file: {}", path.display());
    let mut source = match GstVideoSource::from_file(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[decode] error: could not open {}: {e}", path.display());
            return;
        }
    };
    eprintln!("[decode] pipeline created");

    // Spawn a GStreamer bus monitor so we see errors/warnings/state changes.
    let bus = source.pipeline.bus().expect("pipeline has no bus");
    std::thread::spawn(move || {
        use gstreamer::MessageView;
        for msg in bus.iter_timed(gstreamer::ClockTime::NONE) {
            match msg.view() {
                MessageView::Error(e) => {
                    eprintln!("[gst bus] ERROR from {:?}: {}", msg.src().map(|s| s.name()), e.error());
                    eprintln!("[gst bus]   debug: {:?}", e.debug());
                    break;
                }
                MessageView::Warning(w) => {
                    eprintln!("[gst bus] WARNING: {}", w.error());
                }
                MessageView::StateChanged(sc) => {
                    eprintln!("[gst bus] {:?} state: {:?} → {:?}", msg.src().map(|s| s.name()), sc.old(), sc.current());
                }
                MessageView::Eos(_) => {
                    eprintln!("[gst bus] EOS");
                    break;
                }
                _ => {}
            }
        }
    });

    eprintln!("[decode] starting pipeline...");
    if let Err(e) = source.start().await {
        eprintln!("[decode] error: failed to start pipeline: {e}");
        return;
    }
    eprintln!("[decode] pipeline playing, waiting for frames...");

    loop {
        match source.next_frame().await {
            Ok(Some(frame)) => {
                let bytes = match frame.data.to_bytes().await {
                    Ok(b) => b,
                    Err(e) => {
                        eprintln!("error: frame read failed: {e}");
                        break;
                    }
                };

                let rgb = i420_to_rgb(&bytes, frame.width, frame.height);
                let image =
                    egui::ColorImage::from_rgb([frame.width as usize, frame.height as usize], &rgb);

                if tx.send(image).is_err() {
                    break; // window was closed
                }
            }
            Ok(None) => break, // end of stream
            Err(e) => {
                eprintln!("error: decode failed: {e}");
                break;
            }
        }
    }

    let _ = source.stop().await;
}

// ---------------------------------------------------------------------------
// egui app
// ---------------------------------------------------------------------------

struct PlayerApp {
    rx: mpsc::Receiver<egui::ColorImage>,
    texture: Option<egui::TextureHandle>,
    frame_count: u64,
}

impl PlayerApp {
    fn new(rx: mpsc::Receiver<egui::ColorImage>) -> Self {
        Self {
            rx,
            texture: None,
            frame_count: 0,
        }
    }
}

impl eframe::App for PlayerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Drain the channel; keep only the latest frame to avoid falling behind.
        while let Ok(image) = self.rx.try_recv() {
            self.texture = Some(ctx.load_texture("frame", image, egui::TextureOptions::LINEAR));
            self.frame_count += 1;
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            match &self.texture {
                Some(texture) => {
                    // Scale to fit the panel while preserving aspect ratio.
                    let tex_size = texture.size_vec2();
                    let available = ui.available_size();
                    let scale = (available.x / tex_size.x).min(available.y / tex_size.y);
                    let display_size = tex_size * scale;

                    ui.vertical_centered(|ui| {
                        ui.image(egui::load::SizedTexture::new(texture.id(), display_size));
                        ui.label(format!("frame {}", self.frame_count));
                    });
                }
                None => {
                    ui.centered_and_justified(|ui| {
                        ui.label("Loading…");
                    });
                }
            }
        });

        // Keep repainting so new frames appear promptly.
        ctx.request_repaint_after(Duration::from_millis(16));
    }
}

// ---------------------------------------------------------------------------
// I420 → RGB conversion
// ---------------------------------------------------------------------------

fn i420_to_rgb(data: &[u8], width: u32, height: u32) -> Vec<u8> {
    let w = width as usize;
    let h = height as usize;
    let uv_w = w / 2;
    let y_size = w * h;
    let uv_size = uv_w * (h / 2);

    let y_plane = &data[0..y_size];
    let u_plane = &data[y_size..y_size + uv_size];
    let v_plane = &data[y_size + uv_size..y_size + 2 * uv_size];

    let mut rgb = vec![0u8; w * h * 3];
    for row in 0..h {
        for col in 0..w {
            let y = y_plane[row * w + col] as f32;
            let u = u_plane[(row / 2) * uv_w + (col / 2)] as f32 - 128.0;
            let v = v_plane[(row / 2) * uv_w + (col / 2)] as f32 - 128.0;

            let r = (y + 1.402 * v).clamp(0.0, 255.0) as u8;
            let g = (y - 0.344136 * u - 0.714136 * v).clamp(0.0, 255.0) as u8;
            let b = (y + 1.772 * u).clamp(0.0, 255.0) as u8;

            let idx = (row * w + col) * 3;
            rgb[idx] = r;
            rgb[idx + 1] = g;
            rgb[idx + 2] = b;
        }
    }
    rgb
}

// ---------------------------------------------------------------------------
// Test video download
// ---------------------------------------------------------------------------

fn download_test_video() -> PathBuf {
    let cache_dir = PathBuf::from("target").join("test-videos");
    std::fs::create_dir_all(&cache_dir).expect("create cache dir");
    let path = cache_dir.join("sample.mp4");

    if path.exists() {
        return path;
    }

    // 1 MB, 15 s clip, H.264/AAC, Creative Commons
    let url = "https://file-examples.com/storage/fe84a902ae6a1407994448f/2017/04/file_example_MP4_480_1_5MG.mp4";

    println!("Downloading test video from {url}");
    println!("(pass a path as an argument to skip this: cargo run --example player -- video.mp4)");

    let response = ureq::get(url).call().expect("download failed");
    let mut file = std::fs::File::create(&path).expect("create file");
    std::io::copy(&mut response.into_reader(), &mut file).expect("write file");
    println!("Saved to {}", path.display());

    path
}
