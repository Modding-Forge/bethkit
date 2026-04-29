// SPDX-License-Identifier: Apache-2.0
//!
//! Oblivion item record schemas.
//!
//! Covers WEAP, ARMO, CLOT, AMMO, BOOK, ALCH, INGR, MISC, KEYM, SLGM,
//! APPA, CONT, DOOR, FURN, LIGH.

use crate::schema::{FieldDef, FieldType, RecordSchema, SubRecordDef};
use crate::types::Signature;

use super::common::{
    DESC_DEF, EDID_DEF, FULL_DEF, ICON_DEF, MODL_DEF, QNAM_DEF, SCRI_DEF, SNAM_DEF,
};


static WEAP_DATA_FIELDS: [FieldDef; 7] = [
    FieldDef { name: "Type", kind: FieldType::UInt16 },
    FieldDef { name: "Speed", kind: FieldType::Float32 },
    FieldDef { name: "Reach", kind: FieldType::Float32 },
    FieldDef { name: "Flags", kind: FieldType::UInt32 },
    FieldDef { name: "Value", kind: FieldType::UInt32 },
    FieldDef { name: "Health", kind: FieldType::UInt32 },
    FieldDef { name: "Weight", kind: FieldType::Float32 },
];

static WEAP_MEMBERS: [SubRecordDef; 6] = [
    EDID_DEF,
    FULL_DEF,
    MODL_DEF,
    ICON_DEF,
    SCRI_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&WEAP_DATA_FIELDS),
    },
];

/// WEAP — weapon.
pub static WEAP_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"WEAP"), name: "Weapon", members: &WEAP_MEMBERS };


static ARMO_DATA_FIELDS: [FieldDef; 4] = [
    FieldDef { name: "Armor Rating", kind: FieldType::UInt16 },
    FieldDef { name: "Value", kind: FieldType::UInt32 },
    FieldDef { name: "Health", kind: FieldType::UInt32 },
    FieldDef { name: "Weight", kind: FieldType::Float32 },
];

static ARMO_MEMBERS: [SubRecordDef; 7] = [
    EDID_DEF,
    FULL_DEF,
    SCRI_DEF,
    SubRecordDef {
        sig: Signature(*b"BMDT"),
        name: "Biped Flags",
        required: true,
        repeating: false,
        field: FieldType::ByteArray,
    },
    MODL_DEF,
    ICON_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&ARMO_DATA_FIELDS),
    },
];

/// ARMO — armor.
pub static ARMO_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"ARMO"), name: "Armor", members: &ARMO_MEMBERS };


static CLOT_DATA_FIELDS: [FieldDef; 2] = [
    FieldDef { name: "Value", kind: FieldType::UInt32 },
    FieldDef { name: "Weight", kind: FieldType::Float32 },
];

static CLOT_MEMBERS: [SubRecordDef; 6] = [
    EDID_DEF,
    FULL_DEF,
    SCRI_DEF,
    SubRecordDef {
        sig: Signature(*b"BMDT"),
        name: "Biped Flags",
        required: true,
        repeating: false,
        field: FieldType::ByteArray,
    },
    MODL_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&CLOT_DATA_FIELDS),
    },
];

/// CLOT — clothing (Oblivion-specific; separate from ARMO; merged into ARMO in
/// Fallout 3 and later games).
pub static CLOT_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"CLOT"), name: "Clothing", members: &CLOT_MEMBERS };


static AMMO_DATA_FIELDS: [FieldDef; 6] = [
    FieldDef { name: "Speed", kind: FieldType::Float32 },
    FieldDef { name: "Ignores Normal Weapon Resistance", kind: FieldType::UInt8 },
    FieldDef { name: "_padding", kind: FieldType::ByteArray },
    FieldDef { name: "Value", kind: FieldType::UInt32 },
    FieldDef { name: "Weight", kind: FieldType::Float32 },
    FieldDef { name: "Damage", kind: FieldType::UInt16 },
];

static AMMO_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    FULL_DEF,
    MODL_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&AMMO_DATA_FIELDS),
    },
];

/// AMMO — ammunition.
pub static AMMO_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"AMMO"), name: "Ammunition", members: &AMMO_MEMBERS };


static BOOK_DATA_FIELDS: [FieldDef; 4] = [
    FieldDef { name: "Flags", kind: FieldType::UInt8 },
    FieldDef { name: "Teaches Skill", kind: FieldType::Int8 },
    FieldDef { name: "Value", kind: FieldType::UInt32 },
    FieldDef { name: "Weight", kind: FieldType::Float32 },
];

static BOOK_MEMBERS: [SubRecordDef; 6] = [
    EDID_DEF,
    FULL_DEF,
    MODL_DEF,
    ICON_DEF,
    DESC_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&BOOK_DATA_FIELDS),
    },
];

/// BOOK — book (includes scrolls when the scroll flag is set).
pub static BOOK_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"BOOK"), name: "Book", members: &BOOK_MEMBERS };


static ALCH_MEMBERS: [SubRecordDef; 6] = [
    EDID_DEF,
    FULL_DEF,
    MODL_DEF,
    ICON_DEF,
    SCRI_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Weight",
        required: true,
        repeating: false,
        field: FieldType::Float32,
    },
];

