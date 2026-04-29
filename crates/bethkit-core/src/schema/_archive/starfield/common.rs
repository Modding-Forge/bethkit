// SPDX-License-Identifier: Apache-2.0
//!
//! Reusable static [`SubRecordDef`] helpers shared across many Starfield
//! record definitions.
//!
//! Universal definitions (EDID, MODL) are re-exported from
//! [`crate::schema::shared`]. All Starfield-specific or LString-variant
//! definitions are defined here.
//!
//! Starfield is fully localised, so FULL and DESC use [`FieldType::LString`].

use crate::schema::{ArrayCount, FieldDef, FieldType, SubRecordDef};
use crate::types::Signature;

pub use crate::schema::shared::{EDID_DEF, MODL_DEF};


/// FULL — full display name (localised).
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


/// VMAD — virtual machine adapter (Papyrus script attachments).
pub static VMAD_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"VMAD"),
    name: "Scripts",
    required: false,
    repeating: false,
    field: FieldType::ByteArray,
};


/// CTDA — condition data (repeating).
pub static CTDA_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"CTDA"),
    name: "Condition",
    required: false,
    repeating: true,
    field: FieldType::ByteArray,
};


/// RNAM — race FormID reference.
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

/// SPLO — spell FormID array (count from preceding SPCT).
pub static SPLO_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"SPLO"),
    name: "Spells",
    required: false,
    repeating: false,
    field: FieldType::Array {
        element: &SPLO_ELEMENT,
        count: ArrayCount::PrecedingSibling(Signature(*b"SPCT")),
    },
};


/// EFID — base effect FormID.
pub static EFID_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"EFID"),
    name: "Base Effect",
    required: false,
    repeating: false,
    field: FieldType::FormId,
};

static EFIT_FIELDS: [FieldDef; 3] = [
    FieldDef { name: "Magnitude", kind: FieldType::Float32 },
    FieldDef { name: "Area", kind: FieldType::UInt32 },
    FieldDef { name: "Duration", kind: FieldType::UInt32 },
];

/// EFIT — effect item data (magnitude / area / duration).
pub static EFIT_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"EFIT"),
    name: "Effect Data",
    required: false,
    repeating: false,
    field: FieldType::Struct(&EFIT_FIELDS),
};
