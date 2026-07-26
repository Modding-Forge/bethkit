# xEdit exporter patch set

This directory is reserved for the MPL-2.0 patch series that adds definition
graph introspection to `xDump`. The exporter must initialize xEdit through its
normal game-mode initialization and serialize the resulting interface graph;
it must not parse Pascal source files.

No accepted patch exists yet. Therefore the schema release workflow requires an
externally supplied exporter plus explicit executable, patch-set, and build
hashes. Public schema releases remain blocked until that patch is reviewed,
stored here, and passes the eleven-game differential corpus.

The patch must add two command-line operations:

- `--bethkit-provenance`, which writes one JSON object to standard output.
- `--bethkit-export --game <mode> --output <path>`, which writes contract v1.

The required JSON shape is defined in
`../contract/xedit-export-v1.schema.json`.
