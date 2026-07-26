// SPDX-License-Identifier: Apache-2.0
//!
//! Serializable schema package data model.

use bethkit_core::{Game, Signature};
use serde::{Deserialize, Serialize};

use crate::Expression;

/// Static xEdit conflict priority assigned to a schema node.
#[derive(
    Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum ConflictPriority {
    /// Ignore the node during conflict analysis.
    Ignore,
    /// Treat a newly added value as benign.
    BenignIfAdded,
    /// Treat differences as benign.
    Benign,
    /// Treat differences as an override without a conflict.
    Override,
    /// Translate localized content while comparing the node.
    Translate,
    /// Apply normal conflict semantics.
    #[default]
    Normal,
    /// Apply normal semantics while ignoring an empty value.
    NormalIgnoreEmpty,
    /// Treat differences as critical conflicts.
    Critical,
    /// Compare the node using FormID semantics.
    FormId,
}

/// Stable identifier for a schema node inside one package.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SchemaNodeId(pub u32);

/// A four-byte record or subrecord signature stored in a schema package.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SchemaSignature(pub [u8; 4]);

impl From<Signature> for SchemaSignature {
    fn from(value: Signature) -> Self {
        Self(value.0)
    }
}

impl From<SchemaSignature> for Signature {
    fn from(value: SchemaSignature) -> Self {
        Self(value.0)
    }
}

/// Game mode represented by a schema package.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SchemaGame {
    /// Skyrim Legendary Edition.
    SkyrimLe,
    /// Skyrim Special Edition.
    SkyrimSe,
    /// Skyrim VR.
    SkyrimVr,
    /// Fallout 3.
    #[serde(rename = "fallout_3", alias = "fallout3")]
    Fallout3,
    /// Fallout: New Vegas.
    FalloutNv,
    /// Fallout 4.
    #[serde(rename = "fallout_4", alias = "fallout4")]
    Fallout4,
    /// Fallout 4 VR.
    #[serde(rename = "fallout_4_vr", alias = "fallout4_vr")]
    Fallout4Vr,
    /// Fallout 76.
    #[serde(rename = "fallout_76", alias = "fallout76")]
    Fallout76,
    /// Oblivion.
    Oblivion,
    /// Morrowind.
    Morrowind,
    /// Starfield.
    Starfield,
}

impl SchemaGame {
    /// Returns all game modes supported by the current Bethkit API.
    pub const fn all() -> [Self; 11] {
        [
            Self::SkyrimLe,
            Self::SkyrimSe,
            Self::SkyrimVr,
            Self::Fallout3,
            Self::FalloutNv,
            Self::Fallout4,
            Self::Fallout4Vr,
            Self::Fallout76,
            Self::Oblivion,
            Self::Morrowind,
            Self::Starfield,
        ]
    }

    /// Returns the stable lower-case package slug.
    pub const fn slug(self) -> &'static str {
        match self {
            Self::SkyrimLe => "skyrim_le",
            Self::SkyrimSe => "skyrim_se",
            Self::SkyrimVr => "skyrim_vr",
            Self::Fallout3 => "fallout_3",
            Self::FalloutNv => "fallout_nv",
            Self::Fallout4 => "fallout_4",
            Self::Fallout4Vr => "fallout_4_vr",
            Self::Fallout76 => "fallout_76",
            Self::Oblivion => "oblivion",
            Self::Morrowind => "morrowind",
            Self::Starfield => "starfield",
        }
    }
}

impl From<Game> for SchemaGame {
    fn from(value: Game) -> Self {
        match value {
            Game::SkyrimLE => Self::SkyrimLe,
            Game::SkyrimSE => Self::SkyrimSe,
            Game::SkyrimVR => Self::SkyrimVr,
            Game::Fallout3 => Self::Fallout3,
            Game::FalloutNV => Self::FalloutNv,
            Game::Fallout4 => Self::Fallout4,
            Game::Fallout4VR => Self::Fallout4Vr,
            Game::Fallout76 => Self::Fallout76,
            Game::Oblivion => Self::Oblivion,
            Game::Morrowind => Self::Morrowind,
            Game::Starfield => Self::Starfield,
        }
    }
}

/// Validation state recorded by the schema generation pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationStatus {
    /// Package was generated but has not passed corpus validation.
    Candidate,
    /// Package passed structural and corpus validation.
    Validated,
    /// Package passed all release gates and may be embedded.
    Approved,
    /// Package must not be used for new builds.
    Deprecated,
    /// Package failed validation.
    Rejected,
}

