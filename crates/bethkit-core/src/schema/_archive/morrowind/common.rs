// SPDX-License-Identifier: Apache-2.0
//! Reusable static [`SubRecordDef`] helpers shared across Morrowind (TES3)
//! record definitions.
//!
//! Morrowind uses string-based record references (null-terminated `ZString`
//! EditorIDs) instead of FormIDs. The primary EditorID subrecord is `NAME`
//! (not `EDID` as in later games). The full display name uses `FNAM` (not
//! `FULL`), and the icon texture uses `ITEX` (not `ICON`).

use crate::schema::{FieldDef, FieldType, SubRecordDef};
use crate::types::Signature;

pub use crate::schema::shared::MODL_DEF;


/// `NAME` — EditorID / primary record key (null-terminated ZString).
///
/// In Morrowind, `NAME` serves the role of `EDID` in later Bethesda games.
pub static NAME_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"NAME"),
    name: "Editor ID",
    required: true,
    repeating: false,
    field: FieldType::ZString,
};

/// `FNAM` — Full display name (non-localised ZString in Morrowind).
pub static FNAM_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"FNAM"),
    name: "Full Name",
    required: false,
    repeating: false,
    field: FieldType::ZString,
};

/// `ITEX` — Icon texture filename (replaces `ICON` from later games).
pub static ITEX_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"ITEX"),
    name: "Icon Filename",
    required: false,
    repeating: false,
    field: FieldType::ZString,
};

/// `SCRI` — Script reference (ZString EditorID → `SCPT`).
pub static SCRI_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"SCRI"),
    name: "Script",
    required: false,
    repeating: false,
    field: FieldType::ZString,
};

/// `DESC` — Description text (non-localised ZString in Morrowind).
pub static DESC_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"DESC"),
    name: "Description",
    required: false,
    repeating: false,
    field: FieldType::ZString,
};

/// `DELE` — Deleted-record marker (`u32`, value always zero).
pub static DELE_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"DELE"),
    name: "Deleted Marker",
    required: false,
    repeating: false,
    field: FieldType::UInt32,
};


/// Fields of a single `ENAM` spell effect entry (24 bytes).
static SPELL_EFFECT_FIELDS: [FieldDef; 8] = [
    FieldDef { name: "Magic Effect", kind: FieldType::Int16 },
    FieldDef { name: "Skill",        kind: FieldType::Int8 },
    FieldDef { name: "Attribute",    kind: FieldType::Int8 },
    FieldDef { name: "Range",        kind: FieldType::UInt32 },
    FieldDef { name: "Area",         kind: FieldType::UInt32 },
    FieldDef { name: "Duration",     kind: FieldType::UInt32 },
    FieldDef { name: "Magnitude Min", kind: FieldType::UInt32 },
    FieldDef { name: "Magnitude Max", kind: FieldType::UInt32 },
];

/// `ENAM` — Spell or enchantment effect entry (repeating, 24 bytes per entry).
///
/// Used in `SPEL`, `ENCH`, and `ALCH` records.
pub static ENAM_EFFECT_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"ENAM"),
    name: "Effect",
    required: false,
    repeating: true,
    field: FieldType::Struct(&SPELL_EFFECT_FIELDS),
};


/// Fields of the `AIDT` AI data subrecord (12 bytes).
static AIDT_FIELDS: [FieldDef; 6] = [
    FieldDef { name: "Hello",         kind: FieldType::UInt16 },
    FieldDef { name: "Fight",         kind: FieldType::UInt8 },
    FieldDef { name: "Flee",          kind: FieldType::UInt8 },
    FieldDef { name: "Alarm",         kind: FieldType::UInt8 },
    FieldDef { name: "_unused",       kind: FieldType::Unused(3) },
    FieldDef { name: "Service Flags", kind: FieldType::UInt32 },
];

/// `AIDT` — NPC / creature AI behaviour data.
pub static AIDT_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"AIDT"),
    name: "AI Data",
    required: false,
    repeating: false,
    field: FieldType::Struct(&AIDT_FIELDS),
};


/// Fields of an `NPCO` inventory entry (36 bytes).
static NPCO_FIELDS: [FieldDef; 2] = [
    FieldDef { name: "Count",       kind: FieldType::Int32 },
    FieldDef { name: "Item Editor ID", kind: FieldType::ByteArray },
];

/// `NPCO` — Inventory item entry (repeating, 36 bytes: count + 32-byte EditorID).
pub static NPCO_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"NPCO"),
    name: "Inventory Item",
    required: false,
    repeating: true,
    field: FieldType::Struct(&NPCO_FIELDS),
};

/// `NPCS` — Known spell reference (repeating, 32-byte null-padded EditorID → `SPEL`).
pub static NPCS_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"NPCS"),
    name: "Known Spell",
    required: false,
    repeating: true,
    field: FieldType::ByteArray,
};


/// `DODT` — Travel service destination (position + rotation, 24 bytes).
pub static DODT_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"DODT"),
    name: "Travel Destination",
    required: false,
    repeating: true,
    field: FieldType::ByteArray,
};

/// `DNAM` — Travel service destination cell name (ZString → `CELL`).
pub static DNAM_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"DNAM"),
    name: "Travel Destination Cell",
    required: false,
    repeating: true,
    field: FieldType::ZString,
};


/// `AI_W` — Wander AI package (raw binary, 14 bytes).
pub static AI_W_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"AI_W"),
    name: "AI Wander Package",
    required: false,
    repeating: true,
    field: FieldType::ByteArray,
};

/// `AI_T` — Travel AI package (raw binary, 16 bytes).
pub static AI_T_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"AI_T"),
    name: "AI Travel Package",
    required: false,
    repeating: true,
    field: FieldType::ByteArray,
};

/// `AI_F` — Follow AI package (raw binary, variable length + optional `CNDT`).
pub static AI_F_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"AI_F"),
    name: "AI Follow Package",
    required: false,
    repeating: true,
    field: FieldType::ByteArray,
};

/// `AI_E` — Escort AI package (raw binary, variable length + optional `CNDT`).
pub static AI_E_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"AI_E"),
    name: "AI Escort Package",
    required: false,
    repeating: true,
    field: FieldType::ByteArray,
};

/// `AI_A` — Activate AI package (raw binary, 33 bytes).
pub static AI_A_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"AI_A"),
    name: "AI Activate Package",
    required: false,
    repeating: true,
    field: FieldType::ByteArray,
};


/// `XSCL` — Object scale factor (`f32`).
pub static XSCL_DEF: SubRecordDef = SubRecordDef {
    sig: Signature(*b"XSCL"),
    name: "Scale",
    required: false,
    repeating: false,
    field: FieldType::Float32,
};
