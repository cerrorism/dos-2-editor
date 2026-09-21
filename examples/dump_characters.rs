//! Read-only summary of player-party records in a DOS2:DE save.
use std::path::PathBuf;

use dos2_editor::domain::{character, item};
use dos2_editor::format::{lsf, pak::Pak};

fn main() {
    let Some(path) = std::env::args().nth(1).map(PathBuf::from) else {
        eprintln!("usage: dump_characters <save.lsv>");
        std::process::exit(1);
    };
    let resource = Pak::open(&path)
        .and_then(|pak| pak.read("globals.lsf"))
        .and_then(|bytes| lsf::parse(&bytes))
        .unwrap_or_else(|error| {
            eprintln!("failed to read {}: {error}", path.display());
            std::process::exit(1);
        });
    let party = character::party_members(&resource);
    if let Some(index) = std::env::args()
        .nth(2)
        .and_then(|value| value.parse::<usize>().ok())
    {
        match party.get(index) {
            Some(character) => println!("{:#?}", character.node()),
            None => eprintln!("party member index {index} is out of range"),
        }
        return;
    }
    println!("{} player character(s)", party.len());
    for (index, character) in party.iter().enumerate() {
        let inventory_items = character.inventory_handle().map(|handle| {
            item::items(&resource)
                .into_iter()
                .filter(|value| value.parent_handle() == Some(handle))
                .count()
        });
        println!(
            "#{index}: name={:?}, origin={:?}, level={:?}, inventory={:?}, inventory_items={inventory_items:?}",
            character.name(),
            character.origin_name(),
            character.level(),
            character.inventory_handle(),
        );
    }
}
