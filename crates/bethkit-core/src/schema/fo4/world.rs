// SPDX-License-Identifier: Apache-2.0
//!
//! Fallout 4 world and environment record schemas.
//!
//! Covers ACTI, TACT, STAT, GRAS, TREE, FLOR, MSTT, LTEX, LIGH, WATR,
//! WTHR, CLMT, ASPC, ECZN, CELL, WRLD, LAND, REGN, NAVM, NAVI, LSCR, LENS.

use crate::schema::{FieldDef, FieldType, RecordSchema, SubRecordDef};
use crate::types::Signature;

use super::common::{
    CTDA_DEF, DESC_DEF, EDID_DEF, FULL_DEF, KSIZ_DEF, KWDA_DEF, MODL_DEF, OBND_DEF, VMAD_DEF,
};

static ACTI_MEMBERS: [SubRecordDef; 7] = [
    EDID_DEF, VMAD_DEF, OBND_DEF, FULL_DEF, MODL_DEF, KSIZ_DEF, KWDA_DEF,
];

/// ACTI — activator.
pub static ACTI_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"ACTI"),
    name: "Activator",
    members: &ACTI_MEMBERS,
};

static TACT_MEMBERS: [SubRecordDef; 6] = [
    EDID_DEF,
    VMAD_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    SubRecordDef {
        sig: Signature(*b"VNAM"),
        name: "Voice Type",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
];

/// TACT — talking activator.
pub static TACT_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"TACT"),
    name: "Talking Activator",
    members: &TACT_MEMBERS,
};

static STAT_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    OBND_DEF,
    MODL_DEF,
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Max Angle",
        required: false,
        repeating: false,
        field: FieldType::Float32,
    },
];

/// STAT — static object.
pub static STAT_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"STAT"),
    name: "Static Object",
    members: &STAT_MEMBERS,
};

static GRAS_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    OBND_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// GRAS — grass.
pub static GRAS_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"GRAS"),
    name: "Grass",
    members: &GRAS_MEMBERS,
};

static TREE_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    OBND_DEF,
    MODL_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"CNAM"),
        name: "Harvested Sound",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
];

/// TREE — tree.
pub static TREE_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"TREE"),
    name: "Tree",
    members: &TREE_MEMBERS,
};

static FLOR_MEMBERS: [SubRecordDef; 6] = [
    EDID_DEF,
    VMAD_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    SubRecordDef {
        sig: Signature(*b"PFIG"),
        name: "Ingredient",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
];

/// FLOR — flora.
pub static FLOR_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"FLOR"),
    name: "Flora",
    members: &FLOR_MEMBERS,
};

static MSTT_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Flags",
        required: false,
        repeating: false,
        field: FieldType::UInt8,
    },
];

/// MSTT — movable static.
pub static MSTT_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"MSTT"),
    name: "Movable Static",
    members: &MSTT_MEMBERS,
};

static LTEX_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"TNAM"),
        name: "Texture Set",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"MNAM"),
        name: "Material Type",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
];

/// LTEX — land texture.
pub static LTEX_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"LTEX"),
    name: "Land Texture",
    members: &LTEX_MEMBERS,
};

static LIGH_DATA_FIELDS: [FieldDef; 9] = [
    FieldDef {
        name: "Time",
        kind: FieldType::Int32,
    },
    FieldDef {
        name: "Radius",
        kind: FieldType::UInt32,
    },
    FieldDef {
        name: "Color",
        kind: FieldType::UInt32,
    },
    FieldDef {
        name: "Flags",
        kind: FieldType::UInt32,
    },
    FieldDef {
        name: "Falloff Exponent",
        kind: FieldType::Float32,
    },
    FieldDef {
        name: "FOV",
        kind: FieldType::Float32,
    },
    FieldDef {
        name: "Near Clip",
        kind: FieldType::Float32,
    },
    FieldDef {
        name: "Period",
        kind: FieldType::Float32,
    },
    FieldDef {
        name: "Intensity Amplitude",
        kind: FieldType::Float32,
    },
];

static LIGH_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::Struct(&LIGH_DATA_FIELDS),
    },
];

/// LIGH — light.
pub static LIGH_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"LIGH"),
    name: "Light",
    members: &LIGH_MEMBERS,
};

static WATR_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"NNAM"),
        name: "Noise Texture",
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

/// WATR — water type.
pub static WATR_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"WATR"),
    name: "Water Type",
    members: &WATR_MEMBERS,
};

