# dos-2-editor — development notes / handover

**Plan file** (the source of truth for scope, phasing, and the full DOS2:DE domain-knowledge
tables for Characters/Items): `C:\Users\cerro\.claude\plans\we-are-going-to-keen-canyon.md`
on the user's machine. Read it before doing anything else — this file is a status/handover
summary, not a replacement for it.

This is a personal/fun project for the user — no CI, no need for generality beyond
DOS2:DE, hardcoded assumptions are fine where they simplify things. Priority order per the
user: **item editing first**, character editing second.

## Git

Local repo only (no remote) — initialized with `git init`, `main` branch. First commit
(`Scaffold DOS2:DE savegame editor: PAK/LSF format layer`) covers everything described
below as "Implemented and passing" — the Phase 0–2 scaffold and format layer. `target/` and
`*.lsv.bak*` are gitignored. Commit as you go; there's no other backstop for this work.

## Status (Phases 0–2 of the plan: done and verified; Phase 3 onward: not started)

**Implemented and passing `cargo test` (12/12) + clean `cargo clippy --all-targets`:**
- `src/format/primitives.rs` — LE byte cursor `Reader`/`Writer`, plus GUID read/write via
  the `uuid` crate's `from_bytes_le`/`to_bytes_le` (see "UUID" gotcha below).
- `src/format/compression.rs` — shared `CompressionMethod` enum (`None`/`Zlib`/`Lz4`) and
  zlib/LZ4-block/LZ4-frame compress/decompress helpers, used by both `pak.rs` and `lsf.rs`.
- `src/format/pak.rs` — PAK/LSPK container, **version 13 only** (DOS2:DE). Reader + writer,
  inline round-trip tests. `Pak::open`/`parse`/`entry`/`read`/`set_file`/`add_file`/`to_bytes`.
- `src/format/node.rs` — the generic, format-agnostic `Resource`/`Node`/`NodeAttribute`/
  `AttributeType`/`AttributeValue` model shared by LSF/LSX/LSJ (we only implement LSF).
  **Important invariant, documented on `Node`**: a child's key in `Node::children` must
  always equal that child's own `.name` — use `Node::push_child`, never insert into
  `children` by hand with an arbitrary key, or the child silently "loses itself" on
  round-trip (the writer serializes each child using its own `.name`, ignoring the map
  key). This bit me once already while writing the lsf.rs test fixture — see the fix in
  git history / the `push_child` addition if you want the concrete failure mode.
- `src/format/lsf.rs` — LSF binary resource format, **version 3 only** (`VerExtendedNodes`,
  DOS2:DE). `parse()`/`serialize()`. Reader supports both node/attribute table shapes a v3
  file can use (`MetadataFormat::None` and `KeysAndAdjacency`); writer always emits the
  plain `None` shape (see the big module-doc comment at the top of `lsf.rs` for why that's
  safe and much simpler).
- `examples/dump_pak.rs` — lists a real `.lsv`/`.pak`'s contents (name/size/compression).
- `examples/dump_lsf.rs` — parses and pretty-prints a full LSF node tree, either from a
  loose `.lsf` or extracted from a `.lsv` (`cargo run --example dump_lsf -- save.lsv
  globals.lsf`). **This is the tool to run first once a real save is available** — it
  directly confirms or corrects every Characters/Items node-path assumption in the plan.
- `src/app.rs`/`main.rs`/`config.rs`/`save_file.rs` — Phase 0 scaffold only: an egui window
  with a folder picker and a savegame list (by discovery, not by parsing yet). No actual
  save loading/editing wired up.

**Not yet started** (everything from the plan's Phase 3 onward):
- `src/domain/character.rs`, `src/domain/item.rs` — currently empty stub files. This is
  the next real chunk of work: typed getters/setters over `format::node::Node` for the
  Characters/Items region layout the plan documents in detail (node paths, attribute
  names, the `PermanentBoost` bonus bag, rune slots, tags, etc.).
- `src/gamedata/*` — empty stubs. Phase 5 in the plan (game-data stat/localization catalog
  for item names/rarity) — the user explicitly wants this built early, not deferred, once
  the domain layer exists.
- The actual item/character editing UI in `app.rs` — currently just a save-file picker
  shell, no save is actually loaded/parsed/edited yet.
- `src/domain/ids.rs` — empty stub for Phase 7 (new-item creation / GUID minting) —
  explicitly lower priority, no prior art exists anywhere (confirmed during planning).

## Key design decisions worth knowing before you touch this code

1. **PAK V13 quirks** (verified directly against lslib's `PackageReader.cs`/
   `PackageWriter.cs` source, not just its docs): the compressed file list has **no
   `compressedSize` field** for version 13 exactly (that's only for `Version > 13`) — the
   LZ4 payload length is `file_list_size - 4`. Footer is `[header(32 bytes)][headerSize:u32
   = 40]["LSPK"]` at EOF; `OffsetInFile` is already absolute (no `DataOffset` adjustment).
2. **UUID mixed-endian**: LSF's `UUID` attribute type round-trips through .NET's
   `Guid(byte[])` constructor, which uses the "Microsoft mixed-endian" layout, not
   RFC-4122. The `uuid` crate's `from_bytes_le`/`to_bytes_le` implement exactly this
   convention, so `Reader::guid`/`Writer::guid` in `primitives.rs` both round-trip
   correctly *and* print the same string a human would see in Larian's own tools.
3. **PAK per-file storage strategy**: `Pak` keeps each entry's already-compressed bytes
   as-is. `set_file`/`add_file` recompress only the entry being changed, reusing whatever
   compression method that entry already had. `to_bytes()` rebuilds the header/file-list
   from scratch (offsets shift since sizes can change) but reuses unmodified entries'
   compressed bytes verbatim — so an edited save differs from the original only in the
   entry(ies) actually changed, not throughout. This is deliberate: it's the basis for the
   plan's "byte-identical except for the intended edit" trust bar (see the plan's Phase 2
   fallback section).
4. **LSF write always uses the plain (`MetadataFormat::None`) shape**, regardless of what
   shape the source file used, because it's simpler (no sibling-index precomputation, no
   explicit per-attribute offset/next-pointer bookkeeping) and equally valid per the
   version-3 spec — the game's own LSF reader must support both, since lslib itself does.
   **This is flagged in the plan as something to verify**: confirm a save we've rewritten
   this way still loads in the actual game, once a real save is available to test with.
