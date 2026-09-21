//! LSF binary resource format — DOS2:DE only (`LSFVersion` 3,
//! "VerExtendedNodes"). This is the format `globals.lsf`/`meta.lsf`
//! (inside a `.lsv` save) are serialized in. Byte layout verified
//! directly against lslib's `LSFReader.cs`/`LSFWriter.cs`/
//! `LSFCommon.cs`/`BinUtils.cs` (MIT licensed, Norbyte) — not ported
//! wholesale, just the on-disk shape.
//!
//! Read supports both node/attribute table shapes a version-3 file can
//! use (`MetadataFormat::None`, the plain/short shape, and
//! `KeysAndAdjacency`, the extended shape with explicit sibling/
//! next-attribute links) since we don't control what a real save was
//! written with. Write always emits the plain shape: both are equally
//! valid per the version-3 spec (the game's own LSF reader must handle
//! both, since lslib itself round-trips through either), and the plain
//! shape needs far less bookkeeping — no sibling-index precomputation,
//! no explicit per-attribute offsets/next-pointers to track. Flagged in
//! the project plan as something to confirm against a real save
//! actually still loading after being rewritten this way.
//!
//! The string table's bucket assignment is Larian's own tooling detail
//! (driven by .NET's randomized-per-process `string.GetHashCode()` on
//! write — not even reproducible run-to-run by lslib itself) and has no
//! bearing on file validity: a reader just resolves `(bucket, offset)`
//! pairs the file itself records. Our writer uses one string per bucket
//! (a degenerate but entirely spec-valid "hash table") rather than
//! reimplementing Larian's bucketing scheme.
use std::collections::HashMap;

use super::compression::{compress_with, decompress_with, CompressionMethod};
use super::node::{
    AttributeType, AttributeValue, Node, NodeAttribute, Resource, TranslatedFsString, TranslatedFsStringArgument, TranslatedString,
};
use super::primitives::{Reader, Writer};

const MAGIC: &[u8; 4] = b"LSOF";
const VERSION: u32 = 3; // VerExtendedNodes (DOS2:DE)
const CHUNKED: bool = true; // version 3 >= VerChunkedCompress(2)

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MetadataFormat {
    None,
    KeysAndAdjacency,
}

impl MetadataFormat {
    fn from_u32(v: u32) -> Self {
        // `None2 = 2` behaves the same as `None` per lslib.
        if v == 1 {
            MetadataFormat::KeysAndAdjacency
        } else {
            MetadataFormat::None
        }
    }

    fn to_u32(self) -> u32 {
        match self {
            MetadataFormat::None => 0,
            MetadataFormat::KeysAndAdjacency => 1,
        }
    }
}

struct SectionSizes {
    uncompressed: u32,
    size_on_disk: u32,
}

struct Metadata {
    strings: SectionSizes,
    nodes: SectionSizes,
    attributes: SectionSizes,
    values: SectionSizes,
    compression: CompressionMethod,
    metadata_format: MetadataFormat,
}

fn decompress_section(r: &mut Reader, sizes: &SectionSizes, method: CompressionMethod, chunked: bool) -> Result<Vec<u8>, String> {
    if sizes.size_on_disk == 0 && sizes.uncompressed != 0 {
        // Stored raw (no compression header framing).
        return Ok(r.bytes(sizes.uncompressed as usize).to_vec());
    }
    if sizes.size_on_disk == 0 && sizes.uncompressed == 0 {
        return Ok(Vec::new());
    }
    let compressed_size = if method == CompressionMethod::None { sizes.uncompressed } else { sizes.size_on_disk };
    let raw = r.bytes(compressed_size as usize);
    decompress_with(raw, method, sizes.uncompressed as usize, chunked)
}

