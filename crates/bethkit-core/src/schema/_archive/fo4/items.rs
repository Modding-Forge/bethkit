// SPDX-License-Identifier: Apache-2.0
//!
//! Fallout 4 item and crafting record schemas.
//!
//! Covers WEAP, ARMO, ARMA, AMMO, BOOK, ALCH, INGR, MISC, KEYM, SLGM,
//! APPA, COBJ, CONT, DOOR, FURN, OMOD, CMPO, and MSWP.

use crate::schema::{FieldDef, FieldType, RecordSchema, SubRecordDef};
use crate::types::Signature;

use super::common::{
    CTDA_DEF, DESC_DEF, DEST_DEF, EDID_DEF, EFID_DEF, EFIT_DEF, EITM_DEF, ETYP_DEF, FULL_DEF,
    KWDA_DEF, KSIZ_DEF, MODL_DEF, MOD2_DEF, OBND_DEF, VMAD_DEF, YNAM_DEF, ZNAM_DEF,
};
use super::enums::{FO4_WEAPON_ANIM_TYPE_ENUM, FO4_WEAPON_FLAGS, OMOD_PROPERTY_ENUM};


static WEAP_DATA_FIELDS: [FieldDef; 4] = [
    FieldDef { name: "Value", kind: FieldType::UInt32 },
    FieldDef { name: "Weight", kind: FieldType::Float32 },
    FieldDef { name: "Base Damage", kind: FieldType::UInt16 },
    FieldDef { name: "Ammo Capacity", kind: FieldType::UInt16 },
];

static WEAP_MEMBERS: [SubRecordDef; 16] = [
    EDID_DEF,
    VMAD_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    EITM_DEF,
    ETYP_DEF,
    YNAM_DEF,
    ZNAM_DEF,
    KSIZ_DEF,
    KWDA_DEF,
    DESC_DEF,
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::Struct(&WEAP_DATA_FIELDS),
    },
    SubRecordDef {
        sig: Signature(*b"CRDT"),
        name: "Critical Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"VNAM"),
        name: "Animation Type",
        required: false,
        repeating: false,
        field: FieldType::Enum(&FO4_WEAPON_ANIM_TYPE_ENUM),
    },
    SubRecordDef {
        sig: Signature(*b"WNAM"),
        name: "Weapon Flags",
        required: false,
        repeating: false,
        field: FieldType::Flags(&FO4_WEAPON_FLAGS),
    },
];

/// WEAP — weapon.
pub static WEAP_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"WEAP"), name: "Weapon", members: &WEAP_MEMBERS };


static ARMO_DATA_FIELDS: [FieldDef; 3] = [
    FieldDef { name: "Value", kind: FieldType::Int32 },
    FieldDef { name: "Weight", kind: FieldType::Float32 },
    FieldDef { name: "Health", kind: FieldType::UInt32 },
];

static ARMO_MEMBERS: [SubRecordDef; 13] = [
    EDID_DEF,
    VMAD_DEF,
    OBND_DEF,
    FULL_DEF,
    EITM_DEF,
    ETYP_DEF,
    DEST_DEF,
    YNAM_DEF,
    ZNAM_DEF,
    KSIZ_DEF,
    KWDA_DEF,
    DESC_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::Struct(&ARMO_DATA_FIELDS),
    },
];

/// ARMO — armor / clothing.
pub static ARMO_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"ARMO"), name: "Armor", members: &ARMO_MEMBERS };


static ARMA_MEMBERS: [SubRecordDef; 7] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"BOD2"),
        name: "Biped Body Template",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    MODL_DEF,
    MOD2_DEF,
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"SNDD"),
        name: "Footstep Sound",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"ONAM"),
        name: "Art Object",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
];

/// ARMA — armor addon / body part.
pub static ARMA_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"ARMA"), name: "Armor Addon", members: &ARMA_MEMBERS };


static AMMO_DATA_FIELDS: [FieldDef; 4] = [
    FieldDef { name: "Projectile", kind: FieldType::FormId },
    FieldDef { name: "Flags", kind: FieldType::UInt32 },
    FieldDef { name: "Damage", kind: FieldType::Float32 },
    FieldDef { name: "Value", kind: FieldType::UInt32 },
];

static AMMO_MEMBERS: [SubRecordDef; 9] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    DESC_DEF,
    YNAM_DEF,
    ZNAM_DEF,
    KSIZ_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::Struct(&AMMO_DATA_FIELDS),
    },
];

/// AMMO — ammunition.
pub static AMMO_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"AMMO"), name: "Ammunition", members: &AMMO_MEMBERS };


static BOOK_MEMBERS: [SubRecordDef; 9] = [
    EDID_DEF,
    VMAD_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    DESC_DEF,
    YNAM_DEF,
    ZNAM_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// BOOK — book / note / holotape.
pub static BOOK_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"BOOK"), name: "Book", members: &BOOK_MEMBERS };


static ALCH_MEMBERS: [SubRecordDef; 11] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    DEST_DEF,
    YNAM_DEF,
    ZNAM_DEF,
    DESC_DEF,
    KSIZ_DEF,
    KWDA_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Weight",
        required: false,
        repeating: false,
        field: FieldType::Float32,
    },
];

