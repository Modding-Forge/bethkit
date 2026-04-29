// SPDX-License-Identifier: Apache-2.0
//! Morrowind quest and dialogue record schemas: scripts, dialog topics,
//! dialog responses, and quests.

use crate::schema::{FieldDef, FieldType, RecordSchema, SubRecordDef};
use crate::types::Signature;

use super::common::{DELE_DEF, FNAM_DEF, NAME_DEF};


static SCPT_MEMBERS: [SubRecordDef; 5] = [
    SubRecordDef {
        sig: Signature(*b"SCHD"),
        name: "Script Header",
        required: true,
        repeating: false,
        // 52 bytes: name([u8;32]), num_shorts(u32), num_longs(u32),
        //   num_floats(u32), compiled_size(u32), local_var_size(u32)
        // NOTE: The EditorID comes from SCHD.name, not a separate NAME subrecord.
        field: FieldType::ByteArray,
    },
    DELE_DEF,
    SubRecordDef {
        sig: Signature(*b"SCVR"),
        name: "Variable Names",
        required: false,
        repeating: false,
        // Concatenated null-terminated variable name strings.
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"SCDT"),
        name: "Compiled Bytecode",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"SCTX"),
        name: "Script Source",
        required: true,
        repeating: false,
        field: FieldType::ZString,
    },
];

/// Schema for the `SCPT` script record.
///
/// The EditorID is embedded inside the `SCHD` binary header at offset 0
/// (a 32-byte null-padded string), not in a separate `NAME` subrecord.
pub(super) static SCPT_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"SCPT"),
    name: "Script",
    members: &SCPT_MEMBERS,
};


static DIAL_DATA_FIELDS: [FieldDef; 2] = [
    FieldDef { name: "Dialog Type", kind: FieldType::UInt8 },
    FieldDef { name: "_unused",     kind: FieldType::Unused(3) },
];

static DIAL_MEMBERS: [SubRecordDef; 3] = [
    NAME_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Dialog Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&DIAL_DATA_FIELDS),
    },
    DELE_DEF,
];

/// Schema for the `DIAL` dialog topic record.
pub(super) static DIAL_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"DIAL"),
    name: "Dialog Topic",
    members: &DIAL_MEMBERS,
};


static INFO_MEMBERS: [SubRecordDef; 20] = [
    SubRecordDef {
        sig: Signature(*b"INAM"),
        name: "Response ID",
        required: true,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"PNAM"),
        name: "Previous Response ID",
        required: true,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"NNAM"),
        name: "Next Response ID",
        required: true,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Info Data",
        required: true,
        repeating: false,
        // 8 bytes: dialog_type(u32), disposition_or_index(u32)
        //   + packed bytes: faction_rank(i8), sex(i8), player_rank(i8), _unused(u8)
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"ONAM"),
        name: "Speaker",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"RNAM"),
        name: "Speaker Race",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"CNAM"),
        name: "Speaker Class",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    FNAM_DEF,
    SubRecordDef {
        sig: Signature(*b"ANAM"),
        name: "Speaker Cell",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Player Faction",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"SNAM"),
        name: "Sound Filename",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    NAME_DEF,
    DELE_DEF,
    SubRecordDef {
        sig: Signature(*b"SCVR"),
        name: "Condition",
        required: false,
        repeating: true,
        // Variable-length binary condition struct (see TES3 reference).
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"INTV"),
        name: "Condition Value (Int)",
        required: false,
        repeating: true,
        field: FieldType::Int32,
    },
    SubRecordDef {
        sig: Signature(*b"FLTV"),
        name: "Condition Value (Float)",
        required: false,
        repeating: true,
        field: FieldType::Float32,
    },
    SubRecordDef {
        sig: Signature(*b"QSTN"),
        name: "Quest Named",
        required: false,
        repeating: false,
        field: FieldType::UInt8,
    },
    SubRecordDef {
        sig: Signature(*b"QSTF"),
        name: "Quest Finished",
        required: false,
        repeating: false,
        field: FieldType::UInt8,
    },
    SubRecordDef {
        sig: Signature(*b"QSTR"),
        name: "Quest Restarted",
        required: false,
        repeating: false,
        field: FieldType::UInt8,
    },
    SubRecordDef {
        sig: Signature(*b"BNAM"),
        name: "Result Script",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
];

/// Schema for the `INFO` dialog response record.
pub(super) static INFO_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"INFO"),
    name: "Dialog Response",
    members: &INFO_MEMBERS,
};
