//! Verifies visible-name resolution against an installed Definitive Edition game.
use std::path::PathBuf;

use dos2_editor::domain::item;
use dos2_editor::format::{lsf, pak::Pak};
use dos2_editor::gamedata::catalog::{DisplayLanguage, StatsCatalog};

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(game_root) = args.next().map(PathBuf::from) else {
        eprintln!("usage: dump_stats_catalog <game-root> [save.lsv] [english|chinese]");
        std::process::exit(1);
    };
    let save = args.next().map(PathBuf::from);
    let language = match args.next().as_deref() {
        Some("chinese") => DisplayLanguage::SimplifiedChinese,
        Some("english") | None => DisplayLanguage::English,
        Some(other) => {
            eprintln!("unknown language: {other}");
            std::process::exit(1);
        }
    };
    let catalog = StatsCatalog::load(&game_root, language).unwrap_or_else(|error| {
        eprintln!("failed to load catalog: {error}");
        std::process::exit(1);
    });
    println!(
        "{} localization entries, {} template names, {} stats names",
        catalog.localization_count(),
        catalog.template_name_count(),
        catalog.stats_name_count()
    );
    if let Some(save) = save {
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
        for value in item::items(&resource).into_iter().filter(|value| {
            value.stats_id() == Some("ARM_Light_UpperBody") && value.level() == Some(5)
        }) {
            println!(
                "generated ARM_Light_UpperBody => {:?}",
                catalog.generated_name(
                    "ARM_Light_UpperBody",
                    value.item_type(),
                    value.level(),
                    value.level_group_index(),
                    value.name_index(),
                )
            );
        }
        println!("shown {shown} resolved item names");
    }
}
