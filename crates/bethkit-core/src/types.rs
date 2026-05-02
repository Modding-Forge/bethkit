// SPDX-License-Identifier: Apache-2.0
//!
//! Core domain types: signatures, FormIDs, flags, plugin kind, and game
//! context.
//!
//! These types are intentionally game-agnostic at the struct level.
//! Game-specific behaviour (flag bit positions, FormID ranges, etc.) is
//! encapsulated in [`GameContext`].

use std::fmt;

/// A 4-byte ASCII record or subrecord signature (e.g. `b"NPC_"`, `b"WEAP"`).
///
/// Stored as a fixed `[u8; 4]` array to avoid heap allocation and to allow
/// use as hash-map keys and `match` arms.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Signature(pub [u8; 4]);

impl Signature {
    /// Plugin header record (`b"TES4"`).
    pub const TES4: Self = Self(*b"TES4");
    /// Morrowind plugin header record (`b"TES3"`).
    pub const TES3: Self = Self(*b"TES3");
    /// Group record (`b"GRUP"`).
    pub const GRUP: Self = Self(*b"GRUP");
    /// Editor ID subrecord (`b"EDID"`).
    pub const EDID: Self = Self(*b"EDID");
    /// Master file subrecord (`b"MAST"`).
    pub const MAST: Self = Self(*b"MAST");
    /// Plugin header data subrecord (`b"HEDR"`).
    pub const HEDR: Self = Self(*b"HEDR");
    /// Large-field size override subrecord (`b"XXXX"`).
    pub const XXXX: Self = Self(*b"XXXX");
    /// Description subrecord (`b"SNAM"`).
    pub const SNAM: Self = Self(*b"SNAM");
    /// Data subrecord (`b"DATA"`).
    pub const DATA: Self = Self(*b"DATA");
}

impl fmt::Display for Signature {
    /// Formats the signature as a 4-character ASCII string, using hex escapes
    /// for non-printable bytes.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for &b in &self.0 {
            if b.is_ascii_graphic() || b == b' ' {
                write!(f, "{}", b as char)?;
            } else {
                write!(f, "\\x{:02X}", b)?;
            }
        }
        Ok(())
    }
}

impl fmt::Debug for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Signature({self})")
    }
}

/// A raw FormID as stored in the plugin file.
///
/// The top byte is the load-order index of the owning plugin. The lower 3
/// bytes are the object ID within that plugin. For light plugins (ESL), the
/// FormID encoding changes — see [`FormId::esl_slot`] and
/// [`FormId::esl_object`].
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct FormId(pub u32);

impl FormId {
    /// The null FormID (0x00000000), meaning "no reference".
    pub const NULL: Self = Self(0);

    /// Returns the load-order file index (top byte of the raw FormID).
    pub fn file_index(self) -> u8 {
        (self.0 >> 24) as u8
    }

    /// Returns the object ID within the owning plugin (lower 3 bytes).
    pub fn object_id(self) -> u32 {
        self.0 & 0x00FF_FFFF
    }

    /// Returns `true` if this is the null FormID (0x00000000).
    pub fn is_null(self) -> bool {
        self.0 == 0
    }

    /// Returns the ESL slot index (bits 23:12) for light plugin FormIDs.
    ///
    /// Only meaningful when `file_index() == 0xFE`.
    pub fn esl_slot(self) -> u16 {
        ((self.0 >> 12) & 0xFFF) as u16
    }

    /// Returns the object ID within the ESL slot (bits 11:0) for light plugin
    /// FormIDs.
    ///
    /// Only meaningful when `file_index() == 0xFE`.
    pub fn esl_object(self) -> u16 {
        (self.0 & 0xFFF) as u16
    }
}

impl fmt::Display for FormId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#010X}", self.0)
    }
}

impl fmt::Debug for FormId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FormId({:#010X})", self.0)
    }
}

