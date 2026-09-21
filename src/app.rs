use std::path::PathBuf;

use crate::config;
use crate::domain::item;
use crate::format::lsf;
use crate::format::node::{AttributeValue, Node, NodeAttribute, Resource};
use crate::format::pak::Pak;
use crate::save_file;

pub struct Dos2EditorApp {
    save_root: Option<PathBuf>,
    save_files: Vec<PathBuf>,
    selected_idx: Option<usize>,
    status_msg: String,
    resource: Option<Resource>,
    item_filter: String,
    selected_item: Option<usize>,
}

impl Default for Dos2EditorApp {
    fn default() -> Self {
        let save_root = config::load_save_root().or_else(|| {
            save_file::default_save_roots()
                .into_iter()
                .find(|p| p.is_dir())
        });
        let save_files = save_root
            .as_deref()
            .map(save_file::list_savegames)
            .unwrap_or_default();

        Self {
            save_root,
            save_files,
            selected_idx: None,
            status_msg: String::new(),
            resource: None,
            item_filter: String::new(),
            selected_item: None,
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
                        self.resource = None;
                        self.selected_item = None;
                        config::save_save_root(&dir);
                        self.save_root = Some(dir);
                    }
                }
                ui.separator();
                ui.label(&self.status_msg);
            });
        });

        let mut requested_load = None;
        egui::SidePanel::left("save_list").show(ctx, |ui| {
            ui.heading("Savegames");
            egui::ScrollArea::vertical().show(ui, |ui| {
                for (idx, path) in self.save_files.iter().enumerate() {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    let selected = self.selected_idx == Some(idx);
                    if ui.selectable_label(selected, name).clicked() {
                        self.selected_idx = Some(idx);
                        requested_load = Some(path.clone());
                    }
                }
            });
        });
        if let Some(path) = requested_load {
            self.load_save(path);
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("DOS2:DE Save Editor");
            if let Some(resource) = &mut self.resource {
                ui.label("globals.lsf is loaded in memory. The raw tree is editable, but saving is not wired up yet.");
                ui.separator();
                item_browser(ui, resource, &mut self.item_filter, &mut self.selected_item);
                ui.separator();
                raw_resource_editor(ui, resource);
            } else {
                ui.label("Pick a savegame from the left to load and inspect globals.lsf.");
            }
        });
    }
}

impl Dos2EditorApp {
    fn load_save(&mut self, path: PathBuf) {
        self.resource = None;
        self.selected_item = None;
        let result = Pak::open(&path)
            .and_then(|pak| pak.read("globals.lsf"))
            .and_then(|bytes| lsf::parse(&bytes));
        match result {
            Ok(resource) => {
                self.status_msg = format!(
                    "Loaded {} ({} regions)",
                    path.display(),
                    resource.regions.len()
                );
                self.resource = Some(resource);
            }
            Err(error) => self.status_msg = format!("Could not load {}: {error}", path.display()),
        }
    }
}

#[derive(Clone)]
struct ItemListRow {
    index: usize,
    stats_id: String,
    amount: Option<i32>,
    item_type: Option<String>,
}

fn item_browser(
    ui: &mut egui::Ui,
    resource: &mut Resource,
    filter: &mut String,
    selected_item: &mut Option<usize>,
) {
    let filter_lower = filter.to_lowercase();
    let rows: Vec<ItemListRow> = item::items(resource)
        .into_iter()
        .enumerate()
        .filter_map(|(index, item)| {
            let stats_id = item.stats_id()?.to_owned();
            if !filter_lower.is_empty() && !stats_id.to_lowercase().contains(&filter_lower) {
                return None;
            }
            Some(ItemListRow {
                index,
                stats_id,
                amount: item.amount(),
                item_type: item.item_type().map(str::to_owned),
            })
        })
        .collect();

    ui.heading(format!("Items ({})", rows.len()));
    ui.horizontal(|ui| {
        ui.label("Filter Stats ID:");
        ui.add(egui::TextEdit::singleline(filter).hint_text("e.g. TeleportationGloves"));
    });
    ui.columns(2, |columns| {
        egui::ScrollArea::vertical()
            .max_height(280.0)
            .show(&mut columns[0], |ui| {
                for row in &rows {
                    let amount = row
                        .amount
                        .map(|amount| format!(" ×{amount}"))
                        .unwrap_or_default();
                    let rarity = row.item_type.as_deref().unwrap_or("-");
                    let label = format!("#{:03}  {}  [{rarity}]{amount}", row.index, row.stats_id);
                    if ui
                        .selectable_label(*selected_item == Some(row.index), label)
                        .clicked()
                    {
                        *selected_item = Some(row.index);
                    }
                }
            });
        if let Some(index) = *selected_item {
            if let Some(node) = item::item_node_mut(resource, index) {
                item_editor(&mut columns[1], index, node);
            } else {
                columns[1].label("The selected item is no longer present.");
                *selected_item = None;
            }
        } else {
            columns[1].label("Select an item to edit its existing fields.");
        }
    });
}