/// ALCH — potion / alchemy item.
pub static ALCH_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"ALCH"), name: "Potion", members: &ALCH_MEMBERS };


static INGR_DATA_FIELDS: [FieldDef; 2] = [
    FieldDef { name: "Value", kind: FieldType::UInt32 },
    FieldDef { name: "Weight", kind: FieldType::Float32 },
];

static INGR_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    FULL_DEF,
    MODL_DEF,
    ICON_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&INGR_DATA_FIELDS),
    },
];

/// INGR — ingredient.
pub static INGR_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"INGR"), name: "Ingredient", members: &INGR_MEMBERS };


static MISC_DATA_FIELDS: [FieldDef; 2] = [
    FieldDef { name: "Value", kind: FieldType::UInt32 },
    FieldDef { name: "Weight", kind: FieldType::Float32 },
];

static MISC_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    FULL_DEF,
    MODL_DEF,
    ICON_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&MISC_DATA_FIELDS),
    },
];

/// MISC — miscellaneous item.
pub static MISC_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"MISC"), name: "Misc. Item", members: &MISC_MEMBERS };


static KEYM_DATA_FIELDS: [FieldDef; 2] = [
    FieldDef { name: "Value", kind: FieldType::UInt32 },
    FieldDef { name: "Weight", kind: FieldType::Float32 },
];

static KEYM_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    FULL_DEF,
    MODL_DEF,
    ICON_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::Struct(&KEYM_DATA_FIELDS),
    },
];

/// KEYM — key.
pub static KEYM_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"KEYM"), name: "Key", members: &KEYM_MEMBERS };


static SLGM_DATA_FIELDS: [FieldDef; 3] = [
    FieldDef { name: "Value", kind: FieldType::UInt32 },
    FieldDef { name: "Weight", kind: FieldType::Float32 },
    FieldDef { name: "Soul Level", kind: FieldType::UInt8 },
];

static SLGM_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    FULL_DEF,
    MODL_DEF,
    ICON_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&SLGM_DATA_FIELDS),
    },
];

/// SLGM — soul gem.
pub static SLGM_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"SLGM"), name: "Soul Gem", members: &SLGM_MEMBERS };


static APPA_DATA_FIELDS: [FieldDef; 4] = [
    FieldDef { name: "Type", kind: FieldType::UInt8 },
    FieldDef { name: "Value", kind: FieldType::UInt32 },
    FieldDef { name: "Weight", kind: FieldType::Float32 },
    FieldDef { name: "Quality", kind: FieldType::Float32 },
];

static APPA_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    FULL_DEF,
    MODL_DEF,
    ICON_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&APPA_DATA_FIELDS),
    },
];

/// APPA — apparatus (alchemy equipment).
pub static APPA_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"APPA"), name: "Apparatus", members: &APPA_MEMBERS };


static CONT_MEMBERS: [SubRecordDef; 6] = [
    EDID_DEF,
    FULL_DEF,
    MODL_DEF,
    SCRI_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Flags",
        required: true,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SNAM_DEF,
];

/// CONT — container.
pub static CONT_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"CONT"), name: "Container", members: &CONT_MEMBERS };


static DOOR_MEMBERS: [SubRecordDef; 6] = [
    EDID_DEF,
    FULL_DEF,
    MODL_DEF,
    SCRI_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Flags",
        required: false,
        repeating: false,
        field: FieldType::UInt8,
    },
    SNAM_DEF,
];

/// DOOR — door.
pub static DOOR_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"DOOR"), name: "Door", members: &DOOR_MEMBERS };


static FURN_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    FULL_DEF,
    MODL_DEF,
    SubRecordDef {
        sig: Signature(*b"MNAM"),
        name: "Marker Flags",
        required: true,
        repeating: false,
        field: FieldType::UInt32,
    },
];

/// FURN — furniture.
pub static FURN_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"FURN"), name: "Furniture", members: &FURN_MEMBERS };


static LIGH_DATA_FIELDS: [FieldDef; 7] = [
    FieldDef { name: "Time", kind: FieldType::Int32 },
    FieldDef { name: "Radius", kind: FieldType::UInt32 },
    FieldDef { name: "Color", kind: FieldType::UInt32 },
    FieldDef { name: "Flags", kind: FieldType::UInt32 },
    FieldDef { name: "Falloff Exponent", kind: FieldType::Float32 },
    FieldDef { name: "FOV", kind: FieldType::Float32 },
    FieldDef { name: "Value", kind: FieldType::UInt32 },
];

static LIGH_MEMBERS: [SubRecordDef; 6] = [
    EDID_DEF,
    MODL_DEF,
    ICON_DEF,
    SCRI_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&LIGH_DATA_FIELDS),
    },
];

/// LIGH — light source.
pub static LIGH_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"LIGH"), name: "Light", members: &LIGH_MEMBERS };

// Suppress lints for helpers used in struct positions only.
const _: () = {
    let _ = QNAM_DEF;
    let _: &[FieldDef] = &[];
};
