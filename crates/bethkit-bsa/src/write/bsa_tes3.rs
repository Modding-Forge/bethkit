// SPDX-License-Identifier: Apache-2.0
//! Writer for TES3 (Morrowind) BSA archives.
//!
//! TES3 archives are the simplest supported format: a flat file list with no
//! directory hierarchy and no compression.  Files are indexed by a 64-bit hash
//! and **must be stored in ascending hash order**.

use std::fs::File;
use std::io::Write;
use std::path::Path;

use crate::error::{BsaError, Result};
use crate::hash::hash_tes3;
use crate::write::BuilderEntry;

/// Magic bytes that identify a TES3 BSA file.
const MAGIC: [u8; 4] = [0x00, 0x01, 0x00, 0x00];

/// Serialises the provided entries as a TES3 BSA archive and writes it to
/// `dest`.
///
/// # Arguments
///
/// * `dest`    - Output path.  Created or truncated.
/// * `entries` - Files to pack.  Paths must already be normalised.
///
/// # Errors
///
/// Returns [`BsaError::EmptyArchive`] when `entries` is empty, or an I/O
/// error on write failure.
pub(super) fn write(dest: &Path, entries: Vec<BuilderEntry>) -> Result<()> {
    if entries.is_empty() {
        return Err(BsaError::EmptyArchive);
    }

    // NOTE: TES3 BSA requires files to be sorted by their 64-bit hash.
    let mut sorted: Vec<(u64, BuilderEntry)> = entries
        .into_iter()
        .map(|e| {
            // NOTE: TES3 hashes the filename only (no directory prefix).
            let filename = e.path.rsplit('/').next().unwrap_or(e.path.as_str());
            let h = hash_tes3(filename);
            (h, e)
        })
        .collect();
    sorted.sort_by_key(|(h, _)| *h);

    let n = sorted.len();

    // Build the name block: null-terminated strings, and record each offset.
    let mut name_block: Vec<u8> = Vec::new();
    let mut name_offsets: Vec<u32> = Vec::with_capacity(n);
    for (_, entry) in &sorted {
        let off = name_block.len() as u32;
        name_offsets.push(off);
        name_block.extend_from_slice(entry.path.as_bytes());
        name_block.push(0u8);
    }

    // Layout:
    //   header (8):          magic(4) + hash_offset(4)
    //   sizes[n]    (4n):    file sizes
    //   offsets[n]  (4n):    file offsets relative to data_start
    //   name_off[n] (4n):    name offsets relative to name block start
    //   name_block  (?):     null-terminated paths
    //   hashes[n]   (8n):    64-bit hashes
    //   data        (?):     file contents
    //
    // hash_offset is measured from byte 8 (right after the hash_offset field).
    // Byte 8 is where file_count lives (4 bytes), followed by 8n size+offset
    // pairs, 4n name-offset entries, and the name block.
    let name_block_size = name_block.len();
    let hash_offset: u32 = (4 + 8 * n + 4 * n + name_block_size) as u32;
    let _data_start: u64 = 8u64 + hash_offset as u64 + 8u64 * n as u64;

    // Compute data offsets (relative to data_start).
    let mut data_offsets: Vec<u32> = Vec::with_capacity(n);
    let mut running: u32 = 0;
    for (_, entry) in &sorted {
        data_offsets.push(running);
        running = running
            .checked_add(entry.data.len() as u32)
            .ok_or_else(|| BsaError::Corrupt("TES3 archive data exceeds 4 GiB".into()))?;
    }

    let mut out = File::create(dest)?;

    // Header.
    out.write_all(&MAGIC)?;
    out.write_all(&hash_offset.to_le_bytes())?;
    out.write_all(&(n as u32).to_le_bytes())?;

    // File sizes and offsets, interleaved: (size, offset) per file.
    for (i, (_, entry)) in sorted.iter().enumerate() {
        out.write_all(&(entry.data.len() as u32).to_le_bytes())?;
        out.write_all(&data_offsets[i].to_le_bytes())?;
    }

    // Name offsets (relative to name block start).
    for &off in &name_offsets {
        out.write_all(&off.to_le_bytes())?;
    }

    // Name block.
    out.write_all(&name_block)?;

    // Hash table (sorted, 8 bytes each).
    for (hash, _) in &sorted {
        out.write_all(&hash.to_le_bytes())?;
    }

    // File data.
    for (_, entry) in &sorted {
        out.write_all(&entry.data)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::write::BsaVersion;

    /// Verifies that writing and re-reading a TES3 BSA preserves all files.
    #[test]
    fn tes3_roundtrip() -> std::result::Result<(), Box<dyn std::error::Error>> {
        use crate::write::BsaWriter;

        // given
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("test.bsa");

        let files: &[(&str, &[u8])] = &[
            ("meshes/iron.nif", b"nif-data"),
            ("textures/iron.dds", b"dds-data-longer"),
            ("scripts/main.pex", b"pex"),
        ];

        let mut w = BsaWriter::new(BsaVersion::Tes3);
        for (p, d) in files {
            w.add(*p, d.to_vec());
        }

        // when
        w.write_to(&path)?;

        // then
        let archive = crate::open(&path)?;
        assert_eq!(archive.file_count(), 3);
        for (p, expected) in files {
            let extracted = archive.extract(p).expect("file present")?.to_vec();
            assert_eq!(extracted, *expected, "mismatch for {p}");
        }

        Ok(())
    }

    /// Verifies that writing an empty TES3 archive returns EmptyArchive.
    #[test]
    fn tes3_empty_is_error() {
        use crate::write::BsaWriter;
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("empty.bsa");
        let w = BsaWriter::new(BsaVersion::Tes3);
        let result = w.write_to(&path);
        assert!(matches!(result, Err(BsaError::EmptyArchive)));
    }
}
