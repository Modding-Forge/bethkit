// SPDX-License-Identifier: Apache-2.0
//!
//! Fallout 4 magic and effect record schemas.
//!
//! Covers SPEL, MGEF, ENCH, RFCT, PROJ, EXPL, HAZD, PERK, IMGS, IMAD,
//! IPCT, IPDS, ADDN, SPGD, EFSH, and the FO4-specific DMGT record.

use crate::schema::{FieldDef, FieldType, RecordSchema, SubRecordDef};
use crate::types::Signature;

use super::common::{
    CTDA_DEF, DESC_DEF, EDID_DEF, EFID_DEF, EFIT_DEF, FULL_DEF, KSIZ_DEF, KWDA_DEF, MODL_DEF,
    OBND_DEF, VMAD_DEF,
};

static SPEL_MEMBERS: [SubRecordDef; 8] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    KSIZ_DEF,
    KWDA_DEF,
    SubRecordDef {
        sig: Signature(*b"SPIT"),
        name: "Spell Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    EFID_DEF,
    EFIT_DEF,
];

/// SPEL — spell.
pub static SPEL_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"SPEL"),
    name: "Spell",
    members: &SPEL_MEMBERS,
};

static MGEF_MEMBERS: [SubRecordDef; 7] = [
    EDID_DEF,
    VMAD_DEF,
    FULL_DEF,
    KSIZ_DEF,
    KWDA_DEF,
    DESC_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// MGEF — magic effect.
pub static MGEF_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"MGEF"),
    name: "Magic Effect",
    members: &MGEF_MEMBERS,
};

static ENCH_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"ENIT"),
        name: "Effect Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    EFID_DEF,
];

/// ENCH — object effect / enchantment.
pub static ENCH_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"ENCH"),
    name: "Object Effect",
    members: &ENCH_MEMBERS,
};

static RFCT_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Flags",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
];

/// RFCT — visual effect.
pub static RFCT_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"RFCT"),
    name: "Visual Effect",
    members: &RFCT_MEMBERS,
};

static PROJ_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// PROJ — projectile.
pub static PROJ_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"PROJ"),
    name: "Projectile",
    members: &PROJ_MEMBERS,
};

static EXPL_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// EXPL — explosion.
pub static EXPL_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"EXPL"),
    name: "Explosion",
    members: &EXPL_MEMBERS,
};

static HAZD_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// HAZD — hazard.
pub static HAZD_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"HAZD"),
    name: "Hazard",
    members: &HAZD_MEMBERS,
};

static PERK_MEMBERS: [SubRecordDef; 8] = [
    EDID_DEF,
    VMAD_DEF,
    FULL_DEF,
    DESC_DEF,
    SubRecordDef {
        sig: Signature(*b"ICON"),
        name: "Icon",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    CTDA_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"PRKE"),
        name: "Perk Section",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
];

/// PERK — perk.
pub static PERK_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"PERK"),
    name: "Perk",
    members: &PERK_MEMBERS,
};

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
pub static IMGS_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"IMGS"),
    name: "Image Space",
    members: &IMGS_MEMBERS,
};

static IMAD_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DNAM"),
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

static IPCT_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    MODL_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Sound1",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
];

/// IPCT — impact.
pub static IPCT_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"IPCT"),
    name: "Impact",
    members: &IPCT_MEMBERS,
};

static IPDS_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"PNAM"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// IPDS — impact data set.
pub static IPDS_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"IPDS"),
    name: "Impact Data Set",
    members: &IPDS_MEMBERS,
};

static ADDN_MEMBERS: [SubRecordDef; 4] = [
    EDID_DEF,
    OBND_DEF,
    MODL_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Index",
        required: false,
        repeating: false,
        field: FieldType::Int32,
    },
];

/// ADDN — addon node.
pub static ADDN_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"ADDN"),
    name: "Addon Node",
    members: &ADDN_MEMBERS,
};

static SPGD_MEMBERS: [SubRecordDef; 2] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// SPGD — shader particle geometry data.
pub static SPGD_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"SPGD"),
    name: "Shader Particle Geometry Data",
    members: &SPGD_MEMBERS,
};

static EFSH_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"ICON"),
        name: "Fill Texture",
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

/// EFSH — effect shader.
pub static EFSH_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"EFSH"),
    name: "Effect Shader",
    members: &EFSH_MEMBERS,
};

// Suppress lint for imports not directly used in static field position.
// NOTE: DMGT is intentionally not defined here; it lives in simple.rs to avoid
// NOTE: duplication. The VMAD_DEF import is needed for the SPEL/MGEF members above.
const _: () = {
    let _ = &VMAD_DEF;
    let _: &[FieldDef] = &[];
};
