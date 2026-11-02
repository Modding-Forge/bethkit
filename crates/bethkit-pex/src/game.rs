// SPDX-License-Identifier: Apache-2.0
//!
//! Games that use the PEX format and the format differences between them.

use std::fmt;

use bethkit_core::Game;
use bethkit_io::Endianness;

/// The game family a PEX file belongs to.
///
/// Editions of one game (Skyrim LE, SE and VR; Fallout 4 and Fallout 4 VR)
/// share the same PEX format.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PexGame {
    /// Skyrim Legendary Edition, Special Edition and VR.
    Skyrim,
    /// Fallout 4 and Fallout 4 VR.
    Fallout4,
    /// Fallout 76.
    Fallout76,
    /// Starfield.
    Starfield,
}

/// The format features of one [`PexGame`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FormatProfile {
    /// Byte order of every multi-byte value in the file.
    pub byte_order: Endianness,
    /// Game ID stored in the header.
    pub game_id: u16,
    /// Format version written by the reference compiler, as `(major, minor)`.
    pub default_version: (u8, u8),
    /// Objects carry a const flag and structs; variables carry a const flag;
    /// debug information carries property groups and struct orders.
    pub has_fallout4_layout: bool,
    /// Objects carry a guard table.
    pub has_guards: bool,
    /// Highest valid opcode.
    pub max_opcode: u8,
}

impl PexGame {
    /// Returns the format features of the game.
    pub fn profile(self) -> FormatProfile {
        match self {
            Self::Skyrim => FormatProfile {
                byte_order: Endianness::Big,
                game_id: 1,
                default_version: (3, 2),
                has_fallout4_layout: false,
                has_guards: false,
                max_opcode: 0x23,
            },
            Self::Fallout4 => FormatProfile {
                byte_order: Endianness::Little,
                game_id: 2,
                default_version: (3, 9),
                has_fallout4_layout: true,
                has_guards: false,
                max_opcode: 0x2E,
            },
            Self::Fallout76 => FormatProfile {
                byte_order: Endianness::Little,
                game_id: 3,
                default_version: (3, 15),
                has_fallout4_layout: true,
                has_guards: false,
                max_opcode: 0x2F,
            },
            Self::Starfield => FormatProfile {
                byte_order: Endianness::Little,
                game_id: 4,
                default_version: (3, 12),
                has_fallout4_layout: true,
                has_guards: true,
                max_opcode: 0x32,
            },
        }
    }

    /// Returns the game of a little-endian file with the given header game ID.
    ///
    /// Big-endian files are always Skyrim, whatever their game ID says.
    pub fn from_game_id(game_id: u16) -> Option<Self> {
        match game_id {
            2 => Some(Self::Fallout4),
            3 => Some(Self::Fallout76),
            4 => Some(Self::Starfield),
            _ => None,
        }
    }

    /// Returns the PEX format used by a bethkit [`Game`], or `None` for games
    /// without Papyrus.
    pub fn from_game(game: Game) -> Option<Self> {
        match game {
            Game::SkyrimSE | Game::SkyrimLE | Game::SkyrimVR => Some(Self::Skyrim),
            Game::Fallout4 | Game::Fallout4VR => Some(Self::Fallout4),
            Game::Fallout76 => Some(Self::Fallout76),
            Game::Starfield => Some(Self::Starfield),
            Game::Oblivion | Game::Fallout3 | Game::FalloutNV | Game::Morrowind => None,
        }
    }
}

impl fmt::Display for PexGame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Skyrim => f.write_str("Skyrim"),
            Self::Fallout4 => f.write_str("Fallout 4"),
            Self::Fallout76 => f.write_str("Fallout 76"),
            Self::Starfield => f.write_str("Starfield"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies game mapping and the profile of every game.
    #[test]
    fn games_map_to_profiles() -> Result<(), Box<dyn std::error::Error>> {
        // then
        assert_eq!(PexGame::from_game(Game::SkyrimVR), Some(PexGame::Skyrim));
        assert_eq!(
            PexGame::from_game(Game::Fallout4VR),
            Some(PexGame::Fallout4)
        );
        assert_eq!(PexGame::from_game(Game::Oblivion), None);
        for game in [
            PexGame::Skyrim,
            PexGame::Fallout4,
            PexGame::Fallout76,
            PexGame::Starfield,
        ] {
            let profile: FormatProfile = game.profile();
            if game != PexGame::Skyrim {
                assert_eq!(PexGame::from_game_id(profile.game_id), Some(game));
            }
        }
        assert_eq!(PexGame::Skyrim.profile().byte_order, Endianness::Big);
        assert_eq!(PexGame::Fallout76.to_string(), "Fallout 76");
        Ok(())
    }
}
