# dos-2-editor

A save file editor for **Divinity: Original Sin 2 — Definitive Edition**, focused on
character attributes and item attributes (items are the priority). Built with Rust +
[egui](https://github.com/emilk/egui): a small, single-purpose tool with a UI you don't
need a wiki to understand.

The on-disk formats (`.lsv` save container, `LSF` binary resource format) are
reimplemented from scratch in pure Rust, using [Norbyte's lslib](https://github.com/Norbyte/lslib)
(MIT licensed) purely as a format reference — this project never links, calls, or shells
out to lslib/Divine.exe at runtime.

## Status

The editor can load a save, show the actual player characters as separate inventory
tabs, resolve many saved items to their English in-game names, and edit existing item
fields. **Save Edited Copy** writes a separately named save and verifies its PAK/LSF
structure before writing; it never overwrites the selected original. The raw save tree
is available only under **Advanced** for investigation.

Item names come from the installed game's English or Simplified Chinese localization and
Shared templates; choose the display language in the top bar.
Generated/procedural equipment that has no direct template name is shown using a
readable Stat-ID fallback for now. See [CLAUDE.md](CLAUDE.md) for verification details.

## Build

```powershell
cargo build --release
```

Output: `target\release\dos-2-editor.exe`.

> Build from a plain PowerShell prompt, not a shell where Git for Windows' `usr\bin`
> precedes the Visual Studio tools on `PATH` — the MSVC linker gets shadowed by Git's
> POSIX `link.exe` and the build fails with a linker error.

## Testing

```powershell
cargo test
```

Runs structural round-trip tests (`parse(serialize(x)) == x`) for the PAK and LSF format
layers against synthetic data. See [CLAUDE.md](CLAUDE.md) for the `examples/` tools meant
to be run against a real save/game install once one is available.

## Project structure

```
src/
  main.rs, app.rs   — eframe entry point and party-inventory editor UI
  config.rs          — persisted settings (last-used folders)
  save_file.rs        — save-folder discovery
  format/             — PAK container + LSF resource format (DOS2:DE only)
  domain/              — typed Characters/Items accessors
  gamedata/             — game-install English localization/template catalog
```
