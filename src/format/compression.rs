//! Compression primitives shared by `pak.rs` (per-file + file-list) and
//! `lsf.rs` (per-section). Larian's formats mix three different LZ4/zlib
//! usages, confirmed directly against lslib's `Compression.cs`:
//! - raw LZ4 **block** format (no framing, decoder needs the exact
//!   uncompressed size up front) — used for PAK per-file compression,
//!   the PAK file list, and LSF's `Strings` section.
//! - LZ4 **frame** format (self-describing, streamable) — used for LSF's
//!   `Nodes`/`Attributes`/`Values`/`Keys` sections (version >=
//!   `VerChunkedCompress`, which DOS2:DE's version 3 satisfies).
//! - plain zlib (RFC1950) — an alternative method either format can use
//!   instead of LZ4, selected per-file/per-resource by a `CompressionFlags`
//!   byte (low nibble = method, high nibble = level).
use std::io::{Read, Write};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionMethod {
    None,
    Zlib,
    Lz4,
}

impl CompressionMethod {
    /// Decodes the low nibble of a `CompressionFlags` byte (shared by
    /// PAK file entries and LSF's per-resource metadata).
    pub fn from_low_nibble(v: u32) -> Result<Self, String> {
        match v & 0x0F {
            0 => Ok(CompressionMethod::None),
            1 => Ok(CompressionMethod::Zlib),
            2 => Ok(CompressionMethod::Lz4),
            other => Err(format!("unsupported compression method {other} (Zstd/unknown, not needed for DOS2:DE)")),
        }
    }

    pub fn low_nibble(self) -> u32 {
        match self {
            CompressionMethod::None => 0,
            CompressionMethod::Zlib => 1,
            CompressionMethod::Lz4 => 2,
        }
    }
}

pub fn zlib_compress(data: &[u8]) -> Vec<u8> {
    use flate2::write::ZlibEncoder;
    use flate2::Compression;
    let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
    enc.write_all(data).expect("in-memory write cannot fail");
    enc.finish().expect("in-memory finish cannot fail")
}

pub fn zlib_decompress(data: &[u8]) -> Result<Vec<u8>, String> {
    use flate2::read::ZlibDecoder;
    let mut dec = ZlibDecoder::new(data);
    let mut out = Vec::new();
    dec.read_to_end(&mut out).map_err(|e| format!("zlib decompress failed: {e}"))?;
    Ok(out)
}

pub fn lz4_block_compress(data: &[u8]) -> Vec<u8> {
    lz4_flex::block::compress(data)
}

pub fn lz4_block_decompress(data: &[u8], uncompressed_size: usize) -> Result<Vec<u8>, String> {
    lz4_flex::block::decompress(data, uncompressed_size).map_err(|e| format!("lz4 block decompress failed: {e}"))
}

pub fn lz4_frame_compress(data: &[u8]) -> Vec<u8> {
    use lz4_flex::frame::FrameEncoder;
    let mut enc = FrameEncoder::new(Vec::new());
    enc.write_all(data).expect("in-memory write cannot fail");
    enc.finish().expect("in-memory finish cannot fail")
}

pub fn lz4_frame_decompress(data: &[u8]) -> Result<Vec<u8>, String> {
    use lz4_flex::frame::FrameDecoder;
    let mut dec = FrameDecoder::new(data);
    let mut out = Vec::new();
    dec.read_to_end(&mut out).map_err(|e| format!("lz4 frame decompress failed: {e}"))?;
    Ok(out)
}

pub fn crc32(data: &[u8]) -> u32 {
    crc32fast::hash(data)
}

/// Compresses `data` with `method`, using the LZ4 **frame** format
/// instead of raw block when `chunked` is true and `method` is `Lz4`
/// (irrelevant for `Zlib`/`None`, which have no block/frame distinction).
pub fn compress_with(data: &[u8], method: CompressionMethod, chunked: bool) -> Vec<u8> {
    match method {
        CompressionMethod::None => data.to_vec(),
        CompressionMethod::Zlib => zlib_compress(data),
        CompressionMethod::Lz4 => {
            if chunked {
                lz4_frame_compress(data)
            } else {
                lz4_block_compress(data)
            }
        }
    }
}

pub fn decompress_with(data: &[u8], method: CompressionMethod, uncompressed_size: usize, chunked: bool) -> Result<Vec<u8>, String> {
    match method {
        CompressionMethod::None => Ok(data.to_vec()),
        CompressionMethod::Zlib => zlib_decompress(data),
        CompressionMethod::Lz4 => {
            if chunked {
                lz4_frame_decompress(data)
            } else {
                lz4_block_decompress(data, uncompressed_size)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zlib_round_trip() {
        let data = b"the quick brown fox jumps over the lazy dog".repeat(20);
        let compressed = zlib_compress(&data);
        assert_eq!(zlib_decompress(&compressed).unwrap(), data);
    }

    #[test]
    fn lz4_block_round_trip() {
        let data = b"the quick brown fox jumps over the lazy dog".repeat(20);
        let compressed = lz4_block_compress(&data);
        assert_eq!(lz4_block_decompress(&compressed, data.len()).unwrap(), data);
    }

    #[test]
    fn lz4_frame_round_trip() {
        let data = b"the quick brown fox jumps over the lazy dog".repeat(20);
        let compressed = lz4_frame_compress(&data);
        assert_eq!(lz4_frame_decompress(&compressed).unwrap(), data);
    }
}
