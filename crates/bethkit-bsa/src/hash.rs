// SPDX-License-Identifier: Apache-2.0
//!
//! Hashing algorithms used to index files in Bethesda archives.
//!
//! Three distinct algorithms are used across the supported formats:
//! - [`hash_tes3`] — Morrowind BSA (simple XOR/rotate scheme).
//! - [`hash_tes4`] — Oblivion/FO3/Skyrim/SSE BSA (split name+extension hash).
//! - [`hash_fo4`] — Fallout 4 / Starfield BA2 (standard CRC-32).

use std::sync::OnceLock;

/// Computes the TES3 (Morrowind) BSA hash for a file path.
///
/// The algorithm XORs shifted character values across two halves of the
/// (lowercased) filename.  The high 32 bits come from the first half;
/// the low 32 bits from the second half (with a right-rotate mixed in).
///
/// # Arguments
///
/// * `filename` - Full filename (no directory), e.g. `"iron_armor.nif"`.
///
/// # Returns
///
/// A 64-bit hash value.
pub fn hash_tes3(filename: &str) -> u64 {
    let s: Vec<u8> = filename.bytes().map(|b| b.to_ascii_lowercase()).collect();
    let l = s.len() / 2;

    let mut sum: u32 = 0;
    let mut off: u32 = 0;
    for &b in &s[..l] {
        let temp = (b as u32).wrapping_shl(off & 0x1F);
        sum ^= temp;
        off = off.wrapping_add(8);
    }
    let hi = (sum as u64) << 32;

    sum = 0;
    off = 0;
    for &b in &s[l..] {
        let temp = (b as u32).wrapping_shl(off & 0x1F);
        sum ^= temp;
        let n = temp & 0x1F;
        sum = sum.rotate_right(n);
        off = off.wrapping_add(8);
    }
    hi | (sum as u64)
}

/// Computes the TES4 (Oblivion/FO3/Skyrim/SSE) BSA hash for a directory path.
///
/// This is equivalent to `hash_tes4_file(dir_path, "")`.
///
/// # Arguments
///
/// * `dir_path` - Directory path component (lowercase, backslash-separated),
///   e.g. `"meshes\\armor"`.
///
/// # Returns
///
/// A 64-bit hash value.
pub fn hash_tes4_dir(dir_path: &str) -> u64 {
    hash_tes4_file(dir_path, "")
}

/// Computes the TES4 (Oblivion/FO3/Skyrim/SSE) BSA hash for a file.
///
/// The hash encodes:
/// - Low 32 bits: first byte, second-to-last byte, name length, last byte,
///   plus special flags for known extensions (`.kf`, `.nif`, `.dds`, `.wav`).
/// - High 32 bits: sdbm hash of the interior characters and the extension.
///
/// # Arguments
///
/// * `name` - Filename stem without extension (lowercase), e.g. `"iron_armor"`.
/// * `ext`  - Extension including the dot (lowercase), e.g. `".nif"`.  Pass
///   `""` when hashing a directory path.
///
/// # Returns
///
/// A 64-bit hash value.
pub fn hash_tes4_file(name: &str, ext: &str) -> u64 {
    let s: Vec<u8> = name.bytes().map(|b| b.to_ascii_lowercase()).collect();
    let e: Vec<u8> = ext.bytes().map(|b| b.to_ascii_lowercase()).collect();

    let l = s.len();
    if l == 0 {
        return 0;
    }

    let mut result: u64 = s[l - 1] as u64;
    if l > 2 {
        result |= (s[l - 2] as u64) << 8;
    }
    result |= (l as u64) << 16;
    result |= (s[0] as u64) << 24;

    // NOTE: Well-known extension flags occupy specific bit positions in the hash.
    let ext_key = {
        let mut k = [0u8; 4];
        for (i, &b) in e.iter().take(4).enumerate() {
            k[i] = b;
        }
        u32::from_le_bytes(k)
    };
    let ext_bits: u64 = match ext_key {
        0x00666B2E => 0x80,        // ".kf\0"
        0x66696E2E => 0x8000,      // ".nif"
        0x7364642E => 0x8080,      // ".dds"
        0x7661772E => 0x8000_0000, // ".wav"
        _ => 0,
    };
    result |= ext_bits;

    let mut hash: u32 = 0;
    if l > 2 {
        // NOTE: Only the interior characters s[1..l-3] (inclusive) feed the
        // SDBM hash. s[0], s[l-2], and s[l-1] are already encoded in the
        // low 32 bits (bits 24-31, 8-15, and 0-7 respectively), so they must
        // be excluded here to match the reference implementation.
        for &b in &s[1..l - 2] {
            hash = (b as u32)
                .wrapping_add(hash.wrapping_shl(6))
                .wrapping_add(hash.wrapping_shl(16))
                .wrapping_sub(hash);
        }
    }
    let mut ext_hash: u32 = 0;
    for &b in &e {
        ext_hash = (b as u32)
            .wrapping_add(ext_hash.wrapping_shl(6))
            .wrapping_add(ext_hash.wrapping_shl(16))
            .wrapping_sub(ext_hash);
    }
    result += ((hash.wrapping_add(ext_hash)) as u64) << 32;
    result
}

