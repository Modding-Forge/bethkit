// SPDX-License-Identifier: Apache-2.0
//!
//! Parser for TES4 / FO3 / Skyrim LE / Skyrim SSE BSA archives.
//!
//! All three versions share the same on-disk structure; the version field
//! controls a few layout differences (SSE uses 64-bit folder offsets and LZ4
//! compression).

use std::borrow::Cow;
use std::sync::Arc;

use bethkit_io::{decompress_lz4_frame, decompress_zlib, MappedFile};

use ahash::HashMapExt;

use crate::archive::{normalise_path, ArchiveEntry};
use crate::bsa::flags::{ArchiveFlags, FILE_COMPRESS_BIT};
use crate::error::{BsaError, Result};

/// Magic bytes that identify a TES4-family BSA file (`"BSA\0"`).
pub const MAGIC: [u8; 4] = *b"BSA\0";

/// Archive format version codes.
pub mod version {
    /// Oblivion.
    pub const TES4: u32 = 0x67;
    /// Fallout 3 / New Vegas / Skyrim LE.
    pub const FO3: u32 = 0x68;
    /// Skyrim SE / AE.
    pub const SSE: u32 = 0x69;
}

/// Which BSA sub-variant is stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BsaVersion {
    /// Oblivion BSA.
    Tes4,
    /// Fallout 3 / New Vegas / Skyrim LE BSA.
    Fo3,
    /// Skyrim SE / AE BSA (LZ4 Frame compression, 64-bit folder offsets).
    Sse,
}

/// Internal record with everything needed to extract one file.
#[derive(Debug)]
pub(super) struct Tes4Record {
    /// Absolute byte offset of the (possibly compressed) file data.
    pub(super) offset: u64,
    /// Raw size field value (bit 30 is the per-file compress-override flag).
    pub(super) raw_size: u32,
    /// Whether this file is compressed after XOR-ing archive & file flags.
    pub(super) is_compressed: bool,
    /// Whether an embedded filename prefix must be skipped before the data.
    pub(super) has_embed_name: bool,
}

/// A parsed TES4-family BSA archive.
pub struct Tes4Archive {
    source: Arc<MappedFile>,
    bsa_version: BsaVersion,
    entries: Vec<ArchiveEntry>,
    records: Vec<Tes4Record>,
    index: ahash::HashMap<String, usize>,
}

