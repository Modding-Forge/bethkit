# bethkit

[![Build](https://img.shields.io/github/actions/workflow/status/Modding-Forge/bethkit/build.yml?branch=master&label=CI)](https://github.com/Modding-Forge/bethkit/actions/workflows/build.yml) [![License](https://img.shields.io/badge/license-Apache--2.0-blue)](LICENSE) [![Rust](https://img.shields.io/badge/rust-stable-orange?logo=rust)](https://www.rust-lang.org) [![Version](https://img.shields.io/badge/version-0.4.0-yellow)](CHANGELOG.md)

> **⚠️ Beta** — APIs may change before the first stable release.

A fast, zero-copy Rust library for reading and writing Bethesda game plugin and archive files. Callable from any language via a stable C ABI (`bethkit-ffi`).

## What it does

- **Zero-copy plugin parsing** — plugins are memory-mapped; records borrow bytes directly from the mapping without extra allocations
- **Versioned semantic schemas** — deterministic CBOR packages are generated
  from a pinned, provenance-checked xEdit exporter. Runtime builds never
  download or invoke xEdit.
- **BSA / BA2 archives** — read and extract all major formats (BSA TES3/TES4/FO3/SSE, BA2 GNRL/DX10); write new archives with parallel compression
- **Streaming record replace** — `PluginPatcher` rewrites arbitrary records in-place; cost is O(edits), not O(plugin size)
- **Writer** — build new plugins from scratch; eslify existing plugins; set the `LOCALIZED` flag
- **Localized strings** — read, edit, and write `.STRINGS` / `.DLSTRINGS` / `.ILSTRINGS` files; apply translation patches without touching the plugin binary
- **Load-order utilities** — `LoadOrder`, `GlobalFormId`, `PluginCache` for winning-override lookups and EditorID search across multiple plugins
- **C ABI** — `bethkit-ffi` exposes ~110 `extern "C"` functions with a pre-generated `bethkit.h` included in the repository

## Crates

- **`bethkit-io`** — memory-mapped I/O, `SliceCursor`, zlib/LZ4 decompression
- **`bethkit-core`** — schema-free ESP/ESL/ESM parser, writer, patcher, and string tables
- **`bethkit-schema`** — CBOR packages, catalogs, schema grammar, and bounded expression VM
- **`bethkit-semantic`** — semantic views, editing, validation, references, conflicts, and cleaning
- **`bethkit-bsa`** — BSA and BA2 archive reader and writer
- **`bethkit-ffi`** — C ABI wrapper and `bethkit.h` header

## Supported games

The public game model covers Skyrim LE/SE/VR, Fallout 3/NV/4/4VR/76,
Oblivion, Morrowind, and Starfield. Schema packages are embedded only after
all release gates pass for the corresponding game.

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
- [Local build setup](docs/local-build.md)

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
| Versioned CBOR schema runtime                       | ✅          |
| xEdit exporter patch and audited conversion rules  | 🚧 WIP      |
| Eleven-game differential corpus                     | 🚧 WIP      |
| C ABI v2 (`bethkit-ffi`)                            | 🚧 WIP      |
| Python 2.0 bindings — [bethkit.py](https://github.com/Modding-Forge/bethkit.py) | 🚧 WIP |

## License

Apache-2.0
