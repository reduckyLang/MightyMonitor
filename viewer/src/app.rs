//! Thin egui presentation layer.
#[cfg(target_os = "linux")]
use crate::net::ViewerSession;
use crate::net::{Output, SharedOutput, State};
use eframe::egui;
use std::sync::{Arc, Mutex};

pub struct App {
    address: String,
    output: SharedOutput,
    texture: Option<egui::TextureHandle>,
    task: Option<tokio::task::JoinHandle<()>>,
    #[cfg(target_os = "linux")]
    runtime: tokio::runtime::Runtime,
}
impl App {
    pub fn new() -> std::io::Result<Self> {
        Ok(Self {
            address: "127.0.0.1:5000".into(),
            output: Arc::new(Mutex::new(Output::default())),
            texture: None,
            task: None,
            #[cfg(target_os = "linux")]
            runtime: tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()?,
        })
    }
    fn connect(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
        self.texture = None;
        #[cfg(target_os = "linux")]
        match codec_gst::decoder::Decoder::new() {
            Ok(decoder) => {
                let session = ViewerSession::new(decoder);
                self.output = session.output.clone();
                self.task = Some(self.runtime.spawn(session.run(self.address.clone())));
            }
            Err(e) => {
                let mut output = self.output.lock().unwrap_or_else(|p| p.into_inner());
                output.state = State::Closed;
                output.error = Some(e.to_string());
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            let mut output = self.output.lock().unwrap_or_else(|p| p.into_inner());
            output.state = State::Closed;
            output.error = Some("H.264 decoding is currently available only on Linux. Windows decoding is not implemented.".into());
        }
    }
}
impl Drop for App {
    fn drop(&mut self) {
        if let Some(t) = &self.task {
            t.abort();
        }
    }
}
impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        egui::TopBottomPanel::top("connect").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Host IP:port");
                ui.text_edit_singleline(&mut self.address);
                if ui.button("Connect").clicked() {
                    self.connect();
                }
            });
        });
        let mut out = self.output.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(p) = out.picture.take() {
            if let Some(rgb) = crate::render::rgb(&p) {
                let image = egui::ColorImage::from_rgb([p.width as usize, p.height as usize], &rgb);
                match &mut self.texture {
                    Some(t) => t.set(image, egui::TextureOptions::LINEAR),
                    None => {
                        self.texture =
                            Some(ctx.load_texture("video", image, egui::TextureOptions::LINEAR))
                    }
                }
                out.stats.present_ms = p.decoded.elapsed().as_secs_f64() * 1000.0;
                eprintln!("decode->present submission: {:.2} ms", out.stats.present_ms);
            }
        }
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label(format!(
                "{:?} | {:.1} fps | {} kbit/s | RTT {} ms | decode→present {:.2} ms",
                out.state, out.stats.fps, out.stats.kbps, out.stats.rtt_ms, out.stats.present_ms
            ));
            if let Some(e) = &out.error {
                ui.colored_label(egui::Color32::RED, e);
            }
            if let Some(t) = &self.texture {
                ui.add(egui::Image::new(t).fit_to_exact_size({
                    let size = t.size_vec2();
                    size * (ui.available_width() / size.x)
                        .min(ui.available_height() / size.y)
                        .max(0.01)
                }));
            }
        });
        ctx.request_repaint_after(std::time::Duration::from_millis(16));
    }
}
