// SPDX-License-Identifier: Apache-2.0
//!
//! Expression parsing.
//!
//! Binary operators use precedence climbing with these levels, lowest first:
//! `||`, `&&`, comparisons, `+ -`, `* / %`. Below them come the prefix
//! operators `-` and `!`, then `As`/`Is` casts, then member access, indexing
//! and calls. This matches the Creation Kit grammar, where `-x As Int` means
//! `-(x As Int)` and `a.b As T` means `(a.b) As T`.

use super::Parser;
use crate::diagnostics::{DiagnosticCode, Span};
use crate::syntax::ast::{
    Argument, BinaryOp, ExprId, ExprKind, Ident, QualifiedName, TypeRef, UnaryOp,
};
use crate::syntax::token::{Keyword, Token, TokenKind};

fn binary_operator(kind: TokenKind) -> Option<(BinaryOp, u8)> {
    let operator: (BinaryOp, u8) = match kind {
        TokenKind::OrOr => (BinaryOp::Or, 1),
        TokenKind::AndAnd => (BinaryOp::And, 2),
        TokenKind::EqEq => (BinaryOp::Eq, 3),
        TokenKind::NotEq => (BinaryOp::Ne, 3),
        TokenKind::Lt => (BinaryOp::Lt, 3),
        TokenKind::Le => (BinaryOp::Le, 3),
        TokenKind::Gt => (BinaryOp::Gt, 3),
        TokenKind::Ge => (BinaryOp::Ge, 3),
        TokenKind::Plus => (BinaryOp::Add, 4),
        TokenKind::Minus => (BinaryOp::Sub, 4),
        TokenKind::Star => (BinaryOp::Mul, 5),
        TokenKind::Slash => (BinaryOp::Div, 5),
        TokenKind::Percent => (BinaryOp::Mod, 5),
        _ => return None,
    };
    Some(operator)
}

/// Resolves the escapes of a string literal token's source text.
fn unescape(raw: &[u8]) -> Box<[u8]> {
    let inner: &[u8] = raw.strip_prefix(b"\"").unwrap_or(raw);
    let inner: &[u8] = match inner.strip_suffix(b"\"") {
        // An escaped quote at the end means the literal was never closed.
        Some(stripped) if !stripped.ends_with(b"\\") || stripped.ends_with(b"\\\\") => stripped,
        _ => inner,
    };
    let mut value: Vec<u8> = Vec::with_capacity(inner.len());
    let mut bytes = inner.iter().copied();
    while let Some(byte) = bytes.next() {
        if byte != b'\\' {
            value.push(byte);
            continue;
        }
        match bytes.next() {
            Some(b'n') => value.push(b'\n'),
            Some(b't') => value.push(b'\t'),
            Some(other) => value.push(other),
            None => value.push(b'\\'),
        }
    }
    value.into_boxed_slice()
}

