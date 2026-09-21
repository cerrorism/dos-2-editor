# dos-2-editor — development notes / handover

**Plan file** (the source of truth for scope, phasing, and the full DOS2:DE domain-knowledge
tables for Characters/Items): `C:\Users\cerro\.claude\plans\we-are-going-to-keen-canyon.md`
on the user's machine. Read it before doing anything else — this file is a status/handover
summary, not a replacement for it.

This is a personal/fun project for the user — no CI, no need for generality beyond
DOS2:DE, hardcoded assumptions are fine where they simplify things. Priority order per the
user: **item editing first**, character editing second.

## Git

The repository is on `main` and pushes to `git@github.com:cerrorism/dos-2-editor.git`.
`target/` and `*.lsv.bak*` are gitignored. Commit as you go.

## Status (format, safe edited-copy workflow, party inventory UI, and multi-pak localized names done)

**Implemented and passing `cargo test` (21/21) + clean `cargo clippy --all-targets`:**
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
- `src/app.rs`/`main.rs`/`config.rs`/`save_file.rs` — an egui editor with save-folder picker,
  save list, party inventory tabs, focused item editor, and an Advanced-only raw tree.
- `src/domain/item.rs` — thin typed item views/mutators for the verified Items hierarchy,
  including `Stats`, `Amount`, nested stats/rune/`PermanentBoost` values, and same-index
  Creator handles. `examples/dump_items.rs` prints the typed real-save summary (an optional
  item index prints its complete raw node for schema investigation).
- `src/app.rs` has an item-first panel grouped by the verified owner inventory handles of
  actual player characters. It resolves saved item names from the game English localization
  and Shared root templates where possible, with a readable Stat-ID fallback otherwise.
  The focused editor exposes existing Stats, Amount, Slot, nested Level/name indices, rune
  slots, and PermanentBoost fields. “Save Edited Copy” validates a complete PAK/LSF reparse
  and writes a separate non-clobbering sibling copy, never the selected original.
- `src/domain/character.rs` identifies player characters from `Stats.IsPlayer`, reads custom
  player name/origin fallback, and follows their inventory handles. The supplied save yields
  Fane (23), Ifan (20), 洛思/Lohse (65), and Beast (20) inventory items.
- `src/gamedata/localization.rs`/`catalog.rs` parse the selected `English.pak` or
  Simplified Chinese pack for UI text, **plus RootTemplates/generated-stats/item-progression
  data merged from four content paks** (`Shared.pak`, `SharedDOS.pak`, `Origins.pak`,
  `GameMaster.pak` — see design decision #8, this was the actual fix for most of the
  "items don't show a name" gap: the real DOS2:DE campaign content lives in `Origins.pak`,
  not `Shared.pak` alone). Catalogs now contain 92,210 English / 92,161 Simplified Chinese
  localization entries, 3,691 template names, 824 RootTemplate-`Stats`-keyed names, plus a
  separate `Stats.lsb`-derived table (see #9) covering most consumables/misc items that have
  neither a RootTemplate `DisplayName` nor a generated-loot namegroup entry.
- `src/format/lsb.rs` — a new, minimal **LSB** binary resource reader (FW3/DOS2:DE variant
  only; read-only, no writer needed). Uncompressed, much simpler than LSF: one flat static
  string dictionary, a region offset table, then a plain recursive node/attribute tree. Used
  to read `Localization/Stats.lsb` (a `TranslatedStringKeys` table mapping Stats-ID strings
  directly to a localization handle — the actual source of most consumable item names) and,
  going forward, could read `ItemProgression.lsb` properly instead of the byte-scanning hack
  in `progression_handles()` (not yet migrated — that hack still works and wasn't broken by
  this change, just now redundant infrastructure exists to replace it cleanly).
- `progression_group()`/`armor_class()`/`weapon_class()` in `catalog.rs` now map a generated
  item's real resolved stat fields (`ItemGroup` for armor, `WeaponType`+`IsTwoHanded`+`Slot`
  for weapons/shields) to the **exact, exhaustively-verified set of 63 real `RG_<Class>_
  <Rarity>` namegroup keys** (e.g. `RG_LightArmour_Rare`, `RG_Swords_2H_Epic`,
  `RG_Shields_Epic`) — found by dumping every real namegroup key from a real install, not
  guessed. Also handles "Unique"-rarity generated items (still procedurally boosted, but
  with one fixed name) via their own `ItemGroup` value as a direct namegroup key, which is
  how the game itself keys them (no `RG_..._Unique` group exists at all).
