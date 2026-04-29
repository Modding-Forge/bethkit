// SPDX-License-Identifier: Apache-2.0
//! Morrowind item record schemas.
//!
//! Covers weapons, armour, clothing, books, alchemy, ingredients, misc items,
//! apparatus, lockpicks, probes, repair items, lights, and leveled lists.

use crate::schema::{FieldDef, FieldType, RecordSchema, SubRecordDef};
use crate::types::Signature;

use super::common::{
    DELE_DEF, ENAM_EFFECT_DEF, FNAM_DEF, ITEX_DEF, MODL_DEF, NAME_DEF, SCRI_DEF,
};


static WEAP_WPDT_FIELDS: [FieldDef; 14] = [
    FieldDef { name: "Weight",            kind: FieldType::Float32 },
    FieldDef { name: "Value",             kind: FieldType::UInt32 },
    FieldDef { name: "Type",              kind: FieldType::UInt16 },
    FieldDef { name: "Health",            kind: FieldType::UInt16 },
    FieldDef { name: "Speed",             kind: FieldType::Float32 },
    FieldDef { name: "Reach",             kind: FieldType::Float32 },
    FieldDef { name: "Enchanting Charge", kind: FieldType::UInt16 },
    FieldDef { name: "Chop Min",          kind: FieldType::UInt8 },
    FieldDef { name: "Chop Max",          kind: FieldType::UInt8 },
    FieldDef { name: "Slash Min",         kind: FieldType::UInt8 },
    FieldDef { name: "Slash Max",         kind: FieldType::UInt8 },
    FieldDef { name: "Thrust Min",        kind: FieldType::UInt8 },
    FieldDef { name: "Thrust Max",        kind: FieldType::UInt8 },
    FieldDef { name: "Flags",             kind: FieldType::UInt32 },
];

static WEAP_MEMBERS: [SubRecordDef; 8] = [
    NAME_DEF,
    DELE_DEF,
    MODL_DEF,
    FNAM_DEF,
    SubRecordDef {
        sig: Signature(*b"WPDT"),
        name: "Weapon Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&WEAP_WPDT_FIELDS),
    },
    SCRI_DEF,
    ITEX_DEF,
    SubRecordDef {
        sig: Signature(*b"ENAM"),
        name: "Enchantment",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
];

/// Schema for the `WEAP` weapon record.
pub(super) static WEAP_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"WEAP"),
    name: "Weapon",
    members: &WEAP_MEMBERS,
};


static ARMO_AODT_FIELDS: [FieldDef; 6] = [
    FieldDef { name: "Type",              kind: FieldType::UInt32 },
    FieldDef { name: "Weight",            kind: FieldType::Float32 },
    FieldDef { name: "Value",             kind: FieldType::UInt32 },
    FieldDef { name: "Health",            kind: FieldType::UInt32 },
    FieldDef { name: "Enchanting Charge", kind: FieldType::UInt32 },
    FieldDef { name: "Armor Rating",      kind: FieldType::UInt32 },
];

static ARMO_MEMBERS: [SubRecordDef; 11] = [
    NAME_DEF,
    DELE_DEF,
    MODL_DEF,
    FNAM_DEF,
    SCRI_DEF,
    SubRecordDef {
        sig: Signature(*b"AODT"),
        name: "Armour Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&ARMO_AODT_FIELDS),
    },
    ITEX_DEF,
    SubRecordDef {
        sig: Signature(*b"INDX"),
        name: "Biped Object Slot",
        required: false,
        repeating: true,
        field: FieldType::UInt8,
    },
    SubRecordDef {
        sig: Signature(*b"BNAM"),
        name: "Male Body Part",
        required: false,
        repeating: true,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"CNAM"),
        name: "Female Body Part",
        required: false,
        repeating: true,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"ENAM"),
        name: "Enchantment",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
];

/// Schema for the `ARMO` armour record.
pub(super) static ARMO_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"ARMO"),
    name: "Armour",
    members: &ARMO_MEMBERS,
};


static CLOT_CTDT_FIELDS: [FieldDef; 4] = [
    FieldDef { name: "Type",              kind: FieldType::UInt32 },
    FieldDef { name: "Weight",            kind: FieldType::Float32 },
    FieldDef { name: "Value",             kind: FieldType::UInt16 },
    FieldDef { name: "Enchantment Charge", kind: FieldType::UInt16 },
];

static CLOT_MEMBERS: [SubRecordDef; 11] = [
    NAME_DEF,
    DELE_DEF,
    MODL_DEF,
    FNAM_DEF,
    SubRecordDef {
        sig: Signature(*b"CTDT"),
        name: "Clothing Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&CLOT_CTDT_FIELDS),
    },
    SCRI_DEF,
    ITEX_DEF,
    SubRecordDef {
        sig: Signature(*b"INDX"),
        name: "Biped Object Slot",
        required: false,
        repeating: true,
        field: FieldType::UInt8,
    },
    SubRecordDef {
        sig: Signature(*b"BNAM"),
        name: "Male Body Part",
        required: false,
        repeating: true,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"CNAM"),
        name: "Female Body Part",
        required: false,
        repeating: true,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"ENAM"),
        name: "Enchantment",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
];

