//! PAK/LSPK container format — DOS2:DE only (`PackageVersion` 13).
//! Savegames (`.lsv`) and mod packages (`.pak`) share this container.
//! Byte layout verified directly against lslib's `PackageReader.cs`/
//! `PackageWriter.cs`/`PackageFormat.cs` (MIT licensed, Norbyte) — not
//! ported wholesale, just the on-disk shape.
//!
//! Two corrections vs. the general (multi-version) lslib code, specific
//! to version 13 exactly:
//! - The compressed file list has **no `compressedSize` field** (that
//!   only exists for `Version > 13`); the LZ4 payload length is simply
//!   `file_list_size - 4`.
//! - `OffsetInFile` is already absolute in the archive (the `DataOffset`
//!   adjustment in lslib only applies to older uncompressed-file-list
//!   versions).
use std::fs;
use std::path::Path;

use super::compression::{compress_with, crc32, decompress_with, lz4_block_compress, lz4_block_decompress, lz4_frame_decompress, CompressionMethod};
use super::primitives::{Reader, Writer};

const SIGNATURE: u32 = 0x4B50_534C; // "LSPK" little-endian
const HEADER_SIZE_V13: u32 = 32;
const FOOTER_SIZE: usize = HEADER_SIZE_V13 as usize + 8; // header + headerSize(4) + signature(4)
const FILE_NAME_LEN: usize = 256;
const FILE_ENTRY_SIZE: usize = FILE_NAME_LEN + 4 * 6; // 280 bytes
const PADDING: usize = 0x40;
const PADDING_BYTE: u8 = 0xAD;
const FLAG_SOLID: u8 = 0x04;

// PAK per-file/file-list compression is never chunked (raw LZ4 block,
// plain zlib stream) — only LSF's Nodes/Attributes/Values sections use
// the chunked/frame variant.
const CHUNKED: bool = false;

#[derive(Debug, Clone)]
pub struct PakEntry {
    pub name: String,
    pub archive_part: u32,
    /// Raw flags byte(s) from the file entry: low nibble = compression
    /// method, high nibble = compression level (level is write-only
    /// metadata we don't need to interpret to read a file).
    pub flags: u32,
    pub crc: u32,
    pub uncompressed_size: u32,
    /// The on-disk (possibly compressed) bytes, per `flags`'s method.
    pub compressed: Vec<u8>,
}

impl PakEntry {
    fn method(&self) -> Result<CompressionMethod, String> {
        CompressionMethod::from_low_nibble(self.flags)
    }

    pub fn read(&self) -> Result<Vec<u8>, String> {
        decompress_with(&self.compressed, self.method()?, self.uncompressed_size as usize, CHUNKED)
    }
}

#[derive(Debug, Clone, Default)]
pub struct Pak {
    pub entries: Vec<PakEntry>,
}

type RawEntry = (String, usize, usize, u32, u32, u32, u32); // name, offset, size_on_disk, uncompressed_size, archive_part, flags, crc

