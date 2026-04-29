// SPDX-License-Identifier: Apache-2.0
//!
//! Schema definitions for actor / NPC-related SSE record types.
//!
//! Covers: NPC_, RACE, PACK, FACT, CSTY, IDLE, CLAS, EYES, OTFT, BPTD,
//! LVLN, LVLI, LVSP.

use crate::schema::{FieldDef, FieldType, RecordSchema, SubRecordDef};
use crate::types::Signature;

use super::common::{
    CTDA_DEF, DESC_DEF, EDID_DEF, ETYP_DEF, FULL_DEF, ICON_DEF, KSIZ_DEF, KWDA_DEF, MODL_DEF,
    OBND_DEF, RNAM_DEF, SPCT_DEF, SPLO_DEF, VMAD_DEF,
};
use crate::schema::enums::{ACTOR_VALUE_ENUM, NPC_FLAGS, PACKAGE_TYPE_ENUM};


static NPC_ACBS_FIELDS: [FieldDef; 9] = [
    FieldDef { name: "Flags", kind: FieldType::Flags(&NPC_FLAGS) },
    FieldDef { name: "Magicka Offset", kind: FieldType::Int16 },
    FieldDef { name: "Stamina Offset", kind: FieldType::Int16 },
    FieldDef { name: "Level", kind: FieldType::UInt16 },
    FieldDef { name: "Calc Min Level", kind: FieldType::UInt8 },
    FieldDef { name: "Calc Max Level", kind: FieldType::UInt8 },
    FieldDef { name: "Speed Mult", kind: FieldType::UInt16 },
    FieldDef { name: "Disposition Base", kind: FieldType::Int16 },
    FieldDef { name: "Template Data Flags", kind: FieldType::UInt16 },
];

static NPC_MEMBERS: [SubRecordDef; 18] = [
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
        field: FieldType::FormIdTyped(&[Signature(*b"LVLI")]),
    },
    SubRecordDef {
        sig: Signature(*b"VTCK"),
        name: "Voice",
        required: false,
        repeating: false,
        field: FieldType::FormIdTyped(&[Signature(*b"VTYP")]),
    },
    SubRecordDef {
        sig: Signature(*b"TPLT"),
        name: "Template",
        required: false,
        repeating: false,
        field: FieldType::FormIdTyped(&[Signature(*b"LVLN"), Signature(*b"NPC_")]),
    },
    RNAM_DEF,
    SPCT_DEF,
    SPLO_DEF,
    KSIZ_DEF,
    KWDA_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"SHRT"),
        name: "Short Name",
        required: false,
        repeating: false,
        field: FieldType::LString,
    },
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
    SubRecordDef {
        sig: Signature(*b"CNAM"),
        name: "Class",
        required: false,
        repeating: false,
        field: FieldType::FormIdTyped(&[Signature(*b"CLAS")]),
    },
];

/// NPC_ — Non-Player Character.
pub static NPC_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"NPC_"),
    name: "Non-Player Character",
    members: &NPC_MEMBERS,
};


static RACE_MEMBERS: [SubRecordDef; 8] = [
    EDID_DEF,
    FULL_DEF,
    DESC_DEF,
    SPCT_DEF,
    SPLO_DEF,
    KSIZ_DEF,
    KWDA_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Race Data",
        required: true,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// RACE — Race (biological race, not faction).
pub static RACE_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"RACE"),
    name: "Race",
    members: &RACE_MEMBERS,
};


static PACK_PKDT_FIELDS: [FieldDef; 4] = [
    FieldDef { name: "General Flags", kind: FieldType::UInt32 },
    FieldDef { name: "Type", kind: FieldType::Enum(&PACKAGE_TYPE_ENUM) },
    FieldDef { name: "Interrupt Override", kind: FieldType::UInt8 },
    FieldDef { name: "Preferred Speed", kind: FieldType::UInt8 },
];

static PACK_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    VMAD_DEF,
    SubRecordDef {
        sig: Signature(*b"PKDT"),
        name: "Package Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&PACK_PKDT_FIELDS),
    },
    CTDA_DEF,
    SubRecordDef {
        sig: Signature(*b"CNAM"),
        name: "Combat Style",
        required: false,
        repeating: false,
        field: FieldType::FormIdTyped(&[Signature(*b"CSTY")]),
    },
];