bitflags::bitflags! {
    /// Flags stored in the record header.
    ///
    /// Some bits are only valid on the `TES4` header record (plugin-type
    /// flags). Others apply to any main record.
    ///
    /// # Note
    ///
    /// The LIGHT flag bit position is game-dependent. Use
    /// [`GameContext::light_flag`] rather than [`RecordFlags::LIGHT`] when
    /// checking for ESL status.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct RecordFlags: u32 {
        /// Plugin is a master file (`.esm`). Only valid on `TES4`.
        const ESM              = 0x0000_0001;
        /// Plugin has externalized strings (`.strings` files). Only valid on
        /// `TES4`.
        const LOCALIZED        = 0x0000_0080;
        /// Light plugin flag for SSE / FO4 / FO76 (bit 9, `0x0000_0200`).
        ///
        /// For Starfield this bit is instead used as the Update flag; the
        /// light flag moved to bit 8. Always use [`GameContext::light_flag`]
        /// when checking for ESL status across games.
        const LIGHT            = 0x0000_0200;
        /// Record has been deleted.
        const DELETED          = 0x0000_0020;
        /// Record has a LOD tree.
        const HAS_LOD_TREE     = 0x0000_0040;
        /// Record is persistent (never unloaded).
        const PERSISTENT       = 0x0000_0400;
        /// Record starts disabled.
        const INIT_DISABLED    = 0x0000_0800;
        /// Record is ignored.
        const IGNORED          = 0x0000_1000;
        /// Record is a partial form (Starfield).
        const PARTIAL_FORM     = 0x0000_4000;
        /// Record is visible when distant.
        const VISIBLE_DIST     = 0x0000_8000;
        /// Record is dangerous.
        const DANGEROUS        = 0x0002_0000;
        /// Record data is zlib-compressed.
        const COMPRESSED       = 0x0004_0000;
        /// Can't wait near this record.
        const CANT_WAIT        = 0x0008_0000;
    }
}

/// The functional type of a plugin, derived from header flags and file
/// extension.
///
/// Flag checks always take precedence over file extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginKind {
    /// No special flags — a regular `.esp`.
    Plugin,
    /// ESM flag set, or `.esm` extension.
    Master,
    /// LIGHT flag set, or `.esl` extension (SSE, FO4).
    Light,
    /// MEDIUM flag set (Starfield only).
    Medium,
    /// UPDATE flag set (Starfield only).
    Update,
}

impl fmt::Display for PluginKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s: &str = match self {
            Self::Plugin => "Plugin",
            Self::Master => "Master",
            Self::Light => "Light",
            Self::Medium => "Medium",
            Self::Update => "Update",
        };
        f.write_str(s)
    }
}

/// A supported Bethesda game.
///
/// Used by [`GameContext`] to drive all format differences (flag positions,
/// FormID ranges, header signatures, etc.).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Game {
    /// The Elder Scrolls V: Skyrim Special Edition (primary Phase 1 target).
    SkyrimSE,
    /// The Elder Scrolls V: Skyrim Legendary Edition (Classic).
    SkyrimLE,
    /// The Elder Scrolls V: Skyrim VR.
    SkyrimVR,
    /// Fallout 4.
    Fallout4,
    /// Fallout 4 VR.
    Fallout4VR,
    /// Fallout 76.
    Fallout76,
    /// Starfield.
    Starfield,
    /// The Elder Scrolls IV: Oblivion.
    Oblivion,
    /// Fallout 3.
    Fallout3,
    /// Fallout: New Vegas.
    FalloutNV,
    /// The Elder Scrolls III: Morrowind.
    Morrowind,
}

/// Runtime configuration that drives all game-specific format differences.
///
/// Pass a [`GameContext`] through every parse and write operation so that
/// flag bit positions, FormID ranges, and header signatures are always
/// resolved correctly for the target game.
#[derive(Debug, Clone, Copy)]
pub struct GameContext {
    /// The target game.
    pub game: Game,
}

