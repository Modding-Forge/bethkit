// SPDX-License-Identifier: Apache-2.0
//! Writer for BA2 GNRL (general files) archives.
//!
//! BA2 GNRL archives store arbitrary files (meshes, sounds, scripts, …).
//! The format uses CRC-32 hashes for indexing; there is no required sort order.
//! A name table at the end of the file maps each entry to its human-readable
//! path.

use std::fs::File;
use std::io::Write;
use std::path::Path;

use crate::error::{BsaError, Result};
use crate::hash::hash_fo4;

/// BA2 magic bytes (`"BTDX"`).
const MAGIC: [u8; 4] = *b"BTDX";
/// GNRL sub-type magic.
const SUB_GNRL: [u8; 4] = *b"GNRL";
/// Sentinel value at the end of every GNRL file record.
const BAADF00D: u32 = 0xBAAD_F00D;

/// Builder for BA2 GNRL archives.
///
/// # Example
///
/// ```no_run
/// use bethkit_bsa::write::{Ba2GnrlWriter, Ba2Version};
///
/// let mut w = Ba2GnrlWriter::new(Ba2Version::V1);
/// w.add("meshes/foo.nif", b"NIF data".to_vec());
/// w.write_to(std::path::Path::new("output.ba2")).unwrap();
/// ```
pub struct Ba2GnrlWriter {
    version: crate::write::Ba2Version,
    /// Whether to compress each file with zlib.
    compress: bool,
    entries: Vec<crate::write::BuilderEntry>,
}

impl Ba2GnrlWriter {
    /// Creates an empty GNRL writer.
    ///
    /// Default: no compression.
    ///
    /// # Arguments
    ///
    /// * `version` - BA2 format version to write.
    ///
    /// # Returns
    ///
    /// An empty [`Ba2GnrlWriter`].
    pub fn new(version: crate::write::Ba2Version) -> Self {
        Self {
            version,
            compress: false,
            entries: Vec::new(),
        }
    }

    /// Enables or disables per-file zlib compression.
    ///
    /// # Arguments
    ///
    /// * `compress` - `true` to compress all files.
    ///
    /// # Returns
    ///
    /// `self` for chaining.
    pub fn compress(mut self, compress: bool) -> Self {
        self.compress = compress;
        self
    }

    /// Adds a file to the pending archive.
    ///
    /// The `path` is normalised (lowercased, backslashes replaced with `/`)
    /// before storage.
    ///
    /// # Arguments
    ///
    /// * `path` - Virtual archive path, e.g. `"meshes/armor/iron.nif"`.
    /// * `data` - Uncompressed file content.
    pub fn add(&mut self, path: impl Into<String>, data: Vec<u8>) {
        let path = crate::archive::normalise_path(&path.into());
        self.entries.push(crate::write::BuilderEntry {
            path,
            data,
            compress_override: None,
        });
    }

    /// Writes the archive to `dest`.
    ///
    /// # Arguments
    ///
    /// * `dest` - Output file path.  Created or truncated.
    ///
    /// # Errors
    ///
    /// Returns [`BsaError::EmptyArchive`] if no files have been added, or
    /// an I/O error on write failure.
    pub fn write_to(self, dest: &Path) -> Result<()> {
        write(dest, self.version, self.compress, self.entries)
    }
}

