// SPDX-License-Identifier: Apache-2.0
//!
//! Reusable static [`SubRecordDef`] helpers shared across many Fallout 4
//! record definitions.
//!
//! Universal definitions (EDID, MODL) are re-exported from
//! [`crate::schema::shared`].  All Fallout 4 specific or LString-variant
//! definitions are defined here.

use crate::schema::{ArrayCount, FieldDef, FieldType, SubRecordDef};
use crate::types::Signature;

pub use crate::schema::shared::{EDID_DEF, MODL_DEF};


/// FULL — full display name (localised string or inline in FO4).
pub static FULL_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"FULL"),
    name: "Full Name",
    required: false,
    repeating: false,
    field: FieldType::LString,
};

/// DESC — description text (localised).
pub static DESC_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"DESC"),
    name: "Description",
    required: false,
    repeating: false,
    field: FieldType::LString,
};


static OBND_FIELDS: [FieldDef; 6] = [
    FieldDef { name: "X1", kind: FieldType::Int16 },
    FieldDef { name: "Y1", kind: FieldType::Int16 },
    FieldDef { name: "Z1", kind: FieldType::Int16 },
    FieldDef { name: "X2", kind: FieldType::Int16 },
    FieldDef { name: "Y2", kind: FieldType::Int16 },
    FieldDef { name: "Z2", kind: FieldType::Int16 },
];

/// OBND — object bounding box.
pub static OBND_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"OBND"),
    name: "Object Bounds",
    required: false,
    repeating: false,
    field: FieldType::Struct(&OBND_FIELDS),
};


/// KSIZ — keyword count (u32).
pub static KSIZ_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"KSIZ"),
    name: "Keyword Count",
    required: false,
    repeating: false,
    field: FieldType::UInt32,
};

static KWDA_ELEMENT: FieldType = FieldType::FormId;

/// KWDA — keyword FormID array (count from preceding KSIZ).
pub static KWDA_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"KWDA"),
    name: "Keywords",
    required: false,
    repeating: false,
    field: FieldType::Array {
        element: &KWDA_ELEMENT,
        count: ArrayCount::PrecedingSibling(Signature(*b"KSIZ")),
    },
};


/// VMAD — Papyrus virtual machine adapter (raw byte array).
pub static VMAD_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"VMAD"),
    name: "Virtual Machine Adapter",
    required: false,
    repeating: false,
    field: FieldType::ByteArray,
};


/// YNAM — pickup sound FormID.
pub static YNAM_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"YNAM"),
    name: "Sound - Pick Up",
    required: false,
    repeating: false,
    field: FieldType::FormId,
};

/// ZNAM — putdown sound FormID.
pub static ZNAM_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"ZNAM"),
    name: "Sound - Put Down",
    required: false,
    repeating: false,
    field: FieldType::FormId,
};


/// CTDA — condition (raw byte array; complex sub-structure).
pub static CTDA_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"CTDA"),
    name: "Condition",
    required: false,
    repeating: true,
    field: FieldType::ByteArray,
};


/// EFID — base effect FormID (references MGEF).
pub static EFID_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"EFID"),
    name: "Base Effect",
    required: true,
    repeating: true,
    field: FieldType::FormId,
};

static EFIT_FIELDS: [FieldDef; 3] = [
    FieldDef { name: "Magnitude", kind: FieldType::Float32 },
    FieldDef { name: "Area of Effect", kind: FieldType::UInt32 },
    FieldDef { name: "Duration", kind: FieldType::UInt32 },
];

/// EFIT — effect data (magnitude, area, duration).
pub static EFIT_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"EFIT"),
    name: "Effect Data",
    required: true,
    repeating: true,
    field: FieldType::Struct(&EFIT_FIELDS),
};


/// EITM — enchantment / object effect FormID.
pub static EITM_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"EITM"),
    name: "Object Effect",
    required: false,
    repeating: false,
    field: FieldType::FormId,
};


/// RNAM — race FormID.
pub static RNAM_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"RNAM"),
    name: "Race",
    required: false,
    repeating: false,
    field: FieldType::FormId,
};

/// SPCT — spell count (u32).
pub static SPCT_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"SPCT"),
    name: "Spell Count",
    required: false,
    repeating: false,
    field: FieldType::UInt32,
};

static SPLO_ELEMENT: FieldType = FieldType::FormId;

/// SPLO — actor spell / effect FormID (repeating).
pub static SPLO_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"SPLO"),
    name: "Actor Effect",
    required: false,
    repeating: true,
    field: FieldType::Array {
        element: &SPLO_ELEMENT,
        count: ArrayCount::Remainder,
    },
};

/// ETYP — equipment type FormID.
pub static ETYP_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"ETYP"),
    name: "Equipment Type",
    required: false,
    repeating: false,
    field: FieldType::FormId,
};


/// MOD2 — 1st person model path.
pub static MOD2_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"MOD2"),
    name: "1st Person Model Filename",
    required: false,
    repeating: false,
    field: FieldType::ZString,
};


/// DEST — destruction data (raw byte array).
pub static DEST_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"DEST"),
    name: "Destruction Data",
    required: false,
    repeating: false,
    field: FieldType::ByteArray,
};
