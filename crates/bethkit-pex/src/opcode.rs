// SPDX-License-Identifier: Apache-2.0
//!
//! The Papyrus instruction set.
//!
//! [`Opcode::info`] is the single description of every instruction: how many
//! fixed arguments it has, whether a counted list of further arguments follows,
//! which argument receives the result, and which game introduced it. Reader,
//! writer, validation, assembly and analysis all use this table.

use crate::game::PexGame;

/// Static description of one opcode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct OpcodeInfo {
    /// Upper-case mnemonic used in Papyrus assembly.
    pub mnemonic: &'static str,
    /// Number of arguments that always follow the opcode.
    pub fixed_args: u8,
    /// Whether an integer count and that many further arguments follow.
    pub variadic: bool,
    /// Index of the fixed argument that receives the result, if any.
    pub dest_index: Option<u8>,
    /// The first game whose format contains the opcode.
    pub min_game: PexGame,
}

/// Defines the opcode enum and its description table from one list.
macro_rules! opcodes {
    ($($variant:ident = $code:literal, $mnemonic:literal, $fixed:literal,
        $variadic:literal, $dest:expr, $game:ident;)*) => {
        /// A Papyrus virtual machine instruction.
        ///
        /// Each variant is described by its mnemonic and argument layout in
        /// [`Opcode::info`].
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        #[repr(u8)]
        pub enum Opcode {
            $($variant = $code,)*
        }

        impl Opcode {
            /// Returns the opcode with the given byte value.
            pub fn from_byte(byte: u8) -> Option<Self> {
                match byte {
                    $($code => Some(Self::$variant),)*
                    _ => None,
                }
            }

            /// Returns the static description of the opcode.
            pub fn info(self) -> OpcodeInfo {
                match self {
                    $(Self::$variant => OpcodeInfo {
                        mnemonic: $mnemonic,
                        fixed_args: $fixed,
                        variadic: $variadic,
                        dest_index: $dest,
                        min_game: PexGame::$game,
                    },)*
                }
            }

            /// Returns all opcodes in byte order.
            pub fn all() -> &'static [Opcode] {
                &[$(Self::$variant,)*]
            }
        }
    };
}