fn item_editor(ui: &mut egui::Ui, index: usize, node: &mut Node) {
    ui.heading(format!("Item #{index}"));
    string_attribute_editor(ui, node, "Stats");
    integer_attribute_editor(ui, node, "Amount");
    integer_attribute_editor(ui, node, "Slot");
    if let Some(stats) = node.child_mut("Stats") {
        ui.separator();
        ui.label("Item stats");
        integer_attribute_editor(ui, stats, "Level");
        integer_attribute_editor(ui, stats, "LevelGroupIndex");
        integer_attribute_editor(ui, stats, "NameIndex");
        if let Some(runes) = stats.children.get_mut("RuneSlot") {
            for (index, rune) in runes.iter_mut().take(3).enumerate() {
                ui.horizontal(|ui| {
                    ui.label(format!("Rune {index}:"));
                    if let Some(attribute) = rune.attributes.get_mut("RuneStatsID") {
                        raw_attribute_editor(ui, attribute);
                    } else {
                        ui.label("<missing>");
                    }
                });
            }
        }
        if let Some(boosts) = stats.child_mut("PermanentBoost") {
            egui::CollapsingHeader::new("Permanent boosts")
                .show(ui, |ui| raw_node_editor(ui, boosts));
        }
    }
}

fn string_attribute_editor(ui: &mut egui::Ui, node: &mut Node, name: &str) {
    ui.horizontal(|ui| {
        ui.label(format!("{name}:"));
        if let Some(attribute) = node.attributes.get_mut(name) {
            raw_attribute_editor(ui, attribute);
        } else {
            ui.label("<missing>");
        }
    });
}

fn integer_attribute_editor(ui: &mut egui::Ui, node: &mut Node, name: &str) {
    ui.horizontal(|ui| {
        ui.label(format!("{name}:"));
        if let Some(attribute) = node.attributes.get_mut(name) {
            raw_attribute_editor(ui, attribute);
        } else {
            ui.label("<missing>");
        }
    });
}

fn raw_resource_editor(ui: &mut egui::Ui, resource: &mut Resource) {
    let mut names: Vec<String> = resource.regions.keys().cloned().collect();
    names.sort();
    egui::ScrollArea::both().id_salt("raw_tree").show(ui, |ui| {
        for name in names {
            if let Some(region) = resource.regions.get_mut(&name) {
                egui::CollapsingHeader::new(format!("Region: {name}"))
                    .default_open(name == "Items" || name == "Characters")
                    .show(ui, |ui| raw_node_editor(ui, region));
            }
        }
    });
}

fn raw_node_editor(ui: &mut egui::Ui, node: &mut Node) {
    let mut attributes: Vec<String> = node.attributes.keys().cloned().collect();
    attributes.sort();
    for name in attributes {
        ui.horizontal(|ui| {
            ui.label(&name);
            if let Some(attribute) = node.attributes.get_mut(&name) {
                raw_attribute_editor(ui, attribute);
            }
        });
    }

    let mut tags: Vec<String> = node.children.keys().cloned().collect();
    tags.sort();
    for tag in tags {
        if let Some(children) = node.children.get_mut(&tag) {
            for (index, child) in children.iter_mut().enumerate() {
                ui.push_id((&tag, index), |ui| {
                    egui::CollapsingHeader::new(format!("{tag}[{index}]"))
                        .show(ui, |ui| raw_node_editor(ui, child));
                });
            }
        }
    }
}

fn raw_attribute_editor(ui: &mut egui::Ui, attribute: &mut NodeAttribute) {
    match &mut attribute.value {
        AttributeValue::U8(value) => {
            ui.add(egui::DragValue::new(value));
        }
        AttributeValue::I16(value) => {
            ui.add(egui::DragValue::new(value));
        }
        AttributeValue::U16(value) => {
            ui.add(egui::DragValue::new(value));
        }
        AttributeValue::I32(value) => {
            ui.add(egui::DragValue::new(value));
        }
        AttributeValue::U32(value) => {
            ui.add(egui::DragValue::new(value));
        }
        AttributeValue::F32(value) => {
            ui.add(egui::DragValue::new(value));
        }
        AttributeValue::F64(value) => {
            ui.add(egui::DragValue::new(value));
        }
        AttributeValue::U64(value) => {
            ui.add(egui::DragValue::new(value));
        }
        AttributeValue::I64(value) => {
            ui.add(egui::DragValue::new(value));
        }
        AttributeValue::I8(value) => {
            ui.add(egui::DragValue::new(value));
        }
        AttributeValue::Bool(value) => {
            ui.checkbox(value, "");
        }
        AttributeValue::Str(value) => {
            ui.add(egui::TextEdit::singleline(value).desired_width(320.0));
        }
        AttributeValue::Uuid(value) => {
            let mut text = value.to_string();
            let response = ui.add(egui::TextEdit::singleline(&mut text).desired_width(260.0));
            if response.lost_focus() {
                if let Ok(parsed) = text.parse() {
                    *value = parsed;
                }
            }
        }
        AttributeValue::IVec(values) => {
            for value in values {
                ui.add(egui::DragValue::new(value));
            }
        }
        AttributeValue::Vec(values) | AttributeValue::Mat(values) => {
            for value in values {
                ui.add(egui::DragValue::new(value));
            }
        }
        AttributeValue::TranslatedString(value) => {
            if let Some(text) = &mut value.value {
                ui.add(egui::TextEdit::singleline(text).desired_width(320.0));
            } else {
                ui.label(format!("handle: {}", value.handle));
            }
        }
        AttributeValue::TranslatedFsString(value) => {
            ui.label(format!("translated string handle: {}", value.string.handle));
        }
        AttributeValue::ScratchBuffer(value) => {
            ui.label(format!("<{} bytes>", value.len()));
        }
        AttributeValue::None => {
            ui.label("<none>");
        }
    }
}