/// Internal function that serialises the archive.
fn write(
    dest: &Path,
    version: crate::write::Ba2Version,
    compress: bool,
    entries: Vec<crate::write::BuilderEntry>,
) -> Result<()> {
    if entries.is_empty() {
        return Err(BsaError::EmptyArchive);
    }

    let file_count = entries.len() as u32;

    // Prepare per-file stored blobs.
    struct GnrlFile {
        path: String,
        uncompressed_size: u32,
        stored: Vec<u8>,
        packed_size: u32,
    }

    // Compress files in parallel.  rayon's collect preserves input order, so
    // the name-table and record-offset calculations remain correct.
    use rayon::prelude::*;
    let files: Vec<GnrlFile> = entries
        .into_par_iter()
        .map(|entry| {
            let should_compress = entry.compress_override.unwrap_or(compress);
            let uncompressed_size = entry.data.len() as u32;
            let (stored, packed_size) = if should_compress {
                let compressed = compress_zlib(&entry.data)?;
                let packed = compressed.len() as u32;
                (compressed, packed)
            } else {
                (entry.data, 0u32)
            };
            Ok(GnrlFile {
                path: entry.path,
                uncompressed_size,
                stored,
                packed_size,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    // Layout:
    //   Header (24):            MAGIC + version + "GNRL" + file_count + table_offset
    //   File records (36 × N):  per-file metadata
    //   File data (?):          stored blobs
    //   Name table (?):         [u16 len][bytes] per file
    const HEADER_SIZE: u64 = 24;
    const RECORD_SIZE: u64 = 36;

    let records_end: u64 = HEADER_SIZE + file_count as u64 * RECORD_SIZE;

    // Compute per-file data offsets.
    let mut data_offsets: Vec<u64> = Vec::with_capacity(files.len());
    let mut running: u64 = records_end;
    for f in &files {
        data_offsets.push(running);
        running += f.stored.len() as u64;
    }

    // Name table starts after all file data.
    let table_offset: i64 = running as i64;

    // — Write —
    let mut out = File::create(dest)?;

    // Header.
    out.write_all(&MAGIC)?;
    out.write_all(&version.as_u32().to_le_bytes())?;
    out.write_all(&SUB_GNRL)?;
    out.write_all(&file_count.to_le_bytes())?;
    out.write_all(&table_offset.to_le_bytes())?;

    // File records.
    for (i, f) in files.iter().enumerate() {
        let (dir, filename) = split_dir_filename(&f.path);
        let (stem, ext) = split_stem_ext(filename);

        let name_hash = hash_fo4(stem);
        let ext_hash = hash_fo4(ext);
        let dir_hash = hash_fo4(dir);
        let offset = data_offsets[i] as i64;

        out.write_all(&name_hash.to_le_bytes())?;
        out.write_all(&ext_hash.to_le_bytes())?;
        out.write_all(&dir_hash.to_le_bytes())?;
        out.write_all(&offset.to_le_bytes())?;
        out.write_all(&f.packed_size.to_le_bytes())?;
        out.write_all(&f.uncompressed_size.to_le_bytes())?;
        out.write_all(&BAADF00D.to_le_bytes())?;
        out.write_all(&0u32.to_le_bytes())?; // 4-byte padding to fill 36-byte record
    }

    // File data.
    for f in &files {
        out.write_all(&f.stored)?;
    }

    // Name table: [u16 len][bytes] (no null terminator).
    for f in &files {
        let bytes = f.path.as_bytes();
        let len = bytes.len() as u16;
        out.write_all(&len.to_le_bytes())?;
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

    /// Verifies that a BA2 GNRL roundtrip without compression is correct.
    #[test]
    fn gnrl_uncompressed_roundtrip() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("test.ba2");
        let mut w = Ba2GnrlWriter::new(Ba2Version::V1);
        w.add("meshes/iron.nif", b"nif payload".to_vec());
        w.add("scripts/main.pex", b"pex payload".to_vec());

        // when
        w.write_to(&path)?;

        // then
        let archive = crate::open(&path)?;
        assert_eq!(archive.file_count(), 2);
        assert_eq!(
            archive
                .extract("meshes/iron.nif")
                .expect("entry exists")?
                .to_vec(),
            b"nif payload"
        );
        assert_eq!(
            archive
                .extract("scripts/main.pex")
                .expect("entry exists")?
                .to_vec(),
            b"pex payload"
        );
        Ok(())
    }

    /// Verifies that a BA2 GNRL roundtrip with zlib compression is correct.
    #[test]
    fn gnrl_compressed_roundtrip() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("compressed.ba2");
        let payload = b"Repeated data for compression. ".repeat(100);
        let mut w = Ba2GnrlWriter::new(Ba2Version::V1).compress(true);
        w.add("data/big_file.bin", payload.to_vec());

        // when
        w.write_to(&path)?;

        // then
        let archive = crate::open(&path)?;
        assert_eq!(
            archive
                .extract("data/big_file.bin")
                .expect("entry exists")?
                .to_vec(),
            payload
        );
        Ok(())
    }

    /// Verifies that writing the same BA2 GNRL archive twice produces
    /// bit-identical output.
    #[test]
    fn gnrl_write_is_deterministic() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let dir = tempfile::tempdir()?;
        let path_a = dir.path().join("a.ba2");
        let path_b = dir.path().join("b.ba2");
        let payload = b"gnrl determinism payload ".repeat(20);
        let build = || -> std::result::Result<(), Box<dyn std::error::Error>> {
            let path = if path_a.exists() { &path_b } else { &path_a };
            let mut w = Ba2GnrlWriter::new(Ba2Version::V1).compress(true);
            w.add("meshes/nif.nif", payload.to_vec());
            w.add("scripts/pex.pex", b"script_data".to_vec());
            w.write_to(path)?;
            Ok(())
        };
        build()?;
        build()?;

        // when / then — archives must be byte-identical
        let bytes_a = std::fs::read(&path_a)?;
        let bytes_b = std::fs::read(&path_b)?;
        assert_eq!(bytes_a, bytes_b, "BA2 GNRL output is not deterministic");
        Ok(())
    }
}
