// SPDX-License-Identifier: Apache-2.0
//!
//! Versioned semantic callback handlers and built-in xEdit operations.

use std::collections::BTreeMap;
use std::sync::Arc;

use bethkit_core::{FormId, Record, RecordFlags, Signature, WritableRecord};
use bethkit_schema::{CallbackBinding, CallbackImplementation, ConflictPriority, SchemaGame};

use crate::{FieldValue, OwnedFieldValue, Result, SemanticError};

pub(crate) fn is_validation_binding(binding: &CallbackBinding) -> bool {
    matches!(
        &binding.implementation,
        CallbackImplementation::BuiltIn { operation }
            if operation
                .configuration
                .get("phase")
                .and_then(serde_json::Value::as_str)
                == Some("validation")
    )
}

/// Runtime phase in which a semantic callback is invoked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandlerPhase {
    /// Typed value decoding and normalization.
    DecodeNormalize,
    /// Normal xEdit value presentation.
    Display,
    /// Compact xEdit summary presentation.
    Summary,
    /// Stable xEdit sort-key presentation.
    SortKey,
    /// Editable xEdit text presentation.
    EditValue,
    /// Native-value text presentation.
    NativeValue,
    /// Conversion from edited text back to a typed value.
    ParseEditValue,
    /// Dynamic selection of a schema union variant.
    UnionSelection,
    /// Dynamic selection of a schema array's element count.
    ArrayCount,
    /// Validation equivalent to xEdit's `ctCheck`.
    Validation,
    /// Transactional callback after a value is changed.
    AfterSet,
    /// Dynamic reference or link resolution.
    ReferenceResolution,
    /// Dynamic conflict-priority evaluation.
    Conflict,
    /// Record identity and indexing metadata.
    RecordMetadata,
    /// Dynamic field-removability evaluation.
    Removability,
}

/// Public xEdit-compatible presentation mode for a typed value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueFormat {
    /// Normal tree value text.
    Display,
    /// Compact summary text.
    Summary,
    /// Deterministic sort key.
    SortKey,
    /// Text accepted by an editor control.
    EditValue,
    /// Native-value text.
    NativeValue,
}

impl From<ValueFormat> for HandlerPhase {
    fn from(value: ValueFormat) -> Self {
        match value {
            ValueFormat::Display => Self::Display,
            ValueFormat::Summary => Self::Summary,
            ValueFormat::SortKey => Self::SortKey,
            ValueFormat::EditValue => Self::EditValue,
            ValueFormat::NativeValue => Self::NativeValue,
        }
    }
}

/// Main-record metadata shared by callback invocations.
#[derive(Debug, Clone, Copy)]
pub struct HandlerRecordContext {
    /// Main-record signature.
    pub record_signature: Signature,
    /// File-local main-record FormID.
    pub form_id: FormId,
    /// Main-record form version.
    pub form_version: u16,
    /// Game mode selected by the schema package.
    pub game: SchemaGame,
}

/// Grid coordinates returned by an xEdit record-metadata callback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordGridCell {
    /// Signed exterior-cell X coordinate.
    pub x: i32,
    /// Signed exterior-cell Y coordinate.
    pub y: i32,
}

/// One named xEdit record-index entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordIndexKey {
    /// Stable Bethkit name for the xEdit index.
    pub index: String,
    /// Canonical string representation of the indexed value.
    pub key: String,
}

impl HandlerRecordContext {
    /// Creates callback record metadata.
    pub const fn new(
        record_signature: Signature,
        form_id: FormId,
        form_version: u16,
        game: SchemaGame,
    ) -> Self {
        Self {
            record_signature,
            form_id,
            form_version,
            game,
        }
    }
}

/// Context supplied to one semantic callback invocation.
pub struct HandlerContext<'a> {
    /// Exact callback binding selected by the schema package.
    pub binding: &'a CallbackBinding,
    /// Main-record signature.
    pub record_signature: Signature,
    /// File-local main-record FormID.
    pub form_id: FormId,
    /// Main-record form version.
    pub form_version: u16,
    /// Game mode selected by the schema package.
    pub game: SchemaGame,
    /// Deterministic operation configuration from the schema package.
    pub configuration: &'a serde_json::Value,
}

/// One transactional edit requested by a semantic handler.
#[derive(Debug, Clone, PartialEq)]
pub enum HandlerMutation {
    /// Replace an existing field occurrence.
    Set {
        /// Stable schema path.
        path: String,
        /// Zero-based occurrence.
        occurrence: usize,
        /// Replacement value.
        value: OwnedFieldValue,
    },
    /// Replace a field occurrence only when it still has an expected value.
    SetIfEqual {
        /// Stable schema path.
        path: String,
        /// Zero-based occurrence.
        occurrence: usize,
        /// Expected current value.
        expected: OwnedFieldValue,
        /// Replacement value.
        value: OwnedFieldValue,
    },
    /// Insert a new field.
    Insert {
        /// Stable schema path.
        path: String,
        /// Inserted value.
        value: OwnedFieldValue,
    },
    /// Remove an existing field occurrence.
    Remove {
        /// Stable schema path.
        path: String,
        /// Zero-based occurrence.
        occurrence: usize,
    },
    /// Remove every occurrence of one subrecord field.
    RemoveAll {
        /// Stable schema path.
        path: String,
    },
    /// Make an integer counter match a decoded collection length.
    SynchronizeCount {
        /// Stable path of the counter subrecord.
        path: String,
        /// Zero-based counter occurrence.
        occurrence: usize,
        /// Collection length written to the counter.
        value: u64,
        /// Remove an existing optional counter when the length is zero.
        remove_when_zero: bool,
    },
    /// Make an optional field's presence match a semantic condition.
    SynchronizePresence {
        /// Stable path of the optional subrecord.
        path: String,
        /// Zero-based field occurrence.
        occurrence: usize,
        /// Whether the field must exist after the transaction.
        present: bool,
        /// Value used when the field must be inserted.
        value: OwnedFieldValue,
    },
}

/// Typed result returned by a semantic callback handler.
#[derive(Debug)]
pub enum HandlerOutput {
    /// The callback has no externally visible return value.
    None,
    /// Transformed semantic value.
    Value(FieldValue<'static>),
    /// Boolean decision such as visibility, sorting, or inclusion.
    Boolean(bool),
    /// Integer result such as a union selection.
    Integer(i64),
    /// xEdit conflict priority selected for one schema node.
    ConflictPriority(ConflictPriority),
    /// Text result such as an editor identifier.
    Text(String),
    /// File-local FormID result.
    FormId(FormId),
    /// Exterior-cell grid coordinates.
    GridCell(RecordGridCell),
    /// Record index keys.
    IndexKeys(Vec<RecordIndexKey>),
    /// Transactional record edits.
    Mutations(Vec<HandlerMutation>),
}

/// Input supplied to a semantic callback handler.
pub struct HandlerInvocation<'a> {
    /// Record and binding metadata.
    pub context: HandlerContext<'a>,
    /// Runtime phase selecting the xEdit callback behavior.
    pub phase: HandlerPhase,
    /// Optional decoded value for value-oriented callback roles.
    pub value: Option<&'a FieldValue<'static>>,
    /// Previous decoded value for stateful editor callbacks.
    pub old_value: Option<&'a FieldValue<'static>>,
    /// Original main record for callbacks that inspect sibling subrecords.
    pub source_record: Option<&'a Record>,
    /// Transactional writable record for record-level editor callbacks.
    pub source_writable_record: Option<&'a WritableRecord>,
}

/// Versioned implementation of one stable semantic handler.
pub trait SemanticHandler: Send + Sync {
    /// Stable identifier referenced by schema packages.
    fn id(&self) -> &'static str;

    /// Handler implementation version.
    fn version(&self) -> u32;

    /// Executes the callback without mutating the source record.
    ///
    /// Stateful handlers return [`HandlerOutput::Mutations`], which the
    /// editor applies transactionally after the invocation succeeds.
    ///
    /// # Errors
    ///
    /// Returns [`SemanticError`] when the input or operation is invalid.
    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput>;
}

/// Resolves xEdit archive resource hashes to their canonical path text.
///
/// Implementations may consult loaded BA2/BSA indexes or another immutable cache.
/// Calls can occur concurrently from multiple semantic contexts.
pub trait ResourceHashResolver: Send + Sync {
    /// Resolves a file hash.
    fn resolve_file_hash(&self, hash: u64) -> Option<String>;

    /// Resolves a folder hash.
    fn resolve_folder_hash(&self, hash: u64) -> Option<String>;
}

/// Metadata associated with one Wwise object GUID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WwiseGuidInfo {
    name: String,
    object_path: String,
}

impl WwiseGuidInfo {
    /// Creates Wwise object metadata.
    pub fn new(name: impl Into<String>, object_path: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            object_path: object_path.into(),
        }
    }

    /// Returns the Wwise object name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the Wwise object path.
    pub fn object_path(&self) -> &str {
        &self.object_path
    }
}

/// Resolves Wwise object metadata from a binary GUID.
pub trait WwiseGuidResolver: Send + Sync {
    /// Resolves metadata for one GUID.
    fn resolve_wwise_guid(&self, guid: [u8; 16]) -> Option<WwiseGuidInfo>;
}

/// Versioned semantic-handler registry.
#[derive(Clone, Default)]
pub struct SemanticHandlerRegistry {
    handlers: BTreeMap<String, Arc<dyn SemanticHandler>>,
}

#[derive(Clone, Copy)]
enum HandlerRecordSource<'a> {
    None,
    ReadOnly(&'a Record),
    Writable(&'a WritableRecord),
}

impl SemanticHandlerRegistry {
    /// Creates an empty handler registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a registry containing differential-tested built-in handlers.
    pub fn builtin() -> Self {
        let mut registry = Self::new();
        registry.register(Arc::new(NormalizeRadians));
        registry.register(Arc::new(ModelInfoConflictPriority));
        registry.register(Arc::new(IgnoreEmptyConflictPriority));
        registry.register(Arc::new(CellWaterConflictPriority));
        registry.register(Arc::new(ConstantMetadataBoolean));
        registry.register(Arc::new(MorrowindGridCell));
        registry.register(Arc::new(MorrowindGridFormId));
        registry.register(Arc::new(MorrowindReferenceFormId));
        registry.register(Arc::new(NullRecordFormId));
        registry.register(Arc::new(MorrowindGridIdentity));
        registry.register(Arc::new(MorrowindScriptEditorId));
        registry.register(Arc::new(IntegerRecordIndexKey));
        registry.register(Arc::new(StarfieldAvmdIndexKey));
        registry.register(Arc::new(FormatRgb));
        registry.register(Arc::new(RemovableWhenZero));
        registry.register(Arc::new(ResourceHashFormatter { resolver: None }));
        registry.register(Arc::new(ModelInfoCounts));
        registry.register(Arc::new(ModelInfoArrayCount));
        registry.register(Arc::new(CtdaRunOnAfterSet));
        registry.register(Arc::new(CtdaTypeAfterSet));
        registry.register(Arc::new(MessageDisplayTimeAfterSet));
        registry.register(Arc::new(FormListEditorIdAfterSet));
        registry.register(Arc::new(MagicEffectSecondAvWeightAfterSet));
        registry.register(Arc::new(RefreshSiblingUnions));
        registry.register(Arc::new(InvalidateConflicts));
        registry.register(Arc::new(CtdaTypeFormatter));
        registry.register(Arc::new(IntegerLookupFormatter));
        registry.register(Arc::new(SynchronizeCountAfterSet));
        registry.register(Arc::new(SynchronizeRecordCountsAfterSet));
        registry.register(Arc::new(InvalidModelInfoValidation));
        registry.register(Arc::new(WwiseGuidFormatter { resolver: None }));
        registry
    }

    /// Registers or replaces a semantic handler.
    pub fn register(&mut self, handler: Arc<dyn SemanticHandler>) {
        self.handlers.insert(handler.id().to_owned(), handler);
    }

    /// Installs the archive-backed resolver used by xEdit resource-hash formatters.
    ///
    /// This replaces the built-in formatter while preserving its stable handler ID.
    pub fn set_resource_hash_resolver(&mut self, resolver: Arc<dyn ResourceHashResolver>) {
        self.register(Arc::new(ResourceHashFormatter {
            resolver: Some(resolver),
        }));
    }

    /// Installs the metadata resolver used by Starfield Wwise GUID callbacks.
    pub fn set_wwise_guid_resolver(&mut self, resolver: Arc<dyn WwiseGuidResolver>) {
        self.register(Arc::new(WwiseGuidFormatter {
            resolver: Some(resolver),
        }));
    }

    /// Resolves a handler satisfying a minimum implementation version.
    ///
    /// # Errors
    ///
    /// Returns [`SemanticError::MissingHandler`] when the handler is absent
    /// or older than the required version.
    pub fn require(&self, id: &str, minimum_version: u32) -> Result<&dyn SemanticHandler> {
        let handler = self
            .handlers
            .get(id)
            .ok_or_else(|| SemanticError::MissingHandler(id.to_owned()))?;
        if handler.version() < minimum_version {
            return Err(SemanticError::MissingHandler(format!(
                "{id} version {minimum_version} or newer"
            )));
        }
        Ok(handler.as_ref())
    }

    /// Executes one built-in or custom-handler binding.
    ///
    /// # Errors
    ///
    /// Returns [`SemanticError`] when the binding is not executable through
    /// this registry or its registered handler rejects the invocation.
    pub fn invoke(
        &self,
        binding: &CallbackBinding,
        record: HandlerRecordContext,
        phase: HandlerPhase,
        value: Option<&FieldValue<'static>>,
        old_value: Option<&FieldValue<'static>>,
    ) -> Result<HandlerOutput> {
        self.invoke_with_records(
            binding,
            record,
            HandlerRecordSource::None,
            phase,
            value,
            old_value,
        )
    }

    /// Executes a binding with access to the original main record.
    ///
    /// Record-dependent callbacks must use this entry point. The regular
    /// [`Self::invoke`] method deliberately supplies no source record so a
    /// handler cannot silently depend on data its caller did not provide.
    ///
    /// # Errors
    ///
    /// Returns [`SemanticError`] when the binding is not executable or the
    /// selected handler rejects the invocation.
    pub fn invoke_with_source_record<'a>(
        &self,
        binding: &'a CallbackBinding,
        record: HandlerRecordContext,
        source_record: Option<&'a Record>,
        phase: HandlerPhase,
        value: Option<&'a FieldValue<'static>>,
        old_value: Option<&'a FieldValue<'static>>,
    ) -> Result<HandlerOutput> {
        self.invoke_with_records(
            binding,
            record,
            source_record.map_or(HandlerRecordSource::None, HandlerRecordSource::ReadOnly),
            phase,
            value,
            old_value,
        )
    }

    /// Executes a record-level editor binding against a transactional record.
    ///
    /// # Errors
    ///
    /// Returns [`SemanticError`] when the binding is not executable or the
    /// selected handler rejects the invocation.
    pub fn invoke_with_writable_record<'a>(
        &self,
        binding: &'a CallbackBinding,
        record: HandlerRecordContext,
        source_record: &'a WritableRecord,
        phase: HandlerPhase,
        value: Option<&'a FieldValue<'static>>,
        old_value: Option<&'a FieldValue<'static>>,
    ) -> Result<HandlerOutput> {
        self.invoke_with_records(
            binding,
            record,
            HandlerRecordSource::Writable(source_record),
            phase,
            value,
            old_value,
        )
    }

    fn invoke_with_records<'a>(
        &self,
        binding: &'a CallbackBinding,
        record: HandlerRecordContext,
        source: HandlerRecordSource<'a>,
        phase: HandlerPhase,
        value: Option<&'a FieldValue<'static>>,
        old_value: Option<&'a FieldValue<'static>>,
    ) -> Result<HandlerOutput> {
        let empty_configuration = serde_json::Value::Null;
        let (id, minimum_version, configuration) = match &binding.implementation {
            CallbackImplementation::BuiltIn { operation } => (
                operation.id.as_str(),
                operation.minimum_version,
                &operation.configuration,
            ),
            CallbackImplementation::CustomHandler {
                handler,
                minimum_handler_version,
            } => (
                handler.as_str(),
                *minimum_handler_version,
                &empty_configuration,
            ),
            _ => {
                return Err(SemanticError::Handler {
                    handler: binding.callback_id.clone(),
                    message: "binding is not a semantic handler".to_owned(),
                });
            }
        };
        self.require(id, minimum_version)?
            .invoke(HandlerInvocation {
                context: HandlerContext {
                    binding,
                    record_signature: record.record_signature,
                    form_id: record.form_id,
                    form_version: record.form_version,
                    game: record.game,
                    configuration,
                },
                phase,
                value,
                old_value,
                source_record: match source {
                    HandlerRecordSource::ReadOnly(record) => Some(record),
                    HandlerRecordSource::None | HandlerRecordSource::Writable(_) => None,
                },
                source_writable_record: match source {
                    HandlerRecordSource::Writable(record) => Some(record),
                    HandlerRecordSource::None | HandlerRecordSource::ReadOnly(_) => None,
                },
            })
    }
}

