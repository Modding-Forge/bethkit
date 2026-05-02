// SPDX-License-Identifier: Apache-2.0
//!
//! Schema definitions for item / inventory SSE record types.
//!
//! Covers: WEAP, ARMO, ARMA, AMMO, BOOK, ALCH, INGR, MISC, KEYM,
//! SLGM, APPA, COBJ, CONT, DOOR, FURN.

use crate::schema::{FieldDef, FieldType, RecordSchema, SubRecordDef};
use crate::types::Signature;

use super::common::{
    CTDA_DEF, DESC_DEF, DEST_DEF, EAMT_DEF, EDID_DEF, EFID_DEF, EFIT_DEF, EITM_DEF, ETYP_DEF,
    FULL_DEF, ICON_DEF, KSIZ_DEF, KWDA_DEF, MICO_DEF, MOD2_DEF, MOD3_DEF, MODL_DEF, OBND_DEF,
    RNAM_DEF, VMAD_DEF, YNAM_DEF, ZNAM_DEF,
};
use crate::schema::enums::{
    ARMOR_TYPE_ENUM, BOOK_FLAGS, BOOK_TYPE_ENUM, SCHOOL_ENUM, STAGGER_ENUM, WEAPON_ANIM_TYPE_ENUM,
};

static WEAP_DATA_FIELDS: [FieldDef; 3] = [
    FieldDef {
        name: "Value",
        kind: FieldType::UInt32,
    },
    FieldDef {
        name: "Weight",
        kind: FieldType::Float32,
    },
    FieldDef {
        name: "Damage",
        kind: FieldType::UInt16,
    },
];

static WEAP_DNAM_FIELDS: [FieldDef; 22] = [
    FieldDef {
        name: "Animation Type",
        kind: FieldType::Enum(&WEAPON_ANIM_TYPE_ENUM),
    },
    FieldDef {
        name: "Animation Multiplier",
        kind: FieldType::Float32,
    },
    FieldDef {
        name: "Reach",
        kind: FieldType::Float32,
    },
    FieldDef {
        name: "Flags",
        kind: FieldType::UInt16,
    },
    FieldDef {
        name: "Unknown",
        kind: FieldType::UInt8,
    },
    FieldDef {
        name: "Unknown2",
        kind: FieldType::UInt8,
    },
    FieldDef {
        name: "Sight FoV",
        kind: FieldType::Float32,
    },
    FieldDef {
        name: "Unknown3",
        kind: FieldType::Float32,
    },
    FieldDef {
        name: "VATS to-hit chance",
        kind: FieldType::UInt8,
    },
    FieldDef {
        name: "Unknown4",
        kind: FieldType::UInt8,
    },
    FieldDef {
        name: "Proj per Shot",
        kind: FieldType::UInt8,
    },
    FieldDef {
        name: "Unknown5",
        kind: FieldType::UInt8,
    },
    FieldDef {
        name: "Attack Animation Mult",
        kind: FieldType::Float32,
    },
    FieldDef {
        name: "Rugged",
        kind: FieldType::Float32,
    },
    FieldDef {
        name: "Unknown6",
        kind: FieldType::Float32,
    },
    FieldDef {
        name: "Override - Skill",
        kind: FieldType::Int32,
    },
    FieldDef {
        name: "Override - Actor Value",
        kind: FieldType::Int32,
    },
    FieldDef {
        name: "Physical Damage",
        kind: FieldType::Float32,
    },
    FieldDef {
        name: "Base VATS to-hit Chance",
        kind: FieldType::Float32,
    },
    FieldDef {
        name: "Projectile Count Override",
        kind: FieldType::UInt32,
    },
    FieldDef {
        name: "Embedded Weapon AV",
        kind: FieldType::Int32,
    },
    FieldDef {
        name: "Stagger",
        kind: FieldType::Enum(&STAGGER_ENUM),
    },
];

static WEAP_MEMBERS: [SubRecordDef; 16] = [
    EDID_DEF,
    VMAD_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    ICON_DEF,
    MICO_DEF,
    EITM_DEF,
    EAMT_DEF,
    DEST_DEF,
    ETYP_DEF,
    KSIZ_DEF,
    KWDA_DEF,
    DESC_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Game Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&WEAP_DATA_FIELDS),
    },
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Weapon Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&WEAP_DNAM_FIELDS),
    },
];

