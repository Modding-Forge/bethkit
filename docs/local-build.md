# Local build setup

Bethkit can be built without Delphi or a local xEdit checkout. A normal workspace build contains no generated schema package. After Cargo has fetched the Rust dependencies once, that build does not need network access.

## Required for a normal Bethkit build

1. Install [Rust with rustup for 64-bit Windows](https://win.rustup.rs/x86_64).
2. Install [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) and select **Desktop development with C++** plus a Windows SDK.
3. Open a new PowerShell window and run:

   ```powershell
   rustup toolchain install stable --component rustfmt clippy
   cargo build --workspace --locked
   cargo test --workspace --locked
   cargo clippy --workspace --all-targets --locked -- -D warnings
   ```

If Cargo is installed but not visible in the current terminal, reopen the terminal or invoke it once as:

```powershell
& "$env:USERPROFILE\.cargo\bin\cargo.exe" build --workspace --locked
```

## Optional: build the Python package

Install [uv](https://docs.astral.sh/uv/getting-started/installation/) and [Python 3.10 or newer](https://www.python.org/downloads/windows/). In the `bethkit.py` repository run:

```powershell
uv sync --all-groups
uv run maturin develop
uv run ruff check .
uv run pyright
uv run pytest
```

The Python 2.0 migration uses Pydantic models. `uv sync` installs Ruff, Pydantic, Pyright, pytest, and the remaining declared dependencies.

## Schema assets

xEdit exporter development and schema generation live in [Modding-Forge/xDump](https://github.com/Modding-Forge/xDump). The definitions originate in the [xEdit project](https://github.com/TES5Edit/TES5Edit). Bethkit contains neither exporter source nor a build-time xEdit invocation.

To reproduce the Skyrim-enabled release build locally, download and verify the pinned package, export its path, and enable the FFI feature explicitly:

```powershell
$schema = .\scripts\Get-SkyrimSeSchema.ps1
$env:BETHKIT_SKYRIM_SE_SCHEMA = $schema
cargo build --release -p bethkit-ffi --features schema-skyrim-se,generate-header
```

The resulting FFI library embeds only Skyrim Special Edition. `BETHKIT_SCHEMA_BUNDLE` remains available for explicit local testing with an uncommitted catalog bundle. See [SCHEMA.md](../SCHEMA.md) for provenance and the update procedure.
