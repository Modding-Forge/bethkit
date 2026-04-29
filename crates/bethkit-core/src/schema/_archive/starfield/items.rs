// SPDX-License-Identifier: Apache-2.0
//!
//! Starfield item record schemas.
//!
//! Covers WEAP, ARMO, ARMA, AMMO, BOOK, ALCH, INGR, MISC, KEYM, OMOD,
//! SCRL, LGDI, IRES, TERM, BNDS, PDCL, CMPO.

use crate::schema::{FieldDef, FieldType, RecordSchema, SubRecordDef};
use crate::types::Signature;

use super::common::{
    DESC_DEF, EDID_DEF, EFID_DEF, EFIT_DEF, FULL_DEF, KSIZ_DEF, KWDA_DEF, MODL_DEF, OBND_DEF,
    VMAD_DEF,
};
use super::enums::{SF_WEAPON_ANIM_TYPE_ENUM, SF_WEAPON_FLAGS};


static WEAP_DATA_FIELDS: [FieldDef; 5] = [
    FieldDef { name: "Value", kind: FieldType::UInt32 },
    FieldDef { name: "Weight", kind: FieldType::Float32 },
    FieldDef { name: "Damage", kind: FieldType::UInt16 },
    FieldDef { name: "Ammo Capacity", kind: FieldType::UInt16 },
    FieldDef { name: "Flags", kind: FieldType::Flags(&SF_WEAPON_FLAGS) },
];

static WEAP_DNAM_FIELDS: [FieldDef; 3] = [
    FieldDef { name: "Animation Type", kind: FieldType::Enum(&SF_WEAPON_ANIM_TYPE_ENUM) },
    FieldDef { name: "Unused", kind: FieldType::Unused(2) },
    FieldDef { name: "Stagger", kind: FieldType::Float32 },
];

static WEAP_MEMBERS: [SubRecordDef; 12] = [
    EDID_DEF,
    VMAD_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    DESC_DEF,
    KSIZ_DEF,
    KWDA_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Weapon Data",
        required: false,
        repeating: false,
        field: FieldType::Struct(&WEAP_DATA_FIELDS),
    },
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Weapon Extended Data",
        required: false,
        repeating: false,
        field: FieldType::Struct(&WEAP_DNAM_FIELDS),
    },
    SubRecordDef {
        sig: Signature(*b"NNAM"),
        name: "Embedded Node",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"INAM"),
        name: "Impact Dataset",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
];

/// WEAP — weapon.
pub static WEAP_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"WEAP"), name: "Weapon", members: &WEAP_MEMBERS };


static ARMO_DATA_FIELDS: [FieldDef; 2] = [
    FieldDef { name: "Value", kind: FieldType::UInt32 },
    FieldDef { name: "Weight", kind: FieldType::Float32 },
];

static ARMO_MEMBERS: [SubRecordDef; 10] = [
    EDID_DEF,
    VMAD_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    DESC_DEF,
    KSIZ_DEF,
    KWDA_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Armor Data",
        required: false,
        repeating: false,
        field: FieldType::Struct(&ARMO_DATA_FIELDS),
    },
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Armor Rating",
        required: false,
        repeating: false,
        field: FieldType::Float32,
    },
];

/// ARMO — armor piece.
pub static ARMO_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"ARMO"), name: "Armor", members: &ARMO_MEMBERS };


static ARMA_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    MODL_DEF,
    SubRecordDef {
        sig: Signature(*b"MOD2"),
        name: "Female Model",
        required: false,
        repeating: false,
        field: FieldType::ZString,
    },
    SubRecordDef {
        sig: Signature(*b"RNAM"),
        name: "Race",
        required: false,
        repeating: true,
        field: FieldType::FormId,
    },
];

/// ARMA — armor addon (biped mesh variant).
pub static ARMA_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"ARMA"), name: "Armor Addon", members: &ARMA_MEMBERS };


static AMMO_DATA_FIELDS: [FieldDef; 4] = [
    FieldDef { name: "Projectile", kind: FieldType::FormId },
    FieldDef { name: "Flags", kind: FieldType::UInt32 },
    FieldDef { name: "Damage", kind: FieldType::Float32 },
    FieldDef { name: "Value", kind: FieldType::UInt32 },
];

static AMMO_MEMBERS: [SubRecordDef; 8] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    DESC_DEF,
    KSIZ_DEF,
    KWDA_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Ammo Data",
        required: false,
        repeating: false,
        field: FieldType::Struct(&AMMO_DATA_FIELDS),
    },
];

/// AMMO — ammunition.
pub static AMMO_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"AMMO"), name: "Ammunition", members: &AMMO_MEMBERS };


static BOOK_DATA_FIELDS: [FieldDef; 4] = [
    FieldDef { name: "Flags", kind: FieldType::UInt8 },
    FieldDef { name: "Type", kind: FieldType::UInt8 },
    FieldDef { name: "Unused", kind: FieldType::Unused(2) },
    FieldDef { name: "Value", kind: FieldType::UInt32 },
];

static BOOK_MEMBERS: [SubRecordDef; 9] = [
    EDID_DEF,
    VMAD_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    DESC_DEF,
    KSIZ_DEF,
    KWDA_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Book Data",
        required: false,
        repeating: false,
        field: FieldType::Struct(&BOOK_DATA_FIELDS),
    },
];

