//! Parses and reserializes an LSF resource without writing any files.
//!
//! Usage: `cargo run --example roundtrip_lsf -- <save.lsv-or-globals.lsf> [entry]`
use std::path::PathBuf;

use dos2_editor::format::{compression::CompressionMethod, lsf, pak::Pak};

fn main() {
    let Some(path) = std::env::args().nth(1).map(PathBuf::from) else {
        eprintln!(
            "usage: roundtrip_lsf <save.lsv-or-globals.lsf> [entry-name, default globals.lsf]"
        );
        std::process::exit(1);
    };
    let entry = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "globals.lsf".into());
    let source = if path.extension().is_some_and(|extension| {
        extension.eq_ignore_ascii_case("lsv") || extension.eq_ignore_ascii_case("pak")
    }) {
        Pak::open(&path).and_then(|pak| pak.read(&entry))
    } else {
        std::fs::read(&path).map_err(|error| format!("failed to read {}: {error}", path.display()))
    }
    .unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(1);
    });

    let parsed = lsf::parse(&source).unwrap_or_else(|error| {
        eprintln!("parse failed: {error}");
        std::process::exit(1);
    });
    let rewritten = lsf::serialize(&parsed, CompressionMethod::Lz4);
    let reparsed = lsf::parse(&rewritten).unwrap_or_else(|error| {
        eprintln!("reparse failed: {error}");
        std::process::exit(1);
    });
    println!(
        "source: {} bytes; rewritten: {} bytes; byte-identical: {}",
        source.len(),
        rewritten.len(),
        source == rewritten
    );
    println!(
        "structurally identical after parse → serialize → parse: {}",
        parsed == reparsed
    );
}
