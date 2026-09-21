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

## Status (Phases 0–2 done; Phase 3 started and verified against a real save)

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
  with a folder picker and a savegame list. Selecting a save now loads/parses `globals.lsf`
  into memory and presents an editable raw tree; the UI deliberately has no write button yet.
- `src/domain/item.rs` — thin typed item views/mutators for the verified Items hierarchy,
  including `Stats`, `Amount`, nested stats/rune/`PermanentBoost` values, and same-index
  Creator handles. `examples/dump_items.rs` prints the typed real-save summary (an optional
  item index prints its complete raw node for schema investigation).
- `src/app.rs` also has the first item-first panel: a Stats-ID search list and focused
  editor for existing Stats, Amount, Slot, nested Level/name indices, rune slots, and
  PermanentBoost fields. It works in memory only until the safe write path is added.
- `examples/roundtrip_lsf.rs` / `examples/roundtrip_pak.rs` — non-mutating real-file
  round-trip verifiers.

**Still not started / incomplete:**
- `src/domain/character.rs` remains an empty stub. `src/domain/item.rs` is a deliberately
  partial Phase 3/4 implementation; custom name/description, tags, ownership editing,
  per-owner inventory grouping, and all write-path work remain.
- `src/gamedata/*` — empty stubs. Phase 5 in the plan (game-data stat/localization catalog
  for item names/rarity) — the user explicitly wants this built early, not deferred, once
  the domain layer exists.
- The character UI and save-write UI remain unimplemented. The item editor is intentionally
  restricted to modifying in-memory values until scratch-copy backup/reload testing exists.
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

Synthetic tests still cover the format layer, and the supplied real DOS2:DE save has now
been inspected successfully:

- Save: `...\\PlayerProfiles\\cerror1\\Savegames\\Story\\Edit\\Edit.lsv` — a 7-entry,
  non-solid PAK; every entry uses zlib. `globals.lsf` is 5,870,522 bytes.
- `dump_items` found 715 items. The real shape confirms `Items[0].Item[*]` and nested
  `Stats`/`RuneSlot`/`PermanentBoost`. Item owner/parent IDs and Creator handles are
  `ULongLong` engine handles (not UUIDs). Stackable items carry `Amount`; many
  non-stackables omit it entirely. Equipped-item rarity appears in `Stats.ItemType`.
- `roundtrip_lsf`: parse → serialize → parse is structurally equal, but not byte-identical
  (source 5,870,522 bytes; normalized LZ4 writer output 7,604,542 bytes).
- `roundtrip_pak`: all 7 uncompressed entry contents survive intact, but the complete
  archive is not byte-identical (source 5,419,453 bytes; rebuilt output 5,419,471 bytes).
  The project is therefore using the plan's documented semantic-equivalence fallback until
  a scratch-copy edit is manually tested in-game.
- Game install: `D:\\SteamLibrary\\steamapps\\common\\Divinity Original Sin 2`. Definitive
  Edition data uses `DefEd\\Data\\Shared.pak` for generated item stat files (including
  `Public/Shared/Stats/Generated/Data/{Armor,Object,Potion,Shield}.txt` and
  `Weapon.txt`) and `DefEd\\Data\\Localization\\English.pak` for English localization,
  not a single `Data.pak`.

Per the plan's risk checklist, these are the concrete unknowns still to resolve empirically,
in priority order:

1. Confirm the real save's `MetadataFormat` (`None` vs `KeysAndAdjacency`) explicitly; the
   current parser accepts either but does not expose that diagnostic yet.
2. Confirm the character paths and PlayerUpgrade semantics against the real tree before
   implementing the dedicated character panel.
3. Build the item panel and a scratch-copy write-path test that changes one known field,
   reloads it structurally, and preserves a `.bak` backup.
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
