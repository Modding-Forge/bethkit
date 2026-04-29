// SPDX-License-Identifier: Apache-2.0
//!
//! Reusable static [`SubRecordDef`] helpers shared across many Oblivion
//! record definitions.
//!
//! Universal definitions (EDID, MODL, ICON) are re-exported from
//! [`crate::schema::shared`]. Oblivion-specific variants (FULL and DESC as
//! ZString, SCRI script reference) are defined here.

use crate::schema::{FieldType, SubRecordDef};
use crate::types::Signature;

pub use crate::schema::shared::{EDID_DEF, ICON_DEF, MODL_DEF};


/// FULL — full display name (non-localized ZString in Oblivion).
pub static FULL_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"FULL"),
    name: "Full Name",
    required: false,
    repeating: false,
    field: FieldType::ZString,
};


/// DESC — description text (non-localized ZString in Oblivion).
pub static DESC_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"DESC"),
    name: "Description",
    required: false,
    repeating: false,
    field: FieldType::ZString,
};


/// SCRI — attached script (FormId reference to a SCPT record).
pub static SCRI_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"SCRI"),
    name: "Script",
    required: false,
    repeating: false,
    field: FieldType::FormId,
};


/// CTDA — condition entry (raw byte array; structure varies by condition
/// function index).
pub static CTDA_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"CTDA"),
    name: "Condition",
    required: false,
    repeating: true,
    field: FieldType::ByteArray,
};


/// SNAM — open / ambient sound (FormId reference to a SOUN record).
pub static SNAM_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"SNAM"),
    name: "Sound",
    required: false,
    repeating: false,
    field: FieldType::FormId,
};

/// QNAM — close sound (FormId reference to a SOUN record).
pub static QNAM_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"QNAM"),
    name: "Close Sound",
    required: false,
    repeating: false,
    field: FieldType::FormId,
};
