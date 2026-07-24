fn main() {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([800.0, 600.0])
            .with_title("Touring Desktop UI"),
        ..Default::default()
    };
    eframe::run_native(
        "Touring Desktop UI",
        options,
        Box::new(|cc| Ok(touring_bindings::desktop::app::AppState::build_app(cc))),
    )
    .expect("eframe failed to launch the desktop UI");
}