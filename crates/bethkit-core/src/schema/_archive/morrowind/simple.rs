// SPDX-License-Identifier: Apache-2.0
//! Morrowind simple and utility record schemas.
//!
//! Covers the file header (`TES3`), global variables, game settings, statics,
//! sounds, scripts, regions, cells, landscape, path grids, and other records
//! with straightforward or raw binary subrecord layouts.

use crate::schema::{FieldDef, FieldType, RecordSchema, SubRecordDef};
use crate::types::Signature;

use super::common::{
    DELE_DEF, DESC_DEF, FNAM_DEF, MODL_DEF, NAME_DEF, NPCS_DEF, SCRI_DEF,
};


static TES3_MEMBERS: [SubRecordDef; 3] = [
    SubRecordDef {
        sig: Signature(*b"HEDR"),
        name: "File Header",
        required: true,
        repeating: false,
        // 296 bytes: version(f32) + record_flags(u32) + author([u8;32])
        //            + description([u8;256]) + num_records(u32)
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"MAST"),
        name: "Master File",
        required: false,
        repeating: true,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Master File Size",
        required: false,
        repeating: true,
        field: FieldType::UInt64,
    },
];

/// Schema for the `TES3` file-header record.
pub(super) static TES3_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"TES3"),
    name: "File Header",
    members: &TES3_MEMBERS,
};


static GLOB_MEMBERS: [SubRecordDef; 4] = [
    NAME_DEF,
    DELE_DEF,
    SubRecordDef {
        sig: Signature(*b"FNAM"),
        name: "Variable Type",
        required: false,
        repeating: false,
        // Single byte: 0x66='f'=Float, 0x6C='l'=Long, 0x73='s'=Short
        field: FieldType::UInt8,
    },
    SubRecordDef {
        sig: Signature(*b"FLTV"),
        name: "Value",
        required: false,
        repeating: false,
        field: FieldType::Float32,
    },
];

/// Schema for the `GLOB` global variable record.
pub(super) static GLOB_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"GLOB"),
    name: "Global Variable",
    members: &GLOB_MEMBERS,
};


static GMST_MEMBERS: [SubRecordDef; 5] = [
    NAME_DEF,
    DELE_DEF,
    SubRecordDef {
        sig: Signature(*b"STRV"),
        name: "String Value",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"INTV"),
        name: "Integer Value",
        required: false,
        repeating: false,
        field: FieldType::Int32,
    },
    SubRecordDef {
        sig: Signature(*b"FLTV"),
        name: "Float Value",
        required: false,
        repeating: false,
        field: FieldType::Float32,
    },
];

/// Schema for the `GMST` game setting record.
pub(super) static GMST_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"GMST"),
    name: "Game Setting",
    members: &GMST_MEMBERS,
};


static STAT_MEMBERS: [SubRecordDef; 3] = [
    NAME_DEF,
    DELE_DEF,
    MODL_DEF,
];

/// Schema for the `STAT` static (non-interactive) world object record.
pub(super) static STAT_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"STAT"),
    name: "Static Object",
    members: &STAT_MEMBERS,
};


static SOUN_DATA_FIELDS: [FieldDef; 3] = [
    FieldDef { name: "Volume",    kind: FieldType::UInt8 },
    FieldDef { name: "Min Range", kind: FieldType::UInt8 },
    FieldDef { name: "Max Range", kind: FieldType::UInt8 },
];

static SOUN_MEMBERS: [SubRecordDef; 4] = [
    NAME_DEF,
    DELE_DEF,
    SubRecordDef {
        sig: Signature(*b"FNAM"),
        name: "Sound Filename",
        required: true,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Sound Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&SOUN_DATA_FIELDS),
    },
];

/// Schema for the `SOUN` sound record.
pub(super) static SOUN_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"SOUN"),
    name: "Sound",
    members: &SOUN_MEMBERS,
};


