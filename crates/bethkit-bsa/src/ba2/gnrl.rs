// SPDX-License-Identifier: Apache-2.0
//!
//! Parser and extractor for BA2 GNRL (general / non-texture) archives.

use std::borrow::Cow;
use std::sync::Arc;

use bethkit_io::{decompress_zlib, MappedFile};

use ahash::HashMapExt;

use crate::archive::{normalise_path, ArchiveEntry};
use crate::error::{BsaError, Result};

/// Expected tail sentinel at the end of each GNRL file record.
const BAADF00D: u32 = 0xBAAD_F00D;

/// Internal record for one GNRL file.
#[derive(Debug)]
pub(super) struct GnrlRecord {
    /// Absolute byte offset in the archive file.
    pub(super) offset: u64,
    /// Compressed (stored) size; `0` means the file is stored verbatim.
    pub(super) packed_size: u32,
    /// Uncompressed file size in bytes.
    pub(super) size: u32,
}

/// A parsed BA2 GNRL (general files) archive.
pub struct GnrlArchive {
    source: Arc<MappedFile>,
    entries: Vec<ArchiveEntry>,
    records: Vec<GnrlRecord>,
    index: ahash::HashMap<String, usize>,
}

impl GnrlArchive {
    /// Parses a BA2 GNRL archive.
    ///
    /// The cursor must be positioned right after the BA2 file header (i.e.
    /// after reading magic `"BTDX"`, version u32, and sub-type `"GNRL"`).
    /// The full raw file bytes are also required for building the cursor.
    ///
    /// # Arguments
    ///
    /// * `source`      - Memory-mapped archive file.
    /// * `file_count`  - Number of file records (from the BA2 header).
    /// * `table_offset`- Byte offset of the file-name table (from the header).
    ///
    /// # Returns
    ///
    /// A fully-parsed [`GnrlArchive`].
    ///
    /// # Errors
    ///
    /// Returns [`BsaError::Corrupt`] if any structural invariant is violated.
    pub fn parse(source: Arc<MappedFile>, file_count: u32, table_offset: u64) -> Result<Self> {
        let bytes = source.as_bytes();
        // Cursor positioned after magic(4)+version(4)+sub_type(4) = 12 bytes,
        // then BA2 header: file_count(4) + table_offset(8) = 12 more → 24.
        let file_count = file_count as usize;
        // Positioned at first file record.
        let records_start = 24usize; // offset in `bytes`
        let record_size = 36usize; // 4+4+4+4+8+4+4+4 bytes per record
        let names_start = table_offset as usize;

        if records_start + file_count * record_size > bytes.len() {
            return Err(BsaError::Corrupt("GNRL file records out of bounds".into()));
        }

        // Parse name table.
        let names = parse_name_table(bytes, names_start, file_count)?;

        let mut entries = Vec::with_capacity(file_count);
        let mut records = Vec::with_capacity(file_count);
        let mut index: ahash::HashMap<String, usize> = ahash::HashMap::with_capacity(file_count);

        for (i, name) in names.iter().enumerate() {
            let base = records_start + i * record_size;
            let rec_bytes = &bytes[base..base + record_size];
            // NOTE: Record layout: name_hash(4) ext(4) dir_hash(4) unknown(4)
            // NOTE: offset(8) packed_size(4) size(4) tail(4) = 36 bytes total
            let offset = i64::from_le_bytes(
                rec_bytes[12..20]
                    .try_into()
                    .map_err(|_| BsaError::Corrupt(format!("GNRL record {i}: offset read")))?,
            ) as u64;
            let packed_size =
                u32::from_le_bytes(rec_bytes[20..24].try_into().map_err(|_| {
                    BsaError::Corrupt(format!("GNRL record {i}: packed_size read"))
                })?);
            let size = u32::from_le_bytes(
                rec_bytes[24..28]
                    .try_into()
                    .map_err(|_| BsaError::Corrupt(format!("GNRL record {i}: size read")))?,
            );
            let tail = u32::from_le_bytes(
                rec_bytes[28..32]
                    .try_into()
                    .map_err(|_| BsaError::Corrupt(format!("GNRL record {i}: tail read")))?,
            );

            if tail != BAADF00D {
                return Err(BsaError::Corrupt(format!(
                    "GNRL record {i}: expected tail {BAADF00D:#010x}, got {tail:#010x}"
                )));
            }

            let path = normalise_path(name);
            index.insert(path.clone(), i);
            entries.push(ArchiveEntry {
                path,
                uncompressed_size: size,
                compressed_size: if packed_size != 0 {
                    Some(packed_size)
                } else {
                    None
                },
            });
            records.push(GnrlRecord {
                offset,
                packed_size,
                size,
            });
        }

        Ok(Self {
            source,
            entries,
            records,
            index,
        })
    }

    /// Extracts a file by its entry index.
    ///
    /// Returns a zero-copy borrow for uncompressed files, or an owned
    /// decompressed `Vec<u8>` for compressed ones.
    ///
    /// # Errors
    ///
    /// Returns [`BsaError::Corrupt`] on out-of-bounds reads, or a
    /// decompression error if the data is malformed.
    pub(super) fn extract_by_index(&self, idx: usize) -> Result<Cow<'_, [u8]>> {
        let rec = &self.records[idx];
        let bytes = self.source.as_bytes();
        let offset = rec.offset as usize;

        if rec.packed_size != 0 {
            let end = offset + rec.packed_size as usize;
            if end > bytes.len() {
                return Err(BsaError::Corrupt(format!(
                    "GNRL compressed data [{offset}..{end}] out of bounds"
                )));
            }
            let decompressed =
                decompress_zlib(&bytes[offset..end], rec.size as usize).map_err(BsaError::Io)?;
            Ok(Cow::Owned(decompressed))
        } else {
            let end = offset + rec.size as usize;
            if end > bytes.len() {
                return Err(BsaError::Corrupt(format!(
                    "GNRL file data [{offset}..{end}] out of bounds"
                )));
            }
            Ok(Cow::Borrowed(&bytes[offset..end]))
        }
    }

    /// Returns the flat entry list.
    pub(super) fn entries(&self) -> &[ArchiveEntry] {
        &self.entries
    }

    /// Looks up an entry index by path.
    pub(super) fn find(&self, path: &str) -> Option<usize> {
        self.index.get(&normalise_path(path)).copied()
    }
}

/// Parses the file-name table at `start` in `bytes`.
///
/// Each entry is a `u16` length followed by that many bytes (no null).
fn parse_name_table(bytes: &[u8], start: usize, count: usize) -> Result<Vec<String>> {
    let mut names = Vec::with_capacity(count);
    let mut pos = start;
    for i in 0..count {
        if pos + 2 > bytes.len() {
            return Err(BsaError::Corrupt(format!(
                "name table entry {i} length out of bounds at {pos}"
            )));
        }
        let len = u16::from_le_bytes([bytes[pos], bytes[pos + 1]]) as usize;
        pos += 2;
        if pos + len > bytes.len() {
            return Err(BsaError::Corrupt(format!(
                "name table entry {i} string out of bounds"
            )));
        }
        let name = std::str::from_utf8(&bytes[pos..pos + len])
            .map_err(|_| BsaError::Corrupt(format!("name table entry {i} non-UTF-8")))?;
        names.push(name.to_owned());
        pos += len;
    }
    Ok(names)
}
