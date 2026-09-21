//! Read-only summary of the typed item records in a DOS2:DE save.
//!
//! Usage: `cargo run --example dump_items -- <save.lsv>`
use std::path::PathBuf;

use dos2_editor::domain::item;
use dos2_editor::format::{lsf, pak::Pak};

fn main() {
    let Some(path) = std::env::args().nth(1).map(PathBuf::from) else {
        eprintln!("usage: dump_items <save.lsv>");
        std::process::exit(1);
    };
    let resource = Pak::open(&path)
        .and_then(|pak| pak.read("globals.lsf"))
        .and_then(|bytes| lsf::parse(&bytes))
        .unwrap_or_else(|error| {
            eprintln!("failed to read {}: {error}", path.display());
            std::process::exit(1);
        });

    let items = item::items(&resource);
    if let Some(index) = std::env::args()
        .nth(2)
        .and_then(|value| value.parse::<usize>().ok())
    {
        match items.get(index) {
            Some(item) => println!("{:#?}", item.node()),
            None => eprintln!("item index {index} is out of range ({} items)", items.len()),
        }
        return;
    }
    println!("{} item(s)", items.len());
    for (index, item) in items.iter().enumerate() {
        println!(
            "#{index}: stats={:?}, amount={:?}, type={:?}, level={:?}, handle={:?}",
            item.stats_id(),
            item.amount(),
            item.item_type(),
            item.level(),
            item.handle(),
        );
        println!(
            "  slot={:?}, inventory={:?}, custom_name={:?}, runes={:?}",
            item.slot(),
            item.inventory_id(),
            item.custom_display_name(),
            [
                item.rune_stats_id(0),
                item.rune_stats_id(1),
                item.rune_stats_id(2)
            ],
        );
    }
}
