//! LSB binary resource format — the FW3/DOS2:DE variant only (signature
//! `0x40000000`; the BG3 `"LSFM"` variant is out of scope). Read-only:
//! this project only needs LSB to read game-data lookup tables (e.g.
//! `Public/Shared/Localization/Stats.lsb`, `ItemProgression.lsb`), never
//! to write one back. Byte layout verified directly against lslib's
//! `LSBReader.cs`/`Resource.cs` (MIT licensed, Norbyte) — same generic
//! `Resource`/`Node`/`NodeAttribute` model as `format::node`, just a
//! different (simpler, uncompressed) on-disk encoding: one flat static
//! string dictionary, then a region offset table, then a plain recursive
//! node tree (attribute values inline, children inline right after).
use std::collections::HashMap;

use super::node::{AttributeType, AttributeValue, Node, Resource, TranslatedString};
use super::primitives::Reader;

const SIGNATURE_FW3: u32 = 0x4000_0000;

pub fn parse(data: &[u8]) -> Result<Resource, String> {
    let mut r = Reader::new(data);
    if r.remaining() < 40 {
        return Err("file too small to be an LSB resource".into());
    }
    let signature = r.u32();
    if signature != SIGNATURE_FW3 {
        return Err(format!(
            "unsupported LSB signature {signature:#010x} — only the DOS2:DE (FW3) variant is supported"
        ));
    }
    let total_size = r.u32();
    if total_size as usize != data.len() {
        return Err(format!("corrupt LSB file: header declares size {total_size}, actual size {}", data.len()));
    }
    let big_endian = r.u32();
    if big_endian != 0 {
        return Err("big-endian LSB files are not supported".into());
    }
    let _unknown = r.u32();
    let _timestamp = r.u64();
    let _major = r.u32();
    let _minor = r.u32();
    let _revision = r.u32();
    let _build = r.u32();

    let strings = read_static_strings(&mut r)?;

    let num_regions = r.u32();
    let mut resource = Resource::default();
    for _ in 0..num_regions {
        let region_name_id = r.u32();
        let region_offset = r.u32() as usize;
        let region_name = strings
            .get(&region_name_id)
            .ok_or_else(|| format!("dangling region name id {region_name_id}"))?
            .clone();
        let mut region_reader = Reader::new(data);
        region_reader.seek(region_offset);
        let node = read_node(&mut region_reader, &strings)?;
        resource.regions.insert(region_name, node);
    }
    Ok(resource)
}

fn read_static_strings(r: &mut Reader) -> Result<HashMap<u32, String>, String> {
    let count = r.u32();
    let mut strings = HashMap::with_capacity(count as usize);
    for _ in 0..count {
        let text = read_length_prefixed_string(r, false);
        let index = r.u32();
        if strings.insert(index, text).is_some() {
            return Err(format!("duplicate static string id {index}"));
        }
    }
    Ok(strings)
}

fn read_node(r: &mut Reader, strings: &HashMap<u32, String>) -> Result<Node, String> {
    let name_id = r.u32();
    let attribute_count = r.u32();
    let child_count = r.u32();
    let name = strings.get(&name_id).ok_or_else(|| format!("dangling node name id {name_id}"))?.clone();
    let mut node = Node::new(name);

    for _ in 0..attribute_count {
        let attr_name_id = r.u32();
        let type_id = r.u32();
        let attr_name = strings
            .get(&attr_name_id)
            .ok_or_else(|| format!("dangling attribute name id {attr_name_id}"))?
            .clone();
        let ty = AttributeType::from_u32(type_id).ok_or_else(|| format!("unknown attribute type id {type_id}"))?;
        let value = read_attribute_value(r, ty)?;
        node.attributes.insert(attr_name, super::node::NodeAttribute { ty, value });
    }

    for _ in 0..child_count {
        let child = read_node(r, strings)?;
        node.push_child(child);
    }

    Ok(node)
}

/// LSB's length-prefixed string convention: `nullTerminated` strings
/// (used for attribute values) store `len+1` and are followed by a
/// trailing NUL byte; the static-string dictionary stores the exact
/// length with no terminator. Some real LSB files append stray extra
/// NUL bytes to translated-string values, so trailing NULs are always
/// trimmed defensively (matching `LSBReader.cs::ReadString`).
fn read_length_prefixed_string(r: &mut Reader, null_terminated: bool) -> String {
    let stored_len = r.i32();
    let len = if null_terminated { (stored_len - 1).max(0) as usize } else { stored_len.max(0) as usize };
    let mut bytes = r.bytes(len);
    while bytes.last() == Some(&0) {
        bytes = &bytes[..bytes.len() - 1];
    }
    let text = String::from_utf8_lossy(bytes).into_owned();
    if null_terminated {
        let _terminator = r.u8();
    }
    text
}

fn read_attribute_value(r: &mut Reader, ty: AttributeType) -> Result<AttributeValue, String> {
    Ok(match ty {
        AttributeType::String | AttributeType::Path | AttributeType::FixedString | AttributeType::LSString => {
            AttributeValue::Str(read_length_prefixed_string(r, true))
        }
        AttributeType::WString | AttributeType::LSWString => AttributeValue::Str(read_wide_string(r)),
        AttributeType::TranslatedString => {
            // DOS2:DE (FW3) is never the BG3 variant, so the inline value
            // is always present (version tag is always 0).
            let value = read_length_prefixed_string(r, true);
            let handle = read_length_prefixed_string(r, true);
            AttributeValue::TranslatedString(TranslatedString { version: 0, value: Some(value), handle })
        }
        AttributeType::ScratchBuffer => {
            let length = r.i32().max(0) as usize;
            AttributeValue::ScratchBuffer(r.bytes(length).to_vec())
        }
        AttributeType::TranslatedFSString => return Err("TranslatedFSString is not supported in LSB".into()),
        // Every other type is a plain little-endian numeric/vector/
        // matrix/bool/UUID value — identical encoding to LSF's Values
        // blob (both go through the same `BinUtils.ReadAttribute` in
        // lslib), so this mirrors `lsf::read_attribute_value`'s
        // corresponding arm exactly.
        _ => read_numeric_attribute(r, ty),
    })
}

