// SPDX-License-Identifier: Apache-2.0
//!
//! Source locations and diagnostics shared by every Papyrus module.
//!
//! Locations are byte offsets ([`Span`]) so they stay cheap to store and
//! compare; [`LineIndex`] converts them to line and column numbers only when a
//! diagnostic is displayed.

use std::fmt;
use std::ops::Range;

/// A half-open byte range `start..end` inside one source file.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Span {
    start: u32,
    end: u32,
}

impl Span {
    /// Creates a span from `start` to `end`; the bounds are swapped if
    /// `end < start`.
    pub fn new(start: u32, end: u32) -> Self {
        if end < start {
            Self {
                start: end,
                end: start,
            }
        } else {
            Self { start, end }
        }
    }

    /// Returns the first byte offset covered by the span.
    pub fn start(self) -> u32 {
        self.start
    }

    /// Returns the offset one past the last covered byte.
    pub fn end(self) -> u32 {
        self.end
    }

    /// Returns the number of covered bytes.
    pub fn len(self) -> u32 {
        self.end - self.start
    }

    /// Returns `true` if the span covers no bytes.
    pub fn is_empty(self) -> bool {
        self.start == self.end
    }

    /// Returns the smallest span covering both `self` and `other`.
    pub fn to(self, other: Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }

    /// Returns the span as a `usize` range for slicing the source.
    pub fn range(self) -> Range<usize> {
        // NOTE: u32 always fits into usize on the 32- and 64-bit targets bethkit supports.
        self.start as usize..self.end as usize
    }
}

/// How serious a [`Diagnostic`] is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Severity {
    /// The input is invalid; results derived from it are incomplete.
    Error,
    /// The input is accepted but likely not what the author intended.
    Warning,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Error => f.write_str("error"),
            Self::Warning => f.write_str("warning"),
        }
    }
}

/// Stable identifiers for every kind of diagnostic.
///
/// The codes returned by [`DiagnosticCode::code`] never change meaning, so
/// tools can filter or suppress diagnostics by code.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DiagnosticCode {
    /// A byte that cannot start any token.
    UnexpectedCharacter,
    /// A string literal without a closing quote on the same line.
    UnterminatedString,
    /// An unknown escape sequence inside a string literal.
    InvalidEscape,
    /// A `;/` block comment without a closing `/;`.
    UnterminatedComment,
    /// A `{` documentation comment without a closing `}`.
    UnterminatedDocComment,
    /// A malformed numeric literal.
    InvalidNumber,
    /// A `\` line continuation that is not followed by a line break.
    InvalidLineContinuation,
    /// A token that is not valid at its position.
    UnexpectedToken,
    /// The script has no `ScriptName` header.
    MissingScriptName,
    /// `ScriptName` appears after other declarations or more than once.
    MisplacedScriptName,
    /// A block reached the end of the file without its closing keyword.
    MissingEnd,
    /// A block was closed with the wrong keyword.
    MismatchedEnd,
    /// An integer literal outside the 32-bit range.
    IntegerOverflow,
    /// The left side of an assignment is not assignable.
    InvalidAssignmentTarget,
    /// A function inside a full property is not a valid `Get` or `Set`.
    InvalidPropertyAccessor,
    /// A full property defines `Get` or `Set` more than once.
    DuplicatePropertyAccessor,
    /// Expressions or blocks are nested deeper than the parser allows.
    NestingTooDeep,
    /// Syntax that exists in another game's dialect but not in this one.
    UnsupportedSyntax,
    /// A flag file entry with an index outside `0..=31`.
    InvalidFlagIndex,
    /// A flag file target that is not a known declaration kind.
    UnknownFlagTarget,
    /// A flag file defines the same flag name more than once.
    DuplicateFlag,
    /// A composite flag refers to a flag that is not defined before it, or
    /// combines fewer than two flags.
    UnknownFlag,
}