struct NormalizeRadians;

impl SemanticHandler for NormalizeRadians {
    fn id(&self) -> &'static str {
        "normalize.radians"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        let Some(FieldValue::Float(value)) = invocation.value else {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "radians normalizer requires a floating-point value".to_owned(),
            });
        };
        Ok(HandlerOutput::Value(FieldValue::Float(
            normalize_xedit_radians(*value),
        )))
    }
}

struct ModelInfoConflictPriority;

impl SemanticHandler for ModelInfoConflictPriority {
    fn id(&self) -> &'static str {
        "conflict.model_info_form_version"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        Ok(HandlerOutput::ConflictPriority(
            model_info_conflict_priority(invocation.context.game, invocation.context.form_version),
        ))
    }
}

struct IgnoreEmptyConflictPriority;

impl SemanticHandler for IgnoreEmptyConflictPriority {
    fn id(&self) -> &'static str {
        "conflict.ignore_empty"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        let value = invocation.value.ok_or_else(|| SemanticError::Handler {
            handler: self.id().to_owned(),
            message: "empty-value conflict priority requires a value".to_owned(),
        })?;
        Ok(HandlerOutput::ConflictPriority(if is_empty_value(value) {
            ConflictPriority::Ignore
        } else {
            ConflictPriority::Normal
        }))
    }
}

struct CellWaterConflictPriority;

impl SemanticHandler for CellWaterConflictPriority {
    fn id(&self) -> &'static str {
        "conflict.cell_water_height"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::Conflict {
            return Ok(HandlerOutput::None);
        }
        let record = invocation
            .source_record
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "CELL water conflict priority requires the source record".to_owned(),
            })?;
        if record.header.signature != Signature(*b"CELL")
            || invocation.context.record_signature != record.header.signature
            || invocation.context.form_id != record.header.form_id
        {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "CELL water conflict priority received mismatched record metadata"
                    .to_owned(),
            });
        }
        if record.header.flags.contains(RecordFlags::DELETED) {
            return Ok(HandlerOutput::ConflictPriority(ConflictPriority::Normal));
        }
        let Some(data) = record.get(Signature::DATA)? else {
            return Ok(HandlerOutput::ConflictPriority(ConflictPriority::Normal));
        };
        let interior = data.as_bytes().first().is_some_and(|flags| flags & 1 != 0);
        Ok(HandlerOutput::ConflictPriority(if interior {
            ConflictPriority::Ignore
        } else {
            ConflictPriority::Normal
        }))
    }
}

struct ConstantMetadataBoolean;

impl SemanticHandler for ConstantMetadataBoolean {
    fn id(&self) -> &'static str {
        "metadata.constant_boolean"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::RecordMetadata {
            return Ok(HandlerOutput::None);
        }
        let value = invocation
            .context
            .configuration
            .get("value")
            .and_then(serde_json::Value::as_bool)
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "constant metadata decision requires a boolean value".to_owned(),
            })?;
        Ok(HandlerOutput::Boolean(value))
    }
}

struct MorrowindGridCell;

impl SemanticHandler for MorrowindGridCell {
    fn id(&self) -> &'static str {
        "metadata.morrowind.grid_cell"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::RecordMetadata {
            return Ok(HandlerOutput::None);
        }
        let record = require_source_record(self.id(), &invocation)?;
        let grid = morrowind_grid_cell(self.id(), record)?;
        Ok(grid.map_or(HandlerOutput::None, HandlerOutput::GridCell))
    }
}

struct MorrowindGridFormId;

impl SemanticHandler for MorrowindGridFormId {
    fn id(&self) -> &'static str {
        "metadata.morrowind.grid_form_id"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::RecordMetadata {
            return Ok(HandlerOutput::None);
        }
        let record = require_source_record(self.id(), &invocation)?;
        let base = configured_byte(self.id(), invocation.context.configuration, "base")?;
        let Some(grid) = morrowind_grid_cell(self.id(), record)? else {
            return Ok(HandlerOutput::None);
        };
        Ok(grid_cell_form_id(base, grid).map_or(HandlerOutput::None, HandlerOutput::FormId))
    }
}

struct MorrowindReferenceFormId;

impl SemanticHandler for MorrowindReferenceFormId {
    fn id(&self) -> &'static str {
        "metadata.morrowind.reference_form_id"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::RecordMetadata {
            return Ok(HandlerOutput::None);
        }
        let record = require_source_record(self.id(), &invocation)?;
        let Some(frmr) = record.get(Signature(*b"FRMR"))? else {
            return Ok(HandlerOutput::None);
        };
        let mut form_id = frmr.as_form_id()?;
        if form_id.file_index() == 0 {
            form_id.0 |= 0xFF00_0000;
        }
        Ok(HandlerOutput::FormId(form_id))
    }
}

struct NullRecordFormId;

impl SemanticHandler for NullRecordFormId {
    fn id(&self) -> &'static str {
        "metadata.null_form_id"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase == HandlerPhase::RecordMetadata {
            Ok(HandlerOutput::FormId(FormId::NULL))
        } else {
            Ok(HandlerOutput::None)
        }
    }
}

struct MorrowindGridIdentity;

impl SemanticHandler for MorrowindGridIdentity {
    fn id(&self) -> &'static str {
        "metadata.morrowind.grid_identity"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::RecordMetadata {
            return Ok(HandlerOutput::None);
        }
        let record = require_source_record(self.id(), &invocation)?;
        let prefix = invocation
            .context
            .configuration
            .get("prefix")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let fallback_to_editor_id = invocation
            .context
            .configuration
            .get("fallback_to_editor_id")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let identity = match morrowind_grid_cell(self.id(), record)? {
            Some(grid) => format!("{prefix}{}", grid_cell_sort_key(grid)),
            None if fallback_to_editor_id => morrowind_editor_id(record)?.unwrap_or_default(),
            None => String::new(),
        };
        Ok(HandlerOutput::Text(identity))
    }
}

struct MorrowindScriptEditorId;

impl SemanticHandler for MorrowindScriptEditorId {
    fn id(&self) -> &'static str {
        "metadata.morrowind.script_editor_id"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        match invocation.phase {
            HandlerPhase::RecordMetadata => {
                let record = require_source_record(self.id(), &invocation)?;
                if record.header.signature != Signature(*b"SCPT") {
                    return Err(SemanticError::Handler {
                        handler: self.id().to_owned(),
                        message: format!(
                            "record {} is not a Morrowind script",
                            record.header.signature
                        ),
                    });
                }
                let Some(header) = record.get(Signature(*b"SCHD"))? else {
                    return Ok(HandlerOutput::Text(String::new()));
                };
                let bytes = header.as_bytes();
                if bytes.len() < 32 {
                    return Err(SemanticError::Handler {
                        handler: self.id().to_owned(),
                        message: format!(
                            "SCPT header is truncated: expected at least 32 bytes, got {}",
                            bytes.len()
                        ),
                    });
                }
                let end = bytes[..32].iter().position(|byte| *byte == 0).unwrap_or(32);
                let (editor_id, had_errors) =
                    encoding_rs::WINDOWS_1252.decode_without_bom_handling(&bytes[..end]);
                if had_errors {
                    return Err(SemanticError::Handler {
                        handler: self.id().to_owned(),
                        message: "SCPT editor ID is not valid Windows-1252".to_owned(),
                    });
                }
                Ok(HandlerOutput::Text(editor_id.into_owned()))
            }
            HandlerPhase::AfterSet => {
                let field_path = invocation
                    .context
                    .configuration
                    .get("field_path")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| SemanticError::Handler {
                        handler: self.id().to_owned(),
                        message: "SCPT editor ID setter requires `field_path`".to_owned(),
                    })?;
                let value = match invocation.value {
                    Some(FieldValue::String(value)) => value.to_string(),
                    _ => {
                        return Err(SemanticError::Handler {
                            handler: self.id().to_owned(),
                            message: "SCPT editor ID setter requires a string value".to_owned(),
                        });
                    }
                };
                Ok(HandlerOutput::Mutations(vec![HandlerMutation::Set {
                    path: field_path.to_owned(),
                    occurrence: 0,
                    value: OwnedFieldValue::String(value),
                }]))
            }
            _ => Ok(HandlerOutput::None),
        }
    }
}

struct IntegerRecordIndexKey;

impl SemanticHandler for IntegerRecordIndexKey {
    fn id(&self) -> &'static str {
        "metadata.integer_index_key"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::RecordMetadata {
            return Ok(HandlerOutput::None);
        }
        let record = require_source_record(self.id(), &invocation)?;
        let signature = configured_signature(
            self.id(),
            invocation.context.configuration,
            "subrecord_signature",
        )?;
        let index = configured_text(self.id(), invocation.context.configuration, "index")?;
        let Some(subrecord) = record.get(signature)? else {
            return Ok(HandlerOutput::IndexKeys(Vec::new()));
        };
        Ok(HandlerOutput::IndexKeys(vec![RecordIndexKey {
            index: index.to_owned(),
            key: subrecord.as_u32()?.to_string(),
        }]))
    }
}

struct StarfieldAvmdIndexKey;

impl SemanticHandler for StarfieldAvmdIndexKey {
    fn id(&self) -> &'static str {
        "metadata.starfield.avmd_index_key"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::RecordMetadata {
            return Ok(HandlerOutput::None);
        }
        let record = require_source_record(self.id(), &invocation)?;
        if record.header.signature != Signature(*b"AVMD") {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: format!("record {} is not AVMD", record.header.signature),
            });
        }
        let (Some(kind), Some(name)) = (
            record.get(Signature(*b"MNAM"))?,
            record.get(Signature(*b"TNAM"))?,
        ) else {
            return Ok(HandlerOutput::IndexKeys(Vec::new()));
        };
        let index = match kind.as_u32()? {
            1 => "simple_group",
            2 => "complex_group",
            3 => "modulation",
            _ => return Ok(HandlerOutput::IndexKeys(Vec::new())),
        };
        let name = name.as_zstring()?;
        if name.is_empty() {
            return Ok(HandlerOutput::IndexKeys(Vec::new()));
        }
        Ok(HandlerOutput::IndexKeys(vec![RecordIndexKey {
            index: index.to_owned(),
            key: name.to_owned(),
        }]))
    }
}

fn require_source_record<'a>(
    handler: &str,
    invocation: &'a HandlerInvocation<'_>,
) -> Result<&'a Record> {
    invocation
        .source_record
        .ok_or_else(|| SemanticError::Handler {
            handler: handler.to_owned(),
            message: "record metadata callback requires the source record".to_owned(),
        })
}

fn configured_text<'a>(
    handler: &str,
    configuration: &'a serde_json::Value,
    key: &str,
) -> Result<&'a str> {
    configuration
        .get(key)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| SemanticError::Handler {
            handler: handler.to_owned(),
            message: format!("record metadata callback requires string configuration `{key}`"),
        })
}

fn configured_signature(
    handler: &str,
    configuration: &serde_json::Value,
    key: &str,
) -> Result<Signature> {
    let value = configured_text(handler, configuration, key)?;
    let bytes: [u8; 4] = value
        .as_bytes()
        .try_into()
        .map_err(|_| SemanticError::Handler {
            handler: handler.to_owned(),
            message: format!("record metadata configuration `{key}` must be four ASCII bytes"),
        })?;
    if !bytes.iter().all(u8::is_ascii) {
        return Err(SemanticError::Handler {
            handler: handler.to_owned(),
            message: format!("record metadata configuration `{key}` must be four ASCII bytes"),
        });
    }
    Ok(Signature(bytes))
}

fn configured_byte(handler: &str, configuration: &serde_json::Value, key: &str) -> Result<u8> {
    configuration
        .get(key)
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u8::try_from(value).ok())
        .ok_or_else(|| SemanticError::Handler {
            handler: handler.to_owned(),
            message: format!("record metadata callback requires byte configuration `{key}`"),
        })
}

fn morrowind_grid_cell(handler: &str, record: &Record) -> Result<Option<RecordGridCell>> {
    let (signature, x_offset, y_offset) = if record.header.signature == Signature(*b"CELL") {
        (Signature::DATA, 4, 8)
    } else if record.header.signature == Signature(*b"LAND") {
        (Signature(*b"INTV"), 0, 4)
    } else if record.header.signature == Signature(*b"PGRD") {
        (Signature::DATA, 0, 4)
    } else {
        return Err(SemanticError::Handler {
            handler: handler.to_owned(),
            message: format!(
                "record {} does not provide Morrowind grid metadata",
                record.header.signature
            ),
        });
    };
    let Some(subrecord) = record.get(signature)? else {
        return Ok(None);
    };
    let bytes = subrecord.as_bytes();
    if bytes.len() < y_offset + 4 {
        return Err(SemanticError::Handler {
            handler: handler.to_owned(),
            message: format!(
                "{} grid payload is truncated: expected at least {} bytes, got {}",
                record.header.signature,
                y_offset + 4,
                bytes.len()
            ),
        });
    }
    if record.header.signature == Signature(*b"CELL") {
        let flags = u32::from_le_bytes(
            bytes[0..4]
                .try_into()
                .expect("CELL flag slice has an exact checked length"),
        );
        if flags & 1 != 0 {
            return Ok(None);
        }
    }
    let x = i32::from_le_bytes(
        bytes[x_offset..x_offset + 4]
            .try_into()
            .expect("grid X slice has an exact checked length"),
    );
    let y = i32::from_le_bytes(
        bytes[y_offset..y_offset + 4]
            .try_into()
            .expect("grid Y slice has an exact checked length"),
    );
    if record.header.signature == Signature(*b"PGRD") && x == 0 && y == 0 {
        return Ok(None);
    }
    Ok(Some(RecordGridCell { x, y }))
}

fn grid_cell_form_id(base: u8, grid: RecordGridCell) -> Option<FormId> {
    if !(-512..=511).contains(&grid.x) || !(-512..=511).contains(&grid.y) {
        return None;
    }
    let x = u32::try_from(grid.x + 512).ok()?;
    let y = u32::try_from(grid.y + 512).ok()?;
    Some(FormId((u32::from(base) << 16) + (x << 10) + y))
}

