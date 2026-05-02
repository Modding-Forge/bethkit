// SPDX-License-Identifier: Apache-2.0
//! Writer for BA2 DX10 (texture) archives.
//!
//! BA2 DX10 archives store textures as a series of compressed or uncompressed
//! mip-level chunks.  Each texture entry contains a small metadata header
//! followed by one or more chunk records that describe individual mip ranges.
//!
//! This writer packs all mip data into a **single chunk per texture** for
//! simplicity.  The resulting archives are fully compatible with all known
//! readers.

use std::fs::File;
use std::io::Write;
use std::path::Path;

use crate::error::{BsaError, Result};
use crate::hash::hash_fo4;
use crate::write::dds_parse;

/// BA2 magic bytes (`"BTDX"`).
const MAGIC: [u8; 4] = *b"BTDX";
/// DX10 sub-type magic.
const SUB_DX10: [u8; 4] = *b"DX10";
/// Sentinel value at the end of every chunk record.
const BAADF00D: u32 = 0xBAAD_F00D;

/// Builder for BA2 DX10 (texture) archives.
///
/// Accepts DDS files as input.  Each file is parsed to extract its metadata
/// and mip data, which are then serialised in the BA2 DX10 format.
///
/// # Example
///
/// ```no_run
/// use bethkit_bsa::write::{Ba2Dx10Writer, Ba2Version};
///
/// let mut w = Ba2Dx10Writer::new(Ba2Version::V1);
/// let dds_data = std::fs::read("textures/sky.dds").unwrap();
/// w.add("textures/sky.dds", dds_data).unwrap();
/// w.write_to(std::path::Path::new("textures.ba2")).unwrap();
/// ```
pub struct Ba2Dx10Writer {
    version: crate::write::Ba2Version,
    /// Whether to compress each mip chunk with zlib.
    compress: bool,
    entries: Vec<Dx10Entry>,
}

/// An already-parsed texture pending serialisation.
struct Dx10Entry {
    path: String,
    info: dds_parse::DdsInfo,
}

impl Ba2Dx10Writer {
    /// Creates an empty DX10 writer.
    ///
    /// Default: no compression.
    ///
    /// # Arguments
    ///
    /// * `version` - BA2 format version to write.
    ///
    /// # Returns
    ///
    /// An empty [`Ba2Dx10Writer`].
    pub fn new(version: crate::write::Ba2Version) -> Self {
        Self {
            version,
            compress: false,
            entries: Vec::new(),
        }
    }

    /// Enables or disables per-chunk zlib compression.
    ///
    /// # Arguments
    ///
    /// * `compress` - `true` to compress all mip chunks.
    ///
    /// # Returns
    ///
    /// `self` for chaining.
    pub fn compress(mut self, compress: bool) -> Self {
        self.compress = compress;
        self
    }

    /// Parses `dds_data` and queues it for inclusion in the archive.
    ///
    /// The `path` is normalised (lowercased, forward slashes) before storage.
    ///
    /// # Arguments
    ///
    /// * `path`     - Virtual archive path, e.g. `"textures/sky.dds"`.
    /// * `dds_data` - Raw DDS file bytes (must begin with `"DDS "`).
    ///
    /// # Errors
    ///
    /// Returns [`BsaError::InvalidDds`] or [`BsaError::UnsupportedDxgiFormat`]
    /// if the DDS is malformed or uses an unsupported pixel format.
    pub fn add(&mut self, path: impl Into<String>, dds_data: Vec<u8>) -> Result<()> {
        let path = crate::archive::normalise_path(&path.into());
        let info = dds_parse::parse(&dds_data)?;
        self.entries.push(Dx10Entry { path, info });
        Ok(())
    }

    /// Writes the archive to `dest`.
    ///
    /// # Arguments
    ///
    /// * `dest` - Output file path.  Created or truncated.
    ///
    /// # Errors
    ///
    /// Returns [`BsaError::EmptyArchive`] if no textures have been added, or
    /// an I/O error on write failure.
    pub fn write_to(self, dest: &Path) -> Result<()> {
        write(dest, self.version, self.compress, self.entries)
    }
}

