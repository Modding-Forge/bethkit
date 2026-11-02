// SPDX-License-Identifier: Apache-2.0
//!
//! PEX writer.
//!
//! Writes the model in the byte order of its game. Object sizes are reserved
//! and back-patched once each object is complete. Data the game's format
//! cannot hold is rejected instead of being dropped silently.

use bethkit_io::{BigEndian, ByteOrder, ByteWriter, Endianness, LittleEndian, U32Slot};

use crate::error::{PexError, Result};
use crate::game::PexGame;
use crate::model::{
    DebugInfo, Function, Object, PexFile, Property, PropertyFlags, StringId, TypedName, Value,
};
use crate::read::MAGIC;

/// Serializes a PEX file.
///
/// # Errors
///
/// See [`PexFile::write`].
pub(crate) fn write(file: &PexFile<'_>) -> Result<Vec<u8>> {
    match file.game.profile().byte_order {
        Endianness::Big => Writer::<BigEndian>::new(file.game).file(file),
        Endianness::Little => Writer::<LittleEndian>::new(file.game).file(file),
    }
}

/// Converts a list length into its 16-bit count field.
fn count(len: usize, what: &'static str) -> Result<u16> {
    u16::try_from(len).map_err(|_| PexError::TooManyEntries { what, len })
}

struct Writer<O: ByteOrder> {
    out: ByteWriter<O>,
    game: PexGame,
    fallout4_layout: bool,
    guards: bool,
}

impl<O: ByteOrder> Writer<O> {
    fn new(game: PexGame) -> Self {
        let profile = game.profile();
        Self {
            out: ByteWriter::with_capacity(64 * 1024),
            game,
            fallout4_layout: profile.has_fallout4_layout,
            guards: profile.has_guards,
        }
    }

    fn unsupported(&self, what: &'static str) -> PexError {
        PexError::UnsupportedFeature {
            what,
            game: self.game,
        }
    }

    fn file(mut self, file: &PexFile<'_>) -> Result<Vec<u8>> {
        let header = &file.header;
        self.out.write_u32(MAGIC);
        self.out.write_u8(header.major_version);
        self.out.write_u8(header.minor_version);
        self.out.write_u16(header.game_id);
        self.out.write_u64(header.compilation_time);
        self.out.write_u16_prefixed(&header.source_file_name)?;
        self.out.write_u16_prefixed(&header.user_name)?;
        self.out.write_u16_prefixed(&header.computer_name)?;

        let entries = file.strings.entries();
        self.out.write_u16(count(entries.len(), "string table")?);
        for entry in entries {
            self.out.write_u16_prefixed(entry)?;
        }

        match &file.debug_info {
            Some(debug_info) => {
                self.out.write_u8(1);
                self.debug_info(debug_info)?;
            }
            None => self.out.write_u8(0),
        }

        self.out
            .write_u16(count(file.user_flags.len(), "user flags")?);
        for flag in &file.user_flags {
            self.id(flag.name);
            self.out.write_u8(flag.bit);
        }

        self.out.write_u16(count(file.objects.len(), "objects")?);
        for object in &file.objects {
            self.object(object)?;
        }
        Ok(self.out.into_inner())
    }

    fn id(&mut self, id: StringId) {
        self.out.write_u16(id.raw());
    }

    fn ids(&mut self, ids: &[StringId], what: &'static str) -> Result<()> {
        self.out.write_u16(count(ids.len(), what)?);
        for id in ids {
            self.id(*id);
        }
        Ok(())
    }

    fn debug_info(&mut self, debug_info: &DebugInfo) -> Result<()> {
        self.out.write_u64(debug_info.modification_time);
        self.out
            .write_u16(count(debug_info.functions.len(), "debug functions")?);
        for function in &debug_info.functions {
            self.id(function.object_name);
            self.id(function.state_name);
            self.id(function.function_name);
            self.out.write_u8(function.kind.to_byte());
            self.out
                .write_u16(count(function.line_numbers.len(), "line numbers")?);
            for line in &function.line_numbers {
                self.out.write_u16(*line);
            }
        }

        if !self.fallout4_layout {
            if !debug_info.property_groups.is_empty() || !debug_info.struct_orders.is_empty() {
                return Err(self.unsupported("property groups and struct orders"));
            }
            return Ok(());
        }
        self.out
            .write_u16(count(debug_info.property_groups.len(), "property groups")?);
        for group in &debug_info.property_groups {
            self.id(group.object_name);
            self.id(group.group_name);
            self.id(group.doc_string);
            self.out.write_u32(group.user_flags);
            self.ids(&group.property_names, "property group members")?;
        }
        self.out
            .write_u16(count(debug_info.struct_orders.len(), "struct orders")?);
        for order in &debug_info.struct_orders {
            self.id(order.object_name);
            self.id(order.struct_name);
            self.ids(&order.member_names, "struct order members")?;
        }
        Ok(())
    }

