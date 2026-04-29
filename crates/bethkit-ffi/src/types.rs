// SPDX-License-Identifier: Apache-2.0
//!
//! C-representable primitive types shared across all FFI modules.
//!
//! Every `#[repr(C)]` enum and struct defined here is exported verbatim into
//! `bethkit.h` by cbindgen.  Conversion functions that are only used
//! internally are marked `pub(crate)`.

use std::ffi::c_char;

use bethkit_bsa::write::{Ba2Version, BsaVersion};
use bethkit_core::{Game, GameContext, PluginKind, StringFileKind};


/// Bethesda game target for plugin and archive operations.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
// NOTE: variants are constructed by C callers via integer cast, not in Rust.
#[allow(dead_code)]
pub enum BethkitGame {
    /// The Elder Scrolls V: Skyrim Special Edition (and Anniversary Edition).
    SkyrimSe = 0,
    /// Fallout 4.
    Fallout4 = 1,
    /// The Elder Scrolls V: Skyrim (original 2011 release).
    Skyrim = 2,
    /// Fallout 3.
    Fallout3 = 3,
    /// Fallout: New Vegas.
    FalloutNv = 4,
}

/// Plugin file type (determines FormID space and load-order slot allocation).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
// NOTE: variants are constructed by C callers via integer cast, not in Rust.
#[allow(dead_code)]
pub enum BethkitPluginKind {
    /// Full ESP/ESM plugin (up to 0xFFFFFE records, index occupies one master slot).
    Full = 0,
    /// Light ESL plugin (up to 0xFFF records in the 0xFE00–0xFEFF range).
    Light = 1,
    /// Overlay plugin (Starfield+; shares space with the plugin it overrides).
    Overlay = 2,
}


/// Identifies one of the three localized string table file types.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BethkitStringFileKind {
    /// `.strings` — null-terminated inline strings.
    Strings = 0,
    /// `.dlstrings` — length-prefixed strings (dialogue lines).
    DlStrings = 1,
    /// `.ilstrings` — length-prefixed strings (info lines).
    IlStrings = 2,
}


/// BSA archive format version.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
// NOTE: variants are constructed by C callers via integer cast, not in Rust.
#[allow(dead_code)]
pub enum BethkitBsaVersion {
    /// TES III Morrowind BSA (version 0x100).
    Tes3 = 0,
    /// TES IV Oblivion BSA (version 0x67).
    Tes4 = 1,
    /// Fallout 3 / New Vegas BSA (version 0x68).
    Fo3 = 2,
    /// Skyrim / Skyrim SE BSA (version 0x69).
    Sse = 3,
}

/// BA2 archive format version.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
// NOTE: variants are constructed by C callers via integer cast, not in Rust.
#[allow(dead_code)]
pub enum BethkitBa2Version {
    /// Fallout 4 BA2 (version 1).
    V1 = 0,
    /// Fallout 4 Next-Gen BA2 (version 7).
    V7 = 1,
    /// Fallout 4 Next-Gen BA2 (version 8).
    V8 = 2,
}


/// Discriminant for [`BethkitFieldValuePayload`], identifying which union arm
/// is active in a [`BethkitFieldValue`].
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BethkitFieldValueKind {
    /// An integer value (i64); covers UInt8 through UInt64 and Int8 through Int32.
    Int = 0,
    /// A floating-point value (f64); covers Float32.
    Float = 1,
    /// A NUL-terminated inline string borrowed from the record data.
    Str = 2,
    /// An untyped FormID (u32).
    FormId = 3,
    /// A typed FormID with a set of allowed target record signatures.
    FormIdTyped = 4,
    /// Raw bytes borrowed from the record data.
    Bytes = 5,
    /// An enumeration value with an optional resolved name.
    Enum = 6,
    /// A set of named bit flags.
    Flags = 7,
    /// A fixed-layout struct decoded into named sub-fields.
    Struct = 8,
    /// A homogeneous array of field values.
    Array = 9,
    /// A localized string-table ID (only present when the plugin is localized).
    LocalizedId = 10,
    /// The subrecord matching this field definition was absent from the record.
    Missing = 11,
}