/// Serialises all texture entries as a BA2 DX10 archive.
fn write(
    dest: &Path,
    version: crate::write::Ba2Version,
    compress: bool,
    entries: Vec<Dx10Entry>,
) -> Result<()> {
    if entries.is_empty() {
        return Err(BsaError::EmptyArchive);
    }

    let file_count = entries.len() as u32;

    // For each texture: one chunk covering all mip data.
    struct TexChunk {
        /// Stored (possibly compressed) mip bytes.
        stored: Vec<u8>,
        /// Uncompressed size.
        uncompressed: u32,
        /// Compressed size (0 if not compressed).
        packed_size: u32,
    }

    struct TexRecord {
        path: String,
        width: u16,
        height: u16,
        num_mips: u8,
        dxgi_format: u8,
        cube_maps: u16,
        chunk: TexChunk,
    }

    let mut records: Vec<TexRecord> = Vec::with_capacity(entries.len());

    for entry in entries {
        let info = entry.info;
        let all_mip_bytes: Vec<u8> = info.mip_data.clone();
        let uncompressed = all_mip_bytes.len() as u32;

        let (stored, packed_size) = if compress {
            let compressed = compress_zlib(&all_mip_bytes)?;
            let packed = compressed.len() as u32;
            (compressed, packed)
        } else {
            (all_mip_bytes, 0u32)
        };

        records.push(TexRecord {
            path: entry.path,
            width: info.width,
            height: info.height,
            num_mips: info.num_mips,
            dxgi_format: info.dxgi_format,
            cube_maps: info.cube_maps,
            chunk: TexChunk {
                stored,
                uncompressed,
                packed_size,
            },
        });
    }

    // Layout:
    //   Header (24):              MAGIC + version + "DX10" + file_count + table_offset
    //   Tex records (each var):   24-byte header + 1 × 24-byte chunk record
    //     → 48 bytes per texture
    //   Chunk data (?):           concatenated stored chunk bytes
    //   Name table (?):           [u16 len][bytes] per texture
    const HEADER_SIZE: u64 = 24;
    const TEX_RECORD_SIZE: u64 = 48; // 24 header + 1 × 24 chunk

    let tex_records_end: u64 = HEADER_SIZE + file_count as u64 * TEX_RECORD_SIZE;

    // Compute chunk data offsets (absolute from start of file).
    let mut chunk_offsets: Vec<i64> = Vec::with_capacity(records.len());
    let mut running: u64 = tex_records_end;
    for rec in &records {
        chunk_offsets.push(running as i64);
        running += rec.chunk.stored.len() as u64;
    }

    // Name table starts after all chunk data.
    let table_offset: i64 = running as i64;

    // — Write —
    let mut out = File::create(dest)?;

    // Header.
    out.write_all(&MAGIC)?;
    out.write_all(&version.as_u32().to_le_bytes())?;
    out.write_all(&SUB_DX10)?;
    out.write_all(&file_count.to_le_bytes())?;
    out.write_all(&table_offset.to_le_bytes())?;

    // Texture records.
    for (i, rec) in records.iter().enumerate() {
        let (dir, filename) = split_dir_filename(&rec.path);
        let (stem, ext) = split_stem_ext(filename);
        let name_hash = hash_fo4(stem);
        let ext_hash = hash_fo4(ext);
        let dir_hash = hash_fo4(dir);

        // Texture header (24 bytes).
        out.write_all(&name_hash.to_le_bytes())?;
        out.write_all(&ext_hash.to_le_bytes())?;
        out.write_all(&dir_hash.to_le_bytes())?;
        out.write_all(&0u8.to_le_bytes())?; // unknown_tex
        out.write_all(&1u8.to_le_bytes())?; // chunk_count = 1
        out.write_all(&24u16.to_le_bytes())?; // chunk_hdr_size
        out.write_all(&rec.height.to_le_bytes())?;
        out.write_all(&rec.width.to_le_bytes())?;
        out.write_all(&rec.num_mips.to_le_bytes())?;
        out.write_all(&rec.dxgi_format.to_le_bytes())?;
        out.write_all(&rec.cube_maps.to_le_bytes())?;

        // Chunk record (24 bytes): covers mip 0 → num_mips-1.
        out.write_all(&chunk_offsets[i].to_le_bytes())?;
        out.write_all(&rec.chunk.packed_size.to_le_bytes())?;
        out.write_all(&rec.chunk.uncompressed.to_le_bytes())?;
        out.write_all(&0u16.to_le_bytes())?; // start_mip
        out.write_all(&(rec.num_mips.saturating_sub(1) as u16).to_le_bytes())?; // end_mip
        out.write_all(&BAADF00D.to_le_bytes())?;
    }

    // Chunk data.
    for rec in &records {
        out.write_all(&rec.chunk.stored)?;
    }

    // Name table: [u16 len][bytes].
    for rec in &records {
        let bytes = rec.path.as_bytes();
        out.write_all(&(bytes.len() as u16).to_le_bytes())?;
        out.write_all(bytes)?;
    }

    Ok(())
}

