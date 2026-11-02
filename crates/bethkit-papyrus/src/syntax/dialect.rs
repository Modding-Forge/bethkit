// SPDX-License-Identifier: Apache-2.0
//!
//! Papyrus language dialects.
//!
//! The language grew with each game. Words that are keywords in a later
//! dialect (for example `Struct` in Fallout 4) are ordinary identifiers in an
//! earlier one, so the dialect is chosen before lexing.

use std::fmt;

/// The game whose Papyrus dialect a script is written in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Dialect {
    /// Skyrim Legendary Edition, Special Edition and VR.
    Skyrim,
    /// Fallout 4 and Fallout 4 VR: structs, constants, groups, `Var`, custom
    /// events, namespaces and remote events.
    Fallout4,
    /// Fallout 76: the Fallout 4 language.
    Fallout76,
    /// Starfield: the Fallout 4 language plus guards.
    Starfield,
}

impl Dialect {
    /// Returns `true` for dialects with the Fallout 4 language additions.
    pub fn has_fallout4_syntax(self) -> bool {
        !matches!(self, Self::Skyrim)
    }

    /// Returns `true` for dialects with guard declarations and statements.
    pub fn has_guards(self) -> bool {
        matches!(self, Self::Starfield)
    }
}

impl fmt::Display for Dialect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Skyrim => f.write_str("Skyrim"),
            Self::Fallout4 => f.write_str("Fallout 4"),
            Self::Fallout76 => f.write_str("Fallout 76"),
            Self::Starfield => f.write_str("Starfield"),
        }
    }
}
