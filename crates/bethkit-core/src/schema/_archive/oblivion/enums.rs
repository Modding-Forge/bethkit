// SPDX-License-Identifier: Apache-2.0
//! Oblivion-specific enum and flags definitions.

use crate::schema::{EnumDef, FlagsDef};

/// Attribute enumeration for Oblivion.
///
/// Used in CLASS DATA (primary attributes) and NPC_/CREA fields.
pub static OBLIVION_ATTRIBUTE_ENUM: EnumDef = EnumDef {
    name: "Attribute",
    values: &[
        (0, "Strength"),
        (1, "Intelligence"),
        (2, "Willpower"),
        (3, "Agility"),
        (4, "Speed"),
        (5, "Endurance"),
        (6, "Personality"),
        (7, "Luck"),
    ],
};

/// Skill enumeration for Oblivion (21 skills).
///
/// Used in CLASS DATA (major skills), BOOK DATA (teaches), and SKIL records.
pub static OBLIVION_SKILL_ENUM: EnumDef = EnumDef {
    name: "Skill",
    values: &[
        (0, "Armorer"),
        (1, "Athletics"),
        (2, "Blade"),
        (3, "Block"),
        (4, "Blunt"),
        (5, "HandToHand"),
        (6, "HeavyArmor"),
        (7, "Alchemy"),
        (8, "Alteration"),
        (9, "Conjuration"),
        (10, "Destruction"),
        (11, "Illusion"),
        (12, "Mysticism"),
        (13, "Restoration"),
        (14, "Acrobatics"),
        (15, "LightArmor"),
        (16, "Marksman"),
        (17, "MercantileSkill"),
        (18, "Security"),
        (19, "Sneak"),
        (20, "Speechcraft"),
    ],
};

/// Magic school enumeration for Oblivion.
///
/// Used in MGEF (magic effect) and SPEL / ENCH records.
pub static OBLIVION_MAGIC_SCHOOL_ENUM: EnumDef = EnumDef {
    name: "MagicSchool",
    values: &[
        (0, "Alteration"),
        (1, "Conjuration"),
        (2, "Destruction"),
        (3, "Illusion"),
        (4, "Mysticism"),
        (5, "Restoration"),
    ],
};

/// Specialization enumeration for Oblivion classes.
pub static OBLIVION_SPECIALIZATION_ENUM: EnumDef = EnumDef {
    name: "Specialization",
    values: &[(0, "Combat"), (1, "Magic"), (2, "Stealth")],
};

/// Creature type enumeration for Oblivion CREA records.
pub static OBLIVION_CREATURE_TYPE_ENUM: EnumDef = EnumDef {
    name: "CreatureType",
    values: &[
        (0, "Creature"),
        (1, "Daedra"),
        (2, "Undead"),
        (3, "Humanoid"),
        (4, "Horse"),
        (5, "Giant"),
    ],
};

/// Dialogue type enumeration for Oblivion DIAL records.
pub static OBLIVION_DIALOGUE_TYPE_ENUM: EnumDef = EnumDef {
    name: "DialogueType",
    values: &[
        (0, "Topic"),
        (1, "Conversation"),
        (2, "Combat"),
        (3, "Persuasion"),
        (4, "Detection"),
        (5, "Service"),
        (6, "Miscellaneous"),
    ],
};

/// Biped slot flags for Oblivion armor and clothing.
pub static OBLIVION_BIPED_FLAGS: FlagsDef = FlagsDef {
    name: "BipedFlags",
    bits: &[
        (0, "Head"),
        (1, "Hair"),
        (2, "UpperBody"),
        (3, "LowerBody"),
        (4, "Hand"),
        (5, "Foot"),
        (6, "RightRing"),
        (7, "LeftRing"),
        (8, "Amulet"),
        (9, "Weapon"),
        (10, "BackWeapon"),
        (11, "SideWeapon"),
        (12, "Quiver"),
        (13, "Shield"),
        (14, "Torch"),
        (15, "Tail"),
    ],
};

/// Soul level enumeration for Oblivion soul gems.
pub static OBLIVION_SOUL_GEM_ENUM: EnumDef = EnumDef {
    name: "SoulLevel",
    values: &[
        (0, "None"),
        (1, "Petty"),
        (2, "Lesser"),
        (3, "Common"),
        (4, "Greater"),
        (5, "Grand"),
    ],
};

// NOTE: The following enums are defined for completeness and will be wired
// NOTE: into SubRecordDef fields once the relevant schemas are expanded.
const _: () = {
    let _ = &OBLIVION_SKILL_ENUM;
    let _ = &OBLIVION_CREATURE_TYPE_ENUM;
    let _ = &OBLIVION_BIPED_FLAGS;
    let _ = &OBLIVION_SOUL_GEM_ENUM;
};
