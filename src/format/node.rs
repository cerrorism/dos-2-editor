//! The generic, format-agnostic in-memory model shared by every Larian
//! resource format (LSF/LSB/LSX/LSJ all serialize the same tree). This
//! mirrors lslib's `Resource`/`Region`/`Node`/`NodeAttribute` classes
//! (`LSLib/LS/Resource.cs`, `LSLib/LS/NodeAttribute.cs`).
use std::collections::HashMap;

use uuid::Uuid;

/// A parsed resource: a set of named top-level regions, each a root
/// `Node`. `globals.lsf` has (at least) `"Characters"` and `"Items"`
/// regions.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Resource {
    pub regions: HashMap<String, Node>,
}

/// One node in the tree. `children` is keyed by child tag name since
/// multiple siblings commonly share a tag (e.g. many `"Character"`
/// children under one `"Characters"` node) — order within each vec is
/// preserved and matters (it's positional/index-matched against sibling
/// lists like `Items`/`Creators`).
///
/// **Invariant**: a child's map key must always equal that child's own
/// `name` field (matching lslib's `Node.AppendChild`, which files a
/// child under its own name — there is no separate "tag" concept). The
/// writer (`format::lsf::serialize`) trusts this and serializes each
/// child using its own `name`, ignoring the map key entirely, so an
/// inconsistent key silently "loses" itself on a round trip rather than
/// erroring — always build children via a helper that enforces this
/// (e.g. `Node::push_child`) rather than inserting into `children`
/// directly with a hand-picked key.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Node {
    pub name: String,
    pub attributes: HashMap<String, NodeAttribute>,
    pub children: HashMap<String, Vec<Node>>,
}

impl Node {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), attributes: HashMap::new(), children: HashMap::new() }
    }

    pub fn child(&self, tag: &str) -> Option<&Node> {
        self.children.get(tag).and_then(|v| v.first())
    }

    pub fn child_mut(&mut self, tag: &str) -> Option<&mut Node> {
        self.children.get_mut(tag).and_then(|v| v.first_mut())
    }

    pub fn children_of(&self, tag: &str) -> &[Node] {
        self.children.get(tag).map(|v| v.as_slice()).unwrap_or(&[])
    }

    pub fn attr(&self, name: &str) -> Option<&AttributeValue> {
        self.attributes.get(name).map(|a| &a.value)
    }

    pub fn set_attr(&mut self, name: impl Into<String>, ty: AttributeType, value: AttributeValue) {
        self.attributes.insert(name.into(), NodeAttribute { ty, value });
    }

    /// Appends `child` under its own `name` — the only correct way to
    /// add a child (see the invariant note on `children` above).
    pub fn push_child(&mut self, child: Node) {
        self.children.entry(child.name.clone()).or_default().push(child);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct NodeAttribute {
    pub ty: AttributeType,
    pub value: AttributeValue,
}

/// Larian's attribute type-id enum (`LSLib/LS/NodeAttribute.cs`). The
/// full 0-33 range is implemented even though DOS2:DE-era data only
/// exercises up to `UUID`=31; costs nothing and avoids a second table if
/// a BG3-style save is ever fed through by mistake.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttributeType {
    None = 0,
    Byte = 1,
    Short = 2,
    UShort = 3,
    Int = 4,
    UInt = 5,
    Float = 6,
    Double = 7,
    IVec2 = 8,
    IVec3 = 9,
    IVec4 = 10,
    Vec2 = 11,
    Vec3 = 12,
    Vec4 = 13,
    Mat2 = 14,
    Mat3 = 15,
    Mat3x4 = 16,
    Mat4x3 = 17,
    Mat4 = 18,
    Bool = 19,
    String = 20,
    Path = 21,
    FixedString = 22,
    LSString = 23,
    ULongLong = 24,
    ScratchBuffer = 25,
    Long = 26,
    Int8 = 27,
    TranslatedString = 28,
    WString = 29,
    LSWString = 30,
    Uuid = 31,
    Int64 = 32,
    TranslatedFSString = 33,
}

