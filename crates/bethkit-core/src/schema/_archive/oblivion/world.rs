// SPDX-License-Identifier: Apache-2.0
//!
//! Oblivion world and environment record schemas.
//!
//! Covers ACTI, TREE, FLOR, CELL, WRLD, REGN.

use crate::schema::{FieldDef, FieldType, RecordSchema, SubRecordDef};
use crate::types::Signature;

use super::common::{EDID_DEF, FULL_DEF, MODL_DEF, SCRI_DEF, SNAM_DEF};


static ACTI_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    FULL_DEF,
    MODL_DEF,
    SCRI_DEF,
    SNAM_DEF,
];

/// ACTI — activator.
pub static ACTI_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"ACTI"), name: "Activator", members: &ACTI_MEMBERS };


static TREE_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    MODL_DEF,
    SubRecordDef {
        sig: Signature(*b"ICON"),
        name: "Leaf Texture",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Tree Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// TREE — tree.
pub static TREE_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"TREE"), name: "Tree", members: &TREE_MEMBERS };


static FLOR_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    FULL_DEF,
    MODL_DEF,
    SCRI_DEF,
    SubRecordDef {
        sig: Signature(*b"PFIG"),
        name: "Ingredient",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
];

/// FLOR — flora.
pub static FLOR_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"FLOR"), name: "Flora", members: &FLOR_MEMBERS };


static CELL_MEMBERS: [SubRecordDef; 6] = [
    EDID_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Flags",
        required: true,
        repeating: false,
        field: FieldType::UInt8,
    },
    SubRecordDef {
        sig: Signature(*b"XCLL"),
        name: "Lighting",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"XCLC"),
        name: "Grid",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"XCMT"),
        name: "Music",
        required: false,
        repeating: false,
        field: FieldType::UInt8,
    },
];

/// CELL — cell.
pub static CELL_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"CELL"), name: "Cell", members: &CELL_MEMBERS };


static WRLD_MEMBERS: [SubRecordDef; 6] = [
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
        sig: Signature(*b"MNAM"),
        name: "Map Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// WRLD — worldspace.
pub static WRLD_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"WRLD"), name: "Worldspace", members: &WRLD_MEMBERS };


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
pub static REGN_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"REGN"), name: "Region", members: &REGN_MEMBERS };

// Suppress lint for unused FieldDef import check.
const _: () = { let _: &[FieldDef] = &[]; };
