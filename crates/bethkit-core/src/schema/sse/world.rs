// SPDX-License-Identifier: Apache-2.0
//!
//! Schema definitions for world / environment SSE record types.
//!
//! Covers: CELL, WRLD, LAND, REGN, NAVM, NAVI, ACTI, TACT, STAT, GRAS,
//! TREE, FLOR, MSTT, LTEX, LIGH, WATR, WTHR, CLMT, ASPC, ECZN.

use crate::schema::{FieldDef, FieldType, RecordSchema, SubRecordDef};
use crate::types::Signature;

use super::common::{
    DESC_DEF, EDID_DEF, FULL_DEF, KSIZ_DEF, KWDA_DEF, MODL_DEF, OBND_DEF, RNAM_DEF, VMAD_DEF,
};
use crate::schema::enums::{CELL_FLAGS, LIGHT_FLAGS, SOUND_LEVEL_ENUM, WEATHER_CLASS_ENUM};


static ACTI_MEMBERS: [SubRecordDef; 8] = [
    EDID_DEF,
    VMAD_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    KSIZ_DEF,
    KWDA_DEF,
    SubRecordDef {
        sig: Signature(*b"FNAM"),
        name: "Flags",
        required: false,
        repeating: false,
        field: FieldType::UInt16,
    },
];

/// ACTI — Activator (interactive object).
pub static ACTI_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"ACTI"),
    name: "Activator",
    members: &ACTI_MEMBERS,
};


static TACT_MEMBERS: [SubRecordDef; 8] = [
    EDID_DEF,
    VMAD_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    KSIZ_DEF,
    KWDA_DEF,
    SubRecordDef {
        sig: Signature(*b"VNAM"),
        name: "Voice",
        required: false,
        repeating: false,
        field: FieldType::FormIdTyped(&[Signature(*b"VTYP")]),
    },
];

/// TACT — Talking activator.
pub static TACT_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"TACT"),
    name: "Talking Activator",
    members: &TACT_MEMBERS,
};


static STAT_DNAM_FIELDS: [FieldDef; 2] = [
    FieldDef { name: "Max Angle", kind: FieldType::Float32 },
    FieldDef { name: "Material", kind: FieldType::FormIdTyped(&[Signature(*b"MATO")]) },
];

static STAT_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    OBND_DEF,
    MODL_DEF,
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Direction Material",
        required: false,
        repeating: false,
        field: FieldType::Struct(&STAT_DNAM_FIELDS),
    },
    SubRecordDef {
        sig: Signature(*b"MNAM"),
        // NOTE: Not a FormID — four NUL-terminated LOD mesh path strings
        // NOTE: (level 0–3, up to 260 chars each).
        name: "LOD Meshes",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// STAT — Static object.
pub static STAT_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"STAT"),
    name: "Static",
    members: &STAT_MEMBERS,
};


static GRAS_DATA_FIELDS: [FieldDef; 8] = [
    FieldDef { name: "Density", kind: FieldType::UInt8 },
    FieldDef { name: "Min Slope", kind: FieldType::UInt8 },
    FieldDef { name: "Max Slope", kind: FieldType::UInt8 },
    FieldDef { name: "Unknown", kind: FieldType::UInt8 },
    FieldDef { name: "Units From Water Amt", kind: FieldType::UInt16 },
    FieldDef { name: "Pad", kind: FieldType::UInt16 },
    FieldDef { name: "Units From Water Type", kind: FieldType::UInt32 },
    FieldDef { name: "Position Range", kind: FieldType::Float32 },
];

static GRAS_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    OBND_DEF,
    MODL_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&GRAS_DATA_FIELDS),
    },
];

/// GRAS — Grass.
pub static GRAS_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"GRAS"),
    name: "Grass",
    members: &GRAS_MEMBERS,
};


