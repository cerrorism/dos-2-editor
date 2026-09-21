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

Early. The binary format layer has passed both synthetic tests and structural
round trips against a real DOS2:DE save. The app can load that save's `globals.lsf`
and expose its editable in-memory raw tree; it intentionally cannot write a save yet.
Item accessors are under way, while the dedicated item/character panels and game-data
catalog remain to be built. See [CLAUDE.md](CLAUDE.md) for verification details.

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
  main.rs, app.rs   — eframe entry point and egui UI (currently a placeholder shell)
  config.rs          — persisted settings (last-used folders)
  save_file.rs        — save-folder discovery
  format/             — PAK container + LSF resource format (DOS2:DE only)
  domain/              — typed Characters/Items accessors (not yet implemented)
  gamedata/             — game-install stat/localization catalog (not yet implemented)
```