fn read_wide_string(r: &mut Reader) -> String {
    let len = (r.i32() - 1).max(0) as usize;
    let units: Vec<u16> = (0..len).map(|_| r.u16()).collect();
    let _terminator = r.u16();
    String::from_utf16_lossy(&units)
}

fn read_numeric_attribute(r: &mut Reader, ty: AttributeType) -> AttributeValue {
    match ty {
        AttributeType::None => AttributeValue::None,
        AttributeType::Byte => AttributeValue::U8(r.u8()),
        AttributeType::Short => AttributeValue::I16(r.i16()),
        AttributeType::UShort => AttributeValue::U16(r.u16()),
        AttributeType::Int => AttributeValue::I32(r.i32()),
        AttributeType::UInt => AttributeValue::U32(r.u32()),
        AttributeType::Float => AttributeValue::F32(r.f32()),
        AttributeType::Double => AttributeValue::F64(r.f64()),
        AttributeType::IVec2 | AttributeType::IVec3 | AttributeType::IVec4 => {
            let n = ty.vec_len().unwrap();
            AttributeValue::IVec((0..n).map(|_| r.i32()).collect())
        }
        AttributeType::Vec2 | AttributeType::Vec3 | AttributeType::Vec4 => {
            let n = ty.vec_len().unwrap();
            AttributeValue::Vec((0..n).map(|_| r.f32()).collect())
        }
        AttributeType::Mat2 | AttributeType::Mat3 | AttributeType::Mat3x4 | AttributeType::Mat4x3 | AttributeType::Mat4 => {
            let (rows, cols) = ty.mat_dims().unwrap();
            let mut mat = vec![0f32; rows * cols];
            for col in 0..cols {
                for row in 0..rows {
                    mat[row * cols + col] = r.f32();
                }
            }
            AttributeValue::Mat(mat)
        }
        AttributeType::Bool => AttributeValue::Bool(r.u8() != 0),
        AttributeType::ULongLong => AttributeValue::U64(r.u64()),
        AttributeType::Long | AttributeType::Int64 => AttributeValue::I64(r.i64()),
        AttributeType::Int8 => AttributeValue::I8(r.i8()),
        AttributeType::Uuid => AttributeValue::Uuid(r.guid()),
        _ => unreachable!("string/translated/scratch types are handled by read_attribute_value"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::primitives::Writer;

    /// Hand-builds a minimal, real-shaped LSB buffer (header, static
    /// string table, one region, one node with a `String` and an `Int`
    /// attribute) and checks it parses to the expected `Resource` — the
    /// closest thing to a round-trip test this read-only reader can have.
    #[test]
    fn parses_a_minimal_synthetic_document() {
        // Static strings: 0="TestRegion", 1="Item", 2="Name", 3="Amount".
        let strings = ["TestRegion", "Item", "Name", "Amount"];
        let mut string_table = Writer::new();
        string_table.u32(strings.len() as u32);
        for (id, s) in strings.iter().enumerate() {
            string_table.i32(s.len() as i32);
            string_table.bytes(s.as_bytes());
            string_table.u32(id as u32);
        }

        let mut node = Writer::new();
        node.u32(1); // node name id -> "Item"
        node.u32(2); // attribute count
        node.u32(0); // child count
        node.u32(2); // attr name id -> "Name"
        node.u32(AttributeType::String as u32);
        let value = b"Sword";
        node.i32(value.len() as i32 + 1);
        node.bytes(value);
        node.u8(0);
        node.u32(3); // attr name id -> "Amount"
        node.u32(AttributeType::Int as u32);
        node.i32(5);

        let header_len = 40;
        let string_table_bytes = string_table.into_bytes();
        let region_table_len = 4 + (4 + 4); // count + one (nameId, offset) pair
        let node_offset = header_len + string_table_bytes.len() + region_table_len;

        let mut out = Writer::new();
        out.u32(SIGNATURE_FW3);
        out.u32(0); // total size, patched below
        out.u32(0); // big endian
        out.u32(0); // unknown
        out.u64(0); // timestamp
        out.u32(0); // major
        out.u32(0); // minor
        out.u32(0); // revision
        out.u32(0); // build
        assert_eq!(out.len(), header_len);
        out.bytes(&string_table_bytes);
        out.u32(1); // region count
        out.u32(0); // region name id -> "TestRegion"
        out.u32(node_offset as u32);
        out.bytes(&node.into_bytes());

        let mut bytes = out.into_bytes();
        let total_size = (bytes.len() as u32).to_le_bytes();
        bytes[4..8].copy_from_slice(&total_size);

        let resource = parse(&bytes).expect("parse should succeed");
        let region = resource.regions.get("TestRegion").expect("region present");
        assert_eq!(region.name, "Item");
        assert_eq!(region.attr("Name"), Some(&AttributeValue::Str("Sword".to_owned())));
        assert_eq!(region.attr("Amount"), Some(&AttributeValue::I32(5)));
    }
}
