// SPDX-License-Identifier: Apache-2.0
//!
//! Lazily decoded view of a PEX file.
//!
//! [`PexView`] reads the header, string table, debug information and user
//! flags, and locates the objects through their size fields without decoding
//! them. Objects are the finest unit that can be skipped: functions have no
//! size field, so finding the end of one requires decoding its instructions.

use bethkit_io::{BigEndian, Endianness, LittleEndian};

use crate::error::{PexError, Result};
use crate::game::PexGame;
use crate::model::{DebugInfo, Header, Object, PexFile, StringTable, UserFlag};
use crate::read::{detect, Preamble, Reader};

/// A PEX file whose objects are decoded on demand.
#[derive(Clone, Debug)]
pub struct PexView<'a> {
    data: &'a [u8],
    game: PexGame,
    header: Header<'a>,
    strings: StringTable<'a>,
    debug_info: Option<DebugInfo>,
    user_flags: Vec<UserFlag>,
    object_starts: Vec<usize>,
}

impl<'a> PexView<'a> {
    /// Reads everything except the objects, which are only located.
    ///
    /// # Errors
    ///
    /// Returns [`PexError`] for malformed data before the objects, for objects
    /// whose size field points beyond the data, and for bytes after the last
    /// object.
    pub fn parse(data: &'a [u8]) -> Result<Self> {
        let game: PexGame = detect(data)?;
        match game.profile().byte_order {
            Endianness::Big => Self::scan::<BigEndian>(data, game),
            Endianness::Little => Self::scan::<LittleEndian>(data, game),
        }
    }

    fn scan<O: bethkit_io::ByteOrder>(data: &'a [u8], game: PexGame) -> Result<Self> {
        let mut reader: Reader<'a, O> = Reader::new(data, game);
        let preamble: Preamble<'a> = reader.preamble()?;
        let count: u16 = reader.object_count()?;
        let mut object_starts: Vec<usize> = Vec::with_capacity(usize::from(count));
        for _ in 0..count {
            object_starts.push(reader.skip_object()?);
        }
        reader.finish()?;
        Ok(Self {
            data,
            game,
            header: preamble.header,
            strings: preamble.strings,
            debug_info: preamble.debug_info,
            user_flags: preamble.user_flags,
            object_starts,
        })
    }

    /// Returns the game whose format the file uses.
    pub fn game(&self) -> PexGame {
        self.game
    }

    /// Returns the file header.
    pub fn header(&self) -> &Header<'a> {
        &self.header
    }

    /// Returns the string table.
    pub fn strings(&self) -> &StringTable<'a> {
        &self.strings
    }

    /// Returns the debug information, if present.
    pub fn debug_info(&self) -> Option<&DebugInfo> {
        self.debug_info.as_ref()
    }

    /// Returns the user flags.
    pub fn user_flags(&self) -> &[UserFlag] {
        &self.user_flags
    }

    /// Returns the number of objects.
    pub fn object_count(&self) -> usize {
        self.object_starts.len()
    }

    /// Decodes the object at `index`.
    ///
    /// # Errors
    ///
    /// Returns [`PexError::ObjectOutOfRange`] for an invalid index, or any
    /// decoding error of the object.
    pub fn object(&self, index: usize) -> Result<Object> {
        let start: usize = *self
            .object_starts
            .get(index)
            .ok_or(PexError::ObjectOutOfRange {
                index,
                count: self.object_starts.len(),
            })?;
        match self.game.profile().byte_order {
            Endianness::Big => Reader::<BigEndian>::at(self.data, self.game, start)?.object(),
            Endianness::Little => Reader::<LittleEndian>::at(self.data, self.game, start)?.object(),
        }
    }

    /// Decodes all objects and returns the complete file.
    ///
    /// # Errors
    ///
    /// Returns the first decoding error of any object.
    pub fn into_file(self) -> Result<PexFile<'a>> {
        let objects: Vec<Object> = (0..self.object_count())
            .map(|index| self.object(index))
            .collect::<Result<_>>()?;
        Ok(PexFile {
            game: self.game,
            header: self.header,
            strings: self.strings,
            debug_info: self.debug_info,
            user_flags: self.user_flags,
            objects,
        })
    }
}
