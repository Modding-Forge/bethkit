// SPDX-License-Identifier: Apache-2.0
//! Morrowind magic record schemas: spells and enchantments.

use crate::schema::{FieldDef, FieldType, RecordSchema, SubRecordDef};
use crate::types::Signature;

use super::common::{DELE_DEF, ENAM_EFFECT_DEF, FNAM_DEF, NAME_DEF};


static SPEL_SPDT_FIELDS: [FieldDef; 3] = [
    FieldDef { name: "Type",  kind: FieldType::UInt32 },
    FieldDef { name: "Cost",  kind: FieldType::UInt32 },
    FieldDef { name: "Flags", kind: FieldType::UInt32 },
];

static SPEL_MEMBERS: [SubRecordDef; 5] = [
    NAME_DEF,
    DELE_DEF,
    FNAM_DEF,
    SubRecordDef {
        sig: Signature(*b"SPDT"),
        name: "Spell Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&SPEL_SPDT_FIELDS),
    },
    ENAM_EFFECT_DEF,
];

/// Schema for the `SPEL` spell record.
pub(super) static SPEL_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"SPEL"),
    name: "Spell",
    members: &SPEL_MEMBERS,
};


static ENCH_ENDT_FIELDS: [FieldDef; 5] = [
    FieldDef { name: "Cast Type", kind: FieldType::UInt32 },
    FieldDef { name: "Cost",      kind: FieldType::UInt32 },
    FieldDef { name: "Charge",    kind: FieldType::UInt32 },
    FieldDef { name: "Flags",     kind: FieldType::UInt8 },
    FieldDef { name: "_unused",   kind: FieldType::Unused(3) },
];

static ENCH_MEMBERS: [SubRecordDef; 4] = [
    NAME_DEF,
    DELE_DEF,
    SubRecordDef {
        sig: Signature(*b"ENDT"),
        name: "Enchantment Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&ENCH_ENDT_FIELDS),
    },
    ENAM_EFFECT_DEF,
];

/// Schema for the `ENCH` enchantment record.
pub(super) static ENCH_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"ENCH"),
    name: "Enchantment",
    members: &ENCH_MEMBERS,
};
