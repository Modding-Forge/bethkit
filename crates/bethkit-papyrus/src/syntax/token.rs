// SPDX-License-Identifier: Apache-2.0
//!
//! Tokens produced by the Papyrus lexer.
//!
//! Tokens carry only their kind and span; literal values are decoded from the
//! source by the parser, which keeps every token at 12 bytes.

use super::dialect::Dialect;
use crate::diagnostics::Span;

/// A Papyrus keyword. Keywords are case-insensitive.
///
/// Each variant is the keyword of the same name; `SelfRef` is `Self`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Keyword {
    As,
    Auto,
    AutoReadOnly,
    BetaOnly,
    Bool,
    Const,
    CustomEvent,
    DebugOnly,
    Else,
    ElseIf,
    EndEvent,
    EndFunction,
    EndGroup,
    EndIf,
    EndLockGuard,
    EndProperty,
    EndState,
    EndStruct,
    EndTryLockGuard,
    EndWhile,
    Event,
    Extends,
    False,
    Float,
    Function,
    Global,
    Group,
    Guard,
    If,
    Import,
    Int,
    Is,
    LockGuard,
    Native,
    New,
    None,
    Parent,
    Property,
    Return,
    ScriptName,
    SelfRef,
    State,
    String,
    Struct,
    True,
    TryLockGuard,
    Var,
    While,
}

/// Longest keyword (`EndTryLockGuard`), used to size the lookup buffer.
const MAX_KEYWORD_LEN: usize = 15;

impl Keyword {
    /// Looks up an identifier as a keyword of `dialect`, ignoring ASCII case.
    pub fn lookup(text: &[u8], dialect: Dialect) -> Option<Keyword> {
        if text.len() > MAX_KEYWORD_LEN {
            return None;
        }
        let mut buffer = [0u8; MAX_KEYWORD_LEN];
        let lower: &mut [u8] = &mut buffer[..text.len()];
        lower.copy_from_slice(text);
        lower.make_ascii_lowercase();

        let keyword: Keyword = match &*lower {
            b"as" => Self::As,
            b"auto" => Self::Auto,
            b"autoreadonly" => Self::AutoReadOnly,
            b"bool" => Self::Bool,
            b"else" => Self::Else,
            b"elseif" => Self::ElseIf,
            b"endevent" => Self::EndEvent,
            b"endfunction" => Self::EndFunction,
            b"endif" => Self::EndIf,
            b"endproperty" => Self::EndProperty,
            b"endstate" => Self::EndState,
            b"endwhile" => Self::EndWhile,
            b"event" => Self::Event,
            b"extends" => Self::Extends,
            b"false" => Self::False,
            b"float" => Self::Float,
            b"function" => Self::Function,
            b"global" => Self::Global,
            b"if" => Self::If,
            b"import" => Self::Import,
            b"int" => Self::Int,
            b"native" => Self::Native,
            b"new" => Self::New,
            b"none" => Self::None,
            b"parent" => Self::Parent,
            b"property" => Self::Property,
            b"return" => Self::Return,
            b"scriptname" => Self::ScriptName,
            b"self" => Self::SelfRef,
            b"state" => Self::State,
            b"string" => Self::String,
            b"true" => Self::True,
            b"while" => Self::While,
            b"betaonly" => Self::BetaOnly,
            b"const" => Self::Const,
            b"customevent" => Self::CustomEvent,
            b"debugonly" => Self::DebugOnly,
            b"endgroup" => Self::EndGroup,
            b"endstruct" => Self::EndStruct,
            b"group" => Self::Group,
            b"is" => Self::Is,
            b"struct" => Self::Struct,
            b"var" => Self::Var,
            b"guard" => Self::Guard,
            b"lockguard" => Self::LockGuard,
            b"endlockguard" => Self::EndLockGuard,
            b"trylockguard" => Self::TryLockGuard,
            b"endtrylockguard" => Self::EndTryLockGuard,
            _ => return None,
        };
        keyword.is_available_in(dialect).then_some(keyword)
    }

    /// Returns `true` if the keyword is reserved in `dialect`.
    pub fn is_available_in(self, dialect: Dialect) -> bool {
        match self {
            Self::BetaOnly
            | Self::Const
            | Self::CustomEvent
            | Self::DebugOnly
            | Self::EndGroup
            | Self::EndStruct
            | Self::Group
            | Self::Is
            | Self::Struct
            | Self::Var => dialect.has_fallout4_syntax(),
            Self::Guard
            | Self::LockGuard
            | Self::EndLockGuard
            | Self::TryLockGuard
            | Self::EndTryLockGuard => dialect.has_guards(),
            _ => true,
        }
    }

    /// Returns `true` for keywords that may appear in a declaration's flag
    /// list, such as `Global` or `Auto`.
    pub fn is_flag(self) -> bool {
        matches!(
            self,
            Self::Auto
                | Self::AutoReadOnly
                | Self::BetaOnly
                | Self::Const
                | Self::DebugOnly
                | Self::Global
                | Self::Native
        )
    }
}

/// The kind of a [`Token`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TokenKind {
    /// A name that is not a keyword of the dialect.
    Ident,
    /// A keyword.
    Keyword(Keyword),
    /// A decimal or `0x` hexadecimal integer literal.
    Int,
    /// A floating-point literal.
    Float,
    /// A double-quoted string literal, quotes included in the span.
    String,
    /// A `{ ... }` documentation comment, braces included in the span.
    DocComment,
    /// `(`
    LParen,
    /// `)`
    RParen,
    /// `[`
    LBracket,
    /// `]`
    RBracket,
    /// `,`
    Comma,
    /// `.`
    Dot,
    /// `:`
    Colon,
    /// `=`
    Assign,
    /// `+=`
    PlusAssign,
    /// `-=`
    MinusAssign,
    /// `*=`
    StarAssign,
    /// `/=`
    SlashAssign,
    /// `%=`
    PercentAssign,
    /// `+`
    Plus,
    /// `-`
    Minus,
    /// `*`
    Star,
    /// `/`
    Slash,
    /// `%`
    Percent,
    /// `!`
    Bang,
    /// `==`
    EqEq,
    /// `!=`
    NotEq,
    /// `<`
    Lt,
    /// `<=`
    Le,
    /// `>`
    Gt,
    /// `>=`
    Ge,
    /// `&&`
    AndAnd,
    /// `||`
    OrOr,
    /// End of a logical line.
    Newline,
    /// End of input; always the last token.
    Eof,
}

/// A lexed token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Token {
    /// What kind of token this is.
    pub kind: TokenKind,
    /// Where the token is in the source.
    pub span: Span,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies case-insensitive lookup and dialect-specific keywords.
    #[test]
    fn keywords_depend_on_dialect() -> Result<(), Box<dyn std::error::Error>> {
        // then
        assert_eq!(
            Keyword::lookup(b"EndFunction", Dialect::Skyrim),
            Some(Keyword::EndFunction)
        );
        assert_eq!(Keyword::lookup(b"STRUCT", Dialect::Skyrim), None);
        assert_eq!(
            Keyword::lookup(b"STRUCT", Dialect::Fallout4),
            Some(Keyword::Struct)
        );
        assert_eq!(Keyword::lookup(b"LockGuard", Dialect::Fallout4), None);
        assert_eq!(
            Keyword::lookup(b"lockguard", Dialect::Starfield),
            Some(Keyword::LockGuard)
        );
        assert_eq!(Keyword::lookup(b"length", Dialect::Starfield), None);
        assert_eq!(
            Keyword::lookup(b"averyveryverylongname", Dialect::Skyrim),
            None
        );
        Ok(())
    }
}