/// Schema for the `CLOT` clothing record.
pub(super) static CLOT_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"CLOT"),
    name: "Clothing",
    members: &CLOT_MEMBERS,
};


static BOOK_BKDT_FIELDS: [FieldDef; 5] = [
    FieldDef { name: "Weight",             kind: FieldType::Float32 },
    FieldDef { name: "Value",              kind: FieldType::UInt32 },
    FieldDef { name: "Is Scroll",          kind: FieldType::UInt32 },
    FieldDef { name: "Teaches Skill",      kind: FieldType::Int32 },
    FieldDef { name: "Enchantment Charge", kind: FieldType::UInt32 },
];

static BOOK_MEMBERS: [SubRecordDef; 9] = [
    NAME_DEF,
    DELE_DEF,
    MODL_DEF,
    FNAM_DEF,
    SubRecordDef {
        sig: Signature(*b"BKDT"),
        name: "Book Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&BOOK_BKDT_FIELDS),
    },
    SCRI_DEF,
    ITEX_DEF,
    SubRecordDef {
        sig: Signature(*b"TEXT"),
        name: "Book Text",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"ENAM"),
        name: "Enchantment",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
];

/// Schema for the `BOOK` book or scroll record.
pub(super) static BOOK_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"BOOK"),
    name: "Book",
    members: &BOOK_MEMBERS,
};


static ALCH_ALDT_FIELDS: [FieldDef; 3] = [
    FieldDef { name: "Weight",          kind: FieldType::Float32 },
    FieldDef { name: "Value",           kind: FieldType::UInt32 },
    FieldDef { name: "Auto Calculate",  kind: FieldType::UInt32 },
];

static ALCH_MEMBERS: [SubRecordDef; 8] = [
    NAME_DEF,
    DELE_DEF,
    MODL_DEF,
    SubRecordDef {
        sig: Signature(*b"TEXT"),
        name: "Icon Filename",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SCRI_DEF,
    FNAM_DEF,
    SubRecordDef {
        sig: Signature(*b"ALDT"),
        name: "Potion Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&ALCH_ALDT_FIELDS),
    },
    ENAM_EFFECT_DEF,
];

/// Schema for the `ALCH` alchemy / potion record.
pub(super) static ALCH_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"ALCH"),
    name: "Potion",
    members: &ALCH_MEMBERS,
};


static INGR_MEMBERS: [SubRecordDef; 7] = [
    NAME_DEF,
    DELE_DEF,
    MODL_DEF,
    FNAM_DEF,
    SubRecordDef {
        sig: Signature(*b"IRDT"),
        name: "Ingredient Data",
        required: true,
        repeating: false,
        // 56 bytes: weight(f32), value(u32), effects[{magic_effect,skill,attr};4]
        field: FieldType::ByteArray,
    },
    SCRI_DEF,
    ITEX_DEF,
];

/// Schema for the `INGR` ingredient record.
pub(super) static INGR_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"INGR"),
    name: "Ingredient",
    members: &INGR_MEMBERS,
};


static MISC_MCDT_FIELDS: [FieldDef; 3] = [
    FieldDef { name: "Weight", kind: FieldType::Float32 },
    FieldDef { name: "Value",  kind: FieldType::UInt32 },
    FieldDef { name: "Is Key", kind: FieldType::UInt32 },
];

static MISC_MEMBERS: [SubRecordDef; 7] = [
    NAME_DEF,
    DELE_DEF,
    MODL_DEF,
    FNAM_DEF,
    SubRecordDef {
        sig: Signature(*b"MCDT"),
        name: "Misc Item Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&MISC_MCDT_FIELDS),
    },
    SCRI_DEF,
    ITEX_DEF,
];

/// Schema for the `MISC` miscellaneous item record.
pub(super) static MISC_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"MISC"),
    name: "Misc Item",
    members: &MISC_MEMBERS,
};


static APPA_AADT_FIELDS: [FieldDef; 4] = [
    FieldDef { name: "Type",    kind: FieldType::UInt32 },
    FieldDef { name: "Quality", kind: FieldType::Float32 },
    FieldDef { name: "Weight",  kind: FieldType::Float32 },
    FieldDef { name: "Value",   kind: FieldType::UInt32 },
];

static APPA_MEMBERS: [SubRecordDef; 7] = [
    NAME_DEF,
    DELE_DEF,
    MODL_DEF,
    FNAM_DEF,
    SCRI_DEF,
    SubRecordDef {
        sig: Signature(*b"AADT"),
        name: "Apparatus Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&APPA_AADT_FIELDS),
    },
    ITEX_DEF,
];

/// Schema for the `APPA` alchemical apparatus record.
pub(super) static APPA_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"APPA"),
    name: "Apparatus",
    members: &APPA_MEMBERS,
};


