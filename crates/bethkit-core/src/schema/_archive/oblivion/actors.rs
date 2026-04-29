// SPDX-License-Identifier: Apache-2.0
//!
//! Oblivion actor record schemas.
//!
//! Covers NPC_, CREA, RACE, PACK, FACT, CSTY, IDLE, LVLC, LVLI, LVSP,
//! EYES, HAIR, CLAS.

use crate::schema::{FieldDef, FieldType, RecordSchema, SubRecordDef};
use crate::types::Signature;

use super::common::{
    CTDA_DEF, DESC_DEF, EDID_DEF, FULL_DEF, ICON_DEF, MODL_DEF, SCRI_DEF, SNAM_DEF,
};
use super::enums::{OBLIVION_ATTRIBUTE_ENUM, OBLIVION_SPECIALIZATION_ENUM};


static NPC_ACBS_FIELDS: [FieldDef; 8] = [
    FieldDef { name: "Flags", kind: FieldType::UInt32 },
    FieldDef { name: "Base Spell Points", kind: FieldType::UInt16 },
    FieldDef { name: "Fatigue", kind: FieldType::UInt16 },
    FieldDef { name: "Barter Gold", kind: FieldType::UInt16 },
    FieldDef { name: "Level", kind: FieldType::Int16 },
    FieldDef { name: "Calc Min Level", kind: FieldType::UInt16 },
    FieldDef { name: "Calc Max Level", kind: FieldType::UInt16 },
    FieldDef { name: "Disposition", kind: FieldType::UInt16 },
];

static NPC_MEMBERS: [SubRecordDef; 11] = [
    EDID_DEF,
    FULL_DEF,
    MODL_DEF,
    SCRI_DEF,
    SubRecordDef {
        sig: Signature(*b"ACBS"),
        name: "Configuration",
        required: true,
        repeating: false,
        field: FieldType::Struct(&NPC_ACBS_FIELDS),
    },
    SubRecordDef {
        sig: Signature(*b"SNAM"),
        name: "Faction",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"INAM"),
        name: "Death Item",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"RNAM"),
        name: "Race",
        required: true,
        repeating: false,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"AIDT"),
        name: "AI Data",
        required: true,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"PKID"),
        name: "AI Package",
        required: false,
        repeating: true,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: true,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// NPC_ — non-player character.
pub static NPC_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"NPC_"),
    name: "Non-Player Character",
    members: &NPC_MEMBERS,
};


static CREA_MEMBERS: [SubRecordDef; 9] = [
    EDID_DEF,
    FULL_DEF,
    MODL_DEF,
    SCRI_DEF,
    SubRecordDef {
        sig: Signature(*b"ACBS"),
        name: "Configuration",
        required: true,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"INAM"),
        name: "Death Item",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"AIDT"),
        name: "AI Data",
        required: true,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"PKID"),
        name: "AI Package",
        required: false,
        repeating: true,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Creature Data",
        required: true,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// CREA — creature (Oblivion-specific; distinct from NPC_; replaced by NPC_
/// in Skyrim).
pub static CREA_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"CREA"), name: "Creature", members: &CREA_MEMBERS };


static RACE_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    FULL_DEF,
    DESC_DEF,
    SubRecordDef {
        sig: Signature(*b"SPLO"),
        name: "Racial Spell",
        required: false,
        repeating: true,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Race Data",
        required: true,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// RACE — race.
pub static RACE_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"RACE"), name: "Race", members: &RACE_MEMBERS };


static PACK_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"PKDT"),
        name: "Package Data",
        required: true,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"PLDT"),
        name: "Package Location",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    CTDA_DEF,
];

/// PACK — AI package.
pub static PACK_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"PACK"), name: "AI Package", members: &PACK_MEMBERS };


static FACT_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"XNAM"),
        name: "Faction Relation",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Flags",
        required: false,
        repeating: false,
        field: FieldType::UInt8,
    },
    SubRecordDef {
        sig: Signature(*b"RNAM"),
        name: "Rank Name",
        required: false,
        repeating: true,
        field: FieldType::ZString,
    },
];

