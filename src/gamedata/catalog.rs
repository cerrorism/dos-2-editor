//! Lightweight item-template and localized-name catalog built from the installed game.
use std::collections::HashMap;
use std::path::Path;

use uuid::Uuid;

use crate::format::lsf;
use crate::format::node::{AttributeValue, Node};
use crate::format::pak::Pak;

use super::localization;

const SHARED_PAK: &str = "DefEd/Data/Shared.pak";
const ROOT_TEMPLATES: &str = "Public/Shared/RootTemplates/_merged.lsf";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayLanguage {
    English,
    SimplifiedChinese,
}

impl DisplayLanguage {
    pub const ALL: [Self; 2] = [Self::SimplifiedChinese, Self::English];

    pub fn label(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::SimplifiedChinese => "简体中文",
        }
    }

    fn pak_path(self) -> &'static str {
        match self {
            Self::English => "DefEd/Data/Localization/English.pak",
            Self::SimplifiedChinese => "DefEd/Data/Localization/Chinese/Chinese.pak",
        }
    }

    fn xml_path(self) -> &'static str {
        match self {
            Self::English => "Localization/English/english.xml",
            Self::SimplifiedChinese => "Localization/Chinese/chinese.xml",
        }
    }
}

#[derive(Default)]
pub struct StatsCatalog {
    localized: HashMap<String, String>,
    template_names: HashMap<Uuid, String>,
    stats_names: HashMap<String, String>,
}

impl StatsCatalog {
    pub fn load(game_root: &Path, language: DisplayLanguage) -> Result<Self, String> {
        let bytes = Pak::open(&game_root.join(language.pak_path()))?.read(language.xml_path())?;
        let xml = std::str::from_utf8(&bytes)
            .map_err(|error| format!("{} localization is not UTF-8: {error}", language.label()))?;
        let localized = localization::parse_xml(xml);

        let templates = Pak::open(&game_root.join(SHARED_PAK))?.read(ROOT_TEMPLATES)?;
        let resource = lsf::parse(&templates)?;
        let mut catalog = Self {
            localized,
            ..Self::default()
        };
        for region in resource.regions.values() {
            catalog.visit_template_node(region);
        }
        Ok(catalog)
    }

    pub fn display_name(&self, template: Option<Uuid>, stats_id: &str) -> Option<&str> {
        template
            .and_then(|template| self.template_names.get(&template))
            .or_else(|| self.stats_names.get(stats_id))
            .map(String::as_str)
    }

    pub fn localization_count(&self) -> usize {
        self.localized.len()
    }
    pub fn template_name_count(&self) -> usize {
        self.template_names.len()
    }
    pub fn stats_name_count(&self) -> usize {
        self.stats_names.len()
    }

    fn visit_template_node(&mut self, node: &Node) {
        let display_name = translated_handle(node, "DisplayName")
            .and_then(|handle| self.localized.get(handle))
            .cloned();
        if let Some(name) = display_name {
            if let Some(uuid) = uuid_attr(node, "MapKey").or_else(|| uuid_attr(node, "UUID")) {
                self.template_names.insert(uuid, name.clone());
            }
            if let Some(stats) = string_attr(node, "Stats") {
                self.stats_names.entry(stats.to_owned()).or_insert(name);
            }
        }
        for children in node.children.values() {
            for child in children {
                self.visit_template_node(child);
            }
        }
    }
}

fn string_attr<'a>(node: &'a Node, name: &str) -> Option<&'a str> {
    match node.attr(name)? {
        AttributeValue::Str(value) => Some(value),
        _ => None,
    }
}

fn uuid_attr(node: &Node, name: &str) -> Option<Uuid> {
    match node.attr(name)? {
        AttributeValue::Uuid(value) => Some(*value),
        AttributeValue::Str(value) => value.parse().ok(),
        _ => None,
    }
}

fn translated_handle<'a>(node: &'a Node, name: &str) -> Option<&'a str> {
    match node.attr(name)? {
        AttributeValue::TranslatedString(value) => Some(&value.handle),
        AttributeValue::Str(value) if value.starts_with('h') => Some(value),
        _ => None,
    }
}
