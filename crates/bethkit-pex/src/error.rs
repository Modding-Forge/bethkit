// SPDX-License-Identifier: Apache-2.0
//!
//! Error types for the `bethkit-pex` crate.
//!
//! Every read error carries the byte offset where the problem was detected, so
//! damaged files can be inspected with a hex editor.

use crate::game::PexGame;

/// All errors that can be produced by `bethkit-pex` operations.
#[derive(Debug, thiserror::Error)]
pub enum PexError {
    /// A primitive could not be read or written.
    #[error(transparent)]
    Io(#[from] bethkit_io::IoError),

    /// The file does not start with the PEX magic `0xFA57C0DE`.
    #[error("not a PEX file: magic {found:#010x} at offset 0")]
    InvalidMagic {
        /// The first four bytes read as a little-endian integer.
        found: u32,
    },

    /// The major format version is not 3.
    #[error("unsupported PEX version {major}.{minor}")]
    UnsupportedVersion {
        /// The major version of the file.
        major: u8,
        /// The minor version of the file.
        minor: u8,
    },

    /// A little-endian file names a game ID that is not supported.
    #[error("unknown PEX game ID {game_id}")]
    UnknownGame {
        /// The game ID from the header.
        game_id: u16,
    },

    /// An instruction uses an opcode that does not exist at all.
    #[error("invalid opcode {opcode:#04x} at offset {offset}")]
    InvalidOpcode {
        /// The opcode byte.
        opcode: u8,
        /// Offset of the opcode byte.
        offset: usize,
    },

    /// An instruction uses an opcode that the file's game does not support.
    #[error("opcode {opcode:#04x} at offset {offset} is not supported by {game}")]
    OpcodeNotInGame {
        /// The opcode byte.
        opcode: u8,
        /// The game of the file.
        game: PexGame,
        /// Offset of the opcode byte.
        offset: usize,
    },

    /// The debug information flag is neither 0 nor 1.
    #[error("invalid debug information flag {value} at offset {offset}")]
    InvalidDebugFlag {
        /// The flag byte.
        value: u8,
        /// Offset of the flag byte.
        offset: usize,
    },

    /// A value has an unknown type tag.
    #[error("invalid value type {tag} at offset {offset}")]
    InvalidValueType {
        /// The type tag byte.
        tag: u8,
        /// Offset of the type tag.
        offset: usize,
    },

    /// The argument count of a variadic instruction is not a non-negative
    /// integer value.
    #[error("invalid variadic argument count at offset {offset}")]
    InvalidVariadicCount {
        /// Offset of the count value.
        offset: usize,
    },

    /// An instruction is built with the wrong number of arguments.
    #[error("{mnemonic} cannot take {fixed} fixed and {variadic} variadic arguments")]
    InvalidArity {
        /// Mnemonic of the opcode.
        mnemonic: &'static str,
        /// Number of fixed arguments given.
        fixed: usize,
        /// Number of variadic arguments given.
        variadic: usize,
    },

    /// A list has more entries than its 16-bit count field can store.
    #[error("{what} has {len} entries, more than the maximum of 65535")]
    TooManyEntries {
        /// What kind of list overflowed.
        what: &'static str,
        /// Number of entries in the list.
        len: usize,
    },

    /// The model contains data that the target game cannot store.
    #[error("{what} cannot be written for {game}")]
    UnsupportedFeature {
        /// What kind of data is not supported.
        what: &'static str,
        /// The game of the file.
        game: PexGame,
    },

    /// An object index is outside the objects of a view.
    #[error("object index {index} is out of range for {count} objects")]
    ObjectOutOfRange {
        /// The requested index.
        index: usize,
        /// Number of objects in the file.
        count: usize,
    },
}

/// Convenience alias for `Result<T, PexError>`.
pub type Result<T> = std::result::Result<T, PexError>;
