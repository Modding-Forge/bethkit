// SPDX-License-Identifier: Apache-2.0
//!
//! Fallout 4 specific enum and flags definitions for the schema system.
//!
//! Where an enum is semantically identical to its Skyrim counterpart (e.g.
//! `SCHOOL_ENUM`, `STAGGER_ENUM`) the shared definition from
//! [`crate::schema::enums`] is used directly.  Only FO4-specific variants or
//! extended value sets are defined here.

use crate::schema::{EnumDef, FlagsDef};

/// Biped object slot enumeration for Fallout 4.
///
/// Extends the Skyrim slot set with Power Armor components, Pip-Boy, and
/// additional body slots (slots 30-61).
pub static FO4_BIPED_OBJECT_ENUM: EnumDef = EnumDef {
    name: "BipedObject",
    values: &[
        (30, "Head"),
        (31, "Hair"),
        (32, "Body"),
        (33, "Hands"),
        (34, "Forearms"),
        (35, "Amulet"),
        (36, "Ring"),
        (37, "Feet"),
        (38, "Calves"),
        (39, "Shield"),
        (40, "Tail"),
        (41, "LongHair"),
        (42, "Circlet"),
        (43, "Ear"),
        (44, "BodyAddon3"),
        (45, "Teeth"),
        (46, "BodyAddon4"),
        (47, "PipBoy"),
        (48, "BackPack"),
        (49, "FX01"),
        (60, "DecapitateHead"),
        (61, "Decapitate"),
    ],
};

/// Weapon animation type enumeration for Fallout 4.
///
/// Extends the Skyrim set with `Gun`, `Grenade`, and `Mine`.
pub static FO4_WEAPON_ANIM_TYPE_ENUM: EnumDef = EnumDef {
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

/// Quest type enumeration for Fallout 4.
///
/// Includes faction-specific types (Brotherhood of Steel, Institute,
/// Minutemen, Railroad) and DLC categories not present in Skyrim.
pub static FO4_QUEST_TYPE_ENUM: EnumDef = EnumDef {
    name: "QuestType",
    values: &[
        (0, "None"),
        (1, "MainQuest"),
        (2, "BrotherhoodOfSteel"),
        (3, "Institute"),
        (4, "Minutemen"),
        (5, "Railroad"),
        (6, "Misc"),
        (7, "SideQuest"),
        (8, "Location"),
        (9, "Companion"),
        (10, "CreatureDialogue"),
        (11, "Raid"),
        (12, "Dungeon"),
        (13, "DLC01"),
        (14, "DLC02"),
    ],
};

/// Weapon data flags for Fallout 4.
///
/// Significantly extended compared to Skyrim; includes gun-specific behaviour
/// such as automatic fire, bolt action, and scope handling.
pub static FO4_WEAPON_FLAGS: FlagsDef = FlagsDef {
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

/// NPC configuration flags for Fallout 4.
pub static FO4_NPC_FLAGS: FlagsDef = FlagsDef {
    name: "NpcFlags",
    bits: &[
        (0, "Female"),
        (1, "Essential"),
        (2, "IsCharGenFacePreset"),
        (3, "Respawn"),
        (4, "AutoCalcStats"),
        (5, "Unique"),
        (6, "DoesntAffectStealthMeter"),
        (7, "PCSLevelMult"),
        (9, "UseTemplate"),
        (11, "Protected"),
        (13, "Summonable"),
        (15, "DoesntBleed"),
        (17, "BleedoutOverride"),
        (18, "OppositeGenderAnims"),
        (19, "SimpleActor"),
        (22, "IsGhost"),
        (29, "Invulnerable"),
    ],
};

/// Sound level enumeration for Fallout 4.
pub static FO4_SOUND_LEVEL_ENUM: EnumDef = EnumDef {
    name: "SoundLevel",
    values: &[
        (0, "Loud"),
        (1, "Normal"),
        (2, "Silent"),
        (3, "VeryLoud"),
        (4, "Quiet"),
    ],
};

/// Object modification (OMOD) property type enumeration.
pub static OMOD_PROPERTY_ENUM: EnumDef = EnumDef {
    name: "OmodPropertyType",
    values: &[
        (0, "Value_Is_FormID"),
        (1, "Value_Is_Float"),
        (2, "Value_Is_Bool"),
        (3, "Value_Is_Int"),
    ],
};

/// Note type enumeration for NOTE records.
pub static NOTE_TYPE_ENUM: EnumDef = EnumDef {
    name: "NoteType",
    values: &[(0, "Sound"), (1, "Voice"), (2, "Program"), (3, "Terminal")],
};

// NOTE: FO4_SOUND_LEVEL_ENUM is defined but not yet wired into a SubRecordDef;
// NOTE: it will be used by SNDR's LNAM field once that subrecord is detailed.
const _: () = { let _ = &FO4_SOUND_LEVEL_ENUM; };