impl GameContext {
    /// Creates a context targeting Skyrim Special Edition (Phase 1 primary).
    pub fn sse() -> Self {
        Self {
            game: Game::SkyrimSE,
        }
    }

    /// Creates a context targeting Skyrim Legendary Edition (Classic).
    pub fn skyrim_le() -> Self {
        Self {
            game: Game::SkyrimLE,
        }
    }

    /// Creates a context targeting Fallout 4.
    pub fn fallout4() -> Self {
        Self {
            game: Game::Fallout4,
        }
    }

    /// Creates a context targeting Starfield.
    pub fn starfield() -> Self {
        Self {
            game: Game::Starfield,
        }
    }

    /// Creates a context targeting The Elder Scrolls IV: Oblivion.
    pub fn oblivion() -> Self {
        Self {
            game: Game::Oblivion,
        }
    }

    /// Creates a context targeting The Elder Scrolls III: Morrowind.
    pub fn morrowind() -> Self {
        Self {
            game: Game::Morrowind,
        }
    }

    /// Returns the raw bit mask for the "light plugin" flag in record flags.
    ///
    /// SSE / FO4 / FO76 use bit 9 (`0x0000_0200`). Starfield moved the flag
    /// to bit 8 (`0x0000_0100`).
    pub fn light_flag(&self) -> u32 {
        match self.game {
            Game::Starfield => 0x0000_0100,
            _ => 0x0000_0200,
        }
    }

    /// Returns the raw bit mask for the "medium plugin" flag (Starfield only).
    pub fn medium_flag(&self) -> u32 {
        0x0000_0400
    }

    /// Returns the raw bit mask for the "update plugin" flag.
    pub fn update_flag(&self) -> u32 {
        match self.game {
            Game::Starfield => 0x0000_0200,
            _ => 0x0010_0000,
        }
    }

    /// Returns `true` if the game supports light plugins (ESL).
    pub fn supports_light(&self) -> bool {
        matches!(
            self.game,
            Game::SkyrimSE
                | Game::SkyrimVR
                | Game::Fallout4
                | Game::Fallout4VR
                | Game::Fallout76
                | Game::Starfield
        )
    }

    /// Returns `true` if the game supports string-table localisation.
    ///
    /// Skyrim (all editions), Fallout 4 (all editions), Fallout 76, and
    /// Starfield all use `.STRINGS` / `.DLSTRINGS` / `.ILSTRINGS` sidecar
    /// files. Earlier games (Morrowind, Oblivion, Fallout 3, Fallout NV) do
    /// not.
    pub fn supports_localization(&self) -> bool {
        matches!(
            self.game,
            Game::SkyrimLE
                | Game::SkyrimSE
                | Game::SkyrimVR
                | Game::Fallout4
                | Game::Fallout4VR
                | Game::Fallout76
                | Game::Starfield
        )
    }

    /// Returns `true` if the game supports medium plugins (Starfield only).
    pub fn supports_medium(&self) -> bool {
        matches!(self.game, Game::Starfield)
    }

    /// Returns `true` if the game supports update plugins.
    pub fn supports_update(&self) -> bool {
        matches!(self.game, Game::Starfield | Game::SkyrimVR)
    }

    /// Returns the maximum object ID for a new record in a light plugin.
    pub fn light_max_object_id(&self) -> u32 {
        0xFFF
    }

    /// Returns the maximum object ID for a new record in a medium plugin.
    pub fn medium_max_object_id(&self) -> u32 {
        0xFFFF
    }

    /// Returns the expected plugin header signature for this game.
    ///
    /// Morrowind uses `b"TES3"`; all later games use `b"TES4"`.
    pub fn header_signature(&self) -> Signature {
        match self.game {
            Game::Morrowind => Signature(*b"TES3"),
            _ => Signature(*b"TES4"),
        }
    }