- `StatsCatalog::stat_label()` + `BOOST_LABELS` in `catalog.rs`: a known-key ->
  canonical-English-label table for `PermanentBoost` attributes / `Boost` ids / ability &
  talent ids (attributes, resistances, the ~23 school/ability ids, misc combat stats),
  localized once at load time (not per-frame) via a precomputed `label_translations` map.
  Wired into `app.rs`'s `permanent_boost_editor` via `boost_label()`, which shows
  `"<localized> (<raw key>)"` next to the still-editable raw attribute. **This is a
  reasonable approximation, not the authoritative source** — no `Stats.lsb`-equivalent
  table for ability/attribute/talent names was found during investigation (checked
  `Adjectives/Stats.lsb`, which is empty); the real UI presumably resolves these through
  some other mechanism not yet identified. See design decision #9's note on this.
- `examples/roundtrip_lsf.rs` / `examples/roundtrip_pak.rs` — non-mutating real-file
  round-trip verifiers.

**Investigated and found to already work correctly (no code change needed):** origin
companions' names (`character_name()` in `app.rs`, via `Character::origin_name()` +
`StatsCatalog::localized_text()`'s English-text-match trick) — verified against the real
save that `"Fane"`/`"Ifan"`/`"Beast"` each resolve to the correct Chinese name (费恩/伊凡/
比斯特). A hypothesis that this should instead go through the character's own `CurrentTemplate`
UUID (mirroring how items resolve names) was tried and disproven: **origin characters'
`CurrentTemplate`/`OriginalTemplate` UUIDs do not appear in any RootTemplates file at all**,
across all four merged content paks — confirmed by exhaustive search. Don't re-attempt this
without new evidence for where the character's real template identity actually lives.

**Still incomplete / known gaps:**
- A meaningful minority of `ARM_`/`WPN_` items (mostly plain `"Common"`-rarity crafted gear
  like `WPN_CraftedDagger`, `ARM_Conjurer_Helmet`) still don't resolve through any of the
  three name sources (RootTemplate `DisplayName`, generated namegroup, `Stats.lsb`) — these
  fall back to `friendly_stat_name()`'s derived text (e.g. "Crafted Dagger"), which is
  already a reasonable-looking name, just not confirmed to match the game's actual UI text.
  Not investigated further; diminishing returns given the dramatic overall improvement
  (verified via an ad hoc per-prefix resolution audit against the real save: most
  categories — potions, food, scrolls, grenades, tools, quest items, containers, unique
  items, generated armor — now resolve at or near 100%, up from many being 0%).
- Ability/attribute/talent name localization (`stat_label()`) is a hand-maintained
  approximation, not sourced from the game's own data — see the note above.
