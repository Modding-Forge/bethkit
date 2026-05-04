// SPDX-License-Identifier: Apache-2.0
//!
//! Fallout 4 simple / standalone record schemas.
//!
//! Contains records whose payload is small or consists of a single typed
//! field, as well as FO4-specific record types (OMOD, CMPO, INNR, MSWP,
//! LAYR, SCCO, etc.) that have no Skyrim equivalent.

use crate::schema::{FieldDef, FieldType, RecordSchema, SubRecordDef};
use crate::types::Signature;

use super::common::{DESC_DEF, EDID_DEF, FULL_DEF, OBND_DEF};
use super::enums::{FO4_BIPED_OBJECT_ENUM, NOTE_TYPE_ENUM, OMOD_PROPERTY_ENUM};

static TES4_HEDR_FIELDS: [FieldDef; 3] = [
    FieldDef {
        name: "Version",
        kind: FieldType::Float32,
    },
    FieldDef {
        name: "Num Records",
        kind: FieldType::UInt32,
    },
    FieldDef {
        name: "Next Object ID",
        kind: FieldType::UInt32,
    },
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
        sig: Signature(*b"ONAM"),
        name: "Overridden Forms",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// TES4 — plugin file header.
pub static TES4_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"TES4"),
    name: "Plugin File Header",
    members: &TES4_MEMBERS,
};

static KYWD_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"CNAM"),
        name: "Color",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
];

/// KYWD — keyword.
pub static KYWD_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"KYWD"),
    name: "Keyword",
    members: &KYWD_MEMBERS,
};

static AACT_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"CNAM"),
        name: "Color",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
];

/// AACT — action.
pub static AACT_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"AACT"),
    name: "Action",
    members: &AACT_MEMBERS,
};

static TXST_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    OBND_DEF,
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
pub static TXST_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"TXST"),
    name: "Texture Set",
    members: &TXST_MEMBERS,
};

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
pub static GLOB_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"GLOB"),
    name: "Global Variable",
    members: &GLOB_MEMBERS,
};

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
pub static GMST_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"GMST"),
    name: "Game Setting",
    members: &GMST_MEMBERS,
};

static AVIF_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    FULL_DEF,
    DESC_DEF,
    SubRecordDef {
        sig: Signature(*b"ANAM"),
        name: "Abbreviation",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
];

/// AVIF — actor value information.
pub static AVIF_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"AVIF"),
    name: "Actor Value Information",
    members: &AVIF_MEMBERS,
};

static LCRT_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"CNAM"),
        name: "Color",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// LCRT — location reference type.
pub static LCRT_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"LCRT"),
    name: "Location Reference Type",
    members: &LCRT_MEMBERS,
};

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
pub static VTYP_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"VTYP"),
    name: "Voice Type",
    members: &VTYP_MEMBERS,
};

static MATT_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"PNAM"),
        name: "Parent",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"CNAM"),
        name: "Material",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// MATT — material type.
pub static MATT_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"MATT"),
    name: "Material Type",
    members: &MATT_MEMBERS,
};

static COLL_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    DESC_DEF,
    SubRecordDef {
        sig: Signature(*b"BNAM"),
        name: "Layer Index",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
    SubRecordDef {
        sig: Signature(*b"FNAM"),
        name: "Flags",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
];

/// COLL — collision layer.
pub static COLL_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"COLL"),
    name: "Collision Layer",
    members: &COLL_MEMBERS,
};

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
pub static FLST_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"FLST"),
    name: "FormID List",
    members: &FLST_MEMBERS,
};

static LCTN_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"PNAM"),
        name: "Parent Location",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
];

/// LCTN — location.
pub static LCTN_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"LCTN"),
    name: "Location",
    members: &LCTN_MEMBERS,
};

static MESG_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    DESC_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Flags",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
];

/// MESG — message.
pub static MESG_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"MESG"),
    name: "Message",
    members: &MESG_MEMBERS,
};

static DOBJ_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Objects",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// DOBJ — default object manager.
pub static DOBJ_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"DOBJ"),
    name: "Default Object Manager",
    members: &DOBJ_MEMBERS,
};

static LGTM_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// LGTM — lighting template.
pub static LGTM_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"LGTM"),
    name: "Lighting Template",
    members: &LGTM_MEMBERS,
};

static IDLM_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    OBND_DEF,
    SubRecordDef {
        sig: Signature(*b"IDLF"),
        name: "Flags",
        required: false,
        repeating: false,
        field: FieldType::UInt8,
    },
];

