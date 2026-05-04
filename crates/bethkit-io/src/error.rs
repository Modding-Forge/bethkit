// SPDX-License-Identifier: Apache-2.0
//!
//! Error types for the `bethkit-io` crate.

/// All errors that can be produced by `bethkit-io` operations.
#[derive(Debug, thiserror::Error)]
pub enum IoError {
    /// An underlying operating-system I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// The cursor reached the end of the data before the read could complete.
    #[error("Unexpected end of data at offset {offset}")]
    UnexpectedEof { offset: usize },

    /// A decompression operation failed.
    #[error("Decompression failed: {0}")]
    Decompress(String),

    /// Arithmetic overflow computing the end offset of a read operation.
    ///
    /// This can only occur when `offset + len` wraps around `usize::MAX`,
    /// which indicates malformed or adversarial input.
    #[error("offset overflow at offset {offset} adding {len} bytes")]
    OffsetOverflow { offset: usize, len: usize },
}

/// Convenience alias for `Result<T, IoError>`.
pub type Result<T> = std::result::Result<T, IoError>;
