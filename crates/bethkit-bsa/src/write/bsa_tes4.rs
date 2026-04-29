// SPDX-License-Identifier: Apache-2.0
//! Writer for TES4 / FO3 / Skyrim SSE BSA archives.
//!
//! All three versions share the same structural layout.  Key differences:
//! - **TES4** (0x67): 32-bit folder offsets, zlib compression, no embed names.
//! - **FO3** (0x68): 32-bit folder offsets, zlib compression, optional embed
//!   names.
//! - **SSE** (0x69): **64-bit** folder offsets, LZ4 Frame compression, optional
//!   embed names.
//!
//! Files within each folder **must be sorted by hash**; folders themselves
//! must also be sorted by hash.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::Write;
use std::path::Path;

use crate::bsa::flags::{ArchiveFlags, ContentFlags};
use crate::error::{BsaError, Result};
use crate::hash::{hash_tes4_dir, hash_tes4_file};
use crate::write::{BsaVersion, BuilderEntry};

/// BSA file magic.
const MAGIC: [u8; 4] = *b"BSA\0";
/// Oblivion version.
const VERSION_TES4: u32 = 0x67;
/// Fallout 3 / Skyrim LE version.
const VERSION_FO3: u32 = 0x68;
/// Skyrim SE / AE version.
const VERSION_SSE: u32 = 0x69;

/// Sentinel bit in the file `size` field: inverts the archive-wide compress default.
const FILE_COMPRESS_BIT: u32 = 0x4000_0000;

