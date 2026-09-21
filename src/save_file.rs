//! Savegame discovery (finding `.lsv` files) and the backup-before-write
//! save path. Loading/parsing a `.lsv`'s contents lives in
//! `format::pak`/`format::lsf`; this module only knows about the
//! filesystem layout DOS2:DE uses to store savegames.
use std::fs;
use std::path::{Path, PathBuf};

/// The default `PlayerProfiles` root(s) for DOS2:DE, in the order they
/// should be tried. Checks both a plain `Documents` folder and
/// OneDrive-redirected `Documents`, same convention as bg2-editor.
pub fn default_save_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for docs in documents_candidates() {
        roots.push(
            docs.join("Larian Studios")
                .join("Divinity Original Sin 2 Definitive Edition")
                .join("PlayerProfiles"),
        );
    }
    roots
}

fn documents_candidates() -> Vec<PathBuf> {
    let Some(profile) = std::env::var_os("USERPROFILE").map(PathBuf::from) else {
        return Vec::new();
    };
    let mut candidates = vec![profile.join("Documents")];
    if let Ok(entries) = fs::read_dir(&profile) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("OneDrive") {
                candidates.push(entry.path().join("Documents"));
            }
        }
    }
    candidates
}

/// Lists every `.lsv` savegame under a `PlayerProfiles` root (each
/// profile has its own `<profile>/Savegames/` subfolder — a "Savegames"
/// directory can itself contain per-save subfolders too on some
/// installs, so this walks two levels deep looking for `.lsv` files),
/// newest first by modified time.
pub fn list_savegames(player_profiles_root: &Path) -> Vec<PathBuf> {
    let mut found: Vec<(PathBuf, std::time::SystemTime)> = Vec::new();
    collect_lsv_files(player_profiles_root, 3, &mut found);
    found.sort_by_key(|(_, modified)| std::cmp::Reverse(*modified));
    found.into_iter().map(|(p, _)| p).collect()
}

fn collect_lsv_files(dir: &Path, depth: u32, out: &mut Vec<(PathBuf, std::time::SystemTime)>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if depth > 0 {
                collect_lsv_files(&path, depth - 1, out);
            }
        } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("lsv")) {
            if let Ok(modified) = entry.metadata().and_then(|m| m.modified()) {
                out.push((path, modified));
            }
        }
    }
}

/// Builds a non-clobbering backup path (`<name>.lsv.bak`, `.bak2`, ...)
/// next to an original save file.
pub fn make_backup_path(save_path: &Path) -> PathBuf {
    let candidate = save_path.with_extension("lsv.bak");
    if !candidate.exists() {
        return candidate;
    }
    let mut n = 2u32;
    loop {
        let c = save_path.with_extension(format!("lsv.bak{n}"));
        if !c.exists() {
            return c;
        }
        n += 1;
    }
}