impl DiagnosticCode {
    /// Returns the stable textual code, for example `PAP0001`.
    pub fn code(self) -> &'static str {
        match self {
            Self::UnexpectedCharacter => "PAP0001",
            Self::UnterminatedString => "PAP0002",
            Self::InvalidEscape => "PAP0003",
            Self::UnterminatedComment => "PAP0004",
            Self::UnterminatedDocComment => "PAP0005",
            Self::InvalidNumber => "PAP0006",
            Self::InvalidLineContinuation => "PAP0007",
            Self::UnexpectedToken => "PAP0100",
            Self::MissingScriptName => "PAP0101",
            Self::MisplacedScriptName => "PAP0102",
            Self::MissingEnd => "PAP0103",
            Self::MismatchedEnd => "PAP0104",
            Self::IntegerOverflow => "PAP0105",
            Self::InvalidAssignmentTarget => "PAP0106",
            Self::InvalidPropertyAccessor => "PAP0107",
            Self::DuplicatePropertyAccessor => "PAP0108",
            Self::NestingTooDeep => "PAP0109",
            Self::UnsupportedSyntax => "PAP0110",
            Self::InvalidFlagIndex => "PAP0200",
            Self::UnknownFlagTarget => "PAP0201",
            Self::DuplicateFlag => "PAP0202",
            Self::UnknownFlag => "PAP0203",
        }
    }
}

impl fmt::Display for DiagnosticCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

/// A problem found in a source file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    severity: Severity,
    code: DiagnosticCode,
    message: String,
    span: Span,
}

impl Diagnostic {
    /// Creates an error diagnostic.
    pub fn error(code: DiagnosticCode, span: Span, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            code,
            message: message.into(),
            span,
        }
    }

    /// Creates a warning diagnostic.
    pub fn warning(code: DiagnosticCode, span: Span, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Warning,
            code,
            message: message.into(),
            span,
        }
    }

    /// Returns how serious the diagnostic is.
    pub fn severity(&self) -> Severity {
        self.severity
    }

    /// Returns the kind of the diagnostic.
    pub fn code(&self) -> DiagnosticCode {
        self.code
    }

    /// Returns the human-readable description.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns the source location the diagnostic refers to.
    pub fn span(&self) -> Span {
        self.span
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}[{}] at {}..{}: {}",
            self.severity, self.code, self.span.start, self.span.end, self.message
        )
    }
}

/// Maps byte offsets of one source file to 1-based line and column numbers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineIndex {
    line_starts: Vec<u32>,
}

impl LineIndex {
    /// Builds the index for `source`; only `\n` starts a new line.
    ///
    /// Offsets beyond `u32::MAX` are never produced by this crate, so sources
    /// longer than that are indexed only up to that limit.
    pub fn new(source: &[u8]) -> Self {
        let mut line_starts: Vec<u32> = Vec::with_capacity(source.len() / 32 + 1);
        line_starts.push(0);
        line_starts.extend(
            source
                .iter()
                .enumerate()
                .filter(|(_, byte)| **byte == b'\n')
                .filter_map(|(index, _)| u32::try_from(index + 1).ok()),
        );
        Self { line_starts }
    }

    /// Returns the 1-based line and 1-based byte column of `offset`.
    pub fn line_col(&self, offset: u32) -> (u32, u32) {
        let line: usize = match self.line_starts.binary_search(&offset) {
            Ok(line) => line,
            Err(next) => next.saturating_sub(1),
        };
        let start: u32 = self.line_starts.get(line).copied().unwrap_or(0);
        let line_number: u32 = u32::try_from(line).unwrap_or(u32::MAX).saturating_add(1);
        (line_number, offset - start + 1)
    }

    /// Returns the number of lines in the source.
    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that offsets map to the expected line and column.
    #[test]
    fn line_index_maps_offsets() -> Result<(), Box<dyn std::error::Error>> {
        // given
        let index = LineIndex::new(b"ab\ncd\r\n\nx");

        // then
        assert_eq!(index.line_col(0), (1, 1));
        assert_eq!(index.line_col(2), (1, 3));
        assert_eq!(index.line_col(3), (2, 1));
        assert_eq!(index.line_col(8), (4, 1));
        assert_eq!(index.line_count(), 4);
        Ok(())
    }

    /// Verifies span helpers and diagnostic rendering.
    #[test]
    fn spans_and_diagnostics() -> Result<(), Box<dyn std::error::Error>> {
        // given
        let span = Span::new(9, 4).to(Span::new(2, 3));
        let diagnostic = Diagnostic::warning(DiagnosticCode::DuplicateFlag, span, "dup");

        // then
        assert_eq!(span.range(), 2..9);
        assert_eq!(span.len(), 7);
        assert_eq!(diagnostic.to_string(), "warning[PAP0202] at 2..9: dup");
        Ok(())
    }
}
