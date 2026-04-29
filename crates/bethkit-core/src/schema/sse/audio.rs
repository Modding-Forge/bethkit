// SPDX-License-Identifier: Apache-2.0
//!
//! Schema definitions for audio / sound SSE record types.
//!
//! Covers: SOUN, SNDR, MUSC, MUST, SNCT, SOPM, FSTP, FSTS, ARTO, MATO.

use crate::schema::{FieldDef, FieldType, RecordSchema, SubRecordDef};
use crate::types::Signature;

use super::common::{EDID_DEF, FULL_DEF, MODL_DEF, OBND_DEF};
use crate::schema::enums::SOUND_LEVEL_ENUM;


static SOUN_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    OBND_DEF,
    SubRecordDef {
        sig: Signature(*b"SDSC"),
        name: "Sound Descriptor",
        required: true,
        repeating: false,
        field: FieldType::FormIdTyped(&[Signature(*b"SNDR")]),
    },
];

/// SOUN — Sound marker (references a sound descriptor).
pub static SOUN_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"SOUN"),
    name: "Sound Marker",
    members: &SOUN_MEMBERS,
};


static SNDR_MEMBERS: [SubRecordDef; 8] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"CNAM"),
        name: "Category",
        required: false,
        repeating: false,
        field: FieldType::FormIdTyped(&[Signature(*b"SNCT")]),
    },
    SubRecordDef {
        sig: Signature(*b"SNAM"),
        name: "Alternate Template",
        required: false,
        repeating: false,
        field: FieldType::FormIdTyped(&[Signature(*b"SNDR")]),
    },
    SubRecordDef {
        sig: Signature(*b"ITMC"),
        name: "Item Count",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
    SubRecordDef {
        sig: Signature(*b"ITME"),
        name: "Sound File",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"BNAM"),
        // NOTE: Not a FormID — 6-byte struct: freq shift (i8), freq variance (i8),
        // NOTE: priority (u8), db variance (u8), static attenuation/100 (u16).
        name: "Frequency/Priority Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"GNAM"),
        name: "Output Model",
        required: false,
        repeating: false,
        field: FieldType::FormIdTyped(&[Signature(*b"SOPM")]),
    },
    SubRecordDef {
        sig: Signature(*b"LNAM"),
        // NOTE: Not a FormID — 4-byte struct: unknown (u8), looping enum (u8),
        // NOTE: unknown (u8), rumble send value (u8).
        name: "Loop Info",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// SNDR — Sound descriptor (defines a sound asset).
pub static SNDR_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"SNDR"),
    name: "Sound Descriptor",
    members: &SNDR_MEMBERS,
};


static MUSC_DATA_FIELDS: [FieldDef; 3] = [
    FieldDef { name: "Flags", kind: FieldType::UInt16 },
    FieldDef { name: "Priority", kind: FieldType::UInt16 },
    FieldDef { name: "Duck Decibels", kind: FieldType::Float32 },
];

static MUSC_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&MUSC_DATA_FIELDS),
    },
];

/// MUSC — Music type.
pub static MUSC_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"MUSC"),
    name: "Music Type",
    members: &MUSC_MEMBERS,
};


static MUST_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"CNAM"),
        // NOTE: Not a FormID — UInt32 track-type hash.
        // NOTE: Known values: 0x23F678C3 = Palette, 0x6ED7E048 = Single Track,
        // NOTE: 0xA1A9C4D5 = Silent Track.
        name: "Track Type",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
    SubRecordDef {
        sig: Signature(*b"FNAM"),
        name: "File Name",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// MUST — Music track.
pub static MUST_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"MUST"),
    name: "Music Track",
    members: &MUST_MEMBERS,
};


static SNCT_DATA_FIELDS: [FieldDef; 3] = [
    FieldDef { name: "Flags", kind: FieldType::UInt32 },
    FieldDef { name: "Default Menu Volume", kind: FieldType::Float32 },
    FieldDef { name: "Default Menu Frequency", kind: FieldType::Float32 },
];

static SNCT_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"PNAM"),
        name: "Parent Category",
        required: false,
        repeating: false,
        field: FieldType::FormIdTyped(&[Signature(*b"SNCT")]),
    },
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&SNCT_DATA_FIELDS),
    },
];

/// SNCT — Sound category.
pub static SNCT_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"SNCT"),
    name: "Sound Category",
    members: &SNCT_MEMBERS,
};


static SOPM_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"NAM1"),
        name: "Output Type",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"FNAM"),
        name: "Flags",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
];

/// SOPM — Sound output model.
pub static SOPM_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"SOPM"),
    name: "Sound Output Model",
    members: &SOPM_MEMBERS,
};


static FSTP_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Impact Dataset",
        required: true,
        repeating: false,
        field: FieldType::FormIdTyped(&[Signature(*b"IPDS")]),
    },
    SubRecordDef {
        sig: Signature(*b"ANAM"),
        name: "Tag",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
];

/// FSTP — Footstep (single step impact mapping).
pub static FSTP_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"FSTP"),
    name: "Footstep",
    members: &FSTP_MEMBERS,
};


static FSTS_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"XCNT"),
        name: "Footstep Count",
        required: true,
        repeating: false,
        field: FieldType::UInt32,
    },
];

/// FSTS — Footstep set (groups footstep entries).
pub static FSTS_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"FSTS"),
    name: "Footstep Set",
    members: &FSTS_MEMBERS,
};


static ARTO_DATA_FIELDS: [FieldDef; 1] = [
    FieldDef { name: "Art Type", kind: FieldType::UInt32 },
];

static ARTO_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    OBND_DEF,
    MODL_DEF,
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&ARTO_DATA_FIELDS),
    },
];

/// ARTO — Art object (particle effect mesh).
pub static ARTO_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"ARTO"),
    name: "Art Object",
    members: &ARTO_MEMBERS,
};


static MATO_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    OBND_DEF,
    MODL_DEF,
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// MATO — Material object.
pub static MATO_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"MATO"),
    name: "Material Object",
    members: &MATO_MEMBERS,
};

// Suppress dead-code warnings for transitively imported items.
const _: () = {
    let _ = &SOUND_LEVEL_ENUM;
};