5. **String table**: LSF's string table is a hash-bucketed intern table in the general
   case, but the bucket assignment is Larian's own tooling detail (driven by .NET's
   randomized-per-process `string.GetHashCode()` — not even stable run-to-run for lslib
   itself), so it has no bearing on file validity. Our writer uses one string per bucket
   (a degenerate but 100% spec-valid "hash table" — a reader just resolves `(bucket,
   offset)` pairs the file records). Don't try to replicate Larian's actual bucketing;
   there's no reason to.
6. **Solid PAK archives** (`PackageFlags::Solid`, one LZ4-frame-compressed segment for all
   files instead of per-file compression): implemented best-effort on read only (see the
   big doc comment on `unpack_solid` in `pak.rs`) — genuinely unverified whether real
   DOS2:DE saves ever use this, and even lslib's own V13 writer doesn't appear to produce a
   real solid segment despite accepting the flag. Our writer never produces one. Flagged in
   the plan's risk checklist.
7. **Node/PAK sibling order preservation**: `format::lsf::build_resource` (the tree
   assembly step after parsing the flat node table) processes nodes in descending index
   order for a subtle reason — see the comment right above the loop in `lsf.rs` if you need
   to touch it. Getting this backwards silently reverses sibling order (e.g. item lists),
   which the `sibling_order_is_preserved` test exists specifically to catch.

## Verification status

Everything so far is verified only against **synthetic data** (`cargo test`'s inline
round-trip tests: `parse(serialize(x)) == x` for both PAK and LSF, at multiple compression
methods). **Nothing has been checked against a real DOS2:DE save yet** — the user said
they'd supply real save/game-install paths when we get there. Per the plan's risk
checklist, these are the concrete unknowns still to resolve empirically, in priority order:

1. Run `cargo run --example dump_pak -- "<real save>.lsv"` — confirms the actual file list
   inside a real save (expected: `meta.lsf`, `globals.lsf`, a screenshot, maybe more) and
   whether `Solid` ever shows up in practice.
2. Run `cargo run --example dump_lsf -- "<real save>.lsv" globals.lsf` — confirms the
   Characters/Items node-path assumptions from the plan against real structure, and
   whether `MetadataFormat` is `None` or `KeysAndAdjacency` in practice.
3. Once `domain/item.rs`/`domain/character.rs` exist: build a `roundtrip_pak.rs` /
   `roundtrip_lsf.rs` example (per the plan) that round-trips a real save's bytes and
   reports whether it's byte-identical or where it diverges.
4. Eventually: confirm an edited save (written with our always-`MetadataFormat::None` LSF
   shape) actually still loads in the real game — this can't be verified in an agent
   environment at all; it needs the user to try it by hand.

## Build

```powershell
cargo build --release
```

**Gotcha** (same one bg2-editor's README documents): if you build from a shell where Git
for Windows' `usr\bin` precedes the Visual Studio tools on `PATH` (e.g. this repo's default
Bash tool), the MSVC linker gets shadowed by Git's POSIX `link.exe` and every build fails
with a linker error. **Build from plain PowerShell**, not Bash/Git-Bash, until/unless PATH
is fixed. (Confirmed while building this: `cargo build` in Bash failed with `link.exe`
errors on totally unrelated crates; the identical command in PowerShell succeeded.)

## Testing

```powershell
cargo test          # inline structural round-trip tests, no real files needed
cargo clippy --all-targets   # currently clean
cargo run --example dump_pak -- <path>
cargo run --example dump_lsf -- <path> [entry-name]
```

## Suggested next steps for whoever picks this up

1. Re-read the plan file (path at the top of this doc) in full — it has the complete
   Characters/Items node-path and attribute tables that Phase 3 needs to transcribe into
   `domain/character.rs`/`domain/item.rs`.
2. If a real save is available yet, run `dump_pak`/`dump_lsf` against it first and record
   findings in this file before writing domain code against possibly-wrong assumptions.
3. Implement `domain/item.rs` first (explicit user priority): typed accessors per the
   plan's Items table (`Stats` id, `Amount`, rune slots, `CustomDisplayName`/
   `CustomDescription`, `PermanentBoost` bag, tags, ownership), backed by
   `format::node::Node`.
4. Wire a real "load a .lsv, extract+parse globals.lsf, show something real" path into
   `app.rs`'s currently-placeholder UI — even a raw tree view (Phase 3's fallback UI) would
   make this tool minimally useful and testable end-to-end for the first time.
5. Follow the plan's phase order after that (item UI → gamedata catalog → character UI →
   stretch new-item-creation goal).
