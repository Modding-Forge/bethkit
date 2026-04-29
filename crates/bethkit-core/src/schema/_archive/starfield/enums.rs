// SPDX-License-Identifier: Apache-2.0
//!
//! Starfield-specific enum and flags definitions for the schema system.
//!
//! Enums shared with Skyrim SE or Fallout 4 use the definitions from
//! [`crate::schema::enums`]. Only Starfield-specific variants or extended
//! value sets are defined here.

use crate::schema::{EnumDef, FlagsDef};


/// Cause-of-damage enumeration introduced in Starfield.
///
/// Used in DMGT (Damage Type) records and SDLT (Secondary Damage List) entries.
pub static SF_DAMAGE_CAUSE_TYPE_ENUM: EnumDef = EnumDef {
    name: "DamageCauseType",
    values: &[
        (0, "Explosion"),
        (1, "Gun"),
        (2, "BluntWeapon"),
        (3, "HandToHand"),
        (4, "ObjectImpact"),
        (5, "Poison"),
        (6, "Decapitation"),
        (7, "Falling"),
        (8, "Drowning"),
        (9, "Trap"),
        (10, "ShipDestroyed"),
        (11, "Other"),
    ],
};


/// Perk category enumeration for Starfield's perk tree.
///
/// Replaces Skyrim's skill-based system with a flat category system.
pub static SF_PERK_CATEGORY_ENUM: EnumDef = EnumDef {
    name: "PerkCategory",
    values: &[
        (0, "None"),
        (1, "Combat"),
        (2, "Science"),
        (3, "Tech"),
        (4, "Physical"),
        (5, "Social"),
        (6, "All"),
    ],
};

/// Perk skill group (rank tier) for Starfield.
///
/// Perks are gated by skill rank rather than by individual skill level.
pub static SF_PERK_SKILL_GROUP_ENUM: EnumDef = EnumDef {
    name: "PerkSkillGroup",
    values: &[
        (0, "None"),
        (1, "Basic"),
        (2, "Advanced"),
        (3, "Expert"),
        (4, "Master"),
    ],
};


/// How a crafting recipe is learned (RSPJ — Research Project).
pub static SF_LEARN_METHOD_ENUM: EnumDef = EnumDef {
    name: "LearnMethod",
    values: &[
        (0, "PickedUp"),
        (1, "Scrapping"),
        (2, "Ingested"),
        (3, "Default"),
        (4, "Plan"),
    ],
};


/// Quest type enumeration for Starfield.
///
/// Faction-specific types reflect Starfield's major factions (Constellation,
/// UC, Freestar, Crimson Fleet) rather than Fallout 4's factions.
pub static SF_QUEST_TYPE_ENUM: EnumDef = EnumDef {
    name: "QuestType",
    values: &[
        (0, "None"),
        (1, "MainQuest"),
        (2, "Constellation"),
        (3, "UC"),
        (4, "Freestar"),
        (5, "CrimsonFleet"),
        (6, "Misc"),
        (7, "SideQuest"),
        (8, "Location"),
        (9, "Companion"),
        (10, "CreatureDialogue"),
        (11, "Dungeon"),
    ],
};


/// Biped object slot enumeration for Starfield.
///
/// Extends the Fallout 4 set with spacesuit-specific slots.
pub static SF_BIPED_OBJECT_ENUM: EnumDef = EnumDef {
    name: "BipedObject",
    values: &[
        (0, "None"),
        (30, "Head"),
        (31, "Body"),
        (32, "Hands"),
        (33, "Feet"),
        (34, "Shield"),
        (35, "Tail"),
        (36, "LongHair"),
        (37, "Circlet"),
        (38, "Ear"),
        (39, "Amulet"),
        (40, "Ring"),
        (41, "Jetpack"),
        (42, "Backpack"),
        (43, "FaceMask"),
        (44, "SpacesuitHelmet"),
        (45, "SpacesuitBody"),
        (46, "SpacesuitBack"),
        (47, "DecapitateHead"),
        (48, "Decapitate"),
    ],
};

/// Weapon animation type for Starfield.
pub static SF_WEAPON_ANIM_TYPE_ENUM: EnumDef = EnumDef {
    name: "WeaponAnimationType",
    values: &[
        (0, "HandToHandMelee"),
        (1, "OneHandSword"),
        (2, "OneHandDagger"),
        (3, "OneHandAxe"),
        (4, "OneHandMace"),
        (5, "TwoHandSword"),
        (6, "TwoHandAxe"),
        (7, "Bow"),
        (8, "Staff"),
        (9, "Gun"),
        (10, "Grenade"),
        (11, "Mine"),
    ],
};

/// NPC_ configuration flags for Starfield.
pub static SF_NPC_FLAGS: FlagsDef = FlagsDef {
    name: "NPCFlags",
    bits: &[
        (0, "Female"),
        (1, "Essential"),
        (2, "IsCharGenFacePreset"),
        (3, "Respawn"),
        (4, "AutoCalcStats"),
        (7, "PCLevelMult"),
        (8, "UseTemplate"),
        (11, "Protected"),
        (14, "Summonable"),
        (15, "DoesNotBleed"),
        (17, "BleedoutOverride"),
        (18, "OppositeGenderAnims"),
        (19, "SimpleActor"),
        (20, "NoActivationOrHellos"),
        (26, "Invulnerable"),
        (27, "DoesNotAffectStealth"),
        (28, "FlieswithoutPackage"),
    ],
};

/// Weapon data flags for Starfield.
pub static SF_WEAPON_FLAGS: FlagsDef = FlagsDef {
    name: "WeaponFlags",
    bits: &[
        (0, "PlayerOnly"),
        (1, "NPCsUseAmmo"),
        (2, "NoJamAfterReload"),
        (3, "ChargingReload"),
        (4, "MinorCrime"),
        (5, "RangeFixed"),
        (6, "NotUsedInNormalCombat"),
        (7, "ChargingAttack"),
        (8, "HoldInputToPower"),
        (9, "NonHostile"),
        (10, "BoundWeapon"),
        (11, "IgnoresNormalWeaponResistance"),
        (12, "Automatic"),
        (13, "RepeatableSingleFire"),
        (14, "CantDrop"),
        (15, "HideBackpack"),
        (16, "EmbeddedWeapon"),
        (17, "NotPlayable"),
        (18, "HasScope"),
        (19, "BoltAction"),
        (20, "SecondaryWeapon"),
        (21, "DisableShells"),
    ],
};

// NOTE: The following values are defined for schema completeness and will be
// NOTE: wired into SubRecordDef fields when the relevant schemas are expanded.
const _: () = {
    let _ = &SF_DAMAGE_CAUSE_TYPE_ENUM;
    let _ = &SF_PERK_CATEGORY_ENUM;
    let _ = &SF_PERK_SKILL_GROUP_ENUM;
    let _ = &SF_LEARN_METHOD_ENUM;
    let _ = &SF_BIPED_OBJECT_ENUM;
};
