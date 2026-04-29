// SPDX-License-Identifier: Apache-2.0
//! Morrowind actor record schemas.
//!
//! Includes NPCs, creatures, races, classes, factions, skills, and magic
//! effects.

use crate::schema::{FieldDef, FieldType, RecordSchema, SubRecordDef};
use crate::types::Signature;

use super::common::{
    AIDT_DEF, AI_A_DEF, AI_E_DEF, AI_F_DEF, AI_T_DEF, AI_W_DEF, DELE_DEF, DESC_DEF, DNAM_DEF,
    DODT_DEF, ENAM_EFFECT_DEF, FNAM_DEF, ITEX_DEF, MODL_DEF, NAME_DEF, NPCO_DEF, NPCS_DEF,
    SCRI_DEF, XSCL_DEF,
};


static NPC_MEMBERS: [SubRecordDef; 23] = [
    NAME_DEF,
    DELE_DEF,
    MODL_DEF,
    FNAM_DEF,
    SubRecordDef {
        sig: Signature(*b"RNAM"),
        name: "Race",
        required: true,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"CNAM"),
        name: "Class",
        required: true,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"ANAM"),
        name: "Faction",
        required: true,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"BNAM"),
        name: "Head Body Part",
        required: true,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"KNAM"),
        name: "Hair Body Part",
        required: true,
        repeating: false,
        field: FieldType::ZString,
    },
    SCRI_DEF,
    SubRecordDef {
        sig: Signature(*b"NPDT"),
        name: "NPC Stats",
        required: true,
        repeating: false,
        // Union: 52 bytes (manual stats) or 12 bytes (auto-calculate).
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"FLAG"),
        name: "NPC Flags",
        required: true,
        repeating: false,
        field: FieldType::UInt32,
    },
    NPCO_DEF,
    NPCS_DEF,
    AIDT_DEF,
    DODT_DEF,
    DNAM_DEF,
    AI_W_DEF,
    AI_T_DEF,
    AI_F_DEF,
    AI_E_DEF,
    AI_A_DEF,
    XSCL_DEF,
];

/// Schema for the `NPC_` non-player character record.
pub(super) static NPC_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"NPC_"),
    name: "Non-Player Character",
    members: &NPC_MEMBERS,
};


static CREA_MEMBERS: [SubRecordDef; 19] = [
    NAME_DEF,
    DELE_DEF,
    MODL_DEF,
    SubRecordDef {
        sig: Signature(*b"CNAM"),
        name: "Sound Generator Source",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    FNAM_DEF,
    SCRI_DEF,
    SubRecordDef {
        sig: Signature(*b"NPDT"),
        name: "Creature Stats",
        required: true,
        repeating: false,
        // 96 bytes: type(u32), level(u32), attrs([u32;8]), health(u32),
        //   magicka(u32), fatigue(u32), soul(u32), skills([u32;3]),
        //   attack_sets([{min,max};3]), barter_gold(u32)
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"FLAG"),
        name: "Creature Flags",
        required: true,
        repeating: false,
        field: FieldType::UInt32,
    },
    XSCL_DEF,
    NPCO_DEF,
    NPCS_DEF,
    AIDT_DEF,
    DODT_DEF,
    DNAM_DEF,
    AI_W_DEF,
    AI_T_DEF,
    AI_F_DEF,
    AI_E_DEF,
    AI_A_DEF,
];

/// Schema for the `CREA` creature record.
pub(super) static CREA_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"CREA"),
    name: "Creature",
    members: &CREA_MEMBERS,
};


static RACE_MEMBERS: [SubRecordDef; 6] = [
    NAME_DEF,
    DELE_DEF,
    FNAM_DEF,
    SubRecordDef {
        sig: Signature(*b"RADT"),
        name: "Race Data",
        required: true,
        repeating: false,
        // 140 bytes: skill_bonuses([{skill,bonus};7]), base_attrs([{m,f};8]),
        //   height({m,f}), weight({m,f}), flags(u32)
        field: FieldType::ByteArray,
    },
    NPCS_DEF,
    DESC_DEF,
];

/// Schema for the `RACE` race record.
pub(super) static RACE_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"RACE"),
    name: "Race",
    members: &RACE_MEMBERS,
};


