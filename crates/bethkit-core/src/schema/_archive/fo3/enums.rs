// SPDX-License-Identifier: Apache-2.0
//!
//! Fallout 3 enumeration and flag definitions used by record schemas.

use crate::schema::{EnumDef, FlagsDef};


/// Fallout 3 primary actor value enum (S.P.E.C.I.A.L. stats + derived values).
pub static FO3_ACTOR_VALUE_ENUM: EnumDef = EnumDef {
    name: "ActorValue",
    values: &[
        (0,  "Strength"),
        (1,  "Perception"),
        (2,  "Endurance"),
        (3,  "Charisma"),
        (4,  "Intelligence"),
        (5,  "Agility"),
        (6,  "Luck"),
        (7,  "ActionPoints"),
        (8,  "CarryWeight"),
        (9,  "CritChance"),
        (10, "HealRate"),
        (11, "Health"),
        (12, "MeleeDamage"),
        (13, "DamageResistance"),
        (14, "PoisonResistance"),
        (15, "RadResistance"),
        (16, "SpeedMultiplier"),
        (17, "Fatigue"),
        (18, "Karma"),
        (19, "XPReward"),
        (20, "PerceptionCondition"),
        (21, "EnduranceCondition"),
    ],
};


/// Fallout 3 weapon type enum (governs animation set and combat calculations).
pub static FO3_WEAPON_TYPE_ENUM: EnumDef = EnumDef {
    name: "WeaponType",
    values: &[
        (0,  "BigGuns"),
        (1,  "EnergyWeapons"),
        (2,  "SmallGuns"),
        (3,  "MeleeWeapons"),
        (4,  "UnarmedWeapons"),
        (5,  "ThrownWeapons"),
        (6,  "Mine"),
        (7,  "BodyWear"),
        (8,  "HeadWear"),
        (9,  "HandWear"),
        (10, "Chems"),
    ],
};


/// Fallout 3 equipment slot type enum.
pub static FO3_EQUIP_TYPE_ENUM: EnumDef = EnumDef {
    name: "EquipType",
    values: &[
        (0,  "None"),
        (1,  "BigGuns"),
        (2,  "EnergyWeapons"),
        (3,  "SmallGuns"),
        (4,  "MeleeWeapons"),
        (5,  "UnarmedWeapons"),
        (6,  "ThrownWeapons"),
        (7,  "Mine"),
        (8,  "BodyWear"),
        (9,  "HeadWear"),
        (10, "HandWear"),
        (11, "Chems"),
        (12, "Stimpack"),
    ],
};


/// Fallout 3 biped equipment slot flags (bitmask over 20 slots).
pub static FO3_BIPED_OBJECT_FLAGS: FlagsDef = FlagsDef {
    name: "BipedObject",
    bits: &[
        (0,  "Head"),
        (1,  "Hair"),
        (2,  "UpperBody"),
        (3,  "LeftHand"),
        (4,  "RightHand"),
        (5,  "Weapon"),
        (6,  "PipBoy"),
        (7,  "Backpack"),
        (8,  "Necklace"),
        (9,  "Headband"),
        (10, "Hat"),
        (11, "Eyeglasses"),
        (12, "Nosering"),
        (13, "Earrings"),
        (14, "Mask"),
        (15, "Choker"),
        (16, "MouthObject"),
        (17, "BodyAddon1"),
        (18, "BodyAddon2"),
        (19, "BodyAddon3"),
    ],
};


/// Fallout 3 creature type enum.
pub static FO3_CREATURE_TYPE_ENUM: EnumDef = EnumDef {
    name: "CreatureType",
    values: &[
        (0, "Animal"),
        (1, "Mutant"),
        (2, "Robot"),
        (3, "Creature"),
        (4, "Abomination"),
        (5, "FlameCreature"),
        (6, "GiantCreature"),
    ],
};


/// Fallout 3 NPC data flags (bitmask in the ACBS sub-record).
pub static FO3_NPC_FLAGS: FlagsDef = FlagsDef {
    name: "NpcFlags",
    bits: &[
        (0,  "Female"),
        (1,  "Essential"),
        (2,  "IsChargenFacePreset"),
        (3,  "Respawn"),
        (4,  "AutoCalcStats"),
        (7,  "PCLevelMult"),
        (8,  "UseTemplate"),
        (9,  "NoLowLevel"),
        (11, "NoBloodSpray"),
        (12, "NoBloodDecal"),
        (20, "NoVATSMelee"),
        (22, "CanBeAllRaces"),
        (26, "NoKnockdowns"),
        (27, "NotPushable"),
    ],
};

// NOTE: suppress dead_code for enums not yet referenced by record schemas
const _: () = {
    let _ = &FO3_ACTOR_VALUE_ENUM;
    let _ = &FO3_WEAPON_TYPE_ENUM;
    let _ = &FO3_EQUIP_TYPE_ENUM;
    let _ = &FO3_BIPED_OBJECT_FLAGS;
    let _ = &FO3_CREATURE_TYPE_ENUM;
    let _ = &FO3_NPC_FLAGS;
};