static WTHR_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"CNAM"),
        name: "Clouds",
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
    SubRecordDef {
        sig: Signature(*b"GNAM"),
        name: "Aurora",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
];

/// WTHR — weather.
pub static WTHR_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"WTHR"),
    name: "Weather",
    members: &WTHR_MEMBERS,
};

static CLMT_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"WLST"),
        name: "Weather List",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"TNAM"),
        name: "Sun Texture",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
];

/// CLMT — climate.
pub static CLMT_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"CLMT"),
    name: "Climate",
    members: &CLMT_MEMBERS,
};

static ASPC_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    OBND_DEF,
    SubRecordDef {
        sig: Signature(*b"SNAM"),
        name: "Ambient Sound",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"RDAT"),
        name: "Use Defaults From",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
];

/// ASPC — acoustic space.
pub static ASPC_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"ASPC"),
    name: "Acoustic Space",
    members: &ASPC_MEMBERS,
};

static ECZN_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// ECZN — encounter zone.
pub static ECZN_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"ECZN"),
    name: "Encounter Zone",
    members: &ECZN_MEMBERS,
};

static CELL_MEMBERS: [SubRecordDef; 6] = [
    EDID_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Flags",
        required: false,
        repeating: false,
        field: FieldType::UInt16,
    },
    SubRecordDef {
        sig: Signature(*b"XCLC"),
        name: "Grid",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"XCLL"),
        name: "Lighting",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"LTMP"),
        name: "Lighting Template",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
];

/// CELL — cell.
pub static CELL_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"CELL"),
    name: "Cell",
    members: &CELL_MEMBERS,
};

static WRLD_MEMBERS: [SubRecordDef; 8] = [
    EDID_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"WNAM"),
        name: "Parent Worldspace",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"CNAM"),
        name: "Climate",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Flags",
        required: false,
        repeating: false,
        field: FieldType::UInt8,
    },
    SubRecordDef {
        sig: Signature(*b"PNAM"),
        name: "Parent Flags",
        required: false,
        repeating: false,
        field: FieldType::UInt16,
    },
    SubRecordDef {
        sig: Signature(*b"MNAM"),
        name: "Map Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"ONAM"),
        name: "World Map Offset Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// WRLD — worldspace.
pub static WRLD_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"WRLD"),
    name: "Worldspace",
    members: &WRLD_MEMBERS,
};

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
pub static LAND_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"LAND"),
    name: "Landscape",
    members: &LAND_MEMBERS,
};

static REGN_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"WNAM"),
        name: "Worldspace",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"RPLI"),
        name: "Region Point List",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"RDAT"),
        name: "Region Data Entry",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
];

/// REGN — region.
pub static REGN_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"REGN"),
    name: "Region",
    members: &REGN_MEMBERS,
};

static NAVM_MEMBERS: [SubRecordDef; 2] = [
    SubRecordDef {
        sig: Signature(*b"NVNM"),
        name: "Navmesh Geometry",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"ONAM"),
        name: "Onam",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// NAVM — navigation mesh.
pub static NAVM_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"NAVM"),
    name: "Navigation Mesh",
    members: &NAVM_MEMBERS,
};

static NAVI_MEMBERS: [SubRecordDef; 2] = [
    SubRecordDef {
        sig: Signature(*b"NVVX"),
        name: "Navigation Map Vertex",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"NVPP"),
        name: "Preferred Pathing",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// NAVI — navigation mesh info map.
pub static NAVI_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"NAVI"),
    name: "Navigation Mesh Info Map",
    members: &NAVI_MEMBERS,
};

static LSCR_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"ICON"),
        name: "Icon",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    DESC_DEF,
    CTDA_DEF,
];

/// LSCR — load screen.
pub static LSCR_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"LSCR"),
    name: "Load Screen",
    members: &LSCR_MEMBERS,
};

static LENS_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"CNAM"),
        name: "Color Influence",
        required: false,
        repeating: false,
        field: FieldType::Float32,
    },
    SubRecordDef {
        sig: Signature(*b"LFSP"),
        name: "Sprites",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
];

/// LENS — lens flare.
pub static LENS_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"LENS"),
    name: "Lens Flare",
    members: &LENS_MEMBERS,
};

// Suppress lint for imported items not used in static positions.
const _: () = {
    let _ = &VMAD_DEF;
    let _: &[FieldDef] = &[];
};
