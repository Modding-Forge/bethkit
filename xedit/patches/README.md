# xEdit exporter patch set

This directory contains the MPL-2.0 patch series that adds Bethkit export
operations to `xDump`. The exporter must initialize xEdit through its normal
game-mode initialization and serialize the resulting interface graph; it must
not parse Pascal source files.

`0001-xdump-bethkit-provenance.patch` implements the provenance operation.
`0002-xdump-bethkit-schema-export.patch` wires the graph operation into `xDump`;
its reviewable MPL implementation lives in `../exporter`. Static xEdit nodes
are translated directly, while dynamic or not-yet-proven mappings become
explicit custom-decoder callbacks. Patches `0003` and `0004` expose binary
terminators, unused payloads, array termination, and every stored callback role
without parsing Pascal sources. The exporter resolves semantic callback invoke
addresses through the detailed Delphi MAP produced by the same build. Public
schema releases remain blocked until all emitted callbacks are classified, the
required decoders and handlers exist, and the eleven-game differential corpus
passes.

The patch must add two command-line operations:

- `--bethkit-provenance`, which writes one JSON object to standard output.
- `--bethkit-export --game <mode> --output <path>`, which writes contract v2.

The required JSON shape is defined in
`../contract/xedit-export-v2.schema.json`.

Use `../../scripts/Build-XEditExporter.ps1` to create a detached worktree at
the pinned revision, apply this patch series, stamp the patch and Delphi build
hashes, compile `xDump`, and verify the resulting executable.