fn grid_cell_sort_key(grid: RecordGridCell) -> String {
    let x = i64::from(grid.x) - i64::from(i32::MIN);
    let y = i64::from(grid.y) - i64::from(i32::MIN);
    format!("{x:08X}|{y:08X}")
}

fn morrowind_editor_id(record: &Record) -> Result<Option<String>> {
    let Some(name) = record.get(Signature(*b"NAME"))? else {
        return Ok(None);
    };
    Ok(Some(name.as_zstring()?.to_owned()))
}

fn is_empty_value(value: &FieldValue<'_>) -> bool {
    match value {
        FieldValue::Bytes(value) => value.is_empty(),
        FieldValue::String(value) => value.is_empty(),
        FieldValue::Array(values) => values.is_empty(),
        FieldValue::Struct(values) => values.is_empty(),
        _ => false,
    }
}

struct FormatRgb;

impl SemanticHandler for FormatRgb {
    fn id(&self) -> &'static str {
        "format.rgb"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if !matches!(
            invocation.phase,
            HandlerPhase::Display
                | HandlerPhase::Summary
                | HandlerPhase::SortKey
                | HandlerPhase::EditValue
                | HandlerPhase::NativeValue
        ) {
            return Ok(HandlerOutput::None);
        }
        let include_alpha = invocation
            .context
            .configuration
            .get("include_alpha")
            .and_then(serde_json::Value::as_bool)
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "RGB formatter requires a boolean include_alpha setting".to_owned(),
            })?;
        let value = invocation.value.ok_or_else(|| SemanticError::Handler {
            handler: self.id().to_owned(),
            message: "RGB formatter requires a value".to_owned(),
        })?;
        Ok(HandlerOutput::Text(format_rgb(value, include_alpha)?))
    }
}

struct RemovableWhenZero;

impl SemanticHandler for RemovableWhenZero {
    fn id(&self) -> &'static str {
        "edit.removable_when_zero"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        let value = invocation.value.ok_or_else(|| SemanticError::Handler {
            handler: self.id().to_owned(),
            message: "removability check requires a value".to_owned(),
        })?;
        Ok(HandlerOutput::Boolean(removable_when_zero(value)?))
    }
}

struct ResourceHashFormatter {
    resolver: Option<Arc<dyn ResourceHashResolver>>,
}

impl SemanticHandler for ResourceHashFormatter {
    fn id(&self) -> &'static str {
        "format.resource_hash"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        let kind = invocation
            .context
            .configuration
            .get("kind")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "resource-hash formatter requires a kind setting".to_owned(),
            })?;
        let value = invocation.value.ok_or_else(|| SemanticError::Handler {
            handler: self.id().to_owned(),
            message: "resource-hash formatter requires an integer value".to_owned(),
        })?;
        let hash = resource_hash(value)?;
        if invocation.phase == HandlerPhase::EditValue {
            return Ok(HandlerOutput::Text((hash as i64).to_string()));
        }
        if !matches!(
            invocation.phase,
            HandlerPhase::Display | HandlerPhase::Summary | HandlerPhase::SortKey
        ) {
            return Ok(HandlerOutput::Text(String::new()));
        }
        let resolved = match (kind, &self.resolver) {
            ("file", Some(resolver)) => resolver.resolve_file_hash(hash),
            ("folder", Some(resolver)) => resolver.resolve_folder_hash(hash),
            ("file" | "folder", None) => None,
            _ => {
                return Err(SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: format!("unsupported resource-hash kind {kind:?}"),
                });
            }
        };
        let text = resolved
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| match invocation.phase {
                HandlerPhase::Display => format!("{{{hash:016X}}}"),
                HandlerPhase::Summary if hash <= u64::from(u32::MAX) => {
                    format!("{{{hash:08X}}}")
                }
                HandlerPhase::Summary => format!("{{{hash:016X}}}"),
                HandlerPhase::SortKey => format!("{hash:016X}"),
                _ => String::new(),
            });
        Ok(HandlerOutput::Text(text))
    }
}

fn resource_hash(value: &FieldValue<'_>) -> Result<u64> {
    match value {
        FieldValue::Int(value) => Ok(*value as u64),
        FieldValue::UInt(value) => Ok(*value),
        FieldValue::Enumeration { value, .. } => Ok(*value as u64),
        FieldValue::Flags { value, .. } => Ok(*value),
        _ => Err(SemanticError::Handler {
            handler: "format.resource_hash".to_owned(),
            message: "resource-hash formatter requires an integer value".to_owned(),
        }),
    }
}

struct WwiseGuidFormatter {
    resolver: Option<Arc<dyn WwiseGuidResolver>>,
}

impl SemanticHandler for WwiseGuidFormatter {
    fn id(&self) -> &'static str {
        "format.wwise_guid"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase == HandlerPhase::ParseEditValue {
            let Some(FieldValue::String(value)) = invocation.value else {
                return Err(wwise_guid_error(
                    "Wwise GUID edit parsing requires a string value",
                ));
            };
            return Ok(HandlerOutput::Value(FieldValue::Bytes(
                std::borrow::Cow::Owned(parse_wwise_guid(value)?.to_vec()),
            )));
        }
        let value = invocation
            .value
            .ok_or_else(|| wwise_guid_error("Wwise GUID formatting requires a value"))?;
        let guid = wwise_guid_bytes(value)?;
        let canonical = format_wwise_guid(guid);
        if !matches!(
            invocation.phase,
            HandlerPhase::Display
                | HandlerPhase::Summary
                | HandlerPhase::SortKey
                | HandlerPhase::EditValue
                | HandlerPhase::NativeValue
        ) {
            return Ok(HandlerOutput::None);
        }
        if matches!(
            invocation.phase,
            HandlerPhase::SortKey | HandlerPhase::NativeValue
        ) {
            return Ok(HandlerOutput::Text(canonical));
        }
        let Some(resolver) = &self.resolver else {
            return Ok(HandlerOutput::Text(canonical));
        };
        if guid == [0; 16] {
            return Ok(HandlerOutput::Text(String::new()));
        }
        let Some(info) = resolver.resolve_wwise_guid(guid) else {
            return Ok(HandlerOutput::Text(canonical));
        };
        if invocation.phase == HandlerPhase::Summary && !info.name().is_empty() {
            return Ok(HandlerOutput::Text(info.name().to_owned()));
        }
        let mut formatted = if info.name().is_empty() {
            canonical
        } else {
            format!("{} {canonical}", info.name())
        };
        if !info.object_path().is_empty() {
            let object_path = if invocation.phase == HandlerPhase::EditValue {
                truncate_wwise_object_path(info.object_path())
            } else {
                info.object_path().to_owned()
            };
            formatted.push_str(&format!(" \"{object_path}\""));
        }
        Ok(HandlerOutput::Text(formatted))
    }
}

fn wwise_guid_bytes(value: &FieldValue<'_>) -> Result<[u8; 16]> {
    match value {
        FieldValue::Bytes(value) => value
            .as_ref()
            .try_into()
            .map_err(|_| wwise_guid_error("Wwise GUID requires exactly 16 bytes")),
        FieldValue::String(value) => parse_wwise_guid(value),
        _ => Err(wwise_guid_error(
            "Wwise GUID formatting requires bytes or canonical text",
        )),
    }
}

fn format_wwise_guid(guid: [u8; 16]) -> String {
    let data1 = u32::from_le_bytes([guid[0], guid[1], guid[2], guid[3]]);
    let data2 = u16::from_le_bytes([guid[4], guid[5]]);
    let data3 = u16::from_le_bytes([guid[6], guid[7]]);
    format!(
        "{{{data1:08X}-{data2:04X}-{data3:04X}-{:02X}{:02X}-\
         {:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
        guid[8], guid[9], guid[10], guid[11], guid[12], guid[13], guid[14], guid[15]
    )
}

fn parse_wwise_guid(value: &str) -> Result<[u8; 16]> {
    let value = value.trim();
    if value.is_empty() {
        return Ok([0; 16]);
    }
    let canonical = if let Some(start) = value.find('{') {
        let remainder = &value[start..];
        let end = remainder
            .find('}')
            .ok_or_else(|| wwise_guid_error("Wwise GUID is missing a closing brace"))?;
        &remainder[..=end]
    } else {
        value
    };
    let body = canonical
        .strip_prefix('{')
        .and_then(|value| value.strip_suffix('}'))
        .unwrap_or(canonical);
    let parts: Vec<&str> = body.split('-').collect();
    if parts.len() != 5
        || parts[0].len() != 8
        || parts[1].len() != 4
        || parts[2].len() != 4
        || parts[3].len() != 4
        || parts[4].len() != 12
    {
        return Err(wwise_guid_error("Wwise GUID has an invalid shape"));
    }
    let data1 = parse_guid_hex_u32(parts[0])?;
    let data2 = parse_guid_hex_u16(parts[1])?;
    let data3 = parse_guid_hex_u16(parts[2])?;
    let tail = format!("{}{}", parts[3], parts[4]);
    let mut guid = [0_u8; 16];
    guid[0..4].copy_from_slice(&data1.to_le_bytes());
    guid[4..6].copy_from_slice(&data2.to_le_bytes());
    guid[6..8].copy_from_slice(&data3.to_le_bytes());
    for (index, chunk) in tail.as_bytes().chunks_exact(2).enumerate() {
        let text = std::str::from_utf8(chunk)
            .map_err(|_| wwise_guid_error("Wwise GUID contains invalid text"))?;
        guid[8 + index] = u8::from_str_radix(text, 16)
            .map_err(|_| wwise_guid_error("Wwise GUID contains invalid hexadecimal digits"))?;
    }
    Ok(guid)
}

fn parse_guid_hex_u32(value: &str) -> Result<u32> {
    u32::from_str_radix(value, 16)
        .map_err(|_| wwise_guid_error("Wwise GUID contains invalid hexadecimal digits"))
}

fn parse_guid_hex_u16(value: &str) -> Result<u16> {
    u16::from_str_radix(value, 16)
        .map_err(|_| wwise_guid_error("Wwise GUID contains invalid hexadecimal digits"))
}

fn truncate_wwise_object_path(value: &str) -> String {
    if value.chars().count() <= 64 {
        return value.to_owned();
    }
    value.chars().take(61).chain("...".chars()).collect()
}

fn wwise_guid_error(message: impl Into<String>) -> SemanticError {
    SemanticError::Handler {
        handler: "format.wwise_guid".to_owned(),
        message: message.into(),
    }
}

struct ModelInfoCounts;

impl SemanticHandler for ModelInfoCounts {
    fn id(&self) -> &'static str {
        "edit.model_info_counts"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        let value = invocation.value.ok_or_else(|| SemanticError::Handler {
            handler: self.id().to_owned(),
            message: "model-info counter update requires a struct value".to_owned(),
        })?;
        Ok(HandlerOutput::Value(update_model_info_counts(value)?))
    }
}

struct ModelInfoArrayCount;

impl SemanticHandler for ModelInfoArrayCount {
    fn id(&self) -> &'static str {
        "array.model_info_header_count"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::ArrayCount {
            return Ok(HandlerOutput::None);
        }
        let header_index = invocation
            .context
            .configuration
            .get("header_index")
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| model_info_array_error("header_index is missing or invalid"))?;
        let bytes = match invocation.value {
            Some(FieldValue::Bytes(value)) => value.as_ref(),
            _ => return Err(model_info_array_error("array count requires payload bytes")),
        };
        let header_count = bytes
            .get(..4)
            .and_then(|value| value.try_into().ok())
            .map(u32::from_le_bytes)
            .map_or(0_usize, |value| value as usize);
        if header_count <= header_index {
            return Ok(HandlerOutput::Integer(0));
        }
        let offset = header_index
            .checked_mul(4)
            .and_then(|value| value.checked_add(4))
            .ok_or_else(|| model_info_array_error("header offset overflowed"))?;
        let end = offset
            .checked_add(4)
            .ok_or_else(|| model_info_array_error("header offset overflowed"))?;
        let count = bytes
            .get(offset..end)
            .and_then(|value| value.try_into().ok())
            .map(u32::from_le_bytes)
            .unwrap_or(0);
        Ok(HandlerOutput::Integer(i64::from(count)))
    }
}

struct CtdaRunOnAfterSet;

impl SemanticHandler for CtdaRunOnAfterSet {
    fn id(&self) -> &'static str {
        "edit.ctda_run_on"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::AfterSet {
            return Ok(HandlerOutput::None);
        }
        let value = invocation.value.ok_or_else(|| SemanticError::Handler {
            handler: self.id().to_owned(),
            message: "CTDA Run On update requires a new integer value".to_owned(),
        })?;
        let new_value = callback_integer(value, self.id())?;
        let old_value = invocation
            .old_value
            .map(|value| callback_integer(value, self.id()))
            .transpose()?;
        if old_value == Some(new_value) || new_value == 2 {
            return Ok(HandlerOutput::None);
        }
        let parent = invocation
            .context
            .binding
            .path
            .strip_suffix("/7:Run On")
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "CTDA Run On binding has an unexpected schema path".to_owned(),
            })?;
        Ok(HandlerOutput::Mutations(vec![HandlerMutation::Set {
            path: format!("{parent}/8:Reference"),
            occurrence: 0,
            value: OwnedFieldValue::UInt(0),
        }]))
    }
}

struct CtdaTypeAfterSet;

impl SemanticHandler for CtdaTypeAfterSet {
    fn id(&self) -> &'static str {
        "edit.ctda_type"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::AfterSet {
            return Ok(HandlerOutput::None);
        }
        let new_value = u64::try_from(callback_integer(
            invocation.value.ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "CTDA Type update requires a new integer value".to_owned(),
            })?,
            self.id(),
        )?)
        .map_err(|_| SemanticError::Handler {
            handler: self.id().to_owned(),
            message: "CTDA Type value must be non-negative".to_owned(),
        })?;
        let old_value = invocation
            .old_value
            .map(|value| callback_integer(value, self.id()))
            .transpose()?
            .map(u64::try_from)
            .transpose()
            .map_err(|_| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "previous CTDA Type value must be non-negative".to_owned(),
            })?
            .unwrap_or(0);
        if old_value == new_value {
            return Ok(HandlerOutput::None);
        }
        let parent = invocation
            .context
            .binding
            .path
            .strip_suffix("/0:Type")
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "CTDA Type binding has an unexpected schema path".to_owned(),
            })?;
        let mut mutations = Vec::new();
        if (old_value & 0x04) != (new_value & 0x04) {
            mutations.push(HandlerMutation::Set {
                path: format!("{parent}/2:Comparison Value"),
                occurrence: 0,
                value: OwnedFieldValue::UInt(0),
            });
        }
        if matches!(
            invocation.context.game,
            SchemaGame::Fallout3 | SchemaGame::FalloutNv
        ) && new_value & 0x02 != 0
        {
            mutations.push(HandlerMutation::Set {
                path: format!("{parent}/7:Run On"),
                occurrence: 0,
                value: OwnedFieldValue::UInt(1),
            });
            mutations.push(HandlerMutation::Set {
                path: invocation.context.binding.path.clone(),
                occurrence: 0,
                value: OwnedFieldValue::UInt(new_value & !0x02),
            });
        }
        if mutations.is_empty() {
            Ok(HandlerOutput::None)
        } else {
            Ok(HandlerOutput::Mutations(mutations))
        }
    }
}

struct MessageDisplayTimeAfterSet;

