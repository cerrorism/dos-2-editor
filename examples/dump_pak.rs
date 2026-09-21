//! Read-only: lists every file inside a real `.lsv`/`.pak`, with size
//! and compression method. Run against a real save once you have one:
//!
//!   cargo run --example dump_pak -- "<path-to-save>.lsv" [entry-to-print]
//!
//! This is the first thing to run against a real save — it answers the
//! "what files does a real .lsv actually contain" question from the
//! project plan's risk checklist.
use std::path::PathBuf;

use dos2_editor::format::pak::Pak;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().map(PathBuf::from).unwrap_or_else(|| {
        eprintln!("usage: dump_pak <path-to-.lsv-or-.pak> [entry-to-print]");
        std::process::exit(1);
    });

    let pak = match Pak::open(&path) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("failed to open {}: {e}", path.display());
            std::process::exit(1);
        }
    };

    if let Some(entry) = args.next() {
        let bytes = pak.read(&entry).unwrap_or_else(|error| {
            eprintln!("failed to read {entry}: {error}");
            std::process::exit(1);
        });
        print!("{}", String::from_utf8_lossy(&bytes));
        return;
    }

    println!("{} file(s) in {}", pak.entries.len(), path.display());
    println!(
        "{:<50} {:>12} {:>12} {:>10} {:>10}",
        "name", "size", "compressed", "method", "crc"
    );
    for entry in &pak.entries {
        let method = entry.flags & 0x0F;
        let method_name = match method {
            0 => "none",
            1 => "zlib",
            2 => "lz4",
            3 => "zstd",
            _ => "?",
        };
        println!(
            "{:<50} {:>12} {:>12} {:>10} {:>10x}",
            entry.name,
            entry.uncompressed_size,
            entry.compressed.len(),
            method_name,
            entry.crc
        );
    }
}
