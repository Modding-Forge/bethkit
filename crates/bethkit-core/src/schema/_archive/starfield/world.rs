// SPDX-License-Identifier: Apache-2.0
//!
//! Starfield world / environment record schemas.
//!
//! Covers ACTI, TACT, DOOR, CONT, FURN, LIGH, STAT, MSTT, GRAS, TREE,
//! FLOR, CELL, WRLD, REGN, NAVM, WATR, LSCR, LTEX.

use crate::schema::{FieldDef, FieldType, RecordSchema, SubRecordDef};
use crate::types::Signature;

use super::common::{
    CTDA_DEF, DESC_DEF, EDID_DEF, FULL_DEF, KSIZ_DEF, KWDA_DEF, MODL_DEF, OBND_DEF, VMAD_DEF,
};


static ACTI_MEMBERS: [SubRecordDef; 8] = [
    EDID_DEF,
    VMAD_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    KSIZ_DEF,
    KWDA_DEF,
    SubRecordDef {
        sig: Signature(*b"RNAM"),
        name: "Activation Prompt",
        required: false,
        repeating: false,
        field: FieldType::LString,
    },
];

/// ACTI — activator.
pub static ACTI_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"ACTI"), name: "Activator", members: &ACTI_MEMBERS };


static TACT_MEMBERS: [SubRecordDef; 7] = [
    EDID_DEF,
    VMAD_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    KSIZ_DEF,
    KWDA_DEF,
];

/// TACT — talking activator.
pub static TACT_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"TACT"), name: "Talking Activator", members: &TACT_MEMBERS };


static DOOR_MEMBERS: [SubRecordDef; 7] = [
    EDID_DEF,
    VMAD_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    KSIZ_DEF,
    KWDA_DEF,
];

/// DOOR — door.
pub static DOOR_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"DOOR"), name: "Door", members: &DOOR_MEMBERS };


static CONT_MEMBERS: [SubRecordDef; 8] = [
    EDID_DEF,
    VMAD_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    KSIZ_DEF,
    KWDA_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Container Flags",
        required: false,
        repeating: false,
        field: FieldType::UInt8,
    },
];

/// CONT — container.
pub static CONT_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"CONT"), name: "Container", members: &CONT_MEMBERS };


static FURN_MEMBERS: [SubRecordDef; 8] = [
    EDID_DEF,
    VMAD_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    KSIZ_DEF,
    KWDA_DEF,
    SubRecordDef {
        sig: Signature(*b"FNMK"),
        name: "Furniture Marker",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
];

/// FURN — furniture.
pub static FURN_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"FURN"), name: "Furniture", members: &FURN_MEMBERS };


static LIGH_DATA_FIELDS: [FieldDef; 7] = [
    FieldDef { name: "Time", kind: FieldType::Int32 },
    FieldDef { name: "Radius", kind: FieldType::UInt32 },
    FieldDef { name: "Color", kind: FieldType::UInt32 },
    FieldDef { name: "Flags", kind: FieldType::UInt32 },
    FieldDef { name: "Fall-off Exponent", kind: FieldType::Float32 },
    FieldDef { name: "FOV", kind: FieldType::Float32 },
    FieldDef { name: "Value", kind: FieldType::UInt32 },
];

static LIGH_MEMBERS: [SubRecordDef; 6] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Light Data",
        required: false,
        repeating: false,
        field: FieldType::Struct(&LIGH_DATA_FIELDS),
    },
    KSIZ_DEF,
];

/// LIGH — light source.
pub static LIGH_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"LIGH"), name: "Light", members: &LIGH_MEMBERS };


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
pub static STAT_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"STAT"), name: "Static", members: &STAT_MEMBERS };


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

/// MSTT — moveable static.
pub static MSTT_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"MSTT"), name: "Moveable Static", members: &MSTT_MEMBERS };


static GRAS_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    OBND_DEF,
    MODL_DEF,
];

/// GRAS — grass type.
pub static GRAS_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"GRAS"), name: "Grass", members: &GRAS_MEMBERS };


static TREE_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    VMAD_DEF,
    OBND_DEF,
    MODL_DEF,
    SubRecordDef {
        sig: Signature(*b"CNAM"),
        name: "Trunk Flexibility",
        required: false,
        repeating: false,
        field: FieldType::Float32,
    },
];