impl SemanticHandler for MessageDisplayTimeAfterSet {
    fn id(&self) -> &'static str {
        "edit.message_display_time"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::AfterSet {
            return Ok(HandlerOutput::None);
        }
        let Some(value) = invocation.value else {
            return Ok(HandlerOutput::None);
        };
        let new_value = callback_integer(value, self.id())?;
        let old_value = invocation
            .old_value
            .map(|value| callback_integer(value, self.id()))
            .transpose()?
            .unwrap_or(0);
        if old_value & 1 == new_value & 1 {
            return Ok(HandlerOutput::None);
        }
        let path = configured_text(
            self.id(),
            invocation.context.configuration,
            "display_time_path",
        )?
        .to_owned();
        Ok(HandlerOutput::Mutations(vec![
            HandlerMutation::SynchronizePresence {
                path,
                occurrence: 0,
                present: new_value & 1 == 0,
                value: OwnedFieldValue::UInt(0),
            },
        ]))
    }
}

struct FormListEditorIdAfterSet;

impl SemanticHandler for FormListEditorIdAfterSet {
    fn id(&self) -> &'static str {
        "edit.form_list_editor_id"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::AfterSet {
            return Ok(HandlerOutput::None);
        }
        let Some(FieldValue::String(new_value)) = invocation.value else {
            return Ok(HandlerOutput::None);
        };
        let Some(FieldValue::String(old_value)) = invocation.old_value else {
            return Ok(HandlerOutput::None);
        };
        if has_ordered_list_suffix(old_value) == has_ordered_list_suffix(new_value) {
            return Ok(HandlerOutput::None);
        }
        let entry_path =
            configured_text(self.id(), invocation.context.configuration, "entry_path")?;
        Ok(HandlerOutput::Mutations(vec![HandlerMutation::RemoveAll {
            path: entry_path.to_owned(),
        }]))
    }
}

fn has_ordered_list_suffix(value: &str) -> bool {
    value
        .get(value.len().saturating_sub("OrderedList".len())..)
        .is_some_and(|suffix| suffix.eq_ignore_ascii_case("OrderedList"))
}

struct MagicEffectSecondAvWeightAfterSet;

impl SemanticHandler for MagicEffectSecondAvWeightAfterSet {
    fn id(&self) -> &'static str {
        "edit.magic_effect_second_av_weight"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::AfterSet {
            return Ok(HandlerOutput::None);
        }
        let Some(value) = invocation.value else {
            return Ok(HandlerOutput::None);
        };
        let FieldValue::Float(value) = value else {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "second actor-value weight callback requires a float".to_owned(),
            });
        };
        if *value == 0.0 {
            return Ok(HandlerOutput::None);
        }
        let archetype_path = configured_text(
            self.id(),
            invocation.context.configuration,
            "archetype_path",
        )?;
        Ok(HandlerOutput::Mutations(vec![
            HandlerMutation::SetIfEqual {
                path: archetype_path.to_owned(),
                occurrence: 0,
                expected: OwnedFieldValue::UInt(0),
                value: OwnedFieldValue::UInt(0xff),
            },
        ]))
    }
}

struct RefreshSiblingUnions;

impl SemanticHandler for RefreshSiblingUnions {
    fn id(&self) -> &'static str {
        "edit.refresh_sibling_unions"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::AfterSet {
            return Ok(HandlerOutput::None);
        }
        // The editor overlays the full in-flight value tree onto the expression
        // context before selecting unions, which performs xEdit's eager refresh
        // without an additional byte mutation.
        Ok(HandlerOutput::None)
    }
}

struct InvalidateConflicts;

impl SemanticHandler for InvalidateConflicts {
    fn id(&self) -> &'static str {
        "edit.invalidate_conflicts"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::AfterSet {
            return Ok(HandlerOutput::None);
        }
        // xEdit invalidates a mutable record conflict cache here. Bethkit's
        // conflict report is recomputed from its inputs and retains no record
        // cache, so completing the callback requires no byte mutation.
        Ok(HandlerOutput::None)
    }
}

struct CtdaTypeFormatter;

impl SemanticHandler for CtdaTypeFormatter {
    fn id(&self) -> &'static str {
        "format.ctda_type"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase == HandlerPhase::ParseEditValue {
            let FieldValue::String(value) =
                invocation.value.ok_or_else(|| SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: "CTDA Type edit parsing requires text".to_owned(),
                })?
            else {
                return Err(SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: "CTDA Type edit parsing requires text".to_owned(),
                });
            };
            return Ok(HandlerOutput::Value(FieldValue::UInt(
                parse_ctda_type_edit_value(
                    value,
                    matches!(
                        invocation.context.game,
                        SchemaGame::Fallout3 | SchemaGame::FalloutNv
                    ),
                ),
            )));
        }
        let value = u64::try_from(callback_integer(
            invocation.value.ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "CTDA Type formatting requires an integer value".to_owned(),
            })?,
            self.id(),
        )?)
        .map_err(|_| SemanticError::Handler {
            handler: self.id().to_owned(),
            message: "CTDA Type value must be non-negative".to_owned(),
        })?;
        let legacy = matches!(
            invocation.context.game,
            SchemaGame::Fallout3 | SchemaGame::FalloutNv
        );
        let text = match invocation.phase {
            HandlerPhase::Display | HandlerPhase::Summary => {
                format_ctda_type_display(value, legacy)
            }
            HandlerPhase::SortKey => format!("{value:02X}"),
            HandlerPhase::EditValue => format_ctda_type_edit_value(
                value,
                match invocation.context.game {
                    SchemaGame::Fallout3 => 6,
                    _ => 8,
                },
                legacy,
            ),
            HandlerPhase::NativeValue => String::new(),
            _ => return Ok(HandlerOutput::None),
        };
        Ok(HandlerOutput::Text(text))
    }
}

struct IntegerLookupFormatter;

impl SemanticHandler for IntegerLookupFormatter {
    fn id(&self) -> &'static str {
        "format.integer_lookup"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        let values = integer_lookup_values(invocation.context.configuration, self.id())?;
        if invocation.phase == HandlerPhase::ParseEditValue {
            let FieldValue::String(input) =
                invocation.value.ok_or_else(|| SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: "integer lookup edit parsing requires text".to_owned(),
                })?
            else {
                return Err(SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: "integer lookup edit parsing requires text".to_owned(),
                });
            };
            let value = values
                .iter()
                .find(|(_, name)| name.eq_ignore_ascii_case(input))
                .map(|(value, _)| *value)
                .map_or_else(
                    || {
                        input
                            .trim()
                            .parse::<i64>()
                            .map_err(|error| SemanticError::Handler {
                                handler: self.id().to_owned(),
                                message: format!("invalid integer edit value {input:?}: {error}"),
                            })
                    },
                    Ok,
                )?;
            return Ok(HandlerOutput::Value(FieldValue::Int(value)));
        }

        let value = i64::try_from(callback_integer(
            invocation.value.ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "integer lookup formatting requires an integer value".to_owned(),
            })?,
            self.id(),
        )?)
        .map_err(|_| SemanticError::Handler {
            handler: self.id().to_owned(),
            message: "integer lookup value exceeds i64".to_owned(),
        })?;
        let name = values
            .iter()
            .find(|(candidate, _)| *candidate == value)
            .map(|(_, name)| *name);
        let text = match invocation.phase {
            HandlerPhase::Display => name.map_or_else(
                || match configuration_string(
                    invocation.context.configuration,
                    "unknown_display",
                    self.id(),
                ) {
                    Ok("angle") => Ok(format!("<Unknown: {value}>")),
                    Ok(policy) => Err(SemanticError::Handler {
                        handler: self.id().to_owned(),
                        message: format!("unsupported unknown display policy {policy:?}"),
                    }),
                    Err(error) => Err(error),
                },
                |name| Ok(name.to_owned()),
            )?,
            HandlerPhase::Summary => name.map_or_else(
                || match configuration_string(
                    invocation.context.configuration,
                    "unknown_summary",
                    self.id(),
                ) {
                    Ok("decimal") => Ok(value.to_string()),
                    Ok("angle") => Ok(format!("<Unknown: {value}>")),
                    Ok(policy) => Err(SemanticError::Handler {
                        handler: self.id().to_owned(),
                        message: format!("unsupported unknown summary policy {policy:?}"),
                    }),
                    Err(error) => Err(error),
                },
                |name| Ok(name.to_owned()),
            )?,
            HandlerPhase::EditValue => name.map_or_else(|| value.to_string(), str::to_owned),
            HandlerPhase::SortKey => {
                let width = invocation
                    .context
                    .configuration
                    .get("sort_hex_width")
                    .and_then(serde_json::Value::as_u64)
                    .ok_or_else(|| SemanticError::Handler {
                        handler: self.id().to_owned(),
                        message: "integer lookup requires sort_hex_width".to_owned(),
                    })?;
                let width = usize::try_from(width).map_err(|_| SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: "integer lookup sort width exceeds usize".to_owned(),
                })?;
                format!("{:0width$X}", value as u64)
            }
            HandlerPhase::NativeValue => String::new(),
            _ => return Ok(HandlerOutput::None),
        };
        Ok(HandlerOutput::Text(text))
    }
}

struct SynchronizeCountAfterSet;

impl SemanticHandler for SynchronizeCountAfterSet {
    fn id(&self) -> &'static str {
        "edit.sync_count"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::AfterSet {
            return Ok(HandlerOutput::None);
        }
        let value = if let Some(FieldValue::Array(values)) = invocation.value {
            u64::try_from(values.len()).map_err(|_| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "array length exceeds u64".to_owned(),
            })?
        } else if let Some(record) = invocation.source_writable_record {
            let signature = configured_signature(
                self.id(),
                invocation.context.configuration,
                "value_signature",
            )?;
            u64::try_from(
                record
                    .subrecords
                    .iter()
                    .filter(|subrecord| subrecord.signature == signature)
                    .count(),
            )
            .map_err(|_| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "array length exceeds u64".to_owned(),
            })?
        } else {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "counter synchronization requires an array value".to_owned(),
            });
        };
        if invocation
            .context
            .configuration
            .get("counter_missing")
            .and_then(serde_json::Value::as_bool)
            == Some(true)
        {
            return Ok(HandlerOutput::None);
        }
        let path =
            configuration_string(invocation.context.configuration, "counter_path", self.id())?
                .to_owned();
        let required = invocation
            .context
            .configuration
            .get("counter_required")
            .and_then(serde_json::Value::as_bool)
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "counter synchronization requires counter_required".to_owned(),
            })?;
        let nested = invocation
            .context
            .configuration
            .get("counter_nested")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        if nested {
            return Ok(HandlerOutput::Mutations(vec![HandlerMutation::Set {
                path,
                occurrence: 0,
                value: OwnedFieldValue::UInt(value),
            }]));
        }
        Ok(HandlerOutput::Mutations(vec![
            HandlerMutation::SynchronizeCount {
                path,
                occurrence: 0,
                value,
                remove_when_zero: !required,
            },
        ]))
    }
}

struct SynchronizeRecordCountsAfterSet;

impl SemanticHandler for SynchronizeRecordCountsAfterSet {
    fn id(&self) -> &'static str {
        "edit.sync_record_counts"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::AfterSet {
            return Ok(HandlerOutput::None);
        }
        let record = invocation
            .source_writable_record
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "record counter synchronization requires a writable record".to_owned(),
            })?;
        let counters = invocation
            .context
            .configuration
            .get("counters")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "record counter synchronization requires a counters array".to_owned(),
            })?;
        let mut mutations = Vec::with_capacity(counters.len());
        for counter in counters {
            if counter
                .get("counter_missing")
                .and_then(serde_json::Value::as_bool)
                == Some(true)
            {
                continue;
            }
            let path = configured_text(self.id(), counter, "counter_path")?.to_owned();
            let counter_signature = configured_signature(self.id(), counter, "counter_signature")?;
            let counter_subrecord = record
                .subrecords
                .iter()
                .find(|subrecord| subrecord.signature == counter_signature);
            if counter
                .get("only_when_counter_exists")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
                && counter_subrecord.is_none()
            {
                continue;
            }
            let required = counter
                .get("counter_required")
                .and_then(serde_json::Value::as_bool)
                .ok_or_else(|| SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: "record counter entry requires counter_required".to_owned(),
                })?;
            let value_signature = configured_signature(self.id(), counter, "value_signature")?;
            let mode = configured_text(self.id(), counter, "mode")?;
            let only_when_missing = counter
                .get("only_when_missing")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            if only_when_missing
                && record
                    .subrecords
                    .iter()
                    .any(|subrecord| subrecord.signature == value_signature)
            {
                continue;
            }
            let count = match mode {
                "subrecord_count" => record
                    .subrecords
                    .iter()
                    .filter(|subrecord| subrecord.signature == value_signature)
                    .count(),
                "u32_payload_count" => {
                    let mut count = 0_usize;
                    for subrecord in record
                        .subrecords
                        .iter()
                        .filter(|subrecord| subrecord.signature == value_signature)
                    {
                        if subrecord.data.len() % std::mem::size_of::<u32>() != 0 {
                            return Err(SemanticError::Handler {
                                handler: self.id().to_owned(),
                                message: format!(
                                    "{value_signature} payload length {} is not divisible by four",
                                    subrecord.data.len()
                                ),
                            });
                        }
                        count = count
                            .checked_add(subrecord.data.len() / std::mem::size_of::<u32>())
                            .ok_or_else(|| SemanticError::Handler {
                                handler: self.id().to_owned(),
                                message: "record counter value overflowed usize".to_owned(),
                            })?;
                    }
                    count
                }
                _ => {
                    return Err(SemanticError::Handler {
                        handler: self.id().to_owned(),
                        message: format!("unsupported record counter mode {mode:?}"),
                    });
                }
            };
            let value = u64::try_from(count).map_err(|_| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "record counter value exceeds u64".to_owned(),
            })?;
            if counter
                .get("counter_nested")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
            {
                mutations.push(HandlerMutation::Set {
                    path,
                    occurrence: 0,
                    value: OwnedFieldValue::UInt(value),
                });
                continue;
            }
            let remove_when_zero = !required
                && (!only_when_missing
                    || counter_subrecord
                        .is_some_and(|subrecord| subrecord.data.iter().all(|byte| *byte == 0)));
            mutations.push(HandlerMutation::SynchronizeCount {
                path,
                occurrence: 0,
                value,
                remove_when_zero,
            });
        }
        Ok(HandlerOutput::Mutations(mutations))
    }
}

fn integer_lookup_values<'a>(
    configuration: &'a serde_json::Value,
    handler: &str,
) -> Result<Vec<(i64, &'a str)>> {
    let values = configuration
        .get("values")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| SemanticError::Handler {
            handler: handler.to_owned(),
            message: "integer lookup requires a values array".to_owned(),
        })?;
    let mut output = Vec::with_capacity(values.len());
    for entry in values {
        let value = entry
            .get("value")
            .and_then(serde_json::Value::as_i64)
            .ok_or_else(|| SemanticError::Handler {
                handler: handler.to_owned(),
                message: "integer lookup value must be an i64".to_owned(),
            })?;
        let name = entry
            .get("name")
            .and_then(serde_json::Value::as_str)
            .filter(|name| !name.is_empty())
            .ok_or_else(|| SemanticError::Handler {
                handler: handler.to_owned(),
                message: "integer lookup name must not be empty".to_owned(),
            })?;
        if output.iter().any(|(existing_value, existing_name)| {
            *existing_value == value || *existing_name == name
        }) {
            return Err(SemanticError::Handler {
                handler: handler.to_owned(),
                message: "integer lookup contains duplicate values or names".to_owned(),
            });
        }
        output.push((value, name));
    }
    if output.is_empty() {
        return Err(SemanticError::Handler {
            handler: handler.to_owned(),
            message: "integer lookup values must not be empty".to_owned(),
        });
    }
    Ok(output)
}

