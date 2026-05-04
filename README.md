# bethkit

[![Build](https://img.shields.io/github/actions/workflow/status/Modding-Forge/bethkit/build.yml?branch=master&label=CI)](https://github.com/Modding-Forge/bethkit/actions/workflows/build.yml) [![License](https://img.shields.io/badge/license-Apache--2.0-blue)](LICENSE) [![Rust](https://img.shields.io/badge/rust-stable-orange?logo=rust)](https://www.rust-lang.org) [![Version](https://img.shields.io/badge/version-0.3.1-yellow)](CHANGELOG.md)

> **⚠️ Beta** — APIs may change before the first stable release.

A fast, zero-copy Rust library for reading and writing Bethesda game plugin and archive files. Callable from any language via a stable C ABI (`bethkit-ffi`).

## What it does

- **Zero-copy plugin parsing** — plugins are memory-mapped; records borrow bytes directly from the mapping without extra allocations
- **Record schema** — `SchemaRegistry::sse()` covers all 126 SSE record types; `SchemaRegistry::fo4()` covers all 137 FO4 types. `RecordView` decodes subrecords into typed `FieldValue` variants: integers, floats, FormIDs, enums with resolved names, bit-flags, structs, and arrays. All schema data is `&'static` — zero heap allocation at runtime. Placement records (REFR, ACHR) are not schema-covered; complex multi-variant fields fall back to a raw byte slice.
- **BSA / BA2 archives** — read and extract all major formats (BSA TES3/TES4/FO3/SSE, BA2 GNRL/DX10); write new archives with parallel compression
- **Streaming record replace** — `PluginPatcher` rewrites arbitrary records in-place; cost is O(edits), not O(plugin size)
- **Writer** — build new plugins from scratch; eslify existing plugins; set the `LOCALIZED` flag
- **Localized strings** — read, edit, and write `.STRINGS` / `.DLSTRINGS` / `.ILSTRINGS` files; apply translation patches without touching the plugin binary
- **Load-order utilities** — `LoadOrder`, `GlobalFormId`, `PluginCache` for winning-override lookups and EditorID search across multiple plugins
- **C ABI** — `bethkit-ffi` exposes ~110 `extern "C"` functions with a pre-generated `bethkit.h` included in the repository

## Crates

- **`bethkit-io`** — memory-mapped I/O, `SliceCursor`, zlib/LZ4 decompression
- **`bethkit-core`** — ESP/ESL/ESM parser, writer, patcher, string tables, record schema
- **`bethkit-bsa`** — BSA and BA2 archive reader and writer
- **`bethkit-ffi`** — C ABI wrapper and `bethkit.h` header

## Supported games

The parser, writer, and patcher work for all TES4-era games. The current focus is completing full schema coverage for **Skyrim SE** (126 types) and **Fallout 4** (137 types). Schemas for other games (Oblivion, Fallout 3/NV, Starfield, Morrowind) are planned for later milestones.

## Documentation

- [Quick Start](docs/modules/ROOT/pages/quick-start.adoc)
- [Reading Plugins](docs/modules/ROOT/pages/reading-plugins.adoc)
- [Writing &amp; Patching Plugins](docs/modules/ROOT/pages/writing-plugins.adoc)
- [BSA / BA2 Archives](docs/modules/ROOT/pages/archives.adoc)
- [Record Schema](docs/modules/ROOT/pages/schema.adoc)
- [Localized Strings](docs/modules/ROOT/pages/string-tables.adoc)
- [Load Order &amp; FormID Resolution](docs/modules/ROOT/pages/load-order.adoc)
- [C ABI / Language Bindings](docs/modules/ROOT/pages/language-bindings.adoc)
- [Architecture](docs/modules/ROOT/pages/architecture.adoc)

## Compared to alternatives

|                              | bethkit      | sse-plugin-interface | xEdit      | Mutagen         |
| ---------------------------- | ------------ | -------------------- | ---------- | --------------- |
| Language                     | Rust + C ABI | Python               | Delphi     | C#              |
| License                      | Apache-2.0   | MIT                  | MPL 2.0    | GPL-3.0         |
| Embeddable library           | ✅           | ✅                   | ❌ GUI/CLI | ✅              |
| Schema-typed record access   | ✅ runtime   | ❌                   | ✅ full    | ✅ compile-time |
| BSA / BA2 write              | ✅           | ❌                   | ✅         | read only       |
| Streaming record replace     | ✅           | ❌                   | ✅         | ❌              |
| Conflict detection           | ❌           | ❌                   | ✅         | ❌              |
| C ABI for cross-language use | ✅           | ❌                   | ❌         | ❌              |

bethkit's niche is a fast, embeddable, language-agnostic library for direct binary access. **xEdit** is the authoritative reference tool with full conflict detection, a GUI, and schema definitions for every field across every game. **Mutagen** offers compile-time-typed record schemas for C# patcher authors, with the Synthesis framework on top; its GPL-3.0 licence restricts embedding in proprietary tools. **sse-plugin-interface** is minimal by design — purpose-built for SSE-Auto-Translator.

## Status

| Milestone                                           | Status      |
| --------------------------------------------------- | ----------- |
| Parser + writer + tests (SSE)                       | ✅          |
| String tables                                       | ✅          |
| Streaming rewrite (`PluginPatcher`)                 | ✅          |
| BSA / BA2 reader + writer                           | ✅          |
| `PluginCache` (winning override, EditorID lookup)   | ✅          |
| Record schema — SSE (126 types registered, deep coverage WIP) | ⚠️ WIP     |
| Record schema — FO4 (137 types registered, deep coverage WIP) | ⚠️ WIP     |
| C ABI (`bethkit-ffi`)                               | ✅          |
| Python bindings (`bethkit.py`)                      | ⚠️ WIP     |
| Record schema — other games                         | 🗓️ Planned |

## License

Apache-2.0