/// TREE — tree.
pub static TREE_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"TREE"), name: "Tree", members: &TREE_MEMBERS };


static FLOR_MEMBERS: [SubRecordDef; 7] = [
    EDID_DEF,
    VMAD_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    KSIZ_DEF,
    KWDA_DEF,
];

/// FLOR — flora (harvestable plant).
pub static FLOR_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"FLOR"), name: "Flora", members: &FLOR_MEMBERS };


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

/// CELL — cell (interior or exterior worldspace unit).
pub static CELL_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"CELL"), name: "Cell", members: &CELL_MEMBERS };


static WRLD_MEMBERS: [SubRecordDef; 7] = [
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
        sig: Signature(*b"DATA"),
        name: "Flags",
        required: false,
        repeating: false,
        field: FieldType::UInt8,
    },
    SubRecordDef {
        sig: Signature(*b"NAM2"),
        name: "Water",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"NAM4"),
        name: "Climate",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"PNAM"),
        name: "Parent Flags",
        required: false,
        repeating: false,
        field: FieldType::UInt8,
    },
];

/// WRLD — worldspace.
pub static WRLD_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"WRLD"), name: "Worldspace", members: &WRLD_MEMBERS };


static REGN_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"RCLR"),
        name: "Map Color",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
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
];

/// REGN — region.
pub static REGN_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"REGN"), name: "Region", members: &REGN_MEMBERS };


static NAVM_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"NVNM"),
        name: "Navmesh Geometry",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"ONAM"),
        name: "Parent",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// NAVM — navmesh.
pub static NAVM_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"NAVM"), name: "Navmesh", members: &NAVM_MEMBERS };


static WATR_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Visual Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Damage per Second",
        required: false,
        repeating: false,
        field: FieldType::UInt16,
    },
];

/// WATR — water type.
pub static WATR_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"WATR"), name: "Water", members: &WATR_MEMBERS };


static LSCR_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    DESC_DEF,
    SubRecordDef {
        sig: Signature(*b"NNAM"),
        name: "Loading Screen NIF",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"SNAM"),
        name: "Camera Path",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
];

/// LSCR — load screen.
pub static LSCR_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"LSCR"), name: "Load Screen", members: &LSCR_MEMBERS };


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

/// LTEX — landscape texture.
pub static LTEX_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"LTEX"), name: "Landscape Texture", members: &LTEX_MEMBERS };


static SCEN_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    VMAD_DEF,
    SubRecordDef {
        sig: Signature(*b"FNAM"),
        name: "Flags",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
    CTDA_DEF,
    SubRecordDef {
        sig: Signature(*b"PNAM"),
        name: "Phase",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
];

/// SCEN — scene.
pub static SCEN_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"SCEN"), name: "Scene", members: &SCEN_MEMBERS };


static OSWP_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"KNAM"),
        name: "Keyword",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"VNAM"),
        name: "Swap Target",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
];

/// OSWP — object swap (replaces one object model with another).
pub static OSWP_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"OSWP"), name: "Object Swap", members: &OSWP_MEMBERS };


static LMSW_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Swap Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// LMSW — layered material swap (material substitution system).
pub static LMSW_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"LMSW"),
    name: "Layered Material Swap",
    members: &LMSW_MEMBERS,
};


static EFSH_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Shader Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"NAM7"),
        name: "Shader Particle Geometry",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
];

/// EFSH — effect shader.
pub static EFSH_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"EFSH"), name: "Effect Shader", members: &EFSH_MEMBERS };


static VOLI_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Lighting Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// VOLI — volumetric lighting volume.
pub static VOLI_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"VOLI"),
    name: "Volumetric Lighting",
    members: &VOLI_MEMBERS,
};


static ADDN_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    OBND_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Node Index",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
];

/// ADDN — addon node.
pub static ADDN_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"ADDN"), name: "Addon Node", members: &ADDN_MEMBERS };


static ARTO_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    OBND_DEF,
    MODL_DEF,
];

/// ARTO — art object.
pub static ARTO_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"ARTO"), name: "Art Object", members: &ARTO_MEMBERS };

// Suppress unused import warning for CTDA_DEF (used in SCEN).
const _: () = {
    let _ = &CTDA_DEF;
};