pub fn parse(data: &[u8]) -> Result<Resource, String> {
    let mut r = Reader::new(data);
    if r.remaining() < 8 || r.bytes(4) != MAGIC {
        return Err("not an LSF resource (bad magic)".into());
    }
    let version = r.u32();
    if version != VERSION {
        return Err(format!("unsupported LSF version {version} — only version 3 (DOS2:DE) is supported"));
    }

    let _engine_version = r.i32(); // LSFHeader (version < VerBG3ExtendedHeader)

    // LSFMetadataV5 (version < VerBG3NodeKeys): 8x u32 section sizes,
    // then a CompressionFlags byte, 3 reserved bytes, then a u32
    // MetadataFormat. No separate Keys section size fields exist at
    // this header version — see module docs on why that's fine.
    let strings = SectionSizes { uncompressed: r.u32(), size_on_disk: r.u32() };
    let nodes = SectionSizes { uncompressed: r.u32(), size_on_disk: r.u32() };
    let attributes = SectionSizes { uncompressed: r.u32(), size_on_disk: r.u32() };
    let values = SectionSizes { uncompressed: r.u32(), size_on_disk: r.u32() };
    let compression_flags = r.u8();
    let _unknown2 = r.u8();
    let _unknown3 = r.u16();
    let metadata_format = MetadataFormat::from_u32(r.u32());
    let compression = CompressionMethod::from_low_nibble(compression_flags as u32)?;

    let meta = Metadata { strings, nodes, attributes, values, compression, metadata_format };

    let names_bytes = decompress_section(&mut r, &meta.strings, meta.compression, false)?;
    let names = read_names(&names_bytes)?;

    let nodes_bytes = decompress_section(&mut r, &meta.nodes, meta.compression, CHUNKED)?;
    let has_adjacency = meta.metadata_format == MetadataFormat::KeysAndAdjacency;
    let node_defs = read_nodes(&nodes_bytes, has_adjacency)?;

    let attributes_bytes = decompress_section(&mut r, &meta.attributes, meta.compression, CHUNKED)?;
    let attr_defs = if has_adjacency { read_attributes_v3(&attributes_bytes)? } else { read_attributes_v2(&attributes_bytes)? };

    let values_bytes = decompress_section(&mut r, &meta.values, meta.compression, CHUNKED)?;

    // Keys section: only physically present (per lslib's own behavior)
    // for LSFMetadataV6+ headers; a V5 header (our only supported
    // version) has no size fields for it, so it's always absent here.

    build_resource(&names, &node_defs, &attr_defs, &values_bytes)
}

fn read_names(data: &[u8]) -> Result<Vec<Vec<String>>, String> {
    let mut r = Reader::new(data);
    let num_buckets = r.u32() as usize;
    let mut buckets = Vec::with_capacity(num_buckets);
    for _ in 0..num_buckets {
        let chain_len = r.u16() as usize;
        let mut chain = Vec::with_capacity(chain_len);
        for _ in 0..chain_len {
            let len = r.u16() as usize;
            chain.push(String::from_utf8_lossy(r.bytes(len)).into_owned());
        }
        buckets.push(chain);
    }
    Ok(buckets)
}

struct NodeDef {
    name_hash: u32,
    parent_index: i32,
    first_attribute_index: i32,
}

fn read_nodes(data: &[u8], long_nodes: bool) -> Result<Vec<NodeDef>, String> {
    let mut r = Reader::new(data);
    let mut out = Vec::new();
    if long_nodes {
        // LSFNodeEntryV3: NameHashTableIndex, ParentIndex, NextSiblingIndex, FirstAttributeIndex
        while r.remaining() >= 16 {
            let name_hash = r.u32();
            let parent_index = r.i32();
            let _next_sibling_index = r.i32();
            let first_attribute_index = r.i32();
            out.push(NodeDef { name_hash, parent_index, first_attribute_index });
        }
    } else {
        // LSFNodeEntryV2: NameHashTableIndex, FirstAttributeIndex, ParentIndex
        while r.remaining() >= 12 {
            let name_hash = r.u32();
            let first_attribute_index = r.i32();
            let parent_index = r.i32();
            out.push(NodeDef { name_hash, parent_index, first_attribute_index });
        }
    }
    Ok(out)
}

struct AttrDef {
    name_hash: u32,
    type_id: u32,
    length: u32,
    data_offset: u32,
    next_attribute_index: i32,
}

