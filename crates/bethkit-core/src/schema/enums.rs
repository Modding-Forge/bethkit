// SPDX-License-Identifier: Apache-2.0
//!
//! Static enum and flags definitions for Skyrim SE records.
//!
//! All definitions are `&'static` instances of [`EnumDef`] and [`FlagsDef`]
//! suitable for use in [`FieldType::Enum`] and [`FieldType::Flags`].

use super::{EnumDef, FlagsDef};

/// Enum for the 44 actor value indices used in AVIF records and skill/stat
/// references throughout the game.
pub static ACTOR_VALUE_ENUM: EnumDef = EnumDef {
    name: "ActorValue",
    values: &[
        (0, "Aggression"),
        (1, "Confidence"),
        (2, "Energy"),
        (3, "Morality"),
        (4, "Mood"),
        (5, "Assistance"),
        (6, "OneHanded"),
        (7, "TwoHanded"),
        (8, "Archery"),
        (9, "Block"),
        (10, "Smithing"),
        (11, "HeavyArmor"),
        (12, "LightArmor"),
        (13, "Pickpocket"),
        (14, "Lockpicking"),
        (15, "Sneak"),
        (16, "Alchemy"),
        (17, "Speech"),
        (18, "Alteration"),
        (19, "Conjuration"),
        (20, "Destruction"),
        (21, "Illusion"),
        (22, "Restoration"),
        (23, "Enchanting"),
        (24, "Health"),
        (25, "Magicka"),
        (26, "Stamina"),
        (27, "HealRate"),
        (28, "MagickaRate"),
        (29, "StaminaRate"),
        (30, "SpeedMult"),
        (31, "InventoryWeight"),
        (32, "CarryWeight"),
        (33, "CriticalChance"),
        (34, "MeleeDamage"),
        (35, "UnarmedDamage"),
        (36, "Mass"),
        (37, "VoicePoints"),
        (38, "VoiceRate"),
        (39, "DamageResist"),
        (40, "PoisonResist"),
        (41, "ResistFire"),
        (42, "ResistShock"),
        (43, "ResistFrost"),
        (44, "ResistMagic"),
        (45, "ResistDisease"),
        (46, "Unknown46"),
        (47, "Unknown47"),
        (48, "Unknown48"),
        (49, "Unknown49"),
        (50, "Unknown50"),
        (51, "Unknown51"),
        (52, "Unknown52"),
        (53, "Paralysis"),
        (54, "Invisibility"),
        (55, "NightEye"),
        (56, "DetectLifeRange"),
        (57, "WaterBreathing"),
        (58, "WaterWalking"),
        (59, "Unknown59"),
        (60, "Fame"),
        (61, "Infamy"),
        (62, "JumpingBonus"),
        (63, "WardPower"),
        (64, "RightItemCharge"),
        (65, "ArmorPerks"),
        (66, "ShieldPerks"),
        (67, "WardDeflection"),
        (68, "Variable01"),
        (69, "Variable02"),
        (70, "Variable03"),
        (71, "Variable04"),
        (72, "Variable05"),
        (73, "Variable06"),
        (74, "Variable07"),
        (75, "Variable08"),
        (76, "Variable09"),
        (77, "Variable10"),
        (78, "BowSpeedBonus"),
        (79, "FavorActive"),
        (80, "FavorsPerDay"),
        (81, "FavorsPerDayTimer"),
        (82, "LeftItemCharge"),
        (83, "AbsorbChance"),
        (84, "Blindness"),
        (85, "WeaponSpeedMult"),
        (86, "ShoutRecoveryMult"),
        (87, "BowStaggerBonus"),
        (88, "Telekinesis"),
        (89, "FavorPointsBonus"),
        (90, "LastBribedIntimidated"),
        (91, "LastFlattered"),
        (92, "MovementNoiseMult"),
        (93, "BypassVendorStolenCheck"),
        (94, "BypassVendorKeywordCheck"),
        (95, "WaitingForPlayer"),
        (96, "OneHandedMod"),
        (97, "TwoHandedMod"),
        (98, "ArcheryMod"),
        (99, "BlockMod"),
        (100, "SmithingMod"),
        (101, "HeavyArmorMod"),
        (102, "LightArmorMod"),
        (103, "PickpocketMod"),
        (104, "LockpickingMod"),
        (105, "SneakMod"),
        (106, "AlchemyMod"),
        (107, "SpeechMod"),
        (108, "AlterationMod"),
        (109, "ConjurationMod"),
        (110, "DestructionMod"),
        (111, "IllusionMod"),
        (112, "RestorationMod"),
        (113, "EnchantingMod"),
        (114, "OneHandedSkillAdvance"),
        (115, "TwoHandedSkillAdvance"),
        (116, "ArcherySkillAdvance"),
        (117, "BlockSkillAdvance"),
        (118, "SmithingSkillAdvance"),
        (119, "HeavyArmorSkillAdvance"),
        (120, "LightArmorSkillAdvance"),
        (121, "PickpocketSkillAdvance"),
        (122, "LockpickingSkillAdvance"),
        (123, "SneakSkillAdvance"),
        (124, "AlchemySkillAdvance"),
        (125, "SpeechSkillAdvance"),
        (126, "AlterationSkillAdvance"),
        (127, "ConjurationSkillAdvance"),
        (128, "DestructionSkillAdvance"),
        (129, "IllusionSkillAdvance"),
        (130, "RestorationSkillAdvance"),
        (131, "EnchantingSkillAdvance"),
        (132, "LeftWeaponSpeedMult"),
        (133, "DragonSouls"),
        (134, "CombatHealthRegenMult"),
        (135, "OneHandedPowerMod"),
        (136, "TwoHandedPowerMod"),
        (137, "ArcheryPowerMod"),
        (138, "BlockPowerMod"),
        (139, "SmithingPowerMod"),
        (140, "HeavyArmorPowerMod"),
        (141, "LightArmorPowerMod"),
        (142, "PickpocketPowerMod"),
        (143, "LockpickingPowerMod"),
        (144, "SneakPowerMod"),
        (145, "AlchemyPowerMod"),
        (146, "SpeechPowerMod"),
        (147, "AlterationPowerMod"),
        (148, "ConjurationPowerMod"),
        (149, "DestructionPowerMod"),
        (150, "IllusionPowerMod"),
        (151, "RestorationPowerMod"),
        (152, "EnchantingPowerMod"),
        (153, "DragonRend"),
        (154, "AttackDamageMult"),
        (155, "HealRateMult"),
        (156, "MagickaRateMult"),
        (157, "StaminaRateMult"),
        (158, "WerewolfPerks"),
        (159, "VampirePerks"),
        (160, "GrabActorOffset"),
        (161, "Grabbed"),
        (162, "Unknown162"),
        (163, "ReflectDamage"),
    ],
};

