// SPDX-License-Identifier: Apache-2.0
//!
//! Declarative record schema for Bethesda plugin files.
//!
//! A [`SchemaRegistry`] maps record signatures to [`RecordSchema`] values.
//! Each schema describes the subrecords a record type can contain, including
//! field names, primitive types, nested structs, arrays, enums, and flags.
//!
//! All schema data is `&'static` — no heap allocation is needed for the
//! definitions themselves. [`SchemaRegistry::sse`] and [`SchemaRegistry::fo4`]
//! return global singletons built on first use.
//!
//! Schemas for FO3, FONV, Oblivion, Morrowind, and Starfield are archived in
//! `src/schema/_archive/` and not yet compiled.
//!
//! # Example
//!
//! ```rust,no_run
//! use bethkit_core::{GameContext, Plugin, RecordView, SchemaRegistry, Signature};
//!
//! let ctx = GameContext::sse();
//! let plugin = Plugin::open("Skyrim.esm".as_ref(), ctx)?;
//! let registry = SchemaRegistry::sse();
//!
//! let plugin_localized = plugin.is_localized();
//! for group in plugin.groups() {
//!     for record in group.records_recursive() {
//!         if let Some(schema) = registry.get(record.header.signature) {
//!             let view = RecordView::new(record, schema, plugin_localized);
//!             if let Some(field) = view.get_field("Full Name")? {
//!                 println!("{}: {:?}", field.name, field.value);
//!             }
//!         }
//!     }
//! }
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use ahash::HashMap;

use crate::types::Signature;

pub mod enums;
mod fo4;
pub(crate) mod shared;
mod sse;
pub mod view;

pub use view::{FieldEntry, FieldValue, RecordView};

/// Describes all subrecords expected by a single record type.
#[derive(Clone, Copy)]
pub struct RecordSchema {
    /// The 4-byte record type signature (e.g. `b"NPC_"`).
    pub sig: Signature,
    /// Human-readable name for the record type (e.g. `"Non-Player Character"`).
    pub name: &'static str,
    /// Ordered list of subrecord member definitions.
    pub members: &'static [SubRecordDef],
}

/// Defines a single subrecord member within a [`RecordSchema`].
#[derive(Clone, Copy)]
pub struct SubRecordDef {
    /// The 4-byte subrecord signature (e.g. `b"EDID"`).
    pub sig: Signature,
    /// Human-readable field name.
    pub name: &'static str,
    /// Whether the subrecord is required for a valid record.
    pub required: bool,
    /// Whether this subrecord may appear more than once.
    pub repeating: bool,
    /// The data type / structure of this subrecord's payload.
    pub field: FieldType,
}

/// Defines a single named field within a struct or array.
#[derive(Clone, Copy)]
pub struct FieldDef {
    /// Human-readable name for this field.
    pub name: &'static str,
    /// The data type of this field.
    pub kind: FieldType,
}

/// Defines how many elements an [`FieldType::Array`] contains.
#[derive(Clone, Copy)]
pub enum ArrayCount {
    /// A fixed element count.
    Fixed(u32),
    /// Consume all remaining bytes (element count determined by data size).
    Remainder,
    /// The element count was given by a preceding sibling subrecord of the
    /// named signature (e.g. `KSIZ` precedes `KWDA`).
    PrecedingSibling(Signature),
}

