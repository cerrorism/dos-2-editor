use std::path::PathBuf;
use std::sync::Arc;

use crate::config;
use crate::domain::{character, item};
use crate::format::compression::CompressionMethod;
use crate::format::lsf;
use crate::format::node::{AttributeValue, Node, NodeAttribute, Resource};
use crate::format::pak::Pak;
use crate::gamedata::catalog::{DisplayLanguage, StatsCatalog};
use crate::save_file;

pub struct Dos2EditorApp {
    save_root: Option<PathBuf>,
    save_files: Vec<PathBuf>,
    selected_idx: Option<usize>,
    status_msg: String,
    resource: Option<Resource>,
    pak: Option<Pak>,
    catalog: Option<StatsCatalog>,
    catalog_status: String,
    display_language: DisplayLanguage,
    item_filter: String,
    selected_item: Option<usize>,
    selected_party: Option<usize>,
    fonts_configured: bool,
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
            pak: None,
            catalog: None,
            catalog_status: String::new(),
            display_language: DisplayLanguage::SimplifiedChinese,
            item_filter: String::new(),
            selected_item: None,
            selected_party: Some(0),
            fonts_configured: false,
        }
    }
}

impl eframe::App for Dos2EditorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.fonts_configured {
            install_chinese_font(ctx);
            self.fonts_configured = true;
        }
        let mut language_changed = false;
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Open Save Folder…").clicked() {
                    if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                        self.save_files = save_file::list_savegames(&dir);
                        self.selected_idx = None;
                        self.resource = None;
                        self.pak = None;
                        self.catalog = None;
                        self.selected_item = None;
                        config::save_save_root(&dir);
                        self.save_root = Some(dir);
                    }
                }
                let can_save =
                    self.resource.is_some() && self.pak.is_some() && self.selected_idx.is_some();
                if ui
                    .add_enabled(can_save, egui::Button::new("Save Edited Copy…"))
                    .clicked()
                {
                    self.save_edited_copy();
                }
                ui.separator();
                egui::ComboBox::from_id_salt("display_language")
                    .selected_text(self.display_language.label())
                    .show_ui(ui, |ui| {
                        for language in DisplayLanguage::ALL {
                            language_changed |= ui
                                .selectable_value(
                                    &mut self.display_language,
                                    language,
                                    language.label(),
                                )
                                .changed();
                        }
                    });
                ui.label(&self.status_msg);
            });
        });
        if language_changed && self.resource.is_some() {
            self.load_game_data();
        }

        let mut requested_load = None;
        egui::SidePanel::left("save_list").show(ctx, |ui| {
            ui.heading("Savegames");
            egui::ScrollArea::vertical().show(ui, |ui| {
                for (idx, path) in self.save_files.iter().enumerate() {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    if ui
                        .selectable_label(self.selected_idx == Some(idx), name)
                        .clicked()
                    {
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
                ui.label(&self.catalog_status);
                ui.separator();
                party_inventory_editor(
                    ui,
                    resource,
                    self.catalog.as_ref(),
                    &mut self.item_filter,
                    &mut self.selected_item,
                    &mut self.selected_party,
                );
                ui.separator();
                egui::CollapsingHeader::new("Advanced: raw save data")
                    .show(ui, |ui| raw_resource_editor(ui, resource));
            } else {
                ui.label("Pick a savegame from the left to inspect its party inventories.");
            }
        });
    }
}

fn install_chinese_font(ctx: &egui::Context) {
    let font_path = PathBuf::from(r"C:\Windows\Fonts\NotoSansSC-VF.ttf");
    let Ok(bytes) = std::fs::read(&font_path) else {
        return;
    };
    let mut fonts = egui::FontDefinitions::default();
    let name = "noto_sans_sc".to_owned();
    fonts.font_data.insert(
        name.clone(),
        Arc::new(egui::FontData::from_owned(bytes).tweak(egui::FontTweak {
            scale: 0.9,
            ..Default::default()
        })),
    );
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .insert(0, name.clone());
    }
    ctx.set_fonts(fonts);
}

