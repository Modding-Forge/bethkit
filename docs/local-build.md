# Local build setup

Bethkit itself can be built without Delphi, xEdit, generated schemas, or
network access after Cargo has fetched the Rust dependencies once.

## Required for a normal Bethkit build

1. Install [Rust with rustup for 64-bit Windows](https://win.rustup.rs/x86_64).
2. Install [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)
   and select **Desktop development with C++** plus a Windows SDK.
3. Open a new PowerShell window and run:

   ```powershell
   rustup toolchain install stable --component rustfmt clippy
   cargo build --workspace --locked
   cargo test --workspace --locked
   cargo clippy --workspace --all-targets --locked -- -D warnings
   ```

If Cargo is installed but not visible in the current terminal, reopen the
terminal or invoke it once as:

```powershell
& "$env:USERPROFILE\.cargo\bin\cargo.exe" build --workspace --locked
```

## Optional: build the Python package

Install [uv](https://docs.astral.sh/uv/getting-started/installation/) and
[Python 3.10 or newer](https://www.python.org/downloads/windows/). In the
`bethkit.py` repository run:

```powershell
uv sync --all-groups
uv run maturin develop
uv run ruff check .
uv run pyright
uv run pytest
```

The Python 2.0 migration uses Pydantic models. `uv sync` installs Ruff,
Pydantic, Pyright, pytest, and the remaining declared dependencies.

## Optional: build an xEdit schema exporter

This is only required to regenerate official `.bkschema` files. The ordinary
Bethkit build never invokes or downloads xEdit.

Install:

- A licensed [Delphi 12](https://www.embarcadero.com/products/delphi)
  Professional or Enterprise installation for automated command-line builds.
  Delphi Community Edition can build `xDump.dproj` in the IDE, but its license
  disables the `dcc32`/MSBuild command-line compiler.
- [Project Magician](https://www.uweraabe.de/Blog/downloads/download-info/project-magician/).
- [DDevExtensions](https://github.com/DelphiPraxis/DDevExtensions/releases).
- The additional libraries and setup listed in the
  [official xEdit developer documentation](https://github.com/TES5Edit/TES5Edit#developer-documentation).

The local TES5Edit clone is expected at `..\TES5Edit`. The build helper creates
and reuses an isolated worktree under `target\xedit-source`, leaving changes in
the main clone untouched. The current lock is
`xedit-4.1.5f` at
`f5c00f3fa3ee39511185515802647246c807f759`.

Build and provenance-check the exporter with:

```powershell
.\scripts\Build-XEditExporter.ps1
```

With Community Edition, prepare and stamp the worktree without invoking the
disabled command-line compiler:

```powershell
.\scripts\Build-XEditExporter.ps1 -PrepareOnly
```

Open
the generated `project` path printed by the helper (normally
`target\xedit-source\xDump.Release.dproj`) in Delphi. It has an isolated project
identity and is pinned to **Release** and **Win32**, preventing Delphi from
restoring a stale Debug selection. Build the project and finish artifact
verification with:

```powershell
.\scripts\Build-XEditExporter.ps1 -UseExistingIdeBuild
```

The verification command prints the executable, detailed MAP, patch-set, and
build hashes. Use those values to export every game twice and create a callback
inventory:

```powershell
.\scripts\Export-XEditDefinitions.ps1 `
  -Exporter .\target\xedit-exporter\bethkit-xedit-exporter.exe `
  -ExpectedExporterSha256 <exporter hash> `
  -ExpectedMapSha256 <MAP hash> `
  -ExpectedPatchSha256 <patch-set hash> `
  -ExpectedBuildSha256 <build hash>
```

The export helper also writes `callback-audit.json`. It groups callbacks by
their resolved Delphi implementation fingerprint and accounts for every
callback/game binding as an exact UI rule, an implementation rule, a binding
derived from a Custom schema node, or an unclassified release blocker.

The provenance and definition-graph operations are implemented. Exported
dynamic callbacks intentionally fail conversion until matching audited rules
and custom decoders are present, so release-quality schema regeneration remains
blocked at that gate. The callback inventory also lists every custom decoder
path and the games that require it. Refresh the exact UI-only rules with:

```powershell
cargo run -p bethkit-schema --bin bethkit-xedit-converter -- classify-ui `
  .\target\xedit-definitions\callback-inventory.json `
  .\xedit\conversion-rules.json `
  .\xedit\conversion-rules.json
```

This command only adds exact-path rules for callbacks that xEdit exposes as
presentation-only metadata. It does not classify semantic callbacks and refuses
to overwrite a conflicting reviewed rule.

Every classified callback is expanded into an exact-path package binding to a
bounded expression, stable built-in operation, versioned semantic handler,
payload decoder, or UI-only marker. Semantic implementation rules are guarded
by an expected per-game match count and path-set SHA-256. Package validation
rejects manifest counts or runtime requirements that do not match the concrete
bindings.

Generate and inspect all eleven candidate packages with:

```powershell
.\scripts\New-XEditSchemaCatalog.ps1 `
  -Exporter C:\path\to\bethkit-xedit-exporter.exe `
  -ExpectedExporterSha256 <64-hex-digest> `
  -ExpectedMapSha256 <64-hex-digest> `
  -ExpectedPatchSha256 <64-hex-digest> `
  -ExpectedBuildSha256 <64-hex-digest>
```

To embed the resulting catalog in a release build:

```powershell
$env:BETHKIT_SCHEMA_BUNDLE = (
  Resolve-Path .\target\schemas\bethkit.bkschemas
)
cargo build --release -p bethkit-ffi --locked
```
