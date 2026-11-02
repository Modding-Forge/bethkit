// SPDX-License-Identifier: Apache-2.0
//!
//! PEX reader.
//!
//! The byte order is detected once from the magic number; the rest of the
//! file is read by code monomorphised for that byte order, so no field read
//! branches on it. Every error carries the byte offset of the problem.

use std::borrow::Cow;
use std::marker::PhantomData;

use bethkit_io::{BigEndian, ByteOrder, Endianness, LittleEndian, SliceCursor};

use crate::error::{PexError, Result};
use crate::game::PexGame;
use crate::model::{
    DebugFunction, DebugFunctionKind, DebugInfo, Function, FunctionFlags, Header, NamedFunction,
    Object, PexFile, Property, PropertyFlags, PropertyGroup, State, StringId, StringTable, Struct,
    StructMember, StructOrder, TypedName, UserFlag, Value, Variable,
};
use crate::opcode::Opcode;

/// The PEX magic number.
pub(crate) const MAGIC: u32 = 0xFA57_C0DE;

/// Detects the game and byte order of a PEX file from its first eight bytes.
///
/// # Errors
///
/// Returns [`PexError::InvalidMagic`], [`PexError::UnsupportedVersion`] or
/// [`PexError::UnknownGame`] for files that are not PEX files of a supported
/// game, and an I/O error if fewer than eight bytes are available.
pub(crate) fn detect(data: &[u8]) -> Result<PexGame> {
    let mut cursor = SliceCursor::new(data);
    let magic: u32 = cursor.read_u32_as::<LittleEndian>()?;
    let order: Endianness = if magic == MAGIC {
        Endianness::Little
    } else if magic == MAGIC.swap_bytes() {
        Endianness::Big
    } else {
        return Err(PexError::InvalidMagic { found: magic });
    };
    let major: u8 = cursor.read_u8()?;
    let minor: u8 = cursor.read_u8()?;
    if major != 3 {
        return Err(PexError::UnsupportedVersion { major, minor });
    }
    match order {
        // Only Skyrim stores scripts big-endian; its game ID is not needed.
        Endianness::Big => Ok(PexGame::Skyrim),
        Endianness::Little => {
            let game_id: u16 = cursor.read_u16_as::<LittleEndian>()?;
            PexGame::from_game_id(game_id).ok_or(PexError::UnknownGame { game_id })
        }
    }
}

/// Parses a complete PEX file.
///
/// # Errors
///
/// See [`PexFile::parse`].
pub(crate) fn parse(data: &[u8]) -> Result<PexFile<'_>> {
    let game: PexGame = detect(data)?;
    match game.profile().byte_order {
        Endianness::Big => Reader::<BigEndian>::new(data, game).file(),
        Endianness::Little => Reader::<LittleEndian>::new(data, game).file(),
    }
}

/// The parts of a PEX file before the objects.
pub(crate) struct Preamble<'a> {
    pub(crate) header: Header<'a>,
    pub(crate) strings: StringTable<'a>,
    pub(crate) debug_info: Option<DebugInfo>,
    pub(crate) user_flags: Vec<UserFlag>,
}

/// Reads PEX structures in byte order `O`.
pub(crate) struct Reader<'a, O: ByteOrder> {
    cursor: SliceCursor<'a>,
    game: PexGame,
    fallout4_layout: bool,
    guards: bool,
    order: PhantomData<O>,
}

impl<'a, O: ByteOrder> Reader<'a, O> {
    /// Creates a reader at the start of `data`.
    pub(crate) fn new(data: &'a [u8], game: PexGame) -> Self {
        let profile = game.profile();
        Self {
            cursor: SliceCursor::new(data),
            game,
            fallout4_layout: profile.has_fallout4_layout,
            guards: profile.has_guards,
            order: PhantomData,
        }
    }

    /// Creates a reader positioned at `offset`.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if `offset` is beyond the end of `data`.
    pub(crate) fn at(data: &'a [u8], game: PexGame, offset: usize) -> Result<Self> {
        let mut reader = Self::new(data, game);
        reader.cursor = SliceCursor::from_offset(data, offset)?;
        Ok(reader)
    }

