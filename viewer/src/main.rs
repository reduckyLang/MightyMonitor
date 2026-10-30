#![forbid(unsafe_code)]
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().nth(1).as_deref() == Some("--check-runtime") {
        let _decoder = codec_gst::decoder::Decoder::new()?;
        return Ok(());
    }
    let app = trinix_viewer::app::App::new()?;
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../../assets/trinix.png"))?;
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 720.0])
            .with_min_inner_size([760.0, 480.0])
            .with_icon(icon),
        ..Default::default()
    };
    eframe::run_native("Trinix", options, Box::new(|_| Ok(Box::new(app))))?;
    Ok(())
}