static SSCR_MEMBERS: [SubRecordDef; 3] = [
    DELE_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Sort Key",
        required: true,
        repeating: false,
        // Numeric string used as a sort key for startup ordering.
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"NAME"),
        name: "Script",
        required: true,
        repeating: false,
        field: FieldType::ZString,
    },
];

/// Schema for the `SSCR` start-script record.
pub(super) static SSCR_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"SSCR"),
    name: "Start Script",
    members: &SSCR_MEMBERS,
};


static SNDG_MEMBERS: [SubRecordDef; 5] = [
    NAME_DEF,
    DELE_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Generator Type",
        required: true,
        repeating: false,
        // 0=LeftFoot, 1=RightFoot, 2=SwimLeft, 3=SwimRight,
        // 4=Moan, 5=Roar, 6=Scream, 7=Land
        field: FieldType::UInt32,
    },
    SubRecordDef {
        sig: Signature(*b"CNAM"),
        name: "Creature",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"SNAM"),
        name: "Sound",
        required: true,
        repeating: false,
        field: FieldType::ZString,
    },
];

/// Schema for the `SNDG` sound generator record.
pub(super) static SNDG_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"SNDG"),
    name: "Sound Generator",
    members: &SNDG_MEMBERS,
};


static BODY_BYDT_FIELDS: [FieldDef; 4] = [
    FieldDef { name: "Body Part",  kind: FieldType::UInt8 },
    FieldDef { name: "Skin Type",  kind: FieldType::UInt8 },
    FieldDef { name: "Flags",      kind: FieldType::UInt8 },
    FieldDef { name: "Part Type",  kind: FieldType::UInt8 },
];

static BODY_MEMBERS: [SubRecordDef; 5] = [
    NAME_DEF,
    DELE_DEF,
    MODL_DEF,
    SubRecordDef {
        sig: Signature(*b"FNAM"),
        name: "Race",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"BYDT"),
        name: "Body Part Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&BODY_BYDT_FIELDS),
    },
];

/// Schema for the `BODY` body-part record.
pub(super) static BODY_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"BODY"),
    name: "Body Part",
    members: &BODY_MEMBERS,
};


static LTEX_MEMBERS: [SubRecordDef; 4] = [
    DELE_DEF,
    NAME_DEF,
    SubRecordDef {
        sig: Signature(*b"INTV"),
        name: "Texture Index",
        required: true,
        repeating: false,
        field: FieldType::UInt32,
    },
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Texture Filename",
        required: true,
        repeating: false,
        field: FieldType::ZString,
    },
];

/// Schema for the `LTEX` landscape texture record.
pub(super) static LTEX_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"LTEX"),
    name: "Landscape Texture",
    members: &LTEX_MEMBERS,
};


static BSGN_MEMBERS: [SubRecordDef; 6] = [
    NAME_DEF,
    DELE_DEF,
    FNAM_DEF,
    SubRecordDef {
        sig: Signature(*b"TNAM"),
        name: "Constellation Texture",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    DESC_DEF,
    NPCS_DEF,
];

/// Schema for the `BSGN` birthsign record.
pub(super) static BSGN_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"BSGN"),
    name: "Birthsign",
    members: &BSGN_MEMBERS,
};


static REGN_MEMBERS: [SubRecordDef; 7] = [
    DELE_DEF,
    NAME_DEF,
    FNAM_DEF,
    SubRecordDef {
        sig: Signature(*b"WEAT"),
        name: "Weather Chances",
        required: false,
        repeating: false,
        // 10 bytes: clear, cloudy, foggy, overcast, rain, thunder, ash,
        //           blight, snow, blizzard (each u8)
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"BNAM"),
        name: "Sleep Creature",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"CNAM"),
        name: "Map Color RGBA",
        required: true,
        repeating: false,
        field: FieldType::UInt32,
    },
    SubRecordDef {
        sig: Signature(*b"SNAM"),
        name: "Sound Chance",
        required: false,
        repeating: true,
        // Struct: sound name[32] + chance(u8) = 33 bytes per entry
        field: FieldType::ByteArray,
    },
];

