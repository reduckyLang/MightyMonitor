//! Developer-only native UI snapshot (PPM), with no connection or sharing started.
use eframe::egui;
use std::io::Write;
struct Snapshot {
    app: trinix_viewer::app::App,
    frames: u32,
    path: String,
}
impl eframe::App for Snapshot {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.app.update(ctx, frame);
        self.frames += 1;
        if self.frames == 8 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
        for event in ctx.input(|i| i.events.clone()) {
            if let egui::Event::Screenshot { image, .. } = event {
                let mut file = std::fs::File::create(&self.path).expect("snapshot file");
                write!(file, "P6\n{} {}\n255\n", image.size[0], image.size[1]).expect("header");
                for pixel in &image.pixels {
                    file.write_all(&pixel.to_array()[..3]).expect("pixel");
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("Pass a PPM output path")?;
    let snapshot = Snapshot {
        app: trinix_viewer::app::App::new()?,
        frames: 0,
        path,
    };
    eframe::run_native(
        "Trinix preview",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_inner_size([1100.0, 720.0]),
            ..Default::default()
        },
        Box::new(|_| Ok(Box::new(snapshot))),
    )?;
    Ok(())
}
