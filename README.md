# bethkit

> **⚠️ Work In Progress** — bethkit is under active development. APIs may change without notice before the first stable release.

A fast, zero-copy Rust library for reading and writing Bethesda game plugin and archive files.

## Features

- **Zero-copy parsing** - plugin files are memory-mapped; records reference bytes directly into the mapping without extra allocations
- **Multi-game record schemas** - `SchemaRegistry::sse()` covers all 126 SSE record types; `SchemaRegistry::fo4()` covers all 137 Fallout 4 record types (verified against 725 plugins / 938 MiB including all DLC ESMs); `GameContext` encodes binary differences for every other Bethesda game (Skyrim LE/VR, Fallout 3/NV/76, Starfield, Oblivion, Morrowind) — schema definitions for those games are work-in-progress
- **BSA / BA2 archives** - read, extract, and write all major archive formats: BSA TES3 (Morrowind), BSA TES4/FO3/SSE (Oblivion through Skyrim SE), BA2 GNRL and BA2 DX10 (Fallout 4); auto-detection via `bethkit_bsa::open`; concurrent extraction via `Archive::extract`; new archives created with `BsaWriter`, `Ba2GnrlWriter`, and `Ba2Dx10Writer`
- **Group / Record / SubRecord hierarchy** - full access to the GRUP structure, lazy subrecord parsing, XXXX large-data override, transparent zlib and LZ4 decompression
- **Localized strings** - read and write `.STRINGS` / `.DLSTRINGS` / `.ILSTRINGS` sibling files; extract every translatable string from a plugin in one call; apply translation edits without touching the plugin binary at all
- **Streaming rewrite** - `PluginPatcher` replaces arbitrary records while copying everything else verbatim; group sizes are recomputed automatically; cost is linear in the number of edits, not the plugin size
- **Writer** - build new plugin files from scratch; eslify existing plugins to fit the light-plugin FormID range; set the `LOCALIZED` flag for new localized plugins
- **Record schema** - `SchemaRegistry::sse()` (126 SSE types) and `SchemaRegistry::fo4()` (137 FO4 types); `RecordView` decodes any subrecord into typed `FieldValue` variants (integers, floats, ZStrings, FormIDs, structs, arrays, enums with resolved names, bit-flags with resolved names, localized string-table IDs) without allocating schema data at runtime — all definitions are `&'static`; absent or version-truncated fields yield `FieldValue::Missing` rather than an error — see [Record Schema](docs/modules/ROOT/pages/schema.adoc) for use cases and examples
- **Load-order utilities** - `LoadOrder`, `GlobalFormId`, and `PluginCache` for multi-plugin winning-override lookups; `GlobalFormId` identifies a record by owning plugin name + object ID, independent of the current load order so FormIDs remain stable across load-order changes
- **C ABI** - `bethkit-ffi` exposes a stable `extern "C"` surface so Python, C#, C++, and any other language can call in without a Rust toolchain

## Crate layout

| Crate            | Purpose                                                               | Status      |
| ---------------- | --------------------------------------------------------------------- | ----------- |
| `bethkit-io`   | Memory-mapped I/O,`SliceCursor`, zlib/LZ4 decompression             | ✅ Complete |
| `bethkit-core` | ESP/ESL/ESM parser, writer, string tables, patcher                    | ✅ Complete |
| `bethkit-bsa`  | BSA (TES3/TES4/FO3/SSE) and BA2 (GNRL/DX10) archive reader and writer | ✅ Complete |
| `bethkit-ffi`  | C ABI +`bethkit.h` header (via cbindgen)                            | ✅ Complete |

## Documentation

Full API documentation, guides, and examples are available in the [docs/](docs/) folder
(AsciiDoc / Antora):

