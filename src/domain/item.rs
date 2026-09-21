//! Typed views over `Regions["Items"]` in `globals.lsf`.
//!
//! This is deliberately a thin layer over [`Node`]. Unmodelled fields remain
//! available unchanged through the generic format tree.
use crate::format::node::{AttributeType, AttributeValue, Node, Resource};
use uuid::Uuid;

/// Saved items and their same-index `Creator` records. DOS2 stores the two
/// lists in parallel rather than nesting creators under each item.
pub fn items(resource: &Resource) -> Vec<Item<'_>> {
    let Some(factory) = resource
        .regions
        .get("Items")
        .and_then(|region| region.child("ItemFactory"))
    else {
        return Vec::new();
    };
    let item_nodes = factory
        .child("Items")
        .map(|n| n.children_of("Item"))
        .unwrap_or(&[]);
    let creators = factory
        .child("Creators")
        .map(|n| n.children_of("Creator"))
        .unwrap_or(&[]);
    item_nodes
        .iter()
        .enumerate()
        .map(|(index, node)| Item {
            node,
            creator: creators.get(index),
        })
        .collect()
}

/// Returns an item node by its stable position in the saved `Items` list.
/// The position is the same one reported by [`items`].
pub fn item_node_mut(resource: &mut Resource, index: usize) -> Option<&mut Node> {
    resource
        .regions
        .get_mut("Items")?
        .child_mut("ItemFactory")?
        .child_mut("Items")?
        .children
        .get_mut("Item")?
        .get_mut(index)
}

pub struct Item<'a> {
    node: &'a Node,
    creator: Option<&'a Node>,
}

impl<'a> Item<'a> {
    pub fn node(&self) -> &'a Node {
        self.node
    }
    pub fn handle(&self) -> Option<u64> {
        self.creator.and_then(|node| u64_attr(node, "Handle"))
    }
    pub fn stats_id(&self) -> Option<&'a str> {
        str_attr(self.node, "Stats")
    }
    /// `-1` means a non-stackable item and must not be range-clamped by a UI.
    pub fn amount(&self) -> Option<i32> {
        integer_attr(self.node, "Amount")
    }
    pub fn is_generated(&self) -> Option<bool> {
        bool_attr(self.node, "IsGenerated")
    }
    pub fn inventory_id(&self) -> Option<u64> {
        u64_attr(self.node, "Inventory")
    }
    pub fn slot(&self) -> Option<u16> {
        u16_attr(self.node, "Slot")
    }
    pub fn current_template(&self) -> Option<Uuid> {
        match self.node.attr("CurrentTemplate")? {
            AttributeValue::Uuid(value) => Some(*value),
            _ => None,
        }
    }
    pub fn parent_handle(&self) -> Option<u64> {
        u64_attr(self.node, "Parent")
    }
    /// DOS2's item owner is an engine handle (`ULongLong`), not an LSF UUID.
    pub fn original_owner(&self) -> Option<u64> {
        u64_attr(self.node, "OriginalOwnerCharacter")
    }
    pub fn level_override_exists(&self) -> bool {
        self.node.child("LevelOverride").is_some()
    }

    pub fn item_type(&self) -> Option<&'a str> {
        self.stats().and_then(|n| str_attr(n, "ItemType"))
    }
    pub fn level(&self) -> Option<i32> {
        self.stats().and_then(|n| integer_attr(n, "Level"))
    }
    pub fn level_group_index(&self) -> Option<i32> {
        self.stats()
            .and_then(|n| integer_attr(n, "LevelGroupIndex"))
    }
    pub fn name_index(&self) -> Option<i32> {
        self.stats().and_then(|n| integer_attr(n, "NameIndex"))
    }
    pub fn rune_stats_id(&self, slot: usize) -> Option<&'a str> {
        self.stats()?
            .children_of("RuneSlot")
            .get(slot)
            .and_then(|node| str_attr(node, "RuneStatsID"))
    }
    pub fn custom_display_name(&self) -> Option<&'a str> {
        self.node
            .child("CustomDisplayName")
            .and_then(|node| str_attr(node, "CustomDisplayName"))
    }
    pub fn custom_description(&self) -> Option<&'a str> {
        self.node
            .child("CustomDescription")
            .and_then(|node| str_attr(node, "CustomDescription"))
    }
    pub fn bonus(&self, name: &str) -> Option<i32> {
        self.stats()?
            .child("PermanentBoost")
            .and_then(|node| integer_attr(node, name))
    }
    fn stats(&self) -> Option<&'a Node> {
        self.node.child("Stats")
    }
}

/// Mutates only well-known existing item fields. Future UI code can use this
/// instead of duplicating tree traversal and accidentally changing types.
pub struct ItemMut<'a> {
    node: &'a mut Node,
}

impl<'a> ItemMut<'a> {
    pub fn new(node: &'a mut Node) -> Self {
        Self { node }
    }
    pub fn set_stats_id(&mut self, value: impl Into<String>) {
        set_string(self.node, "Stats", value.into())
    }
    pub fn set_amount(&mut self, value: i32) {
        set_i32(self.node, "Amount", value)
    }
    pub fn set_rune_stats_id(&mut self, slot: usize, value: impl Into<String>) -> bool {
        let Some(rune) = self
            .node
            .child_mut("Stats")
            .and_then(|node| node.children.get_mut("RuneSlot"))
            .and_then(|slots| slots.get_mut(slot))
        else {
            return false;
        };
        set_string(rune, "RuneStatsID", value.into());
        true
    }
    pub fn set_bonus(&mut self, name: &str, value: i32) -> bool {
        let Some(boosts) = self
            .node
            .child_mut("Stats")
            .and_then(|node| node.child_mut("PermanentBoost"))
        else {
            return false;
        };
        set_i32(boosts, name, value);
        true
    }
}