/// The data type of a subrecord payload or a struct field.
///
/// `&'static` references ensure all schema data lives in read-only memory.
#[derive(Clone, Copy)]
#[non_exhaustive]
pub enum FieldType {
    /// Unsigned 8-bit integer.
    UInt8,
    /// Unsigned 16-bit little-endian integer.
    UInt16,
    /// Unsigned 32-bit little-endian integer.
    UInt32,
    /// Unsigned 64-bit little-endian integer.
    UInt64,
    /// Signed 8-bit integer.
    Int8,
    /// Signed 16-bit little-endian integer.
    Int16,
    /// Signed 32-bit little-endian integer.
    Int32,
    /// 32-bit little-endian IEEE 754 float.
    Float32,
    /// NUL-terminated UTF-8 string (inline, not localised).
    ZString,
    /// A string that is either inline (when the record is not LOCALIZED) or a
    /// 32-bit string-table ID (when the LOCALIZED flag is set).
    LString,
    /// Raw byte array of unspecified length.
    ByteArray,
    /// A 32-bit FormID (file-relative, not yet resolved).
    FormId,
    /// A 32-bit FormID restricted to records of the given signature set.
    ///
    /// The slice lists valid target record types (e.g. `&[NPC_, LVLN]`).
    FormIdTyped(&'static [Signature]),
    /// A 32-bit integer interpreted as an enumeration.
    Enum(&'static EnumDef),
    /// A 32-bit integer interpreted as a set of named bit flags.
    Flags(&'static FlagsDef),
    /// A fixed-layout struct of named fields packed sequentially.
    Struct(&'static [FieldDef]),
    /// An array of homogeneous elements.
    Array {
        /// The element type.
        element: &'static FieldType,
        /// How to determine the element count.
        count: ArrayCount,
    },
    /// A union whose active variant is chosen by a decider function.
    ///
    /// The decider receives the raw subrecord bytes and returns the
    /// zero-based variant index.
    Union {
        /// Selects the active variant from the raw payload.
        decider: fn(&[u8]) -> usize,
        /// All possible variants; only the selected one is decoded.
        variants: &'static [FieldType],
    },
    /// `n` padding / unused bytes.
    Unused(u8),
}

/// A named integer enumeration.
#[derive(Clone, Copy)]
pub struct EnumDef {
    /// Human-readable name for this enum type.
    pub name: &'static str,
    /// Mapping of integer value to variant name.
    pub values: &'static [(i64, &'static str)],
}

impl EnumDef {
    /// Looks up the name for `value`, if it is a known variant.
    pub fn name_of(&self, value: i64) -> Option<&'static str> {
        self.values
            .iter()
            .find(|(v, _)| *v == value)
            .map(|(_, name)| *name)
    }
}

/// A named bit-flag set.
#[derive(Clone, Copy)]
pub struct FlagsDef {
    /// Human-readable name for this flags type.
    pub name: &'static str,
    /// Mapping of bit position (0-based) to flag name.
    pub bits: &'static [(u8, &'static str)],
}

impl FlagsDef {
    /// Returns an iterator over the names of all set bits in `value`.
    pub fn active_names(&self, value: u64) -> impl Iterator<Item = &'static str> + '_ {
        self.bits
            .iter()
            .filter(move |(bit, _)| (value >> *bit) & 1 != 0)
            .map(|(_, name)| *name)
    }
}

/// Maps record type signatures to their [`RecordSchema`] definitions.
///
/// Use [`SchemaRegistry::sse`] or [`SchemaRegistry::fo4`] to obtain a
/// game-specific global singleton.
pub struct SchemaRegistry {
    map: HashMap<Signature, &'static RecordSchema>,
}

impl SchemaRegistry {
    /// Creates an empty registry.
    pub(crate) fn new() -> Self {
        Self {
            map: HashMap::default(),
        }
    }

    /// Registers a record schema. Later registrations for the same signature
    /// overwrite earlier ones.
    pub(crate) fn register(&mut self, schema: &'static RecordSchema) {
        self.map.insert(schema.sig, schema);
    }