/// Schema for the `REGN` region record.
pub(super) static REGN_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"REGN"),
    name: "Region",
    members: &REGN_MEMBERS,
};


static CELL_DATA_FIELDS: [FieldDef; 3] = [
    FieldDef { name: "Flags",  kind: FieldType::UInt32 },
    FieldDef { name: "Grid X", kind: FieldType::Int32 },
    FieldDef { name: "Grid Y", kind: FieldType::Int32 },
];

static CELL_AMBI_FIELDS: [FieldDef; 4] = [
    FieldDef { name: "Ambient Color",  kind: FieldType::UInt32 },
    FieldDef { name: "Sunlight Color", kind: FieldType::UInt32 },
    FieldDef { name: "Fog Color",      kind: FieldType::UInt32 },
    FieldDef { name: "Fog Density",    kind: FieldType::Float32 },
];

static CELL_MEMBERS: [SubRecordDef; 8] = [
    NAME_DEF,
    DELE_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Cell Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&CELL_DATA_FIELDS),
    },
    SubRecordDef {
        sig: Signature(*b"RGNN"),
        name: "Region",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"INTV"),
        name: "Water Height (Legacy)",
        required: false,
        repeating: false,
        field: FieldType::Int32,
    },
    SubRecordDef {
        sig: Signature(*b"NAM5"),
        name: "Map Color RGBA",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
    SubRecordDef {
        sig: Signature(*b"WHGT"),
        name: "Water Height",
        required: false,
        repeating: false,
        field: FieldType::Float32,
    },
    SubRecordDef {
        sig: Signature(*b"AMBI"),
        name: "Ambient Light",
        required: false,
        repeating: false,
        field: FieldType::Struct(&CELL_AMBI_FIELDS),
    },
];

/// Schema for the `CELL` cell record.
pub(super) static CELL_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"CELL"),
    name: "Cell",
    members: &CELL_MEMBERS,
};


static LAND_INTV_FIELDS: [FieldDef; 2] = [
    FieldDef { name: "Grid X", kind: FieldType::Int32 },
    FieldDef { name: "Grid Y", kind: FieldType::Int32 },
];

static LAND_MEMBERS: [SubRecordDef; 7] = [
    SubRecordDef {
        sig: Signature(*b"INTV"),
        name: "Grid Position",
        required: true,
        repeating: false,
        field: FieldType::Struct(&LAND_INTV_FIELDS),
    },
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Flags",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
    SubRecordDef {
        sig: Signature(*b"VNML"),
        name: "Vertex Normals",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"VHGT"),
        name: "Vertex Heights",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"WNAM"),
        name: "World Map Colors",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"VCLR"),
        name: "Vertex Colors",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"VTEX"),
        name: "Texture Indices",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// Schema for the `LAND` landscape record.
pub(super) static LAND_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"LAND"),
    name: "Landscape",
    members: &LAND_MEMBERS,
};


static PGRD_DATA_FIELDS: [FieldDef; 4] = [
    FieldDef { name: "Grid X",      kind: FieldType::Int32 },
    FieldDef { name: "Grid Y",      kind: FieldType::Int32 },
    FieldDef { name: "Granularity", kind: FieldType::UInt16 },
    FieldDef { name: "Point Count", kind: FieldType::UInt16 },
];

static PGRD_MEMBERS: [SubRecordDef; 4] = [
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Path Grid Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&PGRD_DATA_FIELDS),
    },
    NAME_DEF,
    SubRecordDef {
        sig: Signature(*b"PGRP"),
        name: "Path Grid Points",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"PGRC"),
        name: "Path Grid Connections",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// Schema for the `PGRD` path-grid record.
pub(super) static PGRD_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"PGRD"),
    name: "Path Grid",
    members: &PGRD_MEMBERS,
};

// NOTE: suppress unused-import warning for SCRI_DEF (not used in simple records
//       but re-exported for use in other submodules via the parent mod)
const _: () = {
    let _ = &SCRI_DEF;
};
