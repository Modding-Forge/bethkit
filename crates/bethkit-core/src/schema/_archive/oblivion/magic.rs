// SPDX-License-Identifier: Apache-2.0
//!
//! Oblivion magic record schemas.
//!
//! Covers SPEL, MGEF, ENCH.

use crate::schema::{FieldDef, FieldType, RecordSchema, SubRecordDef};
use crate::types::Signature;

use super::common::{EDID_DEF, FULL_DEF, ICON_DEF};
use super::enums::OBLIVION_MAGIC_SCHOOL_ENUM;


static SPEL_DATA_FIELDS: [FieldDef; 5] = [
    FieldDef { name: "Type", kind: FieldType::UInt32 },
    FieldDef { name: "Cost", kind: FieldType::UInt32 },
    FieldDef { name: "Level", kind: FieldType::UInt32 },
    FieldDef { name: "Flags", kind: FieldType::UInt8 },
    FieldDef { name: "_padding", kind: FieldType::ByteArray },
];

static SPEL_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"SPIT"),
        name: "Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&SPEL_DATA_FIELDS),
    },
];

/// SPEL — spell.
pub static SPEL_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"SPEL"), name: "Spell", members: &SPEL_MEMBERS };


static MGEF_DATA_FIELDS: [FieldDef; 8] = [
    FieldDef { name: "Flags", kind: FieldType::UInt32 },
    FieldDef { name: "Base Cost", kind: FieldType::Float32 },
    FieldDef { name: "Associated Item", kind: FieldType::UInt32 },
    FieldDef {
        name: "Magic School",
        kind: FieldType::Enum(&OBLIVION_MAGIC_SCHOOL_ENUM),
    },
    FieldDef { name: "Resist Value", kind: FieldType::UInt32 },
    FieldDef { name: "Counter Effect Count", kind: FieldType::UInt16 },
    FieldDef { name: "_padding", kind: FieldType::UInt8 },
    FieldDef { name: "Light", kind: FieldType::UInt32 },
];

static MGEF_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"DESC"),
        name: "Description",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&MGEF_DATA_FIELDS),
    },
];

/// MGEF — magic effect.
pub static MGEF_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"MGEF"), name: "Magic Effect", members: &MGEF_MEMBERS };


static ENCH_DATA_FIELDS: [FieldDef; 5] = [
    FieldDef { name: "Type", kind: FieldType::UInt32 },
    FieldDef { name: "Charge Amount", kind: FieldType::UInt32 },
    FieldDef { name: "Cost", kind: FieldType::UInt32 },
    FieldDef { name: "Flags", kind: FieldType::UInt8 },
    FieldDef { name: "_padding", kind: FieldType::ByteArray },
];

static ENCH_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"ENIT"),
        name: "Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&ENCH_DATA_FIELDS),
    },
];

/// ENCH — enchantment (soul gem enchanting and item enchantments).
pub static ENCH_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"ENCH"), name: "Enchantment", members: &ENCH_MEMBERS };

// Suppress lint for icon used indirectly.
const _: () = { let _ = ICON_DEF; };