    fn file(mut self) -> Result<PexFile<'a>> {
        let preamble: Preamble<'a> = self.preamble()?;
        let count: u16 = self.u16()?;
        let mut objects: Vec<Object> = Vec::with_capacity(usize::from(count));
        for _ in 0..count {
            objects.push(self.object()?);
        }
        self.finish()?;
        Ok(PexFile {
            game: self.game,
            header: preamble.header,
            strings: preamble.strings,
            debug_info: preamble.debug_info,
            user_flags: preamble.user_flags,
            objects,
        })
    }

    /// Fails if unread bytes remain.
    ///
    /// # Errors
    ///
    /// Returns [`PexError::TrailingData`] if the cursor is not at the end.
    pub(crate) fn finish(&self) -> Result<()> {
        if self.cursor.is_empty() {
            Ok(())
        } else {
            Err(PexError::TrailingData {
                offset: self.cursor.pos(),
                remaining: self.cursor.remaining(),
            })
        }
    }

    /// Reads the header, string table, debug information and user flags.
    ///
    /// # Errors
    ///
    /// Returns an I/O error for truncated data.
    pub(crate) fn preamble(&mut self) -> Result<Preamble<'a>> {
        // The magic, version and game were validated by `detect`.
        self.cursor.skip(4)?;
        let header = Header {
            major_version: self.cursor.read_u8()?,
            minor_version: self.cursor.read_u8()?,
            game_id: self.u16()?,
            compilation_time: self.cursor.read_u64_as::<O>()?,
            source_file_name: Cow::Borrowed(self.bytes()?),
            user_name: Cow::Borrowed(self.bytes()?),
            computer_name: Cow::Borrowed(self.bytes()?),
        };

        let count: u16 = self.u16()?;
        let mut strings = StringTable::with_capacity(usize::from(count));
        for _ in 0..count {
            // NOTE: at most u16::MAX entries are read, which `push` always accepts.
            strings.push(self.bytes()?)?;
        }

        let flag_offset: usize = self.cursor.pos();
        let debug_info: Option<DebugInfo> = match self.cursor.read_u8()? {
            0 => None,
            1 => Some(self.debug_info()?),
            // Other values would not survive a round trip, and no compiler writes them.
            value => {
                return Err(PexError::InvalidDebugFlag {
                    value,
                    offset: flag_offset,
                })
            }
        };

        let count: u16 = self.u16()?;
        let mut user_flags: Vec<UserFlag> = Vec::with_capacity(usize::from(count));
        for _ in 0..count {
            user_flags.push(UserFlag {
                name: self.id()?,
                bit: self.cursor.read_u8()?,
            });
        }

        Ok(Preamble {
            header,
            strings,
            debug_info,
            user_flags,
        })
    }

    /// Reads the object count.
    ///
    /// # Errors
    ///
    /// Returns an I/O error for truncated data.
    pub(crate) fn object_count(&mut self) -> Result<u16> {
        self.u16()
    }

    /// Skips one object using its size field and returns its byte range start.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if the object extends beyond the data.
    pub(crate) fn skip_object(&mut self) -> Result<usize> {
        let start: usize = self.cursor.pos();
        self.id()?;
        let declared: u32 = self.cursor.read_u32_as::<O>()?;
        // The size counts itself, so four of its bytes were already consumed.
        let rest: usize = usize::try_from(declared.saturating_sub(4)).unwrap_or(usize::MAX);
        self.cursor.skip(rest)?;
        Ok(start)
    }

    fn u16(&mut self) -> Result<u16> {
        Ok(self.cursor.read_u16_as::<O>()?)
    }

    fn u32(&mut self) -> Result<u32> {
        Ok(self.cursor.read_u32_as::<O>()?)
    }

    fn id(&mut self) -> Result<StringId> {
        Ok(StringId::new(self.u16()?))
    }

    fn bytes(&mut self) -> Result<&'a [u8]> {
        Ok(self.cursor.read_u16_prefixed_as::<O>()?)
    }

    fn ids(&mut self) -> Result<Vec<StringId>> {
        let count: u16 = self.u16()?;
        let mut ids: Vec<StringId> = Vec::with_capacity(usize::from(count));
        for _ in 0..count {
            ids.push(self.id()?);
        }
        Ok(ids)
    }

    fn debug_info(&mut self) -> Result<DebugInfo> {
        let modification_time: u64 = self.cursor.read_u64_as::<O>()?;
        let count: u16 = self.u16()?;
        let mut functions: Vec<DebugFunction> = Vec::with_capacity(usize::from(count));
        for _ in 0..count {
            let object_name: StringId = self.id()?;
            let state_name: StringId = self.id()?;
            let function_name: StringId = self.id()?;
            let kind = DebugFunctionKind::from_byte(self.cursor.read_u8()?);
            let lines: u16 = self.u16()?;
            let mut line_numbers: Vec<u16> = Vec::with_capacity(usize::from(lines));
            for _ in 0..lines {
                line_numbers.push(self.u16()?);
            }
            functions.push(DebugFunction {
                object_name,
                state_name,
                function_name,
                kind,
                line_numbers,
            });
        }

        let mut property_groups: Vec<PropertyGroup> = Vec::new();
        let mut struct_orders: Vec<StructOrder> = Vec::new();
        if self.fallout4_layout {
            let count: u16 = self.u16()?;
            property_groups.reserve(usize::from(count));
            for _ in 0..count {
                property_groups.push(PropertyGroup {
                    object_name: self.id()?,
                    group_name: self.id()?,
                    doc_string: self.id()?,
                    user_flags: self.u32()?,
                    property_names: self.ids()?,
                });
            }
            let count: u16 = self.u16()?;
            struct_orders.reserve(usize::from(count));
            for _ in 0..count {
                struct_orders.push(StructOrder {
                    object_name: self.id()?,
                    struct_name: self.id()?,
                    member_names: self.ids()?,
                });
            }
        }

        Ok(DebugInfo {
            modification_time,
            functions,
            property_groups,
            struct_orders,
        })
    }

    /// Reads one object and checks its declared size.
    ///
    /// # Errors
    ///
    /// Returns [`PexError::ObjectSizeMismatch`] if the declared size does not
    /// match, or any error of the contained functions and values.
    pub(crate) fn object(&mut self) -> Result<Object> {
        let name: StringId = self.id()?;
        let size_offset: usize = self.cursor.pos();
        let declared: u32 = self.u32()?;
        let parent_class_name: StringId = self.id()?;
        let doc_string: StringId = self.id()?;
        let const_flag: u8 = if self.fallout4_layout {
            self.cursor.read_u8()?
        } else {
            0
        };
        let user_flags: u32 = self.u32()?;
        let auto_state_name: StringId = self.id()?;

        let mut structs: Vec<Struct> = Vec::new();
        if self.fallout4_layout {
            let count: u16 = self.u16()?;
            structs.reserve(usize::from(count));
            for _ in 0..count {
                structs.push(self.structure()?);
            }
        }

        let count: u16 = self.u16()?;
        let mut variables: Vec<Variable> = Vec::with_capacity(usize::from(count));
        for _ in 0..count {
            variables.push(Variable {
                name: self.id()?,
                type_name: self.id()?,
                user_flags: self.u32()?,
                initial_value: self.value()?,
                const_flag: if self.fallout4_layout {
                    self.cursor.read_u8()?
                } else {
                    0
                },
            });
        }

        let guards: Vec<StringId> = if self.guards { self.ids()? } else { Vec::new() };

        let count: u16 = self.u16()?;
        let mut properties: Vec<Property> = Vec::with_capacity(usize::from(count));
        for _ in 0..count {
            properties.push(self.property()?);
        }

        let count: u16 = self.u16()?;
        let mut states: Vec<State> = Vec::with_capacity(usize::from(count));
        for _ in 0..count {
            let state_name: StringId = self.id()?;
            let functions: u16 = self.u16()?;
            let mut named: Vec<NamedFunction> = Vec::with_capacity(usize::from(functions));
            for _ in 0..functions {
                named.push(NamedFunction {
                    name: self.id()?,
                    function: self.function()?,
                });
            }
            states.push(State {
                name: state_name,
                functions: named,
            });
        }

        let actual: usize = self.cursor.pos() - size_offset;
        if usize::try_from(declared).ok() != Some(actual) {
            return Err(PexError::ObjectSizeMismatch {
                offset: size_offset,
                declared,
                actual,
            });
        }

        Ok(Object {
            name,
            parent_class_name,
            doc_string,
            const_flag,
            user_flags,
            auto_state_name,
            structs,
            variables,
            guards,
            properties,
            states,
        })
    }

    fn structure(&mut self) -> Result<Struct> {
        let name: StringId = self.id()?;
        let count: u16 = self.u16()?;
        let mut members: Vec<StructMember> = Vec::with_capacity(usize::from(count));
        for _ in 0..count {
            members.push(StructMember {
                name: self.id()?,
                type_name: self.id()?,
                user_flags: self.u32()?,
                initial_value: self.value()?,
                const_flag: self.cursor.read_u8()?,
                doc_string: self.id()?,
            });
        }
        Ok(Struct { name, members })
    }

    fn property(&mut self) -> Result<Property> {
        let name: StringId = self.id()?;
        let type_name: StringId = self.id()?;
        let doc_string: StringId = self.id()?;
        let user_flags: u32 = self.u32()?;
        let flags = PropertyFlags::from_bits_retain(self.cursor.read_u8()?);
        let mut property = Property {
            name,
            type_name,
            doc_string,
            user_flags,
            flags,
            auto_var_name: None,
            getter: None,
            setter: None,
        };
        if flags.contains(PropertyFlags::AUTO) {
            property.auto_var_name = Some(self.id()?);
        } else {
            if flags.contains(PropertyFlags::READ) {
                property.getter = Some(self.function()?);
            }
            if flags.contains(PropertyFlags::WRITE) {
                property.setter = Some(self.function()?);
            }
        }
        Ok(property)
    }

    fn typed_names(&mut self) -> Result<Vec<TypedName>> {
        let count: u16 = self.u16()?;
        let mut names: Vec<TypedName> = Vec::with_capacity(usize::from(count));
        for _ in 0..count {
            names.push(TypedName {
                name: self.id()?,
                type_name: self.id()?,
            });
        }
        Ok(names)
    }

    fn function(&mut self) -> Result<Function> {
        let return_type: StringId = self.id()?;
        let doc_string: StringId = self.id()?;
        let user_flags: u32 = self.u32()?;
        let flags = FunctionFlags::from_bits_retain(self.cursor.read_u8()?);
        let params: Vec<TypedName> = self.typed_names()?;
        let locals: Vec<TypedName> = self.typed_names()?;
        let mut function =
            Function::new(return_type, doc_string, user_flags, flags, params, locals);

        let count: u16 = self.u16()?;
        // Most instructions take three arguments; this avoids regrowing in the loop.
        function.reserve(usize::from(count), usize::from(count) * 3);
        for _ in 0..count {
            self.instruction(&mut function)?;
        }
        Ok(function)
    }

    fn instruction(&mut self, function: &mut Function) -> Result<()> {
        let offset: usize = self.cursor.pos();
        let byte: u8 = self.cursor.read_u8()?;
        let opcode = Opcode::from_byte(byte).ok_or(PexError::InvalidOpcode {
            opcode: byte,
            offset,
        })?;
        if !opcode.is_available_in(self.game) {
            return Err(PexError::OpcodeNotInGame {
                opcode: byte,
                game: self.game,
                offset,
            });
        }

        let info = opcode.info();
        let first_argument: u32 =
            u32::try_from(function.argument_count()).map_err(|_| PexError::TooManyEntries {
                what: "function arguments",
                len: function.argument_count(),
            })?;
        for _ in 0..info.fixed_args {
            let value: Value = self.value()?;
            function.push_argument(value);
        }

        let mut variadic_count: u16 = 0;
        if info.variadic {
            let count_offset: usize = self.cursor.pos();
            variadic_count = match self.value()? {
                Value::Integer(count) => u16::try_from(count).ok(),
                _ => None,
            }
            .ok_or(PexError::InvalidVariadicCount {
                offset: count_offset,
            })?;
            for _ in 0..variadic_count {
                let value: Value = self.value()?;
                function.push_argument(value);
            }
        }

        function.push_raw_instruction(opcode, first_argument, variadic_count);
        Ok(())
    }

    fn value(&mut self) -> Result<Value> {
        let offset: usize = self.cursor.pos();
        let tag: u8 = self.cursor.read_u8()?;
        let value: Value = match tag {
            0 => Value::None,
            1 => Value::Identifier(self.id()?),
            2 => Value::String(self.id()?),
            3 => Value::Integer(self.cursor.read_i32_as::<O>()?),
            4 => Value::Float(self.cursor.read_f32_as::<O>()?),
            5 => Value::Bool(self.cursor.read_u8()?),
            _ => return Err(PexError::InvalidValueType { tag, offset }),
        };
        Ok(value)
    }
}
