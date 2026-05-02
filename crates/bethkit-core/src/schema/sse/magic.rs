// SPDX-License-Identifier: Apache-2.0
//!
//! Schema definitions for magic / effect SSE record types.
//!
//! Covers: SPEL, MGEF, ENCH, SCRL, EFSH, EXPL, HAZD, RFCT, PROJ,
//! PERK, IMGS, IMAD, IPCT, IPDS, ADDN, SPGD.

use crate::schema::{FieldDef, FieldType, RecordSchema, SubRecordDef};
use crate::types::Signature;

use super::common::{
    CTDA_DEF, DESC_DEF, EDID_DEF, EFID_DEF, EFIT_DEF, FULL_DEF, KSIZ_DEF, KWDA_DEF, MODL_DEF,
    OBND_DEF, VMAD_DEF,
};
use crate::schema::enums::{
    CASTING_TYPE_ENUM, DELIVERY_ENUM, MAGIC_EFFECT_FLAGS, PROJECTILE_FLAGS, PROJECTILE_TYPE_ENUM,
    SCHOOL_ENUM, SOUND_LEVEL_ENUM, SPELL_TYPE_ENUM,
};

static SPEL_SPIT_FIELDS: [FieldDef; 7] = [
    FieldDef {
        name: "Cost",
        kind: FieldType::UInt32,
    },
    FieldDef {
        name: "Flags",
        kind: FieldType::UInt32,
    },
    FieldDef {
        name: "Type",
        kind: FieldType::Enum(&SPELL_TYPE_ENUM),
    },
    FieldDef {
        name: "Charge Time",
        kind: FieldType::Float32,
    },
    FieldDef {
        name: "Casting Type",
        kind: FieldType::Enum(&CASTING_TYPE_ENUM),
    },
    FieldDef {
        name: "Delivery",
        kind: FieldType::Enum(&DELIVERY_ENUM),
    },
    FieldDef {
        name: "Cast Duration",
        kind: FieldType::Float32,
    },
];

static SPEL_MEMBERS: [SubRecordDef; 8] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    KSIZ_DEF,
    KWDA_DEF,
    DESC_DEF,
    SubRecordDef {
        sig: Signature(*b"SPIT"),
        name: "Spell Item Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&SPEL_SPIT_FIELDS),
    },
    EFID_DEF,
];

/// SPEL — Spell.
pub static SPEL_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"SPEL"),
    name: "Spell",
    members: &SPEL_MEMBERS,
};

static SCRL_SPIT_FIELDS: [FieldDef; 7] = [
    FieldDef {
        name: "Cost",
        kind: FieldType::UInt32,
    },
    FieldDef {
        name: "Flags",
        kind: FieldType::UInt32,
    },
    FieldDef {
        name: "Type",
        kind: FieldType::Enum(&SPELL_TYPE_ENUM),
    },
    FieldDef {
        name: "Charge Time",
        kind: FieldType::Float32,
    },
    FieldDef {
        name: "Casting Type",
        kind: FieldType::Enum(&CASTING_TYPE_ENUM),
    },
    FieldDef {
        name: "Delivery",
        kind: FieldType::Enum(&DELIVERY_ENUM),
    },
    FieldDef {
        name: "Cast Duration",
        kind: FieldType::Float32,
    },
];

static SCRL_DATA_FIELDS: [FieldDef; 2] = [
    FieldDef {
        name: "Value",
        kind: FieldType::UInt32,
    },
    FieldDef {
        name: "Weight",
        kind: FieldType::Float32,
    },
];

static SCRL_MEMBERS: [SubRecordDef; 10] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    KSIZ_DEF,
    KWDA_DEF,
    DESC_DEF,
    SubRecordDef {
        sig: Signature(*b"SPIT"),
        name: "Spell Item Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&SCRL_SPIT_FIELDS),
    },
    EFID_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Item Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&SCRL_DATA_FIELDS),
    },
    SubRecordDef {
        sig: Signature(*b"MDOB"),
        name: "Menu Display Object",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
];

/// SCRL — Scroll (single-use spell item).
pub static SCRL_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"SCRL"),
    name: "Scroll",
    members: &SCRL_MEMBERS,
};

