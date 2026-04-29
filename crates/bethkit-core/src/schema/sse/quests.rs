// SPDX-License-Identifier: Apache-2.0
//!
//! Schema definitions for quest / dialogue SSE record types.
//!
//! Covers: QUST, DIAL, INFO, DLBR, DLVW, SCEN, SMBN, SMQN, SMEN.

use crate::schema::{FieldDef, FieldType, RecordSchema, SubRecordDef};
use crate::types::Signature;

use super::common::{CTDA_DEF, EDID_DEF, FULL_DEF, MODL_DEF, VMAD_DEF};
use crate::schema::enums::{QUEST_FLAGS, QUEST_TYPE_ENUM};


static QUST_DNAM_FIELDS: [FieldDef; 6] = [
    FieldDef { name: "Flags", kind: FieldType::Flags(&QUEST_FLAGS) },
    FieldDef { name: "Priority", kind: FieldType::UInt8 },
    FieldDef { name: "Unknown", kind: FieldType::UInt8 },
    FieldDef { name: "Unknown2", kind: FieldType::UInt8 },
    FieldDef { name: "Quest Type", kind: FieldType::Enum(&QUEST_TYPE_ENUM) },
    FieldDef { name: "Unused", kind: FieldType::UInt8 },
];

static QUST_MEMBERS: [SubRecordDef; 8] = [
    EDID_DEF,
    VMAD_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Quest Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&QUST_DNAM_FIELDS),
    },
    CTDA_DEF,
    SubRecordDef {
        sig: Signature(*b"QOBJ"),
        name: "Objective Index",
        required: false,
        repeating: true,
        field: FieldType::UInt16,
    },
    SubRecordDef {
        sig: Signature(*b"NNAM"),
        name: "Next Quest",
        required: false,
        repeating: false,
        field: FieldType::FormIdTyped(&[Signature(*b"QUST")]),
    },
    SubRecordDef {
        sig: Signature(*b"INDX"),
        name: "Stage Index",
        required: false,
        repeating: true,
        field: FieldType::UInt16,
    },
];

/// QUST — Quest.
pub static QUST_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"QUST"),
    name: "Quest",
    members: &QUST_MEMBERS,
};


static DIAL_DATA_FIELDS: [FieldDef; 4] = [
    FieldDef { name: "Priority", kind: FieldType::Float32 },
    FieldDef { name: "Unknown", kind: FieldType::Float32 },
    FieldDef { name: "Category", kind: FieldType::UInt8 },
    FieldDef { name: "Subtype", kind: FieldType::UInt8 },
];

static DIAL_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"QNAM"),
        name: "Quest",
        required: false,
        repeating: false,
        field: FieldType::FormIdTyped(&[Signature(*b"QUST")]),
    },
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&DIAL_DATA_FIELDS),
    },
];

/// DIAL — Dialogue topic.
pub static DIAL_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"DIAL"),
    name: "Dialogue Topic",
    members: &DIAL_MEMBERS,
};


static INFO_ENAM_FIELDS: [FieldDef; 3] = [
    FieldDef { name: "Flags", kind: FieldType::UInt16 },
    FieldDef { name: "Reset Hours", kind: FieldType::UInt8 },
    FieldDef { name: "Unknown", kind: FieldType::UInt8 },
];

static INFO_MEMBERS: [SubRecordDef; 8] = [
    EDID_DEF,
    VMAD_DEF,
    SubRecordDef {
        sig: Signature(*b"ENAM"),
        name: "Flags",
        required: true,
        repeating: false,
        field: FieldType::Struct(&INFO_ENAM_FIELDS),
    },
    CTDA_DEF,
    SubRecordDef {
        sig: Signature(*b"TRDA"),
        name: "Topic Reference",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"NAM1"),
        name: "Response Text",
        required: false,
        repeating: true,
        field: FieldType::LString,
    },
    SubRecordDef {
        sig: Signature(*b"NAM2"),
        name: "Script Notes",
        required: false,
        repeating: true,
        field: FieldType::LString,
    },
    SubRecordDef {
        sig: Signature(*b"PNAM"),
        name: "Previous Info",
        required: false,
        repeating: false,
        field: FieldType::FormIdTyped(&[Signature(*b"INFO")]),
    },
];

/// INFO — Dialogue response.
pub static INFO_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"INFO"),
    name: "Info",
    members: &INFO_MEMBERS,
};


static DLBR_DATA_FIELDS: [FieldDef; 2] = [
    FieldDef { name: "Unknown", kind: FieldType::UInt32 },
    FieldDef { name: "Flags", kind: FieldType::UInt32 },
];

