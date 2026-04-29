// SPDX-License-Identifier: Apache-2.0
//!
//! Oblivion quest and dialogue record schemas.
//!
//! Covers QUST, DIAL, INFO, PLYR.

use crate::schema::{FieldDef, FieldType, RecordSchema, SubRecordDef};
use crate::types::Signature;

use super::common::{CTDA_DEF, EDID_DEF, FULL_DEF, SCRI_DEF};
use super::enums::OBLIVION_DIALOGUE_TYPE_ENUM;


static QUST_DATA_FIELDS: [FieldDef; 3] = [
    FieldDef { name: "Flags", kind: FieldType::UInt8 },
    FieldDef { name: "Priority", kind: FieldType::UInt8 },
    FieldDef { name: "_padding", kind: FieldType::ByteArray },
];

static QUST_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    SCRI_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "General",
        required: true,
        repeating: false,
        field: FieldType::Struct(&QUST_DATA_FIELDS),
    },
    CTDA_DEF,
];

/// QUST — quest.
pub static QUST_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"QUST"), name: "Quest", members: &QUST_MEMBERS };


static DIAL_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Dialogue Type",
        required: true,
        repeating: false,
        field: FieldType::Enum(&OBLIVION_DIALOGUE_TYPE_ENUM),
    },
];

/// DIAL — dialogue topic.
pub static DIAL_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"DIAL"), name: "Dialog Topic", members: &DIAL_MEMBERS };


static INFO_DATA_FIELDS: [FieldDef; 3] = [
    FieldDef { name: "Type", kind: FieldType::UInt8 },
    FieldDef { name: "Next Speaker", kind: FieldType::UInt8 },
    FieldDef { name: "Flags", kind: FieldType::UInt8 },
];

static INFO_MEMBERS: [SubRecordDef; 6] = [
    SubRecordDef {
        sig: Signature(*b"INAM"),
        name: "Previous Info",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Info Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&INFO_DATA_FIELDS),
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
        sig: Signature(*b"NAME"),
        name: "Topics",
        required: false,
        repeating: true,
        field: FieldType::FormId,
    },
    CTDA_DEF,
];

/// INFO — dialogue response record.
pub static INFO_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"INFO"),
    name: "Dialog Response",
    members: &INFO_MEMBERS,
};


/// PLYR — player reference (singleton; no subrecords).
pub static PLYR_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"PLYR"), name: "Player Reference", members: &[] };

// Suppress lint for enum used in FieldType position.
const _: () = {
    let _: &[FieldDef] = &[];
    let _ = &OBLIVION_DIALOGUE_TYPE_ENUM;
};
