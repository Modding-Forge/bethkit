# Schema release metadata

Bethkit does not store generated schema packages in Git. Files in `releases/` pin individual game packages published by [Modding-Forge/xDump](https://github.com/Modding-Forge/xDump) and record the checksum required before a package may be embedded in a release build.

The default Rust workspace build remains schema-free. The release workflow downloads only the selected game package, verifies it against the pinned SHA-256 value, and passes its local path to the corresponding Cargo feature.

See [SCHEMA.md](../SCHEMA.md) for provenance, measured coverage, and update instructions.
