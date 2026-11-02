// SPDX-License-Identifier: Apache-2.0
//!
//! The data model of a PEX file.
//!
//! The types mirror the file layout one to one, so writing an unmodified model
//! reproduces the source bytes exactly. Every name in the model is a
//! [`StringId`] into the [`StringTable`]. Raw flag bytes are kept unchanged,
//! including bits without a known meaning.
//!
//! The model is plain data: it does not prevent dangling string IDs or
//! inconsistent debug line tables. Such problems are reported by validation,
//! and the writer only rejects what cannot be encoded at all.

mod function;
mod string_table;

use std::borrow::Cow;

use bitflags::bitflags;

pub use function::{Function, Instruction, InstructionRef, TypedName};
pub use string_table::{StringId, StringTable};

use crate::error::Result;
use crate::game::PexGame;

/// A complete PEX file.
#[derive(Clone, Debug, PartialEq)]
pub struct PexFile<'a> {
    /// The game whose format the file uses.
    pub game: PexGame,
    /// The file header.
    pub header: Header<'a>,
    /// The string table holding every name and string literal.
    pub strings: StringTable<'a>,
    /// Debug information, if the script was compiled with it.
    pub debug_info: Option<DebugInfo>,
    /// The user flag names and their bit positions.
    pub user_flags: Vec<UserFlag>,
    /// The script objects; compiled scripts contain exactly one.
    pub objects: Vec<Object>,
}

impl<'a> PexFile<'a> {
    /// Parses a PEX file, borrowing strings from `data`.
    ///
    /// # Errors
    ///
    /// Returns [`crate::PexError`] with the byte offset of the first problem
    /// for truncated or malformed data, unsupported versions or games, and
    /// opcodes the file's game does not support.
    pub fn parse(data: &'a [u8]) -> Result<Self> {
        crate::read::parse(data)
    }

    /// Serializes the file in the byte order of its game.
    ///
    /// # Errors
    ///
    /// Returns [`crate::PexError::TooManyEntries`] if a list exceeds its
    /// 16-bit count, [`crate::PexError::UnsupportedFeature`] if the model
    /// contains data the game cannot store, or an I/O error if a string or
    /// object is too large.
    pub fn write(&self) -> Result<Vec<u8>> {
        crate::write::write(self)
    }

    /// Copies all borrowed strings, detaching the file from its source buffer.
    pub fn into_owned(self) -> PexFile<'static> {
        PexFile {
            game: self.game,
            header: self.header.into_owned(),
            strings: self.strings.into_owned(),
            debug_info: self.debug_info,
            user_flags: self.user_flags,
            objects: self.objects,
        }
    }
}

/// The PEX file header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Header<'a> {
    /// Major format version; always 3.
    pub major_version: u8,
    /// Minor format version.
    pub minor_version: u8,
    /// Game ID as stored in the file.
    pub game_id: u16,
    /// Compilation time as a Unix timestamp.
    pub compilation_time: u64,
    /// Name of the source file the script was compiled from.
    pub source_file_name: Cow<'a, [u8]>,
    /// Name of the user who compiled the script.
    pub user_name: Cow<'a, [u8]>,
    /// Name of the computer the script was compiled on.
    pub computer_name: Cow<'a, [u8]>,
}

impl Header<'_> {
    /// Copies all borrowed strings.
    pub fn into_owned(self) -> Header<'static> {
        Header {
            major_version: self.major_version,
            minor_version: self.minor_version,
            game_id: self.game_id,
            compilation_time: self.compilation_time,
            source_file_name: Cow::Owned(self.source_file_name.into_owned()),
            user_name: Cow::Owned(self.user_name.into_owned()),
            computer_name: Cow::Owned(self.computer_name.into_owned()),
        }
    }
}

/// Debug information: source line numbers and editor metadata.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DebugInfo {
    /// Modification time of the source file as a Unix timestamp.
    pub modification_time: u64,
    /// Line tables of the functions.
    pub functions: Vec<DebugFunction>,
    /// Property groups (Fallout 4 and later).
    pub property_groups: Vec<PropertyGroup>,
    /// Declaration order of struct members (Fallout 4 and later).
    pub struct_orders: Vec<StructOrder>,
}

/// The kind of function a [`DebugFunction`] describes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DebugFunctionKind {
    /// An ordinary function or event.
    Normal,
    /// A property getter.
    Getter,
    /// A property setter.
    Setter,
    /// A value without a known meaning, kept for exact round trips.
    Other(u8),
}

impl DebugFunctionKind {
    /// Returns the kind stored as `byte`.
    pub fn from_byte(byte: u8) -> Self {
        match byte {
            0 => Self::Normal,
            1 => Self::Getter,
            2 => Self::Setter,
            other => Self::Other(other),
        }
    }

    /// Returns the byte that stores the kind.
    pub fn to_byte(self) -> u8 {
        match self {
            Self::Normal => 0,
            Self::Getter => 1,
            Self::Setter => 2,
            Self::Other(other) => other,
        }
    }
}

/// Source line numbers of one function.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DebugFunction {
    /// Name of the object containing the function.
    pub object_name: StringId,
    /// Name of the state containing the function; empty for the default state.
    pub state_name: StringId,
    /// Name of the function, or of the property for accessors.
    pub function_name: StringId,
    /// What kind of function this is.
    pub kind: DebugFunctionKind,
    /// The source line of each instruction, in instruction order.
    pub line_numbers: Vec<u16>,
}

/// A group of properties shown together in the editor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PropertyGroup {
    /// Name of the object containing the group.
    pub object_name: StringId,
    /// Name of the group; empty for ungrouped properties.
    pub group_name: StringId,
    /// Documentation of the group.
    pub doc_string: StringId,
    /// User flags of the group.
    pub user_flags: u32,
    /// Names of the properties in the group, in declaration order.
    pub property_names: Vec<StringId>,
}

