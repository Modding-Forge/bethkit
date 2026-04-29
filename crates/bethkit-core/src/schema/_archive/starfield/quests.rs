// SPDX-License-Identifier: Apache-2.0
//!
//! Starfield quest and dialogue record schemas.
//!
//! Covers QUST, DIAL, INFO, DLBR, DLVW, SCEN (quest scenes), SPCH
//! (speech challenge), TERM-related UI, GPOF, GPOG (gameplay options),
//! TMLM (terminal menu).

use crate::schema::{FieldType, RecordSchema, SubRecordDef};
use crate::types::Signature;

use super::common::{CTDA_DEF, EDID_DEF, FULL_DEF, VMAD_DEF};
use super::enums::SF_QUEST_TYPE_ENUM;


static QUST_MEMBERS: [SubRecordDef; 8] = [
    EDID_DEF,
    VMAD_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Quest Data",
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
        sig: Signature(*b"QSTN"),
        name: "Stage Name",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"QTGL"),
        name: "Quest Type",
        required: false,
        repeating: false,
        field: FieldType::Enum(&SF_QUEST_TYPE_ENUM),
    },
];

/// QUST — quest.
pub static QUST_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"QUST"), name: "Quest", members: &QUST_MEMBERS };


static DIAL_MEMBERS: [SubRecordDef; 4] = [
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
        sig: Signature(*b"DATA"),
        name: "Topic Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// DIAL — dialog topic.
pub static DIAL_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"DIAL"), name: "Dialog Topic", members: &DIAL_MEMBERS };


static INFO_MEMBERS: [SubRecordDef; 6] = [
    EDID_DEF,
    VMAD_DEF,
    SubRecordDef {
        sig: Signature(*b"ENAM"),
        name: "Responses Flag",
        required: false,
        repeating: false,
        field: FieldType::UInt16,
    },
    CTDA_DEF,
    SubRecordDef {
        sig: Signature(*b"TRDT"),
        name: "Response Data",
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
];

/// INFO — dialog response.
pub static INFO_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"INFO"), name: "Dialog Response", members: &INFO_MEMBERS };


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
        name: "Start Dialogue",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
];

/// DLBR — dialog branch.
pub static DLBR_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"DLBR"), name: "Dialog Branch", members: &DLBR_MEMBERS };


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
        name: "Branch",
        required: false,
        repeating: true,
        field: FieldType::FormId,
    },
];

/// DLVW — dialog view (graph layout for dialog editor).
pub static DLVW_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"DLVW"), name: "Dialog View", members: &DLVW_MEMBERS };


static SPCH_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"SPWI"),
        name: "Win Info",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"SPLO"),
        name: "Loss Info",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"DIFF"),
        name: "Difficulty",
        required: false,
        repeating: false,
        field: FieldType::UInt8,
    },
    CTDA_DEF,
];

/// SPCH — speech challenge (Starfield persuasion minigame definition).
pub static SPCH_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"SPCH"), name: "Speech Challenge", members: &SPCH_MEMBERS };


static GPOF_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"NNAM"),
        name: "Name",
        required: false,
        repeating: false,
        field: FieldType::LString,
    },
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Default Value",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"TNAM"),
        name: "Type",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
    SubRecordDef {
        sig: Signature(*b"VNAM"),
        name: "Value Range",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// GPOF — gameplay option (individual accessibility / game setting).
pub static GPOF_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"GPOF"),
    name: "Gameplay Option",
    members: &GPOF_MEMBERS,
};


static GPOG_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"NNAM"),
        name: "Name",
        required: false,
        repeating: false,
        field: FieldType::LString,
    },
    SubRecordDef {
        sig: Signature(*b"BNAM"),
        name: "Option",
        required: false,
        repeating: true,
        field: FieldType::FormId,
    },
];

/// GPOG — gameplay options group (groups related GPOF entries).
pub static GPOG_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"GPOG"),
    name: "Gameplay Options Group",
    members: &GPOG_MEMBERS,
};


static TMLM_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Display Theme",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
    SubRecordDef {
        sig: Signature(*b"INAM"),
        name: "Menu Item",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
    VMAD_DEF,
];

/// TMLM — terminal menu (in-world terminal UI definition).
pub static TMLM_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"TMLM"), name: "Terminal Menu", members: &TMLM_MEMBERS };


static SOUN_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"SDSC"),
        name: "Sound Descriptor",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
];

/// SOUN — sound marker.
pub static SOUN_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"SOUN"), name: "Sound Marker", members: &SOUN_MEMBERS };
