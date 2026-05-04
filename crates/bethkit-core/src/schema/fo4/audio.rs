// SPDX-License-Identifier: Apache-2.0
//!
//! Fallout 4 audio record schemas.
//!
//! Covers SOUN, SNDR, MUSC, MUST, SNCT, SOPM, FSTP, FSTS, ARTO, MATO,
//! and the FO4-specific AECH (Audio Effect Chain).

use crate::schema::{FieldType, RecordSchema, SubRecordDef};
use crate::types::Signature;

use super::common::EDID_DEF;

static SOUN_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"SDSC"),
        name: "Sound Descriptor",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
];

/// SOUN — sound marker.
pub static SOUN_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"SOUN"),
    name: "Sound Marker",
    members: &SOUN_MEMBERS,
};

static SNDR_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"CNAM"),
        name: "Category",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"GNAM"),
        name: "Sound Category",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"BNAM"),
        name: "Audio Output Override",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
];

/// SNDR — sound descriptor.
pub static SNDR_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"SNDR"),
    name: "Sound Descriptor",
    members: &SNDR_MEMBERS,
};

static MUSC_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"FNAM"),
        name: "Flags",
        required: false,
        repeating: false,
        field: FieldType::UInt16,
    },
    SubRecordDef {
        sig: Signature(*b"PNAM"),
        name: "Priority",
        required: false,
        repeating: false,
        field: FieldType::UInt16,
    },
];

/// MUSC — music type.
pub static MUSC_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"MUSC"),
    name: "Music Type",
    members: &MUSC_MEMBERS,
};

static MUST_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"CNAM"),
        name: "Track Type",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
    SubRecordDef {
        sig: Signature(*b"ANAM"),
        name: "File",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
];

/// MUST — music track.
pub static MUST_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"MUST"),
    name: "Music Track",
    members: &MUST_MEMBERS,
};

static SNCT_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"FULL"),
        name: "Full Name",
        required: false,
        repeating: false,
        field: FieldType::LString,
    },
    SubRecordDef {
        sig: Signature(*b"PNAM"),
        name: "Parent",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"FNAM"),
        name: "Flags",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
];

/// SNCT — sound category.
pub static SNCT_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"SNCT"),
    name: "Sound Category",
    members: &SNCT_MEMBERS,
};

static SOPM_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"NAM1"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// SOPM — sound output model.
pub static SOPM_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"SOPM"),
    name: "Sound Output Model",
    members: &SOPM_MEMBERS,
};

static FSTP_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Impact Data Set",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"ANAM"),
        name: "Tag",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
];

/// FSTP — footstep.
pub static FSTP_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"FSTP"),
    name: "Footstep",
    members: &FSTP_MEMBERS,
};

static FSTS_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"XCNT"),
        name: "Count",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
];

/// FSTS — footstep set.
pub static FSTS_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"FSTS"),
    name: "Footstep Set",
    members: &FSTS_MEMBERS,
};

static ARTO_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"MODL"),
        name: "Model",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Type",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
];

/// ARTO — art object.
pub static ARTO_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"ARTO"),
    name: "Art Object",
    members: &ARTO_MEMBERS,
};

static MATO_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"MODL"),
        name: "Model",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Property Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// MATO — material object.
pub static MATO_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"MATO"),
    name: "Material Object",
    members: &MATO_MEMBERS,
};

static AECH_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"KSIZ"),
        name: "Effect Count",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
];

/// AECH — audio effect chain (Fallout 4 specific; 7.1-channel audio effect
/// chain used by sound output models).
pub static AECH_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"AECH"),
    name: "Audio Effect Chain",
    members: &AECH_MEMBERS,
};
