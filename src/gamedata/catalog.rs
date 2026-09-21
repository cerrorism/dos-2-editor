//! Lightweight item-template and localized-name catalog built from the installed game.
use std::collections::HashMap;
use std::path::Path;

use uuid::Uuid;

use crate::format::lsf;
use crate::format::node::{AttributeValue, Node};
use crate::format::pak::Pak;

use super::localization;
use super::statfile::StatDatabase;

const SHARED_PAK: &str = "DefEd/Data/Shared.pak";
const ROOT_TEMPLATES: &str = "Public/Shared/RootTemplates/_merged.lsf";
const ENGLISH_PAK: &str = "DefEd/Data/Localization/English.pak";
const ENGLISH_XML: &str = "Localization/English/english.xml";
const ITEM_PROGRESSION_NAMES: &str = "Public/Shared/Stats/Generated/Data/ItemProgressionNames.txt";
const ITEM_PROGRESSION_STRINGS: &str = "Public/Shared/Localization/ItemProgression.lsb";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DisplayLanguage {
    English,
    #[default]
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
    english_localized: HashMap<String, String>,
    template_names: HashMap<Uuid, String>,
    stats_names: HashMap<String, String>,
    generated_stats: StatDatabase,
    progression_names: HashMap<String, Vec<String>>,
    progression_handles: HashMap<String, String>,
    language: DisplayLanguage,
}

#[derive(Clone, Debug, Default)]
pub struct GeneratedItemDetails {
    pub requirements: Vec<String>,
    pub bonuses: Vec<String>,
}

impl StatsCatalog {
    pub fn load(game_root: &Path, language: DisplayLanguage) -> Result<Self, String> {
        let bytes = Pak::open(&game_root.join(language.pak_path()))?.read(language.xml_path())?;
        let xml = std::str::from_utf8(&bytes)
            .map_err(|error| format!("{} localization is not UTF-8: {error}", language.label()))?;
        let localized = localization::parse_xml(xml);
        let english_localized = if language == DisplayLanguage::English {
            localized.clone()
        } else {
            let english = Pak::open(&game_root.join(ENGLISH_PAK))?.read(ENGLISH_XML)?;
            let english = std::str::from_utf8(&english)
                .map_err(|error| format!("English localization is not UTF-8: {error}"))?;
            localization::parse_xml(english)
        };

        let shared = Pak::open(&game_root.join(SHARED_PAK))?;
        let templates = shared.read(ROOT_TEMPLATES)?;
        let resource = lsf::parse(&templates)?;
        let mut catalog = Self {
            localized,
            english_localized,
            language,
            ..Self::default()
        };
        for region in resource.regions.values() {
            catalog.visit_template_node(region);
        }
        for entry in &shared.entries {
            if entry
                .name
                .starts_with("Public/Shared/Stats/Generated/Data/")
                && entry.name.ends_with(".txt")
            {
                let bytes = entry.read()?;
                let text = std::str::from_utf8(&bytes).map_err(|error| {
                    format!("generated stat file {} is not UTF-8: {error}", entry.name)
                })?;
                catalog.generated_stats.parse_and_merge(text);
            }
        }
        let names = shared.read(ITEM_PROGRESSION_NAMES)?;
        let names = std::str::from_utf8(&names)
            .map_err(|error| format!("item progression names are not UTF-8: {error}"))?;
        catalog.progression_names = parse_progression_names(names);
        let strings = shared.read(ITEM_PROGRESSION_STRINGS)?;
        catalog.progression_handles = progression_handles(&strings, &catalog.progression_names);
        Ok(catalog)
    }

    pub fn display_name(&self, template: Option<Uuid>, stats_id: &str) -> Option<&str> {
        template
            .and_then(|template| self.template_names.get(&template))
            .or_else(|| self.stats_names.get(stats_id))
            .map(String::as_str)
    }

    pub fn generated_name(
        &self,
        stats_id: &str,
        item_type: Option<&str>,
        level: Option<i32>,
        level_group_index: Option<i32>,
        name_index: Option<i32>,
    ) -> Option<&str> {
        let _ = (level, level_group_index); // Full group selection will use these fields.
        let name_index = usize::try_from(name_index?).ok()?;
        let fields = self.generated_stats.resolved_fields(stats_id);
        let group = progression_group(fields.get("ItemGroup")?, item_type?)?;
        let english_name = self.progression_names.get(&group)?.get(name_index)?;
        let handle = self.progression_handles.get(english_name)?;
        self.localized.get(handle).map(String::as_str)
    }

