// SPDX-License-Identifier: Apache-2.0
//!
//! Lazily decoded view of a PEX file.
//!
//! [`PexView`] reads the header, string table, debug information and user
//! flags, and locates the objects through their size fields without decoding
//! them. Objects are the finest unit that can be skipped: functions have no
//! size field, so finding the end of one requires decoding its instructions.
//!
//! Size fields are only trusted when they lead exactly to the end of the
//! file. Otherwise (third-party compilers write wrong sizes, some tools leave
//! bytes after the last object) the objects are located by decoding them.

use bethkit_io::{BigEndian, ByteOrder, Endianness, LittleEndian};

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
    trailing_data: &'a [u8],
}

impl<'a> PexView<'a> {
    /// Reads everything except the objects, which are only located.
    ///
    /// # Errors
    ///
    /// Returns [`PexError`] for malformed data before the objects, and for
    /// objects that can neither be skipped nor decoded.
    pub fn parse(data: &'a [u8]) -> Result<Self> {
        let game: PexGame = detect(data)?;
        match game.profile().byte_order {
            Endianness::Big => Self::scan::<BigEndian>(data, game),
            Endianness::Little => Self::scan::<LittleEndian>(data, game),
        }
    }

    fn scan<O: ByteOrder>(data: &'a [u8], game: PexGame) -> Result<Self> {
        let mut reader: Reader<'a, O> = Reader::new(data, game);
        let preamble: Preamble<'a> = reader.preamble()?;
        let count: u16 = reader.object_count()?;
        let objects_offset: usize = reader.pos();

        let (object_starts, trailing_data) =
            match Self::locate_by_size::<O>(data, game, objects_offset, count) {
                Some(starts) => (starts, &data[data.len()..]),
                None => Self::locate_by_decoding::<O>(data, game, objects_offset, count)?,
            };

        Ok(Self {
            data,
            game,
            header: preamble.header,
            strings: preamble.strings,
            debug_info: preamble.debug_info,
            user_flags: preamble.user_flags,
            object_starts,
            trailing_data,
        })
    }

    /// Skips objects by their size fields; `None` unless they end exactly at
    /// the end of the data.
    fn locate_by_size<O: ByteOrder>(
        data: &'a [u8],
        game: PexGame,
        offset: usize,
        count: u16,
    ) -> Option<Vec<usize>> {
        let mut reader: Reader<'a, O> = Reader::at(data, game, offset).ok()?;
        let mut starts: Vec<usize> = Vec::with_capacity(usize::from(count));
        for _ in 0..count {
            starts.push(reader.skip_object().ok()?);
        }
        (reader.pos() == data.len()).then_some(starts)
    }

    /// Decodes objects to find their boundaries and the trailing bytes.
    fn locate_by_decoding<O: ByteOrder>(
        data: &'a [u8],
        game: PexGame,
        offset: usize,
        count: u16,
    ) -> Result<(Vec<usize>, &'a [u8])> {
        let mut reader: Reader<'a, O> = Reader::at(data, game, offset)?;
        let mut starts: Vec<usize> = Vec::with_capacity(usize::from(count));
        for _ in 0..count {
            starts.push(reader.pos());
            reader.object()?;
        }
        Ok((starts, reader.rest()?))
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

    /// Returns the bytes after the last object.
    pub fn trailing_data(&self) -> &'a [u8] {
        self.trailing_data
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
            trailing_data: std::borrow::Cow::Borrowed(self.trailing_data),
        })
    }
}