/// ALCH — ingestible (food, drink, chems).
pub static ALCH_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"ALCH"), name: "Ingestible", members: &ALCH_MEMBERS };


static INGR_MEMBERS: [SubRecordDef; 8] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    YNAM_DEF,
    ZNAM_DEF,
    EFID_DEF,
    EFIT_DEF,
];

/// INGR — ingredient.
pub static INGR_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"INGR"), name: "Ingredient", members: &INGR_MEMBERS };


static MISC_DATA_FIELDS: [FieldDef; 2] = [
    FieldDef { name: "Value", kind: FieldType::Int32 },
    FieldDef { name: "Weight", kind: FieldType::Float32 },
];

static MISC_MEMBERS: [SubRecordDef; 7] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    YNAM_DEF,
    ZNAM_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::Struct(&MISC_DATA_FIELDS),
    },
];

/// MISC — miscellaneous item.
pub static MISC_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"MISC"), name: "Miscellaneous Item", members: &MISC_MEMBERS };


static KEYM_MEMBERS: [SubRecordDef; 7] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    YNAM_DEF,
    ZNAM_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// KEYM — key.
pub static KEYM_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"KEYM"), name: "Key", members: &KEYM_MEMBERS };


static SLGM_MEMBERS: [SubRecordDef; 6] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    YNAM_DEF,
    ZNAM_DEF,
];

/// SLGM — soul gem.
pub static SLGM_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"SLGM"), name: "Soul Gem", members: &SLGM_MEMBERS };


static APPA_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// APPA — apparatus.
pub static APPA_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"APPA"), name: "Apparatus", members: &APPA_MEMBERS };


static COBJ_MEMBERS: [SubRecordDef; 7] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"FNAM"),
        name: "Flags",
        required: false,
        repeating: false,
        field: FieldType::UInt16,
    },
    SubRecordDef {
        sig: Signature(*b"CTDA"),
        name: "Condition",
        required: false,
        repeating: true,
        field: FieldType::ByteArray,
    },
    SubRecordDef {
        sig: Signature(*b"CNAM"),
        name: "Result Object",
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
        name: "Result Count",
        required: false,
        repeating: false,
        field: FieldType::UInt16,
    },
    SubRecordDef {
        sig: Signature(*b"FVPA"),
        name: "Component Array",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// COBJ — constructible object (crafting recipe).
pub static COBJ_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"COBJ"),
    name: "Constructible Object",
    members: &COBJ_MEMBERS,
};


static CONT_MEMBERS: [SubRecordDef; 7] = [
    EDID_DEF,
    VMAD_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    DEST_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// CONT — container.
pub static CONT_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"CONT"), name: "Container", members: &CONT_MEMBERS };


static DOOR_MEMBERS: [SubRecordDef; 7] = [
    EDID_DEF,
    VMAD_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    DEST_DEF,
    SubRecordDef {
        sig: Signature(*b"FNAM"),
        name: "Flags",
        required: false,
        repeating: false,
        field: FieldType::UInt8,
    },
];

/// DOOR — door.
pub static DOOR_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"DOOR"), name: "Door", members: &DOOR_MEMBERS };


static FURN_MEMBERS: [SubRecordDef; 6] = [
    EDID_DEF,
    VMAD_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    SubRecordDef {
        sig: Signature(*b"MNAM"),
        name: "Marker Entry Points",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// FURN — furniture.
pub static FURN_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"FURN"), name: "Furniture", members: &FURN_MEMBERS };


static OMOD_MEMBERS: [SubRecordDef; 8] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    DESC_DEF,
    MODL_DEF,
    KSIZ_DEF,
    KWDA_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// OMOD — object modification (Fallout 4 specific; weapon / armour mods with
/// include hierarchy, priority, and property list).
pub static OMOD_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"OMOD"),
    name: "Object Modification",
    members: &OMOD_MEMBERS,
};


static CMPO_MEMBERS: [SubRecordDef; 5] = [
    EDID_DEF,
    FULL_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Scrap Item",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"MNAM"),
        name: "Scrap Scalar",
        required: false,
        repeating: false,
        field: FieldType::Float32,
    },
    KSIZ_DEF,
];

/// CMPO — component (Fallout 4 specific; base unit for crafting and scrapping).
pub static CMPO_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"CMPO"), name: "Component", members: &CMPO_MEMBERS };


static MSWP_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    SubRecordDef {
        sig: Signature(*b"FNAM"),
        name: "Base Object",
        required: false,
        repeating: false,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"BNAM"),
        name: "Swap Entries",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// MSWP — material swap (Fallout 4 specific; defines material substitutions
/// applied to a model's texture sets).
pub static MSWP_SCHEMA: RecordSchema =
    RecordSchema { sig: Signature(*b"MSWP"), name: "Material Swap", members: &MSWP_MEMBERS };

// Suppress lint for enum used via type annotation only.
const _: () = {
    let _ = &OMOD_PROPERTY_ENUM;
    let _ = &CTDA_DEF;
};
