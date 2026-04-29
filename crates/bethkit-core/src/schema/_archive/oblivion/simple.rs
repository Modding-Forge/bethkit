// SPDX-License-Identifier: Apache-2.0
//!
//! Oblivion simple and utility record schemas.
//!
//! Covers TES4, GMST, GLOB, ANIO, GRAS, STAT, LTEX, WATR, WTHR, CLMT,
//! LAND, PGRD, ROAD, SBSP, SCPT, SKIL, BSGN, SGST, EFSH, LSCR, SOUN.

use crate::schema::{FieldDef, FieldType, RecordSchema, SubRecordDef};
use crate::types::Signature;

use super::common::{DESC_DEF, EDID_DEF, FULL_DEF, ICON_DEF, MODL_DEF, SCRI_DEF};


static TES4_MEMBERS: [SubRecordDef; 4] = [
    SubRecordDef {
        sig: Signature(*b"HEDR"),
        name: "Header",
        required: true,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"CNAM"),
        name: "Author",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"SNAM"),
        name: "Description",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"MAST"),
        name: "Master Filename",
        required: false,
        repeating: true,
        field: FieldType::ZString,
    },
];

/// TES4 — main file header.
pub static TES4_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"TES4"), name: "File Header", members: &TES4_MEMBERS };


static GMST_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Value",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// GMST — game setting.
pub static GMST_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"GMST"), name: "Game Setting", members: &GMST_MEMBERS };


static GLOB_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"FNAM"),
        name: "Type",
        required: false,
        repeating: false,
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

/// GLOB — global variable.
pub static GLOB_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"GLOB"), name: "Global Variable", members: &GLOB_MEMBERS };


static ANIO_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    MODL_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Idle Animation",
        required: true,
        repeating: false,
        field: FieldType::FormId,
    },
];

/// ANIO — animated object.
pub static ANIO_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"ANIO"), name: "Animated Object", members: &ANIO_MEMBERS };


static GRAS_DATA_FIELDS: [FieldDef; 5] = [
    FieldDef { name: "Density", kind: FieldType::UInt8 },
    FieldDef { name: "Min Slope", kind: FieldType::UInt8 },
    FieldDef { name: "Max Slope", kind: FieldType::UInt8 },
    FieldDef { name: "Wave Period", kind: FieldType::UInt16 },
    FieldDef { name: "Flags", kind: FieldType::UInt8 },
];

static GRAS_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    MODL_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&GRAS_DATA_FIELDS),
    },
];

/// GRAS — grass.
pub static GRAS_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"GRAS"), name: "Grass", members: &GRAS_MEMBERS };


static STAT_MEMBERS: [SubRecordDef; 2] = [EDID_DEF, MODL_DEF];

/// STAT — static object.
pub static STAT_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"STAT"), name: "Static Object", members: &STAT_MEMBERS };


static LTEX_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    ICON_DEF,
    SubRecordDef {
        sig: Signature(*b"HNAM"),
        name: "Havok Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"SNAM"),
        name: "Texture Specular Exponent",
        required: false,
        repeating: false,
        field: FieldType::UInt8,
    },
];

/// LTEX — landscape texture.
pub static LTEX_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"LTEX"),
    name: "Landscape Texture",
    members: &LTEX_MEMBERS,
};


static WATR_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"TNAM"),
        name: "Texture",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"ANAM"),
        name: "Opacity",
        required: false,
        repeating: false,
        field: FieldType::UInt8,
    },
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// WATR — water type.
pub static WATR_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"WATR"), name: "Water", members: &WATR_MEMBERS };


static WTHR_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"CNAM"),
        name: "Lower Cloud Layer",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Upper Cloud Layer",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"SNAM"),
        name: "Sound",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
];

/// WTHR — weather.
pub static WTHR_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"WTHR"), name: "Weather", members: &WTHR_MEMBERS };


static CLMT_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"WLST"),
        name: "Weather Types",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"FNAM"),
        name: "Sun Texture",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"GNAM"),
        name: "Sun Glare Texture",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
];