fn configuration_string<'a>(
    configuration: &'a serde_json::Value,
    key: &str,
    handler: &str,
) -> Result<&'a str> {
    configuration
        .get(key)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| SemanticError::Handler {
            handler: handler.to_owned(),
            message: format!("integer lookup requires {key}"),
        })
}

fn format_ctda_type_display(value: u64, legacy: bool) -> String {
    let operator = match value & if legacy { 0xF0 } else { 0xE0 } {
        0x00 => "Equal to",
        0x20 => "Not equal to",
        0x40 => "Greater than",
        0x60 => "Greater than or equal to",
        0x80 => "Less than",
        0xA0 => "Less than or equal to",
        _ => "<Unknown Compare operator>",
    };
    let mut flags = Vec::new();
    if value & 0x01 != 0 {
        flags.push("Or".to_owned());
    }
    if value & 0x02 != 0 {
        flags.push(if legacy {
            "Run on target".to_owned()
        } else {
            "Use aliases".to_owned()
        });
    }
    if value & 0x04 != 0 {
        flags.push("Use global".to_owned());
    }
    if legacy {
        if value & 0x08 != 0 {
            flags.push("<Unknown: 3>".to_owned());
        }
    } else {
        if value & 0x08 != 0 {
            flags.push("Use packdata".to_owned());
        }
        if value & 0x10 != 0 {
            flags.push("Swap Subject and Target".to_owned());
        }
    }
    if flags.is_empty() {
        operator.to_owned()
    } else {
        format!("{operator} / {}", flags.join(", "))
    }
}

fn format_ctda_type_edit_value(value: u64, width: usize, legacy: bool) -> String {
    let mut output = vec![b'0'; width];
    match value & 0xE0 {
        0x00 => output[0] = b'1',
        0x40 => output[1] = b'1',
        0x60 => {
            output[0] = b'1';
            output[1] = b'1';
        }
        0x80 => output[2] = b'1',
        0xA0 => {
            output[0] = b'1';
            output[2] = b'1';
        }
        _ => {}
    }
    if value & 0x01 != 0 {
        output[3] = b'1';
    }
    if legacy {
        if value & 0x04 != 0 {
            output[4] = b'1';
        }
        if value & 0x02 != 0 {
            output[5] = b'1';
        }
    } else {
        if value & 0x02 != 0 {
            output[4] = b'1';
        }
        if value & 0x04 != 0 {
            output[5] = b'1';
        }
        if value & 0x08 != 0 {
            output[6] = b'1';
        }
        if value & 0x10 != 0 {
            output[7] = b'1';
        }
    }
    output.into_iter().map(char::from).collect()
}

fn parse_ctda_type_edit_value(value: &str, legacy: bool) -> u64 {
    let mut bits = value.bytes().chain(std::iter::repeat(b'0'));
    let equal = bits.next() == Some(b'1');
    let greater = bits.next() == Some(b'1');
    let lesser = bits.next() == Some(b'1');
    let mut result = match (equal, greater, lesser) {
        (true, true, false) => 0x60,
        (true, false, true) => 0xA0,
        (false, true, true) => 0x20,
        (false, true, false) => 0x40,
        (false, false, true) => 0x80,
        (false, false, false) => 0x20,
        _ => 0x00,
    };
    if bits.next() == Some(b'1') {
        result |= 0x01;
    }
    if legacy {
        if bits.next() == Some(b'1') {
            result |= 0x04;
        }
        if bits.next() == Some(b'1') {
            result |= 0x02;
        }
    } else {
        for mask in [0x02, 0x04, 0x08, 0x10] {
            if bits.next() == Some(b'1') {
                result |= mask;
            }
        }
    }
    result
}

struct InvalidModelInfoValidation;

impl SemanticHandler for InvalidModelInfoValidation {
    fn id(&self) -> &'static str {
        "validate.invalid_model_info"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::Validation {
            return Ok(HandlerOutput::None);
        }
        Ok(HandlerOutput::Text(
            "SubRecord has invalid format for the Form Version of this record".to_owned(),
        ))
    }
}