- [Quick Start](docs/modules/ROOT/pages/quick-start.adoc)
- [Reading Plugins](docs/modules/ROOT/pages/reading-plugins.adoc)
- [Writing &amp; Patching Plugins](docs/modules/ROOT/pages/writing-plugins.adoc)
- [Load Order &amp; FormID Resolution](docs/modules/ROOT/pages/load-order.adoc)
- [Record Schema](docs/modules/ROOT/pages/schema.adoc)
- [Localized Strings](docs/modules/ROOT/pages/string-tables.adoc)
- [BSA / BA2 Archives](docs/modules/ROOT/pages/archives.adoc)
- [Architecture &amp; Crate Layout](docs/modules/ROOT/pages/architecture.adoc)

## Supported games

The primary targets are Skyrim Special Edition and Fallout 4. The binary format layer (parser, writer, patcher) works for all TES4-era games. Record-schema definitions are complete for SSE (126 types) and Fallout 4 (137 types); work-in-progress schema data exists for several other games in the repository but is not yet integrated.

| Variant                                   | Game                                        | Status                                             |
| ----------------------------------------- | ------------------------------------------- | -------------------------------------------------- |
| `Game::SkyrimSE`                        | The Elder Scrolls V: Skyrim Special Edition | ✅ Full support (parser, schema, writer, archives) |
| `Game::SkyrimLE`                        | Skyrim Legendary Edition                    | ⚠️ Format layer only — no record schema           |
| `Game::SkyrimVR`                        | Skyrim VR                                   | ⚠️ Format layer only — no record schema           |
| `Game::Fallout4` / `Game::Fallout4VR` | Fallout 4 / VR                              | ✅ Full support (parser, schema, writer, archives) |
| `Game::Starfield`                       | Starfield                                   | ⚠️ Format layer — schema WIP (not yet integrated)  |
| `Game::Oblivion`                        | The Elder Scrolls IV: Oblivion              | ⚠️ Format layer — schema WIP (not yet integrated)  |
| `Game::Fallout3`                        | Fallout 3                                   | ⚠️ Format layer — schema WIP (not yet integrated)  |
| `Game::FalloutNV`                       | Fallout: New Vegas                          | ⚠️ Format layer — schema WIP (not yet integrated)  |
| `Game::Morrowind`                       | The Elder Scrolls III: Morrowind            | ⚠️ Format layer — schema WIP (not yet integrated)  |

---

## Comparison

### What does each tool target?

