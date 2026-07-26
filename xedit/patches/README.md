# xEdit exporter patch set

This directory contains the MPL-2.0 patch series that adds Bethkit export
operations to `xDump`. The exporter must initialize xEdit through its normal
game-mode initialization and serialize the resulting interface graph; it must
not parse Pascal source files.

`0001-xdump-bethkit-provenance.patch` implements the provenance operation.
The definition-graph operation is still outstanding, so public schema releases
remain blocked until it is implemented, reviewed, and passes the eleven-game
differential corpus.

The patch must add two command-line operations:

- `--bethkit-provenance`, which writes one JSON object to standard output.
- `--bethkit-export --game <mode> --output <path>`, which writes contract v1.

The required JSON shape is defined in
`../contract/xedit-export-v1.schema.json`.

Use `../../scripts/Build-XEditExporter.ps1` to create a detached worktree at
the pinned revision, apply this patch series, stamp the patch and Delphi build
hashes, compile `xDump`, and verify the resulting executable.