/// Provenance and compatibility metadata for one schema package.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SchemaManifest {
    /// Schema format version encoded in this package.
    pub format_version: u16,
    /// Game mode represented by this package.
    pub game: SchemaGame,
    /// Stable package version.
    pub package_version: String,
    /// Upstream source repository.
    pub source_repository: String,
    /// Pinned upstream release tag.
    pub source_tag: String,
    /// Full upstream commit hash.
    pub source_commit: String,
    /// SHA-256 of the pinned upstream source archive.
    pub source_archive_sha256: String,
    /// Version of the xEdit graph exporter.
    pub exporter_version: String,
    /// SHA-256 of the exporter executable.
    pub exporter_binary_sha256: String,
    /// SHA-256 of the detailed Delphi MAP paired with the exporter.
    pub exporter_map_sha256: String,
    /// SHA-256 of the patch set applied to xEdit.
    pub exporter_patch_sha256: String,
    /// Hash identifying the external Delphi build environment.
    pub exporter_build_sha256: String,
    /// SHA-256 of path-based conversion rules.
    pub conversion_rules_sha256: String,
    /// Minimum compatible Bethkit version.
    pub minimum_bethkit_version: String,
    /// Minimum compatible C ABI version.
    pub minimum_abi_version: u32,
    /// Release validation state.
    pub validation_status: ValidationStatus,
    /// SHA-256 of the external validation corpus.
    pub corpus_sha256: String,
    /// Number of records validated by the release pipeline.
    pub validated_records: u64,
    /// Byte coverage measured by the release pipeline.
    pub byte_coverage: f64,
    /// Total number of callbacks reported by the exporter.
    pub callbacks_total: u64,
    /// Number of callbacks classified by conversion rules.
    pub callbacks_classified: u64,
    /// Custom decoders required by this package.
    pub required_decoders: Vec<DecoderRequirement>,
    /// Semantic callback handlers required by this package.
    pub required_handlers: Vec<HandlerRequirement>,
}

/// A custom semantic decoder required by a package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecoderRequirement {
    /// Stable decoder identifier.
    pub id: String,
    /// Minimum compatible decoder implementation version.
    pub minimum_version: u32,
}

/// A semantic callback handler required by a package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandlerRequirement {
    /// Stable handler identifier.
    pub id: String,
    /// Minimum compatible handler implementation version.
    pub minimum_version: u32,
}

/// Classification assigned to xEdit callbacks during conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallbackClass {
    /// Callback was translated into an expression.
    Declarative,
    /// Callback maps directly to a built-in runtime operation.
    BuiltIn,
    /// Callback is implemented by a registered semantic handler.
    #[serde(rename = "custom_handler", alias = "custom")]
    CustomHandler,
    /// Callback affects xEdit presentation only.
    UserInterfaceOnly,
}

/// Executable representation of one classified xEdit callback.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CallbackImplementation {
    /// Callback behavior represented by a bounded expression.
    Declarative {
        /// Expression evaluated by the semantic runtime.
        expression: Expression,
    },
    /// Callback behavior implemented by a stable built-in operation.
    BuiltIn {
        /// Structured built-in operation.
        operation: BuiltInOperation,
    },
    /// Callback is satisfied by the payload decoder attached to the schema node.
    PayloadDecoder {
        /// Stable custom payload decoder identifier.
        decoder: String,
        /// Minimum compatible payload decoder version.
        minimum_decoder_version: u32,
    },
    /// Callback behavior implemented by a registered semantic handler.
    CustomHandler {
        /// Stable semantic handler identifier.
        handler: String,
        /// Minimum compatible handler version.
        minimum_handler_version: u32,
    },
    /// Callback affects presentation only and is not executed at runtime.
    UserInterfaceOnly,
}

/// Versioned built-in semantic operation with deterministic configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuiltInOperation {
    /// Stable operation identifier resolved through the semantic handler registry.
    pub id: String,
    /// Minimum compatible handler implementation version.
    pub minimum_version: u32,
    /// Handler-specific deterministic configuration.
    pub configuration: serde_json::Value,
}

/// Exact schema-path binding for one classified xEdit callback.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallbackBinding {
    /// Exact stable schema path.
    pub path: String,
    /// Stable exporter callback role.
    pub callback_id: String,
    /// Optional callback slot for arrays of callbacks attached to one definition.
    pub callback_slot: Option<u32>,
    /// Build-bound implementation fingerprint used for audit traceability.
    pub implementation_fingerprint: String,
    /// Executable callback representation.
    pub implementation: CallbackImplementation,
}

/// Byte order used by a primitive numeric field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ByteOrder {
    /// Least-significant byte first.
    LittleEndian,
    /// Most-significant byte first.
    BigEndian,
}

/// Integer representation used by a primitive field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntegerType {
    /// Integer width in bytes.
    pub width: u8,
    /// Whether the integer is signed.
    pub signed: bool,
    /// Byte order used by the integer.
    pub byte_order: ByteOrder,
}

/// String encoding and termination behavior.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StringType {
    /// Encoding identifier such as `utf8`, `windows_1252`, or `localized`.
    pub encoding: String,
    /// Whether localized plugins replace the text bytes with a four-byte string-table ID.
    #[serde(default)]
    pub localized: bool,
    /// Whether a zero byte terminates the value.
    pub zero_terminated: bool,
    /// Fixed byte length when the string is not variable-sized.
    pub fixed_length: Option<u32>,
    /// Optional unsigned little-endian byte-length prefix.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length_prefix: Option<StringLengthPrefix>,
    /// Optional structural terminator byte following the complete string representation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trailing_terminator: Option<u8>,
}