    pub fn localized_text(&self, english: &str) -> String {
        if self.language == DisplayLanguage::English {
            return english.to_owned();
        }
        self.english_localized
            .iter()
            .find_map(|(handle, text)| {
                (text == english)
                    .then(|| self.localized.get(handle))
                    .flatten()
            })
            .cloned()
            .unwrap_or_else(|| english.to_owned())
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

    pub fn generated_item_details(
        &self,
        stats_id: &str,
        boost_ids: impl IntoIterator<Item = String>,
    ) -> GeneratedItemDetails {
        let base = self.generated_stats.resolved_fields(stats_id);
        let requirements = base
            .get("Requirements")
            .map(|requirements| vec![self.requirement_line(requirements)])
            .unwrap_or_default();
        let bonuses = boost_ids
            .into_iter()
            .flat_map(|boost_id| {
                let fields = self.generated_stats.resolved_fields(&boost_id);
                let fields = if fields.is_empty() {
                    self.generated_stats
                        .resolved_fields(&format!("_{boost_id}"))
                } else {
                    fields
                };
                fields
                    .into_iter()
                    .filter_map(|(key, value)| self.bonus_line(&key, &value))
                    .collect::<Vec<_>>()
            })
            .collect();
        GeneratedItemDetails {
            requirements,
            bonuses,
        }
    }

    fn requirement_line(&self, value: &str) -> String {
        let value = value
            .replace("Finesse", &self.localized_text("Finesse"))
            .replace("Strength", &self.localized_text("Strength"))
            .replace("Intelligence", &self.localized_text("Intelligence"));
        match self.language {
            DisplayLanguage::English => format!("Requires {value}"),
            DisplayLanguage::SimplifiedChinese => format!("需要{value}"),
        }
    }

    fn bonus_line(&self, key: &str, value: &str) -> Option<String> {
        let label = match key {
            "FinesseBoost" => "Finesse",
            "StrengthBoost" => "Strength",
            "IntelligenceBoost" => "Intelligence",
            "ConstitutionBoost" => "Constitution",
            "MemoryBoost" => "Memory",
            "WitsBoost" => "Wits",
            "WarriorLore" => "Warfare",
            "RogueLore" => "Scoundrel",
            "AirSpecialist" => "Aerotheurge",
            "WaterSpecialist" => "Hydrosophist",
            "FireSpecialist" => "Pyrokinetic",
            "EarthSpecialist" => "Geomancer",
            "Summoning" => "Summoning",
            "Huntsman" => "Huntsman",
            "Polymorph" => "Polymorph",
            _ => return None,
        };
        let numeric = value.parse::<i32>().ok()?;
        (numeric != 0).then(|| format!("{numeric:+} {}", self.localized_text(label)))
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

fn parse_progression_names(source: &str) -> HashMap<String, Vec<String>> {
    let mut groups = HashMap::new();
    let mut current: Option<(String, Vec<String>)> = None;
    for line in source.lines().map(str::trim) {
        if let Some(name) = quoted_after(line, "new namegroup ") {
            if let Some((name, values)) = current.take() {
                groups.insert(name, values);
            }
            current = Some((name.to_owned(), Vec::new()));
        } else if let Some(name) = quoted_after(line, "add name ") {
            if let Some((_, values)) = &mut current {
                values.push(name.to_owned());
            }
        }
    }
    if let Some((name, values)) = current {
        groups.insert(name, values);
    }
    groups
}

fn progression_handles(
    binary: &[u8],
    groups: &HashMap<String, Vec<String>>,
) -> HashMap<String, String> {
    let mut handles = HashMap::new();
    for name in groups.values().flatten() {
        let needle = [name.as_bytes(), &[0]].concat();
        let mut start = 0;
        while let Some(offset) = find_bytes(&binary[start..], &needle) {
            let offset = start + offset + needle.len();
            let Some(length_bytes) = binary.get(offset..offset + 4) else {
                break;
            };
            let length = u32::from_le_bytes(length_bytes.try_into().unwrap()) as usize;
            let Some(handle) = binary.get(offset + 4..offset + 4 + length) else {
                break;
            };
            if let Ok(handle) = std::str::from_utf8(handle) {
                if handle.starts_with('h') {
                    handles
                        .entry(name.clone())
                        .or_insert_with(|| handle.trim_end_matches('\0').to_owned());
                    break;
                }
            }
            start = offset;
        }
    }
    handles
}

fn progression_group(item_group: &str, rarity: &str) -> Option<String> {
    let armour = if item_group.starts_with("Light") {
        "LightArmour"
    } else if item_group.starts_with("Heavy") {
        "HeavyArmour"
    } else if item_group.starts_with("Mage") {
        "MageArmour"
    } else {
        return None;
    };
    Some(format!("RG_{armour}_{rarity}"))
}

fn quoted_after<'a>(line: &'a str, prefix: &str) -> Option<&'a str> {
    let rest = line.strip_prefix(prefix)?.trim_start().strip_prefix('"')?;
    rest.split_once('"').map(|(value, _)| value)
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}
