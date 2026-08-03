# Skyrim Special Edition schema

Bethkit release builds currently embed one semantic schema package for Skyrim Special Edition. No Skyrim Legendary Edition, Skyrim VR, Fallout, Oblivion, Morrowind, or Starfield schema is included.

The package derives its record structures and callback metadata from the [xEdit project](https://github.com/TES5Edit/TES5Edit). [Modding-Forge/xDump](https://github.com/Modding-Forge/xDump) pins the upstream source, builds the exporter, audits the conversion rules, validates the generated package, and publishes it independently from Bethkit. Bethkit does not claim independent authorship of xEdit's schema coverage.

## Distribution

Generated schema binaries are not stored in the Bethkit repository. The pinned source for Bethkit 0.4.0 is:

| Property | Value |
| --- | --- |
| xDump release | [`v0.1.0`](https://github.com/Modding-Forge/xDump/releases/tag/v0.1.0) |
| Package | [`skyrim_se.bkschema`](https://github.com/Modding-Forge/xDump/releases/download/v0.1.0/skyrim_se.bkschema) |
| SHA-256 | `8066dd6c64fe61583bafe0f8e57c6d78bbaf4d84d41a427fe98d383e976ee092` |
| Package version | `0.1.0` |
| Validation status | `candidate` |

The machine-readable pin is stored in [`schemas/releases/skyrim_se.json`](schemas/releases/skyrim_se.json).

Ordinary local workspace builds contain no schema data. The release workflow downloads the pinned package, verifies its complete file hash, enables `schema-skyrim-se`, and passes the verified path through `BETHKIT_SKYRIM_SE_SCHEMA`. The resulting FFI library embeds only that package. The standalone `.bkschema` file is also included in each platform archive so applications can load it directly without loading a multi-game catalog.

## Manual release-equivalent build

PowerShell users can reproduce the schema acquisition and native build locally:

```powershell
$schema = .\scripts\Get-SkyrimSeSchema.ps1
$env:BETHKIT_SKYRIM_SE_SCHEMA = $schema
cargo build --release -p bethkit-ffi --features schema-skyrim-se,generate-header
```

The downloader accepts only the pinned `Modding-Forge/xDump` release URL and removes a downloaded file when its SHA-256 value does not match. Cargo never downloads schemas itself.

## Provenance

| Component | Value |
| --- | --- |
| xEdit version | `4.1.5f` |
| xEdit source tag | `xedit-4.1.5f` |
| xEdit source commit | `f5c00f3fa3ee39511185515802647246c807f759` |
| xEdit source archive SHA-256 | `8c89ff9822375cdd0b5183ecb0cda04ae6529a9e8bda7ea78426a43bac13d3e8` |
| xDump exporter version | `2` |
| Exporter executable SHA-256 | `ad13a6ffc05c02938c78d33ff04cdba5865eda895d97d30c84a5799f895ba547` |
| Exporter map SHA-256 | `fd1a94922315630f2a87429000b212f952ed778d3b057f0e0330895da7d0280c` |
| Exporter patch SHA-256 | `e130b3765fa7713383dd53d775cfbb7a4989b28c05d62ed751a85811ad17fd63` |
| Exporter build SHA-256 | `7feeef896a17ceead9bac1af4ee45932ad171e945dac8e69a2a13d9f9be56452` |

## Definition coverage

The exported Skyrim Special Edition package contains:

- 134 record-type definitions;
- 2,581 callback bindings, of which 2,387 are semantic and 194 are presentation-only;
- 80 declarative callback bindings represented directly in schema grammar or expressions;
- three schema paths requiring the `xedit.dtinteger@1` decoder;
- 402 condition-function definitions covering the exported xEdit table through index 1028.

Callback classification is complete for this export: every callback/game binding in the xDump audit is classified, and no conversion rule is unused. These counts describe definition coverage, not proof that every possible field value or callback branch has appeared in a test plugin.

## Runtime validation coverage

The package was compared with the provenance-matched xDump exporter against the complete installed Skyrim Special Edition load order used for the xDump release:

| Measure                         |            Result |
| ------------------------------- | ----------------: |
| Plugin cases                    | 117 of 117 passed |
| Semantically validated records  |             7,828 |
| Semantic errors                 |                 0 |
| Non-fatal warnings              |               314 |
| Byte-identical no-op roundtrips |        117 of 117 |

Existing plugins are checked with Bethkit's xEdit-compatible validation mode. Missing fields that xEdit materializes while loading remain visible as warnings, while strict validation continues to report them as errors. No compatibility warning changes the source bytes, and every tested no-op write remained byte-identical.

## Current limits

The differential corpus covers record discovery, stable record metadata, schema-guided decoding, structural validation, required fields, byte coverage, and lossless no-op writing. It does not yet provide exhaustive differential coverage for every formatter branch, link resolver, mutation, cleaning operation, malformed input, or possible record value. The package therefore remains marked as `candidate` even though the measured Skyrim corpus passed.

Schema-independent parsing and archive APIs may support additional games, but callers must provide a compatible package before using semantic APIs for those games.

## Updating the pin

Schema updates are prepared and reviewed in [Modding-Forge/xDump](https://github.com/Modding-Forge/xDump). To update Bethkit:

1. publish the reviewed game package in an immutable xDump release;
2. update the release URL and SHA-256 value in `schemas/releases/skyrim_se.json`;
3. update the provenance and coverage measurements in this document;
4. run the downloader and the complete Bethkit test suite with `schema-skyrim-se` enabled;
5. confirm that no generated `.bkschema` file is staged in Git.