static TREE_CNAM_FIELDS: [FieldDef; 6] = [
    FieldDef { name: "Trunk Flexibility", kind: FieldType::Float32 },
    FieldDef { name: "Branch Flexibility", kind: FieldType::Float32 },
    FieldDef { name: "Trunk Amplitude", kind: FieldType::Float32 },
    FieldDef { name: "Front Amplitude", kind: FieldType::Float32 },
    FieldDef { name: "Back Amplitude", kind: FieldType::Float32 },
    FieldDef { name: "Side Amplitude", kind: FieldType::Float32 },
];

static TREE_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    OBND_DEF,
    MODL_DEF,
    SubRecordDef {
        sig: Signature(*b"PFIG"),
        name: "Plant Ingredient",
        required: false,
        repeating: false,
        field: FieldType::FormIdTyped(&[Signature(*b"INGR"), Signature(*b"ALCH"), Signature(*b"MISC"), Signature(*b"LVLI")]),
    },
    SubRecordDef {
        sig: Signature(*b"CNAM"),
        name: "Tree Data",
        required: false,
        repeating: false,
        field: FieldType::Struct(&TREE_CNAM_FIELDS),
    },
];

/// TREE — Tree.
pub static TREE_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"TREE"),
    name: "Tree",
    members: &TREE_MEMBERS,
};


static FLOR_MEMBERS: [SubRecordDef; 7] = [
    EDID_DEF,
    VMAD_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    KSIZ_DEF,
    KWDA_DEF,
];

/// FLOR — Flora (harvestable plant).
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
        name: "On Local Map",
        required: false,
        repeating: false,
        field: FieldType::UInt8,
    },
];

/// MSTT — Moveable static object.
pub static MSTT_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"MSTT"),
    name: "Moveable Static",
    members: &MSTT_MEMBERS,
};


static LTEX_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"TNAM"),
        name: "Texture Set",
        required: false,
        repeating: false,
        field: FieldType::FormIdTyped(&[Signature(*b"TXST")]),
    },
    SubRecordDef {
        sig: Signature(*b"MNAM"),
        name: "Material Type",
        required: false,
        repeating: false,
        field: FieldType::FormIdTyped(&[Signature(*b"MATT")]),
    },
];

/// LTEX — Land texture.
pub static LTEX_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"LTEX"),
    name: "Land Texture",
    members: &LTEX_MEMBERS,
};


static LIGH_DATA_FIELDS: [FieldDef; 9] = [
    FieldDef { name: "Time", kind: FieldType::Int32 },
    FieldDef { name: "Radius", kind: FieldType::UInt32 },
    FieldDef { name: "Color (RGBA)", kind: FieldType::UInt32 },
    FieldDef { name: "Flags", kind: FieldType::Flags(&LIGHT_FLAGS) },
    FieldDef { name: "Falloff Exp", kind: FieldType::Float32 },
    FieldDef { name: "FOV", kind: FieldType::Float32 },
    FieldDef { name: "Near Clip", kind: FieldType::Float32 },
    FieldDef { name: "Period", kind: FieldType::Float32 },
    FieldDef { name: "Intensity Amplitude", kind: FieldType::Float32 },
];

static LIGH_MEMBERS: [SubRecordDef; 6] = [
    EDID_DEF,
    OBND_DEF,
    MODL_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Light Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&LIGH_DATA_FIELDS),
    },
    SubRecordDef {
        sig: Signature(*b"FNAM"),
        name: "Fade value",
        required: false,
        repeating: false,
        field: FieldType::Float32,
    },
];

/// LIGH — Light source.
pub static LIGH_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"LIGH"),
    name: "Light",
    members: &LIGH_MEMBERS,
};


static WATR_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"NNAM"),
        name: "Noise Normals",
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
        name: "Visual / Sound Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// WATR — Water type.
pub static WATR_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"WATR"),
    name: "Water",
    members: &WATR_MEMBERS,
};