/// Splits a filename into its stem and extension.
///
/// Returns `("stem", ".ext")` where the extension includes the dot, or
/// `("name", "")` if there is no dot.
///
/// # Arguments
///
/// * `filename` - Filename to split (may include directory separators).
///
/// # Returns
///
/// A tuple of `(stem, extension)` as string slices.
pub fn split_name_ext(filename: &str) -> (&str, &str) {
    match filename.rfind('.') {
        Some(pos) => (&filename[..pos], &filename[pos..]),
        None => (filename, ""),
    }
}

/// Computes the FO4/BA2 CRC-32 hash for a path component.
///
/// This is a standard IEEE CRC-32 (polynomial `0xEDB88320`) applied to the
/// lowercased, slash-normalised bytes of the input.  Non-ASCII bytes are
/// skipped.  Forward slashes are converted to backslashes before hashing.
///
/// # Arguments
///
/// * `s` - The string to hash (filename stem, extension, or directory path).
///
/// # Returns
///
/// A 32-bit CRC value.
pub fn hash_fo4(s: &str) -> u32 {
    let table = crc32_table();
    let mut result: u32 = 0;
    for b in s.bytes() {
        if b > 127 {
            continue;
        }
        let c = if b == b'/' {
            b'\\'
        } else {
            b.to_ascii_lowercase()
        };
        result = (result >> 8) ^ table[((result ^ c as u32) & 0xFF) as usize];
    }
    result
}

/// Returns a reference to the lazily-initialised IEEE CRC-32 lookup table.
///
/// The table is built once using the reflected polynomial `0xEDB88320` and
/// then stored in a [`OnceLock`].
fn crc32_table() -> &'static [u32; 256] {
    static TABLE: OnceLock<[u32; 256]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = [0u32; 256];
        for i in 0..256u32 {
            let mut c = i;
            for _ in 0..8 {
                if c & 1 != 0 {
                    c = 0xEDB8_8320 ^ (c >> 1);
                } else {
                    c >>= 1;
                }
            }
            table[i as usize] = c;
        }
        table
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the TES4 hash of a known filename produces the expected value.
    ///
    /// Reference value derived from the TES5Edit (`wbBSArchive.pas`)
    /// `CreateHashTES4` implementation applied to `cuirass.nif`.
    #[test]
    fn tes4_hash_known_value() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let (stem, ext) = split_name_ext("cuirass.nif");

        // when
        let h = hash_tes4_file(stem, ext);

        // then
        assert_eq!(h, 0x0a9125a0_6307f373);
        Ok(())
    }

    /// Verifies that a short (3-char) stem produces an SDBM interior-loop of
    /// exactly zero iterations — the only interior byte `s[1]` would also be
    /// `s[l-2]`, which is already encoded in bits 8-15 and must not be fed
    /// into the hash a second time.
    #[test]
    fn tes4_hash_three_char_stem() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given / when
        let h = hash_tes4_file("abc", ".nif");

        // then — upper 32 bits come from ext_hash alone (interior loop is empty)
        assert_eq!(h, 0x92cd45fd_6103e263);
        Ok(())
    }

    /// Verifies that the CRC-32 table entry for index 1 matches the known IEEE value.
    #[test]
    fn crc32_table_index_one() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let table = crc32_table();

        // when
        let value = table[1];

        // then
        assert_eq!(value, 0x7707_3096);
        Ok(())
    }

    /// Verifies that `hash_fo4` is deterministic and produces non-zero output.
    #[test]
    fn fo4_hash_non_zero() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let s = "textures/armor/iron/cuirass.dds";

        // when
        let h = hash_fo4(s);

        // then
        assert_ne!(h, 0);
        assert_eq!(hash_fo4(s), h);
        Ok(())
    }
}