    fn object(&mut self, object: &Object) -> Result<()> {
        self.id(object.name);
        let size: U32Slot = self.out.reserve_u32();
        self.id(object.parent_class_name);
        self.id(object.doc_string);
        if self.fallout4_layout {
            self.out.write_u8(object.const_flag);
        } else if object.const_flag != 0 {
            return Err(self.unsupported("const scripts"));
        }
        self.out.write_u32(object.user_flags);
        self.id(object.auto_state_name);

        if self.fallout4_layout {
            self.out.write_u16(count(object.structs.len(), "structs")?);
            for structure in &object.structs {
                self.id(structure.name);
                self.out
                    .write_u16(count(structure.members.len(), "struct members")?);
                for member in &structure.members {
                    self.id(member.name);
                    self.id(member.type_name);
                    self.out.write_u32(member.user_flags);
                    self.value(member.initial_value);
                    self.out.write_u8(member.const_flag);
                    self.id(member.doc_string);
                }
            }
        } else if !object.structs.is_empty() {
            return Err(self.unsupported("structs"));
        }

        self.out
            .write_u16(count(object.variables.len(), "variables")?);
        for variable in &object.variables {
            self.id(variable.name);
            self.id(variable.type_name);
            self.out.write_u32(variable.user_flags);
            self.value(variable.initial_value);
            if self.fallout4_layout {
                self.out.write_u8(variable.const_flag);
            } else if variable.const_flag != 0 {
                return Err(self.unsupported("const variables"));
            }
        }

        if self.guards {
            self.ids(&object.guards, "guards")?;
        } else if !object.guards.is_empty() {
            return Err(self.unsupported("guards"));
        }

        self.out
            .write_u16(count(object.properties.len(), "properties")?);
        for property in &object.properties {
            self.property(property)?;
        }

        self.out.write_u16(count(object.states.len(), "states")?);
        for state in &object.states {
            self.id(state.name);
            self.out
                .write_u16(count(state.functions.len(), "state functions")?);
            for named in &state.functions {
                self.id(named.name);
                self.function(&named.function)?;
            }
        }

        self.out.patch_u32_with_size_since(size)?;
        Ok(())
    }

    fn property(&mut self, property: &Property) -> Result<()> {
        self.id(property.name);
        self.id(property.type_name);
        self.id(property.doc_string);
        self.out.write_u32(property.user_flags);
        self.out.write_u8(property.flags.bits());
        if property.flags.contains(PropertyFlags::AUTO) {
            // An auto property without a backing variable cannot be encoded.
            let auto_var: StringId = property
                .auto_var_name
                .ok_or_else(|| self.unsupported("auto properties without a variable"))?;
            self.id(auto_var);
            return Ok(());
        }
        for (flag, accessor, what) in [
            (
                PropertyFlags::READ,
                &property.getter,
                "readable properties without a getter",
            ),
            (
                PropertyFlags::WRITE,
                &property.setter,
                "writable properties without a setter",
            ),
        ] {
            if property.flags.contains(flag) {
                let function: &Function =
                    accessor.as_ref().ok_or_else(|| self.unsupported(what))?;
                self.function(function)?;
            }
        }
        Ok(())
    }

    fn typed_names(&mut self, names: &[TypedName], what: &'static str) -> Result<()> {
        self.out.write_u16(count(names.len(), what)?);
        for name in names {
            self.id(name.name);
            self.id(name.type_name);
        }
        Ok(())
    }

    fn function(&mut self, function: &Function) -> Result<()> {
        self.id(function.return_type);
        self.id(function.doc_string);
        self.out.write_u32(function.user_flags);
        self.out.write_u8(function.flags.bits());
        self.typed_names(&function.params, "parameters")?;
        self.typed_names(&function.locals, "locals")?;
        self.out
            .write_u16(count(function.instruction_count(), "instructions")?);
        for instruction in function.instructions() {
            if !instruction.opcode.is_available_in(self.game) {
                return Err(PexError::OpcodeNotInGame {
                    opcode: instruction.opcode as u8,
                    game: self.game,
                    offset: self.out.len(),
                });
            }
            self.out.write_u8(instruction.opcode as u8);
            for value in instruction.fixed {
                self.value(*value);
            }
            if instruction.opcode.info().variadic {
                // NOTE: variadic lists hold at most u16::MAX values by construction.
                let len: i32 = i32::try_from(instruction.variadic.len()).unwrap_or(i32::MAX);
                self.value(Value::Integer(len));
                for value in instruction.variadic {
                    self.value(*value);
                }
            }
        }
        Ok(())
    }

    fn value(&mut self, value: Value) {
        match value {
            Value::None => self.out.write_u8(0),
            Value::Identifier(id) => {
                self.out.write_u8(1);
                self.id(id);
            }
            Value::String(id) => {
                self.out.write_u8(2);
                self.id(id);
            }
            Value::Integer(number) => {
                self.out.write_u8(3);
                self.out.write_i32(number);
            }
            Value::Float(number) => {
                self.out.write_u8(4);
                self.out.write_f32(number);
            }
            Value::Bool(raw) => {
                self.out.write_u8(5);
                self.out.write_u8(raw);
            }
        }
    }
}
