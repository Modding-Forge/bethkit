// SPDX-License-Identifier: Apache-2.0
//!
//! Papyrus language support for bethkit.
//!
//! This crate contains the source-level half of bethkit's Papyrus support: a
//! lexer and an error-recovering parser for `.psc` scripts in the dialects of
//! Skyrim, Fallout 4, Fallout 76 and Starfield, the abstract syntax tree they
//! produce, and a parser for `.flg` flag definition files.
//!
//! Sources are read as raw bytes. Papyrus identifiers and keywords are ASCII,
//! while string literals and comments may be in any 8-bit code page, so string
//! literal values are kept as bytes and never re-encoded.
//!
//! ```
//! use bethkit_papyrus::{parse, Dialect};
//!
//! let parsed = parse(b"ScriptName Hello\nFunction Greet()\nEndFunction\n", Dialect::Skyrim)?;
//! assert!(!parsed.has_errors());
//! # Ok::<(), bethkit_papyrus::PapyrusError>(())
//! ```

mod diagnostics;
mod error;
mod flags;
pub mod syntax;

pub use diagnostics::{Diagnostic, DiagnosticCode, LineIndex, Severity, Span};
pub use error::{PapyrusError, Result};
pub use flags::{FlagDefinition, FlagFile, FlagTargets};
pub use syntax::{parse, Dialect, ParsedScript};
