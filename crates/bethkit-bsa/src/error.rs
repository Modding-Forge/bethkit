// SPDX-License-Identifier: Apache-2.0
//!
//! Error types for the `bethkit-bsa` crate.

use bethkit_io::IoError;

/// All errors that can occur when opening or reading a BSA or BA2 archive.
#[derive(Debug, thiserror::Error)]
pub enum BsaError {
    /// An I/O or decompression error from the `bethkit-io` layer.
    #[error("I/O error: {0}")]
    Io(#[from] IoError),

    /// The file does not begin with a recognised archive magic number.
    #[error("invalid archive magic: {got:?}")]
    InvalidMagic {
        /// The four bytes that were found.
        got: [u8; 4],
    },

    /// The archive declares a version not supported by this implementation.
    #[error("unsupported archive version: {version:#x}")]
    UnsupportedVersion {
        /// The raw version field read from the file.
        version: u32,
    },

    /// The BA2 archive declares a sub-type not supported by this implementation.
    #[error("unsupported BA2 sub-type: {sub_type:?}")]
    UnsupportedSubType {
        /// The four-byte sub-type magic read from the file.
        sub_type: [u8; 4],
    },

    /// The archive header or data structure is internally inconsistent.
    #[error("corrupt archive: {0}")]
    Corrupt(String),

    /// Attempted to write an archive that contains no files.
    #[error("cannot write an empty archive")]
    EmptyArchive,

    /// The DDS file provided to the BA2 DX10 writer is malformed or uses an
    /// unsupported pixel format.
    #[error("invalid DDS input: {0}")]
    InvalidDds(String),

    /// The DXGI format stored in the DDS file is not supported by the BA2 DX10
    /// writer.
    #[error("unsupported DXGI format: {format}")]
    UnsupportedDxgiFormat {
        /// The raw DXGI format value.
        format: u8,
    },

    /// A plain [`std::io::Error`] from the standard library, produced during
    /// archive writing.
    #[error("I/O write error: {0}")]
    WriteIo(#[from] std::io::Error),
}

/// Alias so callers can write `bsa::Result<T>` instead of
/// `std::result::Result<T, bsa::BsaError>`.
pub type Result<T> = std::result::Result<T, BsaError>;
