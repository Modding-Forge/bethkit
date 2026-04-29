// SPDX-License-Identifier: Apache-2.0
//!
//! Parser and extractor for BA2 DX10 (texture) archives.
//!
//! DX10 archives store textures as a set of compressed or uncompressed mip
//! chunks.  Extraction reassembles the full DDS file by prepending the
//! reconstructed DDS header to the concatenated mip data.

use std::sync::Arc;

use bethkit_io::{decompress_zlib, MappedFile};

use ahash::HashMapExt;

use crate::archive::{normalise_path, ArchiveEntry};
use crate::ba2::dds::build_dds_header;
use crate::error::{BsaError, Result};

/// Expected tail sentinel at the end of each DX10 texture chunk.
const BAADF00D: u32 = 0xBAAD_F00D;

/// One mip-level chunk stored inside the archive.
#[derive(Debug, Clone)]
struct Dx10Chunk {
    offset: u64,
    packed_size: u32,
    size: u32,
    _start_mip: u16,
    _end_mip: u16,
}

/// Internal record for one DX10 texture.
#[derive(Debug)]
struct Dx10Record {
    width: u16,
    height: u16,
    num_mips: u8,
    dxgi_format: u8,
    cube_maps: u16,
    chunks: Vec<Dx10Chunk>,
}

/// A parsed BA2 DX10 (texture) archive.
pub struct Dx10Archive {
    source: Arc<MappedFile>,
    entries: Vec<ArchiveEntry>,
    records: Vec<Dx10Record>,
    index: ahash::HashMap<String, usize>,
}

impl Dx10Archive {
    /// Parses a BA2 DX10 archive.
    ///
    /// The `bytes` slice must contain the entire archive.  The cursor is
    /// expected to be positioned at offset 24 (right after the shared BA2
    /// header).
    ///
    /// # Arguments
    ///
    /// * `source`       - Memory-mapped archive file.
    /// * `file_count`   - Number of texture file records.
    /// * `table_offset` - Byte offset of the file-name table.
    ///
    /// # Returns
    ///
    /// A fully-parsed [`Dx10Archive`].
    ///
    /// # Errors
    ///
    /// Returns [`BsaError::Corrupt`] on structural violations.
    pub fn parse(source: Arc<MappedFile>, file_count: u32, table_offset: u64) -> Result<Self> {
        let bytes = source.as_bytes();
        let file_count = file_count as usize;
        let names_start = table_offset as usize;

        // Parse names first so we can build paths.
        let names = parse_name_table(bytes, names_start, file_count)?;

        // DX10 file record: 24 bytes header + chunk_count × 24 bytes.
        // Position starts at byte 24 (after the shared BA2 header).
        let mut pos = 24usize;

        let mut entries = Vec::with_capacity(file_count);
        let mut records = Vec::with_capacity(file_count);
        let mut index: ahash::HashMap<String, usize> = ahash::HashMap::with_capacity(file_count);

        for (i, name) in names.iter().enumerate() {
            // File record header (24 bytes):
            // name_hash:u32, ext:[u8;4], dir_hash:u32,
            // unknown_tex:u8, chunk_count:u8, chunk_hdr_size:u16,
            // height:u16, width:u16, num_mips:u8, dxgi:u8, cube_maps:u16
            if pos + 24 > bytes.len() {
                return Err(BsaError::Corrupt(format!(
                    "DX10 record {i} header out of bounds at {pos}"
                )));
            }
            let chunk_count = bytes[pos + 13] as usize;
            // _chunk_hdr_size bytes[14..16] — always 24, we skip it
            let height = u16::from_le_bytes([bytes[pos + 16], bytes[pos + 17]]);
            let width = u16::from_le_bytes([bytes[pos + 18], bytes[pos + 19]]);
            let num_mips = bytes[pos + 20];
            let dxgi_format = bytes[pos + 21];
            let cube_maps = u16::from_le_bytes([bytes[pos + 22], bytes[pos + 23]]);
            pos += 24;

            // Chunk records (24 bytes each).
            let mut chunks = Vec::with_capacity(chunk_count);
            for j in 0..chunk_count {
                if pos + 24 > bytes.len() {
                    return Err(BsaError::Corrupt(format!(
                        "DX10 record {i} chunk {j} out of bounds"
                    )));
                }
                let chunk_offset = i64::from_le_bytes(
                    bytes[pos..pos + 8]
                        .try_into()
                        .map_err(|_| BsaError::Corrupt(format!("DX10 chunk {i}/{j}: offset")))?,
                ) as u64;
                let packed_size = u32::from_le_bytes(
                    bytes[pos + 8..pos + 12]
                        .try_into()
                        .map_err(|_| {
                            BsaError::Corrupt(format!("DX10 chunk {i}/{j}: packed_size"))
                        })?,
                );
                let size = u32::from_le_bytes(
                    bytes[pos + 12..pos + 16]
                        .try_into()
                        .map_err(|_| BsaError::Corrupt(format!("DX10 chunk {i}/{j}: size")))?,
                );
                let start_mip = u16::from_le_bytes([bytes[pos + 16], bytes[pos + 17]]);
                let end_mip = u16::from_le_bytes([bytes[pos + 18], bytes[pos + 19]]);
                let tail = u32::from_le_bytes(
                    bytes[pos + 20..pos + 24]
                        .try_into()
                        .map_err(|_| BsaError::Corrupt(format!("DX10 chunk {i}/{j}: tail")))?,
                );
                if tail != BAADF00D {
                    return Err(BsaError::Corrupt(format!(
                        "DX10 record {i} chunk {j}: bad tail {tail:#010x}"
                    )));
                }
                pos += 24;
                chunks.push(Dx10Chunk {
                    offset: chunk_offset,
                    packed_size,
                    size,
                    _start_mip: start_mip,
                    _end_mip: end_mip,
                });
            }

            let uncompressed_size: u32 = chunks.iter().map(|c| c.size).sum::<u32>()
                + build_dds_header(width, height, num_mips, dxgi_format, cube_maps).len() as u32;

            let path = normalise_path(name);
            index.insert(path.clone(), i);
            entries.push(ArchiveEntry {
                path,
                uncompressed_size,
                compressed_size: None, // mixed; reported at entry level
            });
            records.push(Dx10Record {
                width,
                height,
                num_mips,
                dxgi_format,
                cube_maps,
                chunks,
            });
        }

        Ok(Self {
            source,
            entries,
            records,
            index,
        })
    }

