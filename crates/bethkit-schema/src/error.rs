// SPDX-License-Identifier: Apache-2.0
//!
//! Errors produced while loading, validating, or evaluating schemas.

/// Errors produced by `bethkit-schema`.
#[derive(Debug, thiserror::Error)]
pub enum SchemaError {
    /// File-system access failed.
    #[error("schema I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON input could not be decoded.
    #[error("invalid schema JSON: {0}")]
    Json(#[from] serde_json::Error),

    /// CBOR input could not be decoded.
    #[error("invalid schema CBOR: {0}")]
    CborDecode(String),

    /// CBOR output could not be encoded.
    #[error("schema CBOR encoding failed: {0}")]
    CborEncode(String),

    /// The package or catalog magic header is invalid.
    #[error("invalid schema magic header")]
    InvalidMagic,

    /// The package format is not supported by this runtime.
    #[error("unsupported schema format version {0}")]
    UnsupportedVersion(u16),

    /// The package exceeds a configured safety limit.
    #[error("schema limit exceeded: {0}")]
    LimitExceeded(String),

    /// The package digest does not match its payload.
    #[error("schema payload hash mismatch")]
    HashMismatch,

    /// The package graph is structurally invalid.
    #[error("invalid schema graph: {0}")]
    InvalidGraph(String),

    /// A requested game package is absent.
    #[error("schema package is unavailable for {0}")]
    MissingGame(String),

    /// No schema bundle was embedded in this build.
    #[error("this build has no embedded schema catalog; set BETHKIT_SCHEMA_BUNDLE at build time")]
    EmbeddedUnavailable,

    /// An expression is invalid for its current evaluation context.
    #[error("schema expression failed: {0}")]
    Expression(String),

    /// One or more exported semantic callbacks have no checked rule.
    #[error("unclassified xEdit callbacks: {0}")]
    UnclassifiedCallbacks(String),
}

/// Convenience result type for schema operations.
pub type Result<T> = std::result::Result<T, SchemaError>;