/// FACT — faction.
pub static FACT_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"FACT"), name: "Faction", members: &FACT_MEMBERS };


static CSTY_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"CSTD"),
        name: "Standard Combat Style",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"CSAD"),
        name: "Advanced Combat Style",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// CSTY — combat style.
pub static CSTY_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"CSTY"), name: "Combat Style", members: &CSTY_MEMBERS };


static IDLE_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    MODL_DEF,
    CTDA_DEF,
    SubRecordDef {
        sig: Signature(*b"ANAM"),
        name: "Related Idle Animations",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Idle Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// IDLE — idle animation.
pub static IDLE_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"IDLE"), name: "Idle Animation", members: &IDLE_MEMBERS };


static LVLC_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"LVLD"),
        name: "Chance None",
        required: false,
        repeating: false,
        field: FieldType::UInt8,
    },
    SubRecordDef {
        sig: Signature(*b"LVLF"),
        name: "Flags",
        required: false,
        repeating: false,
        field: FieldType::UInt8,
    },
    SubRecordDef {
        sig: Signature(*b"LVLO"),
        name: "Leveled Object",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
];

/// LVLC — leveled creature (Oblivion-specific; replaced by LVLN in Skyrim).
pub static LVLC_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"LVLC"),
    name: "Leveled Creature",
    members: &LVLC_MEMBERS,
};


static LVLI_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"LVLD"),
        name: "Chance None",
        required: false,
        repeating: false,
        field: FieldType::UInt8,
    },
    SubRecordDef {
        sig: Signature(*b"LVLF"),
        name: "Flags",
        required: false,
        repeating: false,
        field: FieldType::UInt8,
    },
    SubRecordDef {
        sig: Signature(*b"LVLO"),
        name: "Leveled Object",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
];

/// LVLI — leveled item.
pub static LVLI_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"LVLI"), name: "Leveled Item", members: &LVLI_MEMBERS };


static LVSP_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"LVLD"),
        name: "Chance None",
        required: false,
        repeating: false,
        field: FieldType::UInt8,
    },
    SubRecordDef {
        sig: Signature(*b"LVLF"),
        name: "Flags",
        required: false,
        repeating: false,
        field: FieldType::UInt8,
    },
    SubRecordDef {
        sig: Signature(*b"LVLO"),
        name: "Leveled Object",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
];

/// LVSP — leveled spell.
pub static LVSP_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"LVSP"), name: "Leveled Spell", members: &LVSP_MEMBERS };


static EYES_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Playable",
        required: false,
        repeating: false,
        field: FieldType::UInt8,
    },
];

/// EYES — eye type.
pub static EYES_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"EYES"), name: "Eyes", members: &EYES_MEMBERS };


static HAIR_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    FULL_DEF,
    MODL_DEF,
    ICON_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Flags",
        required: false,
        repeating: false,
        field: FieldType::UInt8,
    },
];

/// HAIR — hair type (Oblivion-specific; hair is folded into RACE in Skyrim).
pub static HAIR_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"HAIR"), name: "Hair", members: &HAIR_MEMBERS };


static CLAS_DATA_FIELDS: [FieldDef; 6] = [
    FieldDef { name: "Primary Attribute 1", kind: FieldType::UInt32 },
    FieldDef { name: "Primary Attribute 2", kind: FieldType::UInt32 },
    FieldDef {
        name: "Specialization",
        kind: FieldType::Enum(&OBLIVION_SPECIALIZATION_ENUM),
    },
    FieldDef { name: "Major Skills", kind: FieldType::ByteArray },
    FieldDef { name: "Flags", kind: FieldType::UInt32 },
    FieldDef { name: "Buys/Sells Services", kind: FieldType::UInt32 },
];

static CLAS_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    FULL_DEF,
    DESC_DEF,
    ICON_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&CLAS_DATA_FIELDS),
    },
];

/// CLAS — class.
pub static CLAS_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"CLAS"), name: "Class", members: &CLAS_MEMBERS };

// Suppress lints for enum values only used in FieldType positions.
const _: () = {
    let _ = &OBLIVION_ATTRIBUTE_ENUM;
    let _ = SNAM_DEF;
};
