//! Tiny persisted config (last-used save folder and game install folder)
//! so the user doesn't have to re-pick them every launch. Stored as a
//! `key=value` text file under the user's per-user app-data folder
//! (`%APPDATA%\dos-2-editor\dos-2-editor.cfg` on Windows) — not next to
//! the executable, since that path differs between `cargo run` (a
//! `target\debug\...` exe) and a manually-run release build.
use std::collections::HashMap;
use std::path::{Path, PathBuf};

fn config_dir() -> Option<PathBuf> {
    if let Some(appdata) = std::env::var_os("APPDATA") {
        return Some(PathBuf::from(appdata).join("dos-2-editor"));
    }
    std::env::current_exe().ok()?.parent().map(|d| d.to_path_buf())
}

fn config_path() -> Option<PathBuf> {
    let dir = config_dir()?;
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join("dos-2-editor.cfg"))
}

fn load_map() -> HashMap<String, String> {
    let mut map = HashMap::new();
    let Some(path) = config_path() else { return map };
    let Ok(text) = std::fs::read_to_string(path) else { return map };
    for line in text.lines() {
        if let Some((k, v)) = line.split_once('=') {
            map.insert(k.trim().to_owned(), v.trim().to_owned());
        }
    }
    map
}

fn save_map(map: &HashMap<String, String>) {
    let Some(path) = config_path() else { return };
    let mut text = String::new();
    for (k, v) in map {
        text.push_str(k);
        text.push('=');
        text.push_str(v);
        text.push('\n');
    }
    let _ = std::fs::write(path, text);
}

fn load_dir(key: &str) -> Option<PathBuf> {
    let map = load_map();
    let v = map.get(key)?;
    let p = PathBuf::from(v);
    p.is_dir().then_some(p)
}

fn save_dir(key: &str, dir: &Path) {
    let mut map = load_map();
    map.insert(key.to_owned(), dir.to_string_lossy().into_owned());
    save_map(&map);
}

pub fn load_game_root() -> Option<PathBuf> {
    load_dir("game_root")
}

pub fn save_game_root(root: &Path) {
    save_dir("game_root", root);
}

pub fn load_save_root() -> Option<PathBuf> {
    load_dir("save_root")
}

pub fn save_save_root(root: &Path) {
    save_dir("save_root", root);
}