impl Dos2EditorApp {
    fn load_save(&mut self, path: PathBuf) {
        self.resource = None;
        self.pak = None;
        self.catalog = None;
        self.selected_item = None;
        self.selected_party = Some(0);
        let result = Pak::open(&path).and_then(|pak| {
            let resource = lsf::parse(&pak.read("globals.lsf")?)?;
            Ok((pak, resource))
        });
        match result {
            Ok((pak, resource)) => {
                self.status_msg = format!(
                    "Loaded {} ({} regions)",
                    path.display(),
                    resource.regions.len()
                );
                self.pak = Some(pak);
                self.resource = Some(resource);
                self.load_game_data();
            }
            Err(error) => self.status_msg = format!("Could not load {}: {error}", path.display()),
        }
    }

    fn load_game_data(&mut self) {
        let game_root = PathBuf::from(r"D:\SteamLibrary\steamapps\common\Divinity Original Sin 2");
        match StatsCatalog::load(&game_root, self.display_language) {
            Ok(catalog) => {
                self.catalog_status = format!(
                    "{} item names loaded ({} names from game data).",
                    self.display_language.label(),
                    catalog.localization_count(),
                );
                self.catalog = Some(catalog);
            }
            Err(error) => {
                self.catalog_status = format!(
                    "Game localization was unavailable; showing readable Stat ID fallbacks. ({error})"
                );
            }
        }
    }

    fn save_edited_copy(&mut self) {
        let Some(original_path) = self
            .selected_idx
            .and_then(|index| self.save_files.get(index))
            .cloned()
        else {
            self.status_msg = "No save is selected.".into();
            return;
        };
        let Some(resource) = &self.resource else {
            self.status_msg = "No globals.lsf resource is loaded.".into();
            return;
        };
        let Some(mut pak) = self.pak.clone() else {
            self.status_msg = "No save package is loaded.".into();
            return;
        };

        let rewritten_lsf = lsf::serialize(resource, CompressionMethod::Zlib);
        let result = pak
            .set_file("globals.lsf", &rewritten_lsf)
            .map(|_| pak.to_bytes())
            .and_then(|bytes| {
                let verify_pak = Pak::parse(&bytes)?;
                let verify_resource = lsf::parse(&verify_pak.read("globals.lsf")?)?;
                if verify_resource != *resource {
                    return Err(
                        "saved globals.lsf did not structurally match the in-memory edit".into(),
                    );
                }
                Ok(bytes)
            });
        match result {
            Ok(bytes) => {
                let output_path = save_file::make_edited_copy_path(&original_path);
                match std::fs::write(&output_path, bytes) {
                    Ok(()) => {
                        self.status_msg =
                            format!("Wrote verified edited copy: {}", output_path.display())
                    }
                    Err(error) => self.status_msg = format!("Could not write edited copy: {error}"),
                }
            }
            Err(error) => self.status_msg = format!("Could not validate edited copy: {error}"),
        }
    }
}

#[derive(Clone)]
struct ItemListRow {
    index: usize,
    display_name: String,
    stats_id: String,
    amount: Option<i32>,
    item_type: Option<String>,
    equipped: bool,
}

