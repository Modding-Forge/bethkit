// SPDX-License-Identifier: Apache-2.0
//!
//! Fallout 3 actor record schemas.
//!
//! Includes humanoid NPCs, creatures, races, factions, AI packages, combat
//! styles, and related supporting records.

use crate::schema::{RecordSchema, SubRecordDef, FieldType};
use crate::types::Signature;

use super::common::{CTDA_DEF, DATA_DEF, DESC_DEF, DNAM_DEF, EDID_DEF, FULL_DEF, MODL_DEF,
    MODT_DEF, SCRI_DEF};


static NPC_MEMBERS: [SubRecordDef; 15] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"OBND"),
        name: "Object Bounds",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    FULL_DEF,
    MODL_DEF,
    MODT_DEF,
    SubRecordDef {
        sig: Signature(*b"ACBS"),
        name: "Configuration",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
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
    SubRecordDef {
        sig: Signature(*b"RNAM"),
        name: "Race",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    SCRI_DEF,
    SubRecordDef {
        sig: Signature(*b"CNTO"),
        name: "Item",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"AIDT"),
        name: "AI Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    DATA_DEF,
];

/// NPC_ — humanoid non-player character.
pub static NPC_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"NPC_"),
    name: "Non-Player Character",
    members: &NPC_MEMBERS,
};


static CREA_MEMBERS: [SubRecordDef; 16] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"OBND"),
        name: "Object Bounds",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    FULL_DEF,
    MODL_DEF,
    MODT_DEF,
    SubRecordDef {
        sig: Signature(*b"ACBS"),
        name: "Configuration",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
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
    SCRI_DEF,
    SubRecordDef {
        sig: Signature(*b"CNTO"),
        name: "Item",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"AIDT"),
        name: "AI Data",
        required: false,
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
        sig: Signature(*b"TPLT"),
        name: "Template",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"ZNAM"),
        name: "Combat Style",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    DATA_DEF,
    DNAM_DEF,
];

/// CREA — creature (non-humanoid NPC; removed in Skyrim).
pub static CREA_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"CREA"), name: "Creature", members: &CREA_MEMBERS };


static RACE_MEMBERS: [SubRecordDef; 6] = [
    EDID_DEF,
    FULL_DEF,
    DESC_DEF,
    SubRecordDef {
        sig: Signature(*b"SPLO"),
        name: "Spell",
        required: false,
        repeating: true,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"XNAM"),
        name: "Relation",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
    DATA_DEF,
];

/// RACE — playable or NPC race definition.
pub static RACE_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"RACE"), name: "Race", members: &RACE_MEMBERS };


static PACK_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"PKDT"),
        name: "Package Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"PSDT"),
        name: "Schedule",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    CTDA_DEF,
    DATA_DEF,
];

/// PACK — AI behaviour package.
pub static PACK_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"PACK"), name: "Package", members: &PACK_MEMBERS };


static FACT_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"XNAM"),
        name: "Relation",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
    DATA_DEF,
    SubRecordDef {
        sig: Signature(*b"RNAM"),
        name: "Rank",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
];

/// FACT — faction definition.
pub static FACT_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"FACT"), name: "Faction", members: &FACT_MEMBERS };


static CSTY_MEMBERS: [SubRecordDef; 4] = [EDID_DEF, DATA_DEF, DNAM_DEF,
    SubRecordDef {
        sig: Signature(*b"CNAM"),
        name: "Dodge/Cover",
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
    MODL_DEF,
    MODT_DEF,
    DATA_DEF,
];

/// IDLE — idle animation definition.
pub static IDLE_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"IDLE"),
    name: "Idle Animation",
    members: &IDLE_MEMBERS,
};


static LVLN_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"OBND"),
        name: "Object Bounds",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
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
        name: "Leveled List Entry",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
];

/// LVLN — leveled NPC list.
pub static LVLN_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"LVLN"), name: "Leveled NPC", members: &LVLN_MEMBERS };


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
        name: "Leveled List Entry",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
];

/// LVLC — leveled creature list (removed in Skyrim).
pub static LVLC_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"LVLC"),
    name: "Leveled Creature",
    members: &LVLC_MEMBERS,
};


static LVLI_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"OBND"),
        name: "Object Bounds",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
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
        name: "Leveled List Entry",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
];

/// LVLI — leveled item list.
pub static LVLI_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"LVLI"), name: "Leveled Item", members: &LVLI_MEMBERS };


static HAIR_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    FULL_DEF,
    MODL_DEF,
    MODT_DEF,
    DATA_DEF,
];

/// HAIR — hair style (merged into HDPT in Skyrim).
pub static HAIR_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"HAIR"), name: "Hair", members: &HAIR_MEMBERS };


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
    DATA_DEF,
];

/// EYES — eye texture definition.
pub static EYES_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"EYES"), name: "Eyes", members: &EYES_MEMBERS };


static CLAS_MEMBERS: [SubRecordDef; 4] = [EDID_DEF, FULL_DEF, DESC_DEF, DATA_DEF];

/// CLAS — actor class (determines skill bonuses and AI behaviour).
pub static CLAS_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"CLAS"), name: "Class", members: &CLAS_MEMBERS };


static PERK_MEMBERS: [SubRecordDef; 7] = [
    EDID_DEF,
    FULL_DEF,
    DESC_DEF,
    SubRecordDef {
        sig: Signature(*b"ICON"),
        name: "Icon",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"MICO"),
        name: "Small Icon",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    DATA_DEF,
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


static BPTD_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    MODL_DEF,
    SubRecordDef {
        sig: Signature(*b"BPTN"),
        name: "Body Part Name",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
];

/// BPTD — body part data (hit detection and gore).
pub static BPTD_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"BPTD"), name: "Body Part Data", members: &BPTD_MEMBERS };


static HDPT_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    FULL_DEF,
    MODL_DEF,
    MODT_DEF,
    DATA_DEF,
];

/// HDPT — head part (replaces HAIR/EYES in later games).
pub static HDPT_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"HDPT"), name: "Head Part", members: &HDPT_MEMBERS };