/// IDLM — idle marker.
pub static IDLM_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"IDLM"),
    name: "Idle Marker",
    members: &IDLM_MEMBERS,
};

static ANIO_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"BNAM"),
        name: "Default Animation",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
];

/// ANIO — animated object.
pub static ANIO_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"ANIO"),
    name: "Animated Object",
    members: &ANIO_MEMBERS,
};

static HDPT_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"MODL"),
        name: "Model Filename",
        required: false,
        repeating: false,
        field: FieldType::ZString,
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
        name: "Type",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
];

/// HDPT — head part.
pub static HDPT_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"HDPT"),
    name: "Head Part",
    members: &HDPT_MEMBERS,
};

static MOVT_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"SPED"),
        name: "Speed Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// MOVT — movement type.
pub static MOVT_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"MOVT"),
    name: "Movement Type",
    members: &MOVT_MEMBERS,
};

static EQUP_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"PNAM"),
        name: "Parent Slot",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Use All Parents",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
];

/// EQUP — equip slot.
pub static EQUP_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"EQUP"),
    name: "Equip Slot",
    members: &EQUP_MEMBERS,
};

static RELA_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// RELA — relationship.
pub static RELA_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"RELA"),
    name: "Relationship",
    members: &RELA_MEMBERS,
};

static DEBR_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// DEBR — debris.
pub static DEBR_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"DEBR"),
    name: "Debris",
    members: &DEBR_MEMBERS,
};

static ASTP_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"MPRT"),
        name: "Male Parent Title",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"FPRT"),
        name: "Female Parent Title",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Flags",
        required: false,
        repeating: false,
        field: FieldType::UInt8,
    },
];

/// ASTP — association type.
pub static ASTP_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"ASTP"),
    name: "Association Type",
    members: &ASTP_MEMBERS,
};

static CAMS_MEMBERS: [SubRecordDef; 3] = [
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
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// CAMS — camera shot.
pub static CAMS_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"CAMS"),
    name: "Camera Shot",
    members: &CAMS_MEMBERS,
};

static CPTH_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// CPTH — camera path.
pub static CPTH_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"CPTH"),
    name: "Camera Path",
    members: &CPTH_MEMBERS,
};

static LAYR_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"PNAM"),
        name: "Parent Layer",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
];

/// LAYR — layer (Fallout 4 specific; organises references into named layers).
pub static LAYR_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"LAYR"),
    name: "Layer",
    members: &LAYR_MEMBERS,
};

static SCCO_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"QNAM"),
        name: "Quest",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
];

/// SCCO — scene collection (Fallout 4 specific; groups scenes into a 2-D
/// grid associated with a quest).
pub static SCCO_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"SCCO"),
    name: "Scene Collection",
    members: &SCCO_MEMBERS,
};

static DFOB_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Object",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
];

/// DFOB — default object (Fallout 4 specific).
pub static DFOB_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"DFOB"),
    name: "Default Object",
    members: &DFOB_MEMBERS,
};

static KSSM_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// KSSM — sound keyword mapping (Fallout 4 specific).
pub static KSSM_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"KSSM"),
    name: "Sound Keyword Mapping",
    members: &KSSM_MEMBERS,
};

static NOTE_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"MODL"),
        name: "Model Filename",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Type",
        required: false,
        repeating: false,
        field: FieldType::Enum(&NOTE_TYPE_ENUM),
    },
];

/// NOTE — note / holotape (Fallout 4 specific).
pub static NOTE_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"NOTE"),
    name: "Note",
    members: &NOTE_MEMBERS,
};

static OVIS_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// OVIS — object visibility manager (Fallout 4 specific).
pub static OVIS_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"OVIS"),
    name: "Object Visibility Manager",
    members: &OVIS_MEMBERS,
};

static RFGP_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// RFGP — reference group (Fallout 4 specific).
pub static RFGP_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"RFGP"),
    name: "Reference Group",
    members: &RFGP_MEMBERS,
};

static STAG_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// STAG — animation sound tag set (Fallout 4 specific).
pub static STAG_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"STAG"),
    name: "Animation Sound Tag Set",
    members: &STAG_MEMBERS,
};

static BNDS_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    OBND_DEF,
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// BNDS — bendable spline (Fallout 4 specific).
pub static BNDS_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"BNDS"),
    name: "Bendable Spline",
    members: &BNDS_MEMBERS,
};