static WTHR_DATA_FIELDS: [FieldDef; 7] = [
    FieldDef { name: "Wind Speed", kind: FieldType::UInt8 },
    FieldDef { name: "Wind Angle", kind: FieldType::UInt8 },
    FieldDef { name: "Trans Delta", kind: FieldType::UInt8 },
    FieldDef { name: "Sun Glare", kind: FieldType::UInt8 },
    FieldDef { name: "Sun Damage", kind: FieldType::UInt8 },
    FieldDef { name: "Precipitation Begin", kind: FieldType::UInt8 },
    FieldDef { name: "Weather Class", kind: FieldType::Enum(&WEATHER_CLASS_ENUM) },
];

static WTHR_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Cloud Texture Layers",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"CNAM"),
        name: "Sky Colors",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&WTHR_DATA_FIELDS),
    },
];

/// WTHR — Weather type.
pub static WTHR_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"WTHR"),
    name: "Weather",
    members: &WTHR_MEMBERS,
};


static CLMT_WLST_FIELDS: [FieldDef; 3] = [
    FieldDef { name: "Weather", kind: FieldType::FormIdTyped(&[Signature(*b"WTHR")]) },
    FieldDef { name: "Chance", kind: FieldType::Int32 },
    FieldDef { name: "Global", kind: FieldType::FormIdTyped(&[Signature(*b"GLOB")]) },
];

static CLMT_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"WLST"),
        name: "Weather List",
        required: false,
        repeating: true,
        field: FieldType::Struct(&CLMT_WLST_FIELDS),
    },
    SubRecordDef {
        sig: Signature(*b"FNAM"),
        name: "Sun Texture",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Timing Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// CLMT — Climate.
pub static CLMT_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"CLMT"),
    name: "Climate",
    members: &CLMT_MEMBERS,
};


static ASPC_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    OBND_DEF,
    SubRecordDef {
        sig: Signature(*b"SNAM"),
        name: "Ambient Sound",
        required: false,
        repeating: false,
        field: FieldType::FormIdTyped(&[Signature(*b"SNDR")]),
    },
    SubRecordDef {
        sig: Signature(*b"RDAT"),
        name: "Reverb Type",
        required: false,
        repeating: false,
        field: FieldType::FormIdTyped(&[Signature(*b"REVB")]),
    },
    SubRecordDef {
        sig: Signature(*b"BNAM"),
        name: "Region",
        required: false,
        repeating: false,
        field: FieldType::FormIdTyped(&[Signature(*b"REGN")]),
    },
];

/// ASPC — Acoustic space.
pub static ASPC_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"ASPC"),
    name: "Acoustic Space",
    members: &ASPC_MEMBERS,
};


static ECZN_DATA_FIELDS: [FieldDef; 5] = [
    FieldDef { name: "Owner", kind: FieldType::FormIdTyped(&[Signature(*b"NPC_"), Signature(*b"FACT")]) },
    FieldDef { name: "Location", kind: FieldType::FormIdTyped(&[Signature(*b"LCTN")]) },
    FieldDef { name: "Rank", kind: FieldType::Int8 },
    FieldDef { name: "Min Level", kind: FieldType::Int8 },
    FieldDef { name: "Flags", kind: FieldType::UInt8 },
];

static ECZN_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&ECZN_DATA_FIELDS),
    },
];

/// ECZN — Encounter zone.
pub static ECZN_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"ECZN"),
    name: "Encounter Zone",
    members: &ECZN_MEMBERS,
};


static CELL_MEMBERS: [SubRecordDef; 8] = [
    EDID_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Flags",
        required: false,
        repeating: false,
        field: FieldType::Flags(&CELL_FLAGS),
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
        field: FieldType::FormIdTyped(&[Signature(*b"LGTM")]),
    },
    SubRecordDef {
        sig: Signature(*b"XCLW"),
        name: "Water Height",
        required: false,
        repeating: false,
        field: FieldType::Float32,
    },
    SubRecordDef {
        sig: Signature(*b"XCWT"),
        name: "Water",
        required: false,
        repeating: false,
        field: FieldType::FormIdTyped(&[Signature(*b"WATR")]),
    },
];

/// CELL — Cell (interior or exterior area).
pub static CELL_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"CELL"),
    name: "Cell",
    members: &CELL_MEMBERS,
};