/// Enum for the six schools of magic.
pub static SCHOOL_ENUM: EnumDef = EnumDef {
    name: "School",
    values: &[
        (0, "Alteration"),
        (1, "Conjuration"),
        (2, "Destruction"),
        (3, "Illusion"),
        (4, "Restoration"),
        (5, "Enchanting"),
    ],
};

/// Enum for weapon animation types (DNAM.animationType).
pub static WEAPON_ANIM_TYPE_ENUM: EnumDef = EnumDef {
    name: "WeaponAnimationType",
    values: &[
        (0, "HandToHand"),
        (1, "OneHandSword"),
        (2, "OneHandDagger"),
        (3, "OneHandAxe"),
        (4, "OneHandMace"),
        (5, "TwoHandSword"),
        (6, "TwoHandAxe"),
        (7, "Bow"),
        (8, "Staff"),
        (9, "Crossbow"),
    ],
};

/// Enum for stagger values on weapons.
pub static STAGGER_ENUM: EnumDef = EnumDef {
    name: "Stagger",
    values: &[
        (0, "None"),
        (1, "Small"),
        (2, "Medium"),
        (3, "Large"),
        (4, "Largest"),
    ],
};

/// Enum for armor type (DNAM in ARMO/ARMA).
pub static ARMOR_TYPE_ENUM: EnumDef = EnumDef {
    name: "ArmorType",
    values: &[(0, "LightArmor"), (1, "HeavyArmor"), (2, "Clothing")],
};