/// Writes a TES4-family BSA archive to `dest`.
///
/// # Arguments
///
/// * `dest`        - Output path.  Created or truncated.
/// * `version`     - Which BSA variant to produce.
/// * `compress`    - Archive-wide default compression flag.
/// * `embed_names` - Whether to write a filename prefix before each file's data.
/// * `entries`     - Files to pack.  Paths must already be normalised.
///
/// # Errors
///
/// Returns [`BsaError::EmptyArchive`] when `entries` is empty.
pub(super) fn write(
    dest: &Path,
    version: BsaVersion,
    compress: bool,
    embed_names: bool,
    entries: Vec<BuilderEntry>,
) -> Result<()> {
    if entries.is_empty() {
        return Err(BsaError::EmptyArchive);
    }

    let raw_version = match version {
        BsaVersion::Tes4 => VERSION_TES4,
        BsaVersion::Fo3 => VERSION_FO3,
        BsaVersion::Sse => VERSION_SSE,
        BsaVersion::Tes3 => unreachable!("TES3 handled by bsa_tes3 writer"),
    };

    let is_sse = version == BsaVersion::Sse;
    // NOTE: EMBEDNAME is only meaningful for FO3 and SSE; caller already
    // set the flag appropriately.
    let do_embed = embed_names && matches!(version, BsaVersion::Fo3 | BsaVersion::Sse);

    // Compress each file and group by folder.
    // Key: folder hash (u64); Value: ordered list of (file_hash, folder_str,
    // file_name_str, raw_size, stored_blob).
    // We use a BTreeMap keyed by (folder_hash, file_hash) so the final output
    // is deterministically sorted by hash without an extra sort pass.
    struct FileEntry {
        folder_name: String,
        file_name: String,
        folder_hash: u64,
        file_hash: u64,
        /// Actual bytes written to disk (may include embed-name prefix,
        /// uncompressed-size u32, and compressed/raw payload).
        stored: Vec<u8>,
        /// Uncompressed size, stored before compressed payload when compressed.
        is_compressed: bool,
    }

    let mut file_entries: Vec<FileEntry> = Vec::with_capacity(entries.len());
    let mut content_flags = ContentFlags::empty();

    for entry in entries {
        // Split path into folder and filename.
        let (folder_str, file_str) = match entry.path.rfind('/') {
            Some(idx) => (entry.path[..idx].to_owned(), entry.path[idx + 1..].to_owned()),
            None => (String::new(), entry.path.clone()),
        };

        // Compute content flags from extension.
        if let Some(ext) = file_str.rfind('.').map(|i| &file_str[i..]) {
            content_flags |= content_flag_for_ext(ext);
        }

        // Split filename stem/extension for hashing.
        let (stem, ext) = split_stem_ext(&file_str);
        let folder_hash = hash_tes4_dir(&folder_str.replace('/', "\\"));
        let file_hash = hash_tes4_file(stem, ext);

        // Determine whether this file is compressed.
        let should_compress = entry.compress_override.unwrap_or(compress);

        let is_compressed = should_compress;

        // Build the payload (data that will be stored at the file's offset).
        let mut stored: Vec<u8> = Vec::new();

        // Optional embedded file name prefix.
        if do_embed {
            let fname_bytes = file_str.as_bytes();
            let name_len = fname_bytes.len().min(255) as u8;
            stored.push(name_len);
            stored.extend_from_slice(&fname_bytes[..name_len as usize]);
        }

        if is_compressed {
            let compressed = compress_data(&entry.data, is_sse)?;
            stored.extend_from_slice(&(entry.data.len() as u32).to_le_bytes());
            stored.extend_from_slice(&compressed);
        } else {
            stored.extend_from_slice(&entry.data);
        }

        file_entries.push(FileEntry {
            folder_name: folder_str,
            file_name: file_str,
            folder_hash,
            file_hash,
            stored,
            is_compressed,
        });
    }

    // Sort: primary by folder hash, secondary by file hash.
    file_entries.sort_by_key(|e| (e.folder_hash, e.file_hash));

    // Group into folders (preserve sorted order).
    // folder_hash → (folder_name, Vec<index_into_file_entries>)
    let mut folder_order: Vec<u64> = Vec::new();
    let mut folder_map: BTreeMap<u64, (String, Vec<usize>)> = BTreeMap::new();
    for (idx, fe) in file_entries.iter().enumerate() {
        let slot = folder_map
            .entry(fe.folder_hash)
            .or_insert_with(|| {
                folder_order.push(fe.folder_hash);
                (fe.folder_name.clone(), Vec::new())
            });
        slot.1.push(idx);
    }
    // NOTE: BTreeMap iteration is sorted by key, which equals hash order since
    // we sorted file_entries by folder_hash first.
    let folders: Vec<(u64, String, Vec<usize>)> = folder_map
        .into_iter()
        .map(|(h, (name, idxs))| (h, name, idxs))
        .collect();

    let folder_count = folders.len() as u32;
    let file_count = file_entries.len() as u32;

    // Folder names length: for each folder, 1 (length byte) + name_len + 1 (null).
    let folder_names_len: u32 = folders
        .iter()
        .map(|(_, name, _)| 1u32 + name.len() as u32 + 1u32)
        .sum();
    // File names length: each file name + null terminator.
    let file_names_len: u32 = file_entries
        .iter()
        .map(|fe| fe.file_name.len() as u32 + 1u32)
        .sum();

    // Compute ArchiveFlags.
    let mut archive_flags = ArchiveFlags::PATHNAMES | ArchiveFlags::FILENAMES;
    if compress {
        archive_flags |= ArchiveFlags::COMPRESS;
    }
    if do_embed {
        archive_flags |= ArchiveFlags::EMBEDNAME;
    }

    // Layout sizes:
    //   Header:          36 bytes
    //   Folder records:  folder_count × (16 TES4/FO3 | 24 SSE)
    //   Folder data:     Σ (1 + name_len + 1 + file_count_in_folder × 16)
    //   Name table:      file_names_len bytes
    //   File data:       Σ stored.len()
    let folder_record_size: u64 = if is_sse { 24 } else { 16 };
    let folders_offset: u32 = 36;
    let folder_records_end: u64 = 36 + folder_count as u64 * folder_record_size;

    // Compute where each folder's data block starts.
    let mut folder_data_offsets: Vec<u64> = Vec::with_capacity(folders.len());
    let mut pos = folder_records_end;
    for (_, name, idxs) in &folders {
        folder_data_offsets.push(pos);
        let block_size = 1u64 + name.len() as u64 + 1u64 + idxs.len() as u64 * 16;
        pos += block_size;
    }

    // Name table starts right after all folder data blocks.
    let name_table_start: u64 = pos;

    // File data starts right after name table.
    let file_data_start: u64 = name_table_start + file_names_len as u64;

    // Compute per-file data offsets (absolute from archive start, u32).
    let mut file_data_offsets: Vec<u32> = Vec::with_capacity(file_entries.len());
    let mut running_data: u64 = file_data_start;
    for fe in &file_entries {
        let off = u32::try_from(running_data).map_err(|_| {
            BsaError::Corrupt("BSA file data offset exceeds 4 GiB (u32 limit)".into())
        })?;
        file_data_offsets.push(off);
        running_data += fe.stored.len() as u64;
    }

    // Compute the size field for each file record.
    // Bit 30 inverts the archive-wide compress flag for that specific file.
    // When archive compress == file compress: bit 30 = 0.
    // When they differ: bit 30 = 1.
    let raw_sizes: Vec<u32> = file_entries
        .iter()
        .map(|fe| {
            let compress_bit = if fe.is_compressed != compress {
                FILE_COMPRESS_BIT
            } else {
                0
            };
            // NOTE: stored.len() is the on-disk size (embed prefix + payload).
            // The size field tracks the total stored bytes.
            fe.stored.len() as u32 | compress_bit
        })
        .collect();

    // — Write —
    let mut out = File::create(dest)?;

    // Header (36 bytes).
    out.write_all(&MAGIC)?;
    out.write_all(&raw_version.to_le_bytes())?;
    out.write_all(&folders_offset.to_le_bytes())?;
    out.write_all(&archive_flags.bits().to_le_bytes())?;
    out.write_all(&folder_count.to_le_bytes())?;
    out.write_all(&file_count.to_le_bytes())?;
    out.write_all(&folder_names_len.to_le_bytes())?;
    out.write_all(&file_names_len.to_le_bytes())?;
    out.write_all(&content_flags.bits().to_le_bytes())?;

    // Folder records.
    for (i, (folder_hash, _, idxs)) in folders.iter().enumerate() {
        out.write_all(&folder_hash.to_le_bytes())?;
        out.write_all(&(idxs.len() as u32).to_le_bytes())?;
        if is_sse {
            out.write_all(&0u32.to_le_bytes())?; // unknown
            out.write_all(&folder_data_offsets[i].to_le_bytes())?;
        } else {
            let off32 = u32::try_from(folder_data_offsets[i]).map_err(|_| {
                BsaError::Corrupt("BSA folder data offset exceeds 4 GiB".into())
            })?;
            out.write_all(&off32.to_le_bytes())?;
        }
    }

    // Folder data blocks.
    // For each folder: (1 byte name_len_incl_null)(name bytes)(0x00)(file records).
    let mut global_file_idx: usize = 0;
    for (_, name, idxs) in &folders {
        let name_bytes = name.as_bytes();
        let name_len_byte = (name_bytes.len() as u8).wrapping_add(1); // +1 for null
        out.write_all(&[name_len_byte])?;
        out.write_all(name_bytes)?;
        out.write_all(&[0u8])?; // null terminator

        for &file_idx in idxs {
            let file_hash = file_entries[file_idx].file_hash;
            let raw_size = raw_sizes[file_idx];
            let offset = file_data_offsets[global_file_idx];
            out.write_all(&file_hash.to_le_bytes())?;
            out.write_all(&raw_size.to_le_bytes())?;
            out.write_all(&offset.to_le_bytes())?;
            global_file_idx += 1;
        }
    }

    // Name table: null-terminated file names.
    for fe in &file_entries {
        out.write_all(fe.file_name.as_bytes())?;
        out.write_all(&[0u8])?;
    }

    // File data.
    for fe in &file_entries {
        out.write_all(&fe.stored)?;
    }

    Ok(())
}