/// Compresses `data` with zlib at default level.
fn compress_zlib(data: &[u8]) -> Result<Vec<u8>> {
    use flate2::write::ZlibEncoder;
    use flate2::Compression;
    use std::io::Write;

    let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
    enc.write_all(data)?;
    Ok(enc.finish()?)
}

/// Returns `(dir, filename)` from a normalised forward-slash path.
fn split_dir_filename(path: &str) -> (&str, &str) {
    match path.rfind('/') {
        Some(idx) => (&path[..idx], &path[idx + 1..]),
        None => ("", path),
    }
}

/// Splits a filename into `(stem, ext)` where ext includes the dot.
fn split_stem_ext(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(idx) => (&name[..idx], &name[idx..]),
        None => (name, ""),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::write::Ba2Version;

    /// Builds a minimal valid 1×1 BC1 (DXT1) DDS file with 1 mip level.
    fn minimal_bc1_dds() -> Vec<u8> {
        let mut dds = Vec::with_capacity(136);
        // Magic
        dds.extend_from_slice(b"DDS ");
        // DDS_HEADER (124 bytes starting with dwSize=124)
        dds.extend_from_slice(&124u32.to_le_bytes()); // dwSize
                                                      // dwFlags: CAPS | HEIGHT | WIDTH | PIXELFORMAT | MIPMAPCOUNT | LINEARSIZE
        let flags: u32 = 0x0001 | 0x0002 | 0x0004 | 0x1000 | 0x2_0000 | 0x8_0000;
        dds.extend_from_slice(&flags.to_le_bytes());
        dds.extend_from_slice(&1u32.to_le_bytes()); // height = 1
        dds.extend_from_slice(&1u32.to_le_bytes()); // width = 1
        dds.extend_from_slice(&8u32.to_le_bytes()); // pitch_or_linear (BC1 1×1 = 8)
        dds.extend_from_slice(&1u32.to_le_bytes()); // depth = 1
        dds.extend_from_slice(&1u32.to_le_bytes()); // mip count = 1
        dds.extend_from_slice(&[0u8; 44]); // dwReserved1[11]
                                           // DDS_PIXELFORMAT (32 bytes)
        dds.extend_from_slice(&32u32.to_le_bytes()); // pf.dwSize
        dds.extend_from_slice(&4u32.to_le_bytes()); // pf.dwFlags = FOURCC
        dds.extend_from_slice(b"DXT1"); // FourCC
        dds.extend_from_slice(&[0u8; 20]); // remaining pf fields
                                           // dwCaps, dwCaps2, dwCaps3, dwCaps4, dwReserved2 (20 bytes)
        let caps: u32 = 0x0000_1000 | 0x0040_0000; // TEXTURE | MIPMAP
        dds.extend_from_slice(&caps.to_le_bytes());
        dds.extend_from_slice(&[0u8; 16]);
        // 1 mip of BC1 data for 1×1 = 8 bytes
        dds.extend_from_slice(&[0u8; 8]);
        dds
    }

    /// Verifies that a BA2 DX10 roundtrip with a minimal DDS preserves content.
    #[test]
    fn dx10_roundtrip() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("textures.ba2");
        let dds = minimal_bc1_dds();
        let mut w = Ba2Dx10Writer::new(Ba2Version::V1);
        w.add("textures/test.dds", dds.clone())?;

        // when
        w.write_to(&path)?;

        // then
        let archive = crate::open(&path)?;
        assert_eq!(archive.file_count(), 1);
        let extracted = archive
            .extract("textures/test.dds")
            .expect("entry exists")?
            .to_vec();
        // The extracted DDS should start with "DDS " and contain the same
        // pixel data (mip bytes).  Header is reconstructed so we only check
        // that the magic is present and the pixel data matches.
        assert!(
            extracted.starts_with(b"DDS "),
            "extracted missing DDS magic"
        );
        // BC1 1×1 mip = 8 bytes; they appear after the reconstructed header.
        assert!(extracted.ends_with(&[0u8; 8]), "mip data mismatch");
        Ok(())
    }
}
