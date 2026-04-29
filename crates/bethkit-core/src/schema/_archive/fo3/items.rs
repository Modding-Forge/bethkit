// SPDX-License-Identifier: Apache-2.0
//!
//! Fallout 3 item record schemas.
//!
//! Covers weapons, armour, ammunition, consumables, misc items, keys, notes,
//! terminals, and the constructible object recipe record.

use crate::schema::{RecordSchema, SubRecordDef, FieldType};
use crate::types::Signature;

use super::common::{
    DATA_DEF, DESC_DEF, DNAM_DEF, EDID_DEF, FULL_DEF, ICON_DEF, MICO_DEF, MODL_DEF, MODT_DEF,
    OBND_DEF, SCRI_DEF, YNAM_DEF, ZNAM_DEF,
};


static WEAP_MEMBERS: [SubRecordDef; 14] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    MODT_DEF,
    ICON_DEF,
    MICO_DEF,
    SCRI_DEF,
    SubRecordDef {
        sig: Signature(*b"EITM"),
        name: "Object Effect",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"ANAM"),
        name: "Enchantment Points",
        required: false,
        repeating: false,
        field: FieldType::UInt16,
    },
    YNAM_DEF,
    ZNAM_DEF,
    DATA_DEF,
    DNAM_DEF,
];

/// WEAP — weapon definition.
pub static WEAP_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"WEAP"), name: "Weapon", members: &WEAP_MEMBERS };


static ARMO_MEMBERS: [SubRecordDef; 13] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"SCRI"),
        name: "Script",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"EITM"),
        name: "Object Effect",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"BMDT"),
        name: "Biped Model Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    MODL_DEF,
    MODT_DEF,
    ICON_DEF,
    MICO_DEF,
    YNAM_DEF,
    ZNAM_DEF,
    DATA_DEF,
];

/// ARMO — armour record.
pub static ARMO_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"ARMO"), name: "Armor", members: &ARMO_MEMBERS };


static ARMA_MEMBERS: [SubRecordDef; 7] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"BMDT"),
        name: "Biped Model Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    MODL_DEF,
    MODT_DEF,
    DATA_DEF,
];

/// ARMA — armour addon (alternate model for a biped slot).
pub static ARMA_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"ARMA"), name: "Armor Addon", members: &ARMA_MEMBERS };


static AMMO_MEMBERS: [SubRecordDef; 9] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    MODT_DEF,
    ICON_DEF,
    MICO_DEF,
    SubRecordDef {
        sig: Signature(*b"SNAM"),
        name: "Sound",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    DATA_DEF,
];

/// AMMO — ammunition definition.
pub static AMMO_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"AMMO"), name: "Ammunition", members: &AMMO_MEMBERS };


static BOOK_MEMBERS: [SubRecordDef; 11] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    MODT_DEF,
    ICON_DEF,
    MICO_DEF,
    SCRI_DEF,
    DESC_DEF,
    YNAM_DEF,
    DATA_DEF,
];

/// BOOK — book or holotape.
pub static BOOK_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"BOOK"), name: "Book", members: &BOOK_MEMBERS };


static ALCH_MEMBERS: [SubRecordDef; 11] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    MODT_DEF,
    ICON_DEF,
    MICO_DEF,
    SCRI_DEF,
    DESC_DEF,
    YNAM_DEF,
    DATA_DEF,
];

/// ALCH — ingestible (chems, food, stimpaks, RadAway, etc.).
pub static ALCH_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"ALCH"), name: "Ingestible", members: &ALCH_MEMBERS };


static INGR_MEMBERS: [SubRecordDef; 7] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    MODT_DEF,
    ICON_DEF,
    DATA_DEF,
];

/// INGR — crafting ingredient.
pub static INGR_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"INGR"), name: "Ingredient", members: &INGR_MEMBERS };


static MISC_MEMBERS: [SubRecordDef; 9] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    MODT_DEF,
    ICON_DEF,
    MICO_DEF,
    SCRI_DEF,
    DATA_DEF,
];

/// MISC — miscellaneous item.
pub static MISC_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"MISC"), name: "Misc. Item", members: &MISC_MEMBERS };


static KEYM_MEMBERS: [SubRecordDef; 9] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    MODT_DEF,
    ICON_DEF,
    MICO_DEF,
    SCRI_DEF,
    DATA_DEF,
];

/// KEYM — key item.
pub static KEYM_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"KEYM"), name: "Key", members: &KEYM_MEMBERS };


static NOTE_MEMBERS: [SubRecordDef; 8] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    MODT_DEF,
    ICON_DEF,
    SubRecordDef {
        sig: Signature(*b"YNAM"),
        name: "Sound - Pick Up",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    DATA_DEF,
];

/// NOTE — note, recording, or video log.
pub static NOTE_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"NOTE"), name: "Note", members: &NOTE_MEMBERS };


static TERM_MEMBERS: [SubRecordDef; 9] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    MODT_DEF,
    SCRI_DEF,
    DESC_DEF,
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Base Hacking Difficulty",
        required: false,
        repeating: false,
        field: FieldType::UInt8,
    },
    SubRecordDef {
        sig: Signature(*b"ITXT"),
        name: "Item Text",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
];

/// TERM — interactive terminal.
pub static TERM_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"TERM"), name: "Terminal", members: &TERM_MEMBERS };


static COBJ_MEMBERS: [SubRecordDef; 6] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"CTDA"),
        name: "Condition",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"CNTO"),
        name: "Component",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"BNAM"),
        name: "Workbench Keyword",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"CNAM"),
        name: "Created Object",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"NAM1"),
        name: "Created Count",
        required: false,
        repeating: false,
        field: FieldType::UInt16,
    },
];

/// COBJ — constructible object (crafting recipe).
pub static COBJ_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"COBJ"),
    name: "Constructible Object",
    members: &COBJ_MEMBERS,
};

// NOTE: SCRI_DEF is imported but TERM uses its own inline sub-record def;
// suppress unused warning.
const _: () = { let _ = &SCRI_DEF; };
