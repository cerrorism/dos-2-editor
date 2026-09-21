//! Little-endian byte cursor helpers used by every binary format reader
//! in `format/`.
//!
//! GUID note: LSF's `UUID` attribute type round-trips through .NET's
//! `Guid(byte[])` constructor (see `lslib`'s `BinUtils.cs`), which uses
//! the "Microsoft mixed-endian" GUID layout — the first 4 bytes (Data1)
//! and next two 2-byte fields (Data2/Data3) are little-endian, while the
//! last 8 bytes (Data4) are literal/big-endian — not RFC-4122 byte
//! order. The `uuid` crate has exactly this convention built in as
//! `Uuid::from_bytes_le`/`to_bytes_le`, so reading/writing a UUID
//! attribute with those functions both round-trips correctly *and*
//! prints the same string a human would see in Larian's own tools.
use uuid::Uuid;

pub struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    pub fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }

    pub fn seek(&mut self, pos: usize) {
        self.pos = pos;
    }

    pub fn bytes(&mut self, n: usize) -> &'a [u8] {
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        s
    }

    pub fn u8(&mut self) -> u8 {
        self.bytes(1)[0]
    }
    pub fn u16(&mut self) -> u16 {
        u16::from_le_bytes(self.bytes(2).try_into().unwrap())
    }
    pub fn u32(&mut self) -> u32 {
        u32::from_le_bytes(self.bytes(4).try_into().unwrap())
    }
    pub fn u64(&mut self) -> u64 {
        u64::from_le_bytes(self.bytes(8).try_into().unwrap())
    }
    pub fn i8(&mut self) -> i8 {
        self.u8() as i8
    }
    pub fn i16(&mut self) -> i16 {
        self.u16() as i16
    }
    pub fn i32(&mut self) -> i32 {
        self.u32() as i32
    }
    pub fn i64(&mut self) -> i64 {
        self.u64() as i64
    }
    pub fn f32(&mut self) -> f32 {
        f32::from_bits(self.u32())
    }
    pub fn f64(&mut self) -> f64 {
        f64::from_bits(self.u64())
    }

    pub fn guid(&mut self) -> Uuid {
        let raw: [u8; 16] = self.bytes(16).try_into().unwrap();
        Uuid::from_bytes_le(raw)
    }

    /// Reads a fixed-size buffer and trims at the first NUL byte,
    /// decoding the rest as UTF-8 (used for PAK file-entry names, which
    /// are null-terminated within a fixed-size field).
    pub fn fixed_str(&mut self, len: usize) -> String {
        let raw = self.bytes(len);
        let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
        String::from_utf8_lossy(&raw[..end]).into_owned()
    }

    /// Reads a `u32`-length-prefixed UTF-8 string (LSF's common string
    /// encoding for `String`/`Path`/`FixedString`/`LSString` attribute
    /// values).
    pub fn len_prefixed_str(&mut self) -> String {
        let len = self.u32() as usize;
        String::from_utf8_lossy(self.bytes(len)).into_owned()
    }
}

pub struct Writer {
    buf: Vec<u8>,
}

impl Default for Writer {
    fn default() -> Self {
        Self::new()
    }
}

impl Writer {
    pub fn new() -> Self {
        Self { buf: Vec::new() }
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.buf
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    pub fn bytes(&mut self, b: &[u8]) {
        self.buf.extend_from_slice(b);
    }
    pub fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }
    pub fn u16(&mut self, v: u16) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    pub fn u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    pub fn u64(&mut self, v: u64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    pub fn i8(&mut self, v: i8) {
        self.u8(v as u8);
    }
    pub fn i16(&mut self, v: i16) {
        self.u16(v as u16);
    }
    pub fn i32(&mut self, v: i32) {
        self.u32(v as u32);
    }
    pub fn i64(&mut self, v: i64) {
        self.u64(v as u64);
    }
    pub fn f32(&mut self, v: f32) {
        self.u32(v.to_bits());
    }
    pub fn f64(&mut self, v: f64) {
        self.u64(v.to_bits());
    }

    pub fn guid(&mut self, v: Uuid) {
        self.bytes(&v.to_bytes_le());
    }

    /// Writes `s` truncated/NUL-padded to exactly `len` bytes (mirrors
    /// `Reader::fixed_str`).
    pub fn fixed_str(&mut self, s: &str, len: usize) {
        let mut raw = vec![0u8; len];
        let bytes = s.as_bytes();
        let n = bytes.len().min(len.saturating_sub(1));
        raw[..n].copy_from_slice(&bytes[..n]);
        self.bytes(&raw);
    }

    pub fn len_prefixed_str(&mut self, s: &str) {
        let bytes = s.as_bytes();
        self.u32(bytes.len() as u32);
        self.bytes(bytes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitive_round_trip() {
        let mut w = Writer::new();
        w.u32(0xDEADBEEF);
        w.i16(-1234);
        w.f32(3.5);
        w.fixed_str("hello", 8);
        w.len_prefixed_str("world");
        let guid = Uuid::new_v4();
        w.guid(guid);

        let bytes = w.into_bytes();
        let mut r = Reader::new(&bytes);
        assert_eq!(r.u32(), 0xDEADBEEF);
        assert_eq!(r.i16(), -1234);
        assert_eq!(r.f32(), 3.5);
        assert_eq!(r.fixed_str(8), "hello");
        assert_eq!(r.len_prefixed_str(), "world");
        assert_eq!(r.guid(), guid);
    }
}