static WRLD_DATA_FIELDS: [FieldDef; 2] = [
    FieldDef { name: "Flags", kind: FieldType::UInt8 },
    FieldDef { name: "Unknown", kind: FieldType::UInt8 },
];

static WRLD_MEMBERS: [SubRecordDef; 9] = [
    EDID_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"WCTR"),
        name: "Fixed Center Coordinates",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"LTMP"),
        name: "Interior Lighting",
        required: false,
        repeating: false,
        field: FieldType::FormIdTyped(&[Signature(*b"LGTM")]),
    },
    SubRecordDef {
        sig: Signature(*b"XLCN"),
        name: "Location",
        required: false,
        repeating: false,
        field: FieldType::FormIdTyped(&[Signature(*b"LCTN")]),
    },
    SubRecordDef {
        sig: Signature(*b"XNAM"),
        name: "Music Type Override",
        required: false,
        repeating: false,
        field: FieldType::FormIdTyped(&[Signature(*b"MUSC")]),
    },
    SubRecordDef {
        sig: Signature(*b"CNAM"),
        name: "Climate",
        required: false,
        repeating: false,
        field: FieldType::FormIdTyped(&[Signature(*b"CLMT")]),
    },
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Flags",
        required: false,
        repeating: false,
        field: FieldType::Struct(&WRLD_DATA_FIELDS),
    },
    RNAM_DEF,
];

/// WRLD — Worldspace.
pub static WRLD_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"WRLD"),
    name: "Worldspace",
    members: &WRLD_MEMBERS,
};


static LAND_MEMBERS: [SubRecordDef; 3] = [
    SubRecordDef {
        sig: Signature(*b"VHGT"),
        name: "Height Map",
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
        sig: Signature(*b"ATXT"),
        name: "Alpha Texture",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
];

/// LAND — Landscape geometry.
pub static LAND_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"LAND"),
    name: "Land",
    members: &LAND_MEMBERS,
};


static REGN_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"WNAM"),
        name: "Worldspace",
        required: false,
        repeating: false,
        field: FieldType::FormIdTyped(&[Signature(*b"WRLD")]),
    },
    SubRecordDef {
        sig: Signature(*b"RDAT"),
        name: "Region Data",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"RPLI"),
        name: "Region Area Line",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
];

/// REGN — Region (map-based area definition).
pub static REGN_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"REGN"),
    name: "Region",
    members: &REGN_MEMBERS,
};


static NAVM_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"NVNM"),
        name: "Nav Mesh Data",
        required: true,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"ONAM"),
        name: "Nav Mesh Geometry",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// NAVM — Navigation mesh.
pub static NAVM_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"NAVM"),
    name: "Navigation Mesh",
    members: &NAVM_MEMBERS,
};


static NAVI_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"NVMI"),
        name: "Nav Mesh Info",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
];

/// NAVI — Navigation mesh info map.
pub static NAVI_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"NAVI"),
    name: "Navigation Mesh Info Map",
    members: &NAVI_MEMBERS,
};


static LSCR_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DESC"),
        name: "Description",
        required: true,
        repeating: false,
        field: FieldType::LString,
    },
];

/// LSCR — Load screen.
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
        sig: Signature(*b"DNAM"),
        name: "Fade Distance Radius Scale",
        required: false,
        repeating: false,
        field: FieldType::Float32,
    },
];

/// LENS — Lens flare (added in Skyrim SE).
pub static LENS_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"LENS"),
    name: "Lens Flare",
    members: &LENS_MEMBERS,
};


static VOLI_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"CNAM"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// VOLI — Volumetric lighting (added in Skyrim SE).
pub static VOLI_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"VOLI"),
    name: "Volumetric Lighting",
    members: &VOLI_MEMBERS,
};

// Suppress dead-code warnings for imports used only in secondary defs.
const _: () = {
    let _ = &DESC_DEF;
    let _ = &SOUND_LEVEL_ENUM;
};
