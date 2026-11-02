// SPDX-License-Identifier: Apache-2.0
//!
//! Byte-oriented Papyrus lexer.
//!
//! The lexer works directly on the source bytes and never allocates per token:
//! it produces a flat vector of 12-byte [`Token`]s and leaves literal decoding
//! to the parser. Line breaks are significant in Papyrus, so they become
//! [`TokenKind::Newline`] tokens; runs of blank lines, comments and `\` line
//! continuations are folded away here so the parser only sees logical lines.

use super::dialect::Dialect;
use super::token::{Keyword, Token, TokenKind};
use crate::diagnostics::{Diagnostic, DiagnosticCode, Span};

/// The output of [`tokenize`].
pub(crate) struct Lexed {
    /// All tokens, always terminated by `Newline` (unless empty) and `Eof`.
    pub(crate) tokens: Vec<Token>,
    /// Problems found while lexing.
    pub(crate) diagnostics: Vec<Diagnostic>,
}

/// Splits `source` into tokens.
///
/// The caller guarantees that `source.len()` fits into a `u32`.
pub(crate) fn tokenize(source: &[u8], dialect: Dialect) -> Lexed {
    let mut lexer = Lexer {
        source,
        pos: 0,
        dialect,
        tokens: Vec::with_capacity(source.len() / 4 + 2),
        diagnostics: Vec::new(),
    };
    lexer.run();
    Lexed {
        tokens: lexer.tokens,
        diagnostics: lexer.diagnostics,
    }
}

/// Converts a byte position to a span offset.
///
/// Sources are limited to `u32::MAX` bytes before lexing, so the fallback is
/// never taken.
fn offset(pos: usize) -> u32 {
    u32::try_from(pos).unwrap_or(u32::MAX)
}

fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_ident_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

struct Lexer<'s> {
    source: &'s [u8],
    pos: usize,
    dialect: Dialect,
    tokens: Vec<Token>,
    diagnostics: Vec<Diagnostic>,
}

