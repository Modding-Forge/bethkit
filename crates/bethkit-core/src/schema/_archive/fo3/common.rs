// SPDX-License-Identifier: Apache-2.0
//!
//! Reusable static [`SubRecordDef`] helpers shared across Fallout 3 record
//! definitions.
//!
//! `FULL` and `DESC` use [`FieldType::ZString`] because Fallout 3 predates the
//! localisation system introduced in Skyrim / Fallout 4. Universal definitions
//! (EDID, MODL, MODT, ICON, MICO) are re-exported from
//! [`crate::schema::shared`].

use crate::schema::{FieldDef, FieldType, SubRecordDef};
use crate::types::Signature;

pub use crate::schema::shared::{EDID_DEF, ICON_DEF, MICO_DEF, MODL_DEF, MODT_DEF};


/// FULL — full display name (non-localised ZString in Fallout 3).
pub static FULL_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"FULL"),
    name: "Full Name",
    required: false,
    repeating: false,
    field: FieldType::ZString,
};

/// DESC — description text (non-localised ZString in Fallout 3).
pub static DESC_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"DESC"),
    name: "Description",
    required: false,
    repeating: false,
    field: FieldType::ZString,
};


static OBND_FIELDS: [FieldDef; 6] = [
    FieldDef { name: "X1", kind: FieldType::Int16 },
    FieldDef { name: "Y1", kind: FieldType::Int16 },
    FieldDef { name: "Z1", kind: FieldType::Int16 },
    FieldDef { name: "X2", kind: FieldType::Int16 },
    FieldDef { name: "Y2", kind: FieldType::Int16 },
    FieldDef { name: "Z2", kind: FieldType::Int16 },
];

/// OBND — object bounding box (6 signed 16-bit integers).
pub static OBND_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"OBND"),
    name: "Object Bounds",
    required: false,
    repeating: false,
    field: FieldType::Struct(&OBND_FIELDS),
};


/// SCRI — attached script FormID (Oblivion / FO3 / FNV era).
pub static SCRI_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"SCRI"),
    name: "Script",
    required: false,
    repeating: false,
    field: FieldType::FormId,
};


/// CTDA — condition entry (repeating raw byte array; complex sub-structure).
pub static CTDA_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"CTDA"),
    name: "Condition",
    required: false,
    repeating: true,
    field: FieldType::ByteArray,
};


/// YNAM — pick-up sound FormID.
pub static YNAM_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"YNAM"),
    name: "Sound - Pick Up",
    required: false,
    repeating: false,
    field: FieldType::FormId,
};

/// ZNAM — put-down sound FormID.
pub static ZNAM_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"ZNAM"),
    name: "Sound - Put Down",
    required: false,
    repeating: false,
    field: FieldType::FormId,
};


/// DATA — primary game-stats data block (raw byte array; format varies per
/// record type).
pub static DATA_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"DATA"),
    name: "Data",
    required: false,
    repeating: false,
    field: FieldType::ByteArray,
};

/// DNAM — extended / secondary data block.
pub static DNAM_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"DNAM"),
    name: "Data (Extended)",
    required: false,
    repeating: false,
    field: FieldType::ByteArray,
};
