//! Lightweight item-template and localized-name catalog built from the installed game.
//!
//! DOS2:DE splits its RootTemplates/generated-stats/item-progression data
//! across several content paks, not just `Shared.pak`: the actual
//! campaign content (including most named/unique items and the origin
//! companions' own character templates) lives in `Origins.pak`
//! (`Public/DivinityOrigins_<guid>/...`), with further contributions from
//! `GameMaster.pak` and `SharedDOS.pak`. Only the per-language
//! localization pak (`English.pak`/`Chinese.pak`/...) is a single
//! merged file covering the whole game's text — RootTemplates/stats
//! content is not. Confirmed empirically via `dump_pak` against a real
//! install; merging only `Shared.pak` (the original implementation)
//! left most weapons/potions/food/scrolls/etc. unresolved because their
//! RootTemplate lives in `Origins.pak`.
use std::collections::HashMap;
use std::path::Path;

use uuid::Uuid;

use crate::format::node::{AttributeValue, Node};
use crate::format::pak::Pak;
use crate::format::{lsb, lsf};

use super::localization;
use super::statfile::StatDatabase;

/// Paks (relative to the game install root's `DefEd/Data/`) that
/// contribute RootTemplates/generated-stats/item-progression data.
/// `Origins.pak` is the main DOS2 campaign; `GameMaster.pak` is GM-mode
/// content; `SharedDOS.pak` is a small DOS2-specific supplement to
/// `Shared.pak`'s engine-wide base. Patches were checked and found to
/// carry only zero-length placeholder entries for these paths (i.e. no
/// real overrides), so they're intentionally not included.
const CONTENT_PAKS: &[&str] = &["Shared.pak", "SharedDOS.pak", "Origins.pak", "GameMaster.pak"];
const ENGLISH_PAK: &str = "DefEd/Data/Localization/English.pak";
const ENGLISH_XML: &str = "Localization/English/english.xml";