    /// Returns the expected HEDR version float for this game.
    ///
    /// This is informational only — the parser does not enforce it.
    pub fn hedr_version(&self) -> f32 {
        match self.game {
            Game::Morrowind => 1.30,
            Game::Oblivion => 1.0,
            Game::Fallout3 => 0.94,
            Game::FalloutNV => 0.94,
            Game::SkyrimLE => 0.94,
            Game::SkyrimSE => 1.7,
            Game::SkyrimVR => 1.7,
            Game::Fallout4 => 0.95,
            Game::Fallout4VR => 0.95,
            Game::Fallout76 => 68.0,
            Game::Starfield => 0.96,
        }
    }

    /// Derives the [`PluginKind`] from the raw record flags stored in a `TES4`
    /// header record.
    ///
    /// Flag bits take precedence over file extension. Check order:
    /// medium → update → light → master → plugin.
    ///
    /// # Arguments
    ///
    /// * `flags` - The [`RecordFlags`] value from the `TES4` record header.
    pub fn plugin_kind_from_flags(&self, flags: RecordFlags) -> PluginKind {
        let raw: u32 = flags.bits();
        if self.supports_medium() && (raw & self.medium_flag()) != 0 {
            return PluginKind::Medium;
        }
        if self.supports_update() && (raw & self.update_flag()) != 0 {
            return PluginKind::Update;
        }
        if self.supports_light() && (raw & self.light_flag()) != 0 {
            return PluginKind::Light;
        }
        if (raw & RecordFlags::ESM.bits()) != 0 {
            return PluginKind::Master;
        }
        PluginKind::Plugin
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that Signature::Display formats printable bytes as characters.
    #[test]
    fn signature_display_printable() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let sig = Signature(*b"NPC_");

        // when
        let display = sig.to_string();

        // then
        assert_eq!(display, "NPC_");
        Ok(())
    }

    /// Verifies that FormId correctly splits file_index and object_id.
    #[test]
    fn form_id_split() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let fid = FormId(0x01_00_AB_CD);

        // when
        let file_index = fid.file_index();
        let object_id = fid.object_id();

        // then
        assert_eq!(file_index, 0x01);
        assert_eq!(object_id, 0x0000_ABCD);
        Ok(())
    }

    /// Verifies that ESL-encoded FormIDs decode slot and object correctly.
    #[test]
    fn form_id_esl_encoding() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let fid = FormId(0xFE00_3A00);

        // when
        let slot = fid.esl_slot();
        let object = fid.esl_object();

        // then
        assert_eq!(slot, 0x003);
        assert_eq!(object, 0xA00);
        Ok(())
    }

    /// Verifies that the SSE light flag is at bit 9 (0x0000_0200).
    #[test]
    fn sse_light_flag_position() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let ctx = GameContext::sse();

        // when
        let flag = ctx.light_flag();

        // then
        assert_eq!(flag, 0x0000_0200);
        Ok(())
    }

    /// Verifies that the Starfield light flag is at bit 8.
    #[test]
    fn starfield_light_flag_position() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let ctx = GameContext {
            game: Game::Starfield,
        };

        // when
        let flag = ctx.light_flag();

        // then
        assert_eq!(flag, 0x0000_0100);
        Ok(())
    }

    /// Verifies plugin_kind_from_flags returns Light when the light flag is
    /// set.
    #[test]
    fn plugin_kind_light_flag() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let ctx = GameContext::sse();
        let flags = RecordFlags::LIGHT;

        // when
        let kind = ctx.plugin_kind_from_flags(flags);

        // then
        assert_eq!(kind, PluginKind::Light);
        Ok(())
    }

    /// Verifies plugin_kind_from_flags returns Master when only ESM is set.
    #[test]
    fn plugin_kind_master_flag() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let ctx = GameContext::sse();
        let flags = RecordFlags::ESM;

        // when
        let kind = ctx.plugin_kind_from_flags(flags);

        // then
        assert_eq!(kind, PluginKind::Master);
        Ok(())
    }
}
