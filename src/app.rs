use std::path::PathBuf;

use crate::config;
use crate::save_file;

pub struct Dos2EditorApp {
    save_root: Option<PathBuf>,
    save_files: Vec<PathBuf>,
    selected_idx: Option<usize>,
    status_msg: String,
}

impl Default for Dos2EditorApp {
    fn default() -> Self {
        let save_root = config::load_save_root()
            .or_else(|| save_file::default_save_roots().into_iter().find(|p| p.is_dir()));
        let save_files = save_root
            .as_deref()
            .map(save_file::list_savegames)
            .unwrap_or_default();

        Self {
            save_root,
            save_files,
            selected_idx: None,
            status_msg: String::new(),
        }
    }
}

impl eframe::App for Dos2EditorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("📁 Open Save Folder…").clicked() {
                    if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                        self.save_files = save_file::list_savegames(&dir);
                        self.selected_idx = None;
                        config::save_save_root(&dir);
                        self.save_root = Some(dir);
                    }
                }
                ui.separator();
                ui.label(&self.status_msg);
            });
        });

        egui::SidePanel::left("save_list").show(ctx, |ui| {
            ui.heading("Savegames");
            egui::ScrollArea::vertical().show(ui, |ui| {
                for (idx, path) in self.save_files.iter().enumerate() {
                    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    let selected = self.selected_idx == Some(idx);
                    if ui.selectable_label(selected, name).clicked() {
                        self.selected_idx = Some(idx);
                        self.status_msg = format!("Selected {}", path.display());
                    }
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("DOS2:DE Save Editor");
            ui.label("Pick a savegame from the left to begin.");
        });
    }
}