fn read_attributes_v2(data: &[u8]) -> Result<Vec<AttrDef>, String> {
    let mut r = Reader::new(data);
    let mut out: Vec<AttrDef> = Vec::new();
    // Reconstructs the per-node attribute linked list the same way
    // lslib's `ReadAttributesV2` does: track the last-seen attribute
    // index per node (offset by 1 so node index -1 has a slot) and link
    // `NextAttributeIndex` retroactively when a later attribute for the
    // same node appears.
    let mut prev_attribute_refs: Vec<i32> = Vec::new();
    let mut data_offset: u32 = 0;
    let mut index: i32 = 0;
    while r.remaining() >= 12 {
        let name_hash = r.u32();
        let type_and_length = r.u32();
        let node_index = r.i32();

        let type_id = type_and_length & 0x3F;
        let length = type_and_length >> 6;

        out.push(AttrDef { name_hash, type_id, length, data_offset, next_attribute_index: -1 });

        let ni = (node_index + 1) as usize;
        if prev_attribute_refs.len() > ni {
            if prev_attribute_refs[ni] != -1 {
                out[prev_attribute_refs[ni] as usize].next_attribute_index = index;
            }
            prev_attribute_refs[ni] = index;
        } else {
            prev_attribute_refs.resize(ni, -1);
            prev_attribute_refs.push(index);
        }

        data_offset += length;
        index += 1;
    }
    Ok(out)
}

fn read_attributes_v3(data: &[u8]) -> Result<Vec<AttrDef>, String> {
    let mut r = Reader::new(data);
    let mut out = Vec::new();
    // LSFAttributeEntryV3: NameHashTableIndex, TypeAndLength, NextAttributeIndex, Offset
    while r.remaining() >= 16 {
        let name_hash = r.u32();
        let type_and_length = r.u32();
        let next_attribute_index = r.i32();
        let offset = r.u32();
        let type_id = type_and_length & 0x3F;
        let length = type_and_length >> 6;
        out.push(AttrDef { name_hash, type_id, length, data_offset: offset, next_attribute_index });
    }
    Ok(out)
}

fn resolve_name(names: &[Vec<String>], hash: u32) -> Result<&str, String> {
    let bucket = (hash >> 16) as usize;
    let offset = (hash & 0xFFFF) as usize;
    names
        .get(bucket)
        .and_then(|chain| chain.get(offset))
        .map(|s| s.as_str())
        .ok_or_else(|| format!("dangling name reference (bucket {bucket}, offset {offset})"))
}

fn build_resource(names: &[Vec<String>], node_defs: &[NodeDef], attr_defs: &[AttrDef], values: &[u8]) -> Result<Resource, String> {
    let mut nodes: Vec<Node> = Vec::with_capacity(node_defs.len());
    // Index in `nodes` a fully-built node ended up at is the same as its
    // index in `node_defs`/`nodes` here (we build in file order, one
    // pass), matching how lslib assigns node indices while reading.
    for def in node_defs {
        let name = resolve_name(names, def.name_hash)?.to_owned();
        let mut node = Node::new(name);

        if def.first_attribute_index != -1 {
            let mut attr_idx = def.first_attribute_index;
            loop {
                let attr = attr_defs
                    .get(attr_idx as usize)
                    .ok_or_else(|| format!("dangling attribute index {attr_idx}"))?;
                let attr_name = resolve_name(names, attr.name_hash)?.to_owned();
                let ty = AttributeType::from_u32(attr.type_id).ok_or_else(|| format!("unknown attribute type id {}", attr.type_id))?;
                let value = read_attribute_value(ty, values, attr.data_offset, attr.length)?;
                node.attributes.insert(attr_name, super::node::NodeAttribute { ty, value });

                if attr.next_attribute_index == -1 {
                    break;
                }
                attr_idx = attr.next_attribute_index;
            }
        }

        nodes.push(node);
    }

    // Second pass: attach each node to its parent, walking indices from
    // highest to lowest. A node's parent always has a lower index than
    // the node itself (nodes are written in pre-order: a node's struct
    // is emitted, then its children's, recursively — see `LSFWriter`),
    // so by the time we reach index `i` here, `parent_index` (< i) is
    // still sitting untouched in `built`, since moving a node out only
    // happens when the loop reaches *that node's own* index, which for
    // a lower index happens *later* in this descending walk.
    //
    // This visits children in descending index order, i.e. the reverse
    // of their true on-disk sibling order, so `reverse_children_order`
    // below fixes that back up in one pass at the end (cheaper than
    // `Vec::insert(0, ..)`-ing each child, which would be O(n^2) for a
    // save's larger sibling lists like `Items`).
    let mut built: Vec<Option<Node>> = nodes.into_iter().map(Some).collect();
    for i in (0..node_defs.len()).rev() {
        let parent_index = node_defs[i].parent_index;
        if parent_index == -1 {
            continue; // region; attached to `resource` below instead
        }
        let child = built[i].take().ok_or("node visited twice")?;
        let parent = built[parent_index as usize]
            .as_mut()
            .ok_or("parent node already moved (index/parent-order assumption violated)")?;
        parent.children.entry(child.name.clone()).or_default().push(child);
    }

    let mut resource = Resource::default();
    for i in 0..node_defs.len() {
        if node_defs[i].parent_index == -1 {
            let mut node = built[i].take().ok_or("region visited twice")?;
            reverse_children_order(&mut node);
            resource.regions.insert(node.name.clone(), node);
        }
    }

    Ok(resource)
}