fn str_attr<'a>(node: &'a Node, name: &str) -> Option<&'a str> {
    match node.attr(name)? {
        AttributeValue::Str(value) => Some(value),
        _ => None,
    }
}

fn integer_attr(node: &Node, name: &str) -> Option<i32> {
    match node.attr(name)? {
        AttributeValue::I32(value) => Some(*value),
        AttributeValue::U32(value) => i32::try_from(*value).ok(),
        AttributeValue::I64(value) => i32::try_from(*value).ok(),
        AttributeValue::U8(value) => Some((*value).into()),
        AttributeValue::I16(value) => Some((*value).into()),
        AttributeValue::U16(value) => Some((*value).into()),
        AttributeValue::I8(value) => Some((*value).into()),
        _ => None,
    }
}

fn u64_attr(node: &Node, name: &str) -> Option<u64> {
    match node.attr(name)? {
        AttributeValue::U64(value) => Some(*value),
        AttributeValue::U32(value) => Some((*value).into()),
        AttributeValue::U16(value) => Some((*value).into()),
        AttributeValue::U8(value) => Some((*value).into()),
        _ => None,
    }
}

fn u16_attr(node: &Node, name: &str) -> Option<u16> {
    match node.attr(name)? {
        AttributeValue::U16(value) => Some(*value),
        AttributeValue::U8(value) => Some((*value).into()),
        AttributeValue::U32(value) => u16::try_from(*value).ok(),
        AttributeValue::I32(value) => u16::try_from(*value).ok(),
        _ => None,
    }
}

fn bool_attr(node: &Node, name: &str) -> Option<bool> {
    match node.attr(name)? {
        AttributeValue::Bool(value) => Some(*value),
        _ => None,
    }
}

fn set_string(node: &mut Node, name: &str, value: String) {
    if let Some(attribute) = node.attributes.get_mut(name) {
        if matches!(attribute.value, AttributeValue::Str(_)) {
            attribute.value = AttributeValue::Str(value);
            return;
        }
    }
    node.set_attr(name, AttributeType::FixedString, AttributeValue::Str(value));
}

fn set_i32(node: &mut Node, name: &str, value: i32) {
    if let Some(attribute) = node.attributes.get_mut(name) {
        if matches!(attribute.value, AttributeValue::I32(_)) {
            attribute.value = AttributeValue::I32(value);
            return;
        }
    }
    node.set_attr(name, AttributeType::Int, AttributeValue::I32(value));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_resource() -> Resource {
        let mut item = Node::new("Item");
        item.set_attr(
            "Stats",
            AttributeType::FixedString,
            AttributeValue::Str("WPN_Sword_A".into()),
        );
        item.set_attr("Amount", AttributeType::Int, AttributeValue::I32(-1));
        let mut stats = Node::new("Stats");
        let mut rune = Node::new("RuneSlot");
        rune.set_attr(
            "RuneStatsID",
            AttributeType::FixedString,
            AttributeValue::Str("LOOT_Rune_Flame_A".into()),
        );
        stats.push_child(rune);
        let mut boosts = Node::new("PermanentBoost");
        boosts.set_attr("Strength", AttributeType::Int, AttributeValue::I32(2));
        stats.push_child(boosts);
        item.push_child(stats);
        let mut item_list = Node::new("Items");
        item_list.push_child(item);
        let mut creator = Node::new("Creator");
        creator.set_attr(
            "Handle",
            AttributeType::ULongLong,
            AttributeValue::U64(0x0100_0000_0000_0001),
        );
        let mut creators = Node::new("Creators");
        creators.push_child(creator);
        let mut factory = Node::new("ItemFactory");
        factory.push_child(item_list);
        factory.push_child(creators);
        let mut region = Node::new("Items");
        region.push_child(factory);
        Resource {
            regions: [("Items".into(), region)].into_iter().collect(),
        }
    }

    #[test]
    fn follows_the_real_item_and_creator_layout() {
        let resource = sample_resource();
        let all = items(&resource);
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].handle(), Some(0x0100_0000_0000_0001));
        assert_eq!(all[0].stats_id(), Some("WPN_Sword_A"));
        assert_eq!(all[0].amount(), Some(-1));
        assert_eq!(all[0].rune_stats_id(0), Some("LOOT_Rune_Flame_A"));
        assert_eq!(all[0].bonus("Strength"), Some(2));
    }

    #[test]
    fn mutators_preserve_existing_string_type() {
        let mut resource = sample_resource();
        let item = resource
            .regions
            .get_mut("Items")
            .unwrap()
            .child_mut("ItemFactory")
            .unwrap()
            .child_mut("Items")
            .unwrap()
            .children
            .get_mut("Item")
            .unwrap()
            .first_mut()
            .unwrap();
        let mut view = ItemMut::new(item);
        view.set_stats_id("ARM_Helmet_A");
        assert!(view.set_bonus("Strength", 4));
        assert_eq!(view.node.attributes["Stats"].ty, AttributeType::FixedString);
        assert_eq!(
            view.node.attr("Stats"),
            Some(&AttributeValue::Str("ARM_Helmet_A".into()))
        );
        assert_eq!(
            view.node
                .child("Stats")
                .unwrap()
                .child("PermanentBoost")
                .unwrap()
                .attr("Strength"),
            Some(&AttributeValue::I32(4))
        );
    }

    #[test]
    fn mutable_lookup_uses_the_same_item_index_as_the_read_view() {
        let mut resource = sample_resource();
        let node = item_node_mut(&mut resource, 0).unwrap();
        ItemMut::new(node).set_stats_id("ARM_Helmet_A");
        assert_eq!(items(&resource)[0].stats_id(), Some("ARM_Helmet_A"));
    }
}