static MGEF_DATA_FIELDS: [FieldDef; 16] = [
    FieldDef {
        name: "Flags",
        kind: FieldType::Flags(&MAGIC_EFFECT_FLAGS),
    },
    FieldDef {
        name: "Base Cost",
        kind: FieldType::Float32,
    },
    FieldDef {
        name: "Related ID",
        kind: FieldType::FormId,
    },
    FieldDef {
        name: "Skill",
        kind: FieldType::Enum(&SCHOOL_ENUM),
    },
    FieldDef {
        name: "Resist",
        kind: FieldType::Int32,
    },
    FieldDef {
        name: "Counter Effect Count",
        kind: FieldType::UInt32,
    },
    FieldDef {
        name: "Light",
        kind: FieldType::FormIdTyped(&[Signature(*b"LIGH")]),
    },
    FieldDef {
        name: "Taper Weight",
        kind: FieldType::Float32,
    },
    FieldDef {
        name: "Hit Shader",
        kind: FieldType::FormIdTyped(&[Signature(*b"EFSH")]),
    },
    FieldDef {
        name: "Enchant Shader",
        kind: FieldType::FormIdTyped(&[Signature(*b"EFSH")]),
    },
    FieldDef {
        name: "Minimum Skill Level",
        kind: FieldType::UInt32,
    },
    FieldDef {
        name: "Spellmaking Area",
        kind: FieldType::UInt32,
    },
    FieldDef {
        name: "Spellmaking Casting Time",
        kind: FieldType::Float32,
    },
    FieldDef {
        name: "Taper Curve",
        kind: FieldType::Float32,
    },
    FieldDef {
        name: "Taper Duration",
        kind: FieldType::Float32,
    },
    FieldDef {
        name: "Second AV Weight",
        kind: FieldType::Float32,
    },
];

static MGEF_MEMBERS: [SubRecordDef; 10] = [
    EDID_DEF,
    FULL_DEF,
    DESC_DEF,
    KSIZ_DEF,
    KWDA_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&MGEF_DATA_FIELDS),
    },
    SubRecordDef {
        sig: Signature(*b"SNDD"),
        name: "Sound",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Magic Effect Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    CTDA_DEF,
    EFID_DEF,
];

/// MGEF — Magic effect.
pub static MGEF_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"MGEF"),
    name: "Magic Effect",
    members: &MGEF_MEMBERS,
};

static ENCH_ENIT_FIELDS: [FieldDef; 6] = [
    FieldDef {
        name: "Enchantment Cost",
        kind: FieldType::UInt32,
    },
    FieldDef {
        name: "Flags",
        kind: FieldType::UInt32,
    },
    FieldDef {
        name: "Cast Type",
        kind: FieldType::Enum(&CASTING_TYPE_ENUM),
    },
    FieldDef {
        name: "Enchantment Amount",
        kind: FieldType::UInt32,
    },
    FieldDef {
        name: "Target Type",
        kind: FieldType::Enum(&DELIVERY_ENUM),
    },
    FieldDef {
        name: "Enchant Type",
        kind: FieldType::UInt32,
    },
];

static ENCH_MEMBERS: [SubRecordDef; 6] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"ENIT"),
        name: "Enchantment Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&ENCH_ENIT_FIELDS),
    },
    EFID_DEF,
    EFIT_DEF,
];

/// ENCH — Object enchantment.
pub static ENCH_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"ENCH"),
    name: "Object Effect",
    members: &ENCH_MEMBERS,
};

static RFCT_DATA_FIELDS: [FieldDef; 3] = [
    FieldDef {
        name: "Effect Art Object",
        kind: FieldType::FormIdTyped(&[Signature(*b"ARTO")]),
    },
    FieldDef {
        name: "Shader",
        kind: FieldType::FormIdTyped(&[Signature(*b"EFSH")]),
    },
    FieldDef {
        name: "Flags",
        kind: FieldType::UInt32,
    },
];

static RFCT_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&RFCT_DATA_FIELDS),
    },
];

/// RFCT — Visual effect (reference effect).
pub static RFCT_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"RFCT"),
    name: "Visual Effect",
    members: &RFCT_MEMBERS,
};

