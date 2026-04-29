// SPDX-License-Identifier: Apache-2.0
//! Morrowind world-object record schemas.
//!
//! Covers activators, doors, containers, and placed-object references.

use crate::schema::{FieldType, RecordSchema, SubRecordDef};
use crate::types::Signature;

use super::common::{
    DELE_DEF, FNAM_DEF, MODL_DEF, NAME_DEF, NPCO_DEF, SCRI_DEF, XSCL_DEF,
};


static ACTI_MEMBERS: [SubRecordDef; 5] = [
    NAME_DEF,
    DELE_DEF,
    MODL_DEF,
    FNAM_DEF,
    SCRI_DEF,
];

/// Schema for the `ACTI` activator record.
pub(super) static ACTI_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"ACTI"),
    name: "Activator",
    members: &ACTI_MEMBERS,
};


static DOOR_MEMBERS: [SubRecordDef; 7] = [
    NAME_DEF,
    DELE_DEF,
    MODL_DEF,
    FNAM_DEF,
    SCRI_DEF,
    SubRecordDef {
        sig: Signature(*b"SNAM"),
        name: "Open Sound",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"ANAM"),
        name: "Close Sound",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
];

/// Schema for the `DOOR` door record.
pub(super) static DOOR_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"DOOR"),
    name: "Door",
    members: &DOOR_MEMBERS,
};


static CONT_MEMBERS: [SubRecordDef; 8] = [
    NAME_DEF,
    DELE_DEF,
    MODL_DEF,
    FNAM_DEF,
    SubRecordDef {
        sig: Signature(*b"CNDT"),
        name: "Weight Capacity",
        required: true,
        repeating: false,
        field: FieldType::Float32,
    },
    SubRecordDef {
        sig: Signature(*b"FLAG"),
        name: "Container Flags",
        required: true,
        repeating: false,
        // bit0=Organic, bit1=Respawns, bit3=CanHoldItems
        field: FieldType::UInt32,
    },
    SCRI_DEF,
    NPCO_DEF,
];

/// Schema for the `CONT` container record.
pub(super) static CONT_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"CONT"),
    name: "Container",
    members: &CONT_MEMBERS,
};


static REFR_MEMBERS: [SubRecordDef; 20] = [
    SubRecordDef {
        sig: Signature(*b"CNDT"),
        name: "Cell Coordinates",
        required: false,
        repeating: false,
        // { cell_x: i32, cell_y: i32 } — used for cell transitions
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"FRMR"),
        name: "Object Index",
        required: true,
        repeating: false,
        // File-local u32 index (not a FormID — Morrowind predates FormIDs).
        field: FieldType::UInt32,
    },
    NAME_DEF,
    SubRecordDef {
        sig: Signature(*b"UNAM"),
        name: "Reference Blocked",
        required: false,
        repeating: false,
        field: FieldType::UInt8,
    },
    XSCL_DEF,
    SubRecordDef {
        sig: Signature(*b"ANAM"),
        name: "Owner",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"BNAM"),
        name: "Owner Global Variable",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"CNAM"),
        name: "Owner Faction",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"INDX"),
        name: "Faction Rank",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
    SubRecordDef {
        sig: Signature(*b"XCHG"),
        name: "Enchantment Charge",
        required: false,
        repeating: false,
        field: FieldType::Float32,
    },
    SubRecordDef {
        sig: Signature(*b"XSOL"),
        name: "Soul Trapped",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"INTV"),
        name: "Health Remaining",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
    SubRecordDef {
        sig: Signature(*b"NAM9"),
        name: "Item Count",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
    SubRecordDef {
        sig: Signature(*b"DODT"),
        name: "Teleport Destination",
        required: false,
        repeating: false,
        // 6× f32: position (XYZ) + rotation (XYZ)
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Teleport Cell",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"FLTV"),
        name: "Lock Level",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
    SubRecordDef {
        sig: Signature(*b"KNAM"),
        name: "Key",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"TNAM"),
        name: "Trap",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    DELE_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Position / Rotation",
        required: false,
        repeating: false,
        // 6× f32: position (XYZ) + rotation (XYZ)
        field: FieldType::ByteArray,
    },
];

/// Schema for the `REFR` placed-object reference record.
///
/// Morrowind references use a file-local `FRMR` index rather than persistent
/// FormIDs. The `FRMR` field is always the second subrecord.
pub(super) static REFR_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"REFR"),
    name: "Placed Object Reference",
    members: &REFR_MEMBERS,
};
