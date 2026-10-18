#![forbid(unsafe_code)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let app = mightymonitor_viewer::app::App::new()?;
    eframe::run_native(
        "MightyMonitor",
        eframe::NativeOptions::default(),
        Box::new(|_| Ok(Box::new(app))),
    )?;
    Ok(())
}
