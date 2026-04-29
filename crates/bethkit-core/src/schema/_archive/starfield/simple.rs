// SPDX-License-Identifier: Apache-2.0
//!
//! Starfield simple / standalone record schemas.
//!
//! Contains records whose payload is small or consists of a single typed
//! field, as well as Starfield-specific record types that have no direct
//! counterpart in earlier games.

use crate::schema::{FieldDef, FieldType, RecordSchema, SubRecordDef};
use crate::types::Signature;

use super::common::{
    CTDA_DEF, DESC_DEF, EDID_DEF, FULL_DEF, KSIZ_DEF, KWDA_DEF, MODL_DEF, OBND_DEF,
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
        sig: Signature(*b"ONAM"),
        name: "Overridden Forms",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// TES4 — plugin file header.
pub static TES4_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"TES4"), name: "Plugin File Header", members: &TES4_MEMBERS };


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
pub static KYWD_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"KYWD"), name: "Keyword", members: &KYWD_MEMBERS };


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
pub static AACT_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"AACT"), name: "Action", members: &AACT_MEMBERS };


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
pub static TXST_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"TXST"), name: "Texture Set", members: &TXST_MEMBERS };


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
pub static VTYP_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"VTYP"), name: "Voice Type", members: &VTYP_MEMBERS };


static MATT_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"PNAM"),
        name: "Parent Material",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"CNAM"),
        name: "Color",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// MATT — material type.
pub static MATT_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"MATT"), name: "Material Type", members: &MATT_MEMBERS };


static COLL_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    DESC_DEF,
    SubRecordDef {
        sig: Signature(*b"BNAM"),
        name: "Layer ID",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
];

/// COLL — collision layer.
pub static COLL_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"COLL"), name: "Collision Layer", members: &COLL_MEMBERS };


static FLST_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"LNAM"),
        name: "FormID",
        required: false,
        repeating: true,
        field: FieldType::FormId,
    },
];

/// FLST — form ID list.
pub static FLST_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"FLST"), name: "FormID List", members: &FLST_MEMBERS };


static LCTN_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    FULL_DEF,
    KSIZ_DEF,
    KWDA_DEF,
];

/// LCTN — location.
pub static LCTN_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"LCTN"), name: "Location", members: &LCTN_MEMBERS };


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
pub static MESG_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"MESG"), name: "Message", members: &MESG_MEMBERS };


static DOBJ_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Data",
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
        name: "Lighting Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// LGTM — lighting template.
pub static LGTM_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"LGTM"), name: "Lighting Template", members: &LGTM_MEMBERS };


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
pub static IDLM_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"IDLM"), name: "Idle Marker", members: &IDLM_MEMBERS };


static ANIO_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    MODL_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Unload Event",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
];

/// ANIO — animated object.
pub static ANIO_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"ANIO"), name: "Animated Object", members: &ANIO_MEMBERS };


static HDPT_MEMBERS: [SubRecordDef; 4] = [
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
];

/// HDPT — head part.
pub static HDPT_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"HDPT"), name: "Head Part", members: &HDPT_MEMBERS };


static MOVT_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"MNAM"),
        name: "Name",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
];

/// MOVT — movement type.
pub static MOVT_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"MOVT"), name: "Movement Type", members: &MOVT_MEMBERS };


static EQUP_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// EQUP — equip type.
pub static EQUP_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"EQUP"), name: "Equip Type", members: &EQUP_MEMBERS };


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
pub static RELA_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"RELA"), name: "Relationship", members: &RELA_MEMBERS };


static DEBR_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
];

/// DEBR — debris.
pub static DEBR_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"DEBR"), name: "Debris", members: &DEBR_MEMBERS };


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
        field: FieldType::UInt32,
    },
];

/// ASTP — association type.
pub static ASTP_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"ASTP"), name: "Association Type", members: &ASTP_MEMBERS };


static CAMS_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    MODL_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Camera Shot Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// CAMS — camera shot.
pub static CAMS_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"CAMS"), name: "Camera Shot", members: &CAMS_MEMBERS };


static CPTH_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    CTDA_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// CPTH — camera path.
pub static CPTH_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"CPTH"), name: "Camera Path", members: &CPTH_MEMBERS };