/// Undoes the sibling-order reversal from processing `build_resource`'s
/// attachment pass in descending index order.
fn reverse_children_order(node: &mut Node) {
    for children in node.children.values_mut() {
        children.reverse();
        for child in children.iter_mut() {
            reverse_children_order(child);
        }
    }
}

fn read_attribute_value(ty: AttributeType, values: &[u8], offset: u32, length: u32) -> Result<AttributeValue, String> {
    let slice = values
        .get(offset as usize..(offset + length) as usize)
        .ok_or_else(|| format!("attribute value out of bounds (offset {offset}, length {length})"))?;
    let mut r = Reader::new(slice);

    Ok(match ty {
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
            // Column-major on disk (matches lslib's `BinUtils.ReadAttribute`).
            let mut mat = vec![0f32; rows * cols];
            for col in 0..cols {
                for row in 0..rows {
                    mat[row * cols + col] = r.f32();
                }
            }
            AttributeValue::Mat(mat)
        }
        AttributeType::Bool => AttributeValue::Bool(r.u8() != 0),
        AttributeType::String | AttributeType::Path | AttributeType::FixedString | AttributeType::LSString | AttributeType::WString | AttributeType::LSWString => {
            AttributeValue::Str(read_lsf_string(&mut r, length as usize))
        }
        AttributeType::ULongLong => AttributeValue::U64(r.u64()),
        AttributeType::ScratchBuffer => AttributeValue::ScratchBuffer(r.bytes(length as usize).to_vec()),
        AttributeType::Long | AttributeType::Int64 => AttributeValue::I64(r.i64()),
        AttributeType::Int8 => AttributeValue::I8(r.i8()),
        AttributeType::TranslatedString => {
            // DOS2:DE (version 3) is always below `VerBG3`, so the
            // inline-value form is always used (version tag is 0).
            let value_length = r.i32() as usize;
            let value = read_lsf_string(&mut r, value_length);
            let handle_length = r.i32() as usize;
            let handle = read_lsf_string(&mut r, handle_length);
            AttributeValue::TranslatedString(TranslatedString { version: 0, value: Some(value), handle })
        }
        AttributeType::Uuid => AttributeValue::Uuid(r.guid()),
        AttributeType::TranslatedFSString => AttributeValue::TranslatedFsString(read_translated_fs_string(&mut r)?),
    })
}

/// LSF's length-prefixed string convention: the attribute/field's
/// recorded `length` includes a trailing NUL byte, so the actual text is
/// `length - 1` bytes.
fn read_lsf_string(r: &mut Reader, length: usize) -> String {
    if length == 0 {
        return String::new();
    }
    let bytes = r.bytes(length - 1);
    let _null_terminator = r.u8();
    String::from_utf8_lossy(bytes).into_owned()
}

