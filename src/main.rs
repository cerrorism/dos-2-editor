use dos2_editor::app;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1100.0, 750.0]),
        ..Default::default()
    };
    eframe::run_native(
        "DOS2:DE Save Editor",
        options,
        Box::new(|_cc| Ok(Box::new(app::Dos2EditorApp::default()))),
    )
}