/// PACK — AI package.
pub static PACK_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"PACK"),
    name: "Package",
    members: &PACK_MEMBERS,
};


static FACT_DATA_FIELDS: [FieldDef; 2] = [
    FieldDef { name: "Flags", kind: FieldType::UInt32 },
    FieldDef { name: "Unknown", kind: FieldType::UInt8 },
];

static FACT_MEMBERS: [SubRecordDef; 6] = [
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
        field: FieldType::Struct(&FACT_DATA_FIELDS),
    },
    SubRecordDef {
        sig: Signature(*b"JAIL"),
        name: "Jail",
        required: false,
        repeating: false,
        field: FieldType::FormIdTyped(&[Signature(*b"REFR")]),
    },
    SubRecordDef {
        sig: Signature(*b"WAIT"),
        name: "Follower Wait Marker",
        required: false,
        repeating: false,
        field: FieldType::FormIdTyped(&[Signature(*b"REFR")]),
    },
];

/// FACT — Faction.
pub static FACT_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"FACT"),
    name: "Faction",
    members: &FACT_MEMBERS,
};


static CSTY_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"CSGD"),
        name: "General Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"CSMD"),
        name: "Melee Data",
        required: false,
        repeating: false,
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

/// CSTY — Combat style.
pub static CSTY_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"CSTY"),
    name: "Combat Style",
    members: &CSTY_MEMBERS,
};


static IDLE_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    CTDA_DEF,
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Behavior Graph",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"ENAM"),
        name: "Animation Event",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"ANAM"),
        name: "Related Idles",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// IDLE — Idle animation.
pub static IDLE_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"IDLE"),
    name: "Idle Animation",
    members: &IDLE_MEMBERS,
};


static CLAS_DATA_FIELDS: [FieldDef; 4] = [
    FieldDef { name: "Unknown", kind: FieldType::Int32 },
    FieldDef { name: "Training Skill", kind: FieldType::Enum(&ACTOR_VALUE_ENUM) },
    FieldDef { name: "Training Level", kind: FieldType::UInt8 },
    FieldDef { name: "Skill Weights", kind: FieldType::ByteArray },
];

static CLAS_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    FULL_DEF,
    DESC_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&CLAS_DATA_FIELDS),
    },
];

/// CLAS — Character class.
pub static CLAS_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"CLAS"),
    name: "Class",
    members: &CLAS_MEMBERS,
};


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
        required: true,
        repeating: false,
        field: FieldType::UInt8,
    },
];

/// EYES — Eyes (visual appearance component).
pub static EYES_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"EYES"),
    name: "Eyes",
    members: &EYES_MEMBERS,
};


static OTFT_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"INAM"),
        name: "Items",
        required: true,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// OTFT — Outfit (set of worn items).
pub static OTFT_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"OTFT"),
    name: "Outfit",
    members: &OTFT_MEMBERS,
};


static BPTD_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    MODL_DEF,
    SubRecordDef {
        sig: Signature(*b"BPTN"),
        name: "Part Name",
        required: false,
        repeating: true,
        field: FieldType::LString,
    },
];

/// BPTD — Body part data (hit / dismemberment zones).
pub static BPTD_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"BPTD"),
    name: "Body Part Data",
    members: &BPTD_MEMBERS,
};


static LVLN_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    OBND_DEF,
    SubRecordDef {
        sig: Signature(*b"LVLD"),
        name: "Chance None",
        required: true,
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

/// LVLN — Leveled NPC list.
pub static LVLN_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"LVLN"),
    name: "Leveled NPC",
    members: &LVLN_MEMBERS,
};


static LVLI_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    OBND_DEF,
    SubRecordDef {
        sig: Signature(*b"LVLD"),
        name: "Chance None",
        required: true,
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

/// LVLI — Leveled item list.
pub static LVLI_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"LVLI"),
    name: "Leveled Item",
    members: &LVLI_MEMBERS,
};


static LVSP_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    OBND_DEF,
    SubRecordDef {
        sig: Signature(*b"LVLD"),
        name: "Chance None",
        required: true,
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

/// LVSP — Leveled spell list.
pub static LVSP_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"LVSP"),
    name: "Leveled Spell",
    members: &LVSP_MEMBERS,
};

// Suppress unused-import warnings for items pulled in transitively.
const _: () = {
    let _ = &ETYP_DEF;
    let _ = &ICON_DEF;
};