/// A typed FormID with its allowed target record-type signatures.
///
/// `allowed_sigs` points to a static array of 4-byte signatures; the slice is
/// `allowed_count` entries long.  The array lives in static memory and must
/// never be freed by the caller.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct BethkitTypedFormId {
    /// The raw file-local FormID value.
    pub raw: u32,
    /// Pointer to the first element of the allowed-signatures array.
    pub allowed_sigs: *const [u8; 4],
    /// Number of entries in `allowed_sigs`.
    pub allowed_count: usize,
}

/// An enumeration field value with its raw integer and optional resolved name.
///
/// `name` is null when the raw value does not correspond to any known variant
/// in the schema.  When non-null it points to a static string and must not
/// be freed.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct BethkitEnumVal {
    /// The raw integer value from the record.
    pub value: i64,
    /// Name of the enum variant, or null if unknown.
    pub name: *const c_char,
}

/// A flags field value with the raw integer and the names of all active bits.
///
/// `active_names` points to an array of `active_count` static C-string
/// pointers.  The *array itself* is heap-allocated and is freed when the
/// enclosing [`BethkitFieldValue`] is released (via the view or entry free
/// functions).  The individual string pointers point into static memory and
/// must not be freed.
#[repr(C)]
pub struct BethkitFlagsVal {
    /// The raw integer value from the record.
    pub raw_value: u64,
    /// Pointer to the first element of the active-names pointer array.
    pub active_names: *const *const c_char,
    /// Number of entries in `active_names`.
    pub active_count: usize,
}


/// Converts a [`BethkitGame`] to a [`GameContext`] usable by bethkit-core.
pub(crate) fn game_to_ctx(game: BethkitGame) -> GameContext {
    match game {
        BethkitGame::SkyrimSe => GameContext { game: Game::SkyrimSE },
        BethkitGame::Fallout4 => GameContext { game: Game::Fallout4 },
        BethkitGame::Skyrim => GameContext { game: Game::SkyrimLE },
        BethkitGame::Fallout3 => GameContext { game: Game::Fallout3 },
        BethkitGame::FalloutNv => GameContext { game: Game::FalloutNV },
    }
}

/// Converts a Rust [`PluginKind`] to the C-ABI equivalent.
pub(crate) fn plugin_kind_from_rust(kind: PluginKind) -> BethkitPluginKind {
    match kind {
        PluginKind::Plugin | PluginKind::Master | PluginKind::Medium | PluginKind::Update =>
            BethkitPluginKind::Full,
        PluginKind::Light => BethkitPluginKind::Light,
    }
}

/// Converts a Rust [`StringFileKind`] to the C-ABI equivalent.
pub(crate) fn string_kind_from_rust(kind: StringFileKind) -> BethkitStringFileKind {
    match kind {
        StringFileKind::Strings => BethkitStringFileKind::Strings,
        StringFileKind::DLStrings => BethkitStringFileKind::DlStrings,
        StringFileKind::ILStrings => BethkitStringFileKind::IlStrings,
    }
}

/// Converts a C-ABI [`BethkitStringFileKind`] to the Rust equivalent.
pub(crate) fn string_kind_to_rust(kind: BethkitStringFileKind) -> StringFileKind {
    match kind {
        BethkitStringFileKind::Strings => StringFileKind::Strings,
        BethkitStringFileKind::DlStrings => StringFileKind::DLStrings,
        BethkitStringFileKind::IlStrings => StringFileKind::ILStrings,
    }
}

/// Converts a C-ABI [`BethkitBsaVersion`] to the Rust equivalent.
pub(crate) fn bsa_version_to_rust(v: BethkitBsaVersion) -> BsaVersion {
    match v {
        BethkitBsaVersion::Tes3 => BsaVersion::Tes3,
        BethkitBsaVersion::Tes4 => BsaVersion::Tes4,
        BethkitBsaVersion::Fo3 => BsaVersion::Fo3,
        BethkitBsaVersion::Sse => BsaVersion::Sse,
    }
}

/// Converts a C-ABI [`BethkitBa2Version`] to the Rust equivalent.
pub(crate) fn ba2_version_to_rust(v: BethkitBa2Version) -> Ba2Version {
    match v {
        BethkitBa2Version::V1 => Ba2Version::V1,
        BethkitBa2Version::V7 => Ba2Version::V7,
        BethkitBa2Version::V8 => Ba2Version::V8,
    }
}