/// Internal stat/attribute/ability identifiers that appear as flat
/// `PermanentBoost` attributes, `Boost`/`Object` values, or
/// `Abilities`/`Talents` entries, paired with the canonical English UI
/// label used to look up their localized text (see `label_translations`
/// below). Not exhaustive, but covers the field tables the project plan
/// documents for DOS2:DE's Characters/Items schema.
const BOOST_LABELS: &[(&str, &str)] = &[
    ("FinesseBoost", "Finesse"),
    ("StrengthBoost", "Strength"),
    ("IntelligenceBoost", "Intelligence"),
    ("ConstitutionBoost", "Constitution"),
    ("MemoryBoost", "Memory"),
    ("WitsBoost", "Wits"),
    ("Finesse", "Finesse"),
    ("Strength", "Strength"),
    ("Intelligence", "Intelligence"),
    ("Constitution", "Constitution"),
    ("Memory", "Memory"),
    ("Wits", "Wits"),
    ("WarriorLore", "Warfare"),
    ("RogueLore", "Scoundrel"),
    ("RangerLore", "Huntsman"),
    ("AirSpecialist", "Aerotheurge"),
    ("WaterSpecialist", "Hydrosophist"),
    ("FireSpecialist", "Pyrokinetic"),
    ("EarthSpecialist", "Geomancer"),
    ("Necromancy", "Necromancer"),
    ("Summoning", "Summoning"),
    ("Polymorph", "Polymorph"),
    ("Sourcery", "Sourcery"),
    ("DualWielding", "Dual Wielding"),
    ("TwoHanded", "Two-Handed"),
    ("SingleHanded", "Single-Handed"),
    ("Ranged", "Ranged"),
    ("PainReflection", "Pain Reflection"),
    ("Leadership", "Leadership"),
    ("Perseverance", "Perseverance"),
    ("Luck", "Luck"),
    ("Barter", "Bartering"),
    ("Persuasion", "Persuasion"),
    ("Loremaster", "Loremaster"),
    ("Telekinesis", "Telekinesis"),
    ("Thievery", "Thievery"),
    ("Sneaking", "Sneaking"),
    ("PiercingResistance", "Piercing Resistance"),
    ("PhysicalResistance", "Physical Resistance"),
    ("FireResistance", "Fire Resistance"),
    ("WaterResistance", "Water Resistance"),
    ("EarthResistance", "Earth Resistance"),
    ("AirResistance", "Air Resistance"),
    ("PoisonResistance", "Poison Resistance"),
    ("DamageBoost", "Damage"),
    ("MagicArmorValue", "Magic Armor"),
    ("ArmorValue", "Armor"),
    ("CriticalChance", "Critical Chance"),
    ("Accuracy", "Accuracy"),
    ("Dodge", "Dodge"),
    ("Vitality", "Vitality"),
    ("LifeSteal", "Life Steal"),
    ("Blocking", "Blocking"),
    ("Movement", "Movement Speed"),
    ("Initiative", "Initiative"),
];

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
    /// Stats-ID -> localization handle, from `Localization/Stats.lsb`
    /// (an LSB `TranslatedStringKeys` table). This is where most static,
    /// non-equipment items (potions, food, scrolls, tools, grenades)
    /// actually get their display name — their RootTemplate has no
    /// `DisplayName` at all, and they're not part of the generated-loot
    /// namegroup system either (verified empirically against a real
    /// install: `POTION_Minor_Healing_Potion`/`FOOD_Cheese` have neither,
    /// but resolve cleanly through this table).
    stats_lsb_handles: HashMap<String, String>,
    generated_stats: StatDatabase,
    progression_names: HashMap<String, Vec<String>>,
    progression_handles: HashMap<String, String>,
    /// Precomputed once at load time (not per-frame/per-lookup): English
    /// `BOOST_LABELS` label text -> localized text, so the UI can show
    /// readable ability/attribute/resistance names without re-scanning
    /// the full ~90k-entry localization table on every widget redraw.
    label_translations: HashMap<&'static str, String>,
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

        let mut catalog = Self { localized, english_localized, language, ..Self::default() };

        let mut progression_lsb_blobs = Vec::new();
        for pak_name in CONTENT_PAKS {
            let pak = Pak::open(&game_root.join("DefEd/Data").join(pak_name))?;
            catalog.merge_content_pak(&pak, &mut progression_lsb_blobs)?;
        }
        for blob in &progression_lsb_blobs {
            let handles = progression_handles(blob, &catalog.progression_names);
            for (name, handle) in handles {
                catalog.progression_handles.entry(name).or_insert(handle);
            }
        }

        catalog.label_translations = BOOST_LABELS
            .iter()
            .map(|(_, english)| *english)
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .filter_map(|english| Some((english, catalog.translate_label(english)?)))
            .collect();

        Ok(catalog)
    }

    /// Reads every RootTemplates/`.lsf` file, every generated
    /// `Stats/Generated/Data/*.txt` file (except `ItemProgressionNames.txt`,
    /// which has its own `new namegroup`/`add name` grammar), the
    /// `ItemProgressionNames.txt` file itself, and queues up any
    /// `Localization/ItemProgression.lsb` blob for the caller to resolve
    /// once every pak's `progression_names` have been merged.
    fn merge_content_pak(&mut self, pak: &Pak, progression_lsb_blobs: &mut Vec<Vec<u8>>) -> Result<(), String> {
        for entry in &pak.entries {
            let name = entry.name.as_str();
            if name.contains("/RootTemplates/") && name.ends_with(".lsf") {
                let bytes = entry.read()?;
                let resource = lsf::parse(&bytes)?;
                for region in resource.regions.values() {
                    self.visit_template_node(region);
                }
            } else if name.ends_with("Stats/Generated/Data/ItemProgressionNames.txt") {
                let bytes = entry.read()?;
                let text = std::str::from_utf8(&bytes)
                    .map_err(|error| format!("{name} is not UTF-8: {error}"))?;
                for (group, values) in parse_progression_names(text) {
                    self.progression_names.entry(group).or_insert(values);
                }
            } else if name.contains("/Stats/Generated/Data/") && name.ends_with(".txt") {
                let bytes = entry.read()?;
                let text = std::str::from_utf8(&bytes)
                    .map_err(|error| format!("{name} is not UTF-8: {error}"))?;
                self.generated_stats.parse_and_merge(text);
            } else if name.ends_with("Localization/ItemProgression.lsb") {
                progression_lsb_blobs.push(entry.read()?);
            } else if name.ends_with("Localization/Stats.lsb") {
                let bytes = entry.read()?;
                let resource = lsb::parse(&bytes)?;
                for region in resource.regions.values() {
                    for key_node in region.children_of("TranslatedStringKey") {
                        let (Some(AttributeValue::Str(stats_id)), Some(AttributeValue::TranslatedString(content))) =
                            (key_node.attr("UUID"), key_node.attr("Content"))
                        else {
                            continue;
                        };
                        self.stats_lsb_handles.entry(stats_id.clone()).or_insert_with(|| content.handle.clone());
                    }
                }
            }
        }
        Ok(())
    }

    pub fn display_name(&self, template: Option<Uuid>, stats_id: &str) -> Option<&str> {
        template
            .and_then(|template| self.template_names.get(&template))
            .or_else(|| self.stats_names.get(stats_id))
            .map(String::as_str)
            .or_else(|| {
                let handle = self.stats_lsb_handles.get(stats_id)?;
                self.localized.get(handle).map(String::as_str)
            })
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
        // Hand-authored "Unique"-rarity generated items (still procedurally
        // boosted, but with a fixed one-off name) use their own `ItemGroup`
        // value as the namegroup key directly, rather than the synthesized
        // `RG_<class>_<rarity>` key used for ordinary randomized loot (which
        // has no "_Unique" variant at all — verified against a real install:
        // `ARM_UNIQUE_Hildurs_Plate_UpperBody`'s `ItemGroup` field is
        // literally `"ARM_UNIQUE_Hildurs_Plate_UpperBody"`).
        let group = fields
            .get("ItemGroup")
            .filter(|group| self.progression_names.contains_key(*group))
            .cloned()
            .or_else(|| progression_group(&fields, item_type?))?;
        let english_name = self.progression_names.get(&group)?.get(name_index)?;
        let handle = self.progression_handles.get(english_name)?;
        self.localized.get(handle).map(String::as_str)
    }

    pub fn localized_text(&self, english: &str) -> String {
        if self.language == DisplayLanguage::English {
            return english.to_owned();
        }
        self.translate_label(english).unwrap_or_else(|| english.to_owned())
    }

    /// Localized label for a known internal stat/ability/resistance key
    /// (see `BOOST_LABELS`) — `None` if `key` isn't one we know how to
    /// label at all (as opposed to knowing it but having no translation,
    /// which falls back to the English label).
    pub fn stat_label(&self, key: &str) -> Option<&str> {
        let (_, english) = BOOST_LABELS.iter().find(|(k, _)| *k == key)?;
        Some(self.label_translations.get(english).map(String::as_str).unwrap_or(english))
    }

    fn translate_label(&self, english: &str) -> Option<String> {
        self.english_localized.iter().find_map(|(handle, text)| {
            (text == english).then(|| self.localized.get(handle)).flatten().cloned()
        })
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
        let label = self.stat_label(key)?;
        let numeric = value.parse::<i32>().ok()?;
        (numeric != 0).then(|| format!("{numeric:+} {label}"))
    }

    fn visit_template_node(&mut self, node: &Node) {
        let display_name = translated_handle(node, "DisplayName")
            .and_then(|handle| self.localized.get(handle))
            .cloned();
        if let Some(name) = display_name {
            if let Some(uuid) = uuid_attr(node, "MapKey").or_else(|| uuid_attr(node, "UUID")) {
                self.template_names.entry(uuid).or_insert_with(|| name.clone());
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

/// Maps a generated item's resolved stat fields to one of the 63 real
/// `RG_<Class>_<Rarity>` namegroup keys (verified exhaustively against a
/// real install — see the project's diagnostic notes). `rarity` is the
/// item's `Stats.ItemType` value (`"Common"`/`"Rare"`/`"Epic"`/...) —
/// most classes only define `_Rare`/`_Epic` groups, so `Common`-rarity
/// generated gear correctly finds no group and falls back to the base
/// item's own name, matching what the game actually shows.
fn progression_group(fields: &std::collections::BTreeMap<String, String>, rarity: &str) -> Option<String> {
    let class = armor_class(fields).or_else(|| weapon_class(fields))?;
    Some(format!("RG_{class}_{rarity}"))
}

/// `{Light,Heavy,Mage}` + body-slot `ItemGroup` values map to a
/// same-prefix `RG_` class, but the *suffix* isn't uniform: Heavy boots
/// are named "Shoes" and Mage's chest/head pieces are "Robe"/"Hood", not
/// "Armour"/"Helmet" like the other two weight classes.
fn armor_class(fields: &std::collections::BTreeMap<String, String>) -> Option<String> {
    let item_group = fields.get("ItemGroup")?;
    let (prefix, rest) = ["Light", "Heavy", "Mage"]
        .into_iter()
        .find_map(|prefix| item_group.strip_prefix(prefix).map(|rest| (prefix, rest)))?;
    let suffix = match (prefix, rest) {
        (_, "LowerBody") => "Pants",
        (_, "Gloves") => "Gloves",
        ("Light" | "Heavy", "UpperBody") => "Armour",
        ("Mage", "UpperBody") => "Robe",
        ("Light" | "Mage", "Boots") => "Boots",
        ("Heavy", "Boots") => "Shoes",
        ("Light" | "Heavy", "Helmet") => "Helmet",
        ("Mage", "Helmet") => "Hood",
        _ => return None,
    };
    Some(format!("{prefix}{suffix}"))
}

/// Weapons/shields use `WeaponType`/`IsTwoHanded`/`Slot` instead of
/// `ItemGroup` (which for weapons is just the item's own Stats ID, not a
/// shared class). Spears/staves/bows/crossbows/daggers/wands only ever
/// have one handedness in the real namegroup list, so those skip the
/// 1H/2H suffix entirely.
fn weapon_class(fields: &std::collections::BTreeMap<String, String>) -> Option<String> {
    if fields.get("Slot").map(String::as_str) == Some("Shield") {
        return Some("Shields".to_owned());
    }
    let weapon_type = fields.get("WeaponType")?;
    let two_handed = fields.get("IsTwoHanded").map(String::as_str) == Some("Yes");
    let handedness = if two_handed { "2H" } else { "1H" };
    Some(match weapon_type.as_str() {
        "Sword" => format!("Swords_{handedness}"),
        "Axe" => format!("Axes_{handedness}"),
        "Mace" => format!("Maces_{handedness}"),
        "Spear" => "Spears_2H".to_owned(),
        "Staff" => "Staves_2H".to_owned(),
        "Bow" => "Bows".to_owned(),
        "Crossbow" => "Crossbows".to_owned(),
        "Knife" => "Daggers".to_owned(),
        "Wand" => "Wands_1H".to_owned(),
        _ => return None,
    })
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
