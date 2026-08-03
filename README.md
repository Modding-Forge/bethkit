# bethkit

[![Build](https://img.shields.io/github/actions/workflow/status/Modding-Forge/bethkit/build.yml?branch=master&label=CI)](https://github.com/Modding-Forge/bethkit/actions/workflows/build.yml) [![License](https://img.shields.io/badge/license-Apache--2.0-blue)](LICENSE) [![Rust](https://img.shields.io/badge/rust-stable-orange?logo=rust)](https://www.rust-lang.org)

Bethkit is a Rust workspace for reading, writing, and inspecting Bethesda plugin and archive formats. The project is in active development, and its public APIs may still change before a stable release.

Skyrim Special Edition is the current schema-backed target. The lower-level format APIs also model other Bethesda games, but Bethkit release builds currently include only the Skyrim Special Edition schema.

## Components

- `bethkit-io` provides bounded cursors, memory-mapped input, and compression support.
- `bethkit-core` provides schema-independent plugin parsing, writing, patching, string tables, load-order handling, and record caches.
- `bethkit-schema` loads versioned schema packages and optional build-time catalogs.
- `bethkit-semantic` provides schema-guided views, editing, validation, references, conflicts, and cleaning operations.
- `bethkit-bsa` reads and writes supported BSA and BA2 archive variants.
- `bethkit-ffi` exposes the Rust functionality through a C ABI and the checked-in `bethkit.h` header.

## Skyrim Special Edition schema

Bethkit does not store generated schema packages in Git. The release workflow downloads only `skyrim_se.bkschema` from [xDump v0.1.0](https://github.com/Modding-Forge/xDump/releases/tag/v0.1.0), verifies its pinned SHA-256 value, embeds it in the release FFI libraries, and includes the standalone package in each platform archive. Ordinary local workspace builds remain schema-free unless the Skyrim schema feature and a verified package path are supplied explicitly.

The definitions originate in the [xEdit project](https://github.com/TES5Edit/TES5Edit). [Modding-Forge/xDump](https://github.com/Modding-Forge/xDump) pins the upstream xEdit source, builds the exporter, audits the conversion rules, validates the result, and publishes the package consumed by Bethkit.

See [SCHEMA.md](SCHEMA.md) for provenance, measured coverage, validation results, and current limitations.

## Documentation

- [Schema provenance and coverage](SCHEMA.md)
- [Local build setup](docs/local-build.md)
- Generate the Rust API reference locally with `cargo doc --workspace --no-deps`.
- The C ABI is declared in [`crates/bethkit-ffi/bethkit.h`](crates/bethkit-ffi/bethkit.h).

Python bindings are maintained separately in [Modding-Forge/bethkit.py](https://github.com/Modding-Forge/bethkit.py).

## Related projects

The projects below solve overlapping problems with different APIs and tradeoffs. This table is intended as orientation, not as a ranking.

| Project | Primary form | Schema approach | Practical focus |
| --- | --- | --- | --- |
| Bethkit | Rust crates and C ABI | Runtime schema packages; release builds currently embed Skyrim Special Edition only | Embeddable plugin, archive, and semantic operations |
| [xEdit](https://github.com/TES5Edit/TES5Edit) | Delphi desktop application and command-line modes | Upstream definitions covering Bethesda record formats and editor behavior | Interactive inspection, conflict analysis, cleaning, editing, and reference behavior |
| [Mutagen](https://github.com/Mutagen-Modding/Mutagen) | .NET libraries | Generated, statically typed record APIs | C# mod inspection and patcher development |
| [sse-plugin-interface](https://github.com/Cutleast/sse-plugin-interface) | Python package | Skyrim Special Edition-oriented Python model | Lightweight Python access for Skyrim plugin workflows |

Bethkit does not replace xEdit as a reference implementation or interactive modding tool. Its current schema-backed Skyrim support exists because xEdit's definitions can be exported and consumed by a smaller embeddable runtime.

## Build and test

Bethkit targets stable Rust and edition 2021.

```text
cargo build --workspace
cargo lint
cargo test --workspace
```

Tests that require installed game data skip automatically when their local fixtures are unavailable. See [local build setup](docs/local-build.md) for the optional live-test configuration.

## License

Apache-2.0
