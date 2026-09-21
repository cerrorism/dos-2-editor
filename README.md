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

Early. The binary format layer (PAK container + LSF resource parser/writer) is
implemented and passes structural round-trip tests against synthetic data. The
Characters/Items domain layer and the actual editing UI are not yet built. See
[CLAUDE.md](CLAUDE.md) for the detailed status and a handover doc for whoever picks this
up next.

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
