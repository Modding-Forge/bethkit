// SPDX-License-Identifier: Apache-2.0
//!
//! Starfield magic / combat record schemas.
//!
//! Covers SPEL, MGEF, ENCH, DMGT, SDLT, PROJ, EXPL, HAZD, IPCT, IPDS.

use crate::schema::{FieldDef, FieldType, RecordSchema, SubRecordDef};
use crate::types::Signature;

use super::common::{
    CTDA_DEF, DESC_DEF, EDID_DEF, EFID_DEF, EFIT_DEF, FULL_DEF, KSIZ_DEF, KWDA_DEF, MODL_DEF,
    OBND_DEF,
};


static SPEL_MEMBERS: [SubRecordDef; 9] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    DESC_DEF,
    KSIZ_DEF,
    KWDA_DEF,
    SubRecordDef {
        sig: Signature(*b"SPIT"),
        name: "Spell Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    EFID_DEF,
    EFIT_DEF,
];

/// SPEL — spell.
pub static SPEL_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"SPEL"), name: "Spell", members: &SPEL_MEMBERS };


static MGEF_DATA_FIELDS: [FieldDef; 6] = [
    FieldDef { name: "Flags", kind: FieldType::UInt32 },
    FieldDef { name: "Base Cost", kind: FieldType::Float32 },
    FieldDef { name: "Resist Value", kind: FieldType::UInt32 },
    FieldDef { name: "Skill", kind: FieldType::UInt32 },
    FieldDef { name: "Level", kind: FieldType::UInt32 },
    FieldDef { name: "Casting Type", kind: FieldType::UInt32 },
];

static MGEF_MEMBERS: [SubRecordDef; 8] = [
    EDID_DEF,
    FULL_DEF,
    DESC_DEF,
    KSIZ_DEF,
    KWDA_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Magic Effect Data",
        required: false,
        repeating: false,
        field: FieldType::Struct(&MGEF_DATA_FIELDS),
    },
    CTDA_DEF,
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Unknown",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// MGEF — magic effect.
pub static MGEF_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"MGEF"), name: "Magic Effect", members: &MGEF_MEMBERS };


static ENCH_MEMBERS: [SubRecordDef; 7] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"ENIT"),
        name: "Enchantment Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    KSIZ_DEF,
    KWDA_DEF,
    EFID_DEF,
];

/// ENCH — object effect (enchantment).
pub static ENCH_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"ENCH"), name: "Enchantment", members: &ENCH_MEMBERS };


static DMGT_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    KSIZ_DEF,
];

/// DMGT — damage type definition (Starfield-specific per-type damage model).
pub static DMGT_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"DMGT"), name: "Damage Type", members: &DMGT_MEMBERS };


static SDLT_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DAMA"),
        name: "Damage Association",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"ACTV"),
        name: "Actor Value",
        required: false,
        repeating: true,
        field: FieldType::FormId,
    },
];

/// SDLT — secondary damage list (maps actor values to damage types).
pub static SDLT_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"SDLT"),
    name: "Secondary Damage List",
    members: &SDLT_MEMBERS,
};


static PROJ_DATA_FIELDS: [FieldDef; 7] = [
    FieldDef { name: "Flags", kind: FieldType::UInt16 },
    FieldDef { name: "Type", kind: FieldType::UInt16 },
    FieldDef { name: "Gravity", kind: FieldType::Float32 },
    FieldDef { name: "Speed", kind: FieldType::Float32 },
    FieldDef { name: "Range", kind: FieldType::Float32 },
    FieldDef { name: "Light", kind: FieldType::FormId },
    FieldDef { name: "Muzzle Flash", kind: FieldType::FormId },
];

static PROJ_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Projectile Data",
        required: false,
        repeating: false,
        field: FieldType::Struct(&PROJ_DATA_FIELDS),
    },
];

/// PROJ — projectile.
pub static PROJ_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"PROJ"), name: "Projectile", members: &PROJ_MEMBERS };


static EXPL_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Explosion Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// EXPL — explosion.
pub static EXPL_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"EXPL"), name: "Explosion", members: &EXPL_MEMBERS };


static HAZD_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Hazard Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// HAZD — hazard (environmental damage zone).
pub static HAZD_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"HAZD"), name: "Hazard", members: &HAZD_MEMBERS };


static IPCT_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    MODL_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Impact Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Sound Level",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
];

/// IPCT — impact (surface material impact definition).
pub static IPCT_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"IPCT"), name: "Impact", members: &IPCT_MEMBERS };


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

/// IPDS — impact data set.
pub static IPDS_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"IPDS"), name: "Impact Data Set", members: &IPDS_MEMBERS };
