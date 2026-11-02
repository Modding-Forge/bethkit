// SPDX-License-Identifier: Apache-2.0
//!
//! Parser for Papyrus flag definition files (`.flg`).
//!
//! A flag file assigns user flag names such as `Hidden` or `Conditional` to
//! bit indices and restricts where they may be used:
//!
//! ```text
//! /* block comment */
//! // line comment
//! Flag Hidden 0
//! {
//!     Script
//!     Property
//! }
//! Flag Conditional 1
//! ```
//!
//! A flag without a target list is allowed everywhere. More than one index
//! may be given; the flag then sets every listed bit.

use bitflags::bitflags;

use crate::diagnostics::{Diagnostic, DiagnosticCode, Severity, Span};
use crate::error::{PapyrusError, Result};

bitflags! {
    /// The declaration kinds a user flag may be applied to.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct FlagTargets: u8 {
        /// The script header.
        const SCRIPT = 1 << 0;
        /// Properties.
        const PROPERTY = 1 << 1;
        /// Script variables.
        const VARIABLE = 1 << 2;
        /// Functions and events.
        const FUNCTION = 1 << 3;
        /// Property groups (Fallout 4 and later).
        const GROUP = 1 << 4;
        /// Struct members (Fallout 4 and later).
        const STRUCT_VAR = 1 << 5;
        /// Guards (Starfield).
        const GUARD = 1 << 6;
    }
}

impl FlagTargets {
    fn from_keyword(name: &[u8]) -> Option<Self> {
        let target: FlagTargets = match name.to_ascii_lowercase().as_slice() {
            b"script" => Self::SCRIPT,
            b"property" => Self::PROPERTY,
            b"variable" => Self::VARIABLE,
            b"function" => Self::FUNCTION,
            b"group" => Self::GROUP,
            b"structvar" => Self::STRUCT_VAR,
            b"guard" => Self::GUARD,
            _ => return None,
        };
        Some(target)
    }
}

/// One `Flag` entry of a flag file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlagDefinition {
    name: String,
    mask: u32,
    targets: FlagTargets,
    span: Span,
}

impl FlagDefinition {
    /// Returns the flag name as written in the file.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the bits the flag sets in a user flags mask.
    pub fn mask(&self) -> u32 {
        self.mask
    }

    /// Returns where the flag may be used.
    pub fn targets(&self) -> FlagTargets {
        self.targets
    }

    /// Returns the location of the definition.
    pub fn span(&self) -> Span {
        self.span
    }
}

/// A parsed flag definition file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FlagFile {
    flags: Vec<FlagDefinition>,
    diagnostics: Vec<Diagnostic>,
}

impl FlagFile {
    /// Parses the bytes of a `.flg` file.
    ///
    /// # Errors
    ///
    /// Returns [`PapyrusError::SourceTooLarge`] if `source` is longer than
    /// `u32::MAX` bytes. Problems in the file are reported through
    /// [`FlagFile::diagnostics`].
    pub fn parse(source: &[u8]) -> Result<Self> {
        let max: usize = usize::try_from(u32::MAX).unwrap_or(usize::MAX);
        if source.len() > max {
            return Err(PapyrusError::SourceTooLarge {
                len: source.len(),
                max,
            });
        }
        let mut parser = FlagParser {
            words: words(source),
            pos: 0,
            file: FlagFile::default(),
        };
        parser.run();
        Ok(parser.file)
    }

    /// Returns all flag definitions in file order.
    pub fn flags(&self) -> &[FlagDefinition] {
        &self.flags
    }

    /// Returns the flag named `name`, ignoring ASCII case.
    pub fn get(&self, name: &str) -> Option<&FlagDefinition> {
        self.flags
            .iter()
            .find(|flag| flag.name.eq_ignore_ascii_case(name))
    }

    /// Returns the problems found while parsing.
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

/// A word, number or brace of a flag file with its location.
#[derive(Clone, Copy)]
struct Word<'s> {
    text: &'s [u8],
    span: Span,
}

/// Splits a flag file into words and braces, dropping whitespace and comments.
fn words(source: &[u8]) -> Vec<Word<'_>> {
    let offset = |pos: usize| u32::try_from(pos).unwrap_or(u32::MAX);
    let mut words: Vec<Word<'_>> = Vec::new();
    let mut pos: usize = 0;
    while pos < source.len() {
        let rest: &[u8] = &source[pos..];
        if rest.starts_with(b"//") || rest.starts_with(b";") {
            pos += rest
                .iter()
                .position(|byte| *byte == b'\n')
                .unwrap_or(rest.len());
        } else if rest.starts_with(b"/*") {
            let end: usize = rest[2..]
                .windows(2)
                .position(|pair| pair == b"*/")
                .map_or(rest.len(), |end| end + 4);
            pos += end;
        } else if matches!(rest[0], b'{' | b'}') {
            words.push(Word {
                text: &rest[..1],
                span: Span::new(offset(pos), offset(pos + 1)),
            });
            pos += 1;
        } else if rest[0].is_ascii_whitespace() {
            pos += 1;
        } else {
            let ends_word = |index: usize| {
                let byte: u8 = rest[index];
                byte.is_ascii_whitespace()
                    || matches!(byte, b'{' | b'}' | b';')
                    || (byte == b'/' && matches!(rest.get(index + 1), Some(b'/' | b'*')))
            };
            let len: usize = (1..rest.len())
                .find(|index| ends_word(*index))
                .unwrap_or(rest.len());
            words.push(Word {
                text: &rest[..len],
                span: Span::new(offset(pos), offset(pos + len)),
            });
            pos += len;
        }
    }
    words
}

struct FlagParser<'s> {
    words: Vec<Word<'s>>,
    pos: usize,
    file: FlagFile,
}

