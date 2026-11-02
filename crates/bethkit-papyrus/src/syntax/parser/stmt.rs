// SPDX-License-Identifier: Apache-2.0
//!
//! Statement parsing.

use super::{ends_block, Parser};
use crate::diagnostics::{DiagnosticCode, Span};
use crate::syntax::ast::{
    AssignOp, ConditionalBlock, ExprId, ExprKind, Ident, StmtId, StmtKind, TypeRef,
};
use crate::syntax::token::{Keyword, Token, TokenKind};

impl Parser<'_> {
    /// Parses statements until a keyword that cannot start a statement or the
    /// end of the file. The terminating token is not consumed.
    pub(super) fn block(&mut self) -> Vec<StmtId> {
        let mut statements: Vec<StmtId> = Vec::new();
        loop {
            self.skip_newlines();
            match self.peek().kind {
                TokenKind::Eof => break,
                TokenKind::Keyword(keyword) if ends_block(keyword) => break,
                _ => {}
            }
            let start: usize = self.pos;
            match self.statement() {
                Some(statement) => statements.push(statement),
                None => {
                    let ended_line: bool = self.pos > start
                        && self.tokens.get(self.pos - 1).map(|token| token.kind)
                            == Some(TokenKind::Newline);
                    if !ended_line {
                        self.recover_line();
                    }
                }
            }
        }
        statements
    }

    fn statement(&mut self) -> Option<StmtId> {
        let token: Token = self.peek();
        match token.kind {
            TokenKind::Keyword(Keyword::If) => self.nested(Self::if_statement),
            TokenKind::Keyword(Keyword::While) => self.nested(Self::while_statement),
            TokenKind::Keyword(Keyword::Return) => self.return_statement(),
            TokenKind::Keyword(Keyword::LockGuard) => self.nested(Self::lock_guard),
            TokenKind::Keyword(Keyword::TryLockGuard) => self.nested(Self::try_lock_guard),
            _ if self.at_define() => self.define(),
            _ => self.expression_statement(),
        }
    }

    /// Runs a block-structured statement parser with a nesting check.
    fn nested(&mut self, parse: fn(&mut Self) -> Option<StmtId>) -> Option<StmtId> {
        if !self.enter() {
            return None;
        }
        let statement: Option<StmtId> = parse(self);
        self.leave();
        statement
    }

    /// Returns `true` if the current line declares a local variable.
    fn at_define(&self) -> bool {
        match self.peek().kind {
            TokenKind::Keyword(
                Keyword::Bool | Keyword::Int | Keyword::Float | Keyword::String | Keyword::Var,
            ) => true,
            TokenKind::Ident => match self.peek_nth(1).kind {
                TokenKind::Ident => true,
                TokenKind::LBracket => self.peek_nth(2).kind == TokenKind::RBracket,
                TokenKind::Colon => self.lookahead_type_then(TokenKind::Ident),
                _ => false,
            },
            _ => false,
        }
    }

    fn define(&mut self) -> Option<StmtId> {
        let ty: TypeRef = self.type_ref()?;
        let name: Ident = self.ident("a variable name")?;
        let initial: Option<ExprId> = match self.eat(TokenKind::Assign) {
            Some(_) => Some(self.expression()?),
            None => None,
        };
        let is_const: bool = self.eat_keyword(Keyword::Const).is_some();
        let span: Span = self.span_from(ty.span);
        self.expect_line_end();
        Some(self.ast.push_stmt(
            StmtKind::Define {
                ty,
                name,
                initial,
                is_const,
            },
            span,
        ))
    }

    fn expression_statement(&mut self) -> Option<StmtId> {
        let start: Span = self.peek().span;
        let target: ExprId = self.expression()?;
        let op: Option<AssignOp> = match self.peek().kind {
            TokenKind::Assign => Some(AssignOp::Assign),
            TokenKind::PlusAssign => Some(AssignOp::Add),
            TokenKind::MinusAssign => Some(AssignOp::Sub),
            TokenKind::StarAssign => Some(AssignOp::Mul),
            TokenKind::SlashAssign => Some(AssignOp::Div),
            TokenKind::PercentAssign => Some(AssignOp::Mod),
            _ => None,
        };
        let kind: StmtKind = match op {
            Some(op) => {
                self.bump();
                let target_expr = &self.ast[target];
                if !matches!(
                    target_expr.kind,
                    ExprKind::Name(_) | ExprKind::Member { .. } | ExprKind::Index { .. }
                ) {
                    let span: Span = target_expr.span;
                    self.error(
                        DiagnosticCode::InvalidAssignmentTarget,
                        span,
                        "only variables, properties and array elements can be assigned",
                    );
                }
                let value: ExprId = self.expression()?;
                StmtKind::Assign { target, op, value }
            }
            None => StmtKind::Expr(target),
        };
        let span: Span = self.span_from(start);
        self.expect_line_end();
        Some(self.ast.push_stmt(kind, span))
    }

    fn return_statement(&mut self) -> Option<StmtId> {
        let start: Span = self.bump().span;
        let value: Option<ExprId> = if self.at_line_end() {
            None
        } else {
            Some(self.expression()?)
        };
        let span: Span = self.span_from(start);
        self.expect_line_end();
        Some(self.ast.push_stmt(StmtKind::Return(value), span))
    }

    /// Parses `condition NEWLINE body` after the opening keyword.
    ///
    /// A broken condition is replaced by an error expression so the body and
    /// the closing keyword are still matched.
    fn conditional_block(&mut self, start: Span) -> ConditionalBlock {
        let condition: ExprId = match self.expression() {
            Some(condition) => {
                self.expect_line_end();
                condition
            }
            None => {
                let span: Span = self.peek().span;
                self.recover_line();
                self.ast.push_expr(ExprKind::Error, span)
            }
        };
        let body: Vec<StmtId> = self.block();
        ConditionalBlock {
            condition,
            body,
            span: self.span_from(start),
        }
    }

    fn if_statement(&mut self) -> Option<StmtId> {
        let start: Span = self.bump().span;
        let mut branches: Vec<ConditionalBlock> = vec![self.conditional_block(start)];
        let mut otherwise: Option<Vec<StmtId>> = None;
        loop {
            let token: Token = self.peek();
            match token.kind {
                TokenKind::Keyword(Keyword::ElseIf) if otherwise.is_none() => {
                    self.bump();
                    branches.push(self.conditional_block(token.span));
                }
                TokenKind::Keyword(Keyword::Else) if otherwise.is_none() => {
                    self.bump();
                    self.expect_line_end();
                    otherwise = Some(self.block());
                }
                TokenKind::Keyword(Keyword::Else | Keyword::ElseIf) => {
                    self.error(
                        DiagnosticCode::UnexpectedToken,
                        token.span,
                        "'Else' must be the last branch of an 'If'",
                    );
                    self.bump();
                    self.recover_line();
                    let _ignored: Vec<StmtId> = self.block();
                }
                _ => break,
            }
        }
        self.expect_end(Keyword::EndIf, start, "'If'");
        let span: Span = self.span_from(start);
        Some(self.ast.push_stmt(
            StmtKind::If {
                branches,
                otherwise,
            },
            span,
        ))
    }

    fn while_statement(&mut self) -> Option<StmtId> {
        let start: Span = self.bump().span;
        let block: ConditionalBlock = self.conditional_block(start);
        self.expect_end(Keyword::EndWhile, start, "'While'");
        let span: Span = self.span_from(start);
        Some(self.ast.push_stmt(StmtKind::While(block), span))
    }

    fn guard_list(&mut self) -> Option<Vec<Ident>> {
        let mut guards: Vec<Ident> = vec![self.ident("a guard name")?];
        while self.eat(TokenKind::Comma).is_some() {
            guards.push(self.ident("a guard name")?);
        }
        self.expect_line_end();
        Some(guards)
    }

    fn lock_guard(&mut self) -> Option<StmtId> {
        let start: Span = self.bump().span;
        let guards: Vec<Ident> = self.guard_list()?;
        let body: Vec<StmtId> = self.block();
        self.expect_end(Keyword::EndLockGuard, start, "'LockGuard'");
        let span: Span = self.span_from(start);
        Some(
            self.ast
                .push_stmt(StmtKind::LockGuard { guards, body }, span),
        )
    }

    fn try_lock_guard(&mut self) -> Option<StmtId> {
        let start: Span = self.bump().span;
        let guards: Vec<Ident> = self.guard_list()?;
        let body: Vec<StmtId> = self.block();
        let otherwise: Option<Vec<StmtId>> = match self.eat_keyword(Keyword::Else) {
            Some(_) => {
                self.expect_line_end();
                Some(self.block())
            }
            None => None,
        };
        self.expect_end(Keyword::EndTryLockGuard, start, "'TryLockGuard'");
        let span: Span = self.span_from(start);
        Some(self.ast.push_stmt(
            StmtKind::TryLockGuard {
                guards,
                body,
                otherwise,
            },
            span,
        ))
    }
}