fn party_inventory_editor(
    ui: &mut egui::Ui,
    resource: &mut Resource,
    catalog: Option<&StatsCatalog>,
    filter: &mut String,
    selected_item: &mut Option<usize>,
    selected_party: &mut Option<usize>,
) {
    let party = character::party_members(resource);
    ui.heading("Party inventories");
    ui.horizontal_wrapped(|ui| {
        for (index, member) in party.iter().enumerate() {
            let inventory = member.inventory_handle();
            let count = inventory.map_or(0, |handle| {
                item::items(resource)
                    .into_iter()
                    .filter(|entry| entry.parent_handle() == Some(handle))
                    .count()
            });
            if ui
                .selectable_label(
                    *selected_party == Some(index),
                    format!("{} ({count})", character_name(member)),
                )
                .clicked()
            {
                *selected_party = Some(index);
                *selected_item = None;
            }
        }
        if ui
            .selectable_label(selected_party.is_none(), "All saved items")
            .clicked()
        {
            *selected_party = None;
            *selected_item = None;
        }
    });

    let selected_inventory = selected_party
        .and_then(|party_index| party.get(party_index))
        .and_then(character::Character::inventory_handle);
    let filter_lower = filter.to_lowercase();
    let rows: Vec<ItemListRow> = item::items(resource)
        .into_iter()
        .enumerate()
        .filter_map(|(index, entry)| {
            if selected_party.is_some() && entry.parent_handle() != selected_inventory {
                return None;
            }
            let stats_id = entry.stats_id()?.to_owned();
            let display_name = item_name(&entry, catalog);
            if !filter_lower.is_empty()
                && !display_name.to_lowercase().contains(&filter_lower)
                && !stats_id.to_lowercase().contains(&filter_lower)
            {
                return None;
            }
            Some(ItemListRow {
                index,
                display_name,
                stats_id,
                amount: entry.amount(),
                item_type: entry.item_type().map(str::to_owned),
                equipped: entry.is_equipped(),
            })
        })
        .collect();

    ui.horizontal(|ui| {
        ui.label(format!("Items ({})", rows.len()));
        ui.add(egui::TextEdit::singleline(filter).hint_text("Filter by item name"));
    });
    ui.columns(2, |columns| {
        egui::ScrollArea::vertical()
            .max_height(360.0)
            .show(&mut columns[0], |ui| {
                if selected_party.is_some() {
                    item_list_section(
                        ui,
                        "Equipped",
                        rows.iter().filter(|row| row.equipped),
                        selected_item,
                    );
                    ui.separator();
                    item_list_section(
                        ui,
                        "Backpack",
                        rows.iter().filter(|row| !row.equipped),
                        selected_item,
                    );
                } else {
                    item_list_section(ui, "Saved items", rows.iter(), selected_item);
                }
            });
        if let Some(index) = *selected_item {
            if let Some(node) = item::item_node_mut(resource, index) {
                item_editor(&mut columns[1], index, node, catalog);
            } else {
                columns[1].label("The selected item is no longer present.");
                *selected_item = None;
            }
        } else {
            columns[1].label("Select an item to view and edit its saved fields.");
        }
    });
}

fn item_list_section<'a>(
    ui: &mut egui::Ui,
    heading: &str,
    rows: impl Iterator<Item = &'a ItemListRow>,
    selected_item: &mut Option<usize>,
) {
    let rows: Vec<&ItemListRow> = rows.collect();
    ui.strong(format!("{heading} ({})", rows.len()));
    for row in rows {
        let amount = row
            .amount
            .filter(|amount| *amount > 1)
            .map(|amount| format!(" ×{amount}"))
            .unwrap_or_default();
        let rarity = row.item_type.as_deref().unwrap_or("Item");
        if ui
            .selectable_label(
                *selected_item == Some(row.index),
                format!("{}  [{rarity}]{amount}", row.display_name),
            )
            .on_hover_text(&row.stats_id)
            .clicked()
        {
            *selected_item = Some(row.index);
        }
    }
}

fn character_name(character: &character::Character<'_>) -> String {
    character
        .name()
        .filter(|name| !name.trim().is_empty())
        .or_else(|| character.origin_name())
        .unwrap_or("Unnamed party member")
        .to_owned()
}

fn item_name(item: &item::Item<'_>, catalog: Option<&StatsCatalog>) -> String {
    item.custom_display_name()
        .filter(|name| !name.trim().is_empty())
        .map(str::to_owned)
        .or_else(|| {
            item.stats_id().and_then(|stats_id| {
                catalog
                    .and_then(|catalog| catalog.display_name(item.current_template(), stats_id))
                    .map(str::to_owned)
            })
        })
        .unwrap_or_else(|| {
            item.stats_id()
                .map(friendly_stat_name)
                .unwrap_or_else(|| "Unnamed item".into())
        })
}

