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

## Schema assets

xEdit exporter development and schema generation live in the separate sibling
`xDump` workspace. Bethkit neither contains nor invokes those tools.

Once all eleven packages pass xDump's release gates and have been reviewed,
copy the approved files manually to `schemas/embedded`. A normal Bethkit build
automatically embeds `schemas/embedded/bethkit.bkschemas` when that fixed
catalog exists. `BETHKIT_SCHEMA_BUNDLE` remains available for explicit local
testing with an uncommitted catalog.
