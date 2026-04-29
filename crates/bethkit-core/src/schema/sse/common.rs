// SPDX-License-Identifier: Apache-2.0
//!
//! Reusable static [`SubRecordDef`] helpers shared across many SSE record
//! definitions.
//!
//! These mirror xEdit's `wbEDID`, `wbFULL`, `wbOBND`, etc. helper
//! functions — each produces the canonical definition for its subrecord
//! type so that individual record files can reference them by name rather
//! than repeating the same definition.

use crate::schema::{ArrayCount, FieldDef, FieldType, SubRecordDef};
use crate::types::Signature;

// Re-export universal definitions so callers can use this module as a
// one-stop import.
pub use crate::schema::shared::{EDID_DEF, ICON_DEF, MICO_DEF, MODL_DEF};


/// FULL — Full display name.
///
/// May be a localised string-table ID (when the parent record has the
/// LOCALIZED flag) or an inline NUL-terminated string.
pub static FULL_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"FULL"),
    name: "Full Name",
    required: false,
    repeating: false,
    field: FieldType::LString,
};

/// DESC — Description text (localised).
pub static DESC_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"DESC"),
    name: "Description",
    required: false,
    repeating: false,
    field: FieldType::LString,
};

/// MOD2 — 1st person model path.
pub static MOD2_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"MOD2"),
    name: "1st Person Model Filename",
    required: false,
    repeating: false,
    field: FieldType::ZString,
};

/// MOD3 — Scope model path.
pub static MOD3_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"MOD3"),
    name: "3rd Person Model Filename",
    required: false,
    repeating: false,
    field: FieldType::ZString,
};

/// YNAM — Pickup sound reference.
pub static YNAM_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"YNAM"),
    name: "Sound - Pick Up",
    required: false,
    repeating: false,
    field: FieldType::FormIdTyped(&[Signature(*b"SNDR"), Signature(*b"SOUN")]),
};

/// ZNAM — Drop sound reference.
pub static ZNAM_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"ZNAM"),
    name: "Sound - Put Down",
    required: false,
    repeating: false,
    field: FieldType::ZString,
};

/// VMAD — Papyrus script data (raw byte array).
pub static VMAD_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"VMAD"),
    name: "Virtual Machine Adapter",
    required: false,
    repeating: false,
    field: FieldType::ByteArray,
};


/// OBND field layout: 6 signed 16-bit integers (X1,Y1,Z1, X2,Y2,Z2).
static OBND_FIELDS: [FieldDef; 6] = [
    FieldDef { name: "X1", kind: FieldType::Int16 },
    FieldDef { name: "Y1", kind: FieldType::Int16 },
    FieldDef { name: "Z1", kind: FieldType::Int16 },
    FieldDef { name: "X2", kind: FieldType::Int16 },
    FieldDef { name: "Y2", kind: FieldType::Int16 },
    FieldDef { name: "Z2", kind: FieldType::Int16 },
];

/// OBND — Object bounds (bounding box).
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

static KWDA_ELEMENT: FieldType = FieldType::FormIdTyped(&[Signature(*b"KYWD")]);

/// KWDA — keyword FormID array (count given by preceding KSIZ).
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


/// DEST — Destruction data (raw byte array; complex sub-structure).
pub static DEST_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"DEST"),
    name: "Destruction Data",
    required: false,
    repeating: false,
    field: FieldType::ByteArray,
};


/// EITM — Enchantment / Object Effect FormID.
pub static EITM_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"EITM"),
    name: "Object Effect",
    required: false,
    repeating: false,
    field: FieldType::FormIdTyped(&[Signature(*b"ENCH"), Signature(*b"SPEL")]),
};

/// EAMT — Enchantment amount (u16).
pub static EAMT_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"EAMT"),
    name: "Enchantment Amount",
    required: false,
    repeating: false,
    field: FieldType::UInt16,
};


/// EFID — Base Effect FormID (references MGEF).
pub static EFID_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"EFID"),
    name: "Base Effect",
    required: true,
    repeating: true,
    field: FieldType::FormIdTyped(&[Signature(*b"MGEF")]),
};

static EFIT_FIELDS: [FieldDef; 3] = [
    FieldDef { name: "Magnitude", kind: FieldType::Float32 },
    FieldDef { name: "Area of Effect", kind: FieldType::UInt32 },
    FieldDef { name: "Duration", kind: FieldType::UInt32 },
];

/// EFIT — Effect data (magnitude, area, duration).
pub static EFIT_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"EFIT"),
    name: "Effect Data",
    required: true,
    repeating: true,
    field: FieldType::Struct(&EFIT_FIELDS),
};

/// CTDA — Condition (raw byte array; complex sub-structure).
pub static CTDA_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"CTDA"),
    name: "Condition",
    required: false,
    repeating: true,
    field: FieldType::ByteArray,
};


/// ETYP — Equipment type FormID.
pub static ETYP_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"ETYP"),
    name: "Equipment Type",
    required: false,
    repeating: false,
    field: FieldType::FormIdTyped(&[Signature(*b"EQUP")]),
};


/// PRPS — Properties array (raw byte array).
// NOTE: Reserved for future script-property schema expansion.
#[allow(dead_code)]
pub static PRPS_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"PRPS"),
    name: "Properties",
    required: false,
    repeating: false,
    field: FieldType::ByteArray,
};


/// RNAM — Race FormID.
pub static RNAM_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"RNAM"),
    name: "Race",
    required: false,
    repeating: false,
    field: FieldType::FormIdTyped(&[Signature(*b"RACE")]),
};


/// SPCT — Spell count (u32).
pub static SPCT_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"SPCT"),
    name: "Spell Count",
    required: false,
    repeating: false,
    field: FieldType::UInt32,
};

static SPLO_ELEMENT: FieldType =
    FieldType::FormIdTyped(&[Signature(*b"SPEL"), Signature(*b"LVSP")]);

/// SPLO — Spell FormID (one entry per spell; repeating).
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