/// CLMT — climate.
pub static CLMT_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"CLMT"), name: "Climate", members: &CLMT_MEMBERS };


static LAND_MEMBERS: [SubRecordDef; 3] = [
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Flags",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
    SubRecordDef {
        sig: Signature(*b"VHGT"),
        name: "Vertex Height Map",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"VTEX"),
        name: "Vertex Textures",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// LAND — landscape / terrain.
pub static LAND_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"LAND"), name: "Landscape", members: &LAND_MEMBERS };


static PGRD_MEMBERS: [SubRecordDef; 2] = [
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Path Grid Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"PGAG"),
        name: "Path Grid Point Array",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// PGRD — path grid (Oblivion pathfinding data; replaced by NAVM in Skyrim).
pub static PGRD_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"PGRD"), name: "Path Grid", members: &PGRD_MEMBERS };


static ROAD_MEMBERS: [SubRecordDef; 1] = [SubRecordDef {
    sig: Signature(*b"PGRP"),
    name: "Point Array",
    required: false,
    repeating: false,
    field: FieldType::ByteArray,
}];

/// ROAD — road (used for NPC pathfinding in Oblivion worldspace).
pub static ROAD_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"ROAD"), name: "Road", members: &ROAD_MEMBERS };


static SBSP_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Dimensions",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// SBSP — subspace (Oblivion interior zone size hint).
pub static SBSP_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"SBSP"), name: "Subspace", members: &SBSP_MEMBERS };


static SCPT_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"SCHR"),
        name: "Script Header",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"SCTX"),
        name: "Script Source",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
];

/// SCPT — Oblivion script (compiled + source text).
pub static SCPT_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"SCPT"), name: "Script", members: &SCPT_MEMBERS };


static SKIL_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"INDX"),
        name: "Skill Index",
        required: true,
        repeating: false,
        field: FieldType::Int32,
    },
    DESC_DEF,
    ICON_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Skill Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// SKIL — skill definition (Oblivion-specific; replaced by actor values in
/// Skyrim).
pub static SKIL_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"SKIL"), name: "Skill", members: &SKIL_MEMBERS };


static BSGN_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"ICON"),
        name: "Constellation Filename",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    DESC_DEF,
];

/// BSGN — birthsign (Oblivion-specific; removed in Skyrim).
pub static BSGN_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"BSGN"), name: "Birthsign", members: &BSGN_MEMBERS };


static SGST_DATA_FIELDS: [FieldDef; 3] = [
    FieldDef { name: "Value", kind: FieldType::UInt32 },
    FieldDef { name: "Weight", kind: FieldType::Float32 },
    FieldDef { name: "Uses", kind: FieldType::UInt8 },
];

static SGST_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    FULL_DEF,
    MODL_DEF,
    ICON_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&SGST_DATA_FIELDS),
    },
];

/// SGST — sigil stone (Oblivion-specific enchanting item).
pub static SGST_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"SGST"), name: "Sigil Stone", members: &SGST_MEMBERS };


static EFSH_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"ICON"),
        name: "Fill Texture",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"ICO2"),
        name: "Particle Shader Texture",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// EFSH — effect shader.
pub static EFSH_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"EFSH"), name: "Effect Shader", members: &EFSH_MEMBERS };


static LSCR_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    ICON_DEF,
    DESC_DEF,
    SubRecordDef {
        sig: Signature(*b"LNAM"),
        name: "Load Screen Location",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
];

/// LSCR — load screen.
pub static LSCR_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"LSCR"), name: "Load Screen", members: &LSCR_MEMBERS };


static SOUN_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
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

/// SOUN — sound.
pub static SOUN_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"SOUN"), name: "Sound", members: &SOUN_MEMBERS };

// Suppress lints for static items not yet referenced directly.
const _: () = {
    let _: &[FieldDef] = &[];
    let _ = &SCRI_DEF;
};
