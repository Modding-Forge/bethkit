// SPDX-License-Identifier: Apache-2.0
//!
//! The PEX string table.
//!
//! Entries are raw bytes. PEX stores no encoding: names are ASCII, string
//! literals use whatever code page the compiler saw, and some tools write
//! UTF-8. Keeping bytes makes round trips exact; decoding is left to callers.

use std::borrow::Cow;
use std::fmt;

use crate::error::{PexError, Result};

/// Index of an entry in a [`StringTable`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StringId(u16);

impl StringId {
    /// Creates an ID from its raw 16-bit index.
    pub fn new(index: u16) -> Self {
        Self(index)
    }

    /// Returns the raw 16-bit index.
    pub fn raw(self) -> u16 {
        self.0
    }

    /// Returns the index as `usize` for slicing.
    pub fn index(self) -> usize {
        usize::from(self.0)
    }
}

impl fmt::Display for StringId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.0)
    }
}

/// The string table of a PEX file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StringTable<'a> {
    entries: Vec<Cow<'a, [u8]>>,
}

impl<'a> StringTable<'a> {
    /// Creates an empty table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a table that can hold `capacity` entries without reallocating.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: Vec::with_capacity(capacity),
        }
    }

    /// Returns the number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` if the table has no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns the bytes of an entry, or `None` for an index out of range.
    pub fn get(&self, id: StringId) -> Option<&[u8]> {
        self.entries.get(id.index()).map(|entry| &**entry)
    }

    /// Iterates over all entries with their IDs.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (StringId, &[u8])> {
        self.entries.iter().enumerate().map(|(index, entry)| {
            // NOTE: the table never holds more than u16::MAX entries, see `push`.
            let raw: u16 = u16::try_from(index).unwrap_or(u16::MAX);
            (StringId(raw), &**entry)
        })
    }

    /// Appends an entry and returns its ID.
    ///
    /// # Errors
    ///
    /// Returns [`PexError::TooManyEntries`] if the table already holds the
    /// maximum of 65535 entries.
    pub fn push(&mut self, entry: impl Into<Cow<'a, [u8]>>) -> Result<StringId> {
        let raw: u16 = u16::try_from(self.entries.len())
            .ok()
            .filter(|raw| *raw < u16::MAX)
            .ok_or(PexError::TooManyEntries {
                what: "string table",
                len: self.entries.len() + 1,
            })?;
        self.entries.push(entry.into());
        Ok(StringId(raw))
    }

    /// Copies all borrowed entries.
    pub fn into_owned(self) -> StringTable<'static> {
        StringTable {
            entries: self
                .entries
                .into_iter()
                .map(|entry| Cow::Owned(entry.into_owned()))
                .collect(),
        }
    }

    /// Returns the entries for serialization.
    pub(crate) fn entries(&self) -> &[Cow<'a, [u8]>] {
        &self.entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies lookup, iteration and the entry limit.
    #[test]
    fn table_lookup_and_limit() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let mut table = StringTable::new();
        let first: StringId = table.push(&b"Actor"[..])?;
        let second: StringId = table.push(b"text".to_vec())?;

        // then
        assert_eq!(table.get(first), Some(&b"Actor"[..]));
        assert_eq!(table.get(StringId::new(9)), None);
        assert_eq!(
            table.iter().map(|(id, _)| id).collect::<Vec<_>>(),
            vec![first, second]
        );
        assert_eq!(second.to_string(), "#1");

        let mut full = StringTable::with_capacity(usize::from(u16::MAX));
        for _ in 0..u16::MAX {
            full.push(&b""[..])?;
        }
        assert!(matches!(
            full.push(&b""[..]),
            Err(PexError::TooManyEntries { .. })
        ));
        Ok(())
    }
}