/// WEAP — Weapon.
pub static WEAP_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"WEAP"),
    name: "Weapon",
    members: &WEAP_MEMBERS,
};

static ARMO_DATA_FIELDS: [FieldDef; 3] = [
    FieldDef {
        name: "Value",
        kind: FieldType::UInt32,
    },
    FieldDef {
        name: "Weight",
        kind: FieldType::Float32,
    },
    FieldDef {
        name: "Health",
        kind: FieldType::UInt32,
    },
];

static ARMO_DNAM_FIELDS: [FieldDef; 2] = [
    FieldDef {
        name: "Armor Rating",
        kind: FieldType::Float32,
    },
    FieldDef {
        name: "Base Rating Override",
        kind: FieldType::Float32,
    },
];

static ARMO_MEMBERS: [SubRecordDef; 14] = [
    EDID_DEF,
    VMAD_DEF,
    OBND_DEF,
    FULL_DEF,
    ICON_DEF,
    MICO_DEF,
    EITM_DEF,
    EAMT_DEF,
    DEST_DEF,
    ETYP_DEF,
    KSIZ_DEF,
    KWDA_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&ARMO_DATA_FIELDS),
    },
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Armor Rating",
        required: true,
        repeating: false,
        field: FieldType::Struct(&ARMO_DNAM_FIELDS),
    },
];

/// ARMO — Armor / clothing.
pub static ARMO_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"ARMO"),
    name: "Armor",
    members: &ARMO_MEMBERS,
};

static ARMA_DNAM_FIELDS: [FieldDef; 9] = [
    FieldDef {
        name: "Biped Object Slot (Primary)",
        kind: FieldType::UInt32,
    },
    FieldDef {
        name: "Biped Object Slot (Secondary)",
        kind: FieldType::UInt32,
    },
    FieldDef {
        name: "Priority",
        kind: FieldType::UInt8,
    },
    FieldDef {
        name: "Unknown",
        kind: FieldType::UInt8,
    },
    FieldDef {
        name: "Unknown2",
        kind: FieldType::UInt16,
    },
    FieldDef {
        name: "Detect Sound Value",
        kind: FieldType::Float32,
    },
    FieldDef {
        name: "Weapon Adjust",
        kind: FieldType::Float32,
    },
    FieldDef {
        name: "Armor Type",
        kind: FieldType::Enum(&ARMOR_TYPE_ENUM),
    },
    FieldDef {
        name: "Unknown3",
        kind: FieldType::UInt8,
    },
];

static ARMA_MEMBERS: [SubRecordDef; 7] = [
    EDID_DEF,
    OBND_DEF,
    RNAM_DEF,
    MODL_DEF,
    MOD2_DEF,
    MOD3_DEF,
    SubRecordDef {
        sig: Signature(*b"DNAM"),
        name: "Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&ARMA_DNAM_FIELDS),
    },
];

/// ARMA — Armor addon (race-specific mesh set).
pub static ARMA_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"ARMA"),
    name: "Armor Addon",
    members: &ARMA_MEMBERS,
};

static AMMO_DATA_FIELDS: [FieldDef; 5] = [
    FieldDef {
        name: "Projectile",
        kind: FieldType::FormIdTyped(&[Signature(*b"PROJ")]),
    },
    FieldDef {
        name: "Flags",
        kind: FieldType::UInt32,
    },
    FieldDef {
        name: "Damage",
        kind: FieldType::Float32,
    },
    FieldDef {
        name: "Value",
        kind: FieldType::UInt32,
    },
    FieldDef {
        name: "Weight",
        kind: FieldType::Float32,
    },
];

static AMMO_MEMBERS: [SubRecordDef; 9] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    ICON_DEF,
    MICO_DEF,
    DEST_DEF,
    KSIZ_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&AMMO_DATA_FIELDS),
    },
];

/// AMMO — Ammunition.
pub static AMMO_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"AMMO"),
    name: "Ammunition",
    members: &AMMO_MEMBERS,
};

static BOOK_DATA_FIELDS: [FieldDef; 6] = [
    FieldDef {
        name: "Flags",
        kind: FieldType::Flags(&BOOK_FLAGS),
    },
    FieldDef {
        name: "Type",
        kind: FieldType::Enum(&BOOK_TYPE_ENUM),
    },
    FieldDef {
        name: "Unused",
        kind: FieldType::Unused(2),
    },
    FieldDef {
        name: "Teaches (Skill / Spell)",
        kind: FieldType::FormIdTyped(&[Signature(*b"SPEL")]),
    },
    FieldDef {
        name: "Value",
        kind: FieldType::UInt32,
    },
    FieldDef {
        name: "Weight",
        kind: FieldType::Float32,
    },
];

