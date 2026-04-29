// SPDX-License-Identifier: Apache-2.0
//!
//! Starfield-exclusive planetary and space system record schemas.
//!
//! These records have no counterpart in any earlier Bethesda game. They
//! define the procedural planet, biome, surface-generation, and resource
//! systems unique to Starfield.

use crate::schema::{FieldType, RecordSchema, SubRecordDef};
use crate::types::Signature;

use super::common::{EDID_DEF, FULL_DEF, KSIZ_DEF, KWDA_DEF, MODL_DEF};


static PNDT_MEMBERS: [SubRecordDef; 9] = [
    EDID_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"ANAM"),
        name: "Body Type",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
    SubRecordDef {
        sig: Signature(*b"BNAM"),
        name: "Parent Star",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"CNAM"),
        name: "Catalog ID",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Orbital Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"ATMO"),
        name: "Atmosphere",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    KSIZ_DEF,
    KWDA_DEF,
];

/// PNDT — planet definition (Starfield-exclusive celestial body).
pub static PNDT_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"PNDT"), name: "Planet", members: &PNDT_MEMBERS };


static STDT_MEMBERS: [SubRecordDef; 7] = [
    EDID_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"ANAM"),
        name: "Spectral Class",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
    SubRecordDef {
        sig: Signature(*b"BNAM"),
        name: "Catalog ID",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Star Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"PNAM"),
        name: "Position",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    KSIZ_DEF,
];

/// STDT — star system definition (Starfield-exclusive).
pub static STDT_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"STDT"), name: "Star", members: &STDT_MEMBERS };


static BIOM_MEMBERS: [SubRecordDef; 7] = [
    EDID_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"SNAM"),
        name: "Surface Sound",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"PNAM"),
        name: "Plant",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"RNAM"),
        name: "Resource",
        required: false,
        repeating: true,
        field: FieldType::FormId,
    },
    KSIZ_DEF,
    KWDA_DEF,
];

/// BIOM — biome definition (terrain biome with flora and resources).
pub static BIOM_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"BIOM"), name: "Biome", members: &BIOM_MEMBERS };


static SUNP_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"CNAM"),
        name: "Disk Color",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Corona Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"ENAM"),
        name: "Glow Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// SUNP — sun / star appearance preset.
pub static SUNP_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"SUNP"), name: "Sun Preset", members: &SUNP_MEMBERS };


static ATMO_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"CNAM"),
        name: "Color",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Atmosphere Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// ATMO — atmosphere definition for a planet.
pub static ATMO_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"ATMO"), name: "Atmosphere", members: &ATMO_MEMBERS };


static SFBK_MEMBERS: [SubRecordDef; 6] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"ANAM"),
        name: "Category",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"ENAM"),
        name: "Biome",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"FNAM"),
        name: "Object",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"WHGT"),
        name: "Weight",
        required: false,
        repeating: false,
        field: FieldType::Float32,
    },
];

/// SFBK — surface block (individual terrain placement tile).
pub static SFBK_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"SFBK"), name: "Surface Block", members: &SFBK_MEMBERS };


static SFPT_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"CNAM"),
        name: "Category",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
    SubRecordDef {
        sig: Signature(*b"BNAM"),
        name: "Biome",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"EFRM"),
        name: "Block Entries",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// SFPT — surface pattern (16x16 grid of surface blocks with rotations).
pub static SFPT_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"SFPT"), name: "Surface Pattern", members: &SFPT_MEMBERS };


static SFPC_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"ENAM"),
        name: "Planet",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"BNAM"),
        name: "Pattern",
        required: false,
        repeating: true,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Config Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// SFPC — surface pattern config (generation settings for surface patterns).
pub static SFPC_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"SFPC"),
    name: "Surface Pattern Config",
    members: &SFPC_MEMBERS,
};


static SFTR_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"CNAM"),
        name: "Category",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"ENAM"),
        name: "Pattern",
        required: false,
        repeating: true,
        field: FieldType::FormId,
    },
];

/// SFTR — surface tree (master procedural terrain generation tree).
pub static SFTR_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"SFTR"), name: "Surface Tree", members: &SFTR_MEMBERS };


static PTST_MEMBERS: [SubRecordDef; 1] = [EDID_DEF];

/// PTST — surface pattern style (style tag for surface patterns).
pub static PTST_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"PTST"),
    name: "Surface Pattern Style",
    members: &PTST_MEMBERS,
};


static RSGD_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"RNAM"),
        name: "Resource",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Generation Parameters",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"BNAM"),
        name: "Biome",
        required: false,
        repeating: true,
        field: FieldType::FormId,
    },
];

/// RSGD — resource generation data (distribution of harvestable resources).
pub static RSGD_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"RSGD"),
    name: "Resource Generation Data",
    members: &RSGD_MEMBERS,
};