/// BOOK — book / holotape.
pub static BOOK_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"BOOK"), name: "Book", members: &BOOK_MEMBERS };


static ALCH_DATA_FIELDS: [FieldDef; 2] = [
    FieldDef { name: "Weight", kind: FieldType::Float32 },
    FieldDef { name: "Flags", kind: FieldType::UInt32 },
];

static ALCH_MEMBERS: [SubRecordDef; 11] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    DESC_DEF,
    KSIZ_DEF,
    KWDA_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Ingestible Data",
        required: false,
        repeating: false,
        field: FieldType::Struct(&ALCH_DATA_FIELDS),
    },
    SubRecordDef {
        sig: Signature(*b"ENIT"),
        name: "Effect Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    EFID_DEF,
    EFIT_DEF,
];

/// ALCH — ingestible (aid item, food, drink).
pub static ALCH_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"ALCH"), name: "Ingestible", members: &ALCH_MEMBERS };


static INGR_MEMBERS: [SubRecordDef; 8] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    KSIZ_DEF,
    KWDA_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Ingredient Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"ENIT"),
        name: "Effect Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// INGR — ingredient.
pub static INGR_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"INGR"), name: "Ingredient", members: &INGR_MEMBERS };


static MISC_DATA_FIELDS: [FieldDef; 2] = [
    FieldDef { name: "Value", kind: FieldType::UInt32 },
    FieldDef { name: "Weight", kind: FieldType::Float32 },
];

static MISC_MEMBERS: [SubRecordDef; 7] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    KSIZ_DEF,
    KWDA_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Misc Data",
        required: false,
        repeating: false,
        field: FieldType::Struct(&MISC_DATA_FIELDS),
    },
];

/// MISC — miscellaneous item.
pub static MISC_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"MISC"), name: "Misc Item", members: &MISC_MEMBERS };


static KEYM_DATA_FIELDS: [FieldDef; 2] = [
    FieldDef { name: "Value", kind: FieldType::UInt32 },
    FieldDef { name: "Weight", kind: FieldType::Float32 },
];

static KEYM_MEMBERS: [SubRecordDef; 6] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    KSIZ_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Key Data",
        required: false,
        repeating: false,
        field: FieldType::Struct(&KEYM_DATA_FIELDS),
    },
];

/// KEYM — key.
pub static KEYM_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"KEYM"), name: "Key", members: &KEYM_MEMBERS };


static OMOD_MEMBERS: [SubRecordDef; 6] = [
    EDID_DEF,
    FULL_DEF,
    DESC_DEF,
    MODL_DEF,
    KSIZ_DEF,
    KWDA_DEF,
];

/// OMOD — object modification (weapon / armor mod attachment).
pub static OMOD_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"OMOD"),
    name: "Object Modification",
    members: &OMOD_MEMBERS,
};


static SCRL_MEMBERS: [SubRecordDef; 7] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    KSIZ_DEF,
    KWDA_DEF,
    SubRecordDef {
        sig: Signature(*b"SPIT"),
        name: "Spell Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// SCRL — scroll.
pub static SCRL_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"SCRL"), name: "Scroll", members: &SCRL_MEMBERS };


static LGDI_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"BNAM"),
        name: "Base Object",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"CNAM"),
        name: "Component",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
];

/// LGDI — legendary item container (Starfield-exclusive).
pub static LGDI_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"LGDI"), name: "Legendary Item", members: &LGDI_MEMBERS };


static IRES_MEMBERS: [SubRecordDef; 6] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    KSIZ_DEF,
    KWDA_DEF,
];

/// IRES — resource item (harvestable planet resource, Starfield-exclusive).
pub static IRES_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"IRES"), name: "Resource", members: &IRES_MEMBERS };


static TERM_MEMBERS: [SubRecordDef; 7] = [
    EDID_DEF,
    VMAD_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    KSIZ_DEF,
    KWDA_DEF,
];

/// TERM — terminal (interactive console object).
pub static TERM_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"TERM"), name: "Terminal", members: &TERM_MEMBERS };


static BNDS_MEMBERS: [SubRecordDef; 3] = [
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

/// BNDS — bendable spline (flexible cable or rope object).
pub static BNDS_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"BNDS"), name: "Bendable Spline", members: &BNDS_MEMBERS };


static PDCL_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    OBND_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Decal Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// PDCL — projected decal.
pub static PDCL_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"PDCL"), name: "Projected Decal", members: &PDCL_MEMBERS };


static CMPO_MEMBERS: [SubRecordDef; 3] = [
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

/// CMPO — component (crafting component material).
pub static CMPO_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"CMPO"), name: "Component", members: &CMPO_MEMBERS };


static COBJ_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"CNAM"),
        name: "Created Object",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"BNAM"),
        name: "Workbench Keyword",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"NAM1"),
        name: "Created Object Count",
        required: false,
        repeating: false,
        field: FieldType::UInt16,
    },
    VMAD_DEF,
];

/// COBJ — constructible object (crafting recipe).
pub static COBJ_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"COBJ"),
    name: "Constructible Object",
    members: &COBJ_MEMBERS,
};
