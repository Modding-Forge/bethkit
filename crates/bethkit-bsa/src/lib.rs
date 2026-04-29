// SPDX-License-Identifier: Apache-2.0
//!
//! `bethkit-bsa` — BSA and BA2 archive reader and writer.
//!
//! Supports the following archive formats:
//! - **BSA TES3**: Morrowind (magic `\x00\x01\x00\x00`) — read + write
//! - **BSA TES4**: Oblivion (version `0x67`) — read + write
//! - **BSA FO3**: Fallout 3 / New Vegas / Skyrim LE (version `0x68`) — read + write
//! - **BSA SSE**: Skyrim SE / AE (version `0x69`, LZ4 Frame) — read + write
//! - **BA2 GNRL**: Fallout 4 general files (versions 1, 7, 8) — read + write
//! - **BA2 DX10**: Fallout 4 textures with full DDS reassembly — read + write

pub mod archive;
pub mod ba2;
pub mod bsa;
pub mod error;
pub mod hash;
pub mod write;

pub use archive::{Archive, ArchiveEntry};
pub use ba2::Ba2Archive;
pub use bsa::BsaArchive;
pub use error::{BsaError, Result};
pub use write::{Ba2Dx10Writer, Ba2GnrlWriter, Ba2Version, BsaVersion, BsaWriter};

/// Opens a BSA or BA2 archive at `path`, auto-detecting the format.
///
/// Returns a boxed [`Archive`] trait object so the caller does not need to
/// know which concrete type was opened.
///
/// # Arguments
///
/// * `path` - Path to the archive file on disk.
///
/// # Returns
///
/// A boxed `dyn Archive` providing access to all contained files.
///
/// # Errors
///
/// Returns [`BsaError::InvalidMagic`] if the file cannot be identified as a
/// supported archive format, or any other [`BsaError`] variant on parse or
/// I/O failure.
pub fn open(path: &std::path::Path) -> Result<Box<dyn Archive>> {
    let bytes = {
        use std::fs::File;
        use std::io::Read;
        let mut buf = [0u8; 4];
        let mut f = File::open(path).map_err(|e| BsaError::Io(bethkit_io::IoError::Io(e)))?;
        f.read_exact(&mut buf)
            .map_err(|e| BsaError::Io(bethkit_io::IoError::Io(e)))?;
        buf
    };

    match bytes {
        bsa::tes3::MAGIC | bsa::tes4::MAGIC => Ok(Box::new(BsaArchive::open(path)?)),
        ba2::MAGIC => Ok(Box::new(Ba2Archive::open(path)?)),
        got => Err(BsaError::InvalidMagic { got }),
    }
}