static GDRY_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// GDRY — god rays (Fallout 4 specific).
pub static GDRY_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"GDRY"),
    name: "God Rays",
    members: &GDRY_MEMBERS,
};

static NOCM_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// NOCM — navmesh obstacle manager (Fallout 4 specific).
pub static NOCM_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"NOCM"),
    name: "Navmesh Obstacle Manager",
    members: &NOCM_MEMBERS,
};

static PKIN_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// PKIN — pack-in prefab structure (Fallout 4 specific).
pub static PKIN_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"PKIN"),
    name: "Pack-In",
    members: &PKIN_MEMBERS,
};

static SCOL_MEMBERS: [SubRecordDef; 3] = [
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

/// SCOL — static collection (Fallout 4 specific).
pub static SCOL_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"SCOL"),
    name: "Static Collection",
    members: &SCOL_MEMBERS,
};

static SCSN_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// SCSN — audio category snapshot (Fallout 4 specific).
pub static SCSN_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"SCSN"),
    name: "Audio Category Snapshot",
    members: &SCSN_MEMBERS,
};

static INNR_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// INNR — instance naming rules (Fallout 4 specific; drives dynamic weapon /
/// armour names based on attached modifications).
pub static INNR_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"INNR"),
    name: "Instance Naming Rules",
    members: &INNR_MEMBERS,
};

static AMDL_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// AMDL — aim model (Fallout 4 specific; defines weapon sighting behaviour).
pub static AMDL_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"AMDL"),
    name: "Aim Model",
    members: &AMDL_MEMBERS,
};

static AORU_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// AORU — attraction rule (Fallout 4 specific).
pub static AORU_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"AORU"),
    name: "Attraction Rule",
    members: &AORU_MEMBERS,
};

/// PLYR — player reference (singleton; behaves like a special NPC_).
pub static PLYR_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"PLYR"),
    name: "Player Reference",
    members: &[],
};

static DMGT_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// DMGT — damage type (Fallout 4 specific).
pub static DMGT_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"DMGT"),
    name: "Damage Type",
    members: &DMGT_MEMBERS,
};

static TRNS_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// TRNS — transform.
pub static TRNS_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"TRNS"),
    name: "Transform",
    members: &TRNS_MEMBERS,
};

static ZOOM_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// ZOOM — zoom data (Fallout 4 specific; defines scope zoom properties).
pub static ZOOM_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"ZOOM"),
    name: "Zoom Data",
    members: &ZOOM_MEMBERS,
};

static CLFM_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"CNAM"),
        name: "Color/Index",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
    SubRecordDef {
        sig: Signature(*b"FNAM"),
        name: "Flags",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
];

/// CLFM — color form (Fallout 4 specific; named colour with optional remapping
/// index used by the material system).
pub static CLFM_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"CLFM"),
    name: "Color",
    members: &CLFM_MEMBERS,
};

static REVB_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: true,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"ANAM"),
        name: "Reverb Class",
        required: true,
        repeating: false,
        field: FieldType::UInt32,
    },
];

/// REVB — reverb parameters (acoustic reverb preset used by sound categories).
pub static REVB_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"REVB"),
    name: "Reverb Parameters",
    members: &REVB_MEMBERS,
};

static DUAL_DATA_FIELDS: [FieldDef; 6] = [
    FieldDef {
        name: "Projectile",
        kind: FieldType::FormId,
    },
    FieldDef {
        name: "Explosion",
        kind: FieldType::FormId,
    },
    FieldDef {
        name: "Effect Shader",
        kind: FieldType::FormId,
    },
    FieldDef {
        name: "Hit Effect Art",
        kind: FieldType::FormId,
    },
    FieldDef {
        name: "Impact Data Set",
        kind: FieldType::FormId,
    },
    FieldDef {
        name: "Inherit Scale",
        kind: FieldType::UInt32,
    },
];

static DUAL_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    OBND_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&DUAL_DATA_FIELDS),
    },
];

/// DUAL — dual cast data (Fallout 4 specific; defines the overrides used when
/// a spell is dual-cast; notably requires an object bounds unlike the SSE
/// variant).
pub static DUAL_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"DUAL"),
    name: "Dual Cast Data",
    members: &DUAL_MEMBERS,
};

// Suppress dead_code lint for enum not yet referenced by other schema items.
const _: () = {
    let _ = &FO4_BIPED_OBJECT_ENUM;
    let _ = &OMOD_PROPERTY_ENUM;
};