static RSPJ_MEMBERS: [SubRecordDef; 8] = [
    EDID_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"BNAM"),
        name: "Category",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
    SubRecordDef {
        sig: Signature(*b"CNAM"),
        name: "Created Form",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"RQPK"),
        name: "Required Perk",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"FVPA"),
        name: "Required Component",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"SNAM"),
        name: "Sound",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    KSIZ_DEF,
];

/// RSPJ — research project (unlockable crafting research, Starfield-exclusive).
pub static RSPJ_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"RSPJ"), name: "Research Project", members: &RSPJ_MEMBERS };


static PCMT_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"PCCB"),
        name: "Content Branch",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"NAM1"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// PCMT — planet content manager tree (root of content generation hierarchy).
pub static PCMT_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"PCMT"),
    name: "Planet Content Manager Tree",
    members: &PCMT_MEMBERS,
};


static PCBN_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"PCCB"),
        name: "Child Branch",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"PRTN"),
        name: "Placement Ratio",
        required: false,
        repeating: false,
        field: FieldType::Float32,
    },
    SubRecordDef {
        sig: Signature(*b"NAM1"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// PCBN — planet content manager branch node (distribution branch).
pub static PCBN_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"PCBN"),
    name: "Planet Content Manager Branch Node",
    members: &PCBN_MEMBERS,
};


static PCCN_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"PCCC"),
        name: "Content",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"PRTN"),
        name: "Placement Ratio",
        required: false,
        repeating: false,
        field: FieldType::Float32,
    },
];

/// PCCN — planet content manager content node (leaf placement node).
pub static PCCN_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"PCCN"),
    name: "Planet Content Manager Content Node",
    members: &PCCN_MEMBERS,
};


static AMBS_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"ASMB"),
        name: "Ambient Sound",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"ASAS"),
        name: "Ambience Set A",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"ASAE"),
        name: "Ambience Set B",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// AMBS — ambience set (ambient sound configuration per biome or location).
pub static AMBS_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"AMBS"), name: "Ambience Set", members: &AMBS_MEMBERS };


static CLDF_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"CNAM"),
        name: "Color",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Cloud Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// CLDF — cloud volume definition.
pub static CLDF_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"CLDF"), name: "Clouds", members: &CLDF_MEMBERS };


static TODD_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "TOD Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// TODD — time of day data.
pub static TODD_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"TODD"), name: "Time Of Day Data", members: &TODD_MEMBERS };


static BMOD_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    MODL_DEF,
    SubRecordDef {
        sig: Signature(*b"TMPP"),
        name: "Template",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// BMOD — bone modifier (bone animation / morphing data).
pub static BMOD_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"BMOD"), name: "Bone Modifier", members: &BMOD_MEMBERS };


static MRPH_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    MODL_DEF,
    SubRecordDef {
        sig: Signature(*b"TCMP"),
        name: "Target Component",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// MRPH — morphable object (mesh that can be morphed at runtime).
pub static MRPH_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"MRPH"), name: "Morphable Object", members: &MRPH_MEMBERS };


static STMP_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"NODE"),
        name: "Snap Node",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
];

/// STMP — snap template (defines snap points for object placement).
pub static STMP_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"STMP"), name: "Snap Template", members: &STMP_MEMBERS };


static STND_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Node Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// STND — snap template node (individual snap connection point).
pub static STND_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"STND"),
    name: "Snap Template Node",
    members: &STND_MEMBERS,
};


static STBH_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"ENAM"),
        name: "Snap Node A",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"PNAM"),
        name: "Snap Node B",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"UNAM"),
        name: "Behavior Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// STBH — snap template behavior (rules governing snap connections).
pub static STBH_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"STBH"),
    name: "Snap Template Behavior",
    members: &STBH_MEMBERS,
};


static AOPS_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"ANAM"),
        name: "Sight Marker Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// AOPS — aim optical sight marker (scope / reticle definition).
pub static AOPS_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"AOPS"),
    name: "Aim Optical Sight Marker",
    members: &AOPS_MEMBERS,
};


static AAMD_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Aim Assist Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// AAMD — aim assist model data.
pub static AAMD_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"AAMD"), name: "Aim Assist Model Data", members: &AAMD_MEMBERS };


static MAAM_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Melee Aim Assist Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// MAAM — melee aim assist model.
pub static MAAM_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"MAAM"),
    name: "Melee Aim Assist Model",
    members: &MAAM_MEMBERS,
};


static BMMO_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"BNAM"),
        name: "Biome",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// BMMO — biome marker object (identifies biome regions in the world).
pub static BMMO_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"BMMO"), name: "Biome Marker Object", members: &BMMO_MEMBERS };
