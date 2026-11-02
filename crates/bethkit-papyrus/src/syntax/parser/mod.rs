// SPDX-License-Identifier: Apache-2.0
//!
//! Recursive-descent Papyrus parser with error recovery.
//!
//! The parser consumes the token vector produced by the lexer and builds the
//! declaration tree plus the expression and statement arenas of [`Ast`].
//! Errors never stop parsing: a broken statement or declaration is reported,
//! the rest of its line is skipped, and parsing continues, so one run reports
//! every problem. Blocks stop at any keyword that cannot start a statement,
//! which lets a missing `EndIf` or `EndFunction` be reported at its opener
//! instead of swallowing the rest of the file.
//!
//! The module is split into declarations (this file), statements and
//! expressions.

mod expr;
mod stmt;
#[cfg(test)]
mod tests;

use super::ast::{
    Ast, CustomEvent, DocComment, Flag, FlagName, Function, FunctionKind, Group, Guard, Ident,
    Import, Item, Parameter, Property, PropertyKind, QualifiedName, Script, ScriptHeader, State,
    StmtId, Struct, TypeName, TypeRef, Variable,
};
use super::dialect::Dialect;
use super::interner::Interner;
use super::token::{Keyword, Token, TokenKind};
use crate::diagnostics::{Diagnostic, DiagnosticCode, Span};

/// Maximum nesting of expressions and blocks before parsing gives up on a
/// construct. Protects against stack exhaustion on adversarial input.
const MAX_DEPTH: u32 = 128;

/// Keywords that can never start a statement. Blocks end when they see one.
fn ends_block(keyword: Keyword) -> bool {
    matches!(
        keyword,
        Keyword::Auto
            | Keyword::CustomEvent
            | Keyword::Else
            | Keyword::ElseIf
            | Keyword::EndEvent
            | Keyword::EndFunction
            | Keyword::EndGroup
            | Keyword::EndIf
            | Keyword::EndLockGuard
            | Keyword::EndProperty
            | Keyword::EndState
            | Keyword::EndStruct
            | Keyword::EndTryLockGuard
            | Keyword::EndWhile
            | Keyword::Event
            | Keyword::Function
            | Keyword::Group
            | Keyword::Guard
            | Keyword::Import
            | Keyword::Property
            | Keyword::ScriptName
            | Keyword::State
            | Keyword::Struct
    )
}

/// Everything the parser produced.
pub(crate) struct ParseOutput {
    pub(crate) script: Script,
    pub(crate) ast: Ast,
    pub(crate) interner: Interner,
    pub(crate) diagnostics: Vec<Diagnostic>,
}

/// Parses a token stream produced by the lexer for `source`.
pub(crate) fn parse_tokens(
    source: &[u8],
    tokens: Vec<Token>,
    dialect: Dialect,
    diagnostics: Vec<Diagnostic>,
) -> ParseOutput {
    let mut parser = Parser {
        source,
        tokens,
        pos: 0,
        last_end: 0,
        dialect,
        depth: 0,
        ast: Ast::default(),
        interner: Interner::new(),
        diagnostics,
    };
    let script: Script = parser.script();
    ParseOutput {
        script,
        ast: parser.ast,
        interner: parser.interner,
        diagnostics: parser.diagnostics,
    }
}

pub(crate) struct Parser<'s> {
    source: &'s [u8],
    tokens: Vec<Token>,
    pos: usize,
    last_end: u32,
    dialect: Dialect,
    depth: u32,
    ast: Ast,
    interner: Interner,
    diagnostics: Vec<Diagnostic>,
}