static TOOL_FIELDS: [FieldDef; 4] = [
    FieldDef { name: "Weight",  kind: FieldType::Float32 },
    FieldDef { name: "Value",   kind: FieldType::UInt32 },
    FieldDef { name: "Quality", kind: FieldType::Float32 },
    FieldDef { name: "Uses",    kind: FieldType::UInt32 },
];

static LOCK_MEMBERS: [SubRecordDef; 7] = [
    NAME_DEF,
    DELE_DEF,
    MODL_DEF,
    FNAM_DEF,
    SubRecordDef {
        sig: Signature(*b"LKDT"),
        name: "Lockpick Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&TOOL_FIELDS),
    },
    SCRI_DEF,
    ITEX_DEF,
];

/// Schema for the `LOCK` lockpick tool record.
pub(super) static LOCK_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"LOCK"),
    name: "Lockpick",
    members: &LOCK_MEMBERS,
};


static PROB_MEMBERS: [SubRecordDef; 7] = [
    NAME_DEF,
    DELE_DEF,
    MODL_DEF,
    FNAM_DEF,
    SubRecordDef {
        sig: Signature(*b"PBDT"),
        name: "Probe Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&TOOL_FIELDS),
    },
    SCRI_DEF,
    ITEX_DEF,
];

/// Schema for the `PROB` probe tool record.
pub(super) static PROB_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"PROB"),
    name: "Probe",
    members: &PROB_MEMBERS,
};


static REPA_MEMBERS: [SubRecordDef; 7] = [
    NAME_DEF,
    DELE_DEF,
    MODL_DEF,
    FNAM_DEF,
    SubRecordDef {
        sig: Signature(*b"RIDT"),
        name: "Repair Item Data",
        required: true,
        repeating: false,
        // Same layout as TOOL_FIELDS (weight, value, uses, quality — order differs).
        field: FieldType::ByteArray,
    },
    SCRI_DEF,
    ITEX_DEF,
];

/// Schema for the `REPA` repair item record.
pub(super) static REPA_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"REPA"),
    name: "Repair Item",
    members: &REPA_MEMBERS,
};


static LIGH_LHDT_FIELDS: [FieldDef; 6] = [
    FieldDef { name: "Weight",     kind: FieldType::Float32 },
    FieldDef { name: "Value",      kind: FieldType::UInt32 },
    FieldDef { name: "Duration",   kind: FieldType::Int32 },
    FieldDef { name: "Radius",     kind: FieldType::UInt32 },
    FieldDef { name: "Color RGBA", kind: FieldType::UInt32 },
    FieldDef { name: "Flags",      kind: FieldType::UInt32 },
];

static LIGH_MEMBERS: [SubRecordDef; 8] = [
    NAME_DEF,
    DELE_DEF,
    MODL_DEF,
    FNAM_DEF,
    ITEX_DEF,
    SubRecordDef {
        sig: Signature(*b"LHDT"),
        name: "Light Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&LIGH_LHDT_FIELDS),
    },
    SCRI_DEF,
    SubRecordDef {
        sig: Signature(*b"SNAM"),
        name: "Looping Sound",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
];

/// Schema for the `LIGH` light source record.
pub(super) static LIGH_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"LIGH"),
    name: "Light",
    members: &LIGH_MEMBERS,
};


static LEVC_MEMBERS: [SubRecordDef; 7] = [
    NAME_DEF,
    DELE_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Leveled Flags",
        required: true,
        repeating: false,
        field: FieldType::UInt32,
    },
    SubRecordDef {
        sig: Signature(*b"NNAM"),
        name: "Chance None",
        required: true,
        repeating: false,
        field: FieldType::UInt8,
    },
    SubRecordDef {
        sig: Signature(*b"INDX"),
        name: "Entry Count",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
    SubRecordDef {
        sig: Signature(*b"CNAM"),
        name: "Creature",
        required: false,
        repeating: true,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"INTV"),
        name: "Required Level",
        required: false,
        repeating: true,
        field: FieldType::UInt16,
    },
];

/// Schema for the `LEVC` leveled creature list record.
pub(super) static LEVC_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"LEVC"),
    name: "Leveled Creature",
    members: &LEVC_MEMBERS,
};


static LEVI_MEMBERS: [SubRecordDef; 7] = [
    NAME_DEF,
    DELE_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Leveled Flags",
        required: true,
        repeating: false,
        field: FieldType::UInt32,
    },
    SubRecordDef {
        sig: Signature(*b"NNAM"),
        name: "Chance None",
        required: true,
        repeating: false,
        field: FieldType::UInt8,
    },
    SubRecordDef {
        sig: Signature(*b"INDX"),
        name: "Entry Count",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
    SubRecordDef {
        sig: Signature(*b"INAM"),
        name: "Item",
        required: false,
        repeating: true,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"INTV"),
        name: "Required Level",
        required: false,
        repeating: true,
        field: FieldType::UInt16,
    },
];

/// Schema for the `LEVI` leveled item list record.
pub(super) static LEVI_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"LEVI"),
    name: "Leveled Item",
    members: &LEVI_MEMBERS,
};
