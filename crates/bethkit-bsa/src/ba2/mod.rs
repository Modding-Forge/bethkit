// SPDX-License-Identifier: Apache-2.0
//!
//! Top-level BA2 (Bethesda Archive 2) dispatch module.
//!
//! Exposes a single [`Ba2Archive`] type that wraps either a GNRL (general
//! files) or a DX10 (textures) BA2 variant and implements the [`Archive`]
//! trait.

pub(crate) mod dds;
pub(crate) mod dx10;
pub(crate) mod gnrl;

use std::borrow::Cow;
use std::sync::Arc;

use bethkit_io::MappedFile;

use crate::archive::{Archive, ArchiveEntry};
use crate::error::{BsaError, Result};

/// BA2 magic bytes (`"BTDX"`).
pub const MAGIC: [u8; 4] = *b"BTDX";

/// Supported BA2 version codes (stored after the `"BTDX"` magic).
pub mod version {
    /// Fallout 4 BA2.
    pub const FO4_V1: u32 = 0x01;
    /// Fallout 4 Next-Gen BA2.
    pub const FO4_NG_V7: u32 = 0x07;
    /// Fallout 4 Next-Gen BA2 (alternate).
    pub const FO4_NG_V8: u32 = 0x08;
}

/// Sub-type magic bytes stored after the version field.
const SUB_GNRL: [u8; 4] = *b"GNRL";
/// Sub-type for DX10 texture archives.
const SUB_DX10: [u8; 4] = *b"DX10";

/// Inner dispatch for supported BA2 sub-types.
enum Inner {
    Gnrl(gnrl::GnrlArchive),
    Dx10(dx10::Dx10Archive),
}

/// A BA2 archive from Fallout 4 or Starfield.
///
/// Opened with [`crate::open`] or directly via [`Ba2Archive::open`].
pub struct Ba2Archive {
    inner: Inner,
}

impl Ba2Archive {
    /// Opens and parses a BA2 archive at `path`.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the `.ba2` file on disk.
    ///
    /// # Returns
    ///
    /// A parsed [`Ba2Archive`].
    ///
    /// # Errors
    ///
    /// Returns [`BsaError::InvalidMagic`], [`BsaError::UnsupportedVersion`],
    /// [`BsaError::UnsupportedSubType`], or I/O / parse errors.
    pub fn open(path: &std::path::Path) -> Result<Self> {
        let mmap = MappedFile::open(path).map_err(BsaError::Io)?;
        Self::from_mmap(Arc::new(mmap))
    }

    /// Parses a BA2 from an already-opened memory-mapped file.
    ///
    /// # Arguments
    ///
    /// * `source` - The memory-mapped archive file.
    ///
    /// # Errors
    ///
    /// Returns [`BsaError::InvalidMagic`], [`BsaError::UnsupportedVersion`],
    /// [`BsaError::UnsupportedSubType`], or [`BsaError::Corrupt`].
    pub fn from_mmap(source: Arc<MappedFile>) -> Result<Self> {
        let bytes = source.as_bytes();
        if bytes.len() < 24 {
            return Err(BsaError::Corrupt("file too small to be a BA2".into()));
        }

        let magic: [u8; 4] = bytes[0..4]
            .try_into()
            .map_err(|_| BsaError::Corrupt("header magic read failed".into()))?;
        if magic != MAGIC {
            return Err(BsaError::InvalidMagic { got: magic });
        }

        let raw_version = u32::from_le_bytes(
            bytes[4..8]
                .try_into()
                .map_err(|_| BsaError::Corrupt("header version read failed".into()))?,
        );
        match raw_version {
            version::FO4_V1 | version::FO4_NG_V7 | version::FO4_NG_V8 => {}
            v => return Err(BsaError::UnsupportedVersion { version: v }),
        }

        let sub_type: [u8; 4] = bytes[8..12]
            .try_into()
            .map_err(|_| BsaError::Corrupt("header sub-type read failed".into()))?;
        let file_count = u32::from_le_bytes(
            bytes[12..16]
                .try_into()
                .map_err(|_| BsaError::Corrupt("header file-count read failed".into()))?,
        );
        let table_offset = i64::from_le_bytes(
            bytes[16..24]
                .try_into()
                .map_err(|_| BsaError::Corrupt("header table-offset read failed".into()))?,
        ) as u64;

        let inner = match sub_type {
            SUB_GNRL => Inner::Gnrl(gnrl::GnrlArchive::parse(source, file_count, table_offset)?),
            SUB_DX10 => Inner::Dx10(dx10::Dx10Archive::parse(source, file_count, table_offset)?),
            got => return Err(BsaError::UnsupportedSubType { sub_type: got }),
        };

        Ok(Self { inner })
    }
}

impl Archive for Ba2Archive {
    fn entries(&self) -> &[ArchiveEntry] {
        match &self.inner {
            Inner::Gnrl(a) => a.entries(),
            Inner::Dx10(a) => a.entries(),
        }
    }

    fn extract(&self, path: &str) -> Option<Result<Cow<'_, [u8]>>> {
        match &self.inner {
            Inner::Gnrl(a) => {
                let idx = a.find(path)?;
                Some(a.extract_by_index(idx))
            }
            Inner::Dx10(a) => {
                let idx = a.find(path)?;
                Some(a.extract_by_index(idx))
            }
        }
    }

    fn format_name(&self) -> &'static str {
        match &self.inner {
            Inner::Gnrl(_) => "BA2 GNRL",
            Inner::Dx10(_) => "BA2 DX10",
        }
    }
}
