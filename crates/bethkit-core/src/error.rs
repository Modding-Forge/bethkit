// SPDX-License-Identifier: Apache-2.0
//!
//! Error types for the `bethkit-core` crate.

use crate::types::Game;

/// All errors that can be produced by `bethkit-core` operations.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// An I/O or decompression error from the underlying `bethkit-io` layer.
    #[error("I/O error: {0}")]
    Io(#[from] bethkit_io::IoError),

    /// The record or group header contained an unexpected signature.
    #[error("Invalid header signature: expected {expected}, got {got}")]
    InvalidSignature { expected: String, got: String },

    /// A GRUP header contained an unknown or invalid group type value.
    #[error("Invalid GRUP type: {0}")]
    InvalidGroupType(i32),

    /// The parser ran out of data while reading a named structure.
    #[error("Unexpected end of file while parsing {context}")]
    UnexpectedEof { context: &'static str },

    /// A FormID in a light plugin exceeded the allowed object-id range.
    #[error("FormID {0:#010X} exceeds light plugin limit (max 0xFFF)")]
    LightFormIdOverflow(u32),

    /// Attempting to eslify a plugin that has too many records.
    #[error("ESL plugin would require {count} records, max is 2048")]
    EslRecordLimitExceeded { count: usize },

    /// An operation is not supported for the given game.
    #[error("Unsupported game for this operation: {0:?}")]
    UnsupportedGame(Game),

    /// A localisation file (`.STRINGS` / `.DLSTRINGS` / `.ILSTRINGS`) is
    /// malformed or could not be classified.
    #[error("Invalid string table: {0}")]
    InvalidStringTable(String),

    /// A localised record references an lstring identifier that is missing
    /// from all loaded string tables.
    #[error("Missing lstring identifier {0:#010X} in loaded string tables")]
    MissingLStringId(u32),

    /// A plugin marked as localised was opened without any string tables
    /// supplied.
    #[error("Plugin is localised but no string tables were provided")]
    LocalizedFlagWithoutTables,

    /// A field could not be decoded because its byte content is not valid
    /// for the expected encoding.
    #[error("Invalid field encoding: {0}")]
    InvalidEncoding(String),
}

/// Convenience alias for `Result<T, CoreError>`.
pub type Result<T> = std::result::Result<T, CoreError>;
