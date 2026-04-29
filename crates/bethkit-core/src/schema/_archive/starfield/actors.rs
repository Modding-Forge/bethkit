// SPDX-License-Identifier: Apache-2.0
//!
//! Starfield actor record schemas.
//!
//! Covers NPC_, RACE, PACK, FACT, CSTY, IDLE, LVLN, LVLI, LVSP, BPTD,
//! OTFT, EYES, CLAS.

use crate::schema::{FieldDef, FieldType, RecordSchema, SubRecordDef};
use crate::types::Signature;

use super::common::{
    CTDA_DEF, EDID_DEF, EFID_DEF, EFIT_DEF, FULL_DEF, KSIZ_DEF, KWDA_DEF, OBND_DEF, RNAM_DEF,
    SPCT_DEF, SPLO_DEF, VMAD_DEF,
};
use super::enums::SF_NPC_FLAGS;


static NPC_ACBS_FIELDS: [FieldDef; 9] = [
    FieldDef { name: "Flags", kind: FieldType::Flags(&SF_NPC_FLAGS) },
    FieldDef { name: "XP Value Offset", kind: FieldType::Int16 },
    FieldDef { name: "Level", kind: FieldType::Int16 },
    FieldDef { name: "Calc Min Level", kind: FieldType::UInt8 },
    FieldDef { name: "Calc Max Level", kind: FieldType::UInt8 },
    FieldDef { name: "Disposition Base", kind: FieldType::Int16 },
    FieldDef { name: "Template Data Flags", kind: FieldType::UInt16 },
    FieldDef { name: "Bleedout Override", kind: FieldType::UInt16 },
    FieldDef { name: "Geared Up Weapons", kind: FieldType::UInt8 },
];

static NPC_MEMBERS: [SubRecordDef; 16] = [
    EDID_DEF,
    VMAD_DEF,
    OBND_DEF,
    SubRecordDef {
        sig: Signature(*b"ACBS"),
        name: "Configuration",
        required: true,
        repeating: false,
        field: FieldType::Struct(&NPC_ACBS_FIELDS),
    },
    SubRecordDef {
        sig: Signature(*b"SNAM"),
        name: "Factions",
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
        sig: Signature(*b"VTCK"),
        name: "Voice",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"TPLT"),
        name: "Template",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    RNAM_DEF,
    SPCT_DEF,
    SPLO_DEF,
    KSIZ_DEF,
    KWDA_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Marker (empty)",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Skills and Stats",
        required: false,
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


static RACE_MEMBERS: [SubRecordDef; 6] = [
    EDID_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"DESC"),
        name: "Description",
        required: false,
        repeating: false,
        field: FieldType::LString,
    },
    SPCT_DEF,
    SPLO_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// RACE — race definition.
pub static RACE_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"RACE"), name: "Race", members: &RACE_MEMBERS };


static PACK_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    VMAD_DEF,
    SubRecordDef {
        sig: Signature(*b"PKDT"),
        name: "Package Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    CTDA_DEF,
];

/// PACK — AI package.
pub static PACK_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"PACK"), name: "Package", members: &PACK_MEMBERS };


static FACT_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"XNAM"),
        name: "Relation",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Flags",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
];

/// FACT — faction.
pub static FACT_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"FACT"), name: "Faction", members: &FACT_MEMBERS };


static CSTY_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "General Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// CSTY — combat style.
pub static CSTY_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"CSTY"), name: "Combat Style", members: &CSTY_MEMBERS };


static IDLE_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Filename",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    CTDA_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// IDLE — animation (idle).
pub static IDLE_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"IDLE"), name: "Animation", members: &IDLE_MEMBERS };


static LVLN_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    OBND_DEF,
    SubRecordDef {
        sig: Signature(*b"LVLD"),
        name: "Chance None",
        required: false,
        repeating: false,
        field: FieldType::UInt8,
    },
    SubRecordDef {
        sig: Signature(*b"LLCT"),
        name: "Entry Count",
        required: false,
        repeating: false,
        field: FieldType::UInt8,
    },
    SubRecordDef {
        sig: Signature(*b"LVLO"),
        name: "Leveled List Entry",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
];

/// LVLN — leveled NPC list.
pub static LVLN_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"LVLN"), name: "Leveled NPC", members: &LVLN_MEMBERS };


static LVLI_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    OBND_DEF,
    SubRecordDef {
        sig: Signature(*b"LVLD"),
        name: "Chance None",
        required: false,
        repeating: false,
        field: FieldType::UInt8,
    },
    SubRecordDef {
        sig: Signature(*b"LLCT"),
        name: "Entry Count",
        required: false,
        repeating: false,
        field: FieldType::UInt8,
    },
    SubRecordDef {
        sig: Signature(*b"LVLO"),
        name: "Leveled List Entry",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
];

/// LVLI — leveled item list.
pub static LVLI_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"LVLI"), name: "Leveled Item", members: &LVLI_MEMBERS };


static LVSP_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    OBND_DEF,
    SubRecordDef {
        sig: Signature(*b"LVLD"),
        name: "Chance None",
        required: false,
        repeating: false,
        field: FieldType::UInt8,
    },
    SubRecordDef {
        sig: Signature(*b"LLCT"),
        name: "Entry Count",
        required: false,
        repeating: false,
        field: FieldType::UInt8,
    },
];

/// LVSP — leveled spell list.
pub static LVSP_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"LVSP"), name: "Leveled Spell", members: &LVSP_MEMBERS };


static LVLP_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    OBND_DEF,
    SubRecordDef {
        sig: Signature(*b"LVLD"),
        name: "Chance None",
        required: false,
        repeating: false,
        field: FieldType::UInt8,
    },
    SubRecordDef {
        sig: Signature(*b"LLCT"),
        name: "Entry Count",
        required: false,
        repeating: false,
        field: FieldType::UInt8,
    },
];

/// LVLP — leveled pack-in list (Starfield-exclusive).
pub static LVLP_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"LVLP"), name: "Leveled Pack In", members: &LVLP_MEMBERS };


static BPTD_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"BPND"),
        name: "Body Part Node",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
];

/// BPTD — body part data.
pub static BPTD_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"BPTD"), name: "Body Part Data", members: &BPTD_MEMBERS };


static OTFT_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"INAM"),
        name: "Items",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// OTFT — outfit.
pub static OTFT_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"OTFT"), name: "Outfit", members: &OTFT_MEMBERS };


static EYES_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"ICON"),
        name: "Texture",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Flags",
        required: false,
        repeating: false,
        field: FieldType::UInt8,
    },
];

/// EYES — eyes definition.
pub static EYES_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"EYES"), name: "Eyes", members: &EYES_MEMBERS };


static CLAS_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"DESC"),
        name: "Description",
        required: false,
        repeating: false,
        field: FieldType::LString,
    },
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// CLAS — character class.
pub static CLAS_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"CLAS"), name: "Class", members: &CLAS_MEMBERS };


static PERK_MEMBERS: [SubRecordDef; 7] = [
    EDID_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"DESC"),
        name: "Description",
        required: false,
        repeating: false,
        field: FieldType::LString,
    },
    VMAD_DEF,
    CTDA_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Perk Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"PRKE"),
        name: "Perk Effect",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
];

/// PERK — perk definition.
pub static PERK_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"PERK"), name: "Perk", members: &PERK_MEMBERS };

// Suppress unused import warning on EFID_DEF/EFIT_DEF brought in for future
// effect-bearing records in this module.
const _: () = {
    let _ = &EFID_DEF;
    let _ = &EFIT_DEF;
};
