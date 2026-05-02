// SPDX-License-Identifier: Apache-2.0
//! Writer API for creating BSA and BA2 archives.
//!
//! Each writer follows a builder pattern:
//! 1. Create a writer via [`BsaWriter::new`] / [`Ba2GnrlWriter::new`] /
//!    [`Ba2Dx10Writer::new`].
//! 2. Add files with the respective `add` method.
//! 3. Call `write_to` to produce the archive on disk.
//!
//! # Parallel compression
//!
//! The TES4 / FO3 / SSE (`bsa_tes4`), BA2 GNRL (`ba2_gnrl`), and BA2 DX10
//! (`ba2_dx10`) writers compress individual files in parallel using
//! [`rayon`].  Output is deterministic: the BSA variants sort by hash before
//! writing, and BA2 variants preserve the insertion order (rayon's collect
//! preserves order).  All compression functions are pure and thread-safe.
//!
//! # Example
//!
//! ```no_run
//! use bethkit_bsa::write::{BsaWriter, BsaVersion};
//!
//! let mut w = BsaWriter::new(BsaVersion::Sse);
//! w.add("meshes/armor/iron.nif", b"NIF data".to_vec());
//! w.write_to(std::path::Path::new("output.bsa")).unwrap();
//! ```

pub mod dds_parse;

mod ba2_dx10;
mod ba2_gnrl;
mod bsa_tes3;
mod bsa_tes4;

pub use ba2_dx10::Ba2Dx10Writer;
pub use ba2_gnrl::Ba2GnrlWriter;

use std::path::Path;

use crate::error::Result;

/// BSA format version selector for [`BsaWriter`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BsaVersion {
    /// Morrowind BSA (flat file list, no compression).
    Tes3,
    /// Oblivion BSA (version `0x67`, zlib compression).
    Tes4,
    /// Fallout 3 / New Vegas / Skyrim LE BSA (version `0x68`, zlib).
    Fo3,
    /// Skyrim SE / AE BSA (version `0x69`, LZ4 Frame compression).
    Sse,
}

/// BA2 format version selector for [`Ba2GnrlWriter`] and [`Ba2Dx10Writer`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ba2Version {
    /// Fallout 4 original (version 1).
    V1,
    /// Fallout 4 NG (version 7).
    V7,
    /// Fallout 4 NG alternate (version 8).
    V8,
}

impl Ba2Version {
    /// Returns the raw u32 version number written to the archive header.
    pub(crate) fn as_u32(self) -> u32 {
        match self {
            Self::V1 => 1,
            Self::V7 => 7,
            Self::V8 => 8,
        }
    }
}

/// A single file pending insertion into an archive.
#[derive(Debug, Clone)]
pub struct BuilderEntry {
    /// Normalised virtual path (lowercase, forward slashes).
    pub(crate) path: String,
    /// Uncompressed file content.
    pub(crate) data: Vec<u8>,
    /// Per-file compression override.
    ///
    /// `Some(true)` forces compression even when the archive default is off,
    /// and vice versa.  `None` inherits the archive-level default.
    pub(crate) compress_override: Option<bool>,
}

/// Builder for BSA archives (all versions: TES3, TES4, FO3, SSE).
///
/// # Usage
///
/// ```no_run
/// use bethkit_bsa::write::{BsaWriter, BsaVersion};
///
/// let mut w = BsaWriter::new(BsaVersion::Sse).compress(true);
/// w.add("textures/sky.dds", vec![0u8; 128]);
/// w.write_to(std::path::Path::new("sky.bsa")).unwrap();
/// ```
pub struct BsaWriter {
    version: BsaVersion,
    /// Archive-wide default compression.  Ignored for [`BsaVersion::Tes3`].
    compress: bool,
    /// Embed a file name prefix before each file's stored data.
    /// Applies to FO3 and SSE only; ignored for TES3 and TES4.
    embed_names: bool,
    entries: Vec<BuilderEntry>,
}

impl BsaWriter {
    /// Creates a new, empty BSA writer.
    ///
    /// Default settings:
    /// - `compress = false`
    /// - `embed_names = true` for FO3 and SSE; `false` otherwise
    ///
    /// # Arguments
    ///
    /// * `version` - Which BSA variant to produce.
    ///
    /// # Returns
    ///
    /// An empty [`BsaWriter`].
    pub fn new(version: BsaVersion) -> Self {
        let embed_names = matches!(version, BsaVersion::Fo3 | BsaVersion::Sse);
        Self {
            version,
            compress: false,
            embed_names,
            entries: Vec::new(),
        }
    }

    /// Sets the archive-wide default compression flag.
    ///
    /// Has no effect for [`BsaVersion::Tes3`] (TES3 does not support
    /// compression).
    ///
    /// # Arguments
    ///
    /// * `compress` - `true` to compress all files by default.
    ///
    /// # Returns
    ///
    /// `self` for chaining.
    pub fn compress(mut self, compress: bool) -> Self {
        self.compress = compress;
        self
    }

    /// Enables or disables embedded file names before file data.
    ///
    /// Defaults to `true` for FO3 and SSE; `false` for TES3 and TES4.
    /// Setting this on TES3 or TES4 has no effect on the output.
    ///
    /// # Arguments
    ///
    /// * `embed` - `true` to embed names.
    ///
    /// # Returns
    ///
    /// `self` for chaining.
    pub fn embed_names(mut self, embed: bool) -> Self {
        self.embed_names = embed;
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
        self.entries.push(BuilderEntry {
            path,
            data,
            compress_override: None,
        });
    }

    /// Writes all pending files as a BSA archive to `dest`.
    ///
    /// # Arguments
    ///
    /// * `dest` - Output file path.  Created or truncated.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::BsaError::EmptyArchive`] when no files have
    /// been added, or a write / compression error otherwise.
    pub fn write_to(self, dest: &Path) -> Result<()> {
        match self.version {
            BsaVersion::Tes3 => bsa_tes3::write(dest, self.entries),
            BsaVersion::Tes4 | BsaVersion::Fo3 | BsaVersion::Sse => bsa_tes4::write(
                dest,
                self.version,
                self.compress,
                self.embed_names,
                self.entries,
            ),
        }
    }
}
