// SPDX-License-Identifier: Apache-2.0
//!
//! Fallout 3 magic and combat record schemas.
//!
//! Covers spells, magic effects, object enchantments, projectiles, explosions,
//! and impact data sets.

use crate::schema::{RecordSchema, SubRecordDef, FieldType};
use crate::types::Signature;

use super::common::{DATA_DEF, DESC_DEF, EDID_DEF, FULL_DEF, MODL_DEF, MODT_DEF, OBND_DEF};


static SPEL_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    FULL_DEF,
    DATA_DEF,
    SubRecordDef {
        sig: Signature(*b"EFID"),
        name: "Base Effect",
        required: false,
        repeating: true,
        field: FieldType::FormId,
    },
];

/// SPEL — spell definition.
pub static SPEL_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"SPEL"), name: "Spell", members: &SPEL_MEMBERS };


static MGEF_MEMBERS: [SubRecordDef; 7] = [
    EDID_DEF,
    FULL_DEF,
    DESC_DEF,
    SubRecordDef {
        sig: Signature(*b"ICON"),
        name: "Large Icon",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    MODL_DEF,
    MODT_DEF,
    DATA_DEF,
];

/// MGEF — base magic effect.
pub static MGEF_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"MGEF"), name: "Base Effect", members: &MGEF_MEMBERS };


static ENCH_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    FULL_DEF,
    DATA_DEF,
];

/// ENCH — object enchantment / weapon / armour effect.
pub static ENCH_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"ENCH"), name: "Object Effect", members: &ENCH_MEMBERS };


static PROJ_MEMBERS: [SubRecordDef; 6] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    MODT_DEF,
    DATA_DEF,
];

/// PROJ — projectile (bullets, missiles, grenades).
pub static PROJ_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"PROJ"), name: "Projectile", members: &PROJ_MEMBERS };


static EXPL_MEMBERS: [SubRecordDef; 6] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    MODT_DEF,
    DATA_DEF,
];

/// EXPL — explosion definition (radius, force, damage).
pub static EXPL_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"EXPL"), name: "Explosion", members: &EXPL_MEMBERS };


static IPCT_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    MODL_DEF,
    MODT_DEF,
    DATA_DEF,
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Sound Level",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// IPCT — impact data (decal / sound when a surface is struck).
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

/// IPDS — impact data set (maps material types to impacts).
pub static IPDS_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"IPDS"), name: "Impact Data Set", members: &IPDS_MEMBERS };