/// Compresses `data` using zlib (TES4/FO3) or LZ4 Frame (SSE).
///
/// # Arguments
///
/// * `data`   - Uncompressed input.
/// * `is_sse` - `true` to use LZ4 Frame; `false` for zlib.
///
/// # Errors
///
/// Returns [`BsaError::WriteIo`] on compression failure.
fn compress_data(data: &[u8], is_sse: bool) -> Result<Vec<u8>> {
    if is_sse {
        compress_lz4_frame(data)
    } else {
        compress_zlib(data)
    }
}

/// Compresses `data` with zlib (default level).
fn compress_zlib(data: &[u8]) -> Result<Vec<u8>> {
    use flate2::write::ZlibEncoder;
    use flate2::Compression;
    use std::io::Write;

    let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
    enc.write_all(data)?;
    Ok(enc.finish()?)
}

/// Compresses `data` with LZ4 Frame format (used by SSE).
fn compress_lz4_frame(data: &[u8]) -> Result<Vec<u8>> {
    use lz4_flex::frame::FrameEncoder;
    use std::io::Write;

    let mut enc = FrameEncoder::new(Vec::new());
    enc.write_all(data)?;
    enc.finish().map_err(|e| {
        crate::error::BsaError::WriteIo(std::io::Error::other(e))
    })
}