static PROJ_DATA_FIELDS: [FieldDef; 14] = [
    FieldDef {
        name: "Flags",
        kind: FieldType::Flags(&PROJECTILE_FLAGS),
    },
    FieldDef {
        name: "Type",
        kind: FieldType::Enum(&PROJECTILE_TYPE_ENUM),
    },
    FieldDef {
        name: "Gravity",
        kind: FieldType::Float32,
    },
    FieldDef {
        name: "Speed",
        kind: FieldType::Float32,
    },
    FieldDef {
        name: "Range",
        kind: FieldType::Float32,
    },
    FieldDef {
        name: "Light",
        kind: FieldType::FormIdTyped(&[Signature(*b"LIGH")]),
    },
    FieldDef {
        name: "Muzzle Flash",
        kind: FieldType::FormIdTyped(&[Signature(*b"LIGH")]),
    },
    FieldDef {
        name: "Tracer Chance",
        kind: FieldType::Float32,
    },
    FieldDef {
        name: "Explosion Alt Trigger Prox",
        kind: FieldType::Float32,
    },
    FieldDef {
        name: "Explosion Alt Trigger Timer",
        kind: FieldType::Float32,
    },
    FieldDef {
        name: "Explosion",
        kind: FieldType::FormIdTyped(&[Signature(*b"EXPL")]),
    },
    FieldDef {
        name: "Sound",
        kind: FieldType::FormIdTyped(&[Signature(*b"SNDR")]),
    },
    FieldDef {
        name: "Muzzle Flash Duration",
        kind: FieldType::Float32,
    },
    FieldDef {
        name: "Fade Duration",
        kind: FieldType::Float32,
    },
];

static PROJ_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&PROJ_DATA_FIELDS),
    },
];

/// PROJ — Projectile.
pub static PROJ_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"PROJ"),
    name: "Projectile",
    members: &PROJ_MEMBERS,
};

static EXPL_DATA_FIELDS: [FieldDef; 12] = [
    FieldDef {
        name: "Light",
        kind: FieldType::FormIdTyped(&[Signature(*b"LIGH")]),
    },
    FieldDef {
        name: "Sound 1",
        kind: FieldType::FormIdTyped(&[Signature(*b"SNDR")]),
    },
    FieldDef {
        name: "Sound 2",
        kind: FieldType::FormIdTyped(&[Signature(*b"SNDR")]),
    },
    FieldDef {
        name: "Impact Dataset",
        kind: FieldType::FormIdTyped(&[Signature(*b"IPDS")]),
    },
    FieldDef {
        name: "Place Hazard",
        kind: FieldType::FormId,
    },
    FieldDef {
        name: "Force",
        kind: FieldType::Float32,
    },
    FieldDef {
        name: "Damage",
        kind: FieldType::Float32,
    },
    FieldDef {
        name: "Inner Radius",
        kind: FieldType::Float32,
    },
    FieldDef {
        name: "Outer Radius",
        kind: FieldType::Float32,
    },
    FieldDef {
        name: "IS Radius",
        kind: FieldType::Float32,
    },
    FieldDef {
        name: "Vertical Offset Mult",
        kind: FieldType::Float32,
    },
    FieldDef {
        name: "Flags",
        kind: FieldType::UInt32,
    },
];

static EXPL_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&EXPL_DATA_FIELDS),
    },
];

/// EXPL — Explosion.
pub static EXPL_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"EXPL"),
    name: "Explosion",
    members: &EXPL_MEMBERS,
};