fn friendly_stat_name(stats_id: &str) -> String {
    const PREFIXES: &[&str] = &[
        "ARM",
        "WPN",
        "LOOT",
        "CON",
        "GRN",
        "SCROLL",
        "SKILLBOOK",
        "ITEM",
        "TOOL",
        "FOOD",
        "FUR",
        "GEN",
    ];
    stats_id
        .split('_')
        .filter(|part| !part.is_empty() && !PREFIXES.contains(part) && *part != "A" && *part != "B")
        .collect::<Vec<_>>()
        .join(" ")
}

fn item_editor(ui: &mut egui::Ui, index: usize, node: &mut Node, catalog: Option<&StatsCatalog>) {
    let stats_id = node
        .attr("Stats")
        .and_then(|attribute| match attribute {
            AttributeValue::Str(value) => Some(value.as_str()),
            _ => None,
        })
        .unwrap_or_default()
        .to_owned();
    let template = node
        .attr("CurrentTemplate")
        .and_then(|attribute| match attribute {
            AttributeValue::Uuid(value) => Some(value),
            _ => None,
        });
    let name = catalog
        .and_then(|catalog| catalog.display_name(template.copied(), &stats_id))
        .map(str::to_owned)
        .unwrap_or_else(|| friendly_stat_name(&stats_id));
    ui.heading(name);
    ui.small(format!("Stats ID: {stats_id} · Save item #{index}"));
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
            permanent_boost_editor(ui, boosts);
        }
    }
    if let Some(generation) = node
        .child_mut("Generation")
        .and_then(|node| node.child_mut("ItemGeneration"))
    {
        generated_bonus_editor(ui, generation);
    }
}

fn permanent_boost_editor(ui: &mut egui::Ui, boosts: &mut Node) {
    egui::CollapsingHeader::new("Permanent bonuses")
        .default_open(true)
        .show(ui, |ui| {
            let mut names: Vec<String> = boosts.attributes.keys().cloned().collect();
            names.sort();
            for name in names {
                ui.horizontal(|ui| {
                    ui.label(&name);
                    if let Some(attribute) = boosts.attributes.get_mut(&name) {
                        raw_attribute_editor(ui, attribute);
                    }
                });
            }
            for (kind, label) in [("Abilities", "Abilities"), ("Talents", "Talents")] {
                let Some(values) = boosts.children.get_mut(kind) else {
                    continue;
                };
                for (index, value) in values.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(format!("{label} {}", index + 1));
                        if let Some(attribute) = value.attributes.get_mut("Object") {
                            raw_attribute_editor(ui, attribute);
                        } else {
                            ui.label("None");
                        }
                    });
                }
            }
        });
}

fn generated_bonus_editor(ui: &mut egui::Ui, generation: &mut Node) {
    let Some(boosts) = generation.children.get_mut("Boost") else {
        return;
    };
    egui::CollapsingHeader::new("Generated item bonuses")
        .default_open(true)
        .show(ui, |ui| {
            ui.small("These are the rolled bonus definitions saved on this generated item.");
            for (index, boost) in boosts.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(format!("Bonus {}", index + 1));
                    if let Some(attribute) = boost.attributes.get_mut("Object") {
                        raw_attribute_editor(ui, attribute);
                    } else {
                        ui.label("<missing>");
                    }
                });
            }
        });
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

#[cfg(test)]
mod tests {
    use super::friendly_stat_name;

    #[test]
    fn derives_a_human_readable_fallback_from_a_stat_id() {
        assert_eq!(
            friendly_stat_name("WPN_UNIQUE_ARX_BrahmosSword_2H"),
            "UNIQUE ARX BrahmosSword 2H"
        );
    }
}