/// Splits a filename string into `(stem, ext)` where ext includes the dot.
///
/// Returns `("", "")` for empty input and `(name, "")` when there is no dot.
fn split_stem_ext(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(idx) => (&name[..idx], &name[idx..]),
        None => (name, ""),
    }
}

/// Maps a file extension (including the dot, lowercase) to a [`ContentFlags`]
/// bit.
fn content_flag_for_ext(ext: &str) -> ContentFlags {
    match ext {
        ".nif" | ".kf" | ".btr" | ".bto" => ContentFlags::MESHES,
        ".dds" => ContentFlags::TEXTURES,
        ".swf" | ".gfx" | ".xml" => ContentFlags::MENUS,
        ".wav" | ".xwm" | ".mp3" => ContentFlags::SOUNDS,
        ".fuz" | ".lip" => ContentFlags::VOICES,
        ".hlsl" | ".fx" => ContentFlags::SHADERS,
        ".spt" | ".lst" => ContentFlags::TREES,
        ".fnt" | ".otf" | ".ttf" => ContentFlags::FONTS,
        _ => ContentFlags::MISC,
    }
}

#[cfg(test)]
mod tests {
    use crate::write::{BsaVersion, BsaWriter};

    /// Verifies that a TES4 BSA roundtrip preserves file content.
    #[test]
    fn tes4_roundtrip() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("test.bsa");
        let mut w = BsaWriter::new(BsaVersion::Tes4);
        w.add("meshes/iron.nif", b"nif data".to_vec());
        w.add("textures/iron.dds", b"dds data".to_vec());

        // when
        w.write_to(&path)?;

        // then
        let archive = crate::open(&path)?;
        assert_eq!(archive.file_count(), 2);
        assert_eq!(
            archive.extract("meshes/iron.nif").expect("entry exists")?.to_vec(),
            b"nif data"
        );
        assert_eq!(
            archive.extract("textures/iron.dds").expect("entry exists")?.to_vec(),
            b"dds data"
        );
        Ok(())
    }

    /// Verifies that a TES4 BSA with compression enabled roundtrips correctly.
    #[test]
    fn tes4_compressed_roundtrip() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("compressed.bsa");
        let payload = b"Hello Skyrim! ".repeat(100);
        let mut w = BsaWriter::new(BsaVersion::Tes4).compress(true);
        w.add("meshes/big.nif", payload.to_vec());

        // when
        w.write_to(&path)?;

        // then
        let archive = crate::open(&path)?;
        assert_eq!(
            archive.extract("meshes/big.nif").expect("entry exists")?.to_vec(),
            payload
        );
        Ok(())
    }

    /// Verifies that an SSE BSA with LZ4 compression roundtrips correctly.
    #[test]
    fn sse_compressed_roundtrip() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("sse.bsa");
        let payload = b"SSE payload data ".repeat(50);
        let mut w = BsaWriter::new(BsaVersion::Sse).compress(true);
        w.add("meshes/sse.nif", payload.to_vec());

        // when
        w.write_to(&path)?;

        // then
        let archive = crate::open(&path)?;
        assert_eq!(
            archive.extract("meshes/sse.nif").expect("entry exists")?.to_vec(),
            payload
        );
        Ok(())
    }

    /// Verifies that an SSE BSA without compression roundtrips correctly.
    #[test]
    fn sse_uncompressed_roundtrip() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("sse_unc.bsa");
        let mut w = BsaWriter::new(BsaVersion::Sse);
        w.add("meshes/a.nif", b"data_a".to_vec());
        w.add("scripts/b.pex", b"data_b".to_vec());

        // when
        w.write_to(&path)?;

        // then
        let archive = crate::open(&path)?;
        assert_eq!(archive.file_count(), 2);
        assert_eq!(
            archive.extract("meshes/a.nif").expect("entry exists")?.to_vec(),
            b"data_a"
        );
        assert_eq!(
            archive.extract("scripts/b.pex").expect("entry exists")?.to_vec(),
            b"data_b"
        );
        Ok(())
    }
}