|                            | **bethkit**         | **[sse-plugin-interface](https://github.com/Cutleast/sse-plugin-interface)** | **[xEdit (TES5Edit)](https://github.com/TES5Edit/TES5Edit)** | **[Mutagen](https://github.com/Mutagen-Modding/Mutagen)** |
| -------------------------- | ------------------------- | ------------------------------------------------------------------------------- | --------------------------------------------------------------- | ------------------------------------------------------------ |
| **Language**         | Rust (+ C ABI)            | Python                                                                          | Delphi / Pascal                                                 | C# (.NET 8/9)                                                |
| **Primary audience** | Library / tooling authors | SSE-Auto-Translator                                                             | Mod authors / power users                                       | C# tool & patcher authors                                    |
| **License**          | Apache-2.0                | MIT                                                                             | MPL 2.0                                                         | GPL-3.0                                                      |
| **Distribution**     | crates.io / source        | PyPI / source                                                                   | source / binary                                                 | NuGet                                                        |

---

### Parser capabilities

| Capability                                         | **bethkit**                                                                                                                          | **[sse-plugin-interface](https://github.com/Cutleast/sse-plugin-interface)** | **[xEdit](https://github.com/TES5Edit/TES5Edit)** | **[Mutagen](https://github.com/Mutagen-Modding/Mutagen)** |
| -------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------- | ---------------------------------------------------- | ------------------------------------------------------------ |
| Open / memory-map plugin                           | ✅                                                                                                                                         | ✅                                                                              | ✅                                                   | ✅ (binary overlay)                                          |
| Group hierarchy traversal                          | ✅                                                                                                                                         | ✅ (internal)                                                                   | ✅                                                   | ✅                                                           |
| Access subrecord by signature                      | ✅                                                                                                                                         | ✅ (internal)                                                                   | ✅                                                   | ⚠️ low-level API only                                      |
| Typed subrecord accessors (u8/u16/u32/f32/zstring) | ✅ (raw bytes + basic types)                                                                                                               | ❌                                                                              | ✅ (full schema)                                     | ✅ (strongly typed per record type)                          |
| Schema-defined field names & types                 | ✅ (`SchemaRegistry::sse()` 126 SSE types; `SchemaRegistry::fo4()` 137 FO4 types)                                                   | ❌                                                              | ✅ (wbDefinitions*.pas per game)                     | ✅ (code-generated per record type)                        |
| Enum / flag resolution (`ActorValue` → name)    | ✅ (`FieldValue::Enum` / `FieldValue::Flags`)                                                                                          | ❌                                                                              | ✅                                                   | ✅                                                           |
| FormLink type-safe cross-record links              | ✅ runtime, schema-driven —`FieldType::FormIdTyped` + `FieldValue::FormIdTyped { raw, allowed }` with `&'static` target-type slices | ❌                                                                              | ⚠️ (`wbFormIDCk` runtime check on write only)    | ✅ (`IFormLink<T>` compile-time typed)                     |
| FormID → EditorID resolution across masters       | ✅ (`PluginCache` + `find_by_editor_id`)                                                                                               | ❌                                                                              | ✅ (full reference graph)                            | ✅ (LinkCache)                                               |
| `&'static` zero-alloc schema definitions         | ✅ all definitions are `&'static`; zero heap allocation for schema at runtime                                                            | ❌                                                                              | ❌ (Delphi objects loaded at startup)                | ❌ (code-gen'd CLR objects)                                  |
| Graceful absent / truncated fields                 | ✅`FieldValue::Missing` - never an error on absent or version-truncated fields                                                           | N/A                                                                             | ⚠️ (can raise exceptions)                          | ⚠️ (exceptions on malformed data)                          |
| Winning override lookup across load order          | ✅ (`PluginCache::records_of_type`)                                                                                                      | ❌                                                                              | ✅                                                   | ✅ (`.WinningOverrides()`)                                 |
| Compressed record (zlib / LZ4)                     | ✅                                                                                                                                         | ✅                                                                              | ✅                                                   | ✅                                                           |
| Large subrecord (XXXX override)                    | ✅                                                                                                                                         | ✅                                                                              | ✅                                                   | ✅                                                           |
| Localized strings (.STRINGS files)                 | ✅                                                                                                                                         | ✅ (via string-table parser)                                                    | ✅                                                   | ✅ (`TranslatedString`, lazy per-language)                 |
| BSA / BA2 archives (read + write)                  | ✅ read + write (`BsaWriter`, `Ba2GnrlWriter`, `Ba2Dx10Writer`)                                                                      | ❌                                                                              | ✅ read + write (`wbBSA.pas` / BSArch)             | ⚠️ read only                                              |
| Concurrent archive extraction                      | ✅ (`rayon`-parallel per file)                                                                                                           | ❌                                                                              | ❌                                                   | ❌                                                           |
| Save game files (.ess / .fos)                      | ❌                                                                                                                                         | ❌                                                                              | ✅                                                   | ❌                                                           |

---

### Multi-game support

| Game                              | bethkit    | [sse-plugin-interface](https://github.com/Cutleast/sse-plugin-interface) | [xEdit](https://github.com/TES5Edit/TES5Edit) | [Mutagen](https://github.com/Mutagen-Modding/Mutagen) |
| --------------------------------- | ---------- | --------------------------------------------------------------------- | ------------------------------------------ | -------------------------------------------------- |
| Skyrim SE / AE                    | ✅ Full    | SSE only                                                              | ✅                                         | ✅                                                 |
| Skyrim LE / VR                    | ⚠️ Format layer only | ❌                                                                    | ✅                                         | ✅                                                 |
| Fallout 4 / VR                    | ✅ Full (parser, schema 137 types, writer, archives) | ❌                                                                    | ✅                                         | ✅                                                 |
| Starfield                         | ⚠️ Schema WIP | ❌                                                                    | ✅                                         | ✅                                                 |
| Oblivion                          | ⚠️ Schema WIP | ❌                                                                    | ✅                                         | ✅                                                 |
| Fallout 3 / New Vegas / Morrowind | ⚠️ Schema WIP | ❌                                                                    | ✅                                         | ❌                                                 |

> **Note (bethkit):** The `GameContext` type encodes game-specific binary differences (header signature, light-flag bit, HEDR version, GRUP type set, etc.) for all games above. Record-schema definitions are complete for SSE (126 types) and Fallout 4 (137 types, verified against a 725-plugin / 938 MiB install including all DLC ESMs). Work-in-progress schema data exists in the repository for Fallout 3, Fallout NV, Starfield, Oblivion, and Morrowind, but is not yet integrated.
>
> **Note (Mutagen):** Code-generated record schemas exist for Skyrim, Fallout 4, Oblivion, and Starfield. Other games are not supported.

---

### Write capabilities

| Capability                                      | bethkit                                    | [sse-plugin-interface](https://github.com/Cutleast/sse-plugin-interface) | [xEdit](https://github.com/TES5Edit/TES5Edit) | [Mutagen](https://github.com/Mutagen-Modding/Mutagen) |
| ----------------------------------------------- | ------------------------------------------ | --------------------------------------------------------------------- | ------------------------------------------ | -------------------------------------------------- |
| Build new plugin from scratch                   | ✅ (`PluginWriter`)                      | ❌                                                                    | ✅                                         | ✅ (`new SkyrimMod(...)`)                        |
| Streaming record replace                        | ✅ (`PluginPatcher`)                     | ❌                                                                    | ✅                                         | ❌ (full mod re-serialized)                        |
| Translate localized strings (no plugin rewrite) | ✅ (`LocalizationSet` + `apply_edits`) | ✅ (`replace_strings`)                                              | ✅                                         | ✅ (`StringsWriter` + `TranslatedString`)      |
| ESLify / compaction (shrink FormID range)       | ✅ (`eslify()`)                          | ❌                                                                    | ✅ (via script)                            | ✅ (`ModCompaction`)                             |
| Set `LOCALIZED` flag on new plugins           | ✅ (`set_localized`)                     | ❌                                                                    | ✅                                         | ✅ (header flag)                                   |
| FormLink remapping (batch)                      | ❌                                         | ❌                                                                    | ✅                                         | ✅ (`RemapLinks`)                                |
| Record duplication with new FormKey             | ❌                                         | ❌                                                                    | ✅                                         | ✅ (`DuplicateInAsNewRecord`)                    |
| Conflict detection                              | ❌                                         | ❌                                                                    | ✅                                         | ❌                                                 |
| Master management                               | ✅ (writer)                                | ❌                                                                    | ✅                                         | ✅ (auto-computed during export)                   |

---

### Integration / embedding

| Capability                     | bethkit       | [sse-plugin-interface](https://github.com/Cutleast/sse-plugin-interface) | [xEdit](https://github.com/TES5Edit/TES5Edit) | [Mutagen](https://github.com/Mutagen-Modding/Mutagen) |
| ------------------------------ | ------------- | --------------------------------------------------------------------- | ------------------------------------------ | -------------------------------------------------- |
| Use as library (no subprocess) | ✅ Rust crate | ✅ Python package                                                     | ❌ GUI app / CLI only                      | ✅ C# NuGet package                                |
| C ABI for cross-language use   | ✅            | ❌                                                                    | ❌                                         | ❌                                                 |
| Patcher pipeline framework     | ❌            | ❌                                                                    | ✅ (Pascal scripts)                        | ✅ (Synthesis)                                     |

---

### Performance design

|                     | bethkit                                | [sse-plugin-interface](https://github.com/Cutleast/sse-plugin-interface) | [xEdit](https://github.com/TES5Edit/TES5Edit) | [Mutagen](https://github.com/Mutagen-Modding/Mutagen) |
| ------------------- | -------------------------------------- | --------------------------------------------------------------------- | ------------------------------------------ | -------------------------------------------------- |
| File access         | `memmap2` (zero-copy)                | Python `io.BytesIO` stream                                          | Delphi stream + cache                      | Lazy binary overlay                                |
| Subrecord parsing   | Lazy (`OnceLock`)                    | Eager                                                                 | Eager                                      | Lazy (overlay) / Eager (mutable class)             |
| Parallel processing | `rayon` available                    | CPython GIL                                                           | Delphi threads                             | .NET TPL / PLINQ                                   |
| Allocation strategy | `Arc<[u8]>` slices into map          | Python bytes copies                                                   | Delphi heap                                | Code-gen'd heap objects                            |
| Record replace cost | O(edits) — unmodified groups verbatim | N/A                                                                   | O(plugin size)                             | O(plugin size) — full re-serialization            |

---

## Design philosophy

- **[sse-plugin-interface](https://github.com/Cutleast/sse-plugin-interface)** is minimal by design — it only does what SSE-Auto-Translator needs (string extraction and injection for one game). It is not a general-purpose library.
- **[Mutagen](https://github.com/Mutagen-Modding/Mutagen)** is the C# ecosystem's answer to a general-purpose modding library. Its standout feature is a fully code-generated, strongly typed schema for each supported game's records (`Npc.Name`, `Weapon.BasicData.Damage`, etc.), making it ideal for C# patcher authors. The Synthesis framework layers a full patcher pipeline on top. Trade-offs: GPL-3.0 licensing, C#-only integration, no streaming writes, no support for Fallout 3/NV/76/Morrowind.
- **[xEdit (TES5Edit)](https://github.com/TES5Edit/TES5Edit)** is the authoritative reference implementation. It has schema definitions for every record and field across every game, conflict detection, reference graphs, a scripting engine, and a GUI. It is a complete modding tool, not a library.
- **bethkit** occupies a different niche: a fast, embeddable, multi-language *library* that gives direct access to the binary structure. Declarative record schemas cover all 126 SSE record types (`SchemaRegistry::sse()`) and all 137 Fallout 4 record types (`SchemaRegistry::fo4()`). The `RecordView` API resolves fields, enums, flags, structs, and arrays — all backed by `&'static` data with zero runtime heap allocation for definitions. The Apache-2.0 license and C ABI make it callable from any language and embeddable in any project without license restrictions.

## Development status

| Milestone                                                            | Status      |
| -------------------------------------------------------------------- | ----------- |
| Parser + writer + tests (SSE)                                        | ✅ Complete |
| String tables (`.STRINGS` / `.DLSTRINGS` / `.ILSTRINGS`)       | ✅ Complete |
| Streaming rewrite (`PluginPatcher`)                                | ✅ Complete |
| Localized-string extraction + translation workflow                   | ✅ Complete |
| BSA / BA2 archive reader                                             | ✅ Complete |
| FormID resolver +`PluginCache` (winning override, EditorID lookup) | ✅ Complete |
| Record schema — 126 SSE record types, `RecordView` API             | ✅ Complete |
| Record schema — 137 Fallout 4 record types, `SchemaRegistry::fo4()` | ✅ Complete |
| C ABI (`bethkit-ffi`)                                              | ✅ Complete |
| Python bindings (`bethkit.py`, via C ABI)                          | ⚠️ WIP    |

## License

Apache-2.0 — see [SPDX](https://spdx.org/licenses/Apache-2.0.html) for the full text.
