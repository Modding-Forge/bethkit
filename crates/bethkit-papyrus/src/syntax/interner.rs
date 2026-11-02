// SPDX-License-Identifier: Apache-2.0
//!
//! Case-insensitive identifier interning.
//!
//! Papyrus names are case-insensitive, so `GetActorBase` and `getactorbase`
//! must compare equal everywhere after parsing. Interning maps every spelling
//! to one [`Symbol`] while remembering the first spelling for display.

use ahash::AHashMap;

/// An interned, case-insensitive name. Cheap to copy and compare.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Symbol(u32);

impl Symbol {
    /// Returns the dense index of the symbol within its [`Interner`].
    pub fn index(self) -> usize {
        // NOTE: symbols are created from Vec indices, so this widening never truncates.
        self.0 as usize
    }
}

/// Stores the names of one or more parsed scripts.
#[derive(Clone, Debug, Default)]
pub struct Interner {
    by_key: AHashMap<Box<[u8]>, Symbol>,
    names: Vec<Box<str>>,
    scratch: Vec<u8>,
}

impl Interner {
    /// Creates an empty interner.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the symbol for `name`, creating it on first use.
    ///
    /// Lookups of known names do not allocate.
    ///
    /// # Panics
    ///
    /// Panics if more than `u32::MAX` distinct names are interned, which is
    /// impossible for sources limited to `u32::MAX` bytes.
    pub fn intern(&mut self, name: &str) -> Symbol {
        self.scratch.clear();
        self.scratch.extend_from_slice(name.as_bytes());
        self.scratch.make_ascii_lowercase();
        if let Some(symbol) = self.by_key.get(self.scratch.as_slice()) {
            return *symbol;
        }
        let symbol =
            Symbol(u32::try_from(self.names.len()).expect("interner holds at most u32::MAX names"));
        self.by_key.insert(self.scratch.as_slice().into(), symbol);
        self.names.push(name.into());
        symbol
    }

    /// Returns the symbol for `name` if it was interned before.
    pub fn get(&self, name: &str) -> Option<Symbol> {
        if name.bytes().any(|byte| byte.is_ascii_uppercase()) {
            let key: Vec<u8> = name.to_ascii_lowercase().into_bytes();
            self.by_key.get(key.as_slice()).copied()
        } else {
            self.by_key.get(name.as_bytes()).copied()
        }
    }

    /// Returns the first spelling that was interned for `symbol`, or an empty
    /// string for a symbol from another interner.
    pub fn resolve(&self, symbol: Symbol) -> &str {
        self.names.get(symbol.index()).map_or("", |name| name)
    }

    /// Returns the number of distinct names.
    pub fn len(&self) -> usize {
        self.names.len()
    }

    /// Returns `true` if no names were interned.
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that interning ignores case and keeps the first spelling.
    #[test]
    fn interning_is_case_insensitive() -> Result<(), Box<dyn std::error::Error>> {
        // given
        let mut interner = Interner::new();

        // when
        let first: Symbol = interner.intern("GetActorBase");
        let second: Symbol = interner.intern("getactorbase");
        let other: Symbol = interner.intern("Game");

        // then
        assert_eq!(first, second);
        assert_ne!(first, other);
        assert_eq!(interner.resolve(second), "GetActorBase");
        assert_eq!(interner.get("GAME"), Some(other));
        assert_eq!(interner.len(), 2);
        Ok(())
    }
}