impl Lexer<'_> {
    fn run(&mut self) {
        if self.source.starts_with(b"\xEF\xBB\xBF") {
            self.pos = 3;
        }
        while let Some(byte) = self.peek(0) {
            match byte {
                b' ' | b'\t' | b'\r' | 0x0B | 0x0C => self.pos += 1,
                b'\n' => {
                    self.push_newline(self.pos);
                    self.pos += 1;
                }
                b';' if self.peek(1) == Some(b'/') => self.block_comment(),
                b';' => self.skip_line_comment(),
                b'{' => self.doc_comment(),
                b'\\' => self.line_continuation(),
                b'"' => self.string(),
                b'0'..=b'9' => self.number(),
                _ if is_ident_start(byte) => self.identifier(),
                _ => self.operator(byte),
            }
        }
        let end: usize = self.source.len();
        self.push_newline(end);
        self.push(TokenKind::Eof, end, end);
    }

    fn peek(&self, ahead: usize) -> Option<u8> {
        self.source.get(self.pos + ahead).copied()
    }

    fn push(&mut self, kind: TokenKind, start: usize, end: usize) {
        self.tokens.push(Token {
            kind,
            span: Span::new(offset(start), offset(end)),
        });
    }

    /// Pushes a line break unless the previous token already ends a line.
    fn push_newline(&mut self, at: usize) {
        let ends_line: bool = self
            .tokens
            .last()
            .is_none_or(|token| token.kind == TokenKind::Newline);
        if !ends_line {
            self.push(TokenKind::Newline, at, at);
        }
    }

    fn error(&mut self, code: DiagnosticCode, start: usize, end: usize, message: String) {
        self.diagnostics.push(Diagnostic::error(
            code,
            Span::new(offset(start), offset(end)),
            message,
        ));
    }

    fn skip_line_comment(&mut self) {
        while let Some(byte) = self.peek(0) {
            if byte == b'\n' {
                break;
            }
            self.pos += 1;
        }
    }

    fn block_comment(&mut self) {
        let start: usize = self.pos;
        let body: &[u8] = &self.source[start + 2..];
        match body.windows(2).position(|pair| pair == b"/;") {
            Some(end) => {
                // A comment spanning lines still separates the statements around it.
                if body[..end].contains(&b'\n') {
                    self.push_newline(start);
                }
                self.pos = start + 2 + end + 2;
            }
            None => {
                self.pos = self.source.len();
                self.error(
                    DiagnosticCode::UnterminatedComment,
                    start,
                    start + 2,
                    "block comment is never closed with '/;'".to_owned(),
                );
            }
        }
    }

    fn doc_comment(&mut self) {
        let start: usize = self.pos;
        match self.source[start..].iter().position(|byte| *byte == b'}') {
            Some(close) => {
                self.pos = start + close + 1;
                self.push(TokenKind::DocComment, start, self.pos);
            }
            None => {
                self.pos = self.source.len();
                self.error(
                    DiagnosticCode::UnterminatedDocComment,
                    start,
                    start + 1,
                    "documentation comment is never closed with '}'".to_owned(),
                );
                self.push(TokenKind::DocComment, start, self.pos);
            }
        }
    }

    fn line_continuation(&mut self) {
        let start: usize = self.pos;
        self.pos += 1;
        while let Some(byte) = self.peek(0) {
            match byte {
                b' ' | b'\t' | b'\r' => self.pos += 1,
                b';' if self.peek(1) != Some(b'/') => self.skip_line_comment(),
                b'\n' => {
                    self.pos += 1;
                    return;
                }
                _ => break,
            }
        }
        if self.pos < self.source.len() {
            self.error(
                DiagnosticCode::InvalidLineContinuation,
                start,
                start + 1,
                "'\\' must be the last character of a line".to_owned(),
            );
        }
    }

    fn string(&mut self) {
        let start: usize = self.pos;
        self.pos += 1;
        loop {
            match self.peek(0) {
                Some(b'"') => {
                    self.pos += 1;
                    break;
                }
                Some(b'\\') => {
                    let escape: Option<u8> = self.peek(1);
                    if !matches!(escape, Some(b'n' | b't' | b'\\' | b'"')) {
                        self.error(
                            DiagnosticCode::InvalidEscape,
                            self.pos,
                            self.pos + 2,
                            "unknown escape sequence; expected \\n, \\t, \\\\ or \\\"".to_owned(),
                        );
                    }
                    let skip: usize = if matches!(escape, None | Some(b'\n')) {
                        1
                    } else {
                        2
                    };
                    self.pos += skip;
                }
                Some(b'\n') | None => {
                    self.error(
                        DiagnosticCode::UnterminatedString,
                        start,
                        self.pos,
                        "string literal is not closed before the end of the line".to_owned(),
                    );
                    break;
                }
                Some(_) => self.pos += 1,
            }
        }
        self.push(TokenKind::String, start, self.pos);
    }

    fn number(&mut self) {
        let start: usize = self.pos;
        let is_hex: bool = self.peek(0) == Some(b'0')
            && matches!(self.peek(1), Some(b'x' | b'X'))
            && self.peek(2).is_some_and(|byte| byte.is_ascii_hexdigit());
        let mut kind: TokenKind = TokenKind::Int;
        if is_hex {
            self.pos += 2;
            self.eat_while(|byte| byte.is_ascii_hexdigit());
        } else {
            self.eat_while(|byte| byte.is_ascii_digit());
            if self.peek(0) == Some(b'.') && self.peek(1).is_some_and(|byte| byte.is_ascii_digit())
            {
                kind = TokenKind::Float;
                self.pos += 1;
                self.eat_while(|byte| byte.is_ascii_digit());
            }
            let has_exponent: bool = matches!(self.peek(0), Some(b'e' | b'E'))
                && match self.peek(1) {
                    Some(b'+' | b'-') => self.peek(2).is_some_and(|byte| byte.is_ascii_digit()),
                    Some(byte) => byte.is_ascii_digit(),
                    None => false,
                };
            if has_exponent {
                kind = TokenKind::Float;
                self.pos += 2;
                self.eat_while(|byte| byte.is_ascii_digit());
            }
        }
        // NOTE: like the Creation Kit compiler, a number simply ends at the first byte that
        // cannot continue it; vanilla Skyrim sources contain `= 1.1Auto`.
        self.push(kind, start, self.pos);
    }

    fn identifier(&mut self) {
        let start: usize = self.pos;
        self.eat_while(is_ident_continue);
        let kind: TokenKind = match Keyword::lookup(&self.source[start..self.pos], self.dialect) {
            Some(keyword) => TokenKind::Keyword(keyword),
            None => TokenKind::Ident,
        };
        self.push(kind, start, self.pos);
    }

    fn operator(&mut self, byte: u8) {
        let next: Option<u8> = self.peek(1);
        let (kind, len): (Option<TokenKind>, usize) = match (byte, next) {
            (b'(', _) => (Some(TokenKind::LParen), 1),
            (b')', _) => (Some(TokenKind::RParen), 1),
            (b'[', _) => (Some(TokenKind::LBracket), 1),
            (b']', _) => (Some(TokenKind::RBracket), 1),
            (b',', _) => (Some(TokenKind::Comma), 1),
            (b'.', _) => (Some(TokenKind::Dot), 1),
            (b':', _) => (Some(TokenKind::Colon), 1),
            (b'=', Some(b'=')) => (Some(TokenKind::EqEq), 2),
            (b'=', _) => (Some(TokenKind::Assign), 1),
            (b'!', Some(b'=')) => (Some(TokenKind::NotEq), 2),
            (b'!', _) => (Some(TokenKind::Bang), 1),
            (b'<', Some(b'=')) => (Some(TokenKind::Le), 2),
            (b'<', _) => (Some(TokenKind::Lt), 1),
            (b'>', Some(b'=')) => (Some(TokenKind::Ge), 2),
            (b'>', _) => (Some(TokenKind::Gt), 1),
            (b'+', Some(b'=')) => (Some(TokenKind::PlusAssign), 2),
            (b'+', _) => (Some(TokenKind::Plus), 1),
            (b'-', Some(b'=')) => (Some(TokenKind::MinusAssign), 2),
            (b'-', _) => (Some(TokenKind::Minus), 1),
            (b'*', Some(b'=')) => (Some(TokenKind::StarAssign), 2),
            (b'*', _) => (Some(TokenKind::Star), 1),
            (b'/', Some(b'=')) => (Some(TokenKind::SlashAssign), 2),
            (b'/', _) => (Some(TokenKind::Slash), 1),
            (b'%', Some(b'=')) => (Some(TokenKind::PercentAssign), 2),
            (b'%', _) => (Some(TokenKind::Percent), 1),
            (b'&', Some(b'&')) => (Some(TokenKind::AndAnd), 2),
            (b'|', Some(b'|')) => (Some(TokenKind::OrOr), 2),
            _ => (None, 1),
        };
        let start: usize = self.pos;
        match kind {
            Some(kind) => {
                self.pos += len;
                self.push(kind, start, self.pos);
            }
            None => {
                // Skip a whole UTF-8 sequence so one stray character yields one diagnostic.
                self.pos += 1;
                self.eat_while(|byte| (0x80..0xC0).contains(&byte));
                self.error(
                    DiagnosticCode::UnexpectedCharacter,
                    start,
                    self.pos,
                    "unexpected character".to_owned(),
                );
            }
        }
    }

    fn eat_while(&mut self, predicate: impl Fn(u8) -> bool) {
        while self.peek(0).is_some_and(&predicate) {
            self.pos += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(source: &[u8], dialect: Dialect) -> Vec<TokenKind> {
        tokenize(source, dialect)
            .tokens
            .iter()
            .map(|token| token.kind)
            .collect()
    }

    /// Verifies comments, continuations and blank lines fold into logical lines.
    #[test]
    fn folds_comments_and_continuations() -> Result<(), Box<dyn std::error::Error>> {
        // given
        let source: &[u8] = b"\n\n; note\nFoo(a, \\ ; wrapped\n b) ;/ multi\nline /; x\n{doc}\n";

        // when
        let lexed: Lexed = tokenize(source, Dialect::Skyrim);

        // then
        use TokenKind::*;
        let kinds: Vec<TokenKind> = lexed.tokens.iter().map(|token| token.kind).collect();
        assert_eq!(
            kinds,
            vec![
                Ident, LParen, Ident, Comma, Ident, RParen, Newline, Ident, Newline, DocComment,
                Newline, Eof
            ]
        );
        assert!(lexed.diagnostics.is_empty());
        Ok(())
    }

    /// Verifies numbers, strings and multi-character operators.
    #[test]
    fn lexes_literals_and_operators() -> Result<(), Box<dyn std::error::Error>> {
        // when
        let kinds: Vec<TokenKind> = kinds(
            b"0x1F 12 1.5 2e3 1.1Auto \"a\\\"b\" <= != && || += %",
            Dialect::Skyrim,
        );

        // then
        use TokenKind::*;
        assert_eq!(
            kinds,
            vec![
                Int,
                Int,
                Float,
                Float,
                Float,
                Keyword(super::Keyword::Auto),
                String,
                Le,
                NotEq,
                AndAnd,
                OrOr,
                PlusAssign,
                Percent,
                Newline,
                Eof
            ]
        );
        Ok(())
    }

    /// Verifies that malformed input produces diagnostics but no panics.
    #[test]
    fn reports_lexical_errors() -> Result<(), Box<dyn std::error::Error>> {
        // when
        let lexed: Lexed = tokenize(
            "\"open\n\"bad\\q\" \u{e4} & ;/ never".as_bytes(),
            Dialect::Skyrim,
        );

        // then
        let codes: Vec<DiagnosticCode> = lexed
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code())
            .collect();
        assert_eq!(
            codes,
            vec![
                DiagnosticCode::UnterminatedString,
                DiagnosticCode::InvalidEscape,
                DiagnosticCode::UnexpectedCharacter,
                DiagnosticCode::UnexpectedCharacter,
                DiagnosticCode::UnterminatedComment,
            ]
        );
        assert_eq!(
            lexed.tokens.last().map(|token| token.kind),
            Some(TokenKind::Eof)
        );
        Ok(())
    }

    /// Verifies that keywords are only reserved in their dialect.
    #[test]
    fn dialect_controls_keywords() -> Result<(), Box<dyn std::error::Error>> {
        // then
        assert_eq!(kinds(b"Struct", Dialect::Skyrim)[0], TokenKind::Ident);
        assert_eq!(
            kinds(b"Struct", Dialect::Fallout4)[0],
            TokenKind::Keyword(Keyword::Struct)
        );
        Ok(())
    }
}