fn read_translated_fs_string(r: &mut Reader) -> Result<TranslatedFsString, String> {
    let value_length = r.i32() as usize;
    let value = read_lsf_string(r, value_length);
    let handle_length = r.i32() as usize;
    let handle = read_lsf_string(r, handle_length);

    let num_args = r.i32();
    let mut arguments = Vec::with_capacity(num_args.max(0) as usize);
    for _ in 0..num_args {
        let key_length = r.i32() as usize;
        let key = read_lsf_string(r, key_length);
        let string = read_translated_fs_string(r)?;
        let value_length = r.i32() as usize;
        let value = read_lsf_string(r, value_length);
        arguments.push(TranslatedFsStringArgument { key, string, value });
    }

    Ok(TranslatedFsString { string: TranslatedString { version: 0, value: Some(value), handle }, arguments })
}

// ---------------------------------------------------------------------
// Writer
// ---------------------------------------------------------------------

/// Serializes a `Resource` back to LSF version-3 bytes, always using the
/// plain (`MetadataFormat::None`) node/attribute table shape (see module
/// docs for why). Region/attribute/child iteration order is sorted by
/// name for deterministic, diffable output — `HashMap`'s own iteration
/// order is unspecified and would otherwise make two serializations of
/// the same `Resource` byte-different for no reason.
pub fn serialize(resource: &Resource, compression: CompressionMethod) -> Vec<u8> {
    let mut strings = StringTable::new();
    let mut node_writer = Writer::new();
    let mut attr_writer = Writer::new();
    let mut value_writer = Writer::new();
    let mut next_node_index: i32 = 0;
    let mut next_attribute_index: i32 = 0;

    let mut region_names: Vec<&String> = resource.regions.keys().collect();
    region_names.sort();
    for name in region_names {
        let node = &resource.regions[name];
        write_node(node, -1, &mut next_node_index, &mut next_attribute_index, &mut strings, &mut node_writer, &mut attr_writer, &mut value_writer);
    }

    let node_bytes = node_writer.into_bytes();
    let attr_bytes = attr_writer.into_bytes();
    let value_bytes = value_writer.into_bytes();
    let string_bytes = strings.write();

    let strings_compressed = compress_with(&string_bytes, compression, false);
    let nodes_compressed = compress_with(&node_bytes, compression, CHUNKED);
    let attrs_compressed = compress_with(&attr_bytes, compression, CHUNKED);
    let values_compressed = compress_with(&value_bytes, compression, CHUNKED);

    let (strings_on_disk, nodes_on_disk, attrs_on_disk, values_on_disk) = if compression == CompressionMethod::None {
        (0, 0, 0, 0)
    } else {
        (strings_compressed.len() as u32, nodes_compressed.len() as u32, attrs_compressed.len() as u32, values_compressed.len() as u32)
    };

    let mut out = Writer::new();
    out.bytes(MAGIC);
    out.u32(VERSION);
    out.i32(0); // engine version: cosmetic, not used by the game beyond display

    out.u32(string_bytes.len() as u32);
    out.u32(strings_on_disk);
    out.u32(node_bytes.len() as u32);
    out.u32(nodes_on_disk);
    out.u32(attr_bytes.len() as u32);
    out.u32(attrs_on_disk);
    out.u32(value_bytes.len() as u32);
    out.u32(values_on_disk);
    out.u8(compression.low_nibble() as u8); // level bits left 0 — decompression never reads them
    out.u8(0); // unknown2
    out.u16(0); // unknown3
    out.u32(MetadataFormat::None.to_u32());

    out.bytes(&strings_compressed);
    out.bytes(&nodes_compressed);
    out.bytes(&attrs_compressed);
    out.bytes(&values_compressed);
    // No Keys section: MetadataFormat::None never has one.

    out.into_bytes()
}

struct StringTable {
    index_of: HashMap<String, u32>,
    strings: Vec<String>,
}

impl StringTable {
    fn new() -> Self {
        Self { index_of: HashMap::new(), strings: Vec::new() }
    }

    /// Returns a `NameHashTableIndex` for `s`, adding it to the table if
    /// new. One string per bucket (chain length always 1) — see module
    /// docs for why that's a valid, simpler alternative to Larian's own
    /// (non-reproducible) bucketing.
    fn add(&mut self, s: &str) -> u32 {
        if let Some(&idx) = self.index_of.get(s) {
            return idx << 16;
        }
        let idx = self.strings.len() as u32;
        self.strings.push(s.to_owned());
        self.index_of.insert(s.to_owned(), idx);
        idx << 16
    }

