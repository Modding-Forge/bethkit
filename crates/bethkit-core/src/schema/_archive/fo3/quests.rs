// SPDX-License-Identifier: Apache-2.0
//!
//! Fallout 3 quest, dialogue, and audio record schemas.

use crate::schema::{RecordSchema, SubRecordDef, FieldType};
use crate::types::Signature;

use super::common::{CTDA_DEF, DATA_DEF, DESC_DEF, EDID_DEF, FULL_DEF};


static QUST_MEMBERS: [SubRecordDef; 8] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"SCRI"),
        name: "Script",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"ICON"),
        name: "Icon",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    DATA_DEF,
    CTDA_DEF,
    SubRecordDef {
        sig: Signature(*b"QSTA"),
        name: "Quest Stage",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"QOBJ"),
        name: "Quest Objective",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
];

/// QUST — quest definition.
pub static QUST_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"QUST"), name: "Quest", members: &QUST_MEMBERS };


static DIAL_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"QSTI"),
        name: "Quest",
        required: false,
        repeating: true,
        field: FieldType::FormId,
    },
    FULL_DEF,
    DATA_DEF,
];

/// DIAL — dialogue topic.
pub static DIAL_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"DIAL"), name: "Dialog Topic", members: &DIAL_MEMBERS };


static INFO_MEMBERS: [SubRecordDef; 7] = [
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Dialog Type / Flags",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"QSTI"),
        name: "Quest",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"TPIC"),
        name: "Topic",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"PNAM"),
        name: "Previous Info",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    CTDA_DEF,
    SubRecordDef {
        sig: Signature(*b"NAM1"),
        name: "Response Text",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"SCRI"),
        name: "Result Script",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// INFO — individual dialogue response / line.
pub static INFO_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"INFO"), name: "Dialog Response", members: &INFO_MEMBERS };


static SOUN_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"OBND"),
        name: "Object Bounds",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"FNAM"),
        name: "Sound Filename",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"SNDD"),
        name: "Sound Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// SOUN — sound (audio file reference with playback parameters).
pub static SOUN_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"SOUN"), name: "Sound", members: &SOUN_MEMBERS };

// NOTE: DESC_DEF is re-exported from common but not used in this module.
const _: () = { let _ = &DESC_DEF; };