    /// Looks up the schema for a record signature.
    ///
    /// Returns `None` if the signature is not registered.
    ///
    /// # Arguments
    ///
    /// * `sig` - The 4-byte record signature to look up.
    pub fn get(&self, sig: Signature) -> Option<&'static RecordSchema> {
        self.map.get(&sig).copied()
    }

    /// Returns the number of registered record schemas.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Returns `true` if no schemas are registered.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Returns the global Skyrim SE schema registry, building it on first
    /// call.
    ///
    /// The returned reference is valid for the lifetime of the process.
    pub fn sse() -> &'static SchemaRegistry {
        sse::registry()
    }

    /// Returns the global Fallout 4 schema registry, building it on first
    /// call.
    ///
    /// The returned reference is valid for the lifetime of the process.
    pub fn fo4() -> &'static SchemaRegistry {
        fo4::registry()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static SSE_EXPECTED_SIGNATURES: &[Signature] = &[
        Signature(*b"AACT"),
        Signature(*b"ACTI"),
        Signature(*b"ADDN"),
        Signature(*b"ALCH"),
        Signature(*b"AMMO"),
        Signature(*b"ANIO"),
        Signature(*b"APPA"),
        Signature(*b"ARMA"),
        Signature(*b"ARMO"),
        Signature(*b"ARTO"),
        Signature(*b"ASPC"),
        Signature(*b"ASTP"),
        Signature(*b"AVIF"),
        Signature(*b"BOOK"),
        Signature(*b"BPTD"),
        Signature(*b"CAMS"),
        Signature(*b"CELL"),
        Signature(*b"CLAS"),
        Signature(*b"CLFM"),
        Signature(*b"CLMT"),
        Signature(*b"COBJ"),
        Signature(*b"COLL"),
        Signature(*b"CONT"),
        Signature(*b"CPTH"),
        Signature(*b"CSTY"),
        Signature(*b"DEBR"),
        Signature(*b"DIAL"),
        Signature(*b"DLBR"),
        Signature(*b"DLVW"),
        Signature(*b"DOBJ"),
        Signature(*b"DOOR"),
        Signature(*b"DUAL"),
        Signature(*b"ECZN"),
        Signature(*b"EFSH"),
        Signature(*b"ENCH"),
        Signature(*b"EQUP"),
        Signature(*b"EXPL"),
        Signature(*b"EYES"),
        Signature(*b"FACT"),
        Signature(*b"FLOR"),
        Signature(*b"FLST"),
        Signature(*b"FSTP"),
        Signature(*b"FSTS"),
        Signature(*b"FURN"),
        Signature(*b"GLOB"),
        Signature(*b"GMST"),
        Signature(*b"GRAS"),
        Signature(*b"HAZD"),
        Signature(*b"HDPT"),
        Signature(*b"IDLE"),
        Signature(*b"IDLM"),
        Signature(*b"IMAD"),
        Signature(*b"IMGS"),
        Signature(*b"INFO"),
        Signature(*b"INGR"),
        Signature(*b"IPCT"),
        Signature(*b"IPDS"),
        Signature(*b"KEYM"),
        Signature(*b"KYWD"),
        Signature(*b"LAND"),
        Signature(*b"LCRT"),
        Signature(*b"LCTN"),
        Signature(*b"LENS"),
        Signature(*b"LGTM"),
        Signature(*b"LIGH"),
        Signature(*b"LSCR"),
        Signature(*b"LTEX"),
        Signature(*b"LVLI"),
        Signature(*b"LVLN"),
        Signature(*b"LVSP"),
        Signature(*b"MATO"),
        Signature(*b"MATT"),
        Signature(*b"MESG"),
        Signature(*b"MGEF"),
        Signature(*b"MISC"),
        Signature(*b"MOVT"),
        Signature(*b"MSTT"),
        Signature(*b"MUSC"),
        Signature(*b"MUST"),
        Signature(*b"NAVI"),
        Signature(*b"NAVM"),
        Signature(*b"NPC_"),
        Signature(*b"OTFT"),
        Signature(*b"PACK"),
        Signature(*b"PARW"),
        Signature(*b"PBAR"),
        Signature(*b"PBEA"),
        Signature(*b"PCON"),
        Signature(*b"PERK"),
        Signature(*b"PFLA"),
        Signature(*b"PGRE"),
        Signature(*b"PHZD"),
        Signature(*b"PLYR"),
        Signature(*b"PMIS"),
        Signature(*b"PROJ"),
        Signature(*b"QUST"),
        Signature(*b"RACE"),
        Signature(*b"REGN"),
        Signature(*b"RELA"),
        Signature(*b"REVB"),
        Signature(*b"RFCT"),
        Signature(*b"SCEN"),
        Signature(*b"SCRL"),
        Signature(*b"SHOU"),
        Signature(*b"SLGM"),
        Signature(*b"SMBN"),
        Signature(*b"SMEN"),
        Signature(*b"SMQN"),
        Signature(*b"SNCT"),
        Signature(*b"SNDR"),
        Signature(*b"SOPM"),
        Signature(*b"SOUN"),
        Signature(*b"SPEL"),
        Signature(*b"SPGD"),
        Signature(*b"STAT"),
        Signature(*b"TACT"),
        Signature(*b"TES4"),
        Signature(*b"TREE"),
        Signature(*b"TXST"),
        Signature(*b"VOLI"),
        Signature(*b"VTYP"),
        Signature(*b"WATR"),
        Signature(*b"WEAP"),
        Signature(*b"WOOP"),
        Signature(*b"WRLD"),
        Signature(*b"WTHR"),
    ];

    fn assert_registry_signatures(registry: &SchemaRegistry, expected: &[Signature]) {
        let mut expected_sorted: Vec<Signature> = expected.to_vec();
        expected_sorted.sort_by_key(|signature: &Signature| signature.0);

        let mut expected_unique: Vec<Signature> = expected_sorted.clone();
        expected_unique.dedup();
        assert_eq!(
            expected_unique.len(),
            expected_sorted.len(),
            "expected signature list contains duplicates",
        );

        let mut actual_sorted: Vec<Signature> = registry.map.keys().copied().collect();
        actual_sorted.sort_by_key(|signature: &Signature| signature.0);

        assert_eq!(actual_sorted, expected_sorted);
    }

    /// Verifies that the SSE registry contains exactly the expected record types.
    #[test]
    fn sse_registry_has_expected_signatures() -> std::result::Result<(), Box<dyn std::error::Error>>
    {
        let registry: &SchemaRegistry = SchemaRegistry::sse();
        assert_registry_signatures(registry, SSE_EXPECTED_SIGNATURES);
        Ok(())
    }

    /// Verifies that NPC_ is registered and has basic fields.
    #[test]
    fn sse_npc_schema_is_registered() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let reg = SchemaRegistry::sse();
        let schema = reg.get(Signature(*b"NPC_")).ok_or("NPC_ not registered")?;
        assert_eq!(schema.name, "Non-Player Character");
        assert!(!schema.members.is_empty());
        Ok(())
    }

    /// Verifies that WEAP is registered and has a DATA member.
    #[test]
    fn sse_weap_has_data_member() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let reg = SchemaRegistry::sse();
        let schema = reg.get(Signature(*b"WEAP")).ok_or("WEAP not registered")?;
        let has_data = schema.members.iter().any(|m| m.sig == Signature(*b"DATA"));
        assert!(has_data, "WEAP schema should have a DATA subrecord");
        Ok(())
    }

    /// Verifies that EnumDef::name_of returns the correct variant name.
    #[test]
    fn enum_def_name_of() -> std::result::Result<(), Box<dyn std::error::Error>> {
        static TEST_ENUM: EnumDef = EnumDef {
            name: "TestEnum",
            values: &[(0, "Zero"), (1, "One"), (2, "Two")],
        };
        assert_eq!(TEST_ENUM.name_of(1), Some("One"));
        assert_eq!(TEST_ENUM.name_of(99), None);
        Ok(())
    }

    /// Verifies that FlagsDef::active_names returns names for set bits.
    #[test]
    fn flags_def_active_names() -> std::result::Result<(), Box<dyn std::error::Error>> {
        static TEST_FLAGS: FlagsDef = FlagsDef {
            name: "TestFlags",
            bits: &[(0, "FlagA"), (1, "FlagB"), (2, "FlagC")],
        };
        let active: Vec<_> = TEST_FLAGS.active_names(0b101).collect();
        assert_eq!(active, ["FlagA", "FlagC"]);
        Ok(())
    }

    static FO4_EXPECTED_SIGNATURES: &[Signature] = &[
        Signature(*b"AACT"),
        Signature(*b"ACTI"),
        Signature(*b"ADDN"),
        Signature(*b"AECH"),
        Signature(*b"ALCH"),
        Signature(*b"AMDL"),
        Signature(*b"AMMO"),
        Signature(*b"ANIO"),
        Signature(*b"AORU"),
        Signature(*b"ARMA"),
        Signature(*b"ARMO"),
        Signature(*b"ARTO"),
        Signature(*b"ASPC"),
        Signature(*b"ASTP"),
        Signature(*b"AVIF"),
        Signature(*b"BNDS"),
        Signature(*b"BOOK"),
        Signature(*b"BPTD"),
        Signature(*b"CAMS"),
        Signature(*b"CELL"),
        Signature(*b"CLAS"),
        Signature(*b"CLFM"),
        Signature(*b"CLMT"),
        Signature(*b"CMPO"),
        Signature(*b"COBJ"),
        Signature(*b"COLL"),
        Signature(*b"CONT"),
        Signature(*b"CPTH"),
        Signature(*b"CSTY"),
        Signature(*b"DEBR"),
        Signature(*b"DFOB"),
        Signature(*b"DIAL"),
        Signature(*b"DLBR"),
        Signature(*b"DLVW"),
        Signature(*b"DMGT"),
        Signature(*b"DOBJ"),
        Signature(*b"DOOR"),
        Signature(*b"DUAL"),
        Signature(*b"ECZN"),
        Signature(*b"EFSH"),
        Signature(*b"ENCH"),
        Signature(*b"EQUP"),
        Signature(*b"EXPL"),
        Signature(*b"EYES"),
        Signature(*b"FACT"),
        Signature(*b"FLOR"),
        Signature(*b"FLST"),
        Signature(*b"FSTP"),
        Signature(*b"FSTS"),
        Signature(*b"FURN"),
        Signature(*b"GDRY"),
        Signature(*b"GLOB"),
        Signature(*b"GMST"),
        Signature(*b"GRAS"),
        Signature(*b"HAZD"),
        Signature(*b"HDPT"),
        Signature(*b"IDLE"),
        Signature(*b"IDLM"),
        Signature(*b"IMAD"),
        Signature(*b"IMGS"),
        Signature(*b"INFO"),
        Signature(*b"INGR"),
        Signature(*b"INNR"),
        Signature(*b"IPCT"),
        Signature(*b"IPDS"),
        Signature(*b"KEYM"),
        Signature(*b"KSSM"),
        Signature(*b"KYWD"),
        Signature(*b"LAND"),
        Signature(*b"LAYR"),
        Signature(*b"LCRT"),
        Signature(*b"LCTN"),
        Signature(*b"LENS"),
        Signature(*b"LGTM"),
        Signature(*b"LIGH"),
        Signature(*b"LSCR"),
        Signature(*b"LTEX"),
        Signature(*b"LVLI"),
        Signature(*b"LVLN"),
        Signature(*b"LVSP"),
        Signature(*b"MATO"),
        Signature(*b"MATT"),
        Signature(*b"MESG"),
        Signature(*b"MGEF"),
        Signature(*b"MISC"),
        Signature(*b"MOVT"),
        Signature(*b"MSTT"),
        Signature(*b"MSWP"),
        Signature(*b"MUSC"),
        Signature(*b"MUST"),
        Signature(*b"NAVI"),
        Signature(*b"NAVM"),
        Signature(*b"NOCM"),
        Signature(*b"NOTE"),
        Signature(*b"NPC_"),
        Signature(*b"OMOD"),
        Signature(*b"OTFT"),
        Signature(*b"OVIS"),
        Signature(*b"PACK"),
        Signature(*b"PERK"),
        Signature(*b"PKIN"),
        Signature(*b"PLYR"),
        Signature(*b"PROJ"),
        Signature(*b"QUST"),
        Signature(*b"RACE"),
        Signature(*b"REGN"),
        Signature(*b"RELA"),
        Signature(*b"REVB"),
        Signature(*b"RFCT"),
        Signature(*b"RFGP"),
        Signature(*b"SCCO"),
        Signature(*b"SCEN"),
        Signature(*b"SCOL"),
        Signature(*b"SCSN"),
        Signature(*b"SMBN"),
        Signature(*b"SMEN"),
        Signature(*b"SMQN"),
        Signature(*b"SNCT"),
        Signature(*b"SNDR"),
        Signature(*b"SOPM"),
        Signature(*b"SOUN"),
        Signature(*b"SPEL"),
        Signature(*b"SPGD"),
        Signature(*b"STAG"),
        Signature(*b"STAT"),
        Signature(*b"TACT"),
        Signature(*b"TERM"),
        Signature(*b"TES4"),
        Signature(*b"TREE"),
        Signature(*b"TRNS"),
        Signature(*b"TXST"),
        Signature(*b"VTYP"),
        Signature(*b"WATR"),
        Signature(*b"WEAP"),
        Signature(*b"WRLD"),
        Signature(*b"WTHR"),
        Signature(*b"ZOOM"),
    ];

    /// Verifies that the FO4 registry contains exactly the expected record types.
    #[test]
    fn fo4_registry_has_expected_signatures(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let registry: &SchemaRegistry = SchemaRegistry::fo4();
        assert_registry_signatures(registry, FO4_EXPECTED_SIGNATURES);
        Ok(())
    }
}