    fn write(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.u32(self.strings.len() as u32); // num hash buckets == num strings
        for s in &self.strings {
            w.u16(1); // chain length
            let bytes = s.as_bytes();
            w.u16(bytes.len() as u16);
            w.bytes(bytes);
        }
        w.into_bytes()
    }
}

#[allow(clippy::too_many_arguments)]
fn write_node(
    node: &Node,
    parent_index: i32,
    next_node_index: &mut i32,
    next_attribute_index: &mut i32,
    strings: &mut StringTable,
    node_writer: &mut Writer,
    attr_writer: &mut Writer,
    value_writer: &mut Writer,
) {
    let my_index = *next_node_index;
    let name_hash = strings.add(&node.name);

    let first_attribute_index = if node.attributes.is_empty() {
        -1
    } else {
        let first = *next_attribute_index;
        write_attributes(node, my_index, next_attribute_index, strings, attr_writer, value_writer);
        first
    };

    // LSFNodeEntryV2: NameHashTableIndex, FirstAttributeIndex, ParentIndex
    node_writer.u32(name_hash);
    node_writer.i32(first_attribute_index);
    node_writer.i32(parent_index);

    *next_node_index += 1;

    let mut child_tags: Vec<&String> = node.children.keys().collect();
    child_tags.sort();
    for tag in child_tags {
        for child in &node.children[tag] {
            write_node(child, my_index, next_node_index, next_attribute_index, strings, node_writer, attr_writer, value_writer);
        }
    }
}

fn write_attributes(
    node: &Node,
    node_index: i32,
    next_attribute_index: &mut i32,
    strings: &mut StringTable,
    attr_writer: &mut Writer,
    value_writer: &mut Writer,
) {
    let mut attr_names: Vec<&String> = node.attributes.keys().collect();
    attr_names.sort();
    for name in attr_names {
        let attr = &node.attributes[name];
        let start = value_writer.len();
        write_attribute_value(value_writer, attr);
        let length = (value_writer.len() - start) as u32;

        let name_hash = strings.add(name);
        let type_and_length = (attr.ty as u32) | (length << 6);

        // LSFAttributeEntryV2: NameHashTableIndex, TypeAndLength, NodeIndex
        attr_writer.u32(name_hash);
        attr_writer.u32(type_and_length);
        attr_writer.i32(node_index);

        *next_attribute_index += 1;
    }
}

fn write_attribute_value(w: &mut Writer, attr: &NodeAttribute) {
    match &attr.value {
        AttributeValue::None => {}
        AttributeValue::U8(v) => w.u8(*v),
        AttributeValue::I16(v) => w.i16(*v),
        AttributeValue::U16(v) => w.u16(*v),
        AttributeValue::I32(v) => w.i32(*v),
        AttributeValue::U32(v) => w.u32(*v),
        AttributeValue::F32(v) => w.f32(*v),
        AttributeValue::F64(v) => w.f64(*v),
        AttributeValue::IVec(v) => {
            for x in v {
                w.i32(*x);
            }
        }
        AttributeValue::Vec(v) => {
            for x in v {
                w.f32(*x);
            }
        }
        AttributeValue::Mat(v) => {
            let (rows, cols) = attr.ty.mat_dims().expect("Mat* attribute type must carry Mat dims");
            for col in 0..cols {
                for row in 0..rows {
                    w.f32(v[row * cols + col]);
                }
            }
        }
        AttributeValue::Bool(b) => w.u8(u8::from(*b)),
        AttributeValue::Str(s) => write_lsf_string(w, s),
        AttributeValue::U64(v) => w.u64(*v),
        AttributeValue::ScratchBuffer(b) => w.bytes(b),
        AttributeValue::I64(v) => w.i64(*v),
        AttributeValue::I8(v) => w.i8(*v),
        AttributeValue::TranslatedString(ts) => {
            write_lsf_string_with_length(w, ts.value.as_deref().unwrap_or(""));
            write_lsf_string_with_length(w, &ts.handle);
        }
        AttributeValue::Uuid(u) => w.guid(*u),
        AttributeValue::TranslatedFsString(fs) => write_translated_fs_string(w, fs),
    }
}