- Character editing, custom item name/description, tags, ownership changes, and new-item
  creation remain unimplemented. The item editor deliberately edits existing fields only.
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
8. **DOS2:DE's game data is split across several content paks, not just `Shared.pak`** —
   confirmed empirically with `dump_pak` against a real install. `Origins.pak` is the actual
   main-campaign content (`Public/DivinityOrigins_<guid>/...`, including most named/unique
   items); `GameMaster.pak` is GM-mode content; `SharedDOS.pak` is a small DOS2-specific
   supplement to Shared's engine-wide base. `catalog.rs`'s `CONTENT_PAKS` list merges
   RootTemplates/generated-stats/item-progression data from all four generically (by
   scanning each pak's entries for path patterns like `/RootTemplates/*.lsf`, not by
   hardcoding e.g. Origins' internal GUID folder name — that GUID is real and stable but
   there's no reason to depend on it). Only the per-language localization pak
   (`English.pak`/`Chinese.pak`) is a single file covering the whole game's text; that part
   was already correct. Patches (`Patch1.pak`..`Patch10.pak`) were checked exhaustively and
   found to carry only zero-length placeholder entries for every RootTemplates/Stats path —
   i.e. no real overrides — so they're intentionally not merged.
9. **Three independent, non-overlapping sources feed item display names**, discovered by
   inspecting real game files rather than guessing: (a) a RootTemplate's own `DisplayName`
   attribute, keyed by the item's `CurrentTemplate` UUID or its `Stats` id — mainly
   equipment/named items; (b) the generated-loot namegroup system
   (`ItemProgressionNames.txt` "new namegroup"/"add name" grammar, keyed by a `RG_<Class>_
   <Rarity>` string built from the item's real stat fields, or by the item's own `ItemGroup`
   value directly for "Unique"-rarity generated items) resolved to a handle via
   `ItemProgression.lsb`; (c) **`Localization/Stats.lsb`** (an LSB `TranslatedStringKeys`
   table, Stats-ID string -> handle) — this is where potions/food/scrolls/tools/grenades
   actually get their name, since they have neither (a) nor (b) (verified directly: e.g.
   `POTION_Minor_Healing_Potion`'s RootTemplate has no `DisplayName` attribute at all, and
   its resolved stat fields have no `ItemGroup` either). `StatsCatalog::display_name` tries
   (a) then (c) in that order; `generated_name` is (b) and is tried separately by callers as
   a fallback. No equivalent "internal id -> handle" table was found for ability/attribute/
   talent names (`Adjectives/Stats.lsb` — despite the promising name — is empty in a real
   install), so those still rely on the coarser `BOOST_LABELS` approximation.

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
  not a single `Data.pak`. **Superseded/expanded by design decision #8**: RootTemplates and
  generated-stats data is also split into `Origins.pak`/`GameMaster.pak`/`SharedDOS.pak`.
- Item-name resolution against the real save, before vs. after this session's catalog work
  (per-Stats-ID-prefix resolved/total, via an ad hoc audit script, not committed): most
  categories went from largely or entirely unresolved to fully resolved — e.g. `POTION`
  0/11 → 11/11, `SCROLL` 0/14 → 14/14, `FOOD` 0/7 → 7/7, `GRN` 0/25 → 25/25, `LOOT` 4/29 →
  29/29, `TOOL` 6/14 → 13/14, `FTJ` (unique generated items) 0/6 → 6/6, unnamed-prefix
  (creature "natural weapon" stat entries, not real inventory items) 0/177 → 148/177,
  `ARM` 63/119 → 98/119, `WPN` 17/49 → 41/49. See design decisions #8/#9 for what changed.

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

1. Manually load a newly saved edited copy in the game before trusting it for real play;
   format-level structural verification cannot prove game compatibility.
2. If the remaining `ARM`/`WPN` "Common"-rarity gap (see "Still incomplete") bothers you,
   the next thing to check is whether these items' names come from yet another source not
   yet found — `dump_lsf`/a fresh ad hoc search (like the ones used to find `Stats.lsb` and
   the real `RG_` namegroup keys this session) against a real `WPN_CraftedDagger`-style item
   is the way to approach it: find real data first, don't guess a mapping.
3. If you want ability/attribute/talent names to be authoritative rather than the
   `BOOST_LABELS` approximation, look for where the game's ability/skill-tree UI actually
   gets its text — `Stats.lsb`'s `TranslatedStringKeys` table was checked and doesn't have
   entries for these keys, and `Adjectives/Stats.lsb` is empty.
4. Consider migrating `progression_handles()`'s byte-scanning hack for `ItemProgression.lsb`
   to a real parse via the new `format::lsb` reader, now that it exists — same technique
   used successfully for `Stats.lsb` (look for a `TranslatedStringKeys` region, or whatever
   `dump`-style inspection reveals `ItemProgression.lsb`'s actual node schema to be).
5. Add focused character editing only after confirming the PlayerUpgrade paths and semantics
   against real saves. Keep item editing the priority.
6. Add custom item names/descriptions, tags, ownership changes, then (last) safe new-item
   creation with GUID/handle allocation.