static LAYR_MEMBERS: [SubRecordDef; 1] = [EDID_DEF];

/// LAYR — layer.
pub static LAYR_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"LAYR"), name: "Layer", members: &LAYR_MEMBERS };


static SCCO_MEMBERS: [SubRecordDef; 1] = [EDID_DEF];

/// SCCO — scene collection.
pub static SCCO_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"SCCO"), name: "Scene Collection", members: &SCCO_MEMBERS };


static NOTE_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Type",
        required: false,
        repeating: false,
        field: FieldType::UInt8,
    },
];

/// NOTE — note.
pub static NOTE_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"NOTE"), name: "Note", members: &NOTE_MEMBERS };


static PLYR_MEMBERS: [SubRecordDef; 0] = [];

/// PLYR — player reference (singleton, no subrecords).
pub static PLYR_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"PLYR"), name: "Player Reference", members: &PLYR_MEMBERS };


static INNR_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"UNAM"),
        name: "Target",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
];

/// INNR — instance naming rules.
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

/// AMDL — aim model (weapon accuracy/spread data).
pub static AMDL_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"AMDL"), name: "Aim Model", members: &AMDL_MEMBERS };


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

/// ZOOM — zoom / scope configuration.
pub static ZOOM_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"ZOOM"), name: "Zoom", members: &ZOOM_MEMBERS };


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

/// AORU — attraction rule.
pub static AORU_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"AORU"), name: "Attraction Rule", members: &AORU_MEMBERS };


static TRNS_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Transform Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// TRNS — transform / offset data.
pub static TRNS_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"TRNS"), name: "Transform", members: &TRNS_MEMBERS };


static TRAV_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Traversal Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// TRAV — traversal type (Jump, Vault, Ladder, Doorway, Activate, Jetpack).
pub static TRAV_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"TRAV"), name: "Traversal", members: &TRAV_MEMBERS };


static CNDF_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    CTDA_DEF,
];

/// CNDF — condition form (reusable condition container).
pub static CNDF_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"CNDF"), name: "Condition Form", members: &CNDF_MEMBERS };


static GCVR_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// GCVR — ground cover (surface-level vegetation / clutter definition).
pub static GCVR_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"GCVR"), name: "Ground Cover", members: &GCVR_MEMBERS };


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

/// OVIS — object visibility manager.
pub static OVIS_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"OVIS"),
    name: "Object Visibility Manager",
    members: &OVIS_MEMBERS,
};


static RFGP_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"NNAM"),
        name: "Name",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
];

/// RFGP — reference group.
pub static RFGP_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"RFGP"), name: "Reference Group", members: &RFGP_MEMBERS };


static STAG_MEMBERS: [SubRecordDef; 1] = [EDID_DEF];

/// STAG — animation sound tag set.
pub static STAG_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"STAG"),
    name: "Animation Sound Tag Set",
    members: &STAG_MEMBERS,
};


static KSSM_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Mapping Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// KSSM — sound keyword mapping.
pub static KSSM_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"KSSM"),
    name: "Sound Keyword Mapping",
    members: &KSSM_MEMBERS,
};


static NOCM_MEMBERS: [SubRecordDef; 1] = [EDID_DEF];

/// NOCM — navmesh obstacle cover manager.
pub static NOCM_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"NOCM"),
    name: "Navmesh Obstacle Cover Manager",
    members: &NOCM_MEMBERS,
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

/// DFOB — default object.
pub static DFOB_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"DFOB"), name: "Default Object", members: &DFOB_MEMBERS };


static PKIN_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// PKIN — pack-in (container for placed objects).
pub static PKIN_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"PKIN"), name: "Pack-In", members: &PKIN_MEMBERS };


static SCOL_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    OBND_DEF,
    MODL_DEF,
];

/// SCOL — static collection.
pub static SCOL_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"SCOL"), name: "Static Collection", members: &SCOL_MEMBERS };


static LVSC_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"LLCT"),
        name: "Entry Count",
        required: false,
        repeating: false,
        field: FieldType::UInt8,
    },
    SubRecordDef {
        sig: Signature(*b"LVLO"),
        name: "Leveled List Entry",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
];

/// LVSC — leveled space cell (space-specific leveled cell list).
pub static LVSC_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"LVSC"),
    name: "Leveled Space Cell",
    members: &LVSC_MEMBERS,
};


