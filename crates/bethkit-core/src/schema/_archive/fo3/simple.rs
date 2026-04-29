// SPDX-License-Identifier: Apache-2.0
//!
//! Fallout 3 simple and utility record schemas.
//!
//! These records have few or no game-specific sub-records beyond EDID and
//! optional model / icon fields.

use crate::schema::{FieldDef, FieldType, RecordSchema, SubRecordDef};
use crate::types::Signature;

use super::common::{
    DATA_DEF, DESC_DEF, DNAM_DEF, EDID_DEF, FULL_DEF, ICON_DEF, MICO_DEF, MODL_DEF, MODT_DEF,
};


static TES4_HEDR_FIELDS: [FieldDef; 3] = [
    FieldDef { name: "Version", kind: FieldType::Float32 },
    FieldDef { name: "Num Records", kind: FieldType::UInt32 },
    FieldDef { name: "Next Object ID", kind: FieldType::UInt32 },
];

static TES4_MEMBERS: [SubRecordDef; 5] = [
    SubRecordDef {
        sig: Signature(*b"HEDR"),
        name: "Header",
        required: true,
        repeating: false,
        field: FieldType::Struct(&TES4_HEDR_FIELDS),
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
        name: "Master File",
        required: false,
        repeating: true,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "File Size",
        required: false,
        repeating: false,
        field: FieldType::UInt64,
    },
];

/// TES4 — plugin file header.
pub static TES4_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"TES4"), name: "File Header", members: &TES4_MEMBERS };


static TXST_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"TX00"),
        name: "Diffuse",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"TX01"),
        name: "Normal/Gloss",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
];

/// TXST — texture set.
pub static TXST_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"TXST"), name: "Texture Set", members: &TXST_MEMBERS };


static MICN_MEMBERS: [SubRecordDef; 2] = [EDID_DEF, ICON_DEF];

/// MICN — menu icon (maps an EditorID to a .dds icon path).
pub static MICN_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"MICN"), name: "Menu Icon", members: &MICN_MEMBERS };


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

/// GLOB — global variable (script-accessible float/long value).
pub static GLOB_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"GLOB"), name: "Global Variable", members: &GLOB_MEMBERS };


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


static AVIF_MEMBERS: [SubRecordDef; 3] = [EDID_DEF, FULL_DEF, DESC_DEF];

/// AVIF — actor value information record.
pub static AVIF_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"AVIF"), name: "Actor Value Info", members: &AVIF_MEMBERS };


static VTYP_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Flags",
        required: false,
        repeating: false,
        field: FieldType::UInt8,
    },
];

/// VTYP — voice type.
pub static VTYP_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"VTYP"), name: "Voice Type", members: &VTYP_MEMBERS };


static CAMS_MEMBERS: [SubRecordDef; 3] = [EDID_DEF, MODL_DEF, DATA_DEF];

/// CAMS — camera shot definition.
pub static CAMS_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"CAMS"), name: "Camera Shot", members: &CAMS_MEMBERS };


static CPTH_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"ANAM"),
        name: "Related Camera Shots",
        required: false,
        repeating: true,
        field: FieldType::FormId,
    },
    DATA_DEF,
];

/// CPTH — camera path.
pub static CPTH_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"CPTH"), name: "Camera Path", members: &CPTH_MEMBERS };


static ASPC_MEMBERS: [SubRecordDef; 3] = [EDID_DEF, MODL_DEF, DATA_DEF];

/// ASPC — acoustic space.
pub static ASPC_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"ASPC"), name: "Acoustic Space", members: &ASPC_MEMBERS };


static IMGS_MEMBERS: [SubRecordDef; 2] = [EDID_DEF, DATA_DEF];

/// IMGS — image space (post-processing parameters).
pub static IMGS_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"IMGS"), name: "Image Space", members: &IMGS_MEMBERS };


static IMAD_MEMBERS: [SubRecordDef; 2] = [EDID_DEF, DATA_DEF];

/// IMAD — image space adapter (animated image space transitions).
pub static IMAD_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"IMAD"),
    name: "Image Space Adapter",
    members: &IMAD_MEMBERS,
};


static LGTM_MEMBERS: [SubRecordDef; 2] = [EDID_DEF, DATA_DEF];

/// LGTM — lighting template.
pub static LGTM_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"LGTM"),
    name: "Lighting Template",
    members: &LGTM_MEMBERS,
};


static MUSC_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"FNAM"),
        name: "File Name",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
];