impl Parser<'_> {
    /// Parses a full expression.
    pub(super) fn expression(&mut self) -> Option<ExprId> {
        if !self.enter() {
            return None;
        }
        let expression: Option<ExprId> = self.binary(1);
        self.leave();
        expression
    }

    fn binary(&mut self, min_precedence: u8) -> Option<ExprId> {
        let mut lhs: ExprId = self.unary()?;
        while let Some((op, precedence)) = binary_operator(self.peek().kind) {
            if precedence < min_precedence {
                break;
            }
            self.bump();
            let rhs: ExprId = self.binary(precedence + 1)?;
            let span: Span = self.ast[lhs].span.to(self.ast[rhs].span);
            lhs = self.ast.push_expr(ExprKind::Binary { op, lhs, rhs }, span);
        }
        Some(lhs)
    }

    fn unary(&mut self) -> Option<ExprId> {
        let token: Token = self.peek();
        let op: UnaryOp = match token.kind {
            TokenKind::Minus => UnaryOp::Neg,
            TokenKind::Bang => UnaryOp::Not,
            _ => return self.cast(),
        };
        self.bump();
        let literal: TokenKind = self.peek().kind;
        if op == UnaryOp::Neg && matches!(literal, TokenKind::Int | TokenKind::Float) {
            // Negative literals are single constants, which also makes -2147483648 valid.
            let number: Token = self.bump();
            let kind: ExprKind = self.number(number, true);
            let span: Span = token.span.to(number.span);
            let literal_expr: ExprId = self.ast.push_expr(kind, span);
            return self.cast_suffix(literal_expr);
        }
        if !self.enter() {
            return None;
        }
        let operand: Option<ExprId> = self.unary();
        self.leave();
        let operand: ExprId = operand?;
        let span: Span = token.span.to(self.ast[operand].span);
        Some(self.ast.push_expr(ExprKind::Unary { op, operand }, span))
    }

    fn cast(&mut self) -> Option<ExprId> {
        let value: ExprId = self.postfix()?;
        self.cast_suffix(value)
    }

    fn cast_suffix(&mut self, mut value: ExprId) -> Option<ExprId> {
        loop {
            let is_cast: bool = match self.peek().kind {
                TokenKind::Keyword(Keyword::As) => true,
                TokenKind::Keyword(Keyword::Is) => false,
                _ => return Some(value),
            };
            self.bump();
            let ty: TypeRef = self.type_ref()?;
            let span: Span = self.ast[value].span.to(ty.span);
            let kind: ExprKind = if is_cast {
                ExprKind::Cast { value, ty }
            } else {
                ExprKind::Is { value, ty }
            };
            value = self.ast.push_expr(kind, span);
        }
    }

    fn postfix(&mut self) -> Option<ExprId> {
        let mut value: ExprId = self.primary()?;
        loop {
            match self.peek().kind {
                TokenKind::Dot => {
                    self.bump();
                    let name: Ident = self.ident("a member name")?;
                    value = if self.at(TokenKind::LParen) {
                        self.call(Some(value), name)?
                    } else {
                        let span: Span = self.ast[value].span.to(name.span);
                        self.ast.push_expr(
                            ExprKind::Member {
                                object: value,
                                name,
                            },
                            span,
                        )
                    };
                }
                TokenKind::LBracket => {
                    self.bump();
                    let index: ExprId = self.expression()?;
                    self.expect(TokenKind::RBracket, "']'")?;
                    let span: Span = self.span_from(self.ast[value].span);
                    value = self.ast.push_expr(
                        ExprKind::Index {
                            array: value,
                            index,
                        },
                        span,
                    );
                }
                _ => return Some(value),
            }
        }
    }

    fn call(&mut self, receiver: Option<ExprId>, name: Ident) -> Option<ExprId> {
        self.bump();
        let mut args: Vec<Argument> = Vec::new();
        if self.eat(TokenKind::RParen).is_none() {
            loop {
                let named: bool =
                    self.at(TokenKind::Ident) && self.peek_nth(1).kind == TokenKind::Assign;
                let arg_name: Option<Ident> = if named {
                    let name: Option<Ident> = self.ident("an argument name");
                    self.bump();
                    name
                } else {
                    None
                };
                let value: ExprId = self.expression()?;
                args.push(Argument {
                    name: arg_name,
                    value,
                });
                if self.eat(TokenKind::Comma).is_none() {
                    break;
                }
            }
            self.expect(TokenKind::RParen, "',' or ')'")?;
        }
        let start: Span = receiver.map_or(name.span, |receiver| self.ast[receiver].span);
        let span: Span = self.span_from(start);
        Some(self.ast.push_expr(
            ExprKind::Call {
                receiver,
                name,
                args,
            },
            span,
        ))
    }

    fn primary(&mut self) -> Option<ExprId> {
        let token: Token = self.peek();
        let kind: ExprKind = match token.kind {
            TokenKind::Int | TokenKind::Float => {
                self.bump();
                self.number(token, false)
            }
            TokenKind::String => {
                self.bump();
                let raw: &[u8] = self.source.get(token.span.range()).unwrap_or_default();
                ExprKind::String(unescape(raw))
            }
            TokenKind::Keyword(Keyword::True) => {
                self.bump();
                ExprKind::Bool(true)
            }
            TokenKind::Keyword(Keyword::False) => {
                self.bump();
                ExprKind::Bool(false)
            }
            TokenKind::Keyword(Keyword::None) => {
                self.bump();
                ExprKind::None
            }
            TokenKind::Keyword(Keyword::SelfRef) => {
                self.bump();
                ExprKind::SelfRef
            }
            TokenKind::Keyword(Keyword::Parent) => {
                self.bump();
                ExprKind::Parent
            }
            TokenKind::LParen => {
                self.bump();
                let inner: ExprId = self.expression()?;
                self.expect(TokenKind::RParen, "')'")?;
                return Some(inner);
            }
            TokenKind::Keyword(Keyword::New) => return self.new_expression(),
            TokenKind::Ident
                if self.peek_nth(1).kind == TokenKind::Colon
                    && self.peek_nth(2).kind == TokenKind::Ident =>
            {
                let path: QualifiedName = self.qualified_name("a script name")?;
                let span: Span = path.span;
                return Some(self.ast.push_expr(ExprKind::Path(path), span));
            }
            TokenKind::Ident => {
                self.bump();
                let name: Ident = self.make_ident(token);
                if self.at(TokenKind::LParen) {
                    return self.call(None, name);
                }
                ExprKind::Name(name)
            }
            _ => {
                self.unexpected("an expression");
                return None;
            }
        };
        Some(self.ast.push_expr(kind, token.span))
    }

    fn new_expression(&mut self) -> Option<ExprId> {
        let start: Span = self.bump().span;
        let element: TypeRef = self.type_base()?;
        if self.eat(TokenKind::LBracket).is_some() {
            let size: ExprId = self.expression()?;
            self.expect(TokenKind::RBracket, "']'")?;
            let span: Span = self.span_from(start);
            return Some(
                self.ast
                    .push_expr(ExprKind::NewArray { element, size }, span),
            );
        }
        let span: Span = self.span_from(start);
        if !self.dialect.has_fallout4_syntax() {
            self.unsupported(span, "creating a struct with 'New'");
        }
        Some(
            self.ast
                .push_expr(ExprKind::NewStruct { ty: element }, span),
        )
    }

    /// Decodes an integer or float literal token.
    fn number(&mut self, token: Token, negative: bool) -> ExprKind {
        let text: String = self.text(token.span).to_owned();
        if token.kind == TokenKind::Float {
            let value: f32 = text.parse::<f32>().unwrap_or_else(|_| {
                self.error(
                    DiagnosticCode::InvalidNumber,
                    token.span,
                    "malformed float literal",
                );
                0.0
            });
            return ExprKind::Float(if negative { -value } else { value });
        }

        let parsed: Option<i64> = match text.get(..2) {
            Some("0x" | "0X") => u32::from_str_radix(&text[2..], 16)
                .ok()
                .map(|bits| i64::from(i32::from_ne_bytes(bits.to_ne_bytes()))),
            _ => text.parse::<i64>().ok(),
        };
        let value: Option<i32> = parsed
            .map(|value| if negative { -value } else { value })
            .and_then(|value| i32::try_from(value).ok());
        match value {
            Some(value) => ExprKind::Int(value),
            None => {
                self.error(
                    DiagnosticCode::IntegerOverflow,
                    token.span,
                    "integer literal does not fit into 32 bits",
                );
                ExprKind::Int(0)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies escape handling, including unterminated literals.
    #[test]
    fn unescapes_string_literals() -> Result<(), Box<dyn std::error::Error>> {
        // then
        assert_eq!(&*unescape(br#""a\nb\t\"c\\""#), b"a\nb\t\"c\\");
        assert_eq!(&*unescape(br#""open"#), b"open");
        assert_eq!(&*unescape(br#""end\""#), b"end\"");
        assert_eq!(&*unescape(b"\"\xe4\""), b"\xe4");
        Ok(())
    }
}
