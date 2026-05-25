/*
 * SpinorML Ltd 🚀 AGPL-3.0 License - https://spinorml.com/license
 *
 * Multi-camera MP4 player demo.
 *
 * Usage:
 *   cargo run --example player --features gstreamer              # downloads a test clip
 *   cargo run --example player --features gstreamer -- video.mp4 # use your own file
 *
 * Opens the same file as two named sources ("cam0", "cam1") via GstMultiVideoSource
 * and renders them side by side in an egui window.
 */

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use eframe::egui;
use streamer_rs::{FrameData, GstMultiVideoSource, GstVideoSource, VideoSource};

fn main() {
    let path = match std::env::args().nth(1) {
        Some(p) => PathBuf::from(p),
        None => download_test_video(),
    };

    let (tx, rx) = mpsc::sync_channel::<(String, egui::ColorImage)>(8);

    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
        rt.block_on(decode_loop(path, tx));
    });

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("streamer-rs player — multi-camera")
            .with_inner_size([1280.0, 560.0]),
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

async fn decode_loop(path: PathBuf, tx: mpsc::SyncSender<(String, egui::ColorImage)>) {
    let sources = [("cam0", &path), ("cam1", &path)];
    let mut multi = GstMultiVideoSource::new();

    for (id, p) in &sources {
        match GstVideoSource::from_file(p) {
            Ok(s) => { multi.add(*id, s); }
            Err(e) => {
                eprintln!("error: could not open {id} ({}): {e}", p.display());
                return;
            }
        }
    }

    if let Err(e) = multi.start().await {
        eprintln!("error: failed to start pipelines: {e}");
        return;
    }

    loop {
        match multi.next_frame().await {
            Ok(Some(frame)) => {
                let id = frame
                    .source_id
                    .as_deref()
                    .unwrap_or("unknown")
                    .to_string();

                let bytes = match frame.data.to_bytes().await {
                    Ok(b) => b,
                    Err(e) => {
                        eprintln!("error: frame read failed for {id}: {e}");
                        break;
                    }
                };

                let rgb = i420_to_rgb(&bytes, frame.width, frame.height);
                let image =
                    egui::ColorImage::from_rgb([frame.width as usize, frame.height as usize], &rgb);

                if tx.send((id, image)).is_err() {
                    break; // window was closed
                }
            }
            Ok(None) => break,
            Err(e) => {
                eprintln!("error: decode failed: {e}");
                break;
            }
        }
    }

    let _ = multi.stop().await;
}

// ---------------------------------------------------------------------------
// egui app
// ---------------------------------------------------------------------------

struct PlayerApp {
    rx: mpsc::Receiver<(String, egui::ColorImage)>,
    textures: HashMap<String, egui::TextureHandle>,
    frame_counts: HashMap<String, u64>,
}

impl PlayerApp {
    fn new(rx: mpsc::Receiver<(String, egui::ColorImage)>) -> Self {
        Self { rx, textures: HashMap::new(), frame_counts: HashMap::new() }
    }
}

impl eframe::App for PlayerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        while let Ok((id, image)) = self.rx.try_recv() {
            let key = format!("frame_{id}");
            self.textures.insert(
                id.clone(),
                ctx.load_texture(key, image, egui::TextureOptions::LINEAR),
            );
            *self.frame_counts.entry(id).or_insert(0) += 1;
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            let mut source_ids: Vec<&String> = self.textures.keys().collect();
            source_ids.sort();

            if source_ids.is_empty() {
                ui.centered_and_justified(|ui| { ui.label("Loading…"); });
            } else {
                let col_width = ui.available_width() / source_ids.len() as f32;
                ui.columns(source_ids.len(), |cols| {
                    for (i, id) in source_ids.iter().enumerate() {
                        let texture = &self.textures[*id];
                        let count = self.frame_counts.get(*id).copied().unwrap_or(0);

                        let tex_size = texture.size_vec2();
                        let available_h = cols[i].available_height() - 24.0; // reserve label
                        let scale = (col_width / tex_size.x).min(available_h / tex_size.y);
                        let display_size = tex_size * scale;

                        cols[i].vertical_centered(|ui| {
                            ui.image(egui::load::SizedTexture::new(texture.id(), display_size));
                            ui.label(format!("{id}  —  frame {count}"));
                        });
                    }
                });
            }
        });

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

    let url = "https://file-examples.com/storage/fe84a902ae6a1407994448f/2017/04/file_example_MP4_480_1_5MG.mp4";

    println!("Downloading test video from {url}");
    println!("(pass a path as an argument to skip this: cargo run --example player -- video.mp4)");

    let response = ureq::get(url).call().expect("download failed");
    let mut file = std::fs::File::create(&path).expect("create file");
    std::io::copy(&mut response.into_reader(), &mut file).expect("write file");
    println!("Saved to {}", path.display());

    path
}
