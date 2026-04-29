// SPDX-License-Identifier: Apache-2.0
//! Morrowind (TES3) enumeration and flag definitions used by record schemas.

use crate::schema::{EnumDef, FlagsDef};


/// Morrowind primary attribute enum (S.P.E.C.I.A.L. equivalent for TES3).
pub static TES3_ATTRIBUTE_ENUM: EnumDef = EnumDef {
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


/// Morrowind skill enum (27 skills, also used as i8 index in effect data).
pub static TES3_SKILL_ENUM: EnumDef = EnumDef {
    name: "Skill",
    values: &[
        (0,  "Block"),
        (1,  "Armorer"),
        (2,  "Medium Armor"),
        (3,  "Heavy Armor"),
        (4,  "Blunt Weapon"),
        (5,  "Long Blade"),
        (6,  "Axe"),
        (7,  "Spear"),
        (8,  "Athletics"),
        (9,  "Enchant"),
        (10, "Destruction"),
        (11, "Alteration"),
        (12, "Illusion"),
        (13, "Conjuration"),
        (14, "Mysticism"),
        (15, "Restoration"),
        (16, "Alchemy"),
        (17, "Unarmored"),
        (18, "Security"),
        (19, "Sneak"),
        (20, "Acrobatics"),
        (21, "Light Armor"),
        (22, "Short Blade"),
        (23, "Marksman"),
        (24, "Mercantile"),
        (25, "Speechcraft"),
        (26, "Hand-To-Hand"),
    ],
};


/// Morrowind class/skill specialization enum.
pub static TES3_SPECIALIZATION_ENUM: EnumDef = EnumDef {
    name: "Specialization",
    values: &[
        (0, "Combat"),
        (1, "Magic"),
        (2, "Stealth"),
    ],
};


/// Morrowind dialog topic type enum.
pub static TES3_DIALOG_TYPE_ENUM: EnumDef = EnumDef {
    name: "DialogType",
    values: &[
        (0, "Regular Topic"),
        (1, "Voice"),
        (2, "Greeting"),
        (3, "Persuasion"),
        (4, "Journal"),
    ],
};


/// Morrowind magic school enum (used in `MGEF` `MEDT` subrecord).
pub static TES3_MAGIC_SCHOOL_ENUM: EnumDef = EnumDef {
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


/// Morrowind magic effect ID enum (143 effects, IDs 0–142).
///
/// Used as `i16` in spell effect structs; −1 means none.
pub static TES3_MAGIC_EFFECT_ENUM: EnumDef = EnumDef {
    name: "MagicEffect",
    values: &[
        (0,   "Water Breathing"),
        (1,   "Swift Swim"),
        (2,   "Water Walking"),
        (3,   "Shield"),
        (4,   "Fire Shield"),
        (5,   "Lightning Shield"),
        (6,   "Frost Shield"),
        (7,   "Burden"),
        (8,   "Feather"),
        (9,   "Jump"),
        (10,  "Levitate"),
        (11,  "Slow Fall"),
        (12,  "Lock"),
        (13,  "Open"),
        (14,  "Fire Damage"),
        (15,  "Shock Damage"),
        (16,  "Frost Damage"),
        (17,  "Drain Attribute"),
        (18,  "Drain Health"),
        (19,  "Drain Magicka"),
        (20,  "Drain Fatigue"),
        (21,  "Drain Skill"),
        (22,  "Damage Attribute"),
        (23,  "Damage Health"),
        (24,  "Damage Magicka"),
        (25,  "Damage Fatigue"),
        (26,  "Damage Skill"),
        (27,  "Poison"),
        (28,  "Weakness To Fire"),
        (29,  "Weakness To Frost"),
        (30,  "Weakness To Shock"),
        (31,  "Weakness To Magicka"),
        (32,  "Weakness To Common Disease"),
        (33,  "Weakness To Blight Disease"),
        (34,  "Weakness To Corprus Disease"),
        (35,  "Weakness To Poison"),
        (36,  "Weakness To Normal Weapons"),
        (37,  "Disintegrate Weapon"),
        (38,  "Disintegrate Armor"),
        (39,  "Invisibility"),
        (40,  "Chameleon"),
        (41,  "Light"),
        (42,  "Sanctuary"),
        (43,  "Night Eye"),
        (44,  "Charm"),
        (45,  "Paralyze"),
        (46,  "Silence"),
        (47,  "Blind"),
        (48,  "Sound"),
        (49,  "Calm Humanoid"),
        (50,  "Calm Creature"),
        (51,  "Frenzy Humanoid"),
        (52,  "Frenzy Creature"),
        (53,  "Demoralize Humanoid"),
        (54,  "Demoralize Creature"),
        (55,  "Rally Humanoid"),
        (56,  "Rally Creature"),
        (57,  "Dispel"),
        (58,  "Soultrap"),
        (59,  "Telekinesis"),
        (60,  "Mark"),
        (61,  "Recall"),
        (62,  "Divine Intervention"),
        (63,  "Almsivi Intervention"),
        (64,  "Detect Animal"),
        (65,  "Detect Enchantment"),
        (66,  "Detect Key"),
        (67,  "Spell Absorption"),
        (68,  "Reflect"),
        (69,  "Cure Common Disease"),
        (70,  "Cure Blight Disease"),
        (71,  "Cure Corprus Disease"),
        (72,  "Cure Poison"),
        (73,  "Cure Paralyzation"),
        (74,  "Restore Attribute"),
        (75,  "Restore Health"),
        (76,  "Restore Magicka"),
        (77,  "Restore Fatigue"),
        (78,  "Restore Skill"),
        (79,  "Fortify Attribute"),
        (80,  "Fortify Health"),
        (81,  "Fortify Magicka"),
        (82,  "Fortify Fatigue"),
        (83,  "Fortify Skill"),
        (84,  "Fortify Maximum Magicka"),
        (85,  "Absorb Attribute"),
        (86,  "Absorb Health"),
        (87,  "Absorb Magicka"),
        (88,  "Absorb Fatigue"),
        (89,  "Absorb Skill"),
        (90,  "Resist Fire"),
        (91,  "Resist Frost"),
        (92,  "Resist Shock"),
        (93,  "Resist Magicka"),
        (94,  "Resist Common Disease"),
        (95,  "Resist Blight Disease"),
        (96,  "Resist Corprus Disease"),
        (97,  "Resist Poison"),
        (98,  "Resist Normal Weapons"),
        (99,  "Resist Paralysis"),
        (100, "Remove Curse"),
        (101, "Turn Undead"),
        (102, "Summon Scamp"),
        (103, "Summon Clannfear"),
        (104, "Summon Daedroth"),
        (105, "Summon Dremora"),
        (106, "Summon Ancestral Ghost"),
        (107, "Summon Skeletal Minion"),
        (108, "Summon Bonewalker"),
        (109, "Summon Greater Bonewalker"),
        (110, "Summon Bonelord"),
        (111, "Summon Winged Twilight"),
        (112, "Summon Hunger"),
        (113, "Summon Golden Saint"),
        (114, "Summon Flame Atronach"),
        (115, "Summon Frost Atronach"),
        (116, "Summon Storm Atronach"),
        (117, "Fortify Attack"),
        (118, "Command Creature"),
        (119, "Command Humanoid"),
        (120, "Bound Dagger"),
        (121, "Bound Longsword"),
        (122, "Bound Mace"),
        (123, "Bound Battle Axe"),
        (124, "Bound Spear"),
        (125, "Bound Longbow"),
        (126, "Extra Spell"),
        (127, "Bound Cuirass"),
        (128, "Bound Helm"),
        (129, "Bound Boots"),
        (130, "Bound Shield"),
        (131, "Bound Gloves"),
        (132, "Corpus"),
        (133, "Vampirism"),
        (134, "Summon Centurion Sphere"),
        (135, "Sun Damage"),
        (136, "Stunted Magicka"),
        (137, "Summon Fabricant"),
        (138, "Summon Creature 01"),
        (139, "Summon Creature 02"),
        (140, "Summon Creature 03"),
        (141, "Summon Creature 04"),
        (142, "Summon Creature 05"),
    ],
};


/// Morrowind NPC flag bits (stored in the `FLAG` subrecord).
pub static TES3_NPC_FLAGS: FlagsDef = FlagsDef {
    name: "NpcFlags",
    bits: &[
        (0,  "Female"),
        (1,  "Essential"),
        (2,  "Respawn"),
        (3,  "AutoCalculateStats"),
        (10, "SkeletonBlood"),
        (11, "MetalBlood"),
    ],
};


/// Morrowind creature flag bits (stored in the `FLAG` subrecord).
pub static TES3_CREATURE_FLAGS: FlagsDef = FlagsDef {
    name: "CreatureFlags",
    bits: &[
        (0,  "Biped"),
        (1,  "Respawn"),
        (2,  "WeaponAndShield"),
        (3,  "CanHoldItems"),
        (4,  "Swims"),
        (5,  "Flies"),
        (6,  "Walks"),
        (7,  "Essential"),
        (10, "SkeletonBlood"),
        (11, "MetalBlood"),
    ],
};


/// Morrowind cell flags (stored in the `DATA` subrecord).
pub static TES3_CELL_FLAGS: FlagsDef = FlagsDef {
    name: "CellFlags",
    bits: &[
        (0, "IsInterior"),
        (1, "HasWater"),
        (2, "IllegalToSleep"),
        (6, "HasMapColor"),
        (7, "BehaveExteriorLike"),
    ],
};


/// Morrowind leveled list flags (`LEVC` / `LEVI` `DATA` subrecord).
pub static TES3_LEVELED_FLAGS: FlagsDef = FlagsDef {
    name: "LeveledFlags",
    bits: &[
        (0, "CalculateFromAllLevels"),
        (1, "CalculateForEachItem"),
    ],
};

// NOTE: suppress dead_code for enums / flags not yet referenced by record schemas
const _: () = {
    let _ = &TES3_ATTRIBUTE_ENUM;
    let _ = &TES3_SKILL_ENUM;
    let _ = &TES3_SPECIALIZATION_ENUM;
    let _ = &TES3_DIALOG_TYPE_ENUM;
    let _ = &TES3_MAGIC_SCHOOL_ENUM;
    let _ = &TES3_MAGIC_EFFECT_ENUM;
    let _ = &TES3_NPC_FLAGS;
    let _ = &TES3_CREATURE_FLAGS;
    let _ = &TES3_CELL_FLAGS;
    let _ = &TES3_LEVELED_FLAGS;
};
