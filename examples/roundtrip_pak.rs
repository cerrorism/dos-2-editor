//! Parses and reserializes a PAK/LSV in memory without writing any files.
//!
//! Usage: `cargo run --example roundtrip_pak -- <save.lsv>`
use std::path::PathBuf;

use dos2_editor::format::pak::Pak;

fn main() {
    let Some(path) = std::env::args().nth(1).map(PathBuf::from) else {
        eprintln!("usage: roundtrip_pak <save.lsv>");
        std::process::exit(1);
    };
    let source = std::fs::read(&path).unwrap_or_else(|error| {
        eprintln!("failed to read {}: {error}", path.display());
        std::process::exit(1);
    });
    let parsed = Pak::parse(&source).unwrap_or_else(|error| {
        eprintln!("parse failed: {error}");
        std::process::exit(1);
    });
    let rewritten = parsed.to_bytes();
    let reparsed = Pak::parse(&rewritten).unwrap_or_else(|error| {
        eprintln!("reparse failed: {error}");
        std::process::exit(1);
    });
    let contents_equal =
        parsed.entries.iter().all(|entry| {
            reparsed.read(&entry.name).ok().as_deref() == entry.read().ok().as_deref()
        }) && parsed.entries.len() == reparsed.entries.len();
    println!(
        "source: {} bytes; rewritten: {} bytes; byte-identical: {}",
        source.len(),
        rewritten.len(),
        source == rewritten
    );
    println!(
        "{} entries; uncompressed entry contents identical: {}",
        parsed.entries.len(),
        contents_equal
    );
}
