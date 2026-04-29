// SPDX-License-Identifier: Apache-2.0
//!
//! Schema definitions for simple / utility SSE record types.
//!
//! Covers: TES4, KYWD, AACT, TXST, GLOB, GMST, VTYP, LCRT, MATT, COLL,
//! CLFM, REVB, SHOU, WOOP, ASTP, EQUP, RELA, DEBR, LGTM, DOBJ, FLST,
//! IDLM, ANIO, HDPT (appearance), LENS, CLDC, SCOL, PLYR.

use crate::schema::{FieldDef, FieldType, FlagsDef, RecordSchema, SubRecordDef};
use crate::types::Signature;

use super::common::{DESC_DEF, EDID_DEF, FULL_DEF, ICON_DEF, MODL_DEF, OBND_DEF};


static TES4_HEDR_FIELDS: [FieldDef; 3] = [
    FieldDef { name: "Version", kind: FieldType::Float32 },
    FieldDef { name: "Number of Records", kind: FieldType::UInt32 },
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
        name: "Master File Data",
        required: false,
        repeating: true,
        field: FieldType::UInt64,
    },
];

/// TES4 — Plugin header record.
pub static TES4_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"TES4"),
    name: "Plugin Header",
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

/// KYWD — Keyword record.
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

/// AACT — Action record.
pub static AACT_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"AACT"),
    name: "Action",
    members: &AACT_MEMBERS,
};


static TXST_MEMBERS: [SubRecordDef; 10] = [
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
    SubRecordDef {
        sig: Signature(*b"TX02"),
        name: "Environment Mask/Subsurface Tint",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"TX03"),
        name: "Glow/Detail Map",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"TX04"),
        name: "Height",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"TX05"),
        name: "Environment",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"TX06"),
        name: "Multilayer",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"TX07"),
        name: "Backlight Mask/Specular",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
];

/// TXST — Texture set.
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
        required: true,
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

/// GLOB — Global variable.
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
        required: true,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// GMST — Game setting (value type depends on EDID prefix: b=bool, i=int,
/// f=float, s=string).
pub static GMST_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"GMST"),
    name: "Game Setting",
    members: &GMST_MEMBERS,
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

/// VTYP — Voice type.
pub static VTYP_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"VTYP"),
    name: "Voice Type",
    members: &VTYP_MEMBERS,
};


static LCRT_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"CNAM"),
        name: "Color",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
];

/// LCRT — Location reference type.
pub static LCRT_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"LCRT"),
    name: "Location Reference Type",
    members: &LCRT_MEMBERS,
};


static MATT_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"PNAM"),
        name: "Material Parent",
        required: false,
        repeating: false,
        field: FieldType::FormIdTyped(&[Signature(*b"MATT")]),
    },
    SubRecordDef {
        sig: Signature(*b"MNAM"),
        name: "Material Name",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"CNAM"),
        name: "Havok Display Color",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
];

/// MATT — Material type.
pub static MATT_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"MATT"),
    name: "Material Type",
    members: &MATT_MEMBERS,
};


/// Interactable flags stored in COLL.GNAM.
static COLL_GNAM_FLAGS: FlagsDef = FlagsDef {
    name: "CollisionInteractableFlags",
    bits: &[(0, "TriggerVolume"), (1, "Sensor"), (2, "NavmeshObstacle")],
};

static COLL_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    DESC_DEF,
    SubRecordDef {
        sig: Signature(*b"BNAM"),
        name: "Index",
        required: true,
        repeating: false,
        field: FieldType::UInt32,
    },
    SubRecordDef {
        sig: Signature(*b"FNAM"),
        name: "Debug Color",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
    SubRecordDef {
        sig: Signature(*b"GNAM"),
        name: "Interactable",
        required: false,
        repeating: true,
        field: FieldType::Flags(&COLL_GNAM_FLAGS),
    },
];

/// COLL — Collision layer.
pub static COLL_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"COLL"),
    name: "Collision Layer",
    members: &COLL_MEMBERS,
};


static CLFM_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"CNAM"),
        name: "Color",
        required: true,
        repeating: false,
        field: FieldType::UInt32,
    },
];

/// CLFM — Color form (named color).
pub static CLFM_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"CLFM"),
    name: "Color",
    members: &CLFM_MEMBERS,
};