opcodes! {
    Nop = 0x00, "NOP", 0, false, None, Skyrim;
    IAdd = 0x01, "IADD", 3, false, Some(0), Skyrim;
    FAdd = 0x02, "FADD", 3, false, Some(0), Skyrim;
    ISub = 0x03, "ISUBTRACT", 3, false, Some(0), Skyrim;
    FSub = 0x04, "FSUBTRACT", 3, false, Some(0), Skyrim;
    IMul = 0x05, "IMULTIPLY", 3, false, Some(0), Skyrim;
    FMul = 0x06, "FMULTIPLY", 3, false, Some(0), Skyrim;
    IDiv = 0x07, "IDIVIDE", 3, false, Some(0), Skyrim;
    FDiv = 0x08, "FDIVIDE", 3, false, Some(0), Skyrim;
    IMod = 0x09, "IMOD", 3, false, Some(0), Skyrim;
    Not = 0x0A, "NOT", 2, false, Some(0), Skyrim;
    INeg = 0x0B, "INEGATE", 2, false, Some(0), Skyrim;
    FNeg = 0x0C, "FNEGATE", 2, false, Some(0), Skyrim;
    Assign = 0x0D, "ASSIGN", 2, false, Some(0), Skyrim;
    Cast = 0x0E, "CAST", 2, false, Some(0), Skyrim;
    CmpEq = 0x0F, "COMPAREEQ", 3, false, Some(0), Skyrim;
    CmpLt = 0x10, "COMPARELT", 3, false, Some(0), Skyrim;
    CmpLe = 0x11, "COMPARELTE", 3, false, Some(0), Skyrim;
    CmpGt = 0x12, "COMPAREGT", 3, false, Some(0), Skyrim;
    CmpGe = 0x13, "COMPAREGTE", 3, false, Some(0), Skyrim;
    Jmp = 0x14, "JUMP", 1, false, None, Skyrim;
    JmpT = 0x15, "JUMPT", 2, false, None, Skyrim;
    JmpF = 0x16, "JUMPF", 2, false, None, Skyrim;
    CallMethod = 0x17, "CALLMETHOD", 3, true, Some(2), Skyrim;
    CallParent = 0x18, "CALLPARENT", 2, true, Some(1), Skyrim;
    CallStatic = 0x19, "CALLSTATIC", 3, true, Some(2), Skyrim;
    Return = 0x1A, "RETURN", 1, false, None, Skyrim;
    StrCat = 0x1B, "STRCAT", 3, false, Some(0), Skyrim;
    PropGet = 0x1C, "PROPGET", 3, false, Some(2), Skyrim;
    PropSet = 0x1D, "PROPSET", 3, false, None, Skyrim;
    ArrayCreate = 0x1E, "ARRAYCREATE", 2, false, Some(0), Skyrim;
    ArrayLength = 0x1F, "ARRAYLENGTH", 2, false, Some(0), Skyrim;
    ArrayGetElement = 0x20, "ARRAYGETELEMENT", 3, false, Some(0), Skyrim;
    ArraySetElement = 0x21, "ARRAYSETELEMENT", 3, false, None, Skyrim;
    ArrayFindElement = 0x22, "ARRAYFINDELEMENT", 4, false, Some(1), Skyrim;
    ArrayRFindElement = 0x23, "ARRAYRFINDELEMENT", 4, false, Some(1), Skyrim;
    Is = 0x24, "IS", 3, false, Some(0), Fallout4;
    StructCreate = 0x25, "STRUCTCREATE", 1, false, Some(0), Fallout4;
    StructGet = 0x26, "STRUCTGET", 3, false, Some(0), Fallout4;
    StructSet = 0x27, "STRUCTSET", 3, false, None, Fallout4;
    ArrayFindStruct = 0x28, "ARRAYFINDSTRUCT", 5, false, Some(1), Fallout4;
    ArrayRFindStruct = 0x29, "ARRAYRFINDSTRUCT", 5, false, Some(1), Fallout4;
    ArrayAdd = 0x2A, "ARRAYADDELEMENTS", 3, false, None, Fallout4;
    ArrayInsert = 0x2B, "ARRAYINSERTELEMENT", 3, false, None, Fallout4;
    ArrayRemoveLast = 0x2C, "ARRAYREMOVELASTELEMENT", 1, false, None, Fallout4;
    ArrayRemove = 0x2D, "ARRAYREMOVEELEMENTS", 3, false, None, Fallout4;
    ArrayClear = 0x2E, "ARRAYCLEARELEMENTS", 1, false, None, Fallout4;
    ArrayGetAllMatchingStructs = 0x2F, "ARRAYGETALLMATCHINGSTRUCTS", 6, false, Some(1),
        Fallout76;
    LockGuards = 0x30, "LOCKGUARDS", 0, true, None, Starfield;
    UnlockGuards = 0x31, "UNLOCKGUARDS", 0, true, None, Starfield;
    TryLockGuards = 0x32, "TRYLOCKGUARDS", 1, true, Some(0), Starfield;
}

impl Opcode {
    /// Returns `true` if `game`'s format contains the opcode.
    pub fn is_available_in(self, game: PexGame) -> bool {
        self.info().min_game <= game && self as u8 <= game.profile().max_opcode
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the table is dense, consistent and matches the game limits.
    #[test]
    fn opcode_table_is_consistent() -> Result<(), Box<dyn std::error::Error>> {
        // then
        for (index, opcode) in Opcode::all().iter().enumerate() {
            assert_eq!(*opcode as usize, index);
            assert_eq!(Opcode::from_byte(*opcode as u8), Some(*opcode));
            let info: OpcodeInfo = opcode.info();
            if let Some(dest) = info.dest_index {
                assert!(dest < info.fixed_args, "{info:?}");
            }
        }
        assert_eq!(Opcode::from_byte(0x33), None);
        assert!(Opcode::ArrayRFindElement.is_available_in(PexGame::Skyrim));
        assert!(!Opcode::Is.is_available_in(PexGame::Skyrim));
        assert!(Opcode::ArrayGetAllMatchingStructs.is_available_in(PexGame::Starfield));
        assert!(!Opcode::ArrayGetAllMatchingStructs.is_available_in(PexGame::Fallout4));
        assert!(!Opcode::LockGuards.is_available_in(PexGame::Fallout76));
        Ok(())
    }
}