impl Tes4Archive {
    /// Parses a TES4/FO3/SSE BSA from a memory-mapped file.
    ///
    /// # Arguments
    ///
    /// * `source`  - The memory-mapped archive file.
    /// * `version` - Raw version field read from the file header.
    ///
    /// # Returns
    ///
    /// A fully-parsed [`Tes4Archive`].
    ///
    /// # Errors
    ///
    /// Returns [`BsaError::UnsupportedVersion`] for unrecognised versions and
    /// [`BsaError::Corrupt`] for structural problems.
    pub fn parse(source: Arc<MappedFile>, raw_version: u32) -> Result<Self> {
        let bsa_version = match raw_version {
            version::TES4 => BsaVersion::Tes4,
            version::FO3 => BsaVersion::Fo3,
            version::SSE => BsaVersion::Sse,
            v => return Err(BsaError::UnsupportedVersion { version: v }),
        };

        let bytes = source.as_bytes();
        let mut cur = bethkit_io::SliceCursor::new(bytes);

        cur.read_array::<4>().map_err(BsaError::Io)?;
        cur.read_u32().map_err(BsaError::Io)?;

        let folders_offset = cur.read_u32().map_err(BsaError::Io)? as usize;
        let archive_flags = ArchiveFlags::from_bits_truncate(cur.read_u32().map_err(BsaError::Io)?);
        let folder_count = cur.read_u32().map_err(BsaError::Io)? as usize;
        let _file_count = cur.read_u32().map_err(BsaError::Io)?;
        let _folder_names_len = cur.read_u32().map_err(BsaError::Io)?;
        let _file_names_len = cur.read_u32().map_err(BsaError::Io)?;
        let _content_flags = cur.read_u32().map_err(BsaError::Io)?;

        let archive_compress = archive_flags.contains(ArchiveFlags::COMPRESS);
        let embed_names = archive_flags.contains(ArchiveFlags::EMBEDNAME)
            && matches!(bsa_version, BsaVersion::Fo3 | BsaVersion::Sse);

        if folders_offset > bytes.len() {
            return Err(BsaError::Corrupt("folders_offset out of bounds".into()));
        }
        let mut cur = bethkit_io::SliceCursor::new(&bytes[folders_offset..]);
        let base = folders_offset;

        // NOTE: Folder record layout:
        // TES4/FO3: Hash(8) + FileCount(4) + Offset(4) = 16 bytes
        // SSE:      Hash(8) + FileCount(4) + Unk32(4) + Offset(8) = 24 bytes
        struct FolderRecord {
            file_count: u32,
            // NOTE: The offset field is ignored during sequential reading; only
            // file_count is retained so we know how many file records follow.
        }
        let mut folders: Vec<FolderRecord> = Vec::with_capacity(folder_count);
        for _ in 0..folder_count {
            cur.read_array::<8>().map_err(BsaError::Io)?; // hash
            let fc = cur.read_u32().map_err(BsaError::Io)?;
            if bsa_version == BsaVersion::Sse {
                cur.read_u32().map_err(BsaError::Io)?; // unk32
                cur.read_array::<8>().map_err(BsaError::Io)?; // offset u64
            } else {
                cur.read_u32().map_err(BsaError::Io)?; // offset u32
            }
            folders.push(FolderRecord { file_count: fc });
        }

        // NOTE: Each folder data block layout:
        //   1 byte:  name length (including the null terminator)
        //   N bytes: folder name + 0x00
        //   file_count x 16 bytes: Hash(8) + Size(u32) + Offset(u32)
        let total_file_count: usize = folders.iter().map(|f| f.file_count as usize).sum();

        let mut raw_entries: Vec<(String, u32, u32)> = Vec::with_capacity(total_file_count);
        let mut folder_names: Vec<String> = Vec::with_capacity(folder_count);

        for folder in &folders {
            // NOTE: name_len includes the null terminator byte.
            let name_len = cur.read_u8().map_err(BsaError::Io)? as usize;
            let name_bytes = cur.read_slice(name_len).map_err(BsaError::Io)?;
            let name_end = if name_len > 0 && name_bytes[name_len - 1] == 0 {
                name_len - 1
            } else {
                name_len
            };
            let folder_name = std::str::from_utf8(&name_bytes[..name_end])
                .map_err(|_| BsaError::Corrupt("non-UTF-8 folder name".into()))?;
            let folder_name = normalise_path(folder_name);
            folder_names.push(folder_name.clone());

            for _ in 0..folder.file_count {
                cur.read_array::<8>().map_err(BsaError::Io)?; // hash
                let raw_size = cur.read_u32().map_err(BsaError::Io)?;
                let raw_offset = cur.read_u32().map_err(BsaError::Io)?;
                // File names are resolved after parsing the name table below.
                raw_entries.push((folder_name.clone(), raw_size, raw_offset));
            }
        }

        // NOTE: `cur` is relative to `base`; convert to absolute byte offset.
        let names_start = base + cur.pos();
        let mut pos = names_start;
        let mut file_names: Vec<String> = Vec::with_capacity(total_file_count);
        for _ in 0..total_file_count {
            let start = pos;
            while pos < bytes.len() && bytes[pos] != 0 {
                pos += 1;
            }
            let name_bytes = &bytes[start..pos];
            let name = std::str::from_utf8(name_bytes)
                .map_err(|_| BsaError::Corrupt("non-UTF-8 file name".into()))?;
            file_names.push(normalise_path(name));
            pos += 1; // skip null
        }

        let mut entries: Vec<ArchiveEntry> = Vec::with_capacity(total_file_count);
        let mut records: Vec<Tes4Record> = Vec::with_capacity(total_file_count);
        let mut index: ahash::HashMap<String, usize> =
            ahash::HashMap::with_capacity(total_file_count);

        for (i, ((folder_prefix, raw_size, raw_offset), file_name)) in
            raw_entries.into_iter().zip(file_names).enumerate()
        {
            let path = if folder_prefix.is_empty() {
                file_name
            } else {
                format!("{folder_prefix}/{file_name}")
            };

            let compress_bit = raw_size & FILE_COMPRESS_BIT != 0;
            let is_compressed = archive_compress ^ compress_bit;
            let actual_size = raw_size & !FILE_COMPRESS_BIT;

            let (uncompressed_size, compressed_size) = if is_compressed {
                // Uncompressed size is stored in the first 4 bytes of the
                // file data; we don't read it yet — we compute sizes at
                // extract time.
                (actual_size, Some(actual_size))
            } else {
                (actual_size, None)
            };

            index.insert(path.clone(), i);
            entries.push(ArchiveEntry {
                path,
                uncompressed_size,
                compressed_size,
            });
            records.push(Tes4Record {
                offset: raw_offset as u64,
                raw_size: actual_size,
                is_compressed,
                has_embed_name: embed_names,
            });
        }

        Ok(Self {
            source,
            bsa_version,
            entries,
            records,
            index,
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
    /// Either a zero-copy borrow (uncompressed) or an owned decompressed
    /// `Vec<u8>` (compressed).
    ///
    /// # Errors
    ///
    /// Returns [`BsaError::Corrupt`] if the byte range is out of bounds, or a
    /// decompression error if the data is malformed.
    pub(super) fn extract_by_index(&self, idx: usize) -> Result<Cow<'_, [u8]>> {
        let rec = &self.records[idx];
        let bytes = self.source.as_bytes();
        let mut offset = rec.offset as usize;

        if offset >= bytes.len() {
            return Err(BsaError::Corrupt(format!(
                "file data offset {offset} out of bounds"
            )));
        }

        let mut remaining = rec.raw_size as usize;

        // NOTE: Embedded filenames start with a 1-byte length prefix.
        if rec.has_embed_name {
            let name_len = bytes
                .get(offset)
                .copied()
                .ok_or_else(|| BsaError::Corrupt("embed name out of bounds".into()))?
                as usize;
            let skip = 1 + name_len;
            if skip > remaining {
                return Err(BsaError::Corrupt("embed name larger than file data".into()));
            }
            offset += skip;
            remaining -= skip;
        }

        if rec.is_compressed {
            // NOTE: The first 4 bytes of compressed data store the uncompressed size.
            if remaining < 4 {
                return Err(BsaError::Corrupt("compressed file too small".into()));
            }
            let unc_bytes = bytes
                .get(offset..offset + 4)
                .ok_or_else(|| BsaError::Corrupt("uncompressed size read out of bounds".into()))?;
            let uncompressed_size =
                u32::from_le_bytes([unc_bytes[0], unc_bytes[1], unc_bytes[2], unc_bytes[3]])
                    as usize;
            offset += 4;
            remaining -= 4;

            let compressed_data = bytes
                .get(offset..offset + remaining)
                .ok_or_else(|| BsaError::Corrupt("compressed data out of bounds".into()))?;

            let decompressed = match self.bsa_version {
                BsaVersion::Sse => decompress_lz4_frame(compressed_data, uncompressed_size)
                    .map_err(BsaError::Io)?,
                _ => decompress_zlib(compressed_data, uncompressed_size).map_err(BsaError::Io)?,
            };
            Ok(Cow::Owned(decompressed))
        } else {
            let end = offset + remaining;
            if end > bytes.len() {
                return Err(BsaError::Corrupt(format!(
                    "file data [{offset}..{end}] out of bounds"
                )));
            }
            Ok(Cow::Borrowed(&bytes[offset..end]))
        }
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
