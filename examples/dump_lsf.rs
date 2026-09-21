//! Read-only: parses an LSF resource (either a loose `.lsf` file, or an
//! entry extracted from a `.lsv`/`.pak`) and pretty-prints its full node
//! tree. Run against a real save once you have one:
//!
//!   cargo run --example dump_lsf -- "<path-to-save>.lsv" globals.lsf
//!   cargo run --example dump_lsf -- "<path-to-save>.lsv" meta.lsf
//!   cargo run --example dump_lsf -- "<path-to-loose-file>.lsf"
//!
//! This is the single most important diagnostic tool in the project: it
//! lets you visually confirm the Characters/Items node-path assumptions
//! in the project plan actually match a real save's structure.
use std::path::PathBuf;

use dos2_editor::format::lsf;
use dos2_editor::format::node::{AttributeValue, Node, Resource};
use dos2_editor::format::pak::Pak;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().map(PathBuf::from).unwrap_or_else(|| {
        eprintln!("usage: dump_lsf <path-to-.lsv-or-.lsf> [entry-name-if-.lsv, default globals.lsf]");
        std::process::exit(1);
    });
    let entry_name = args.next().unwrap_or_else(|| "globals.lsf".to_string());

    let bytes = if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("lsv") || e.eq_ignore_ascii_case("pak")) {
        let pak = match Pak::open(&path) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("failed to open package {}: {e}", path.display());
                std::process::exit(1);
            }
        };
        match pak.read(&entry_name) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("failed to read {entry_name} from package: {e}");
                eprintln!("files in package:");
                for entry in &pak.entries {
                    eprintln!("  {}", entry.name);
                }
                std::process::exit(1);
            }
        }
    } else {
        match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("failed to read {}: {e}", path.display());
                std::process::exit(1);
            }
        }
    };

    let resource = match lsf::parse(&bytes) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("failed to parse LSF: {e}");
            std::process::exit(1);
        }
    };

    print_resource(&resource);
}

fn print_resource(resource: &Resource) {
    let mut names: Vec<&String> = resource.regions.keys().collect();
    names.sort();
    for name in names {
        println!("Region: {name}");
        print_node(&resource.regions[name], 1);
    }
}

fn print_node(node: &Node, depth: usize) {
    let indent = "  ".repeat(depth);
    let mut attr_names: Vec<&String> = node.attributes.keys().collect();
    attr_names.sort();
    for name in attr_names {
        let attr = &node.attributes[name];
        println!("{indent}{name} = {}", format_value(&attr.value));
    }

    let mut child_tags: Vec<&String> = node.children.keys().collect();
    child_tags.sort();
    for tag in child_tags {
        for (i, child) in node.children[tag].iter().enumerate() {
            println!("{indent}[{tag}#{i}] {}", child.name);
            print_node(child, depth + 1);
        }
    }
}

fn format_value(v: &AttributeValue) -> String {
    match v {
        AttributeValue::None => "<none>".to_string(),
        AttributeValue::U8(x) => x.to_string(),
        AttributeValue::I16(x) => x.to_string(),
        AttributeValue::U16(x) => x.to_string(),
        AttributeValue::I32(x) => x.to_string(),
        AttributeValue::U32(x) => x.to_string(),
        AttributeValue::F32(x) => x.to_string(),
        AttributeValue::F64(x) => x.to_string(),
        AttributeValue::IVec(v) => format!("{v:?}"),
        AttributeValue::Vec(v) => format!("{v:?}"),
        AttributeValue::Mat(v) => format!("{v:?}"),
        AttributeValue::Bool(b) => b.to_string(),
        AttributeValue::Str(s) => format!("{s:?}"),
        AttributeValue::U64(x) => x.to_string(),
        AttributeValue::ScratchBuffer(b) => format!("<{} bytes>", b.len()),
        AttributeValue::I64(x) => x.to_string(),
        AttributeValue::I8(x) => x.to_string(),
        AttributeValue::TranslatedString(ts) => format!("{:?} (handle {})", ts.value, ts.handle),
        AttributeValue::Uuid(u) => u.to_string(),
        AttributeValue::TranslatedFsString(fs) => format!("{:?} (handle {})", fs.string.value, fs.string.handle),
    }
}
