// SPDX-License-Identifier: Apache-2.0
//!
//! Parser for TES3 (Morrowind) BSA archives.
//!
//! The TES3 format is a simple flat list of files with no directory hierarchy.
//! Files are never compressed in this format.

use std::sync::Arc;

use bethkit_io::MappedFile;

use ahash::HashMapExt;

use crate::archive::{normalise_path, ArchiveEntry};
use crate::error::{BsaError, Result};

/// Magic bytes that identify a TES3 (Morrowind) BSA file.
pub const MAGIC: [u8; 4] = [0x00, 0x01, 0x00, 0x00];

/// Internal record holding extraction data for one TES3 file.
#[derive(Debug)]
pub(super) struct Tes3Record {
    /// Absolute byte offset of the file data from the archive data section.
    pub(super) data_offset: u64,
    /// Size of the file data in bytes.
    pub(super) size: u32,
}

/// A parsed TES3 (Morrowind) BSA archive.
pub struct Tes3Archive {
    source: Arc<MappedFile>,
    entries: Vec<ArchiveEntry>,
    records: Vec<Tes3Record>,
    index: ahash::HashMap<String, usize>,
    /// Absolute offset into `source` where file data begins.
    data_start: u64,
}

impl Tes3Archive {
    /// Parses a TES3 BSA from a memory-mapped file.
    ///
    /// # Arguments
    ///
    /// * `source` - The memory-mapped archive file.
    ///
    /// # Returns
    ///
    /// A fully-parsed [`Tes3Archive`].
    ///
    /// # Errors
    ///
    /// Returns [`BsaError::Corrupt`] if any part of the header or directory
    /// cannot be read.
    pub fn parse(source: Arc<MappedFile>) -> Result<Self> {
        let bytes = source.as_bytes();
        let mut cur = bethkit_io::SliceCursor::new(bytes);

        // NOTE: First 4 bytes are magic, already verified by the caller.
        cur.read_array::<4>().map_err(BsaError::Io)?;

        let hash_offset = cur.read_u32().map_err(BsaError::Io)?;
        let file_count = cur.read_u32().map_err(BsaError::Io)? as usize;

        let mut raw_sizes = Vec::with_capacity(file_count);
        let mut raw_offsets = Vec::with_capacity(file_count);
        for _ in 0..file_count {
            raw_sizes.push(cur.read_u32().map_err(BsaError::Io)?);
            raw_offsets.push(cur.read_u32().map_err(BsaError::Io)?);
        }

        // NOTE: Name offset table is present but not needed; skip it.
        for _ in 0..file_count {
            cur.read_u32().map_err(BsaError::Io)?;
        }

        let mut names: Vec<String> = Vec::with_capacity(file_count);
        for _ in 0..file_count {
            let start = cur.pos();
            while cur.pos() < bytes.len() {
                let b = cur.read_u8().map_err(BsaError::Io)?;
                if b == 0 {
                    break;
                }
            }
            let end = cur.pos() - 1; // exclude null terminator byte
            let raw = &bytes[start..end];
            let name = std::str::from_utf8(raw)
                .map_err(|_| BsaError::Corrupt("non-UTF-8 file name".into()))?;
            names.push(normalise_path(name));
        }

        // NOTE: data_start = 8 (header) + hash_offset + 8 * file_count (hashes).
        // The hash table immediately follows the name strings; data follows the hashes.
        let data_start: u64 = 8u64 + hash_offset as u64 + 8u64 * file_count as u64;

        let mut entries = Vec::with_capacity(file_count);
        let mut records = Vec::with_capacity(file_count);
        let mut index: ahash::HashMap<String, usize> = ahash::HashMap::with_capacity(file_count);

        for i in 0..file_count {
            let size = raw_sizes[i];
            let raw_off = raw_offsets[i] as u64;
            let path = names[i].clone();
            index.insert(path.clone(), i);
            entries.push(ArchiveEntry {
                path,
                uncompressed_size: size,
                compressed_size: None,
            });
            records.push(Tes3Record {
                data_offset: raw_off,
                size,
            });
        }

        Ok(Self {
            source,
            entries,
            records,
            index,
            data_start,
        })
    }

    /// Extracts the raw bytes of a file by its index.
    ///
    /// # Arguments
    ///
    /// * `idx` - Index into `entries` / `records`.
    ///
    /// # Returns
    ///
    /// A zero-copy slice borrowed from the memory-mapped file.
    ///
    /// # Errors
    ///
    /// Returns [`BsaError::Corrupt`] if the byte range falls outside the file.
    pub(super) fn extract_by_index(&self, idx: usize) -> Result<std::borrow::Cow<'_, [u8]>> {
        let rec = &self.records[idx];
        let start = (self.data_start + rec.data_offset) as usize;
        let end = start + rec.size as usize;
        let bytes = self.source.as_bytes();
        if end > bytes.len() {
            return Err(BsaError::Corrupt(format!(
                "TES3 file data out of bounds at offset {start}"
            )));
        }
        Ok(std::borrow::Cow::Borrowed(&bytes[start..end]))
    }

    /// Returns the flat entry list.
    pub(super) fn entries(&self) -> &[ArchiveEntry] {
        &self.entries
    }

    /// Looks up an entry index by its normalised path.
    pub(super) fn find(&self, path: &str) -> Option<usize> {
        self.index.get(&normalise_path(path)).copied()
    }
}
