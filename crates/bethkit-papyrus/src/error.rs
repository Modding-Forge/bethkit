// SPDX-License-Identifier: Apache-2.0
//!
//! Error types for the `bethkit-papyrus` crate.
//!
//! Problems inside a source file are reported as [`crate::Diagnostic`]s, not as
//! errors, so that a single run reports every issue. [`PapyrusError`] covers
//! the cases where no meaningful result can be produced at all.

/// Errors that prevent a Papyrus operation from producing a result.
#[derive(Debug, thiserror::Error)]
pub enum PapyrusError {
    /// The source is too large to be addressed with 32-bit offsets.
    #[error("source of {len} bytes exceeds the maximum of {max} bytes")]
    SourceTooLarge {
        /// Length of the rejected source in bytes.
        len: usize,
        /// Largest supported source length in bytes.
        max: usize,
    },
}

/// Convenience alias for `Result<T, PapyrusError>`.
pub type Result<T> = std::result::Result<T, PapyrusError>;