/// Enum for biped object slots.
// NOTE: Reserved for future ARMA / HDPT schema expansion.
#[allow(dead_code)]
pub static BIPED_OBJECT_ENUM: EnumDef = EnumDef {
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
        (43, "Ears"),
        (44, "ModMouth"),
        (45, "ModNeck"),
        (46, "ModChestPrimary"),
        (47, "ModBack"),
        (48, "ModMiscPrimary"),
        (49, "ModPelvisPrimary"),
        (50, "DecapitateHead"),
        (51, "Decapitate"),
        (52, "ModPelvisSecondary"),
        (53, "ModLegRight"),
        (54, "ModLegLeft"),
        (55, "ModFaceJewelry"),
        (56, "ModChestSecondary"),
        (57, "ModShoulder"),
        (58, "ModMiscSecondary"),
        (59, "ModMouth2"),
        (60, "ModNeck2"),
        (61, "FX01"),
    ],
};

/// Enum for book type (DATA.type).
pub static BOOK_TYPE_ENUM: EnumDef = EnumDef {
    name: "BookType",
    values: &[(0, "Book/Tome"), (255, "Note/Scroll")],
};

/// Flags for BOOK DATA.flags.
pub static BOOK_FLAGS: FlagsDef = FlagsDef {
    name: "BookFlags",
    bits: &[(0, "TeachesSkill"), (1, "CantBeTaken"), (2, "TeachesSpell")],
};

/// Flags for MGEF DATA.flags.
pub static MAGIC_EFFECT_FLAGS: FlagsDef = FlagsDef {
    name: "MagicEffectFlags",
    bits: &[
        (0, "Hostile"),
        (1, "Recover"),
        (2, "Detrimental"),
        (3, "SnapToNavMesh"),
        (4, "NoHitEvent"),
        (5, "Unknown5"),
        (6, "Unknown6"),
        (7, "Unknown7"),
        (8, "DispelWithKeywords"),
        (9, "NoDuration"),
        (10, "NoMagnitude"),
        (11, "NoArea"),
        (12, "FXPersist"),
        (13, "Unknown13"),
        (14, "GoryVisuals"),
        (15, "HideInUI"),
        (16, "Unknown16"),
        (17, "NoRecast"),
        (18, "Unknown18"),
        (19, "Unknown19"),
        (20, "Unknown20"),
        (21, "PowerAffectsMagnitude"),
        (22, "PowerAffectsDuration"),
        (23, "Unknown23"),
        (24, "Unknown24"),
        (25, "Unknown25"),
        (26, "Painless"),
        (27, "NoHitEffect"),
        (28, "NoDeathDispel"),
    ],
};

/// Enum for magic delivery type (MGEF DATA.delivery).
pub static DELIVERY_ENUM: EnumDef = EnumDef {
    name: "Delivery",
    values: &[
        (0, "Self"),
        (1, "Touch"),
        (2, "Aimed"),
        (3, "TargetActor"),
        (4, "TargetLocation"),
    ],
};

/// Enum for casting type (MGEF DATA.castingType).
pub static CASTING_TYPE_ENUM: EnumDef = EnumDef {
    name: "CastingType",
    values: &[
        (0, "ConstantEffect"),
        (1, "FireAndForget"),
        (2, "Concentration"),
        (3, "Scroll"),
    ],
};

/// Enum for spell type (SPIT.type).
pub static SPELL_TYPE_ENUM: EnumDef = EnumDef {
    name: "SpellType",
    values: &[
        (0, "Spell"),
        (1, "Disease"),
        (2, "Power"),
        (3, "LesserPower"),
        (4, "Ability"),
        (5, "Poison"),
        (10, "Addiction"),
        (11, "Voice"),
    ],
};

/// Enum for casting perk type (SPIT.castingPerk).
// NOTE: Reserved for future SPEL / ENCH schema expansion.
#[allow(dead_code)]
pub static CHARGE_TIME_ENUM: EnumDef = EnumDef {
    name: "CastingPerkType",
    values: &[(0, "None"), (1, "DualCasting")],
};