impl<'s> FlagParser<'s> {
    fn next(&mut self) -> Option<Word<'s>> {
        let word: Option<Word<'s>> = self.words.get(self.pos).copied();
        self.pos += usize::from(word.is_some());
        word
    }

    fn peek_text(&self) -> Option<&'s [u8]> {
        self.words.get(self.pos).map(|word| word.text)
    }

    fn error(&mut self, code: DiagnosticCode, span: Span, message: String) {
        self.file
            .diagnostics
            .push(Diagnostic::error(code, span, message));
    }

    fn run(&mut self) {
        while let Some(word) = self.next() {
            if !word.text.eq_ignore_ascii_case(b"Flag") {
                let shown: String = String::from_utf8_lossy(word.text).into_owned();
                self.error(
                    DiagnosticCode::UnexpectedToken,
                    word.span,
                    format!("expected 'Flag', found '{shown}'"),
                );
                continue;
            }
            self.definition(word.span);
        }
    }

    fn definition(&mut self, start: Span) {
        let Some(name) = self.next() else {
            self.error(
                DiagnosticCode::UnexpectedToken,
                start,
                "expected a flag name".to_owned(),
            );
            return;
        };
        let name_text: String = String::from_utf8_lossy(name.text).into_owned();
        let name_span: Span = name.span;

        let mut mask: u32 = 0;
        let mut has_index: bool = false;
        while let Some(text) = self.peek_text() {
            if !text.iter().all(u8::is_ascii_digit) {
                break;
            }
            let index: Option<u32> = std::str::from_utf8(text)
                .ok()
                .and_then(|digits| digits.parse::<u32>().ok())
                .filter(|index| *index < 32);
            let Some(word) = self.next() else { break };
            let span: Span = word.span;
            match index {
                Some(index) => {
                    mask |= 1 << index;
                    has_index = true;
                }
                None => self.error(
                    DiagnosticCode::InvalidFlagIndex,
                    span,
                    "flag index must be between 0 and 31".to_owned(),
                ),
            }
        }
        if !has_index {
            self.error(
                DiagnosticCode::InvalidFlagIndex,
                name_span,
                format!("flag '{name_text}' has no bit index"),
            );
        }

        let mut targets: FlagTargets = FlagTargets::empty();
        if self.peek_text() == Some(b"{") {
            self.next();
            loop {
                let Some(word) = self.next() else {
                    self.error(
                        DiagnosticCode::UnexpectedToken,
                        start,
                        format!("target list of flag '{name_text}' is never closed with '}}'"),
                    );
                    break;
                };
                if word.text == b"}" {
                    break;
                }
                let span: Span = word.span;
                match FlagTargets::from_keyword(word.text) {
                    Some(target) => targets |= target,
                    None => {
                        let shown: String = String::from_utf8_lossy(word.text).into_owned();
                        self.file.diagnostics.push(Diagnostic::warning(
                            DiagnosticCode::UnknownFlagTarget,
                            span,
                            format!("unknown flag target '{shown}'"),
                        ));
                    }
                }
            }
        }
        if targets.is_empty() {
            targets = FlagTargets::all();
        }

        if !has_index {
            return;
        }
        if self.file.get(&name_text).is_some() {
            self.error(
                DiagnosticCode::DuplicateFlag,
                name_span,
                format!("flag '{name_text}' is defined more than once"),
            );
            return;
        }
        let last: Span = self
            .words
            .get(self.pos.saturating_sub(1))
            .map_or(start, |word| word.span);
        let span: Span = start.to(last);
        self.file.flags.push(FlagDefinition {
            name: name_text,
            mask,
            targets,
            span,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the Skyrim flag file format, including comments and targets.
    #[test]
    fn parses_flag_definitions() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let source: &[u8] = b"/*\n Format: Flag <name> <index> { ... } */\n// note\n\
            Flag Hidden 0\n{\n\tScript\n\tProperty\n}\nFlag Conditional 1 { Script Variable }\n\
            Flag Collapsed 3 4 { Group }\nFlag Anywhere 7\n";

        // when
        let file: FlagFile = FlagFile::parse(source)?;

        // then
        assert!(file.diagnostics().is_empty(), "{:?}", file.diagnostics());
        let hidden: &FlagDefinition = file.get("hidden").ok_or("missing Hidden")?;
        assert_eq!(hidden.mask(), 1);
        assert_eq!(
            hidden.targets(),
            FlagTargets::SCRIPT | FlagTargets::PROPERTY
        );
        assert_eq!(
            file.get("Collapsed").map(FlagDefinition::mask),
            Some(0b11000)
        );
        assert_eq!(
            file.get("Anywhere").map(FlagDefinition::targets),
            Some(FlagTargets::all())
        );
        assert_eq!(file.flags().len(), 4);
        Ok(())
    }

    /// Verifies that malformed entries are reported without aborting.
    #[test]
    fn reports_invalid_entries() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // when
        let file: FlagFile =
            FlagFile::parse(b"Flag Big 32\nFlag A 1 { Nonsense }\nFlag a 2\nOops\nFlag B 3 {")?;

        // then
        let codes: Vec<DiagnosticCode> = file.diagnostics().iter().map(Diagnostic::code).collect();
        assert_eq!(
            codes,
            vec![
                DiagnosticCode::InvalidFlagIndex,
                DiagnosticCode::InvalidFlagIndex,
                DiagnosticCode::UnknownFlagTarget,
                DiagnosticCode::DuplicateFlag,
                DiagnosticCode::UnexpectedToken,
                DiagnosticCode::UnexpectedToken,
            ]
        );
        assert!(file.has_errors());
        assert_eq!(file.flags().len(), 2);
        Ok(())
    }
}