impl Parser<'_> {
    fn peek(&self) -> Token {
        self.peek_nth(0)
    }

    /// Returns the token `n` positions ahead, or the final `Eof` token.
    fn peek_nth(&self, n: usize) -> Token {
        let eof = Token {
            kind: TokenKind::Eof,
            span: Span::new(self.last_end, self.last_end),
        };
        self.tokens
            .get(self.pos + n)
            .or_else(|| self.tokens.last())
            .copied()
            .unwrap_or(eof)
    }

    fn at(&self, kind: TokenKind) -> bool {
        self.peek().kind == kind
    }

    fn at_keyword(&self, keyword: Keyword) -> bool {
        self.at(TokenKind::Keyword(keyword))
    }

    fn at_line_end(&self) -> bool {
        matches!(self.peek().kind, TokenKind::Newline | TokenKind::Eof)
    }

    /// Consumes the current token; never moves past `Eof`.
    fn bump(&mut self) -> Token {
        let token: Token = self.peek();
        if token.kind != TokenKind::Eof {
            self.pos += 1;
            self.last_end = token.span.end();
        }
        token
    }

    fn eat(&mut self, kind: TokenKind) -> Option<Token> {
        self.at(kind).then(|| self.bump())
    }

    fn eat_keyword(&mut self, keyword: Keyword) -> Option<Token> {
        self.eat(TokenKind::Keyword(keyword))
    }

    /// Returns the span from `start` to the end of the last consumed token.
    fn span_from(&self, start: Span) -> Span {
        Span::new(start.start(), self.last_end.max(start.start()))
    }

    fn text(&self, span: Span) -> &str {
        self.source
            .get(span.range())
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
            .unwrap_or("")
    }

    fn describe(&self, token: Token) -> String {
        match token.kind {
            TokenKind::Newline => "end of line".to_owned(),
            TokenKind::Eof => "end of file".to_owned(),
            _ => {
                let text: &[u8] = self.source.get(token.span.range()).unwrap_or_default();
                let shown: String = String::from_utf8_lossy(&text[..text.len().min(40)]).into();
                format!("'{shown}'")
            }
        }
    }

    fn error(&mut self, code: DiagnosticCode, span: Span, message: impl Into<String>) {
        self.diagnostics
            .push(Diagnostic::error(code, span, message));
    }

    /// Reports that `expected` was wanted at the current token.
    fn unexpected(&mut self, expected: &str) {
        let token: Token = self.peek();
        let found: String = self.describe(token);
        self.error(
            DiagnosticCode::UnexpectedToken,
            token.span,
            format!("expected {expected}, found {found}"),
        );
    }

    fn expect(&mut self, kind: TokenKind, expected: &str) -> Option<Token> {
        let token: Option<Token> = self.eat(kind);
        if token.is_none() {
            self.unexpected(expected);
        }
        token
    }

    /// Skips the rest of the current line including its line break.
    fn recover_line(&mut self) {
        while !self.at_line_end() {
            self.bump();
        }
        self.eat(TokenKind::Newline);
    }

    /// Requires the end of a line; reports and skips trailing tokens otherwise.
    fn expect_line_end(&mut self) {
        if self.eat(TokenKind::Newline).is_some() || self.at(TokenKind::Eof) {
            return;
        }
        self.unexpected("end of line");
        self.recover_line();
    }

    /// Skips blank lines and documentation comments that belong to nothing.
    fn skip_newlines(&mut self) {
        while matches!(self.peek().kind, TokenKind::Newline | TokenKind::DocComment) {
            self.bump();
        }
    }

    /// Finishes a declaration header: an optional documentation comment on the
    /// same line, the line break, and an optional documentation comment on the
    /// following line.
    fn header_end(&mut self) -> Option<DocComment> {
        let same_line: Option<DocComment> = self
            .eat(TokenKind::DocComment)
            .map(|token| DocComment { span: token.span });
        self.expect_line_end();
        if same_line.is_some() {
            return same_line;
        }
        let next_line: Option<DocComment> = self
            .eat(TokenKind::DocComment)
            .map(|token| DocComment { span: token.span });
        if next_line.is_some() {
            self.expect_line_end();
        }
        next_line
    }

    /// Increments the nesting depth; reports and returns `false` if too deep.
    fn enter(&mut self) -> bool {
        if self.depth >= MAX_DEPTH {
            let span: Span = self.peek().span;
            self.error(
                DiagnosticCode::NestingTooDeep,
                span,
                format!("nesting exceeds the maximum depth of {MAX_DEPTH}"),
            );
            return false;
        }
        self.depth += 1;
        true
    }

    fn leave(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }

    fn unsupported(&mut self, span: Span, what: &str) {
        let dialect: Dialect = self.dialect;
        self.error(
            DiagnosticCode::UnsupportedSyntax,
            span,
            format!("{what} is not supported in the {dialect} dialect"),
        );
    }

    fn ident(&mut self, expected: &str) -> Option<Ident> {
        let token: Token = self.expect(TokenKind::Ident, expected)?;
        Some(self.make_ident(token))
    }

    fn make_ident(&mut self, token: Token) -> Ident {
        // NOTE: identifier tokens are ASCII by construction, so the text is never empty.
        let text: &str = self
            .source
            .get(token.span.range())
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
            .unwrap_or("");
        Ident {
            symbol: self.interner.intern(text),
            span: token.span,
        }
    }

    fn qualified_name(&mut self, expected: &str) -> Option<QualifiedName> {
        let first: Ident = self.ident(expected)?;
        let mut segments: Vec<Ident> = vec![first];
        while self.at(TokenKind::Colon) {
            let colon: Token = self.bump();
            if !self.dialect.has_fallout4_syntax() {
                self.unsupported(colon.span, "a namespaced name");
            }
            segments.push(self.ident("a name after ':'")?);
        }
        Some(QualifiedName {
            span: self.span_from(first.span),
            segments,
        })
    }

    /// Returns `true` if the current token can start a type.
    fn at_type_start(&self) -> bool {
        matches!(
            self.peek().kind,
            TokenKind::Ident
                | TokenKind::Keyword(
                    Keyword::Bool | Keyword::Int | Keyword::Float | Keyword::String | Keyword::Var
                )
        )
    }

    /// Parses a type without an array suffix.
    fn type_base(&mut self) -> Option<TypeRef> {
        let start: Span = self.peek().span;
        let name: TypeName = match self.peek().kind {
            TokenKind::Keyword(Keyword::Bool) => TypeName::Bool,
            TokenKind::Keyword(Keyword::Int) => TypeName::Int,
            TokenKind::Keyword(Keyword::Float) => TypeName::Float,
            TokenKind::Keyword(Keyword::String) => TypeName::String,
            TokenKind::Keyword(Keyword::Var) => TypeName::Var,
            TokenKind::Ident => {
                let name: QualifiedName = self.qualified_name("a type")?;
                return Some(TypeRef {
                    span: name.span,
                    name: TypeName::Named(name),
                    is_array: false,
                });
            }
            _ => {
                self.unexpected("a type");
                return None;
            }
        };
        self.bump();
        Some(TypeRef {
            name,
            is_array: false,
            span: start,
        })
    }

    /// Parses a type with an optional `[]` suffix.
    fn type_ref(&mut self) -> Option<TypeRef> {
        let mut ty: TypeRef = self.type_base()?;
        if self.at(TokenKind::LBracket) && self.peek_nth(1).kind == TokenKind::RBracket {
            self.bump();
            self.bump();
            ty.is_array = true;
            ty.span = self.span_from(ty.span);
        }
        Some(ty)
    }

    fn flags(&mut self) -> Vec<Flag> {
        let mut flags: Vec<Flag> = Vec::new();
        loop {
            let token: Token = self.peek();
            let name: FlagName = match token.kind {
                TokenKind::Ident => FlagName::User(self.make_ident(token).symbol),
                TokenKind::Keyword(keyword) if keyword.is_flag() => FlagName::Keyword(keyword),
                _ => break,
            };
            self.bump();
            flags.push(Flag {
                name,
                span: token.span,
            });
        }
        flags
    }

    /// Consumes the closing keyword of a block opened at `opener`.
    fn expect_end(&mut self, expected: Keyword, opener: Span, construct: &str) {
        let token: Token = self.peek();
        match token.kind {
            TokenKind::Keyword(found) if found == expected => {
                self.bump();
                self.expect_line_end();
            }
            TokenKind::Keyword(found) if is_end_keyword(found) && !opens_outer(found, expected) => {
                let shown: String = self.describe(token);
                self.error(
                    DiagnosticCode::MismatchedEnd,
                    token.span,
                    format!("{construct} must be closed with '{expected:?}', found {shown}"),
                );
                self.bump();
                self.expect_line_end();
            }
            _ => self.error(
                DiagnosticCode::MissingEnd,
                opener,
                format!("{construct} is never closed with '{expected:?}'"),
            ),
        }
    }

    fn script(&mut self) -> Script {
        self.skip_newlines();
        let header: Option<ScriptHeader> = if self.at_keyword(Keyword::ScriptName) {
            self.script_header()
        } else {
            let span: Span = self.peek().span;
            self.error(
                DiagnosticCode::MissingScriptName,
                span,
                "a script must start with 'ScriptName'",
            );
            None
        };

        let mut items: Vec<Item> = Vec::new();
        loop {
            self.skip_newlines();
            if self.at(TokenKind::Eof) {
                break;
            }
            if self.at_keyword(Keyword::ScriptName) {
                let span: Span = self.peek().span;
                self.error(
                    DiagnosticCode::MisplacedScriptName,
                    span,
                    "'ScriptName' must appear exactly once, before all declarations",
                );
                self.recover_line();
                continue;
            }
            if let Some(item) = self.item() {
                items.push(item);
            }
        }
        Script { header, items }
    }

    fn script_header(&mut self) -> Option<ScriptHeader> {
        let start: Span = self.bump().span;
        let Some(name) = self.qualified_name("a script name") else {
            self.recover_line();
            return None;
        };
        let extends: Option<QualifiedName> = match self.eat_keyword(Keyword::Extends) {
            Some(_) => self.qualified_name("a parent script name"),
            None => None,
        };
        let flags: Vec<Flag> = self.flags();
        let span: Span = self.span_from(start);
        let doc: Option<DocComment> = self.header_end();
        Some(ScriptHeader {
            name,
            extends,
            flags,
            doc,
            span,
        })
    }

    /// Parses one declaration; on failure the line is skipped.
    fn item(&mut self) -> Option<Item> {
        let start_pos: usize = self.pos;
        let token: Token = self.peek();
        let item: Option<Item> = match token.kind {
            TokenKind::Keyword(Keyword::Import) => self.import().map(Item::Import),
            TokenKind::Keyword(Keyword::State) => self.state(false).map(Item::State),
            TokenKind::Keyword(Keyword::Auto)
                if self.peek_nth(1).kind == TokenKind::Keyword(Keyword::State) =>
            {
                self.state(true).map(Item::State)
            }
            TokenKind::Keyword(Keyword::Function | Keyword::Event) => {
                self.function(None, token.span).map(Item::Function)
            }
            TokenKind::Keyword(Keyword::Group) => self.group().map(Item::Group),
            TokenKind::Keyword(Keyword::Struct) => self.structure().map(Item::Struct),
            TokenKind::Keyword(Keyword::CustomEvent) => self.custom_event().map(Item::CustomEvent),
            TokenKind::Keyword(Keyword::Guard) => self.guard().map(Item::Guard),
            _ if self.at_type_start() => self.typed_item(),
            _ => {
                self.unexpected("a declaration");
                None
            }
        };
        if item.is_none() {
            let ended_line: bool = self.pos > start_pos
                && self.tokens.get(self.pos - 1).map(|token| token.kind)
                    == Some(TokenKind::Newline);
            if !ended_line {
                self.recover_line();
            }
        }
        item
    }

    /// Parses a declaration that starts with a type: a variable, a property
    /// or a function with a return type.
    fn typed_item(&mut self) -> Option<Item> {
        let start: Span = self.peek().span;
        let ty: TypeRef = self.type_ref()?;
        match self.peek().kind {
            TokenKind::Keyword(Keyword::Property) => self.property(ty).map(Item::Property),
            TokenKind::Keyword(Keyword::Function | Keyword::Event) => {
                self.function(Some(ty), start).map(Item::Function)
            }
            TokenKind::Ident => self.variable(ty).map(Item::Variable),
            _ => {
                self.unexpected("'Property', 'Function' or a variable name");
                None
            }
        }
    }

    fn import(&mut self) -> Option<Import> {
        let start: Span = self.bump().span;
        let name: QualifiedName = self.qualified_name("a script name")?;
        let span: Span = self.span_from(start);
        self.expect_line_end();
        Some(Import { name, span })
    }

    fn variable(&mut self, ty: TypeRef) -> Option<Variable> {
        let name: Ident = self.ident("a variable name")?;
        let initial = match self.eat(TokenKind::Assign) {
            Some(_) => Some(self.expression()?),
            None => None,
        };
        let flags: Vec<Flag> = self.flags();
        let span: Span = self.span_from(ty.span);
        let doc: Option<DocComment> = self.header_end();
        Some(Variable {
            ty,
            name,
            initial,
            flags,
            doc,
            span,
        })
    }

    fn property(&mut self, ty: TypeRef) -> Option<Property> {
        self.bump();
        let name: Ident = self.ident("a property name")?;
        let initial = match self.eat(TokenKind::Assign) {
            Some(_) => Some(self.expression()?),
            None => None,
        };
        let mut flags: Vec<Flag> = self.flags();
        let auto_kind: Option<PropertyKind> = flags.iter().find_map(|flag| match flag.name {
            FlagName::Keyword(Keyword::Auto) => Some(PropertyKind::Auto),
            FlagName::Keyword(Keyword::AutoReadOnly) => Some(PropertyKind::AutoReadOnly),
            _ => None,
        });
        flags.retain(|flag| {
            !matches!(
                flag.name,
                FlagName::Keyword(Keyword::Auto | Keyword::AutoReadOnly)
            )
        });
        let header_span: Span = self.span_from(ty.span);
        let doc: Option<DocComment> = self.header_end();

        let kind: PropertyKind = match auto_kind {
            Some(kind) => kind,
            None => self.property_accessors(name, header_span),
        };
        let span: Span = match kind {
            PropertyKind::Full { .. } => self.span_from(ty.span),
            _ => header_span,
        };
        Some(Property {
            ty,
            name,
            initial,
            kind,
            flags,
            doc,
            span,
        })
    }

    fn property_accessors(&mut self, name: Ident, opener: Span) -> PropertyKind {
        let mut getter: Option<Box<Function>> = None;
        let mut setter: Option<Box<Function>> = None;
        loop {
            self.skip_newlines();
            let token: Token = self.peek();
            let starts_function: bool = matches!(token.kind, TokenKind::Keyword(Keyword::Function))
                || (self.at_type_start()
                    && self.lookahead_type_then(TokenKind::Keyword(Keyword::Function)));
            if starts_function {
                let return_type: Option<TypeRef> = if self.at_type_start() {
                    self.type_ref()
                } else {
                    None
                };
                let Some(function) = self.function(return_type, token.span) else {
                    continue;
                };
                let accessor: String = self.interner.resolve(function.name.symbol).to_owned();
                let slot: Option<&mut Option<Box<Function>>> =
                    if accessor.eq_ignore_ascii_case("get") {
                        Some(&mut getter)
                    } else if accessor.eq_ignore_ascii_case("set") {
                        Some(&mut setter)
                    } else {
                        None
                    };
                match slot {
                    Some(slot) if slot.is_some() => self.error(
                        DiagnosticCode::DuplicatePropertyAccessor,
                        function.name.span,
                        format!("property accessor '{accessor}' is defined more than once"),
                    ),
                    Some(slot) => *slot = Some(Box::new(function)),
                    None => self.error(
                        DiagnosticCode::InvalidPropertyAccessor,
                        function.name.span,
                        format!(
                            "property '{}' may only contain 'Get' and 'Set' functions",
                            self.interner.resolve(name.symbol)
                        ),
                    ),
                }
                continue;
            }
            match token.kind {
                TokenKind::Keyword(keyword) if ends_block(keyword) => break,
                TokenKind::Eof => break,
                _ => {
                    self.unexpected("'Function' or 'EndProperty'");
                    self.recover_line();
                }
            }
        }
        self.expect_end(Keyword::EndProperty, opener, "property");
        PropertyKind::Full { getter, setter }
    }

    /// Returns `true` if a type starting at the current token is followed by
    /// a token of `kind`. Handles namespaced and array types.
    fn lookahead_type_then(&self, kind: TokenKind) -> bool {
        let mut offset: usize = 1;
        while self.peek_nth(offset).kind == TokenKind::Colon
            && self.peek_nth(offset + 1).kind == TokenKind::Ident
        {
            offset += 2;
        }
        if self.peek_nth(offset).kind == TokenKind::LBracket
            && self.peek_nth(offset + 1).kind == TokenKind::RBracket
        {
            offset += 2;
        }
        self.peek_nth(offset).kind == kind
    }

    /// Parses a function or event starting at `Function`/`Event`; `start` is
    /// the span of the return type if one was written.
    fn function(&mut self, return_type: Option<TypeRef>, start: Span) -> Option<Function> {
        let keyword: Token = self.bump();
        let kind: FunctionKind = match keyword.kind {
            TokenKind::Keyword(Keyword::Event) => FunctionKind::Event,
            _ => FunctionKind::Function,
        };
        let (receiver, name): (Option<QualifiedName>, Ident) = match kind {
            FunctionKind::Function => (None, self.ident("a function name")?),
            FunctionKind::Event => {
                let path: QualifiedName = self.qualified_name("an event name")?;
                if self.eat(TokenKind::Dot).is_some() {
                    if !self.dialect.has_fallout4_syntax() {
                        self.unsupported(path.span, "a remote event");
                    }
                    (Some(path), self.ident("an event name")?)
                } else if path.segments.len() == 1 {
                    (None, path.last()?)
                } else {
                    self.unexpected("'.' and an event name");
                    return None;
                }
            }
        };
        let params: Vec<Parameter> = self.parameters()?;
        let flags: Vec<Flag> = self.flags();
        let header_span: Span = self.span_from(start);
        let doc: Option<DocComment> = self.header_end();

        let is_native: bool = flags
            .iter()
            .any(|flag| flag.name == FlagName::Keyword(Keyword::Native));
        let body: Option<Vec<StmtId>> = if is_native {
            None
        } else {
            let (end, construct): (Keyword, &str) = match kind {
                FunctionKind::Function => (Keyword::EndFunction, "function"),
                FunctionKind::Event => (Keyword::EndEvent, "event"),
            };
            let body: Vec<StmtId> = self.block();
            self.expect_end(end, header_span, construct);
            Some(body)
        };
        Some(Function {
            kind,
            return_type,
            receiver,
            name,
            params,
            flags,
            doc,
            body,
            span: self.span_from(start),
        })
    }

    fn parameters(&mut self) -> Option<Vec<Parameter>> {
        self.expect(TokenKind::LParen, "'('")?;
        let mut params: Vec<Parameter> = Vec::new();
        if self.eat(TokenKind::RParen).is_some() {
            return Some(params);
        }
        loop {
            let ty: TypeRef = self.type_ref()?;
            let name: Ident = self.ident("a parameter name")?;
            let default = match self.eat(TokenKind::Assign) {
                Some(_) => Some(self.expression()?),
                None => None,
            };
            params.push(Parameter {
                span: self.span_from(ty.span),
                ty,
                name,
                default,
            });
            if self.eat(TokenKind::Comma).is_none() {
                break;
            }
        }
        self.expect(TokenKind::RParen, "',' or ')'")?;
        Some(params)
    }

    fn state(&mut self, is_auto: bool) -> Option<State> {
        let start: Span = self.peek().span;
        if is_auto {
            self.bump();
        }
        self.bump();
        let name: Ident = self.ident("a state name")?;
        let header_span: Span = self.span_from(start);
        self.expect_line_end();

        let mut functions: Vec<Function> = Vec::new();
        loop {
            self.skip_newlines();
            let token: Token = self.peek();
            let return_type: Option<TypeRef> = match token.kind {
                TokenKind::Keyword(Keyword::Function | Keyword::Event) => None,
                _ if self.at_type_start()
                    && (self.lookahead_type_then(TokenKind::Keyword(Keyword::Function))
                        || self.lookahead_type_then(TokenKind::Keyword(Keyword::Event))) =>
                {
                    self.type_ref()
                }
                TokenKind::Keyword(keyword) if ends_block(keyword) => break,
                TokenKind::Eof => break,
                _ => {
                    self.unexpected("a function, an event or 'EndState'");
                    self.recover_line();
                    continue;
                }
            };
            match self.function(return_type, token.span) {
                Some(function) => functions.push(function),
                None => self.recover_line(),
            }
        }
        self.expect_end(Keyword::EndState, header_span, "state");
        Some(State {
            name,
            is_auto,
            functions,
            span: self.span_from(start),
        })
    }

    fn group(&mut self) -> Option<Group> {
        let start: Span = self.bump().span;
        let name: Ident = self.ident("a group name")?;
        let flags: Vec<Flag> = self.flags();
        let header_span: Span = self.span_from(start);
        let doc: Option<DocComment> = self.header_end();

        let mut properties: Vec<Property> = Vec::new();
        loop {
            self.skip_newlines();
            if self.at_type_start()
                && self.lookahead_type_then(TokenKind::Keyword(Keyword::Property))
            {
                if let Some(ty) = self.type_ref() {
                    match self.property(ty) {
                        Some(property) => properties.push(property),
                        None => self.recover_line(),
                    }
                }
                continue;
            }
            match self.peek().kind {
                TokenKind::Keyword(keyword) if ends_block(keyword) => break,
                TokenKind::Eof => break,
                _ => {
                    self.unexpected("a property or 'EndGroup'");
                    self.recover_line();
                }
            }
        }
        self.expect_end(Keyword::EndGroup, header_span, "group");
        Some(Group {
            name,
            flags,
            doc,
            properties,
            span: self.span_from(start),
        })
    }

    fn structure(&mut self) -> Option<Struct> {
        let start: Span = self.bump().span;
        let name: Ident = self.ident("a struct name")?;
        let header_span: Span = self.span_from(start);
        let doc: Option<DocComment> = self.header_end();

        let mut members: Vec<Variable> = Vec::new();
        loop {
            self.skip_newlines();
            if self.at_type_start() && self.lookahead_type_then(TokenKind::Ident) {
                if let Some(ty) = self.type_ref() {
                    match self.variable(ty) {
                        Some(member) => members.push(member),
                        None => self.recover_line(),
                    }
                }
                continue;
            }
            match self.peek().kind {
                TokenKind::Keyword(keyword) if ends_block(keyword) => break,
                TokenKind::Eof => break,
                _ => {
                    self.unexpected("a struct member or 'EndStruct'");
                    self.recover_line();
                }
            }
        }
        self.expect_end(Keyword::EndStruct, header_span, "struct");
        Some(Struct {
            name,
            doc,
            members,
            span: self.span_from(start),
        })
    }

    fn custom_event(&mut self) -> Option<CustomEvent> {
        let start: Span = self.bump().span;
        let name: Ident = self.ident("an event name")?;
        let span: Span = self.span_from(start);
        self.expect_line_end();
        Some(CustomEvent { name, span })
    }

    fn guard(&mut self) -> Option<Guard> {
        let start: Span = self.bump().span;
        let name: Ident = self.ident("a guard name")?;
        let flags: Vec<Flag> = self.flags();
        let span: Span = self.span_from(start);
        let doc: Option<DocComment> = self.header_end();
        Some(Guard {
            name,
            flags,
            doc,
            span,
        })
    }
}

fn is_end_keyword(keyword: Keyword) -> bool {
    matches!(
        keyword,
        Keyword::EndEvent
            | Keyword::EndFunction
            | Keyword::EndGroup
            | Keyword::EndIf
            | Keyword::EndLockGuard
            | Keyword::EndProperty
            | Keyword::EndState
            | Keyword::EndStruct
            | Keyword::EndTryLockGuard
            | Keyword::EndWhile
    )
}

/// Returns `true` if `found` closes a construct that encloses blocks closed by
/// `expected`, meaning the inner block is missing its end rather than closed
/// with the wrong keyword.
fn opens_outer(found: Keyword, expected: Keyword) -> bool {
    fn level(keyword: Keyword) -> u8 {
        match keyword {
            Keyword::EndState | Keyword::EndGroup | Keyword::EndStruct => 3,
            Keyword::EndProperty => 2,
            Keyword::EndFunction | Keyword::EndEvent => 1,
            _ => 0,
        }
    }
    level(found) > level(expected)
}