impl AttributeType {
    pub fn from_u32(v: u32) -> Option<Self> {
        use AttributeType as T;
        Some(match v {
            0 => T::None,
            1 => T::Byte,
            2 => T::Short,
            3 => T::UShort,
            4 => T::Int,
            5 => T::UInt,
            6 => T::Float,
            7 => T::Double,
            8 => T::IVec2,
            9 => T::IVec3,
            10 => T::IVec4,
            11 => T::Vec2,
            12 => T::Vec3,
            13 => T::Vec4,
            14 => T::Mat2,
            15 => T::Mat3,
            16 => T::Mat3x4,
            17 => T::Mat4x3,
            18 => T::Mat4,
            19 => T::Bool,
            20 => T::String,
            21 => T::Path,
            22 => T::FixedString,
            23 => T::LSString,
            24 => T::ULongLong,
            25 => T::ScratchBuffer,
            26 => T::Long,
            27 => T::Int8,
            28 => T::TranslatedString,
            29 => T::WString,
            30 => T::LSWString,
            31 => T::Uuid,
            32 => T::Int64,
            33 => T::TranslatedFSString,
            _ => return None,
        })
    }

    /// Number of scalar components for a `*Vec*` type (2/3/4), else `None`.
    pub fn vec_len(self) -> Option<usize> {
        use AttributeType as T;
        match self {
            T::IVec2 | T::Vec2 => Some(2),
            T::IVec3 | T::Vec3 => Some(3),
            T::IVec4 | T::Vec4 => Some(4),
            _ => None,
        }
    }

    /// `(rows, cols)` for a `Mat*` type, else `None`. Matches lslib's
    /// `AttributeTypeExtensions.GetRows`/`GetColumns` exactly (note
    /// `Mat3x4` is rows=3,cols=4 and `Mat4x3` is rows=4,cols=3).
    pub fn mat_dims(self) -> Option<(usize, usize)> {
        use AttributeType as T;
        match self {
            T::Mat2 => Some((2, 2)),
            T::Mat3 => Some((3, 3)),
            T::Mat3x4 => Some((3, 4)),
            T::Mat4x3 => Some((4, 3)),
            T::Mat4 => Some((4, 4)),
            _ => None,
        }
    }

    pub fn is_string_like(self) -> bool {
        use AttributeType as T;
        matches!(self, T::String | T::Path | T::FixedString | T::LSString | T::WString | T::LSWString)
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TranslatedString {
    /// BG3-era version tag; always 0 for DOS2:DE (LSF version 3), where
    /// `value` is always present inline.
    pub version: u16,
    pub value: Option<String>,
    pub handle: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TranslatedFsString {
    pub string: TranslatedString,
    pub arguments: Vec<TranslatedFsStringArgument>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TranslatedFsStringArgument {
    pub key: String,
    pub string: TranslatedFsString,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AttributeValue {
    None,
    U8(u8),
    I16(i16),
    U16(u16),
    I32(i32),
    U32(u32),
    F32(f32),
    F64(f64),
    /// `IVec2`/`IVec3`/`IVec4` — length given by `AttributeType::vec_len`.
    IVec(Vec<i32>),
    /// `Vec2`/`Vec3`/`Vec4` — length given by `AttributeType::vec_len`.
    Vec(Vec<f32>),
    /// `Mat2`/`Mat3`/`Mat3x4`/`Mat4x3`/`Mat4`, column-major, `rows*cols`
    /// entries per `AttributeType::mat_dims`.
    Mat(Vec<f32>),
    Bool(bool),
    /// Backs `String`/`Path`/`FixedString`/`LSString`/`WString`/`LSWString`
    /// — the wire encoding differs only in the container format's framing
    /// (all are plain UTF-8 in LSF), so one variant covers all six; the
    /// sibling `ty: AttributeType` field on `NodeAttribute` is what makes
    /// round-tripping to the exact original type id correct.
    Str(String),
    U64(u64),
    ScratchBuffer(Vec<u8>),
    /// Backs both `Long` (26, unused/legacy) and `Int64` (32) — again
    /// disambiguated by `NodeAttribute::ty` on write.
    I64(i64),
    I8(i8),
    TranslatedString(TranslatedString),
    Uuid(Uuid),
    TranslatedFsString(TranslatedFsString),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attribute_type_round_trips_through_u32() {
        for v in 0..=33u32 {
            let ty = AttributeType::from_u32(v).unwrap();
            assert_eq!(ty as u32, v);
        }
        assert!(AttributeType::from_u32(34).is_none());
    }
}
