//! Verifies visible-name resolution against an installed Definitive Edition game.
use std::path::PathBuf;

use dos2_editor::domain::item;
use dos2_editor::format::{lsf, pak::Pak};
use dos2_editor::gamedata::catalog::StatsCatalog;

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(game_root) = args.next().map(PathBuf::from) else {
        eprintln!("usage: dump_stats_catalog <game-root> [save.lsv]");
        std::process::exit(1);
    };
    let catalog = StatsCatalog::load(&game_root).unwrap_or_else(|error| {
        eprintln!("failed to load catalog: {error}");
        std::process::exit(1);
    });
    println!(
        "{} localization entries, {} template names, {} stats names",
        catalog.localization_count(),
        catalog.template_name_count(),
        catalog.stats_name_count()
    );
    if let Some(save) = args.next().map(PathBuf::from) {
        let resource = Pak::open(&save)
            .and_then(|pak| pak.read("globals.lsf"))
            .and_then(|bytes| lsf::parse(&bytes))
            .unwrap();
        let mut shown = 0;
        for value in item::items(&resource) {
            let stats = value.stats_id().unwrap_or("<missing>");
            if let Some(name) = catalog.display_name(value.current_template(), stats) {
                println!("{stats} => {name}");
                shown += 1;
                if shown == 20 {
                    break;
                }
            }
        }
        println!("shown {shown} resolved item names");
    }
}
