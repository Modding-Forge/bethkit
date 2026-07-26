// SPDX-License-Identifier: Apache-2.0
//!
//! Versioned schema packages for Bethesda plugin semantics.
//!
//! This crate owns the language-neutral schema model, deterministic CBOR
//! package format, bounded expression evaluator, package registry, and
//! optional release-time embedded catalog. It never downloads schemas or
//! invokes xEdit at runtime.

mod error;
mod expression;
mod model;
mod package;
mod registry;

pub use error::{Result, SchemaError};
pub use expression::{EvalContext, EvalValue, Expression};
pub use model::{
    ArrayCount, BuiltInOperation, ByteOrder, CallbackBinding, CallbackClass,
    CallbackImplementation, ConflictPriority, DecoderRequirement, HandlerRequirement, IntegerType,
    PrimitiveType, SchemaGame, SchemaManifest, SchemaNode, SchemaNodeId, SchemaNodeKind,
    SchemaRecord, SchemaSignature, StringLengthPrefix, StringType, ValidationStatus,
};
pub use package::{
    encode_bundle, SchemaLoadLimits, SchemaPackage, BUNDLE_MAGIC, PACKAGE_FORMAT_VERSION,
    PACKAGE_MAGIC,
};
pub use registry::{SchemaCatalog, SchemaRegistry};