fn update_model_info_counts(value: &FieldValue<'_>) -> Result<FieldValue<'static>> {
    let mut updated = value.to_handler_value();
    let FieldValue::Struct(fields) = &mut updated else {
        return Err(model_info_error("model-info value is not a struct"));
    };
    if fields.len() < 4 {
        return Err(model_info_error(format!(
            "model-info struct requires four fields, got {}",
            fields.len()
        )));
    }
    let textures = array_length(&fields[1].value, "Textures")?;
    let addons = array_length(&fields[2].value, "Addons")?;
    let materials = array_length(&fields[3].value, "Materials")?;
    let minimum_headers = if materials > 0 {
        4
    } else if addons > 0 {
        2
    } else if textures > 0 {
        1
    } else {
        0
    };
    let FieldValue::Array(headers) = &mut fields[0].value else {
        return Err(model_info_error("Headers is not an array"));
    };
    while headers.len() < minimum_headers {
        headers.push(FieldValue::UInt(0));
    }
    set_model_info_count(headers, 0, textures)?;
    set_model_info_count(headers, 1, addons)?;
    set_model_info_count(headers, 3, materials)?;
    Ok(updated)
}

fn array_length(value: &FieldValue<'_>, name: &str) -> Result<usize> {
    let FieldValue::Array(values) = value else {
        return Err(model_info_error(format!("{name} is not an array")));
    };
    Ok(values.len())
}

fn callback_integer(value: &FieldValue<'_>, handler: &str) -> Result<i128> {
    match value {
        FieldValue::Int(value) => Ok(i128::from(*value)),
        FieldValue::UInt(value) | FieldValue::Flags { value, .. } => Ok(i128::from(*value)),
        FieldValue::Enumeration { value, .. } => Ok(i128::from(*value)),
        _ => Err(SemanticError::Handler {
            handler: handler.to_owned(),
            message: "callback requires an integer value".to_owned(),
        }),
    }
}

fn set_model_info_count(
    headers: &mut [FieldValue<'static>],
    index: usize,
    count: usize,
) -> Result<()> {
    let Some(header) = headers.get_mut(index) else {
        return Ok(());
    };
    let count = u64::try_from(count)
        .map_err(|_| model_info_error("model-info element count exceeds u64"))?;
    match header {
        FieldValue::Int(value) => {
            *value = i64::try_from(count)
                .map_err(|_| model_info_error("model-info element count exceeds i64"))?;
        }
        FieldValue::UInt(value) | FieldValue::Flags { value, .. } => *value = count,
        FieldValue::Enumeration { value, .. } => {
            *value = i64::try_from(count)
                .map_err(|_| model_info_error("model-info element count exceeds i64"))?;
        }
        _ => return Err(model_info_error("model-info header is not an integer")),
    }
    Ok(())
}

fn model_info_error(message: impl Into<String>) -> SemanticError {
    SemanticError::Handler {
        handler: "edit.model_info_counts".to_owned(),
        message: message.into(),
    }
}

fn model_info_array_error(message: impl Into<String>) -> SemanticError {
    SemanticError::Handler {
        handler: "array.model_info_header_count".to_owned(),
        message: message.into(),
    }
}

fn removable_when_zero(value: &FieldValue<'_>) -> Result<bool> {
    match value {
        FieldValue::Int(value) => Ok(*value == 0),
        FieldValue::UInt(value) => Ok(*value == 0),
        FieldValue::Enumeration { value, .. } => Ok(*value == 0),
        _ => Err(SemanticError::Handler {
            handler: "edit.removable_when_zero".to_owned(),
            message: "removability check requires an integer value".to_owned(),
        }),
    }
}

const fn model_info_conflict_priority(game: SchemaGame, form_version: u16) -> ConflictPriority {
    let uses_modern_model_info = matches!(
        game,
        SchemaGame::SkyrimLe
            | SchemaGame::SkyrimSe
            | SchemaGame::SkyrimVr
            | SchemaGame::Fallout4
            | SchemaGame::Fallout4Vr
            | SchemaGame::Fallout76
            | SchemaGame::Starfield
    );
    if uses_modern_model_info && form_version < 38 {
        ConflictPriority::Ignore
    } else {
        ConflictPriority::Normal
    }
}

fn format_rgb(value: &FieldValue<'_>, include_alpha: bool) -> Result<String> {
    let FieldValue::Struct(components) = value else {
        return Err(SemanticError::Handler {
            handler: "format.rgb".to_owned(),
            message: "RGB formatter requires a struct value".to_owned(),
        });
    };
    let required_components = if include_alpha { 4 } else { 3 };
    if components.len() < required_components {
        return Err(SemanticError::Handler {
            handler: "format.rgb".to_owned(),
            message: format!(
                "RGB formatter requires {required_components} components, got {}",
                components.len()
            ),
        });
    }
    let formatted: Vec<String> = components[..required_components]
        .iter()
        .map(|component| format_color_component(&component.value))
        .collect::<Result<_>>()?;
    Ok(format!(
        "{}({})",
        if include_alpha { "RGBA" } else { "RGB" },
        formatted.join(", ")
    ))
}

fn format_color_component(value: &FieldValue<'_>) -> Result<String> {
    match value {
        FieldValue::Int(value) => Ok(value.to_string()),
        FieldValue::UInt(value) => Ok(value.to_string()),
        FieldValue::Float(value) if value.is_finite() => Ok(value.to_string()),
        _ => Err(SemanticError::Handler {
            handler: "format.rgb".to_owned(),
            message: "RGB components must be finite numeric values".to_owned(),
        }),
    }
}

fn normalize_xedit_radians(value: f64) -> f64 {
    let two_pi = std::f64::consts::TAU;
    let mut result = value;
    if (result / two_pi).abs() > 100.0 {
        result -= result.signum() * two_pi * ((result / two_pi).abs() - 100.0).trunc();
        if (result / two_pi).abs() > 101.0 {
            return f64::NAN;
        }
    }
    while result < 0.0 {
        result += two_pi;
    }
    while result > two_pi {
        result -= two_pi;
    }
    if single_same_value(result, 0.0)
        || result < 0.0
        || single_same_value(result, two_pi)
        || result > two_pi
    {
        0.0
    } else {
        result
    }
}

fn single_same_value(left: f64, right: f64) -> bool {
    const SINGLE_RESOLUTION: f32 = 0.000_000_5;
    let left = left as f32;
    let right = right as f32;
    (left - right).abs() <= (left.abs().min(right.abs()) * SINGLE_RESOLUTION).max(SINGLE_RESOLUTION)
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use super::*;

    struct TestResourceHashResolver;

    impl ResourceHashResolver for TestResourceHashResolver {
        fn resolve_file_hash(&self, hash: u64) -> Option<String> {
            (hash == 0x1234).then(|| "textures/example.dds".to_owned())
        }

        fn resolve_folder_hash(&self, hash: u64) -> Option<String> {
            (hash == 0x5678).then(|| "textures/example".to_owned())
        }
    }

    struct TestWwiseGuidResolver;

    impl WwiseGuidResolver for TestWwiseGuidResolver {
        fn resolve_wwise_guid(&self, guid: [u8; 16]) -> Option<WwiseGuidInfo> {
            (guid == test_wwise_guid()).then(|| {
                WwiseGuidInfo::new(
                    "Play_Test",
                    "\\Events\\Default Work Unit\\Play_Test_With_A_Long_Object_Path_123456789",
                )
            })
        }
    }

    /// Matches xEdit's angle normalization boundaries and large-value guard.
    #[test]
    fn radians_normalizer_matches_xedit_boundaries(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let tau = std::f64::consts::TAU;

        // when
        let negative = normalize_xedit_radians(-0.5);
        let full_turn = normalize_xedit_radians(tau);
        let guarded = normalize_xedit_radians(tau * 202.0);

        // then
        assert!((negative - (tau - 0.5)).abs() < f64::EPSILON);
        assert_eq!(full_turn, 0.0);
        assert_eq!(guarded, 0.0);
        Ok(())
    }

    /// Matches xEdit's `wbModelInfoGetCP` form-version boundary.
    #[test]
    fn model_info_conflict_priority_matches_xedit_boundary() {
        assert_eq!(
            model_info_conflict_priority(SchemaGame::SkyrimSe, 37),
            ConflictPriority::Ignore
        );
        assert_eq!(
            model_info_conflict_priority(SchemaGame::SkyrimSe, 38),
            ConflictPriority::Normal
        );
        assert_eq!(
            model_info_conflict_priority(SchemaGame::Fallout3, 15),
            ConflictPriority::Normal
        );
    }

    /// Matches xEdit's `wbModelInfoUnknownGetCP` empty edit-value behavior.
    #[test]
    fn empty_value_uses_ignored_conflict_priority() {
        assert!(is_empty_value(&FieldValue::Bytes(
            std::borrow::Cow::Borrowed(&[])
        )));
        assert!(!is_empty_value(&FieldValue::Bytes(
            std::borrow::Cow::Borrowed(&[1])
        )));
    }

    /// Matches xEdit's `wbCELLXCLWGetConflictPriority` interior-cell rule.
    #[test]
    fn cell_water_height_ignores_interior_cells() -> Result<()> {
        let binding = CallbackBinding {
            path: "CELL/Water Height".to_owned(),
            callback_id: "def.conflict_priority".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-cell-water-conflict".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "conflict.cell_water_height".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({}),
                },
            },
        };
        let handlers = SemanticHandlerRegistry::builtin();
        let interior = test_cell_record(1, false)?;
        let exterior = test_cell_record(0, false)?;
        let deleted = test_cell_record(1, true)?;

        for (record, expected) in [
            (&interior, ConflictPriority::Ignore),
            (&exterior, ConflictPriority::Normal),
            (&deleted, ConflictPriority::Normal),
        ] {
            let context = HandlerRecordContext::new(
                record.header.signature,
                record.header.form_id,
                record.header.form_version,
                SchemaGame::SkyrimSe,
            );
            let output = handlers.invoke_with_source_record(
                &binding,
                context,
                Some(record),
                HandlerPhase::Conflict,
                None,
                None,
            )?;
            assert!(matches!(
                output,
                HandlerOutput::ConflictPriority(priority) if priority == expected
            ));
        }
        Ok(())
    }

    /// Returns the configured xEdit record-metadata decision exactly.
    #[test]
    fn constant_metadata_boolean_preserves_false() -> Result<()> {
        let binding = CallbackBinding {
            path: "FLST/FormIDs".to_owned(),
            callback_id: "subrecord_array.is_sorted".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-never-sorted".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "metadata.constant_boolean".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({ "value": false }),
                },
            },
        };
        let output = SemanticHandlerRegistry::builtin().invoke(
            &binding,
            HandlerRecordContext::new(Signature(*b"FLST"), FormId::NULL, 0, SchemaGame::SkyrimSe),
            HandlerPhase::RecordMetadata,
            None,
            None,
        )?;
        assert!(matches!(output, HandlerOutput::Boolean(false)));
        Ok(())
    }

    /// Matches xEdit's TES3 grid extraction, FormID, and identity callbacks.
    #[test]
    fn morrowind_cell_metadata_matches_xedit() -> Result<()> {
        let mut exterior_data = 0_u32.to_le_bytes().to_vec();
        exterior_data.extend_from_slice(&(-1_i32).to_le_bytes());
        exterior_data.extend_from_slice(&2_i32.to_le_bytes());
        let exterior = test_record(
            *b"CELL",
            &[(*b"NAME", b"Balmora\0".to_vec()), (*b"DATA", exterior_data)],
        )?;
        let grid_binding = test_metadata_binding(
            "record.grid_cell",
            "metadata.morrowind.grid_cell",
            serde_json::json!({}),
        );
        let form_binding = test_metadata_binding(
            "record.form_id",
            "metadata.morrowind.grid_form_id",
            serde_json::json!({ "base": 160 }),
        );
        let identity_binding = test_metadata_binding(
            "record.identity",
            "metadata.morrowind.grid_identity",
            serde_json::json!({
                "prefix": "<Exterior>",
                "fallback_to_editor_id": true
            }),
        );
        let handlers = SemanticHandlerRegistry::builtin();
        let context =
            HandlerRecordContext::new(Signature(*b"CELL"), FormId::NULL, 0, SchemaGame::Morrowind);

        assert!(matches!(
            handlers.invoke_with_source_record(
                &grid_binding,
                context,
                Some(&exterior),
                HandlerPhase::RecordMetadata,
                None,
                None,
            )?,
            HandlerOutput::GridCell(RecordGridCell { x: -1, y: 2 })
        ));
        assert!(matches!(
            handlers.invoke_with_source_record(
                &form_binding,
                context,
                Some(&exterior),
                HandlerPhase::RecordMetadata,
                None,
                None,
            )?,
            HandlerOutput::FormId(FormId(0x00A7_FE02))
        ));
        assert!(matches!(
            handlers.invoke_with_source_record(
                &identity_binding,
                context,
                Some(&exterior),
                HandlerPhase::RecordMetadata,
                None,
                None,
            )?,
            HandlerOutput::Text(value) if value == "<Exterior>7FFFFFFF|80000002"
        ));

        let mut interior_data = 1_u32.to_le_bytes().to_vec();
        interior_data.extend_from_slice(&10_i32.to_le_bytes());
        interior_data.extend_from_slice(&20_i32.to_le_bytes());
        let interior = test_record(
            *b"CELL",
            &[(*b"NAME", b"Vivec\0".to_vec()), (*b"DATA", interior_data)],
        )?;
        assert!(matches!(
            handlers.invoke_with_source_record(
                &grid_binding,
                context,
                Some(&interior),
                HandlerPhase::RecordMetadata,
                None,
                None,
            )?,
            HandlerOutput::None
        ));
        assert!(matches!(
            handlers.invoke_with_source_record(
                &identity_binding,
                context,
                Some(&interior),
                HandlerPhase::RecordMetadata,
                None,
                None,
            )?,
            HandlerOutput::Text(value) if value == "Vivec"
        ));
        Ok(())
    }

    /// Matches xEdit's TES3 reference and header FormID callbacks.
    #[test]
    fn morrowind_non_grid_form_ids_match_xedit() -> Result<()> {
        let reference = test_record(
            *b"REFR",
            &[(*b"FRMR", 0x0012_3456_u32.to_le_bytes().to_vec())],
        )?;
        let reference_binding = test_metadata_binding(
            "record.form_id",
            "metadata.morrowind.reference_form_id",
            serde_json::json!({}),
        );
        let null_binding = test_metadata_binding(
            "record.form_id",
            "metadata.null_form_id",
            serde_json::json!({}),
        );
        let handlers = SemanticHandlerRegistry::builtin();
        let reference_context =
            HandlerRecordContext::new(Signature(*b"REFR"), FormId::NULL, 0, SchemaGame::Morrowind);

        assert!(matches!(
            handlers.invoke_with_source_record(
                &reference_binding,
                reference_context,
                Some(&reference),
                HandlerPhase::RecordMetadata,
                None,
                None,
            )?,
            HandlerOutput::FormId(FormId(0xFF12_3456))
        ));
        assert!(matches!(
            handlers.invoke(
                &null_binding,
                HandlerRecordContext::new(
                    Signature::TES3,
                    FormId(0xFFFF_FFFF),
                    0,
                    SchemaGame::Morrowind,
                ),
                HandlerPhase::RecordMetadata,
                None,
                None,
            )?,
            HandlerOutput::FormId(FormId::NULL)
        ));
        Ok(())
    }

    /// Reads and updates the fixed SCPT header name used as xEdit's editor ID.
    #[test]
    fn morrowind_script_editor_id_matches_xedit() -> Result<()> {
        let mut header = vec![0_u8; 52];
        header[..11].copy_from_slice(b"HelloWorld\0");
        let record = test_record(*b"SCPT", &[(*b"SCHD", header)])?;
        let binding = test_metadata_binding(
            "record.get_editor_id",
            "metadata.morrowind.script_editor_id",
            serde_json::json!({
                "field_path": "SCPT/0:Script Header/payload/0:Name"
            }),
        );
        let context =
            HandlerRecordContext::new(Signature(*b"SCPT"), FormId::NULL, 0, SchemaGame::Morrowind);
        let handlers = SemanticHandlerRegistry::builtin();

        assert!(matches!(
            handlers.invoke_with_source_record(
                &binding,
                context,
                Some(&record),
                HandlerPhase::RecordMetadata,
                None,
                None,
            )?,
            HandlerOutput::Text(value) if value == "HelloWorld"
        ));
        let new_editor_id = FieldValue::String(Cow::Borrowed("NewScript"));
        assert!(matches!(
            handlers.invoke(
                &binding,
                context,
                HandlerPhase::AfterSet,
                Some(&new_editor_id),
                None,
            )?,
            HandlerOutput::Mutations(mutations)
                if mutations
                    == vec![HandlerMutation::Set {
                        path: "SCPT/0:Script Header/payload/0:Name".to_owned(),
                        occurrence: 0,
                        value: OwnedFieldValue::String("NewScript".to_owned()),
                    }]
        ));
        Ok(())
    }

    /// Builds Starfield's integer and AVMD named indexes exactly like xEdit.
    #[test]
    fn starfield_record_index_keys_match_xedit() -> Result<()> {
        let collision = test_record(*b"COLL", &[(*b"BNAM", 42_u32.to_le_bytes().to_vec())])?;
        let collision_binding = test_metadata_binding(
            "record.index_keys",
            "metadata.integer_index_key",
            serde_json::json!({
                "subrecord_signature": "BNAM",
                "index": "collision_layer"
            }),
        );
        let avmd = test_record(
            *b"AVMD",
            &[
                (*b"MNAM", 2_u32.to_le_bytes().to_vec()),
                (*b"TNAM", b"SurfaceGroup\0".to_vec()),
            ],
        )?;
        let avmd_binding = test_metadata_binding(
            "record.index_keys",
            "metadata.starfield.avmd_index_key",
            serde_json::json!({}),
        );
        let handlers = SemanticHandlerRegistry::builtin();

        assert!(matches!(
            handlers.invoke_with_source_record(
                &collision_binding,
                HandlerRecordContext::new(
                    Signature(*b"COLL"),
                    FormId::NULL,
                    0,
                    SchemaGame::Starfield,
                ),
                Some(&collision),
                HandlerPhase::RecordMetadata,
                None,
                None,
            )?,
            HandlerOutput::IndexKeys(keys)
                if keys
                    == vec![RecordIndexKey {
                        index: "collision_layer".to_owned(),
                        key: "42".to_owned(),
                    }]
        ));
        assert!(matches!(
            handlers.invoke_with_source_record(
                &avmd_binding,
                HandlerRecordContext::new(
                    Signature(*b"AVMD"),
                    FormId::NULL,
                    0,
                    SchemaGame::Starfield,
                ),
                Some(&avmd),
                HandlerPhase::RecordMetadata,
                None,
                None,
            )?,
            HandlerOutput::IndexKeys(keys)
                if keys
                    == vec![RecordIndexKey {
                        index: "complex_group".to_owned(),
                        key: "SurfaceGroup".to_owned(),
                    }]
        ));
        Ok(())
    }

    fn test_metadata_binding(
        callback_id: &str,
        operation: &str,
        configuration: serde_json::Value,
    ) -> CallbackBinding {
        CallbackBinding {
            path: "TEST".to_owned(),
            callback_id: callback_id.to_owned(),
            callback_slot: None,
            implementation_fingerprint: format!("test-{operation}"),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: operation.to_owned(),
                    minimum_version: 1,
                    configuration,
                },
            },
        }
    }

    fn test_record(signature: [u8; 4], subrecords: &[([u8; 4], Vec<u8>)]) -> Result<Record> {
        let data_size: usize = subrecords
            .iter()
            .map(|(_, payload)| 6 + payload.len())
            .sum();
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&signature);
        bytes.extend_from_slice(
            &u32::try_from(data_size)
                .expect("test record data fits in u32")
                .to_le_bytes(),
        );
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        for (subrecord_signature, payload) in subrecords {
            bytes.extend_from_slice(subrecord_signature);
            bytes.extend_from_slice(
                &u16::try_from(payload.len())
                    .expect("test subrecord payload fits in u16")
                    .to_le_bytes(),
            );
            bytes.extend_from_slice(payload);
        }
        let mut cursor = bethkit_io::SliceCursor::new(&bytes);
        Ok(Record::parse_header(
            &mut cursor,
            &bethkit_core::GameContext::sse(),
        )?)
    }

    fn test_cell_record(data_flags: u16, deleted: bool) -> Result<Record> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"CELL");
        bytes.extend_from_slice(&8_u32.to_le_bytes());
        let record_flags = if deleted {
            RecordFlags::DELETED.bits()
        } else {
            0
        };
        bytes.extend_from_slice(&record_flags.to_le_bytes());
        bytes.extend_from_slice(&0x0102_0304_u32.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(&44_u16.to_le_bytes());
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        bytes.extend_from_slice(b"DATA");
        bytes.extend_from_slice(&2_u16.to_le_bytes());
        bytes.extend_from_slice(&data_flags.to_le_bytes());
        let mut cursor = bethkit_io::SliceCursor::new(&bytes);
        Ok(Record::parse_header(
            &mut cursor,
            &bethkit_core::GameContext::sse(),
        )?)
    }

    /// Matches xEdit's `wbRGBAToStr` output without replacing typed values.
    #[test]
    fn rgb_formatter_preserves_rgb_and_rgba_shape() -> Result<()> {
        let component = |name: &str, value: FieldValue<'static>| crate::NamedValue {
            node_id: bethkit_schema::SchemaNodeId(1),
            path: format!("TEST/{name}"),
            name: name.to_owned(),
            span: crate::ByteSpan { start: 0, end: 1 },
            value,
        };
        let rgb = FieldValue::Struct(vec![
            component("Red", FieldValue::UInt(12)),
            component("Green", FieldValue::UInt(34)),
            component("Blue", FieldValue::UInt(56)),
            component(
                "Unused",
                FieldValue::Bytes(std::borrow::Cow::Borrowed(&[0])),
            ),
        ]);
        let rgba = FieldValue::Struct(vec![
            component("Red", FieldValue::Float(12.0)),
            component("Green", FieldValue::Float(34.0)),
            component("Blue", FieldValue::Float(56.0)),
            component("Alpha", FieldValue::Float(78.0)),
        ]);

        assert_eq!(format_rgb(&rgb, false)?, "RGB(12, 34, 56)");
        assert_eq!(format_rgb(&rgba, true)?, "RGBA(12, 34, 56, 78)");
        Ok(())
    }

    /// Matches xEdit's model-info header removal predicate.
    #[test]
    fn zero_header_is_removable() -> Result<()> {
        assert!(removable_when_zero(&FieldValue::UInt(0))?);
        assert!(!removable_when_zero(&FieldValue::UInt(1))?);
        Ok(())
    }

    /// Matches xEdit's 16-digit uppercase fallback for unresolved resource hashes.
    #[test]
    fn resource_hash_fallback_matches_xedit_to_string() -> Result<()> {
        assert_eq!(
            format_resource_hash(None, "file", FieldValue::UInt(0x1234))?,
            "{0000000000001234}"
        );
        assert_eq!(
            format_resource_hash(None, "folder", FieldValue::Int(-1))?,
            "{FFFFFFFFFFFFFFFF}"
        );
        Ok(())
    }

    /// Routes file and folder hashes to the corresponding archive resolver method.
    #[test]
    fn resource_hash_resolver_preserves_file_and_folder_modes() -> Result<()> {
        let resolver: Arc<dyn ResourceHashResolver> = Arc::new(TestResourceHashResolver);
        assert_eq!(
            format_resource_hash(
                Some(Arc::clone(&resolver)),
                "file",
                FieldValue::UInt(0x1234)
            )?,
            "textures/example.dds"
        );
        assert_eq!(
            format_resource_hash(
                Some(Arc::clone(&resolver)),
                "folder",
                FieldValue::UInt(0x5678)
            )?,
            "textures/example"
        );
        assert_eq!(
            format_resource_hash(Some(resolver), "file", FieldValue::UInt(0x5678))?,
            "{0000000000005678}"
        );
        Ok(())
    }

    /// Matches xEdit's summary, sort-key, edit-value, and native-value hash modes.
    #[test]
    fn resource_hash_formatter_respects_callback_phase() -> Result<()> {
        assert_eq!(
            format_resource_hash_in_phase(
                None,
                "file",
                FieldValue::UInt(0x1234),
                HandlerPhase::Summary
            )?,
            "{00001234}"
        );
        assert_eq!(
            format_resource_hash_in_phase(
                None,
                "file",
                FieldValue::UInt(0x1234),
                HandlerPhase::SortKey
            )?,
            "0000000000001234"
        );
        assert_eq!(
            format_resource_hash_in_phase(
                None,
                "file",
                FieldValue::Int(-1),
                HandlerPhase::EditValue
            )?,
            "-1"
        );
        assert_eq!(
            format_resource_hash_in_phase(
                None,
                "file",
                FieldValue::UInt(0x1234),
                HandlerPhase::NativeValue
            )?,
            ""
        );
        Ok(())
    }

    /// Formats Wwise GUIDs using Delphi's mixed-endian canonical representation.
    #[test]
    fn wwise_guid_formatter_matches_xedit_display_modes() -> Result<()> {
        let resolver: Arc<dyn WwiseGuidResolver> = Arc::new(TestWwiseGuidResolver);
        let value = FieldValue::Bytes(std::borrow::Cow::Owned(test_wwise_guid().to_vec()));
        assert_eq!(
            format_wwise_guid(test_wwise_guid()),
            "{00112233-4455-6677-8899-AABBCCDDEEFF}"
        );
        let display = invoke_wwise(Some(Arc::clone(&resolver)), HandlerPhase::Display, &value)?;
        assert!(matches!(
            display,
            HandlerOutput::Text(text)
                if text
                    == "Play_Test {00112233-4455-6677-8899-AABBCCDDEEFF} \
                        \"\\Events\\Default Work Unit\\Play_Test_With_A_Long_Object_Path_123456789\""
        ));
        let summary = invoke_wwise(Some(Arc::clone(&resolver)), HandlerPhase::Summary, &value)?;
        assert!(matches!(
            summary,
            HandlerOutput::Text(text) if text == "Play_Test"
        ));
        let HandlerOutput::Text(edit_value) =
            invoke_wwise(Some(resolver), HandlerPhase::EditValue, &value)?
        else {
            return Err(wwise_guid_error("edit formatter did not return text"));
        };
        let quoted_path = edit_value
            .split('"')
            .nth(1)
            .ok_or_else(|| wwise_guid_error("edit formatter omitted object path"))?;
        assert_eq!(quoted_path.chars().count(), 64);
        assert!(quoted_path.ends_with("..."));
        Ok(())
    }

    /// Extracts canonical GUID text from xEdit's decorated editable value.
    #[test]
    fn wwise_guid_edit_parser_round_trips_binary_value() -> Result<()> {
        let input = FieldValue::String(std::borrow::Cow::Borrowed(
            "Play_Test {00112233-4455-6677-8899-AABBCCDDEEFF} \"\\Events\\Play_Test\"",
        ));
        let output = invoke_wwise(None, HandlerPhase::ParseEditValue, &input)?;
        assert!(matches!(
            output,
            HandlerOutput::Value(FieldValue::Bytes(value))
                if value.as_ref() == test_wwise_guid()
        ));
        assert_eq!(parse_wwise_guid("")?, [0; 16]);
        Ok(())
    }

    /// Mirrors xEdit's model-info header expansion and dependent count updates.
    #[test]
    fn model_info_after_set_updates_only_defined_header_counts() -> Result<()> {
        let field = |name: &str, value: FieldValue<'static>| crate::NamedValue {
            node_id: bethkit_schema::SchemaNodeId(1),
            path: format!("TEST/{name}"),
            name: name.to_owned(),
            span: crate::ByteSpan { start: 0, end: 0 },
            value,
        };
        let model_info = FieldValue::Struct(vec![
            field(
                "Headers",
                FieldValue::Array(vec![FieldValue::UInt(99), FieldValue::UInt(98)]),
            ),
            field(
                "Textures",
                FieldValue::Array(vec![FieldValue::UInt(1), FieldValue::UInt(2)]),
            ),
            field("Addons", FieldValue::Array(vec![FieldValue::UInt(3)])),
            field("Materials", FieldValue::Array(vec![FieldValue::UInt(4)])),
        ]);

        let updated = update_model_info_counts(&model_info)?;

        let FieldValue::Struct(fields) = updated else {
            return Err(model_info_error(
                "model-info handler did not return a struct",
            ));
        };
        let FieldValue::Array(headers) = &fields[0].value else {
            return Err(model_info_error(
                "model-info handler did not preserve headers",
            ));
        };
        assert!(matches!(
            headers.as_slice(),
            [
                FieldValue::UInt(2),
                FieldValue::UInt(1),
                FieldValue::UInt(0),
                FieldValue::UInt(1)
            ]
        ));
        Ok(())
    }

    /// Leaves absent model-info headers absent when all dependent arrays are empty.
    #[test]
    fn model_info_after_set_preserves_empty_header_array() -> Result<()> {
        let field = |name: &str| crate::NamedValue {
            node_id: bethkit_schema::SchemaNodeId(1),
            path: format!("TEST/{name}"),
            name: name.to_owned(),
            span: crate::ByteSpan { start: 0, end: 0 },
            value: FieldValue::Array(Vec::new()),
        };
        let model_info = FieldValue::Struct(vec![
            field("Headers"),
            field("Textures"),
            field("Addons"),
            field("Materials"),
        ]);

        let updated = update_model_info_counts(&model_info)?;

        let FieldValue::Struct(fields) = updated else {
            return Err(model_info_error(
                "model-info handler did not return a struct",
            ));
        };
        assert!(matches!(&fields[0].value, FieldValue::Array(values) if values.is_empty()));
        Ok(())
    }

    /// Mirrors xEdit's CTDA Run On reset without touching unchanged values.
    #[test]
    fn ctda_run_on_clears_reference_only_after_a_relevant_change() -> Result<()> {
        let binding = CallbackBinding {
            path: "TEST/0:CTDA/payload/7:Run On".to_owned(),
            callback_id: "def.after_set".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-ctda-run-on".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "edit.ctda_run_on".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({}),
                },
            },
        };
        let handlers = SemanticHandlerRegistry::builtin();
        let record =
            HandlerRecordContext::new(Signature(*b"TEST"), FormId::NULL, 0, SchemaGame::SkyrimSe);
        let old_reference = FieldValue::Enumeration {
            value: 2,
            name: Some("Reference".to_owned()),
        };
        let new_subject = FieldValue::Int(0);

        let changed = handlers.invoke(
            &binding,
            record,
            HandlerPhase::AfterSet,
            Some(&new_subject),
            Some(&old_reference),
        )?;
        assert!(matches!(
            changed,
            HandlerOutput::Mutations(mutations)
                if matches!(
                    mutations.as_slice(),
                    [HandlerMutation::Set {
                        path,
                        occurrence: 0,
                        value: OwnedFieldValue::UInt(0),
                    }] if path == "TEST/0:CTDA/payload/8:Reference"
                )
        ));

        let unchanged = handlers.invoke(
            &binding,
            record,
            HandlerPhase::AfterSet,
            Some(&new_subject),
            Some(&new_subject),
        )?;
        assert!(matches!(unchanged, HandlerOutput::None));

        let new_reference = FieldValue::UInt(2);
        let reference = handlers.invoke(
            &binding,
            record,
            HandlerPhase::AfterSet,
            Some(&new_reference),
            Some(&new_subject),
        )?;
        assert!(matches!(reference, HandlerOutput::None));
        Ok(())
    }

    /// Preserves the modern and legacy branches of xEdit's CTDA Type update.
    #[test]
    fn ctda_type_updates_global_and_legacy_run_on_state() -> Result<()> {
        let binding = CallbackBinding {
            path: "TEST/0:CTDA/payload/0:Type".to_owned(),
            callback_id: "def.after_set".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-ctda-type".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "edit.ctda_type".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({}),
                },
            },
        };
        let handlers = SemanticHandlerRegistry::builtin();
        let old = FieldValue::UInt(0);
        let modern = FieldValue::UInt(4);
        let modern_output = handlers.invoke(
            &binding,
            HandlerRecordContext::new(Signature(*b"TEST"), FormId::NULL, 0, SchemaGame::SkyrimSe),
            HandlerPhase::AfterSet,
            Some(&modern),
            Some(&old),
        )?;
        assert!(matches!(
            modern_output,
            HandlerOutput::Mutations(mutations)
                if matches!(
                    mutations.as_slice(),
                    [HandlerMutation::Set {
                        path,
                        occurrence: 0,
                        value: OwnedFieldValue::UInt(0),
                    }] if path == "TEST/0:CTDA/payload/2:Comparison Value"
                )
        ));

        let legacy = FieldValue::UInt(6);
        let legacy_output = handlers.invoke(
            &binding,
            HandlerRecordContext::new(Signature(*b"TEST"), FormId::NULL, 0, SchemaGame::Fallout3),
            HandlerPhase::AfterSet,
            Some(&legacy),
            Some(&old),
        )?;
        let HandlerOutput::Mutations(mutations) = legacy_output else {
            return Err(SemanticError::Handler {
                handler: "edit.ctda_type".to_owned(),
                message: "legacy CTDA update did not return mutations".to_owned(),
            });
        };
        assert!(matches!(
            mutations.as_slice(),
            [
                HandlerMutation::Set {
                    path: comparison,
                    occurrence: 0,
                    value: OwnedFieldValue::UInt(0),
                },
                HandlerMutation::Set {
                    path: run_on,
                    occurrence: 0,
                    value: OwnedFieldValue::UInt(1),
                },
                HandlerMutation::Set {
                    path: kind,
                    occurrence: 0,
                    value: OwnedFieldValue::UInt(4),
                }
            ] if comparison == "TEST/0:CTDA/payload/2:Comparison Value"
                && run_on == "TEST/0:CTDA/payload/7:Run On"
                && kind == "TEST/0:CTDA/payload/0:Type"
        ));
        Ok(())
    }

    /// Mirrors xEdit's MESG display-time presence when Message Box changes.
    #[test]
    fn message_flags_toggle_display_time_on_bit_transition() -> Result<()> {
        let binding = CallbackBinding {
            path: "MESG/5:Flags".to_owned(),
            callback_id: "def.after_set".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-message-display-time".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "edit.message_display_time".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({
                        "display_time_path": "MESG/6:Display Time"
                    }),
                },
            },
        };
        let handlers = SemanticHandlerRegistry::builtin();
        let record =
            HandlerRecordContext::new(Signature(*b"MESG"), FormId::NULL, 0, SchemaGame::SkyrimSe);
        let old_message_box = FieldValue::UInt(1);
        let new_notification = FieldValue::UInt(0);

        let added = handlers.invoke(
            &binding,
            record,
            HandlerPhase::AfterSet,
            Some(&new_notification),
            Some(&old_message_box),
        )?;
        assert!(matches!(
            added,
            HandlerOutput::Mutations(mutations)
                if matches!(
                    mutations.as_slice(),
                    [HandlerMutation::SynchronizePresence {
                        path,
                        occurrence: 0,
                        present: true,
                        value: OwnedFieldValue::UInt(0),
                    }] if path == "MESG/6:Display Time"
                )
        ));

        let removed = handlers.invoke(
            &binding,
            record,
            HandlerPhase::AfterSet,
            Some(&old_message_box),
            Some(&new_notification),
        )?;
        assert!(matches!(
            removed,
            HandlerOutput::Mutations(mutations)
                if matches!(
                    mutations.as_slice(),
                    [HandlerMutation::SynchronizePresence {
                        path,
                        occurrence: 0,
                        present: false,
                        value: OwnedFieldValue::UInt(0),
                    }] if path == "MESG/6:Display Time"
                )
        ));

        let unchanged = handlers.invoke(
            &binding,
            record,
            HandlerPhase::AfterSet,
            Some(&FieldValue::UInt(3)),
            Some(&old_message_box),
        )?;
        assert!(matches!(unchanged, HandlerOutput::None));

        let container_replay =
            handlers.invoke(&binding, record, HandlerPhase::AfterSet, None, None)?;
        assert!(matches!(container_replay, HandlerOutput::None));
        Ok(())
    }

    /// Clears FLST entries only when the OrderedList suffix state changes.
    #[test]
    fn form_list_editor_id_clears_entries_on_ordering_transition() -> Result<()> {
        let binding = CallbackBinding {
            path: "FLST/0:Editor ID".to_owned(),
            callback_id: "def.after_set".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-form-list-editor-id".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "edit.form_list_editor_id".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({
                        "entry_path": "FLST/1:FormIDs/repeat/0:FormID"
                    }),
                },
            },
        };
        let handlers = SemanticHandlerRegistry::builtin();
        let record =
            HandlerRecordContext::new(Signature(*b"FLST"), FormId::NULL, 0, SchemaGame::SkyrimSe);
        let unordered = FieldValue::String(std::borrow::Cow::Borrowed("ExampleList"));
        let ordered = FieldValue::String(std::borrow::Cow::Borrowed("MyOrderedList"));

        let cleared = handlers.invoke(
            &binding,
            record,
            HandlerPhase::AfterSet,
            Some(&ordered),
            Some(&unordered),
        )?;
        assert!(matches!(
            cleared,
            HandlerOutput::Mutations(mutations)
                if matches!(
                    mutations.as_slice(),
                    [HandlerMutation::RemoveAll { path }]
                        if path == "FLST/1:FormIDs/repeat/0:FormID"
                )
        ));

        let case_only = handlers.invoke(
            &binding,
            record,
            HandlerPhase::AfterSet,
            Some(&FieldValue::String(std::borrow::Cow::Borrowed(
                "myorderedlist",
            ))),
            Some(&ordered),
        )?;
        assert!(matches!(case_only, HandlerOutput::None));

        let container_replay =
            handlers.invoke(&binding, record, HandlerPhase::AfterSet, None, None)?;
        assert!(matches!(container_replay, HandlerOutput::None));
        Ok(())
    }

    /// Protects a zero MGEF archetype when the second actor-value weight is nonzero.
    #[test]
    fn magic_effect_weight_protects_unset_archetype() -> Result<()> {
        let binding = CallbackBinding {
            path: "MGEF/6:Data/payload/15:Second AV Weight".to_owned(),
            callback_id: "def.after_set".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-mgef-second-av-weight".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "edit.magic_effect_second_av_weight".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({
                        "archetype_path": "MGEF/6:Data/payload/16:Archetype"
                    }),
                },
            },
        };
        let handlers = SemanticHandlerRegistry::builtin();
        let record =
            HandlerRecordContext::new(Signature(*b"MGEF"), FormId::NULL, 0, SchemaGame::Fallout4);

        let protected = handlers.invoke(
            &binding,
            record,
            HandlerPhase::AfterSet,
            Some(&FieldValue::Float(0.25)),
            Some(&FieldValue::Float(0.0)),
        )?;
        assert!(matches!(
            protected,
            HandlerOutput::Mutations(mutations)
                if matches!(
                    mutations.as_slice(),
                    [HandlerMutation::SetIfEqual {
                        path,
                        occurrence: 0,
                        expected: OwnedFieldValue::UInt(0),
                        value: OwnedFieldValue::UInt(0xff),
                    }] if path == "MGEF/6:Data/payload/16:Archetype"
                )
        ));

        let zero = handlers.invoke(
            &binding,
            record,
            HandlerPhase::AfterSet,
            Some(&FieldValue::Float(-0.0)),
            Some(&FieldValue::Float(0.25)),
        )?;
        assert!(matches!(zero, HandlerOutput::None));
        Ok(())
    }

    /// Preserves xEdit's game-specific CTDA type display and edit bit layout.
    #[test]
    fn ctda_type_formatter_round_trips_modern_and_legacy_layouts() -> Result<()> {
        let binding = CallbackBinding {
            path: "TEST/0:CTDA/payload/0:Type".to_owned(),
            callback_id: "integer.formatter".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-ctda-type-formatter".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "format.ctda_type".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({}),
                },
            },
        };
        let handlers = SemanticHandlerRegistry::builtin();
        let modern = FieldValue::UInt(0x75);
        let modern_record =
            HandlerRecordContext::new(Signature(*b"TEST"), FormId::NULL, 0, SchemaGame::SkyrimSe);
        assert!(matches!(
            handlers.invoke(
                &binding,
                modern_record,
                HandlerPhase::Display,
                Some(&modern),
                None,
            )?,
            HandlerOutput::Text(text)
                if text
                    == "Greater than or equal to / Or, Use global, Swap Subject and Target"
        ));
        assert!(matches!(
            handlers.invoke(
                &binding,
                modern_record,
                HandlerPhase::EditValue,
                Some(&modern),
                None,
            )?,
            HandlerOutput::Text(text) if text == "11010101"
        ));
        let modern_edit = FieldValue::String(Cow::Borrowed("11010101"));
        assert!(matches!(
            handlers.invoke(
                &binding,
                modern_record,
                HandlerPhase::ParseEditValue,
                Some(&modern_edit),
                None,
            )?,
            HandlerOutput::Value(FieldValue::UInt(0x75))
        ));

        let legacy = FieldValue::UInt(0xA7);
        let legacy_record =
            HandlerRecordContext::new(Signature(*b"TEST"), FormId::NULL, 0, SchemaGame::Fallout3);
        assert!(matches!(
            handlers.invoke(
                &binding,
                legacy_record,
                HandlerPhase::Display,
                Some(&legacy),
                None,
            )?,
            HandlerOutput::Text(text)
                if text == "Less than or equal to / Or, Run on target, Use global"
        ));
        assert!(matches!(
            handlers.invoke(
                &binding,
                legacy_record,
                HandlerPhase::EditValue,
                Some(&legacy),
                None,
            )?,
            HandlerOutput::Text(text) if text == "101111"
        ));
        let legacy_edit = FieldValue::String(Cow::Borrowed("101111"));
        assert!(matches!(
            handlers.invoke(
                &binding,
                legacy_record,
                HandlerPhase::ParseEditValue,
                Some(&legacy_edit),
                None,
            )?,
            HandlerOutput::Value(FieldValue::UInt(0xA7))
        ));
        Ok(())
    }

    /// Preserves xEdit's runtime integer name table and unknown-value behavior.
    #[test]
    fn integer_lookup_formatter_uses_materialized_edit_values() -> Result<()> {
        let binding = CallbackBinding {
            path: "TEST/value".to_owned(),
            callback_id: "integer.formatter".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-integer-lookup".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "format.integer_lookup".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({
                        "values": [
                            { "value": 7, "name": "GetLucky" },
                            { "value": 42, "name": "GetAnswer" }
                        ],
                        "unknown_display": "angle",
                        "unknown_summary": "decimal",
                        "sort_hex_width": 8
                    }),
                },
            },
        };
        let handlers = SemanticHandlerRegistry::builtin();
        let record =
            HandlerRecordContext::new(Signature(*b"TEST"), FormId::NULL, 0, SchemaGame::SkyrimSe);
        let known = FieldValue::Enumeration {
            value: 42,
            name: Some("GetAnswer".to_owned()),
        };
        for phase in [
            HandlerPhase::Display,
            HandlerPhase::Summary,
            HandlerPhase::EditValue,
        ] {
            assert!(matches!(
                handlers.invoke(&binding, record, phase, Some(&known), None)?,
                HandlerOutput::Text(text) if text == "GetAnswer"
            ));
        }
        let unknown = FieldValue::Int(9);
        assert!(matches!(
            handlers.invoke(
                &binding,
                record,
                HandlerPhase::Display,
                Some(&unknown),
                None,
            )?,
            HandlerOutput::Text(text) if text == "<Unknown: 9>"
        ));
        assert!(matches!(
            handlers.invoke(
                &binding,
                record,
                HandlerPhase::Summary,
                Some(&unknown),
                None,
            )?,
            HandlerOutput::Text(text) if text == "9"
        ));
        assert!(matches!(
            handlers.invoke(
                &binding,
                record,
                HandlerPhase::SortKey,
                Some(&unknown),
                None,
            )?,
            HandlerOutput::Text(text) if text == "00000009"
        ));
        assert!(matches!(
            handlers.invoke(
                &binding,
                record,
                HandlerPhase::NativeValue,
                Some(&unknown),
                None,
            )?,
            HandlerOutput::Text(text) if text.is_empty()
        ));

        let named_edit = FieldValue::String(Cow::Borrowed("getanswer"));
        assert!(matches!(
            handlers.invoke(
                &binding,
                record,
                HandlerPhase::ParseEditValue,
                Some(&named_edit),
                None,
            )?,
            HandlerOutput::Value(FieldValue::Int(42))
        ));
        let numeric_edit = FieldValue::String(Cow::Borrowed("123"));
        assert!(matches!(
            handlers.invoke(
                &binding,
                record,
                HandlerPhase::ParseEditValue,
                Some(&numeric_edit),
                None,
            )?,
            HandlerOutput::Value(FieldValue::Int(123))
        ));
        Ok(())
    }

    /// Converts an array edit into one transactional sibling-counter update.
    #[test]
    fn synchronize_count_handler_tracks_array_length() -> Result<()> {
        let binding = CallbackBinding {
            path: "TEST/1:Values/payload".to_owned(),
            callback_id: "def.after_set".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-sync-count".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "edit.sync_count".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({
                        "counter_path": "TEST/0:Value Count",
                        "counter_required": false
                    }),
                },
            },
        };
        let record =
            HandlerRecordContext::new(Signature(*b"TEST"), FormId::NULL, 0, SchemaGame::SkyrimSe);
        let values = FieldValue::Array(vec![
            FieldValue::UInt(1),
            FieldValue::UInt(2),
            FieldValue::UInt(3),
        ]);

        let output = SemanticHandlerRegistry::builtin().invoke(
            &binding,
            record,
            HandlerPhase::AfterSet,
            Some(&values),
            None,
        )?;

        assert!(matches!(
            output,
            HandlerOutput::Mutations(mutations)
                if matches!(
                    mutations.as_slice(),
                    [HandlerMutation::SynchronizeCount {
                        path,
                        occurrence: 0,
                        value: 3,
                        remove_when_zero: true,
                    }] if path == "TEST/0:Value Count"
                )
        ));
        Ok(())
    }

    /// Writes array lengths to an integer nested inside a sibling subrecord.
    #[test]
    fn synchronize_count_handler_targets_nested_counter_fields() -> Result<()> {
        let binding = CallbackBinding {
            path: "TEST/1:Animations".to_owned(),
            callback_id: "def.after_set".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-nested-counter".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "edit.sync_count".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({
                        "counter_path": "TEST/0:IDLC/payload/0:Animation Count",
                        "counter_required": true,
                        "counter_nested": true
                    }),
                },
            },
        };
        let record =
            HandlerRecordContext::new(Signature(*b"TEST"), FormId::NULL, 0, SchemaGame::Fallout3);
        let values = FieldValue::Array(vec![FieldValue::UInt(1), FieldValue::UInt(2)]);

        let output = SemanticHandlerRegistry::builtin().invoke(
            &binding,
            record,
            HandlerPhase::AfterSet,
            Some(&values),
            None,
        )?;

        assert!(matches!(
            output,
            HandlerOutput::Mutations(mutations)
                if matches!(
                    mutations.as_slice(),
                    [HandlerMutation::Set {
                        path,
                        occurrence: 0,
                        value: OwnedFieldValue::UInt(2),
                    }] if path == "TEST/0:IDLC/payload/0:Animation Count"
                )
        ));
        Ok(())
    }

    /// Matches xEdit's audited no-op when a shared counter callback is
    /// attached in a game whose schema does not define that counter.
    #[test]
    fn synchronize_count_handler_ignores_audited_missing_counter(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let binding = CallbackBinding {
            path: "TEST/0:Values".to_owned(),
            callback_id: "def.after_set".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-missing-counter".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "edit.sync_count".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({
                        "counter_missing": true
                    }),
                },
            },
        };
        let record =
            HandlerRecordContext::new(Signature(*b"TEST"), FormId::NULL, 0, SchemaGame::Starfield);
        let values = FieldValue::Array(vec![FieldValue::UInt(1)]);

        let output = SemanticHandlerRegistry::builtin().invoke(
            &binding,
            record,
            HandlerPhase::AfterSet,
            Some(&values),
            None,
        )?;

        assert!(matches!(output, HandlerOutput::None));
        Ok(())
    }

    /// Counts record values while preserving xEdit's missing-container cleanup behavior.
    #[test]
    fn synchronize_record_counts_handler_reads_transactional_record(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let binding = CallbackBinding {
            path: "TEST".to_owned(),
            callback_id: "def.after_set".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-record-counts".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "edit.sync_record_counts".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({
                        "counters": [
                            {
                                "counter_path": "TEST/0:Keyword Count",
                                "counter_signature": "KSIZ",
                                "counter_required": false,
                                "value_signature": "KWDA",
                                "mode": "u32_payload_count",
                                "only_when_missing": true
                            },
                            {
                                "counter_path": "TEST/2:Condition Count",
                                "counter_signature": "CITC",
                                "counter_required": true,
                                "value_signature": "LVLO",
                                "mode": "subrecord_count"
                            }
                        ]
                    }),
                },
            },
        };
        let record = WritableRecord {
            signature: Signature(*b"TEST"),
            flags: RecordFlags::empty(),
            form_id: FormId::NULL,
            form_version: 0,
            subrecords: vec![
                bethkit_core::WritableSubRecord {
                    signature: Signature(*b"KWDA"),
                    data: vec![0_u8; 12],
                },
                bethkit_core::WritableSubRecord {
                    signature: Signature(*b"CTDA"),
                    data: vec![0_u8; 32],
                },
                bethkit_core::WritableSubRecord {
                    signature: Signature(*b"CTDA"),
                    data: vec![0_u8; 32],
                },
            ],
        };
        let context =
            HandlerRecordContext::new(Signature(*b"TEST"), FormId::NULL, 0, SchemaGame::SkyrimSe);

        let output = SemanticHandlerRegistry::builtin().invoke_with_writable_record(
            &binding,
            context,
            &record,
            HandlerPhase::AfterSet,
            None,
            None,
        )?;

        let HandlerOutput::Mutations(mutations) = output else {
            return Err("record counter handler returned no mutations".into());
        };
        assert!(matches!(
            mutations.as_slice(),
            [HandlerMutation::SynchronizeCount {
                path,
                value: 0,
                remove_when_zero: false,
                ..
            }] if path == "TEST/2:Condition Count"
        ));
        Ok(())
    }

    /// Preserves xEdit's two-step cleanup for an optional container counter.
    #[test]
    fn synchronize_record_counts_handler_clears_counter_before_removal(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let binding = CallbackBinding {
            path: "TEST".to_owned(),
            callback_id: "def.after_set".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-container-cleanup".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "edit.sync_record_counts".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({
                        "counters": [{
                            "counter_path": "TEST/0:Animation Count",
                            "counter_signature": "IDLC",
                            "counter_required": false,
                            "value_signature": "IDLA",
                            "mode": "subrecord_count",
                            "only_when_missing": true,
                            "only_when_counter_exists": true
                        }]
                    }),
                },
            },
        };
        let mut record = WritableRecord {
            signature: Signature(*b"TEST"),
            flags: RecordFlags::empty(),
            form_id: FormId::NULL,
            form_version: 0,
            subrecords: vec![bethkit_core::WritableSubRecord {
                signature: Signature(*b"IDLC"),
                data: vec![3],
            }],
        };
        let context =
            HandlerRecordContext::new(Signature(*b"TEST"), FormId::NULL, 0, SchemaGame::Fallout4);
        let registry = SemanticHandlerRegistry::builtin();

        let output = registry.invoke_with_writable_record(
            &binding,
            context,
            &record,
            HandlerPhase::AfterSet,
            None,
            None,
        )?;

        assert!(matches!(
            output,
            HandlerOutput::Mutations(mutations)
                if matches!(
                    mutations.as_slice(),
                    [HandlerMutation::SynchronizeCount {
                        value: 0,
                        remove_when_zero: false,
                        ..
                    }]
                )
        ));

        record.subrecords[0].data[0] = 0;
        let output = registry.invoke_with_writable_record(
            &binding,
            context,
            &record,
            HandlerPhase::AfterSet,
            None,
            None,
        )?;

        assert!(matches!(
            output,
            HandlerOutput::Mutations(mutations)
                if matches!(
                    mutations.as_slice(),
                    [HandlerMutation::SynchronizeCount {
                        value: 0,
                        remove_when_zero: true,
                        ..
                    }]
                )
        ));
        Ok(())
    }

    /// Reads xEdit model-info array counts from the indexed header slot.
    #[test]
    fn model_info_array_count_reads_header_values() -> Result<()> {
        let binding = CallbackBinding {
            path: "TEST/Textures".to_owned(),
            callback_id: "array.count".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-model-info-count".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "array.model_info_header_count".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({ "header_index": 1 }),
                },
            },
        };
        let record =
            HandlerRecordContext::new(Signature(*b"TEST"), FormId::NULL, 0, SchemaGame::SkyrimSe);
        let payload = FieldValue::Bytes(Cow::Borrowed(&[
            4, 0, 0, 0, 7, 0, 0, 0, 3, 0, 0, 0, 9, 0, 0, 0, 5, 0, 0, 0,
        ]));

        let output = SemanticHandlerRegistry::builtin().invoke(
            &binding,
            record,
            HandlerPhase::ArrayCount,
            Some(&payload),
            None,
        )?;

        assert!(matches!(output, HandlerOutput::Integer(3)));
        Ok(())
    }

    /// Preserves xEdit's exact model-info format validation message.
    #[test]
    fn invalid_model_info_validation_matches_xedit_message() -> Result<()> {
        let binding = CallbackBinding {
            path: "TEST/ERROR".to_owned(),
            callback_id: "def.value_transform".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-invalid-model-info".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "validate.invalid_model_info".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({ "phase": "validation" }),
                },
            },
        };
        let output = InvalidModelInfoValidation.invoke(HandlerInvocation {
            context: HandlerContext {
                binding: &binding,
                record_signature: Signature(*b"TEST"),
                form_id: FormId::NULL,
                form_version: 0,
                game: SchemaGame::SkyrimSe,
                configuration: match &binding.implementation {
                    CallbackImplementation::BuiltIn { operation } => &operation.configuration,
                    _ => unreachable!("test binding is built-in"),
                },
            },
            phase: HandlerPhase::Validation,
            value: None,
            old_value: None,
            source_record: None,
            source_writable_record: None,
        })?;

        assert!(matches!(
            output,
            HandlerOutput::Text(message)
                if message
                    == "SubRecord has invalid format for the Form Version of this record"
        ));
        Ok(())
    }

    fn format_resource_hash(
        resolver: Option<Arc<dyn ResourceHashResolver>>,
        kind: &str,
        value: FieldValue<'static>,
    ) -> Result<String> {
        format_resource_hash_in_phase(resolver, kind, value, HandlerPhase::Display)
    }

    fn format_resource_hash_in_phase(
        resolver: Option<Arc<dyn ResourceHashResolver>>,
        kind: &str,
        value: FieldValue<'static>,
        phase: HandlerPhase,
    ) -> Result<String> {
        let binding = CallbackBinding {
            path: "TEST/Hash".to_owned(),
            callback_id: "integer.formatter".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-resource-hash".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "format.resource_hash".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({ "kind": kind }),
                },
            },
        };
        let output = ResourceHashFormatter { resolver }.invoke(HandlerInvocation {
            context: HandlerContext {
                binding: &binding,
                record_signature: Signature(*b"TEST"),
                form_id: FormId::NULL,
                form_version: 0,
                game: SchemaGame::SkyrimSe,
                configuration: match &binding.implementation {
                    CallbackImplementation::BuiltIn { operation } => &operation.configuration,
                    _ => unreachable!("test binding is built-in"),
                },
            },
            phase,
            value: Some(&value),
            old_value: None,
            source_record: None,
            source_writable_record: None,
        })?;
        match output {
            HandlerOutput::Text(value) => Ok(value),
            _ => Err(SemanticError::Handler {
                handler: "format.resource_hash".to_owned(),
                message: "test formatter returned a non-text result".to_owned(),
            }),
        }
    }

    fn invoke_wwise(
        resolver: Option<Arc<dyn WwiseGuidResolver>>,
        phase: HandlerPhase,
        value: &FieldValue<'static>,
    ) -> Result<HandlerOutput> {
        let binding = CallbackBinding {
            path: "TEST/Guid".to_owned(),
            callback_id: "def.value_transform".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-wwise-guid".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "format.wwise_guid".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({}),
                },
            },
        };
        WwiseGuidFormatter { resolver }.invoke(HandlerInvocation {
            context: HandlerContext {
                binding: &binding,
                record_signature: Signature(*b"TEST"),
                form_id: FormId::NULL,
                form_version: 0,
                game: SchemaGame::Starfield,
                configuration: match &binding.implementation {
                    CallbackImplementation::BuiltIn { operation } => &operation.configuration,
                    _ => unreachable!("test binding is built-in"),
                },
            },
            phase,
            value: Some(value),
            old_value: None,
            source_record: None,
            source_writable_record: None,
        })
    }

    const fn test_wwise_guid() -> [u8; 16] {
        [
            0x33, 0x22, 0x11, 0x00, 0x55, 0x44, 0x77, 0x66, 0x88, 0x99, 0xAA, 0xBB, 0xCC, 0xDD,
            0xEE, 0xFF,
        ]
    }
}
