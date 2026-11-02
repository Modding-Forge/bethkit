// SPDX-License-Identifier: Apache-2.0
//!
//! Lexing and parsing of Papyrus source files.
//!
//! [`parse`] turns the bytes of a `.psc` file into a [`ParsedScript`]. Parsing
//! always succeeds for sources below 4 GiB: syntax errors are collected as
//! diagnostics and the tree contains everything that could be recovered.

pub mod ast;
mod dialect;
mod interner;
mod lexer;
mod parser;
mod token;

pub use dialect::Dialect;
pub use interner::{Interner, Symbol};
pub use token::{Keyword, Token, TokenKind};

use crate::diagnostics::{Diagnostic, Severity};
use crate::error::{PapyrusError, Result};
use ast::{Ast, Ident, Script};

/// The result of parsing one script.
#[derive(Clone, Debug)]
pub struct ParsedScript {
    dialect: Dialect,
    script: Script,
    ast: Ast,
    interner: Interner,
    diagnostics: Vec<Diagnostic>,
}

impl ParsedScript {
    /// Returns the dialect the script was parsed with.
    pub fn dialect(&self) -> Dialect {
        self.dialect
    }

    /// Returns the declarations of the script.
    pub fn script(&self) -> &Script {
        &self.script
    }

    /// Returns the expression and statement arena referenced by the script.
    pub fn ast(&self) -> &Ast {
        &self.ast
    }

    /// Returns the interner holding every name in the script.
    pub fn interner(&self) -> &Interner {
        &self.interner
    }

    /// Returns the spelling of `ident` as first written in the script.
    pub fn name(&self, ident: Ident) -> &str {
        self.interner.resolve(ident.symbol)
    }

    /// Returns all lexer and parser diagnostics in source order.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Returns `true` if any diagnostic is an error.
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity() == Severity::Error)
    }
}

/// Parses the bytes of a Papyrus source file.
///
/// The source may use any 8-bit encoding for strings and comments; an UTF-8
/// byte order mark is skipped.
///
/// # Errors
///
/// Returns [`PapyrusError::SourceTooLarge`] if `source` is longer than
/// `u32::MAX` bytes. Syntax errors are reported through
/// [`ParsedScript::diagnostics`] instead.
pub fn parse(source: &[u8], dialect: Dialect) -> Result<ParsedScript> {
    let max: usize = usize::try_from(u32::MAX).unwrap_or(usize::MAX);
    if source.len() > max {
        return Err(PapyrusError::SourceTooLarge {
            len: source.len(),
            max,
        });
    }
    let lexed = lexer::tokenize(source, dialect);
    let mut output = parser::parse_tokens(source, lexed.tokens, dialect, lexed.diagnostics);
    output
        .diagnostics
        .sort_by_key(|diagnostic| (diagnostic.span().start(), diagnostic.span().end()));
    Ok(ParsedScript {
        dialect,
        script: output.script,
        ast: output.ast,
        interner: output.interner,
        diagnostics: output.diagnostics,
    })
}