static BOOK_MEMBERS: [SubRecordDef; 10] = [
    EDID_DEF,
    VMAD_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    ICON_DEF,
    MICO_DEF,
    DESC_DEF,
    DEST_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&BOOK_DATA_FIELDS),
    },
];

/// BOOK — Book / note / scroll.
pub static BOOK_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"BOOK"),
    name: "Book",
    members: &BOOK_MEMBERS,
};

static ALCH_DATA_FIELDS: [FieldDef; 2] = [
    FieldDef {
        name: "Value",
        kind: FieldType::UInt32,
    },
    FieldDef {
        name: "Weight",
        kind: FieldType::Float32,
    },
];

static ALCH_ENIT_FIELDS: [FieldDef; 4] = [
    FieldDef {
        name: "Value",
        kind: FieldType::Int32,
    },
    FieldDef {
        name: "Flags",
        kind: FieldType::UInt32,
    },
    FieldDef {
        name: "Addiction Chance",
        kind: FieldType::Float32,
    },
    FieldDef {
        name: "Sound - Use",
        kind: FieldType::FormIdTyped(&[Signature(*b"SNDR")]),
    },
];

static ALCH_MEMBERS: [SubRecordDef; 11] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    ICON_DEF,
    MICO_DEF,
    MODL_DEF,
    DEST_DEF,
    YNAM_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Weight",
        required: true,
        repeating: false,
        field: FieldType::Struct(&ALCH_DATA_FIELDS),
    },
    SubRecordDef {
        sig: Signature(*b"ENIT"),
        name: "Effect Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&ALCH_ENIT_FIELDS),
    },
    EFID_DEF,
];

/// ALCH — Alchemy / potion / food.
pub static ALCH_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"ALCH"),
    name: "Ingestible",
    members: &ALCH_MEMBERS,
};

static INGR_DATA_FIELDS: [FieldDef; 2] = [
    FieldDef {
        name: "Value",
        kind: FieldType::UInt32,
    },
    FieldDef {
        name: "Weight",
        kind: FieldType::Float32,
    },
];

static INGR_ENIT_FIELDS: [FieldDef; 3] = [
    FieldDef {
        name: "Value",
        kind: FieldType::Int32,
    },
    FieldDef {
        name: "Flags",
        kind: FieldType::UInt32,
    },
    FieldDef {
        name: "Unknown",
        kind: FieldType::UInt32,
    },
];

static INGR_MEMBERS: [SubRecordDef; 9] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    ICON_DEF,
    DEST_DEF,
    YNAM_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&INGR_DATA_FIELDS),
    },
    SubRecordDef {
        sig: Signature(*b"ENIT"),
        name: "Effect Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&INGR_ENIT_FIELDS),
    },
];

/// INGR — Ingredient (alchemy component).
pub static INGR_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"INGR"),
    name: "Ingredient",
    members: &INGR_MEMBERS,
};

static MISC_DATA_FIELDS: [FieldDef; 2] = [
    FieldDef {
        name: "Value",
        kind: FieldType::UInt32,
    },
    FieldDef {
        name: "Weight",
        kind: FieldType::Float32,
    },
];

static MISC_MEMBERS: [SubRecordDef; 9] = [
    EDID_DEF,
    VMAD_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    ICON_DEF,
    DEST_DEF,
    YNAM_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&MISC_DATA_FIELDS),
    },
];

/// MISC — Miscellaneous item.
pub static MISC_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"MISC"),
    name: "Misc. Item",
    members: &MISC_MEMBERS,
};

static KEYM_DATA_FIELDS: [FieldDef; 2] = [
    FieldDef {
        name: "Value",
        kind: FieldType::UInt32,
    },
    FieldDef {
        name: "Weight",
        kind: FieldType::Float32,
    },
];

static KEYM_MEMBERS: [SubRecordDef; 8] = [
    EDID_DEF,
    VMAD_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    ICON_DEF,
    DEST_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&KEYM_DATA_FIELDS),
    },
];

/// KEYM — Key.
pub static KEYM_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"KEYM"),
    name: "Key",
    members: &KEYM_MEMBERS,
};