/// Flags for NPC_ ACBS.flags.
pub static NPC_FLAGS: FlagsDef = FlagsDef {
    name: "NpcFlags",
    bits: &[
        (0, "Female"),
        (1, "Essential"),
        (2, "IsCharGenFacePreset"),
        (3, "Respawn"),
        (4, "AutoCalcStats"),
        (5, "Unique"),
        (6, "DoesNotAffectStealth"),
        (7, "PcLevelMult"),
        (8, "AudioTemplate"),
        (9, "Unknown9"),
        (10, "Unknown10"),
        (11, "Protected"),
        (12, "Unknown12"),
        (13, "Unknown13"),
        (14, "Summonable"),
        (15, "Unknown15"),
        (16, "DoesNotBleed"),
        (17, "Unknown17"),
        (18, "BleedoutOverride"),
        (19, "OppositeGenderAnims"),
        (20, "SimpleActor"),
        (21, "LoopedScript"),
        (22, "Unknown22"),
        (23, "Unknown23"),
        (24, "Unknown24"),
        (25, "Unknown25"),
        (26, "Unknown26"),
        (27, "Unknown27"),
        (28, "Unknown28"),
        (29, "Unknown29"),
        (30, "Unknown30"),
        (31, "Unknown31"),
    ],
};

/// Enum for NPC ACBS level decider (plain level vs mult).
// NOTE: Reserved for future NPC_ schema expansion.
#[allow(dead_code)]
pub static NPC_LEVEL_TYPE_ENUM: EnumDef = EnumDef {
    name: "NpcLevelType",
    values: &[(0, "Fixed"), (1, "PCLevelMult")],
};

/// Flags for faction rank data.
// NOTE: Reserved for future FACT schema expansion.
#[allow(dead_code)]
pub static FACTION_FLAGS: FlagsDef = FlagsDef {
    name: "FactionFlags",
    bits: &[
        (0, "HiddenFromNPC"),
        (1, "SpecialCombat"),
        (2, "Unknown2"),
        (3, "Unknown3"),
        (4, "Unknown4"),
        (5, "Unknown5"),
        (6, "TrackCrime"),
        (7, "IgnoresMurder"),
        (8, "IgnoresAssault"),
        (9, "IgnoresStealing"),
        (10, "IgnoresTrespass"),
        (11, "DoNotReportCrimesAgainstMembers"),
        (12, "CrimeGold"),
        (13, "IgnoresPickpocket"),
        (14, "Vendor"),
        (15, "CanBeOwner"),
        (16, "IgnoresWerewolf"),
    ],
};

/// Enum for AI package type.
pub static PACKAGE_TYPE_ENUM: EnumDef = EnumDef {
    name: "PackageType",
    values: &[
        (18, "Find"),
        (19, "Follow"),
        (20, "Escort"),
        (21, "Eat"),
        (22, "Sleep"),
        (23, "Wander"),
        (24, "Travel"),
        (25, "Accompany"),
        (26, "UseItemAt"),
        (27, "Ambush"),
        (28, "FleeNotCombat"),
        (36, "Sand"),
        (37, "UseWeapon"),
    ],
};

/// Flags for LIGH DATA.flags.
pub static LIGHT_FLAGS: FlagsDef = FlagsDef {
    name: "LightFlags",
    bits: &[
        (0, "Dynamic"),
        (1, "CanBeCarried"),
        (2, "Negative"),
        (3, "Flicker"),
        (4, "Unknown4"),
        (5, "OffByDefault"),
        (6, "FlickerSlow"),
        (7, "Pulse"),
        (8, "PulseSlow"),
        (9, "SpotLight"),
        (10, "SpotShadow"),
        (11, "HemiShadow"),
        (12, "OmniShadow"),
        (13, "PortalStrictSelect"),
        (14, "ShadowsPortalOnly"),
    ],
};

/// Enum for quest type.
pub static QUEST_TYPE_ENUM: EnumDef = EnumDef {
    name: "QuestType",
    values: &[
        (0, "None"),
        (1, "MainQuest"),
        (2, "MagesGuild"),
        (3, "ThievesGuild"),
        (4, "DarkBrotherhood"),
        (5, "CompanionsQuest"),
        (6, "Miscellaneous"),
        (7, "DaedricQuest"),
        (8, "SideQuest"),
        (9, "CivilWar"),
        (10, "DLC01Vampire"),
        (11, "DLC02Dragonborn"),
    ],
};

/// Flags for quest DATA.flags.
pub static QUEST_FLAGS: FlagsDef = FlagsDef {
    name: "QuestFlags",
    bits: &[
        (0, "StartGameEnabled"),
        (1, "Unknown1"),
        (2, "WildernessEncounter"),
        (3, "AllowRepeatedStages"),
        (4, "Unknown4"),
        (5, "Unknown5"),
        (6, "Unknown6"),
        (7, "Unknown7"),
        (8, "RunOnce"),
        (9, "ExcludeFromDialogueExport"),
        (10, "WarnOnAliasFillFailure"),
    ],
};