/// The declaration order of the members of a struct.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructOrder {
    /// Name of the object containing the struct.
    pub object_name: StringId,
    /// Name of the struct.
    pub struct_name: StringId,
    /// Names of the members in declaration order.
    pub member_names: Vec<StringId>,
}

/// A user flag name and the bit it occupies in user flag masks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct UserFlag {
    /// Name of the flag, for example `Hidden`.
    pub name: StringId,
    /// Bit position of the flag.
    pub bit: u8,
}

/// A literal value: an argument, an initial value or a default value.
///
/// Floats compare by bit pattern, so a model equals itself even when it holds
/// NaN values, and equal models always serialize to equal bytes.
#[derive(Clone, Copy, Debug)]
pub enum Value {
    /// `None`.
    None,
    /// A name: a variable, parameter, function, property, type or state.
    Identifier(StringId),
    /// A string literal.
    String(StringId),
    /// An integer literal.
    Integer(i32),
    /// A float literal; the bit pattern is preserved exactly.
    Float(f32),
    /// A boolean literal; the raw byte is kept for exact round trips.
    Bool(u8),
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::None, Self::None) => true,
            (Self::Identifier(a), Self::Identifier(b)) | (Self::String(a), Self::String(b)) => {
                a == b
            }
            (Self::Integer(a), Self::Integer(b)) => a == b,
            (Self::Float(a), Self::Float(b)) => a.to_bits() == b.to_bits(),
            (Self::Bool(a), Self::Bool(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for Value {}

impl Value {
    /// Returns the string table entry the value refers to, for identifiers and
    /// string literals.
    pub fn string_id(self) -> Option<StringId> {
        match self {
            Self::Identifier(id) | Self::String(id) => Some(id),
            _ => None,
        }
    }
}

/// A script object.
#[derive(Clone, Debug, PartialEq)]
pub struct Object {
    /// Name of the object (the script name).
    pub name: StringId,
    /// Name of the parent script; empty for none.
    pub parent_class_name: StringId,
    /// Documentation of the script.
    pub doc_string: StringId,
    /// Raw const flag (Fallout 4 and later; 0 for Skyrim).
    pub const_flag: u8,
    /// User flags of the script.
    pub user_flags: u32,
    /// Name of the initial state; empty for the default state.
    pub auto_state_name: StringId,
    /// Struct definitions (Fallout 4 and later).
    pub structs: Vec<Struct>,
    /// Script variables, including auto-property backing variables.
    pub variables: Vec<Variable>,
    /// Guard names (Starfield).
    pub guards: Vec<StringId>,
    /// Properties.
    pub properties: Vec<Property>,
    /// States, including the empty-named default state.
    pub states: Vec<State>,
}

/// A struct definition (Fallout 4 and later).
#[derive(Clone, Debug, PartialEq)]
pub struct Struct {
    /// Name of the struct.
    pub name: StringId,
    /// Members in file order.
    pub members: Vec<StructMember>,
}

/// A member of a struct.
#[derive(Clone, Debug, PartialEq)]
pub struct StructMember {
    /// Name of the member.
    pub name: StringId,
    /// Type of the member.
    pub type_name: StringId,
    /// User flags of the member.
    pub user_flags: u32,
    /// Initial value.
    pub initial_value: Value,
    /// Raw const flag.
    pub const_flag: u8,
    /// Documentation of the member.
    pub doc_string: StringId,
}

/// A script variable.
#[derive(Clone, Debug, PartialEq)]
pub struct Variable {
    /// Name of the variable.
    pub name: StringId,
    /// Type of the variable.
    pub type_name: StringId,
    /// User flags of the variable.
    pub user_flags: u32,
    /// Initial value.
    pub initial_value: Value,
    /// Raw const flag (Fallout 4 and later; 0 for Skyrim).
    pub const_flag: u8,
}

bitflags! {
    /// Flags of a property. Unknown bits are preserved.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct PropertyFlags: u8 {
        /// The property can be read.
        const READ = 1;
        /// The property can be written.
        const WRITE = 2;
        /// The property is backed by an automatic variable.
        const AUTO = 4;
    }
}

bitflags! {
    /// Flags of a function. Unknown bits are preserved.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct FunctionFlags: u8 {
        /// The function is global (static).
        const GLOBAL = 1;
        /// The function is implemented by the engine.
        const NATIVE = 2;
    }
}

/// A property.
#[derive(Clone, Debug, PartialEq)]
pub struct Property {
    /// Name of the property.
    pub name: StringId,
    /// Type of the property.
    pub type_name: StringId,
    /// Documentation of the property.
    pub doc_string: StringId,
    /// User flags of the property.
    pub user_flags: u32,
    /// Property flags; [`PropertyFlags::AUTO`] decides whether
    /// `auto_var_name` or the accessors are stored.
    pub flags: PropertyFlags,
    /// Backing variable of an auto property.
    pub auto_var_name: Option<StringId>,
    /// Getter of a full readable property.
    pub getter: Option<Function>,
    /// Setter of a full writable property.
    pub setter: Option<Function>,
}

/// A state and the functions defined in it.
#[derive(Clone, Debug, PartialEq)]
pub struct State {
    /// Name of the state; empty for the default state.
    pub name: StringId,
    /// Functions and events defined in the state.
    pub functions: Vec<NamedFunction>,
}

/// A function together with its name.
#[derive(Clone, Debug, PartialEq)]
pub struct NamedFunction {
    /// Name of the function or event.
    pub name: StringId,
    /// The function.
    pub function: Function,
}
