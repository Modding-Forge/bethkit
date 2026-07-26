// SPDX-License-Identifier: Apache-2.0
//!
//! Errors produced by semantic decoding, editing, and analysis.

/// Errors produced by `bethkit-semantic`.
#[derive(Debug, thiserror::Error)]
pub enum SemanticError {
    /// The raw plugin layer rejected an operation.
    #[error("plugin operation failed: {0}")]
    Core(#[from] bethkit_core::CoreError),

    /// The schema layer rejected an operation.
    #[error("schema operation failed: {0}")]
    Schema(#[from] bethkit_schema::SchemaError),

    /// No schema exists for a record signature.
    #[error("no schema exists for record signature {0}")]
    MissingRecordSchema(String),

    /// A package requires a custom decoder that is not registered.
    #[error("required custom decoder is unavailable: {0}")]
    MissingDecoder(String),

    /// A custom decoder rejected its payload.
    #[error("custom decoder {decoder} failed: {message}")]
    Decoder {
        /// Stable decoder identifier.
        decoder: String,
        /// Decoder-provided error context.
        message: String,
    },

    /// A schema field could not be decoded from its bytes.
    #[error("field {path} could not be decoded: {message}")]
    Decode {
        /// Stable schema path.
        path: String,
        /// Decoding error context.
        message: String,
    },

    /// A requested schema path does not exist.
    #[error("schema path does not exist: {0}")]
    MissingPath(String),

    /// A requested field occurrence does not exist.
    #[error("field occurrence {occurrence} does not exist for {path}")]
    MissingOccurrence {
        /// Stable schema path.
        path: String,
        /// Zero-based occurrence.
        occurrence: usize,
    },

    /// A value cannot be encoded for its schema type.
    #[error("field {path} cannot be encoded: {message}")]
    Encode {
        /// Stable schema path.
        path: String,
        /// Encoding error context.
        message: String,
    },
}

/// Convenience result type for semantic operations.
pub type Result<T> = std::result::Result<T, SemanticError>;
