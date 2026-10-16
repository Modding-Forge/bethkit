# Third-party component notices

Bethkit remains licensed under Apache-2.0. The following separately licensed components are linked into the native library. Their upstream copyright and permission notices are included under `licenses/encoding/` and must accompany the distributed native archives and Python wheels. This list documents the encoding components; it is not a complete inventory of all existing Bethkit dependencies.

| Component | Version | License used for redistribution | Source |
| --- | --- | --- | --- |
| chardetng | 1.0.0 | MIT (upstream offers Apache-2.0 OR MIT) | https://github.com/hsivonen/chardetng |
| encoding_rs | 0.8.35 | MIT and BSD-3-Clause for WHATWG-derived tables | https://github.com/hsivonen/encoding_rs |
| memchr | 2.8.0 | MIT (upstream offers MIT OR Unlicense) | https://github.com/BurntSushi/memchr |
| cfg-if | 1.0.4 | MIT (upstream offers Apache-2.0 OR MIT) | https://github.com/rust-lang/cfg-if |

chardetng's generated `src/data.rs` carries a CC0 public-domain dedication: https://creativecommons.org/publicdomain/zero/1.0/. The detector's software license still applies to its other source files. No Python `chardet` code or xEdit source is copied into this implementation. Test fixtures for translated text are synthetic; third-party plugin assets are not distributed.

## Optional native allocator

The opt-in `bethkit-ffi/fast-mode` feature (via `fast-allocator`) links mimalloc 0.1.52, libmimalloc-sys 0.1.49 and the latter's bundled Microsoft mimalloc v3 implementation. The Rust wrapper and sys crate use MIT, Copyright 2019 Octavian Oncescu, retained in [mimalloc-rust-MIT.txt](licenses/allocator/mimalloc-rust-MIT.txt). Microsoft's MIT notice, Copyright 2018-2025 Microsoft Corporation and Daan Leijen, is retained in [mimalloc-MIT.txt](licenses/allocator/mimalloc-MIT.txt). Sources are [the Rust wrapper](https://github.com/purpleprotocol/mimalloc_rust) and [Microsoft mimalloc](https://github.com/microsoft/mimalloc). Distributions enabling this feature must additionally include `licenses/allocator/` and this notice. Default builds do not enable it; existing release pipelines are unchanged.

This selects an allocator for Rust-owned memory in standalone native builds, not a replacement for Python's allocator or the C caller's allocation functions. Callers must still use the matching Bethkit free function and must not exchange handles or allocations between different library builds. Static-link consumers with their own global allocator should leave the feature disabled. Building the optional dependency requires a C compiler in addition to the normal Rust prerequisites. No GPL or LGPL component is introduced by this option.