/// MUSC — music type.
pub static MUSC_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"MUSC"), name: "Music Type", members: &MUSC_MEMBERS };


static ANIO_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    MODL_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Animation ID",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
];

/// ANIO — animated object.
pub static ANIO_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"ANIO"), name: "Animated Object", members: &ANIO_MEMBERS };


static NAVI_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"NVER"),
        name: "Version",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
];

/// NAVI — navmesh info map.
pub static NAVI_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"NAVI"), name: "Navmesh Info Map", members: &NAVI_MEMBERS };


static DEBR_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Debris Data",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
    MODL_DEF,
];

/// DEBR — debris.
pub static DEBR_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"DEBR"), name: "Debris", members: &DEBR_MEMBERS };


static IDLM_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"OBND"),
        name: "Object Bounds",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"IDLF"),
        name: "Flags",
        required: false,
        repeating: false,
        field: FieldType::UInt8,
    },
    SubRecordDef {
        sig: Signature(*b"IDLA"),
        name: "Animations",
        required: false,
        repeating: true,
        field: FieldType::FormId,
    },
];

/// IDLM — idle marker.
pub static IDLM_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"IDLM"), name: "Idle Marker", members: &IDLM_MEMBERS };


static FLST_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"LNAM"),
        name: "Form",
        required: false,
        repeating: true,
        field: FieldType::FormId,
    },
];

/// FLST — FormID list.
pub static FLST_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"FLST"), name: "Form List", members: &FLST_MEMBERS };


static DOBJ_MEMBERS: [SubRecordDef; 2] = [EDID_DEF, DATA_DEF];

/// DOBJ — default object manager.
pub static DOBJ_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"DOBJ"),
    name: "Default Object Manager",
    members: &DOBJ_MEMBERS,
};


static MESG_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    DESC_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"INAM"),
        name: "Icon",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    DATA_DEF,
];

/// MESG — message box / notification.
pub static MESG_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"MESG"), name: "Message", members: &MESG_MEMBERS };


static EFSH_MEMBERS: [SubRecordDef; 4] = [EDID_DEF, ICON_DEF, MICO_DEF, DATA_DEF];

/// EFSH — effect shader.
pub static EFSH_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"EFSH"), name: "Effect Shader", members: &EFSH_MEMBERS };


static SCOL_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"OBND"),
        name: "Object Bounds",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    MODL_DEF,
    SubRecordDef {
        sig: Signature(*b"ONAM"),
        name: "Parts",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
];

/// SCOL — static collection.
pub static SCOL_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"SCOL"),
    name: "Static Collection",
    members: &SCOL_MEMBERS,
};


static RGDL_MEMBERS: [SubRecordDef; 3] = [EDID_DEF, FULL_DEF, DATA_DEF];

/// RGDL — ragdoll physics definition.
pub static RGDL_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"RGDL"), name: "Ragdoll", members: &RGDL_MEMBERS };


static RADS_MEMBERS: [SubRecordDef; 2] = [EDID_DEF, DATA_DEF];

/// RADS — radiation stage definition.
pub static RADS_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"RADS"),
    name: "Radiation Stage",
    members: &RADS_MEMBERS,
};


static CLMT_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"WLST"),
        name: "Weather List",
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
    DATA_DEF,
];

/// CLMT — climate (weather playlist and sun settings).
pub static CLMT_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"CLMT"), name: "Climate", members: &CLMT_MEMBERS };


static SCPT_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"SCHR"),
        name: "Script Header",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"SCDA"),
        name: "Compiled Script",
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

/// SCPT — compiled Papyrus / Fallout script record.
pub static SCPT_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"SCPT"), name: "Script", members: &SCPT_MEMBERS };


static ECZN_MEMBERS: [SubRecordDef; 2] = [EDID_DEF, DATA_DEF];

/// ECZN — encounter zone.
pub static ECZN_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"ECZN"), name: "Encounter Zone", members: &ECZN_MEMBERS };


static WTHR_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"CNAM"),
        name: "Lower Layer",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Upper Layer",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    MODL_DEF,
    DATA_DEF,
];

/// WTHR — weather type definition.
pub static WTHR_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"WTHR"), name: "Weather", members: &WTHR_MEMBERS };

// NOTE: suppress unused import warning for DNAM_DEF / MODT_DEF
const _: () = {
    let _ = &DNAM_DEF;
    let _ = &MODT_DEF;
};
