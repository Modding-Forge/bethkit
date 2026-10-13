# Encoding component notices

Bethkit remains licensed under Apache-2.0. The following separately licensed components are linked into the native library. Their upstream copyright and permission notices are included under `licenses/encoding/` and must accompany the distributed native archives and Python wheels. This list documents the encoding components; it is not a complete inventory of all existing Bethkit dependencies.

| Component | Version | License used for redistribution | Source |
| --- | --- | --- | --- |
| chardetng | 1.0.0 | MIT (upstream offers Apache-2.0 OR MIT) | https://github.com/hsivonen/chardetng |
| encoding_rs | 0.8.35 | MIT and BSD-3-Clause for WHATWG-derived tables | https://github.com/hsivonen/encoding_rs |
| memchr | 2.8.0 | MIT (upstream offers MIT OR Unlicense) | https://github.com/BurntSushi/memchr |
| cfg-if | 1.0.4 | MIT (upstream offers Apache-2.0 OR MIT) | https://github.com/rust-lang/cfg-if |

chardetng's generated `src/data.rs` carries a CC0 public-domain dedication: https://creativecommons.org/publicdomain/zero/1.0/. The detector's software license still applies to its other source files. No Python `chardet` code or xEdit source is copied into this implementation. Test fixtures for translated text are synthetic; third-party plugin assets are not distributed.
