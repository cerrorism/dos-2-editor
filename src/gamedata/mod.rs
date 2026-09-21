//! Game-data catalog (Data.pak stat/localization parsing) for item
//! name/rarity resolution. TODO (project plan Phase 5) — start with the
//! empirical inspection step (run the `dump_pak` example against a real
//! `Data.pak`) before writing `statfile.rs`'s parser.
pub mod catalog;
pub mod localization;
pub mod statfile;
