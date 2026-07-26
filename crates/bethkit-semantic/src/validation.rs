// SPDX-License-Identifier: Apache-2.0
//!
//! Structured semantic validation diagnostics.

use bethkit_core::{FormId, Signature};
use bethkit_schema::SchemaNodeId;

use crate::ByteSpan;

/// Diagnostic severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    /// Informational observation.
    Information,
    /// Suspicious but recoverable condition.
    Warning,
    /// Invalid record data.
    Error,
}

/// Stable machine-readable diagnostic code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticCode {
    /// A required schema node is absent.
    MissingRequired,
    /// A subrecord appeared in an invalid order.
    InvalidOrder,
    /// A non-repeating node occurred more than once.
    UnexpectedDuplicate,
    /// A payload could not be decoded.
    InvalidPayload,
    /// A payload byte was not consumed by the schema.
    UncoveredBytes,
    /// Two decoded fields consumed the same payload bytes.
    OverlappingBytes,
    /// A FormID target has an invalid record type.
    InvalidFormIdTarget,
    /// A subrecord is unknown to the current schema.
    UnknownSubrecord,
}

/// One structured semantic validation diagnostic.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    /// Severity of the finding.
    pub severity: DiagnosticSeverity,
    /// Stable diagnostic code.
    pub code: DiagnosticCode,
    /// Human-readable context.
    pub message: String,
    /// Main-record signature.
    pub record_signature: Signature,
    /// Main-record FormID.
    pub form_id: FormId,
    /// Schema node when the finding maps to one.
    pub node_id: Option<SchemaNodeId>,
    /// Stable schema path when available.
    pub path: Option<String>,
    /// Byte span inside the affected subrecord payload.
    pub span: Option<ByteSpan>,
}

/// Collection of diagnostics produced by one validation operation.
#[derive(Debug, Default, Clone)]
pub struct ValidationReport {
    diagnostics: Vec<Diagnostic>,
}

impl ValidationReport {
    /// Creates an empty report.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a diagnostic.
    pub fn push(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    /// Returns all diagnostics in deterministic discovery order.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Returns whether the report contains an error.
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error)
    }
}