static HAZD_DATA_FIELDS: [FieldDef; 11] = [
    FieldDef {
        name: "Limit",
        kind: FieldType::UInt32,
    },
    FieldDef {
        name: "Radius",
        kind: FieldType::Float32,
    },
    FieldDef {
        name: "Lifetime",
        kind: FieldType::Float32,
    },
    FieldDef {
        name: "Image Space Radius",
        kind: FieldType::Float32,
    },
    FieldDef {
        name: "Target Interval",
        kind: FieldType::Float32,
    },
    FieldDef {
        name: "Flags",
        kind: FieldType::UInt32,
    },
    FieldDef {
        name: "Spell",
        kind: FieldType::FormIdTyped(&[Signature(*b"SPEL"), Signature(*b"ENCH")]),
    },
    FieldDef {
        name: "Light",
        kind: FieldType::FormIdTyped(&[Signature(*b"LIGH")]),
    },
    FieldDef {
        name: "Impact Dataset",
        kind: FieldType::FormIdTyped(&[Signature(*b"IPDS")]),
    },
    FieldDef {
        name: "Sound",
        kind: FieldType::FormIdTyped(&[Signature(*b"SNDR")]),
    },
    FieldDef {
        name: "Sound Level",
        kind: FieldType::Enum(&SOUND_LEVEL_ENUM),
    },
];

static HAZD_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&HAZD_DATA_FIELDS),
    },
];

/// HAZD — Hazard (area of effect damage zone).
pub static HAZD_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"HAZD"),
    name: "Hazard",
    members: &HAZD_MEMBERS,
};

static PERK_MEMBERS: [SubRecordDef; 8] = [
    EDID_DEF,
    VMAD_DEF,
    FULL_DEF,
    DESC_DEF,
    KSIZ_DEF,
    KWDA_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Perk Data",
        required: true,
        repeating: false,
        field: FieldType::ByteArray,
    },
    CTDA_DEF,
];

/// PERK — Perk.
pub static PERK_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"PERK"),
    name: "Perk",
    members: &PERK_MEMBERS,
};

static IMGS_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// IMGS — Image space modifier settings.
pub static IMGS_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"IMGS"),
    name: "Image Space",
    members: &IMGS_MEMBERS,
};

static IMAD_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Data",
        required: true,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// IMAD — Image space modifier animation.
pub static IMAD_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"IMAD"),
    name: "Image Space Modifier",
    members: &IMAD_MEMBERS,
};

static IPCT_DATA_FIELDS: [FieldDef; 6] = [
    FieldDef {
        name: "Effect Duration",
        kind: FieldType::Float32,
    },
    FieldDef {
        name: "Effect Orientation",
        kind: FieldType::UInt32,
    },
    FieldDef {
        name: "Angle Threshold",
        kind: FieldType::Float32,
    },
    FieldDef {
        name: "Placement Radius",
        kind: FieldType::Float32,
    },
    FieldDef {
        name: "Sound Level",
        kind: FieldType::Enum(&SOUND_LEVEL_ENUM),
    },
    FieldDef {
        name: "Flags",
        kind: FieldType::UInt32,
    },
];

static IPCT_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    MODL_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&IPCT_DATA_FIELDS),
    },
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Decal Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// IPCT — Impact.
pub static IPCT_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"IPCT"),
    name: "Impact",
    members: &IPCT_MEMBERS,
};

static IPDS_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"PNAM"),
        name: "Impact",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
];

/// IPDS — Impact dataset (maps material types to impacts).
pub static IPDS_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"IPDS"),
    name: "Impact Dataset",
    members: &IPDS_MEMBERS,
};

static ADDN_DATA_FIELDS: [FieldDef; 2] = [
    FieldDef {
        name: "Node Index",
        kind: FieldType::Int32,
    },
    FieldDef {
        name: "Flags",
        kind: FieldType::UInt16,
    },
];

static ADDN_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    OBND_DEF,
    MODL_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&ADDN_DATA_FIELDS),
    },
];

/// ADDN — Addon node (particle system attachment).
pub static ADDN_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"ADDN"),
    name: "Addon Node",
    members: &ADDN_MEMBERS,
};

static SPGD_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: true,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"ICON"),
        name: "Texture",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
];

/// SPGD — Shader particle geometry data.
pub static SPGD_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"SPGD"),
    name: "Shader Particle Geometry",
    members: &SPGD_MEMBERS,
};

static EFSH_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: true,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"ICON"),
        name: "Fill Texture",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
];

/// EFSH — Effect shader.
pub static EFSH_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"EFSH"),
    name: "Effect Shader",
    members: &EFSH_MEMBERS,
};

// Suppress dead-code warnings for imports only used in other record defs.
const _: () = {
    let _ = &VMAD_DEF;
};
