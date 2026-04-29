// SPDX-License-Identifier: Apache-2.0
//!
//! [`SubRecordDef`] helpers that are identical across every supported game.
//!
//! Only definitions whose signature, name, and [`FieldType`] are the same in
//! every Bethesda game engine era belong here.  Definitions that differ
//! between eras (e.g. `FULL` is [`FieldType::LString`] in SSE / Fallout 4 but
//! [`FieldType::ZString`] in Oblivion / Morrowind) must live in the
//! game-specific `common.rs` file instead.

use crate::schema::{FieldType, SubRecordDef};
use crate::types::Signature;

/// EDID — Editor ID (NUL-terminated ASCII string).
///
/// Present in every game from Morrowind onward; always a
/// [`FieldType::ZString`].
pub static EDID_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"EDID"),
    name: "Editor ID",
    required: false,
    repeating: false,
    field: FieldType::ZString,
};

/// MODL — Primary / static model path (NUL-terminated string).
///
/// Used in every game era as the main NIF model reference.
pub static MODL_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"MODL"),
    name: "Model Filename",
    required: false,
    repeating: false,
    field: FieldType::ZString,
};

/// ICON — Large inventory icon path (NUL-terminated string).
///
/// Present in Morrowind through modern games; always a path to a `.dds`
/// or `.tex` file.
pub static ICON_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"ICON"),
    name: "Large Icon Filename",
    required: false,
    repeating: false,
    field: FieldType::ZString,
};

/// MICO — Small / map icon path (NUL-terminated string).
///
/// Companion to `ICON`; shares the same type and semantics across all
/// games that use it.
pub static MICO_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"MICO"),
    name: "Small Icon Filename",
    required: false,
    repeating: false,
    field: FieldType::ZString,
};

/// MODT — Model texture hashes (raw byte array, Oblivion / FO3 era).
///
/// Stores pre-computed BSA texture path hashes for fast lookup.  Replaced
/// by `MODS` in later engine versions.
// NOTE: Used by the FO3 schema archive modules; not included in the active SSE schema.
#[allow(dead_code)]
pub static MODT_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"MODT"),
    name: "Model Texture Hashes",
    required: false,
    repeating: false,
    field: FieldType::ByteArray,
};

/// MODS — Model alternate textures (raw byte array, Skyrim era and later).
///
/// Encodes a list of named texture set overrides applied to the model.
// NOTE: Used by the FO4 and Starfield schema modules.
#[allow(dead_code)]
pub static MODS_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"MODS"),
    name: "Model Alternate Textures",
    required: false,
    repeating: false,
    field: FieldType::ByteArray,
};