/// Flags for CELL DATA.flags.
pub static CELL_FLAGS: FlagsDef = FlagsDef {
    name: "CellFlags",
    bits: &[
        (0, "IsInterior"),
        (1, "HasWater"),
        (2, "CantTravelFromHere"),
        (3, "NoLODWater"),
        (4, "Unknown4"),
        (5, "PublicArea"),
        (6, "HandChanged"),
        (7, "ShowSky"),
        (8, "UseSkyLighting"),
    ],
};

/// Enum for weather classification.
pub static WEATHER_CLASS_ENUM: EnumDef = EnumDef {
    name: "WeatherClass",
    values: &[
        (0, "None"),
        (1, "Pleasant"),
        (2, "Cloudy"),
        (3, "Rainy"),
        (4, "Snow"),
    ],
};

/// Flags for PROJ DATA.flags.
pub static PROJECTILE_FLAGS: FlagsDef = FlagsDef {
    name: "ProjectileFlags",
    bits: &[
        (0, "Hitscan"),
        (1, "Explosion"),
        (2, "AltTrigger"),
        (3, "MuzzleFlash"),
        (4, "Unknown4"),
        (5, "CanBeDisabled"),
        (6, "CanBePickedUp"),
        (7, "Supersonic"),
        (8, "PinsLimbs"),
        (9, "PassThroughSmallTransparent"),
        (10, "DisableCombatAimCorrection"),
    ],
};

/// Enum for projectile type.
pub static PROJECTILE_TYPE_ENUM: EnumDef = EnumDef {
    name: "ProjectileType",
    values: &[
        (1, "Missile"),
        (2, "Lobber"),
        (4, "Beam"),
        (8, "Flame"),
        (16, "Cone"),
        (32, "Barrier"),
        (64, "Arrow"),
    ],
};

/// Enum for sound level.
pub static SOUND_LEVEL_ENUM: EnumDef = EnumDef {
    name: "SoundLevel",
    values: &[(0, "Loud"), (1, "Normal"), (2, "Silent"), (3, "VeryLoud")],
};

/// Enum for equip slot (ETYP reference target).
// NOTE: Reserved for future EQUP / item schema expansion.
#[allow(dead_code)]
pub static EQUIP_TYPE_ENUM: EnumDef = EnumDef {
    name: "EquipType",
    values: &[
        (0, "None"),
        (1, "BiggerWeapons"),
        (2, "BothHands"),
        (3, "BothWeapons"),
        (4, "Caster"),
        (5, "MinorLeft"),
        (6, "MinorRight"),
        (7, "Shield"),
        (8, "Voice"),
        (9, "Either"),
    ],
};

/// Enum for impactdataset material types.
// NOTE: Reserved for future MATT / impact schema expansion.
#[allow(dead_code)]
pub static MATERIAL_TYPE_ENUM: EnumDef = EnumDef {
    name: "MaterialType",
    values: &[
        (0, "Stone"),
        (1, "Cloth"),
        (2, "Dirt"),
        (3, "Glass"),
        (4, "Grass"),
        (5, "Metal"),
        (6, "Organic"),
        (7, "Skin"),
        (8, "Water"),
        (9, "Wood"),
        (10, "HeavyStone"),
        (11, "HeavyMetal"),
        (12, "HeavyWood"),
        (13, "Chain"),
        (14, "Snow"),
        (15, "Elevator"),
        (16, "HollowMetal"),
        (17, "SheetMetal"),
        (18, "Sand"),
        (19, "BrokenConcrete"),
        (20, "VehicleBody"),
        (21, "VehiclePartSolid"),
        (22, "VehiclePartHollow"),
        (23, "BarrelSmall"),
        (24, "BottleSmall"),
        (25, "BoxLarge"),
        (26, "BoxMedium"),
        (27, "BoxSmall"),
        (28, "IceThin"),
        (29, "GlassThin"),
        (30, "FabricThick"),
        (31, "FabricThin"),
        (32, "OrganicLarge"),
        (33, "Skin"),
    ],
};
