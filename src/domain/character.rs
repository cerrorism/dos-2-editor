//! Typed views over player characters in `Regions["Characters"]`.
use crate::format::node::{AttributeValue, Node, Resource};

/// The playable party, identified by `Stats.IsPlayer` in the save.
pub fn party_members(resource: &Resource) -> Vec<Character<'_>> {
    let Some(characters) = resource
        .regions
        .get("Characters")
        .and_then(|region| region.child("CharacterFactory"))
        .and_then(|factory| factory.child("Characters"))
        .map(|list| list.children_of("Character"))
    else {
        return Vec::new();
    };
    characters
        .iter()
        .filter(|node| {
            node.child("Stats")
                .and_then(|stats| bool_attr(stats, "IsPlayer"))
                .unwrap_or(false)
        })
        .map(|node| Character { node })
        .collect()
}

pub struct Character<'a> {
    node: &'a Node,
}

impl<'a> Character<'a> {
    pub fn node(&self) -> &'a Node {
        self.node
    }

    /// Engine handle used by `Item.OriginalOwnerCharacter` / `Item.Parent`.
    pub fn handle(&self) -> Option<u64> {
        u64_attr(self.node, "Handle").or_else(|| u64_attr(self.node, "MyGuid"))
    }

    pub fn inventory_handle(&self) -> Option<u64> {
        u64_attr(self.node, "Inventory")
    }

    pub fn name(&self) -> Option<&'a str> {
        self.node
            .child("PlayerData")?
            .child("PlayerCustomData")
            .and_then(|data| string_attr(data, "Name"))
    }

    pub fn origin_name(&self) -> Option<&'a str> {
        self.node
            .child("PlayerData")?
            .child("PlayerCustomData")
            .and_then(|data| string_attr(data, "OriginName"))
    }

    pub fn level(&self) -> Option<i32> {
        self.node
            .child("Stats")
            .and_then(|stats| integer_attr(stats, "Level"))
    }
}

fn string_attr<'a>(node: &'a Node, name: &str) -> Option<&'a str> {
    match node.attr(name)? {
        AttributeValue::Str(value) => Some(value),
        AttributeValue::TranslatedString(value) => value.value.as_deref(),
        _ => None,
    }
}

fn bool_attr(node: &Node, name: &str) -> Option<bool> {
    match node.attr(name)? {
        AttributeValue::Bool(value) => Some(*value),
        _ => None,
    }
}

fn integer_attr(node: &Node, name: &str) -> Option<i32> {
    match node.attr(name)? {
        AttributeValue::I32(value) => Some(*value),
        AttributeValue::I8(value) => Some((*value).into()),
        AttributeValue::U8(value) => Some((*value).into()),
        AttributeValue::U16(value) => Some((*value).into()),
        _ => None,
    }
}

fn u64_attr(node: &Node, name: &str) -> Option<u64> {
    match node.attr(name)? {
        AttributeValue::U64(value) => Some(*value),
        AttributeValue::U32(value) => Some((*value).into()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::node::{AttributeType, AttributeValue};

    #[test]
    fn only_returns_player_characters() {
        let mut stats = Node::new("Stats");
        stats.set_attr("IsPlayer", AttributeType::Bool, AttributeValue::Bool(true));
        let mut data = Node::new("PlayerCustomData");
        data.set_attr(
            "Name",
            AttributeType::FixedString,
            AttributeValue::Str("Lohse".into()),
        );
        let mut player_data = Node::new("PlayerData");
        player_data.push_child(data);
        let mut player = Node::new("Character");
        player.set_attr("Handle", AttributeType::ULongLong, AttributeValue::U64(42));
        player.push_child(stats);
        player.push_child(player_data);
        let mut npc = Node::new("Character");
        npc.push_child(Node::new("Stats"));
        let mut list = Node::new("Characters");
        list.push_child(player);
        list.push_child(npc);
        let mut factory = Node::new("CharacterFactory");
        factory.push_child(list);
        let mut region = Node::new("Characters");
        region.push_child(factory);
        let resource = Resource {
            regions: [("Characters".into(), region)].into_iter().collect(),
        };

        let party = party_members(&resource);
        assert_eq!(party.len(), 1);
        assert_eq!(party[0].name(), Some("Lohse"));
        assert_eq!(party[0].handle(), Some(42));
    }
}