static SLGM_DATA_FIELDS: [FieldDef; 2] = [
    FieldDef {
        name: "Value",
        kind: FieldType::UInt32,
    },
    FieldDef {
        name: "Weight",
        kind: FieldType::Float32,
    },
];

static SLGM_MEMBERS: [SubRecordDef; 8] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    ICON_DEF,
    DEST_DEF,
    YNAM_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&SLGM_DATA_FIELDS),
    },
];

/// SLGM — Soul gem.
pub static SLGM_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"SLGM"),
    name: "Soul Gem",
    members: &SLGM_MEMBERS,
};

static APPA_DATA_FIELDS: [FieldDef; 4] = [
    FieldDef {
        name: "Type",
        kind: FieldType::UInt8,
    },
    FieldDef {
        name: "Unknown",
        kind: FieldType::Unused(3),
    },
    FieldDef {
        name: "Value",
        kind: FieldType::UInt32,
    },
    FieldDef {
        name: "Weight",
        kind: FieldType::Float32,
    },
];

static APPA_MEMBERS: [SubRecordDef; 8] = [
    EDID_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    ICON_DEF,
    DESC_DEF,
    DEST_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&APPA_DATA_FIELDS),
    },
];

/// APPA — Apparatus (alchemy tool).
pub static APPA_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"APPA"),
    name: "Apparatus",
    members: &APPA_MEMBERS,
};

static COBJ_MEMBERS: [SubRecordDef; 6] = [
    EDID_DEF,
    CTDA_DEF,
    SubRecordDef {
        sig: Signature(*b"CNAM"),
        name: "Created Object",
        required: true,
        repeating: false,
        field: FieldType::FormId,
    },
    SubRecordDef {
        sig: Signature(*b"BNAM"),
        name: "Workbench Keyword",
        required: true,
        repeating: false,
        field: FieldType::FormIdTyped(&[Signature(*b"KYWD")]),
    },
    SubRecordDef {
        sig: Signature(*b"NAM1"),
        name: "Created Object Count",
        required: false,
        repeating: false,
        field: FieldType::UInt16,
    },
    SubRecordDef {
        sig: Signature(*b"FNAM"),
        name: "Conditions",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// COBJ — Constructible object (crafting recipe).
pub static COBJ_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"COBJ"),
    name: "Constructible Object",
    members: &COBJ_MEMBERS,
};

static CONT_DATA_FIELDS: [FieldDef; 2] = [
    FieldDef {
        name: "Flags",
        kind: FieldType::UInt8,
    },
    FieldDef {
        name: "Weight",
        kind: FieldType::Float32,
    },
];

static CONT_MEMBERS: [SubRecordDef; 8] = [
    EDID_DEF,
    VMAD_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    SubRecordDef {
        sig: Signature(*b"COCT"),
        name: "Item Count",
        required: false,
        repeating: false,
        field: FieldType::UInt32,
    },
    DEST_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: true,
        repeating: false,
        field: FieldType::Struct(&CONT_DATA_FIELDS),
    },
];

/// CONT — Container (chest, barrel, etc.).
pub static CONT_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"CONT"),
    name: "Container",
    members: &CONT_MEMBERS,
};

static DOOR_MEMBERS: [SubRecordDef; 8] = [
    EDID_DEF,
    VMAD_DEF,
    OBND_DEF,
    FULL_DEF,
    MODL_DEF,
    DEST_DEF,
    YNAM_DEF,
    SubRecordDef {
        sig: Signature(*b"FNAM"),
        name: "Flags",
        required: false,
        repeating: false,
        field: FieldType::UInt8,
    },
];

/// DOOR — Door / teleport marker.
pub static DOOR_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"DOOR"),
    name: "Door",
    members: &DOOR_MEMBERS,
};

static FURN_MEMBERS: [SubRecordDef; 8] = [
    EDID_DEF, VMAD_DEF, OBND_DEF, FULL_DEF, MODL_DEF, DEST_DEF, KSIZ_DEF, KWDA_DEF,
];

/// FURN — Furniture (chair, bench, etc.).
pub static FURN_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"FURN"),
    name: "Furniture",
    members: &FURN_MEMBERS,
};

// NOTE: suppress dead-code warnings for unused enum reference imports.
const _: () = {
    let _ = &SCHOOL_ENUM;
    let _ = &EFIT_DEF;
    let _ = &ZNAM_DEF;
};
