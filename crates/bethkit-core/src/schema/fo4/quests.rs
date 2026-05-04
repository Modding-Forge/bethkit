// SPDX-License-Identifier: Apache-2.0
//!
//! Fallout 4 quest and dialogue record schemas.
//!
//! Covers QUST, DIAL, INFO, DLBR, DLVW, SCEN, SMBN, SMQN, SMEN.

use crate::schema::{FieldDef, FieldType, RecordSchema, SubRecordDef};
use crate::types::Signature;

use super::common::{CTDA_DEF, EDID_DEF, FULL_DEF, VMAD_DEF};
use super::enums::FO4_QUEST_TYPE_ENUM;

static QUST_MEMBERS: [SubRecordDef; 7] = [
    EDID_DEF,
    VMAD_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "General",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"FLTR"),
        name: "Object Window Filter",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    CTDA_DEF,
    SubRecordDef {
        sig: Signature(*b"QTGL"),
        name: "Quest Log",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
];

/// QUST — quest.
pub static QUST_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"QUST"),
    name: "Quest",
    members: &QUST_MEMBERS,
};

static DIAL_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"PNAM"),
        name: "Priority",
        required: false,
        repeating: false,
        field: FieldType::Float32,
    },
    SubRecordDef {
        sig: Signature(*b"QNAM"),
        name: "Quest",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// DIAL — dialogue topic.
pub static DIAL_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"DIAL"),
    name: "Dialogue Topic",
    members: &DIAL_MEMBERS,
};

static INFO_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    VMAD_DEF,
    SubRecordDef {
        sig: Signature(*b"ENAM"),
        name: "Responses Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    CTDA_DEF,
    SubRecordDef {
        sig: Signature(*b"NAM0"),
        name: "Response Text",
        required: false,
        repeating: true,
        field: FieldType::LString,
    },
];

/// INFO — dialogue response record.
pub static INFO_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"INFO"),
    name: "Dialogue Response",
    members: &INFO_MEMBERS,
};

static DLBR_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"QNAM"),
        name: "Quest",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Flags",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
    SubRecordDef {
        sig: Signature(*b"SNAM"),
        name: "Starting Topic",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
];

/// DLBR — dialogue branch.
pub static DLBR_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"DLBR"),
    name: "Dialogue Branch",
    members: &DLBR_MEMBERS,
};

static DLVW_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"QNAM"),
        name: "Quest",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"BNAM"),
        name: "Branches",
        required: false,
        repeating: true,
        field: FieldType::FormId,
    },
];

/// DLVW — dialogue view.
pub static DLVW_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"DLVW"),
    name: "Dialogue View",
    members: &DLVW_MEMBERS,
};

static SCEN_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    VMAD_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"PNAM"),
        name: "Flags",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
    CTDA_DEF,
];

/// SCEN — scene.
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
        field: FieldType::FormId,
    },
    CTDA_DEF,
];

/// SMBN — story manager branch node.
pub static SMBN_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"SMBN"),
    name: "Story Manager Branch Node",
    members: &SMBN_MEMBERS,
};

static SMQN_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"PNAM"),
        name: "Parent",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    CTDA_DEF,
    SubRecordDef {
        sig: Signature(*b"QNAM"),
        name: "Quests",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
];

/// SMQN — story manager quest node.
pub static SMQN_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"SMQN"),
    name: "Story Manager Quest Node",
    members: &SMQN_MEMBERS,
};

static SMEN_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"PNAM"),
        name: "Parent",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"ENAM"),
        name: "Event",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
];

/// SMEN — story manager event node.
pub static SMEN_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"SMEN"),
    name: "Story Manager Event Node",
    members: &SMEN_MEMBERS,
};

// Suppress lint for enum referenced only via type annotation.
const _: () = {
    let _ = &FO4_QUEST_TYPE_ENUM;
    let _: &[FieldDef] = &[];
};