static REVB_DATA_FIELDS: [FieldDef; 8] = [
    FieldDef { name: "Decay Time", kind: FieldType::UInt16 },
    FieldDef { name: "HF Reference", kind: FieldType::UInt16 },
    FieldDef { name: "Room Filter", kind: FieldType::Int8 },
    FieldDef { name: "Room HF Filter", kind: FieldType::Int8 },
    FieldDef { name: "Reflections", kind: FieldType::Int8 },
    FieldDef { name: "Reverb Amp", kind: FieldType::Int8 },
    FieldDef { name: "Decay HF Ratio", kind: FieldType::UInt8 },
    FieldDef { name: "Reflections Delay", kind: FieldType::UInt8 },
];

static REVB_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&REVB_DATA_FIELDS),
    },
];

/// REVB — Reverb parameters.
pub static REVB_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"REVB"),
    name: "Reverb Parameters",
    members: &REVB_MEMBERS,
};


static SHOU_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    FULL_DEF,
    DESC_DEF,
    SubRecordDef {
        sig: Signature(*b"MDOB"),
        name: "Menu Display Object",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
];

/// SHOU — Dragon shout.
pub static SHOU_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"SHOU"),
    name: "Shout",
    members: &SHOU_MEMBERS,
};


static WOOP_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"TNAM"),
        name: "Translation",
        required: false,
        repeating: false,
        field: FieldType::LString,
    },
];

/// WOOP — Word of power.
pub static WOOP_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"WOOP"),
    name: "Word of Power",
    members: &WOOP_MEMBERS,
};


static ASTP_MEMBERS: [SubRecordDef; 5] = [
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
        sig: Signature(*b"MCHT"),
        name: "Male Child Title",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"FCHT"),
        name: "Female Child Title",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
];

/// ASTP — Association type.
pub static ASTP_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"ASTP"),
    name: "Association Type",
    members: &ASTP_MEMBERS,
};


static EQUP_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"PNAM"),
        name: "Slot Parents",
        required: false,
        repeating: true,
        field: FieldType::FormIdTyped(&[Signature(*b"EQUP")]),
    },
];

/// EQUP — Equip slot.
pub static EQUP_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"EQUP"),
    name: "Equip Slot",
    members: &EQUP_MEMBERS,
};


static RELA_DATA_FIELDS: [FieldDef; 4] = [
    FieldDef { name: "Parent", kind: FieldType::FormIdTyped(&[Signature(*b"NPC_")]) },
    FieldDef { name: "Child", kind: FieldType::FormIdTyped(&[Signature(*b"NPC_")]) },
    FieldDef { name: "Rank", kind: FieldType::UInt16 },
    FieldDef { name: "Flags", kind: FieldType::UInt8 },
];

static RELA_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&RELA_DATA_FIELDS),
    },
];

/// RELA — Relationship.
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
        required: true,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// DEBR — Debris.
pub static DEBR_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"DEBR"),
    name: "Debris",
    members: &DEBR_MEMBERS,
};


static LGTM_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: true,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// LGTM — Lighting template.
pub static LGTM_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"LGTM"),
    name: "Lighting Template",
    members: &LGTM_MEMBERS,
};


static DOBJ_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Default Objects",
        required: true,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// DOBJ — Default object manager.
pub static DOBJ_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"DOBJ"),
    name: "Default Object Manager",
    members: &DOBJ_MEMBERS,
};


static FLST_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"LNAM"),
        name: "FormIDs",
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


static IDLM_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    OBND_DEF,
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
        field: FieldType::FormIdTyped(&[Signature(*b"IDLE")]),
    },
];

/// IDLM — Idle marker.
pub static IDLM_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"IDLM"),
    name: "Idle Marker",
    members: &IDLM_MEMBERS,
};


static ANIO_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    MODL_DEF,
    SubRecordDef {
        sig: Signature(*b"BNAM"),
        name: "Unload Event",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
];

/// ANIO — Animated object.
pub static ANIO_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"ANIO"),
    name: "Animated Object",
    members: &ANIO_MEMBERS,
};