static DLBR_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"QNAM"),
        name: "Quest",
        required: true,
        repeating: false,
        field: FieldType::FormIdTyped(&[Signature(*b"QUST")]),
    },
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&DLBR_DATA_FIELDS),
    },
    CTDA_DEF,
    SubRecordDef {
        sig: Signature(*b"SNAM"),
        name: "Starting Topic",
        required: false,
        repeating: false,
        field: FieldType::FormIdTyped(&[Signature(*b"DIAL")]),
    },
];

/// DLBR — Dialogue branch.
pub static DLBR_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"DLBR"),
    name: "Dialogue Branch",
    members: &DLBR_MEMBERS,
};


static DLVW_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"QNAM"),
        name: "Quest",
        required: false,
        repeating: false,
        field: FieldType::FormIdTyped(&[Signature(*b"QUST")]),
    },
    SubRecordDef {
        sig: Signature(*b"BNAM"),
        name: "Branches",
        required: false,
        repeating: true,
        field: FieldType::FormIdTyped(&[Signature(*b"DLBR")]),
    },
    SubRecordDef {
        sig: Signature(*b"TNAM"),
        name: "Topics",
        required: false,
        repeating: true,
        field: FieldType::FormIdTyped(&[Signature(*b"DIAL")]),
    },
    SubRecordDef {
        sig: Signature(*b"ENAM"),
        name: "Flags",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
];

/// DLVW — Dialogue view (editor layout metadata).
pub static DLVW_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"DLVW"),
    name: "Dialogue View",
    members: &DLVW_MEMBERS,
};


static SCEN_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    VMAD_DEF,
    SubRecordDef {
        sig: Signature(*b"FNAM"),
        name: "Flags",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
    CTDA_DEF,
    SubRecordDef {
        sig: Signature(*b"PNAM"),
        name: "Phase Marker",
        required: false,
        repeating: true,
        field: FieldType::UInt32,
    },
];

/// SCEN — Scene.
pub static SCEN_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"SCEN"),
    name: "Scene",
    members: &SCEN_MEMBERS,
};


static SMBN_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"PNAM"),
        name: "Parent",
        required: false,
        repeating: false,
        field: FieldType::FormIdTyped(&[Signature(*b"SMQN"), Signature(*b"SMBN"), Signature(*b"SMEN")]),
    },
    SubRecordDef {
        sig: Signature(*b"SNAM"),
        name: "Child",
        required: false,
        repeating: true,
        field: FieldType::FormIdTyped(&[Signature(*b"SMQN"), Signature(*b"SMBN"), Signature(*b"SMEN")]),
    },
];

/// SMBN — Story manager branch node.
pub static SMBN_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"SMBN"),
    name: "Story Manager Branch Node",
    members: &SMBN_MEMBERS,
};


static SMQN_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"PNAM"),
        name: "Parent",
        required: false,
        repeating: false,
        field: FieldType::FormIdTyped(&[Signature(*b"SMQN"), Signature(*b"SMBN"), Signature(*b"SMEN")]),
    },
    SubRecordDef {
        sig: Signature(*b"SNAM"),
        name: "Child",
        required: false,
        repeating: true,
        field: FieldType::FormIdTyped(&[Signature(*b"SMQN"), Signature(*b"SMBN"), Signature(*b"SMEN")]),
    },
    CTDA_DEF,
    SubRecordDef {
        sig: Signature(*b"QNAM"),
        name: "Quest",
        required: false,
        repeating: false,
        field: FieldType::FormIdTyped(&[Signature(*b"QUST")]),
    },
];

/// SMQN — Story manager quest node.
pub static SMQN_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"SMQN"),
    name: "Story Manager Quest Node",
    members: &SMQN_MEMBERS,
};


static SMEN_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"PNAM"),
        name: "Parent",
        required: false,
        repeating: false,
        field: FieldType::FormIdTyped(&[Signature(*b"SMQN"), Signature(*b"SMBN"), Signature(*b"SMEN")]),
    },
    SubRecordDef {
        sig: Signature(*b"SNAM"),
        name: "Child",
        required: false,
        repeating: true,
        field: FieldType::FormIdTyped(&[Signature(*b"SMQN"), Signature(*b"SMBN"), Signature(*b"SMEN")]),
    },
    CTDA_DEF,
];

/// SMEN — Story manager event node.
pub static SMEN_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"SMEN"),
    name: "Story Manager Event Node",
    members: &SMEN_MEMBERS,
};

// Suppress dead-code warnings for imports used only transitively.
const _: () = {
    let _ = &MODL_DEF;
};