impl Pak {
    pub fn open(path: &Path) -> Result<Self, String> {
        let data = fs::read(path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
        Self::parse(&data)
    }

    pub fn parse(data: &[u8]) -> Result<Self, String> {
        if data.len() < FOOTER_SIZE {
            return Err("file too small to be an LSPK package".into());
        }
        let capacity = data.len();

        let mut footer = Reader::new(&data[capacity - 8..]);
        let header_size = footer.u32() as usize;
        let signature = footer.u32();
        if signature != SIGNATURE {
            return Err("not an LSPK package (missing footer signature) — only version 13 (DOS2:DE) is supported".into());
        }
        if header_size != FOOTER_SIZE {
            return Err(format!(
                "unexpected LSPK header size {header_size} (expected {FOOTER_SIZE}) — only PackageVersion 13 (DOS2:DE) is supported"
            ));
        }

        let header_start = capacity - header_size;
        let mut r = Reader::new(&data[header_start..]);
        let version = r.u32();
        if version != 13 {
            return Err(format!("unsupported PAK version {version} — only version 13 (DOS2:DE) is supported"));
        }
        let file_list_offset = r.u32() as usize;
        let file_list_size = r.u32() as usize;
        let _num_parts = r.u16();
        let flags = r.u8();
        let _priority = r.u8();
        let _md5 = r.bytes(16);

        if file_list_size < 4 {
            return Err("corrupt file list (size < 4)".into());
        }

        let mut flr = Reader::new(&data[file_list_offset..]);
        let num_files = flr.u32() as usize;
        let compressed_list = data
            .get(file_list_offset + 4..file_list_offset + file_list_size)
            .ok_or("file list extends past end of archive")?;
        let list_bytes = lz4_block_decompress(compressed_list, num_files * FILE_ENTRY_SIZE)?;

        let mut lr = Reader::new(&list_bytes);
        let mut raw_entries: Vec<RawEntry> = Vec::with_capacity(num_files);
        for _ in 0..num_files {
            let name = lr.fixed_str(FILE_NAME_LEN);
            let offset_in_file = lr.u32() as usize;
            let size_on_disk = lr.u32() as usize;
            let uncompressed_size = lr.u32();
            let archive_part = lr.u32();
            let entry_flags = lr.u32();
            let crc = lr.u32();
            raw_entries.push((name, offset_in_file, size_on_disk, uncompressed_size, archive_part, entry_flags, crc));
        }

        let solid = flags & FLAG_SOLID != 0;
        let entries = if solid {
            unpack_solid(data, &raw_entries)?
        } else {
            raw_entries
                .into_iter()
                .map(|(name, offset, size_on_disk, uncompressed_size, archive_part, entry_flags, crc)| {
                    if archive_part != 0 {
                        return Err(format!("multi-part archives are not supported ({name} is in part {archive_part})"));
                    }
                    let compressed = data
                        .get(offset..offset + size_on_disk)
                        .ok_or_else(|| format!("file entry {name} out of bounds"))?
                        .to_vec();
                    Ok(PakEntry { name, archive_part, flags: entry_flags, crc, uncompressed_size, compressed })
                })
                .collect::<Result<Vec<_>, String>>()?
        };

        Ok(Pak { entries })
    }

    pub fn entry(&self, name: &str) -> Option<&PakEntry> {
        self.entries.iter().find(|e| e.name.eq_ignore_ascii_case(name))
    }

    pub fn read(&self, name: &str) -> Result<Vec<u8>, String> {
        self.entry(name).ok_or_else(|| format!("no such file in package: {name}"))?.read()
    }

    /// Replaces a file's content, recompressing with the same method the
    /// entry already used — so files we don't intend to touch the
    /// encoding of keep whatever method the original save used.
    pub fn set_file(&mut self, name: &str, new_uncompressed: &[u8]) -> Result<(), String> {
        let entry = self
            .entries
            .iter_mut()
            .find(|e| e.name.eq_ignore_ascii_case(name))
            .ok_or_else(|| format!("no such file in package: {name}"))?;
        let method = CompressionMethod::from_low_nibble(entry.flags)?;
        let compressed = compress_with(new_uncompressed, method, CHUNKED);
        entry.crc = crc32(&compressed);
        entry.uncompressed_size = new_uncompressed.len() as u32;
        entry.compressed = compressed;
        Ok(())
    }

    /// Adds a brand-new file, compressed with `method`.
    pub fn add_file(&mut self, name: impl Into<String>, uncompressed: &[u8], method: CompressionMethod) {
        let compressed = compress_with(uncompressed, method, CHUNKED);
        self.entries.push(PakEntry {
            name: name.into(),
            archive_part: 0,
            flags: method.low_nibble(),
            crc: crc32(&compressed),
            uncompressed_size: uncompressed.len() as u32,
            compressed,
        });
    }

    /// Serializes this package back into `.lsv`/`.pak` bytes. Rebuilds
    /// the file-list/header/footer from scratch (offsets are recomputed,
    /// since an edited file's compressed size can differ from the
    /// original), but reuses each entry's already-compressed bytes
    /// as-is — untouched files stay byte-identical in content (only
    /// their absolute file offset may shift), and only entries changed
    /// via `set_file`/`add_file` differ at the content level. Never
    /// writes a Solid archive (see `unpack_solid`'s doc comment).
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Writer::new();
        let mut positioned = Vec::with_capacity(self.entries.len());
        for entry in &self.entries {
            let offset = out.len();
            out.bytes(&entry.compressed);
            pad_to(&mut out, PADDING);
            positioned.push((entry, offset));
        }

        let file_list_offset = out.len();
        let mut list = Writer::new();
        for (entry, offset) in &positioned {
            list.fixed_str(&entry.name, FILE_NAME_LEN);
            list.u32(*offset as u32);
            list.u32(entry.compressed.len() as u32);
            list.u32(entry.uncompressed_size);
            list.u32(entry.archive_part);
            list.u32(entry.flags);
            list.u32(entry.crc);
        }
        let compressed_list = lz4_block_compress(&list.into_bytes());

        out.u32(self.entries.len() as u32);
        out.bytes(&compressed_list);
        let file_list_size = (out.len() - file_list_offset) as u32;

        out.u32(13); // version
        out.u32(file_list_offset as u32);
        out.u32(file_list_size);
        out.u16(1); // num_parts
        out.u8(0); // flags — we never write Solid
        out.u8(0); // priority
        out.bytes(&[0u8; 16]); // md5 — left zeroed; the game doesn't check it, only lslib's own tools do

        out.u32(HEADER_SIZE_V13 + 8);
        out.u32(SIGNATURE);

        out.into_bytes()
    }
}

