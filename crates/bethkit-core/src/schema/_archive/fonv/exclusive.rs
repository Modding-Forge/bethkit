// SPDX-License-Identifier: Apache-2.0
//!
//! Fallout New Vegas exclusive record schemas.
//!
//! These records have no counterpart in Fallout 3. They include the challenge
//! system, reputation tracking, casino games, crafting recipes, item mods,
//! survival mechanics, and media systems.

use crate::schema::{FieldType, RecordSchema, SubRecordDef};
use crate::types::Signature;

use super::super::fo3::common::{
    DATA_DEF, DESC_DEF, EDID_DEF, FULL_DEF, MODL_DEF, MODT_DEF, OBND_DEF,
};

static CHAL_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"ICON"),
        name: "Icon",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    DESC_DEF,
    DATA_DEF,
];

/// CHAL — challenge definition (FNV achievement-like system).
pub static CHAL_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"CHAL"),
    name: "Challenge",
    members: &CHAL_MEMBERS,
};

static REPU_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"ICON"),
        name: "Icon",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    DATA_DEF,
];

/// REPU — faction reputation (FNV faction standing system).
pub static REPU_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"REPU"),
    name: "Reputation",
    members: &REPU_MEMBERS,
};

static IMOD_MEMBERS: [SubRecordDef; 8] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    MODT_DEF,
    SubRecordDef {
        sig: Signature(*b"ICON"),
        name: "Icon",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    DESC_DEF,
    DATA_DEF,
];

/// IMOD — weapon / armour item mod (FNV weapon modification system).
pub static IMOD_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"IMOD"),
    name: "Item Mod",
    members: &IMOD_MEMBERS,
};

static RCPE_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"RCQY"),
        name: "Ingredient",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"RCOD"),
        name: "Output",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"RCCT"),
        name: "Category",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
];

/// RCPE — crafting recipe (FNV campfire / workbench crafting).
pub static RCPE_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"RCPE"),
    name: "Recipe",
    members: &RCPE_MEMBERS,
};

static RCCT_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Workbench Keyword",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
];

/// RCCT — recipe category (groups crafting recipes by workbench type).
pub static RCCT_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"RCCT"),
    name: "Recipe Category",
    members: &RCCT_MEMBERS,
};

static CSNO_MEMBERS: [SubRecordDef; 3] = [EDID_DEF, FULL_DEF, DATA_DEF];

/// CSNO — casino definition (FNV gambling venues).
pub static CSNO_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"CSNO"),
    name: "Casino",
    members: &CSNO_MEMBERS,
};

static CHIP_MEMBERS: [SubRecordDef; 6] =
    [EDID_DEF, OBND_DEF, FULL_DEF, MODL_DEF, MODT_DEF, DATA_DEF];

/// CHIP — casino chip currency item.
pub static CHIP_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"CHIP"),
    name: "Casino Chip",
    members: &CHIP_MEMBERS,
};

static CCRD_MEMBERS: [SubRecordDef; 7] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    MODT_DEF,
    SubRecordDef {
        sig: Signature(*b"ICON"),
        name: "Icon",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    DATA_DEF,
];

/// CCRD — caravan card (FNV card game item).
pub static CCRD_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"CCRD"),
    name: "Caravan Card",
    members: &CCRD_MEMBERS,
};

static CDCK_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Card",
        required: false,
        repeating: true,
        field: FieldType::FormId,
    },
];

/// CDCK — caravan deck (collection of caravan cards).
pub static CDCK_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"CDCK"),
    name: "Caravan Deck",
    members: &CDCK_MEMBERS,
};

static CMNY_MEMBERS: [SubRecordDef; 5] = [EDID_DEF, OBND_DEF, FULL_DEF, MODL_DEF, DATA_DEF];

/// CMNY — caravan money (currency used in caravan gambling).
pub static CMNY_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"CMNY"),
    name: "Caravan Money",
    members: &CMNY_MEMBERS,
};

static DEHY_MEMBERS: [SubRecordDef; 2] = [EDID_DEF, DATA_DEF];

/// DEHY — dehydration stage (FNV Hardcore mode thirst level).
pub static DEHY_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"DEHY"),
    name: "Dehydration Stage",
    members: &DEHY_MEMBERS,
};

static HUNG_MEMBERS: [SubRecordDef; 2] = [EDID_DEF, DATA_DEF];

/// HUNG — hunger stage (FNV Hardcore mode hunger level).
pub static HUNG_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"HUNG"),
    name: "Hunger Stage",
    members: &HUNG_MEMBERS,
};

static SLPD_MEMBERS: [SubRecordDef; 2] = [EDID_DEF, DATA_DEF];

/// SLPD — sleep deprivation stage (FNV Hardcore mode fatigue level).
pub static SLPD_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"SLPD"),
    name: "Sleep Deprivation Stage",
    members: &SLPD_MEMBERS,
};

static MSET_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"NAM1"),
        name: "Battle (During)",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
];

/// MSET — media set (FNV dynamic music system configuration).
pub static MSET_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"MSET"),
    name: "Media Set",
    members: &MSET_MEMBERS,
};

static ALOC_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"NAM1"),
        name: "Neutral Set",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
];

/// ALOC — media location controller (FNV dynamic music zone).
pub static ALOC_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"ALOC"),
    name: "Media Location Controller",
    members: &ALOC_MEMBERS,
};

static AMEF_MEMBERS: [SubRecordDef; 3] = [EDID_DEF, FULL_DEF, DATA_DEF];

/// AMEF — ammunition effect (FNV special ammo type modifier).
pub static AMEF_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"AMEF"),
    name: "Ammo Effect",
    members: &AMEF_MEMBERS,
};

static LSCT_MEMBERS: [SubRecordDef; 2] = [EDID_DEF, DATA_DEF];

/// LSCT — load screen type (categorises load screen entries).
pub static LSCT_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"LSCT"),
    name: "Load Screen Type",
    members: &LSCT_MEMBERS,
};