/// Length prefix stored before a variable-sized string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StringLengthPrefix {
    /// Width of the unsigned little-endian length value in bytes.
    pub width: u8,
    /// Byte offset from the start of the field to the first string byte.
    pub offset: u8,
}

/// Primitive field type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PrimitiveType {
    /// Integer value.
    Integer {
        /// Integer representation.
        integer: IntegerType,
    },
    /// IEEE floating-point value.
    Float {
        /// Float width in bytes.
        width: u8,
        /// Byte order used by the float.
        byte_order: ByteOrder,
        /// Multiplier applied after raw-value normalization.
        #[serde(default = "default_float_scale")]
        scale: f64,
        /// Decimal places retained by xEdit, or [`i32::MIN`] when rounding is disabled.
        #[serde(default = "default_float_digits")]
        digits: i32,
    },
    /// String value.
    String {
        /// String representation.
        string: StringType,
    },
    /// Raw bytes with an optional fixed length.
    Bytes {
        /// Fixed byte length or `None` for the remaining payload.
        length: Option<u32>,
    },
    /// FormID with optional target record signatures.
    FormId {
        /// Valid target record signatures.
        targets: Vec<SchemaSignature>,
    },
    /// Enumeration with signed integer keys.
    Enumeration {
        /// Integer representation.
        integer: IntegerType,
        /// Named values in deterministic numeric order.
        values: Vec<(i64, String)>,
    },
    /// Named bit flags.
    Flags {
        /// Integer representation.
        integer: IntegerType,
        /// Bit positions and names.
        bits: Vec<(u8, String)>,
    },
    /// Explicitly unused bytes.
    Unused {
        /// Number of unused bytes.
        length: u32,
    },
}

/// Rule used to determine an array's element count.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ArrayCount {
    /// Fixed element count.
    Fixed {
        /// Number of elements.
        count: u32,
    },
    /// Element count is stored immediately before the array elements.
    Prefixed {
        /// Integer layout used by the count prefix.
        integer: IntegerType,
    },
    /// Count is read from an expression.
    Expression {
        /// Expression producing a non-negative count.
        expression: Expression,
    },
    /// Elements consume the remaining payload.
    Remainder,
}

/// One node in the ordered schema grammar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SchemaNode {
    /// Stable node identifier.
    pub id: SchemaNodeId,
    /// Stable path within the package.
    pub path: String,
    /// Human-readable name.
    pub name: String,
    /// Whether the node is required.
    pub required: bool,
    /// Static conflict priority before any semantic callback override.
    #[serde(default)]
    pub conflict_priority: ConflictPriority,
    /// Optional inclusion condition.
    pub condition: Option<Expression>,
    /// Node behavior.
    #[serde(flatten)]
    pub kind: SchemaNodeKind,
}

const fn default_float_scale() -> f64 {
    1.0
}

const fn default_float_digits() -> i32 {
    i32::MIN
}

/// Supported node kinds in the schema grammar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SchemaNodeKind {
    /// Ordered child nodes.
    Sequence {
        /// Child nodes in declaration order.
        children: Vec<SchemaNode>,
    },
    /// Exactly one matching child alternative.
    Choice {
        /// Alternative child nodes.
        alternatives: Vec<SchemaNode>,
    },
    /// Repeated child node or group.
    Repeat {
        /// Minimum repetition count.
        minimum: u32,
        /// Optional maximum repetition count.
        maximum: Option<u32>,
        /// Repeated node.
        child: Box<SchemaNode>,
    },
    /// Subrecord with a typed payload node.
    Subrecord {
        /// Subrecord signature.
        signature: SchemaSignature,
        /// Payload definition.
        payload: Box<SchemaNode>,
    },
    /// Primitive value.
    Primitive {
        /// Primitive representation.
        primitive: PrimitiveType,
    },
    /// Packed struct.
    Struct {
        /// Struct fields in byte order.
        fields: Vec<SchemaNode>,
    },
    /// Homogeneous array.
    Array {
        /// Element definition.
        element: Box<SchemaNode>,
        /// Element count rule.
        count: ArrayCount,
    },
    /// Union selected by a declarative expression.
    Union {
        /// Expression producing the zero-based variant index.
        selector: Expression,
        /// Union variants.
        variants: Vec<SchemaNode>,
    },
    /// Custom Rust decoder and encoder.
    Custom {
        /// Stable decoder identifier.
        decoder: String,
        /// Decoder-specific configuration serialized as JSON.
        configuration: serde_json::Value,
    },
    /// Compressed child payload.
    Compressed {
        /// Compression algorithm identifier.
        algorithm: String,
        /// Decompressed child definition.
        child: Box<SchemaNode>,
    },
    /// Reference to another node by stable path.
    Reference {
        /// Target path.
        target: String,
    },
}

/// Schema for one main-record signature.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SchemaRecord {
    /// Main-record signature.
    pub signature: SchemaSignature,
    /// Human-readable record name.
    pub name: String,
    /// Ordered grammar root.
    pub root: SchemaNode,
}