fn pad_to(w: &mut Writer, boundary: usize) {
    let rem = w.len() % boundary;
    if rem != 0 {
        w.bytes(&vec![PADDING_BYTE; boundary - rem]);
    }
}

/// Solid archives store every file's content concatenated into one LZ4
/// **frame** rather than per-file blocks. This is unconfirmed to ever
/// appear in a real DOS2:DE `.lsv` (flagged as a "verify against real
/// data" risk in the project plan) and lslib's own V13 writer doesn't
/// actually emit a real solid segment despite accepting the flag — so
/// there's no reference round-trip to match. Implemented best-effort:
/// decompress the whole segment once and materialize each file as
/// plain, uncompressed (`CompressionMethod::None`) — i.e. this
/// "un-solidifies" on read. `to_bytes` never re-creates a solid archive.
fn unpack_solid(data: &[u8], raw_entries: &[RawEntry]) -> Result<Vec<PakEntry>, String> {
    let first_offset = raw_entries.iter().map(|e| e.1).min().ok_or("empty solid archive")?;
    let total_size_on_disk: usize = raw_entries.iter().map(|e| e.2).sum();
    let total_uncompressed: usize = raw_entries.iter().map(|e| e.3 as usize).sum();

    let frame = data
        .get(first_offset..first_offset + total_size_on_disk)
        .ok_or("solid segment out of bounds")?;
    let decompressed = lz4_frame_decompress(frame)?;
    if decompressed.len() != total_uncompressed {
        return Err(format!(
            "solid segment decompressed to {} bytes, expected {total_uncompressed}",
            decompressed.len()
        ));
    }

    let mut entries = Vec::with_capacity(raw_entries.len());
    let mut pos = 0usize;
    for (name, _offset, _size_on_disk, uncompressed_size, archive_part, _flags, _crc) in raw_entries {
        let size = *uncompressed_size as usize;
        let bytes = decompressed
            .get(pos..pos + size)
            .ok_or("solid segment shorter than declared uncompressed sizes")?
            .to_vec();
        pos += size;
        entries.push(PakEntry {
            name: name.clone(),
            archive_part: *archive_part,
            flags: 0,
            crc: crc32(&bytes),
            uncompressed_size: *uncompressed_size,
            compressed: bytes,
        });
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_synthetic_package() {
        let mut pak = Pak::default();
        pak.add_file("meta.lsf", b"pretend meta contents", CompressionMethod::Zlib);
        pak.add_file("globals.lsf", &b"pretend globals contents, longer".repeat(50), CompressionMethod::Lz4);
        pak.add_file("Screenshot.png", b"\x89PNG\r\n\x1a\nfakepngdata", CompressionMethod::None);

        let bytes = pak.to_bytes();
        let reparsed = Pak::parse(&bytes).expect("re-parse should succeed");

        assert_eq!(reparsed.entries.len(), pak.entries.len());
        assert_eq!(reparsed.read("meta.lsf").unwrap(), b"pretend meta contents");
        assert_eq!(reparsed.read("globals.lsf").unwrap(), b"pretend globals contents, longer".repeat(50));
        assert_eq!(reparsed.read("Screenshot.png").unwrap(), b"\x89PNG\r\n\x1a\nfakepngdata");
    }

    #[test]
    fn set_file_recompresses_with_original_method() {
        let mut pak = Pak::default();
        pak.add_file("globals.lsf", b"original content", CompressionMethod::Lz4);
        pak.set_file("globals.lsf", b"edited content, different length!").unwrap();

        let bytes = pak.to_bytes();
        let reparsed = Pak::parse(&bytes).unwrap();
        assert_eq!(reparsed.read("globals.lsf").unwrap(), b"edited content, different length!");
        assert_eq!(reparsed.entry("globals.lsf").unwrap().method().unwrap(), CompressionMethod::Lz4);
    }

    #[test]
    fn case_insensitive_lookup() {
        let mut pak = Pak::default();
        pak.add_file("Globals.lsf", b"x", CompressionMethod::None);
        assert!(pak.entry("globals.lsf").is_some());
    }
}