    /// Extracts and reassembles a texture by index.
    ///
    /// Returns the full DDS file as an owned `Vec<u8>` (header + mip data).
    ///
    /// # Errors
    ///
    /// Returns [`BsaError::Corrupt`] on out-of-bounds reads or decompression
    /// errors.
    pub(super) fn extract_by_index(&self, idx: usize) -> Result<std::borrow::Cow<'_, [u8]>> {
        let rec = &self.records[idx];
        let bytes = self.source.as_bytes();

        let mut header = build_dds_header(
            rec.width,
            rec.height,
            rec.num_mips,
            rec.dxgi_format,
            rec.cube_maps,
        );
        let total_tex: usize = rec.chunks.iter().map(|c| c.size as usize).sum();
        let mut out = Vec::with_capacity(header.len() + total_tex);
        out.append(&mut header);

        for (j, chunk) in rec.chunks.iter().enumerate() {
            let offset = chunk.offset as usize;
            if chunk.packed_size != 0 {
                let end = offset + chunk.packed_size as usize;
                if end > bytes.len() {
                    return Err(BsaError::Corrupt(format!(
                        "DX10 chunk {j} compressed data out of bounds"
                    )));
                }
                let decompressed = decompress_zlib(&bytes[offset..end], chunk.size as usize)
                    .map_err(BsaError::Io)?;
                out.extend_from_slice(&decompressed);
            } else {
                let end = offset + chunk.size as usize;
                if end > bytes.len() {
                    return Err(BsaError::Corrupt(format!(
                        "DX10 chunk {j} data out of bounds"
                    )));
                }
                out.extend_from_slice(&bytes[offset..end]);
            }
        }

        Ok(std::borrow::Cow::Owned(out))
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

/// Parses the file-name table (same format as GNRL).
fn parse_name_table(bytes: &[u8], start: usize, count: usize) -> Result<Vec<String>> {
    let mut names = Vec::with_capacity(count);
    let mut pos = start;
    for i in 0..count {
        if pos + 2 > bytes.len() {
            return Err(BsaError::Corrupt(format!(
                "DX10 name table entry {i} out of bounds at {pos}"
            )));
        }
        let len = u16::from_le_bytes([bytes[pos], bytes[pos + 1]]) as usize;
        pos += 2;
        if pos + len > bytes.len() {
            return Err(BsaError::Corrupt(format!(
                "DX10 name table entry {i} string out of bounds"
            )));
        }
        let name = std::str::from_utf8(&bytes[pos..pos + len])
            .map_err(|_| BsaError::Corrupt(format!("DX10 name table entry {i} non-UTF-8")))?;
        names.push(name.to_owned());
        pos += len;
    }
    Ok(names)
}