fn write_lsf_string(w: &mut Writer, s: &str) {
    w.bytes(s.as_bytes());
    w.u8(0);
}

fn write_lsf_string_with_length(w: &mut Writer, s: &str) {
    let bytes = s.as_bytes();
    w.i32((bytes.len() + 1) as i32);
    w.bytes(bytes);
    w.u8(0);
}

fn write_translated_fs_string(w: &mut Writer, fs: &TranslatedFsString) {
    write_lsf_string_with_length(w, fs.string.value.as_deref().unwrap_or(""));
    write_lsf_string_with_length(w, &fs.string.handle);
    w.i32(fs.arguments.len() as i32);
    for arg in &fs.arguments {
        write_lsf_string_with_length(w, &arg.key);
        write_translated_fs_string(w, &arg.string);
        write_lsf_string_with_length(w, &arg.value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::node::{AttributeType as T, AttributeValue as V};
    use uuid::Uuid;

    fn sample_resource() -> Resource {
        let mut resource = Resource::default();

        let mut stats = Node::new("Stats");
        stats.set_attr("Experience", T::Int64, V::I64(12345));
        stats.set_attr("IsPlayer", T::Bool, V::Bool(true));

        let mut rune_slot = Node::new("RuneSlot");
        rune_slot.set_attr("RuneStatsID", T::FixedString, V::Str("LOOT_Rune_Flame_A".into()));

        let mut item = Node::new("Item");
        item.set_attr("Stats", T::FixedString, V::Str("WPN_Sword_A".into()));
        item.set_attr("Amount", T::Int, V::I32(1));
        item.set_attr("OriginalOwnerCharacter", T::Uuid, V::Uuid(Uuid::new_v4()));
        item.set_attr(
            "CustomDisplayName",
            T::TranslatedString,
            V::TranslatedString(TranslatedString { version: 0, value: Some("A Fine Sword".into()), handle: "h123".into() }),
        );
        item.push_child(rune_slot);
        item.push_child({
            let mut second = Node::new("RuneSlot");
            second.set_attr("RuneStatsID", T::FixedString, V::Str(String::new()));
            second
        });

        let mut items_factory = Node::new("ItemFactory");
        items_factory.push_child(item);

        let mut items_region = Node::new("Items");
        items_region.push_child(items_factory);
        items_region.push_child(stats);

        resource.regions.insert("Items".into(), items_region);
        resource
    }

    #[test]
    fn round_trip_none_compression() {
        let resource = sample_resource();
        let bytes = serialize(&resource, CompressionMethod::None);
        let reparsed = parse(&bytes).expect("parse should succeed");
        assert_eq!(reparsed, resource);
    }

    #[test]
    fn round_trip_lz4() {
        let resource = sample_resource();
        let bytes = serialize(&resource, CompressionMethod::Lz4);
        let reparsed = parse(&bytes).expect("parse should succeed");
        assert_eq!(reparsed, resource);
    }

    #[test]
    fn round_trip_zlib() {
        let resource = sample_resource();
        let bytes = serialize(&resource, CompressionMethod::Zlib);
        let reparsed = parse(&bytes).expect("parse should succeed");
        assert_eq!(reparsed, resource);
    }

    #[test]
    fn sibling_order_is_preserved() {
        let resource = sample_resource();
        let bytes = serialize(&resource, CompressionMethod::Lz4);
        let reparsed = parse(&bytes).unwrap();

        let item = &reparsed.regions["Items"].children["ItemFactory"][0].children["Item"][0];
        let rune_slots = &item.children["RuneSlot"];
        assert_eq!(rune_slots.len(), 2);
        assert_eq!(rune_slots[0].attr("RuneStatsID"), Some(&V::Str("LOOT_Rune_Flame_A".into())));
        assert_eq!(rune_slots[1].attr("RuneStatsID"), Some(&V::Str(String::new())));
    }
}