static CLAS_MEMBERS: [SubRecordDef; 5] = [
    NAME_DEF,
    DELE_DEF,
    FNAM_DEF,
    SubRecordDef {
        sig: Signature(*b"CLDT"),
        name: "Class Data",
        required: true,
        repeating: false,
        // 60 bytes: primary_attrs([i32;2]), specialization(u32),
        //   skill_sets([{minor,major};5]), playable(u32), service_flags(u32)
        field: FieldType::ByteArray,
    },
    DESC_DEF,
];

/// Schema for the `CLAS` class record.
pub(super) static CLAS_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"CLAS"),
    name: "Class",
    members: &CLAS_MEMBERS,
};


static FACT_MEMBERS: [SubRecordDef; 7] = [
    NAME_DEF,
    DELE_DEF,
    FNAM_DEF,
    SubRecordDef {
        sig: Signature(*b"RNAM"),
        name: "Rank Name",
        required: false,
        repeating: true,
        // Up to 10 rank names, each a [u8;32] null-padded string.
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"FADT"),
        name: "Faction Data",
        required: true,
        repeating: false,
        // 240 bytes: complex nested rank requirements + favored attributes/skills
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"ANAM"),
        name: "Faction Relation",
        required: false,
        repeating: true,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"INTV"),
        name: "Reaction Value",
        required: false,
        repeating: true,
        field: FieldType::Int32,
    },
];

/// Schema for the `FACT` faction record.
pub(super) static FACT_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"FACT"),
    name: "Faction",
    members: &FACT_MEMBERS,
};


static SKIL_MEMBERS: [SubRecordDef; 4] = [
    SubRecordDef {
        sig: Signature(*b"INDX"),
        name: "Skill Index",
        required: true,
        repeating: false,
        // Skill enum ID (0–26); primary key for this record type.
        field: FieldType::UInt32,
    },
    DELE_DEF,
    SubRecordDef {
        sig: Signature(*b"SKDT"),
        name: "Skill Data",
        required: true,
        repeating: false,
        // 24 bytes: governing_attr(i32), skill_type(u32),
        //   actions(union: 4 × f32, skill-dependent layout)
        field: FieldType::ByteArray,
    },
    DESC_DEF,
];

/// Schema for the `SKIL` skill record.
///
/// Identified by `INDX` (skill enum), not by `NAME`.
pub(super) static SKIL_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"SKIL"),
    name: "Skill",
    members: &SKIL_MEMBERS,
};


static MGEF_MEDT_FIELDS: [FieldDef; 9] = [
    FieldDef { name: "School",           kind: FieldType::UInt32 },
    FieldDef { name: "Base Cost",        kind: FieldType::Float32 },
    FieldDef { name: "Flags",            kind: FieldType::UInt32 },
    FieldDef { name: "Light Red",        kind: FieldType::UInt32 },
    FieldDef { name: "Light Green",      kind: FieldType::UInt32 },
    FieldDef { name: "Light Blue",       kind: FieldType::UInt32 },
    FieldDef { name: "Size Multiplier",  kind: FieldType::Float32 },
    FieldDef { name: "Speed Multiplier", kind: FieldType::Float32 },
    FieldDef { name: "Size Cap",         kind: FieldType::Float32 },
];

static MGEF_MEMBERS: [SubRecordDef; 14] = [
    SubRecordDef {
        sig: Signature(*b"INDX"),
        name: "Effect Index",
        required: true,
        repeating: false,
        // Magic effect enum ID (0–142); primary key for this record type.
        field: FieldType::UInt32,
    },
    DELE_DEF,
    SubRecordDef {
        sig: Signature(*b"MEDT"),
        name: "Magic Effect Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&MGEF_MEDT_FIELDS),
    },
    ITEX_DEF,
    SubRecordDef {
        sig: Signature(*b"PTEX"),
        name: "Particle Texture",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"BSND"),
        name: "Bolt Sound",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"CSND"),
        name: "Cast Sound",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"HSND"),
        name: "Hit Sound",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"ASND"),
        name: "Area Sound",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"CVFX"),
        name: "Casting Visual",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"BVFX"),
        name: "Bolt Visual",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"HVFX"),
        name: "Hit Visual",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"AVFX"),
        name: "Area Visual",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    DESC_DEF,
];

/// Schema for the `MGEF` magic effect record.
///
/// Identified by `INDX` (magic effect enum), not by `NAME`.
pub(super) static MGEF_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"MGEF"),
    name: "Magic Effect",
    members: &MGEF_MEMBERS,
};

// NOTE: suppress unused-import warnings for defs used by actors but not MGEF
const _: () = {
    let _ = &ENAM_EFFECT_DEF;
};