static HDPT_MEMBERS: [SubRecordDef; 7] = [
    EDID_DEF,
    FULL_DEF,
    MODL_DEF,
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
    SubRecordDef {
        sig: Signature(*b"HNAM"),
        name: "Extra Parts",
        required: false,
        repeating: true,
        field: FieldType::FormIdTyped(&[Signature(*b"HDPT")]),
    },
    SubRecordDef {
        sig: Signature(*b"NAM0"),
        name: "Base Texture",
        required: false,
        repeating: false,
        field: FieldType::FormIdTyped(&[Signature(*b"TXST")]),
    },
];

/// HDPT — Head part.
pub static HDPT_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"HDPT"),
    name: "Head Part",
    members: &HDPT_MEMBERS,
};


static LCTN_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"KWDA"),
        name: "Keywords",
        required: false,
        repeating: true,
        field: FieldType::FormIdTyped(&[Signature(*b"KYWD")]),
    },
    SubRecordDef {
        sig: Signature(*b"PNAM"),
        name: "Parent Location",
        required: false,
        repeating: false,
        field: FieldType::FormIdTyped(&[Signature(*b"LCTN")]),
    },
    SubRecordDef {
        sig: Signature(*b"MNAM"),
        name: "Music",
        required: false,
        repeating: false,
        field: FieldType::FormIdTyped(&[Signature(*b"MUSC")]),
    },
];

/// LCTN — Location.
pub static LCTN_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"LCTN"),
    name: "Location",
    members: &LCTN_MEMBERS,
};


static MESG_MEMBERS: [SubRecordDef; 6] = [
    EDID_DEF,
    DESC_DEF,
    FULL_DEF,
    ICON_DEF,
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Message Box",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
    SubRecordDef {
        sig: Signature(*b"ITXT"),
        name: "Button Text",
        required: false,
        repeating: true,
        field: FieldType::LString,
    },
];

/// MESG — Message (popup / notification).
pub static MESG_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"MESG"),
    name: "Message",
    members: &MESG_MEMBERS,
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

/// AVIF — Actor value information.
pub static AVIF_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"AVIF"),
    name: "Actor Value Information",
    members: &AVIF_MEMBERS,
};


static CAMS_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    MODL_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: true,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// CAMS — Camera shot.
pub static CAMS_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"CAMS"),
    name: "Camera Shot",
    members: &CAMS_MEMBERS,
};


static CPTH_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"ANAM"),
        name: "Related Camera Paths",
        required: false,
        repeating: true,
        field: FieldType::FormIdTyped(&[Signature(*b"CPTH")]),
    },
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: true,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// CPTH — Camera path.
pub static CPTH_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"CPTH"),
    name: "Camera Path",
    members: &CPTH_MEMBERS,
};


static MOVT_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"MNAM"),
        name: "Name",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"SPED"),
        name: "Speeds",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"INAM"),
        name: "Anim Change Thresholds",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"JNAM"),
        name: "Default Flags",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// MOVT — Movement type.
pub static MOVT_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"MOVT"),
    name: "Movement Type",
    members: &MOVT_MEMBERS,
};


static DUAL_DATA_FIELDS: [FieldDef; 6] = [
    FieldDef { name: "Projectile", kind: FieldType::FormIdTyped(&[Signature(*b"PROJ")]) },
    FieldDef { name: "Explosion", kind: FieldType::FormIdTyped(&[Signature(*b"EXPL")]) },
    FieldDef { name: "Effect Shader", kind: FieldType::FormIdTyped(&[Signature(*b"EFSH")]) },
    FieldDef { name: "Hit Effect Art", kind: FieldType::FormIdTyped(&[Signature(*b"ARTO")]) },
    FieldDef { name: "Impact Data Set", kind: FieldType::FormIdTyped(&[Signature(*b"IPDS")]) },
    FieldDef { name: "Inherit Scale", kind: FieldType::UInt32 },
];

static DUAL_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&DUAL_DATA_FIELDS),
    },
];

/// DUAL — Dual cast data.
pub static DUAL_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"DUAL"),
    name: "Dual Cast Data",
    members: &DUAL_MEMBERS,
};


/// PLYR — Player reference (minimal).
pub static PLYR_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"PLYR"),
    name: "Player Reference",
    members: &[EDID_DEF],
};

// NOTE: FieldType::UInt64 is needed for TES4 DATA (file size); declare it in
// FieldType enum extension below.