static GBFT_MEMBERS: [SubRecordDef; 1] = [EDID_DEF];

/// GBFT — generic base form template.
pub static GBFT_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"GBFT"),
    name: "Generic Base Form Template",
    members: &GBFT_MEMBERS,
};


static GBFM_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    OBND_DEF,
    SubRecordDef {
        sig: Signature(*b"TNAM"),
        name: "Template",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
];

/// GBFM — generic base form (templated form instance).
pub static GBFM_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"GBFM"), name: "Generic Base Form", members: &GBFM_MEMBERS };


static LVLB_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"LLCT"),
        name: "Entry Count",
        required: false,
        repeating: false,
        field: FieldType::UInt8,
    },
    SubRecordDef {
        sig: Signature(*b"LVLO"),
        name: "Leveled List Entry",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
];

/// LVLB — leveled base form (leveled form template).
pub static LVLB_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"LVLB"),
    name: "Leveled Base Form",
    members: &LVLB_MEMBERS,
};


static PMFT_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// PMFT — photo mode feature.
pub static PMFT_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"PMFT"), name: "Photo Mode Feature", members: &PMFT_MEMBERS };


static AFFE_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// AFFE — affinity event (companion affinity trigger).
pub static AFFE_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"AFFE"), name: "Affinity Event", members: &AFFE_MEMBERS };


static CURV_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Curve Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// CURV — curve table (animation or value curve).
pub static CURV_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"CURV"), name: "Curve Table", members: &CURV_MEMBERS };


static CUR3_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Curve Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// CUR3 — 3D curve data.
pub static CUR3_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"CUR3"), name: "Curve 3D", members: &CUR3_MEMBERS };


static SECH_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    OBND_DEF,
];

/// SECH — sound echo marker.
pub static SECH_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"SECH"), name: "Sound Echo Marker", members: &SECH_MEMBERS };


static FXPD_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Expression Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// FXPD — facial expression data.
pub static FXPD_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"FXPD"),
    name: "Facial Expression Data",
    members: &FXPD_MEMBERS,
};


static ASPC_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    OBND_DEF,
    SubRecordDef {
        sig: Signature(*b"SNAM"),
        name: "Ambient Sound",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
];

/// ASPC — acoustic space.
pub static ASPC_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"ASPC"), name: "Acoustic Space", members: &ASPC_MEMBERS };


static REVB_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Reverb Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// REVB — reverb parameters.
pub static REVB_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"REVB"), name: "Reverb Parameters", members: &REVB_MEMBERS };


static MUSC_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"FNAM"),
        name: "Flags",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
    SubRecordDef {
        sig: Signature(*b"PNAM"),
        name: "Priority",
        required: false,
        repeating: false,
        field: FieldType::UInt16,
    },
];

/// MUSC — music type.
pub static MUSC_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"MUSC"), name: "Music Type", members: &MUSC_MEMBERS };


static MUST_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"CNAM"),
        name: "File Path",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"ANAM"),
        name: "Track Type",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
];

/// MUST — music track.
pub static MUST_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"MUST"), name: "Music Track", members: &MUST_MEMBERS };


static IMGS_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"ENAM"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// IMGS — image space.
pub static IMGS_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"IMGS"), name: "Image Space", members: &IMGS_MEMBERS };


static IMAD_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// IMAD — image space adapter.
pub static IMAD_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"IMAD"),
    name: "Image Space Adapter",
    members: &IMAD_MEMBERS,
};


static CLFM_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"CNAM"),
        name: "Color",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
];

/// CLFM — color form.
pub static CLFM_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"CLFM"), name: "Color", members: &CLFM_MEMBERS };


static DUAL_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// DUAL — dual cast data.
pub static DUAL_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"DUAL"), name: "Dual Cast Data", members: &DUAL_MEMBERS };


static FSTP_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Impact Dataset",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"ANAM"),
        name: "Tag",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
];

/// FSTP — footstep.
pub static FSTP_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"FSTP"), name: "Footstep", members: &FSTP_MEMBERS };


static FSTS_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"XCNT"),
        name: "Count",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
];

/// FSTS — footstep set.
pub static FSTS_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"FSTS"), name: "Footstep Set", members: &FSTS_MEMBERS };


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


