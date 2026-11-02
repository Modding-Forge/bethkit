// SPDX-License-Identifier: Apache-2.0
//!
//! Compiled Papyrus script (`.pex`) support for bethkit.
//!
//! This crate reads and writes `.pex` files of Skyrim (LE, SE, VR), Fallout 4
//! (including VR), Fallout 76 and Starfield. The byte order and the game are
//! detected from the file itself: Skyrim scripts are big-endian, all later
//! games are little-endian and identify themselves through the header game ID.
//!
//! [`PexFile`] is the complete data model. It borrows string table entries and
//! header strings from the source buffer, so parsing a memory-mapped file
//! copies no text; [`PexFile::into_owned`] detaches it from the buffer.
//! [`PexView`] parses only the header, string table, debug information and
//! user flags, and decodes objects on demand.
//!
//! Writing an unmodified file reproduces it byte for byte.
//!
//! ```no_run
//! use bethkit_pex::PexFile;
//!
//! let bytes: Vec<u8> = std::fs::read("Scripts/Actor.pex")?;
//! let pex = PexFile::parse(&bytes)?;
//! assert_eq!(pex.write()?, bytes);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

mod error;
mod game;
pub mod model;
mod opcode;
mod read;
mod view;
mod write;

pub use error::{PexError, Result};
pub use game::{FormatProfile, PexGame};
pub use model::PexFile;
pub use opcode::{Opcode, OpcodeInfo};
pub use view::PexView;
