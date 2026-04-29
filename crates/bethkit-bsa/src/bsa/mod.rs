// SPDX-License-Identifier: Apache-2.0
//!
//! Top-level BSA (Bethesda Softworks Archive) dispatch module.
//!
//! Exposes a single [`BsaArchive`] type that wraps any of the supported BSA
//! sub-formats (TES3, TES4/FO3/SSE) and implements the [`Archive`] trait.

pub mod flags;
pub(crate) mod tes3;
pub(crate) mod tes4;

use std::borrow::Cow;
use std::sync::Arc;

use bethkit_io::MappedFile;

use crate::archive::{Archive, ArchiveEntry};
use crate::error::{BsaError, Result};

/// Any of the supported BSA format variants.
enum Inner {
    Tes3(tes3::Tes3Archive),
    Tes4(tes4::Tes4Archive),
}

/// A BSA archive from any supported game era.
///
/// Opened with [`crate::open`] or directly via [`BsaArchive::open`].
pub struct BsaArchive {
    inner: Inner,
}

impl BsaArchive {
    /// Opens and parses a BSA archive at `path`.
    ///
    /// The format and version are auto-detected from the file magic and version
    /// field.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the BSA file on disk.
    ///
    /// # Returns
    ///
    /// A parsed [`BsaArchive`].
    ///
    /// # Errors
    ///
    /// Returns [`BsaError::InvalidMagic`] for unknown magic bytes,
    /// [`BsaError::UnsupportedVersion`] for unknown version codes, or
    /// [`BsaError::Corrupt`] / [`BsaError::Io`] for I/O and parse failures.
    pub fn open(path: &std::path::Path) -> Result<Self> {
        let mmap = MappedFile::open(path).map_err(BsaError::Io)?;
        Self::from_mmap(Arc::new(mmap))
    }

    /// Parses a BSA from an already-opened memory-mapped file.
    ///
    /// # Arguments
    ///
    /// * `source` - The memory-mapped archive file.
    ///
    /// # Errors
    ///
    /// Returns [`BsaError::InvalidMagic`], [`BsaError::UnsupportedVersion`],
    /// or [`BsaError::Corrupt`] on parse errors.
    pub fn from_mmap(source: Arc<MappedFile>) -> Result<Self> {
        let bytes = source.as_bytes();
        if bytes.len() < 8 {
            return Err(BsaError::Corrupt("file too small to be a BSA".into()));
        }
        let magic: [u8; 4] = [bytes[0], bytes[1], bytes[2], bytes[3]];

        match magic {
            tes3::MAGIC => {
                let archive = tes3::Tes3Archive::parse(source)?;
                Ok(Self {
                    inner: Inner::Tes3(archive),
                })
            }
            tes4::MAGIC => {
                let version = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
                let archive = tes4::Tes4Archive::parse(source, version)?;
                Ok(Self {
                    inner: Inner::Tes4(archive),
                })
            }
            got => Err(BsaError::InvalidMagic { got }),
        }
    }
}

impl Archive for BsaArchive {
    fn entries(&self) -> &[ArchiveEntry] {
        match &self.inner {
            Inner::Tes3(a) => a.entries(),
            Inner::Tes4(a) => a.entries(),
        }
    }

    fn extract(&self, path: &str) -> Option<Result<Cow<'_, [u8]>>> {
        match &self.inner {
            Inner::Tes3(a) => {
                let idx = a.find(path)?;
                Some(a.extract_by_index(idx))
            }
            Inner::Tes4(a) => {
                let idx = a.find(path)?;
                Some(a.extract_by_index(idx))
            }
        }
    }

    fn format_name(&self) -> &'static str {
        match &self.inner {
            Inner::Tes3(_) => "BSA TES3",
            Inner::Tes4(_) => "BSA TES4/FO3/SSE",
        }
    }
}
