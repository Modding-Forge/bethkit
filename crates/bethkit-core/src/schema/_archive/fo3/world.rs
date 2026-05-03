// SPDX-License-Identifier: Apache-2.0
//!
//! Fallout 3 world / environment record schemas.
//!
//! Covers activators, containers, furniture, lights, statics, cells,
//! worldspaces, and terrain data records.

use crate::schema::{RecordSchema, SubRecordDef, FieldType};
use crate::types::Signature;

use super::common::{
    DATA_DEF, DESC_DEF, EDID_DEF, FULL_DEF, MODL_DEF, MODT_DEF, OBND_DEF, SCRI_DEF,
};


static ACTI_MEMBERS: [SubRecordDef; 7] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    MODT_DEF,
    SCRI_DEF,
    SubRecordDef {
        sig: Signature(*b"SNAM"),
        name: "Sound - Activate",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
];

/// ACTI — activator (interactive world object with a script).
pub static ACTI_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"ACTI"), name: "Activator", members: &ACTI_MEMBERS };


static TACT_MEMBERS: [SubRecordDef; 6] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    MODT_DEF,
    SCRI_DEF,
];

/// TACT — talking activator (activator that triggers dialogue).
pub static TACT_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"TACT"),
    name: "Talking Activator",
    members: &TACT_MEMBERS,
};


static DOOR_MEMBERS: [SubRecordDef; 8] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    MODT_DEF,
    SCRI_DEF,
    SubRecordDef {
        sig: Signature(*b"SNAM"),
        name: "Sound - Open",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    DATA_DEF,
];

/// DOOR — interior / exterior door.
pub static DOOR_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"DOOR"), name: "Door", members: &DOOR_MEMBERS };


static CONT_MEMBERS: [SubRecordDef; 9] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    MODT_DEF,
    SCRI_DEF,
    SubRecordDef {
        sig: Signature(*b"CNTO"),
        name: "Item",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"SNAM"),
        name: "Sound - Open",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    DATA_DEF,
];

/// CONT — container (chest, locker, box).
pub static CONT_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"CONT"), name: "Container", members: &CONT_MEMBERS };


static FURN_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    DATA_DEF,
];

/// FURN — furniture (chair, bed, workbench, etc.).
pub static FURN_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"FURN"), name: "Furniture", members: &FURN_MEMBERS };


static LIGH_MEMBERS: [SubRecordDef; 8] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    MODT_DEF,
    SCRI_DEF,
    SubRecordDef {
        sig: Signature(*b"ICON"),
        name: "Icon",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    DATA_DEF,
];

/// LIGH — dynamic light source.
pub static LIGH_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"LIGH"), name: "Light", members: &LIGH_MEMBERS };


static STAT_MEMBERS: [SubRecordDef; 3] = [EDID_DEF, OBND_DEF, MODL_DEF];

/// STAT — static decorative mesh.
pub static STAT_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"STAT"), name: "Static", members: &STAT_MEMBERS };


static MSTT_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    DATA_DEF,
];

/// MSTT — moveable static (physics-enabled decorative mesh).
pub static MSTT_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"MSTT"),
    name: "Moveable Static",
    members: &MSTT_MEMBERS,
};


static GRAS_MEMBERS: [SubRecordDef; 4] = [EDID_DEF, OBND_DEF, MODL_DEF, DATA_DEF];

/// GRAS — grass (procedural ground cover).
pub static GRAS_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"GRAS"), name: "Grass", members: &GRAS_MEMBERS };


static TREE_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    OBND_DEF,
    MODL_DEF,
    MODT_DEF,
    DATA_DEF,
];

/// TREE — billboard tree.
pub static TREE_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"TREE"), name: "Tree", members: &TREE_MEMBERS };


static CELL_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Flags",
        required: false,
        repeating: false,
        field: FieldType::UInt8,
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
];

/// CELL — interior or exterior cell (map grid square).
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
        sig: Signature(*b"CNAM"),
        name: "Climate",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"NAM2"),
        name: "Water",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    MODL_DEF,
    DATA_DEF,
];

/// WRLD — worldspace (overworld map area).
pub static WRLD_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"WRLD"), name: "Worldspace", members: &WRLD_MEMBERS };


static REGN_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"ICON"),
        name: "Map Icon",
        required: false,
        repeating: false,
        field: FieldType::ZString,
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
    SubRecordDef {
        sig: Signature(*b"RDAT"),
        name: "Region Data",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
];

/// REGN — region (polygon area used for weather / sound / spawn rules).
pub static REGN_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"REGN"), name: "Region", members: &REGN_MEMBERS };


static NAVM_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Navmesh Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// NAVM — navigation mesh (pathfinding data for a cell).
pub static NAVM_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"NAVM"), name: "Navmesh", members: &NAVM_MEMBERS };


static WATR_MEMBERS: [SubRecordDef; 4] = [EDID_DEF, FULL_DEF, MODL_DEF, DATA_DEF];

/// WATR — water type definition.
pub static WATR_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"WATR"), name: "Water", members: &WATR_MEMBERS };


static LSCR_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"ICON"),
        name: "Image",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    DESC_DEF,
    SubRecordDef {
        sig: Signature(*b"LNAM"),
        name: "Location",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
];

/// LSCR — load screen definition (image and hint text).
pub static LSCR_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"LSCR"), name: "Load Screen", members: &LSCR_MEMBERS };


static LTEX_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"ICON"),
        name: "Texture",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"HNAM"),
        name: "Havok Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"SNAM"),
        name: "Texture Specular",
        required: false,
        repeating: false,
        field: FieldType::UInt8,
    },
    SubRecordDef {
        sig: Signature(*b"GNAM"),
        name: "Grass",
        required: false,
        repeating: true,
        field: FieldType::FormId,
    },
];

/// LTEX — landscape texture assignment.
pub static LTEX_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"LTEX"),
    name: "Landscape Texture",
    members: &LTEX_MEMBERS,
};


static PWAT_MEMBERS: [SubRecordDef; 3] = [EDID_DEF, OBND_DEF, MODL_DEF];

/// PWAT — placeable water (a static mesh that acts as water surface).
pub static PWAT_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"PWAT"),
    name: "Placeable Water",
    members: &PWAT_MEMBERS,
};


static LAND_MEMBERS: [SubRecordDef; 3] = [
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Landscape Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
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
];

/// LAND — landscape terrain data (height map, vertex colours, texture layers).
pub static LAND_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"LAND"), name: "Landscape", members: &LAND_MEMBERS };


static ADDN_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    OBND_DEF,
    MODL_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Node Index",
        required: false,
        repeating: false,
        field: FieldType::Int32,
    },
];

/// ADDN - addon node (attaches a particle or mesh to a skeleton node).
pub static ADDN_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"ADDN"), name: "Addon Node", members: &ADDN_MEMBERS };


static PLYR_MEMBERS: [SubRecordDef; 1] = [EDID_DEF];

/// PLYR - player reference singleton (only one per plugin).
pub static PLYR_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"PLYR"),
    name: "Player Reference",
    members: &PLYR_MEMBERS,
};
