// SPDX-License-Identifier: Apache-2.0
//!
//! Versioned semantic callback handlers and built-in xEdit operations.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bethkit_core::{FormId, Record, RecordFlags, Signature, WritableRecord};
use bethkit_schema::{
    CallbackBinding, CallbackImplementation, ConditionFunctionTable, ConflictPriority, SchemaGame,
};

use crate::{value::float_from_raw, FieldValue, OwnedFieldValue, Result, SemanticError};

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

pub(crate) fn runs_during_validation(binding: &CallbackBinding) -> bool {
    is_validation_binding(binding)
        || matches!(
            binding.callback_id.as_str(),
            "integer.formatter" | "string.formatter"
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
    /// Dynamic inclusion of the next element in a schema array.
    ArrayElementInclusion,
    /// Schema-native value initialization for a newly created or reset field.
    DefaultValue,
    /// Validation equivalent to xEdit's `ctCheck`.
    Validation,
    /// Transactional normalization performed when a writable record is loaded.
    AfterLoad,
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
    /// Whether the source plugin uses localized string tables.
    pub plugin_localized: bool,
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
            plugin_localized: false,
        }
    }

    /// Marks whether the source plugin uses localized string tables.
    pub const fn with_plugin_localized(mut self, plugin_localized: bool) -> Self {
        self.plugin_localized = plugin_localized;
        self
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
    /// Whether the source plugin uses localized string tables.
    pub plugin_localized: bool,
    /// Deterministic operation configuration from the schema package.
    pub configuration: &'a serde_json::Value,
}

/// One transactional edit requested by a semantic handler.
#[derive(Debug, Clone, PartialEq)]
pub enum HandlerMutation {
    /// Replace main-record flags before initial decoding.
    SetRecordFlags {
        /// Stable record-root path.
        path: String,
        /// Complete replacement flags.
        flags: RecordFlags,
    },
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
    /// Replace one existing subrecord payload with handler-produced bytes.
    ReplacePayload {
        /// Stable subrecord schema path.
        path: String,
        /// Zero-based occurrence.
        occurrence: usize,
        /// Complete replacement payload.
        data: Vec<u8>,
    },
    /// Insert a subrecord with handler-produced bytes before initial decoding.
    InsertPayload {
        /// Stable subrecord schema path.
        path: String,
        /// Complete inserted payload.
        data: Vec<u8>,
    },
    /// Reset a field occurrence to the schema-native default selected in the current edit context.
    ResetToDefault {
        /// Stable schema path.
        path: String,
        /// Zero-based occurrence.
        occurrence: usize,
    },
    /// Insert a subrecord using its schema-native default in the current edit context.
    InsertDefault {
        /// Stable subrecord schema path.
        path: String,
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
    /// Remove every assigned subrecord contained by one structural schema node.
    RemoveContainer {
        /// Stable path of the sequence, repeat, choice, or other container node.
        path: String,
    },
    /// Remove one repeated structural container and its assigned subrecords.
    RemoveContainerOccurrence {
        /// Stable path of the repeated child container.
        path: String,
        /// Zero-based occurrence within the active outer repeat scope.
        occurrence: usize,
    },
    /// Remove every raw subrecord with one signature before initial decoding.
    RemoveAllBySignature {
        /// Stable record path that owns the raw subrecords.
        path: String,
        /// Raw subrecord signature.
        signature: Signature,
    },
    /// Remove the first raw subrecord with one signature before initial decoding.
    RemoveFirstBySignature {
        /// Stable record path that owns the raw subrecords.
        path: String,
        /// Raw subrecord signature.
        signature: Signature,
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
    /// Parsed edit value plus transactional sibling mutations.
    ParsedValue {
        /// Typed value written to the callback's own schema node.
        value: FieldValue<'static>,
        /// Sibling edits produced while parsing the text.
        mutations: Vec<HandlerMutation>,
    },
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
    /// Resolved semantic element link.
    Link(SemanticLink),
    /// Exterior-cell grid coordinates.
    GridCell(RecordGridCell),
    /// Record index keys.
    IndexKeys(Vec<RecordIndexKey>),
    /// Transactional record edits.
    Mutations(Vec<HandlerMutation>),
    /// Replacement bytes for the active top-level subrecord payload.
    SubrecordPayload(Vec<u8>),
}

/// Stable target identity returned by an xEdit link callback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticLink {
    /// One resolved main record.
    Record {
        /// File-local FormID of the target record.
        form_id: FormId,
    },
    /// One effective alias inside a resolved quest record.
    QuestAlias {
        /// File-local FormID of the quest reference used by the source value.
        quest_form_id: FormId,
        /// Numeric alias identifier inside the winning quest definition.
        alias_index: i64,
    },
    /// One nested element inside the source record.
    Element {
        /// Stable schema path of the linked element.
        path: String,
        /// Zero-based enclosing array positions from outermost to innermost.
        array_indices: Vec<usize>,
    },
    /// One nested element inside another resolved main record.
    ExternalElement {
        /// File-local FormID of the containing record.
        record_form_id: FormId,
        /// Stable schema path of the linked element.
        path: String,
        /// Zero-based enclosing array positions from outermost to innermost.
        array_indices: Vec<usize>,
    },
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
    /// Decoded container that owns the value and any sibling fields used by the callback.
    pub value_scope: Option<&'a FieldValue<'static>>,
    /// Original main record for callbacks that inspect sibling subrecords.
    pub source_record: Option<&'a Record>,
    /// Transactional writable record for record-level editor callbacks.
    pub source_writable_record: Option<&'a WritableRecord>,
    /// Top-level subrecord being decoded or encoded, when the callback is payload-local.
    pub source_subrecord_index: Option<usize>,
    /// Zero-based positions of the enclosing schema arrays, from outermost to innermost.
    pub array_indices: &'a [usize],
}

/// Exact top-level subrecord source supplied to a payload-local callback.
pub enum HandlerSubrecordSource<'a> {
    /// Immutable parsed record and the active subrecord index.
    ReadOnly {
        /// Parsed source record.
        record: &'a Record,
        /// Active top-level subrecord index.
        index: usize,
    },
    /// Transactional record and the active subrecord index.
    Writable {
        /// Transactional source record.
        record: &'a WritableRecord,
        /// Active top-level subrecord index.
        index: usize,
    },
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

/// Resolves the next object identifier offered by an xEdit plugin-header editor.
///
/// Implementations normally return the source plugin's highest object identifier plus one.
/// The record context lets a resolver select the source plugin in load-order-aware applications.
pub trait NextObjectIdResolver: Send + Sync {
    /// Returns the next file-local object identifier for the source record.
    fn next_object_id(&self, source: HandlerRecordContext) -> Option<u32>;
}

/// xEdit-compatible presentation metadata for a resolved FormID link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormLinkInfo {
    value: String,
    short_name: String,
    signature: Option<Signature>,
    editor_id: Option<String>,
    quest_aliases: Option<Vec<QuestAliasInfo>>,
    quest_stages: Option<Vec<QuestStageInfo>>,
    quest_objectives: Option<Vec<QuestObjectiveInfo>>,
    script_variables: Option<ScriptVariableMetadata>,
    magic_effect_actor_value: Option<i64>,
    magic_effect_flags: Option<u32>,
    magic_effect_associated_item: Option<i64>,
}

/// Key used to query one of xEdit's named record indexes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordIndexKeyValue {
    /// Case-sensitive text key.
    Text(String),
    /// Signed integer key.
    Integer(i64),
}

/// Resolved record returned from one of xEdit's named indexes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedRecordInfo {
    form_id: FormId,
    link: FormLinkInfo,
}

/// Resolved nested element inside another main record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedElementInfo {
    record_form_id: FormId,
    path: String,
    array_indices: Vec<usize>,
    summary: String,
    containing_record_name: String,
}

/// Resolved navigation-mesh metadata required by xEdit edge callbacks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedNavmeshInfo {
    record_form_id: FormId,
    load_order_form_id: u32,
    name: String,
    triangles_path: String,
    triangle_count: usize,
}

/// Starfield NPC face-entry collection selected through the effective race and gender.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NpcFaceEntryKind {
    /// Chargen face-dial entry keyed by its skin index.
    FaceDial,
    /// Chargen face-morph phenotype keyed by its morph index.
    FaceMorphPhenotype,
}

impl ResolvedNavmeshInfo {
    /// Creates resolved navigation-mesh metadata.
    pub fn new(
        record_form_id: FormId,
        load_order_form_id: u32,
        name: impl Into<String>,
        triangles_path: impl Into<String>,
        triangle_count: usize,
    ) -> Self {
        Self {
            record_form_id,
            load_order_form_id,
            name: name.into(),
            triangles_path: triangles_path.into(),
            triangle_count,
        }
    }

    /// Returns the file-local FormID of the navigation mesh.
    pub const fn record_form_id(&self) -> FormId {
        self.record_form_id
    }

    /// Returns the load-order FormID used by xEdit sort keys.
    pub const fn load_order_form_id(&self) -> u32 {
        self.load_order_form_id
    }

    /// Returns the xEdit name of the navigation mesh.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the stable schema path of the triangle array.
    pub fn triangles_path(&self) -> &str {
        &self.triangles_path
    }

    /// Returns the number of triangles in the effective navigation mesh.
    pub const fn triangle_count(&self) -> usize {
        self.triangle_count
    }
}

impl ResolvedElementInfo {
    /// Creates resolved external-element metadata.
    pub fn new(
        record_form_id: FormId,
        path: impl Into<String>,
        array_indices: Vec<usize>,
        summary: impl Into<String>,
        containing_record_name: impl Into<String>,
    ) -> Self {
        Self {
            record_form_id,
            path: path.into(),
            array_indices,
            summary: summary.into(),
            containing_record_name: containing_record_name.into(),
        }
    }

    /// Returns the file-local FormID of the containing record.
    pub const fn record_form_id(&self) -> FormId {
        self.record_form_id
    }

    /// Returns the stable schema path of the nested target.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Returns the target's enclosing array positions.
    pub fn array_indices(&self) -> &[usize] {
        &self.array_indices
    }

    /// Returns the nested element summary.
    pub fn summary(&self) -> &str {
        &self.summary
    }

    /// Returns the xEdit name of the containing main record.
    pub fn containing_record_name(&self) -> &str {
        &self.containing_record_name
    }
}

impl IndexedRecordInfo {
    /// Creates indexed-record metadata.
    pub const fn new(form_id: FormId, link: FormLinkInfo) -> Self {
        Self { form_id, link }
    }

    /// Returns the file-local FormID of the indexed record.
    pub const fn form_id(&self) -> FormId {
        self.form_id
    }

    /// Returns the record's xEdit-compatible presentation metadata.
    pub const fn link(&self) -> &FormLinkInfo {
        &self.link
    }
}

impl FormLinkInfo {
    /// Creates presentation metadata for one resolved main record.
    pub fn new(value: impl Into<String>, short_name: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            short_name: short_name.into(),
            signature: None,
            editor_id: None,
            quest_aliases: None,
            quest_stages: None,
            quest_objectives: None,
            script_variables: None,
            magic_effect_actor_value: None,
            magic_effect_flags: None,
            magic_effect_associated_item: None,
        }
    }

    /// Adds the exact editor ID used by xEdit's link-dependent callbacks.
    pub fn with_editor_id(mut self, editor_id: impl Into<String>) -> Self {
        self.editor_id = Some(editor_id.into());
        self
    }

    /// Adds the resolved main-record signature used by link-dependent union selectors.
    pub fn with_signature(mut self, signature: Signature) -> Self {
        self.signature = Some(signature);
        self
    }

    /// Marks the resolved record as a quest and supplies its effective aliases.
    pub fn with_quest_aliases(mut self, aliases: Vec<QuestAliasInfo>) -> Self {
        self.quest_aliases = Some(aliases);
        self
    }

    /// Marks the resolved record as a quest and supplies its effective stages.
    pub fn with_quest_stages(mut self, stages: Vec<QuestStageInfo>) -> Self {
        self.quest_stages = Some(stages);
        self
    }

    /// Marks the resolved record as a quest and supplies its effective objectives.
    pub fn with_quest_objectives(mut self, objectives: Vec<QuestObjectiveInfo>) -> Self {
        self.quest_objectives = Some(objectives);
        self
    }

    /// Supplies the effective legacy script state used by condition variable callbacks.
    pub fn with_script_variables(mut self, metadata: ScriptVariableMetadata) -> Self {
        self.script_variables = Some(metadata);
        self
    }

    /// Adds the effective numeric actor value stored by a resolved magic effect.
    pub fn with_magic_effect_actor_value(mut self, actor_value: i64) -> Self {
        self.magic_effect_actor_value = Some(actor_value);
        self
    }

    /// Adds the effective flags and associated item stored by a resolved magic effect.
    pub fn with_magic_effect_metadata(mut self, flags: u32, associated_item: i64) -> Self {
        self.magic_effect_flags = Some(flags);
        self.magic_effect_associated_item = Some(associated_item);
        self
    }

    /// Returns the normal xEdit value text for the linked record.
    pub fn value(&self) -> &str {
        &self.value
    }

    /// Returns the compact xEdit main-record name.
    pub fn short_name(&self) -> &str {
        &self.short_name
    }

    /// Returns the resolved main-record signature when one is available.
    pub fn signature(&self) -> Option<Signature> {
        self.signature
    }

    /// Returns the linked record's editor ID when one is available.
    pub fn editor_id(&self) -> Option<&str> {
        self.editor_id.as_deref()
    }

    /// Returns effective quest aliases, or `None` when the record is not a quest.
    pub fn quest_aliases(&self) -> Option<&[QuestAliasInfo]> {
        self.quest_aliases.as_deref()
    }

    /// Returns effective quest stages, or `None` when the record is not a quest.
    pub fn quest_stages(&self) -> Option<&[QuestStageInfo]> {
        self.quest_stages.as_deref()
    }

    /// Returns effective quest objectives, or `None` when the record is not a quest.
    pub fn quest_objectives(&self) -> Option<&[QuestObjectiveInfo]> {
        self.quest_objectives.as_deref()
    }

    /// Returns effective legacy script metadata when the resolver supplied it.
    pub fn script_variables(&self) -> Option<&ScriptVariableMetadata> {
        self.script_variables.as_ref()
    }

    /// Returns the effective numeric actor value when the record is a magic effect.
    pub fn magic_effect_actor_value(&self) -> Option<i64> {
        self.magic_effect_actor_value
    }

    /// Returns the effective flags when the record is a magic effect.
    pub fn magic_effect_flags(&self) -> Option<u32> {
        self.magic_effect_flags
    }

    /// Returns the effective associated item when the record is a magic effect.
    pub fn magic_effect_associated_item(&self) -> Option<i64> {
        self.magic_effect_associated_item
    }
}

/// xEdit-compatible metadata for one effective quest alias.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestAliasInfo {
    index: i64,
    editor_id: String,
}

impl QuestAliasInfo {
    /// Creates quest-alias metadata.
    pub fn new(index: i64, editor_id: impl Into<String>) -> Self {
        Self {
            index,
            editor_id: editor_id.into(),
        }
    }

    /// Returns the numeric alias identifier.
    pub fn index(&self) -> i64 {
        self.index
    }

    /// Returns the alias editor ID, which may be empty.
    pub fn editor_id(&self) -> &str {
        &self.editor_id
    }
}

/// xEdit-compatible metadata for one effective quest stage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestStageInfo {
    index: i64,
    log_entry: String,
}

impl QuestStageInfo {
    /// Creates quest-stage metadata.
    pub fn new(index: i64, log_entry: impl Into<String>) -> Self {
        Self {
            index,
            log_entry: log_entry.into(),
        }
    }

    /// Returns the numeric stage index.
    pub fn index(&self) -> i64 {
        self.index
    }

    /// Returns the first log-entry text used by xEdit's stage formatter.
    pub fn log_entry(&self) -> &str {
        &self.log_entry
    }
}

/// xEdit-compatible metadata for one effective quest objective.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestObjectiveInfo {
    index: i64,
    display_text: String,
}

impl QuestObjectiveInfo {
    /// Creates quest-objective metadata.
    pub fn new(index: i64, display_text: impl Into<String>) -> Self {
        Self {
            index,
            display_text: display_text.into(),
        }
    }

    /// Returns the numeric objective index.
    pub fn index(&self) -> i64 {
        self.index
    }

    /// Returns the objective display text used by xEdit.
    pub fn display_text(&self) -> &str {
        &self.display_text
    }
}

/// xEdit-compatible metadata for one effective legacy script local variable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptVariableInfo {
    index: i64,
    name: String,
}

impl ScriptVariableInfo {
    /// Creates local-variable metadata.
    pub fn new(index: i64, name: impl Into<String>) -> Self {
        Self {
            index,
            name: name.into(),
        }
    }

    /// Returns the numeric script-local index.
    pub fn index(&self) -> i64 {
        self.index
    }

    /// Returns the script-local name.
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Effective script-reference state used by legacy xEdit condition callbacks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptVariableMetadata {
    /// The resolved main record contains no `SCRI` subrecord.
    MissingReference,
    /// The `SCRI` subrecord does not resolve to a valid script.
    InvalidReference,
    /// The script resolves and exposes its winning local-variable table.
    Resolved {
        /// xEdit name of the effective script record.
        script_name: String,
        /// Effective local variables.
        variables: Vec<ScriptVariableInfo>,
    },
}

impl ScriptVariableMetadata {
    /// Creates metadata for one resolved effective script.
    pub fn resolved(script_name: impl Into<String>, variables: Vec<ScriptVariableInfo>) -> Self {
        Self::Resolved {
            script_name: script_name.into(),
            variables,
        }
    }
}

/// Resolves file-local FormIDs to xEdit-compatible record presentation metadata.
///
/// Implementations are normally scoped to one plugin and its load order so the
/// file-local FormID can be interpreted against the correct master list.
pub trait FormLinkResolver: Send + Sync {
    /// Resolves one FormID and its schema-declared target signatures.
    fn resolve_form_id(
        &self,
        source: HandlerRecordContext,
        form_id: FormId,
        targets: &[Signature],
    ) -> Option<FormLinkInfo>;

    /// Resolves one Oblivion magic-effect record through its four-byte effect code.
    ///
    /// The default returns `None` for resolvers without legacy MGEF-code metadata.
    fn resolve_magic_effect_code(
        &self,
        _source: HandlerRecordContext,
        _code: u32,
    ) -> Option<FormLinkInfo> {
        None
    }

    /// Resolves one record through an xEdit named index.
    ///
    /// The default returns `None` for resolvers that only support FormIDs.
    fn resolve_record_index(
        &self,
        _source: HandlerRecordContext,
        _index: &str,
        _key: &RecordIndexKeyValue,
    ) -> Option<IndexedRecordInfo> {
        None
    }

    /// Resolves one Starfield snap-template node for a source or linked reference.
    ///
    /// `reference_form_id` is `None` when xEdit starts from the source main record.
    /// The default returns `None` for resolvers without snap-template metadata.
    fn resolve_snap_node(
        &self,
        _source: HandlerRecordContext,
        _reference_form_id: Option<FormId>,
        _node_id: i64,
    ) -> Option<ResolvedElementInfo> {
        None
    }

    /// Returns the source record's load-order FormID for deterministic sort keys.
    ///
    /// The default returns `None` for resolvers without load-order identity metadata.
    fn source_load_order_form_id(&self, _source: HandlerRecordContext) -> Option<u32> {
        None
    }

    /// Returns the effective master override's ordered NPC morph keys.
    ///
    /// The default returns `None` for resolvers without override-chain metadata.
    fn source_master_morph_keys(&self, _source: HandlerRecordContext) -> Option<Vec<u32>> {
        None
    }

    /// Returns the source plugin filename used by file-specific xEdit callbacks.
    ///
    /// The default returns `None` for resolvers without source-file metadata.
    fn source_file_name(&self, _source: HandlerRecordContext) -> Option<String> {
        None
    }

    /// Returns the source record's immediate parent group type.
    ///
    /// The default returns `None` for resolvers without plugin group metadata.
    fn source_parent_group_type(&self, _source: HandlerRecordContext) -> Option<u32> {
        None
    }

    /// Resolves one navigation mesh and its effective triangle-array metadata.
    ///
    /// The default returns `None` for resolvers without navigation-mesh metadata.
    fn resolve_navmesh(
        &self,
        _source: HandlerRecordContext,
        _form_id: FormId,
    ) -> Option<ResolvedNavmeshInfo> {
        None
    }

    /// Resolves one Starfield NPC face entry through its effective race and gender.
    ///
    /// The default returns `None` for resolvers without NPC chargen metadata.
    fn resolve_npc_face_entry(
        &self,
        _source: HandlerRecordContext,
        _kind: NpcFaceEntryKind,
        _index: i64,
    ) -> Option<ResolvedElementInfo> {
        None
    }

    /// Resolves the effective quest context inherited by an INFO condition.
    ///
    /// The default returns `None` because resolving INFO parent groups requires
    /// load-order context beyond one parsed record.
    fn resolve_info_condition_quest(
        &self,
        _source: HandlerRecordContext,
        _record: &Record,
    ) -> Option<FormLinkInfo> {
        None
    }

    /// Resolves the inherited quest FormID used by a condition in its record context.
    ///
    /// This covers parent-group relationships that cannot be derived from the
    /// bytes of one main record, including INFO to DIAL and Starfield's parent
    /// quest fallbacks.
    fn resolve_condition_quest_form_id(
        &self,
        _source: HandlerRecordContext,
        _record: &Record,
    ) -> Option<FormId> {
        None
    }
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
    condition_function_table: Option<Arc<ConditionFunctionTable>>,
    form_link_resolver: Option<Arc<dyn FormLinkResolver>>,
    remove_offset_data: bool,
}

#[derive(Clone, Copy)]
enum HandlerRecordSource<'a> {
    None,
    ReadOnly(&'a Record),
    Writable(&'a WritableRecord),
}

pub(crate) struct HandlerInvocationAccess<'a> {
    source: HandlerRecordSource<'a>,
    value_scope: Option<&'a FieldValue<'static>>,
    source_subrecord_index: Option<usize>,
    array_indices: &'a [usize],
}

impl<'a> HandlerInvocationAccess<'a> {
    pub(crate) fn read_only_with_scope(
        record: &'a Record,
        value_scope: Option<&'a FieldValue<'static>>,
    ) -> Self {
        Self {
            source: HandlerRecordSource::ReadOnly(record),
            value_scope,
            source_subrecord_index: None,
            array_indices: &[],
        }
    }

    pub(crate) fn read_only_subrecord_with_scope(
        record: &'a Record,
        index: usize,
        value_scope: Option<&'a FieldValue<'static>>,
    ) -> Self {
        Self {
            source: HandlerRecordSource::ReadOnly(record),
            value_scope,
            source_subrecord_index: Some(index),
            array_indices: &[],
        }
    }

    pub(crate) fn writable_subrecord_with_scope(
        record: &'a WritableRecord,
        index: usize,
        value_scope: Option<&'a FieldValue<'static>>,
    ) -> Self {
        Self {
            source: HandlerRecordSource::Writable(record),
            value_scope,
            source_subrecord_index: Some(index),
            array_indices: &[],
        }
    }

    pub(crate) fn writable_with_scope(
        record: &'a WritableRecord,
        value_scope: Option<&'a FieldValue<'static>>,
    ) -> Self {
        Self {
            source: HandlerRecordSource::Writable(record),
            value_scope,
            source_subrecord_index: None,
            array_indices: &[],
        }
    }

    pub(crate) fn with_array_indices(mut self, array_indices: &'a [usize]) -> Self {
        self.array_indices = array_indices;
        self
    }
}

impl Default for HandlerInvocationAccess<'_> {
    fn default() -> Self {
        Self {
            source: HandlerRecordSource::None,
            value_scope: None,
            source_subrecord_index: None,
            array_indices: &[],
        }
    }
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
        registry.register(Arc::new(FormatVec3));
        registry.register(Arc::new(FormatAngleDegrees));
        registry.register(Arc::new(FormatGeographicCoordinate));
        registry.register(Arc::new(FormatTimestampDate));
        registry.register(Arc::new(FormatScriptSummary));
        registry.register(Arc::new(FormatItemSummary { resolver: None }));
        registry.register(Arc::new(FormatFactionRelation { resolver: None }));
        registry.register(Arc::new(FormatObjectProperty { resolver: None }));
        registry.register(Arc::new(FormatCrowdProperty { resolver: None }));
        registry.register(Arc::new(FormatVmadObjectAlias { resolver: None }));
        registry.register(Arc::new(FormatCtdaQuestStage { resolver: None }));
        registry.register(Arc::new(FormatCtdaVariableName { resolver: None }));
        registry.register(Arc::new(FormatCtdaQuestObjective { resolver: None }));
        registry.register(Arc::new(OverlayCtdaQuest { resolver: None }));
        registry.register(Arc::new(FormatCtdaContextQuestStage { resolver: None }));
        registry.register(Arc::new(FormatCtdaConditionAlias { resolver: None }));
        registry.register(Arc::new(FormatCtdaStringParameter));
        registry.register(Arc::new(ResolveVmadObjectAliasLink { resolver: None }));
        registry.register(Arc::new(ResolveQuestAliasLink { resolver: None }));
        registry.register(Arc::new(ResolveLegendaryFilterMod { resolver: None }));
        registry.register(Arc::new(ResolveNpcFaceEntry { resolver: None }));
        registry.register(Arc::new(FormatLandscapePosition));
        registry.register(Arc::new(FormatClimateMoons));
        registry.register(Arc::new(FormatClimateTime));
        registry.register(Arc::new(FormatAlocTime));
        registry.register(Arc::new(FormatIdleAnimationGroup));
        registry.register(Arc::new(FormatWeatherClassification));
        registry.register(Arc::new(FixedHexIntegerFormatter));
        registry.register(Arc::new(ScaledInt4Formatter));
        registry.register(Arc::new(HideFfffFormatter));
        registry.register(Arc::new(CloudSpeedFormatter));
        registry.register(Arc::new(NextObjectIdFormatter { resolver: None }));
        registry.register(Arc::new(RemovableWhenZero));
        registry.register(Arc::new(ResourceHashFormatter { resolver: None }));
        registry.register(Arc::new(ModelInfoCounts));
        registry.register(Arc::new(ModelInfoArrayCount));
        registry.register(Arc::new(WorldspaceOffsetColumnCount));
        registry.register(Arc::new(OblivionPathGridConnectionCount));
        registry.register(Arc::new(StarSlotArrayElementInclusion));
        registry.register(Arc::new(StarSlotDefaultValue));
        registry.register(Arc::new(SelectCtdaParameter { table: None }));
        registry.register(Arc::new(SelectCoedOwner { resolver: None }));
        registry.register(Arc::new(SelectNoteData));
        registry.register(Arc::new(SelectSoundDescriptorData));
        registry.register(Arc::new(SelectAudioEffectData));
        registry.register(Arc::new(SelectStarfieldComponentData));
        registry.register(Arc::new(SelectStarfieldComponentDat2));
        registry.register(Arc::new(SelectOblivionObmeEfitParameter));
        registry.register(Arc::new(SelectOblivionObmeEfixParameter));
        registry.register(Arc::new(SelectGameSettingValue));
        registry.register(Arc::new(SelectLegacyNoteVoice));
        registry.register(Arc::new(SelectPackageInputValue));
        registry.register(Arc::new(SelectMorrowindGlobalValue));
        registry.register(Arc::new(SelectOblivionMiscActorValue));
        registry.register(Arc::new(SelectPerkEffectData));
        registry.register(Arc::new(SelectPerkEntryPointData));
        registry.register(Arc::new(SelectPerkEpf3));
        registry.register(Arc::new(SelectRecordFlag));
        registry.register(Arc::new(SelectBoneModifierType));
        registry.register(Arc::new(SelectEmptyString));
        registry.register(Arc::new(CtdaFunctionFormatter { table: None }));
        registry.register(Arc::new(FormatCtdaCondition {
            table: None,
            resolver: None,
        }));
        registry.register(Arc::new(FormatBlueprintComponentSummary { resolver: None }));
        registry.register(Arc::new(ResolveBlueprintComponent));
        registry.register(Arc::new(FormatIndexedRecordName { resolver: None }));
        registry.register(Arc::new(ResolveIndexedRecord { resolver: None }));
        registry.register(Arc::new(FormatAvmdEntryReference { resolver: None }));
        registry.register(Arc::new(ResolveAvmdEntryReference { resolver: None }));
        registry.register(Arc::new(FormatSnapNodeSummary { resolver: None }));
        registry.register(Arc::new(ResolveSnapNode { resolver: None }));
        registry.register(Arc::new(ResolveLocalArrayElement));
        registry.register(Arc::new(FormatNavmeshVertex));
        registry.register(Arc::new(FormatNavmeshEdge { resolver: None }));
        registry.register(Arc::new(ResolveNavmeshEdge { resolver: None }));
        registry.register(Arc::new(CtdaRunOnAfterSet));
        registry.register(Arc::new(CtdaTypeAfterSet));
        registry.register(Arc::new(LegacyCtdaAfterLoad));
        registry.register(Arc::new(LegacyEfitAfterLoad { resolver: None }));
        registry.register(Arc::new(VerifyModernEfitAfterLoad));
        registry.register(Arc::new(EmbeddedScriptAfterLoad));
        registry.register(Arc::new(OblivionEfitAfterLoad { resolver: None }));
        registry.register(Arc::new(VerifyOblivionEfixAfterLoad));
        registry.register(Arc::new(RemoveOrphanedKeywordArrayAfterLoad));
        registry.register(Arc::new(VerifyInertBodyTemplateAfterLoad));
        registry.register(Arc::new(MessageAfterLoad));
        registry.register(Arc::new(DefaultObjectArrayAfterLoad));
        registry.register(Arc::new(SkyrimWeaponAfterLoad));
        registry.register(Arc::new(LightAfterLoad));
        registry.register(Arc::new(SkyrimCellAfterLoad));
        registry.register(Arc::new(FalloutCellAfterLoad));
        registry.register(Arc::new(OblivionCellAfterLoad { resolver: None }));
        registry.register(Arc::new(OblivionPathGridAfterLoad));
        registry.register(Arc::new(OblivionInterCellConnectionsAfterLoad));
        registry.register(Arc::new(LegacyEffectShaderAfterLoad));
        registry.register(Arc::new(LegacyFactionAfterLoad));
        registry.register(Arc::new(LegacyWaterAfterLoad));
        registry.register(Arc::new(OblivionReferenceAfterLoad));
        registry.register(Arc::new(OblivionLeveledListAfterLoad));
        registry.register(Arc::new(OblivionMagicEffectAfterLoad { resolver: None }));
        registry.register(Arc::new(LegacyNpcAfterLoad));
        registry.register(Arc::new(LegacyInfoAfterLoad));
        registry.register(Arc::new(LegacySoundAfterLoad));
        registry.register(Arc::new(LegacyWeaponAfterLoad));
        registry.register(Arc::new(LegacyPackageAfterLoad));
        registry.register(Arc::new(FalloutLeveledListAfterLoad));
        registry.register(Arc::new(FalloutNpcAfterLoad { resolver: None }));
        registry.register(Arc::new(LegacyMagicEffectAfterLoad));
        registry.register(Arc::new(SkyrimReferenceAfterLoad));
        registry.register(Arc::new(FalloutReferenceAfterLoad { resolver: None }));
        registry.register(Arc::new(FalloutSceneBehaviorAfterLoad));
        registry.set_remove_offset_data(true);
        registry.register(Arc::new(RegionPointOrderAfterLoad));
        registry.register(Arc::new(MessageDisplayTimeAfterSet));
        registry.register(Arc::new(FormListEditorIdAfterSet));
        registry.register(Arc::new(GameSettingEditorIdAfterSet));
        registry.register(Arc::new(PerkEffectTypeAfterSet));
        registry.register(Arc::new(MagicEffectAssocItemAfterSet));
        registry.register(Arc::new(PackageInputTypeAfterSet));
        registry.register(Arc::new(QuestScriptNameAfterSet));
        registry.register(Arc::new(LegacyPerkEntryPointAfterSet));
        registry.register(Arc::new(LegacyPerkFunctionAfterSet));
        registry.register(Arc::new(LegacyPerkParameterTypeAfterSet));
        registry.register(Arc::new(HeadPartsAfterSet));
        registry.register(Arc::new(MagicEffectSecondAvWeightAfterSet));
        registry.register(Arc::new(MagicEffectArchetypeAfterSet));
        registry.register(Arc::new(ResetSiblingDefault));
        registry.register(Arc::new(RefreshSiblingUnions));
        registry.register(Arc::new(InvalidateConflicts));
        registry.register(Arc::new(CtdaTypeFormatter));
        registry.register(Arc::new(IntegerLookupFormatter));
        registry.register(Arc::new(EventFunctionMemberFormatter));
        registry.register(Arc::new(SynchronizeCountAfterSet));
        registry.register(Arc::new(SynchronizeContainerCountsAfterSet));
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

    /// Installs the source-plugin resolver used by xEdit's `?` object-ID edit value.
    ///
    /// This replaces the built-in formatter while preserving its stable handler ID.
    pub fn set_next_object_id_resolver(&mut self, resolver: Arc<dyn NextObjectIdResolver>) {
        self.register(Arc::new(NextObjectIdFormatter {
            resolver: Some(resolver),
        }));
    }

    /// Selects whether plugin-header OFST data is removed during load.
    ///
    /// The default is `true`, matching xEdit. Setting this to `false` matches
    /// xEdit's `-dontremoveoffsetdata` command-line option.
    pub fn set_remove_offset_data(&mut self, enabled: bool) {
        self.remove_offset_data = enabled;
        self.register(Arc::new(RemoveOffsetDataAfterLoad { enabled }));
        self.register(Arc::new(RemoveWorldspaceOffsetDataAfterLoad { enabled }));
        self.register(Arc::new(WorldspaceAfterLoad {
            remove_offset_data: enabled,
            source_file_load_order: None,
        }));
    }

    pub(crate) fn set_worldspace_source_file_load_order(&mut self, load_order: u32) {
        self.register(Arc::new(WorldspaceAfterLoad {
            remove_offset_data: self.remove_offset_data,
            source_file_load_order: Some(load_order),
        }));
    }

    /// Installs the load-order resolver used by FormID-dependent summaries.
    pub fn set_form_link_resolver(&mut self, resolver: Arc<dyn FormLinkResolver>) {
        self.form_link_resolver = Some(Arc::clone(&resolver));
        self.register(Arc::new(FormatItemSummary {
            resolver: Some(Arc::clone(&resolver)),
        }));
        self.register(Arc::new(FormatFactionRelation {
            resolver: Some(Arc::clone(&resolver)),
        }));
        self.register(Arc::new(FormatObjectProperty {
            resolver: Some(Arc::clone(&resolver)),
        }));
        self.register(Arc::new(FormatCrowdProperty {
            resolver: Some(Arc::clone(&resolver)),
        }));
        self.register(Arc::new(FormatVmadObjectAlias {
            resolver: Some(Arc::clone(&resolver)),
        }));
        self.register(Arc::new(FormatCtdaQuestStage {
            resolver: Some(Arc::clone(&resolver)),
        }));
        self.register(Arc::new(FormatCtdaVariableName {
            resolver: Some(Arc::clone(&resolver)),
        }));
        self.register(Arc::new(FormatCtdaQuestObjective {
            resolver: Some(Arc::clone(&resolver)),
        }));
        self.register(Arc::new(OverlayCtdaQuest {
            resolver: Some(Arc::clone(&resolver)),
        }));
        self.register(Arc::new(FormatCtdaContextQuestStage {
            resolver: Some(Arc::clone(&resolver)),
        }));
        self.register(Arc::new(FormatCtdaConditionAlias {
            resolver: Some(Arc::clone(&resolver)),
        }));
        self.register(Arc::new(ResolveVmadObjectAliasLink {
            resolver: Some(Arc::clone(&resolver)),
        }));
        self.register(Arc::new(ResolveQuestAliasLink {
            resolver: Some(Arc::clone(&resolver)),
        }));
        self.register(Arc::new(ResolveLegendaryFilterMod {
            resolver: Some(Arc::clone(&resolver)),
        }));
        self.register(Arc::new(ResolveNpcFaceEntry {
            resolver: Some(Arc::clone(&resolver)),
        }));
        self.register(Arc::new(LegacyEfitAfterLoad {
            resolver: Some(Arc::clone(&resolver)),
        }));
        self.register(Arc::new(OblivionEfitAfterLoad {
            resolver: Some(Arc::clone(&resolver)),
        }));
        self.register(Arc::new(FalloutReferenceAfterLoad {
            resolver: Some(Arc::clone(&resolver)),
        }));
        self.register(Arc::new(FalloutNpcAfterLoad {
            resolver: Some(Arc::clone(&resolver)),
        }));
        self.register(Arc::new(OblivionMagicEffectAfterLoad {
            resolver: Some(Arc::clone(&resolver)),
        }));
        self.register(Arc::new(OblivionCellAfterLoad {
            resolver: Some(Arc::clone(&resolver)),
        }));
        self.register(Arc::new(SelectCoedOwner {
            resolver: Some(Arc::clone(&resolver)),
        }));
        self.register(Arc::new(FormatCtdaCondition {
            table: self.condition_function_table.clone(),
            resolver: Some(resolver),
        }));
        self.register(Arc::new(FormatBlueprintComponentSummary {
            resolver: self.form_link_resolver.clone(),
        }));
        self.register(Arc::new(FormatIndexedRecordName {
            resolver: self.form_link_resolver.clone(),
        }));
        self.register(Arc::new(ResolveIndexedRecord {
            resolver: self.form_link_resolver.clone(),
        }));
        self.register(Arc::new(FormatAvmdEntryReference {
            resolver: self.form_link_resolver.clone(),
        }));
        self.register(Arc::new(ResolveAvmdEntryReference {
            resolver: self.form_link_resolver.clone(),
        }));
        self.register(Arc::new(FormatSnapNodeSummary {
            resolver: self.form_link_resolver.clone(),
        }));
        self.register(Arc::new(ResolveSnapNode {
            resolver: self.form_link_resolver.clone(),
        }));
        self.register(Arc::new(FormatNavmeshEdge {
            resolver: self.form_link_resolver.clone(),
        }));
        self.register(Arc::new(ResolveNavmeshEdge {
            resolver: self.form_link_resolver.clone(),
        }));
    }

    /// Installs the metadata resolver used by Starfield Wwise GUID callbacks.
    pub fn set_wwise_guid_resolver(&mut self, resolver: Arc<dyn WwiseGuidResolver>) {
        self.register(Arc::new(WwiseGuidFormatter {
            resolver: Some(resolver),
        }));
    }

    /// Installs the package-specific xEdit condition-function table.
    ///
    /// This replaces the table-less built-in selector while preserving its
    /// stable handler identifier and installs the matching function formatter.
    pub fn set_condition_function_table(&mut self, table: Arc<ConditionFunctionTable>) {
        self.condition_function_table = Some(Arc::clone(&table));
        self.register(Arc::new(SelectCtdaParameter {
            table: Some(Arc::clone(&table)),
        }));
        self.register(Arc::new(CtdaFunctionFormatter {
            table: Some(Arc::clone(&table)),
        }));
        self.register(Arc::new(FormatCtdaCondition {
            table: Some(table),
            resolver: self.form_link_resolver.clone(),
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
            HandlerInvocationAccess::default(),
            phase,
            value,
            old_value,
        )
    }

    /// Executes a binding with its decoded sibling-value container.
    ///
    /// # Errors
    ///
    /// Returns [`SemanticError`] when the binding is not executable or the
    /// selected handler rejects the invocation.
    pub fn invoke_with_value_scope<'a>(
        &self,
        binding: &'a CallbackBinding,
        record: HandlerRecordContext,
        phase: HandlerPhase,
        value: Option<&'a FieldValue<'static>>,
        old_value: Option<&'a FieldValue<'static>>,
        value_scope: Option<&'a FieldValue<'static>>,
    ) -> Result<HandlerOutput> {
        self.invoke_with_records(
            binding,
            record,
            HandlerInvocationAccess {
                source: HandlerRecordSource::None,
                value_scope,
                source_subrecord_index: None,
                array_indices: &[],
            },
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
            HandlerInvocationAccess {
                source: source_record
                    .map_or(HandlerRecordSource::None, HandlerRecordSource::ReadOnly),
                value_scope: None,
                source_subrecord_index: None,
                array_indices: &[],
            },
            phase,
            value,
            old_value,
        )
    }

    /// Executes a payload callback with its exact top-level source-subrecord position.
    ///
    /// # Errors
    ///
    /// Returns [`SemanticError`] when the binding is not executable or the
    /// selected handler rejects the invocation.
    pub fn invoke_with_subrecord<'a>(
        &self,
        binding: &'a CallbackBinding,
        record: HandlerRecordContext,
        source: HandlerSubrecordSource<'a>,
        phase: HandlerPhase,
        value: Option<&'a FieldValue<'static>>,
        old_value: Option<&'a FieldValue<'static>>,
    ) -> Result<HandlerOutput> {
        let (source, source_subrecord_index) = match source {
            HandlerSubrecordSource::ReadOnly { record, index } => {
                (HandlerRecordSource::ReadOnly(record), index)
            }
            HandlerSubrecordSource::Writable { record, index } => {
                (HandlerRecordSource::Writable(record), index)
            }
        };
        self.invoke_with_records(
            binding,
            record,
            HandlerInvocationAccess {
                source,
                value_scope: None,
                source_subrecord_index: Some(source_subrecord_index),
                array_indices: &[],
            },
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
            HandlerInvocationAccess {
                source: HandlerRecordSource::Writable(source_record),
                value_scope: None,
                source_subrecord_index: None,
                array_indices: &[],
            },
            phase,
            value,
            old_value,
        )
    }

    pub(crate) fn invoke_with_records<'a>(
        &self,
        binding: &'a CallbackBinding,
        record: HandlerRecordContext,
        access: HandlerInvocationAccess<'a>,
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
                    plugin_localized: record.plugin_localized,
                    configuration,
                },
                phase,
                value,
                old_value,
                value_scope: access.value_scope,
                source_record: match access.source {
                    HandlerRecordSource::ReadOnly(record) => Some(record),
                    HandlerRecordSource::None | HandlerRecordSource::Writable(_) => None,
                },
                source_writable_record: match access.source {
                    HandlerRecordSource::Writable(record) => Some(record),
                    HandlerRecordSource::None | HandlerRecordSource::ReadOnly(_) => None,
                },
                source_subrecord_index: access.source_subrecord_index,
                array_indices: access.array_indices,
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

fn configured_u8_array(
    handler: &str,
    configuration: &serde_json::Value,
    key: &str,
) -> Result<Vec<u8>> {
    let values = configuration
        .get(key)
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| SemanticError::Handler {
            handler: handler.to_owned(),
            message: format!("callback configuration `{key}` must be an array"),
        })?;
    values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            value
                .as_u64()
                .and_then(|value| u8::try_from(value).ok())
                .ok_or_else(|| SemanticError::Handler {
                    handler: handler.to_owned(),
                    message: format!("callback configuration `{key}[{index}]` must be a byte"),
                })
        })
        .collect()
}

fn configured_text_array<'a>(
    handler: &str,
    configuration: &'a serde_json::Value,
    key: &str,
) -> Result<Vec<&'a str>> {
    let values = configuration
        .get(key)
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| SemanticError::Handler {
            handler: handler.to_owned(),
            message: format!("callback configuration `{key}` must be an array"),
        })?;
    values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            value.as_str().ok_or_else(|| SemanticError::Handler {
                handler: handler.to_owned(),
                message: format!("callback configuration `{key}[{index}]` must be text"),
            })
        })
        .collect()
}

fn configured_u8_matrix(
    handler: &str,
    configuration: &serde_json::Value,
    key: &str,
    width: usize,
) -> Result<Vec<Vec<u8>>> {
    let rows = configuration
        .get(key)
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| SemanticError::Handler {
            handler: handler.to_owned(),
            message: format!("callback configuration `{key}` must be an array"),
        })?;
    rows.iter()
        .enumerate()
        .map(|(row_index, row)| {
            let values = row
                .as_array()
                .filter(|values| values.len() == width)
                .ok_or_else(|| SemanticError::Handler {
                    handler: handler.to_owned(),
                    message: format!(
                        "callback configuration `{key}[{row_index}]` must contain {width} bytes"
                    ),
                })?;
            values
                .iter()
                .enumerate()
                .map(|(column_index, value)| {
                    value
                        .as_u64()
                        .and_then(|value| u8::try_from(value).ok())
                        .ok_or_else(|| SemanticError::Handler {
                            handler: handler.to_owned(),
                            message: format!(
                                "callback configuration `{key}[{row_index}][{column_index}]` \
                                 must be a byte"
                            ),
                        })
                })
                .collect()
        })
        .collect()
}

fn configured_optional_text<'a>(
    handler: &str,
    configuration: &'a serde_json::Value,
    key: &str,
) -> Result<Option<&'a str>> {
    match configuration.get(key) {
        None => Ok(None),
        Some(value) => value
            .as_str()
            .map(Some)
            .ok_or_else(|| SemanticError::Handler {
                handler: handler.to_owned(),
                message: format!("callback configuration `{key}` must be a string"),
            }),
    }
}

fn configured_optional_bool(
    handler: &str,
    configuration: &serde_json::Value,
    key: &str,
) -> Result<Option<bool>> {
    match configuration.get(key) {
        None => Ok(None),
        Some(value) => value
            .as_bool()
            .map(Some)
            .ok_or_else(|| SemanticError::Handler {
                handler: handler.to_owned(),
                message: format!("callback configuration `{key}` must be a boolean"),
            }),
    }
}

fn configured_signature(
    handler: &str,
    configuration: &serde_json::Value,
    key: &str,
) -> Result<Signature> {
    let value = configured_text(handler, configuration, key)?;
    parse_configured_signature(handler, key, value)
}

fn parse_configured_signature(handler: &str, key: &str, value: &str) -> Result<Signature> {
    let bytes: [u8; 4] = value
        .as_bytes()
        .try_into()
        .map_err(|_| SemanticError::Handler {
            handler: handler.to_owned(),
            message: format!("callback configuration `{key}` must be four ASCII bytes"),
        })?;
    if !bytes.iter().all(u8::is_ascii) {
        return Err(SemanticError::Handler {
            handler: handler.to_owned(),
            message: format!("callback configuration `{key}` must be four ASCII bytes"),
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

fn configured_i64(handler: &str, configuration: &serde_json::Value, key: &str) -> Result<i64> {
    configuration
        .get(key)
        .and_then(serde_json::Value::as_i64)
        .ok_or_else(|| SemanticError::Handler {
            handler: handler.to_owned(),
            message: format!("configuration key {key} is not a signed integer"),
        })
}

fn configured_u64(handler: &str, configuration: &serde_json::Value, key: &str) -> Result<u64> {
    configuration
        .get(key)
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| SemanticError::Handler {
            handler: handler.to_owned(),
            message: format!("configuration key {key} is not an unsigned integer"),
        })
}

fn configured_bool(handler: &str, configuration: &serde_json::Value, key: &str) -> Result<bool> {
    configuration
        .get(key)
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| SemanticError::Handler {
            handler: handler.to_owned(),
            message: format!("configuration key {key} is not a boolean"),
        })
}

fn configured_optional_digits(
    handler: &str,
    configuration: &serde_json::Value,
    key: &str,
) -> Result<Option<usize>> {
    match configuration.get(key) {
        None => Ok(None),
        Some(value) => value
            .as_u64()
            .and_then(|digits| usize::try_from(digits).ok())
            .filter(|digits| *digits <= 19)
            .map(Some)
            .ok_or_else(|| SemanticError::Handler {
                handler: handler.to_owned(),
                message: format!("callback configuration `{key}` must be between 0 and 19"),
            }),
    }
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
        if invocation.phase != HandlerPhase::Summary {
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
        let digits =
            configured_optional_digits(self.id(), invocation.context.configuration, "digits")?;
        let alpha_digits = configured_optional_digits(
            self.id(),
            invocation.context.configuration,
            "alpha_digits",
        )?;
        let value = invocation.value.ok_or_else(|| SemanticError::Handler {
            handler: self.id().to_owned(),
            message: "RGB formatter requires a value".to_owned(),
        })?;
        Ok(HandlerOutput::Text(format_rgb(
            value,
            include_alpha,
            digits,
            alpha_digits,
        )?))
    }
}

struct FormatVec3;

impl SemanticHandler for FormatVec3 {
    fn id(&self) -> &'static str {
        "format.vec3"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::Summary {
            return Ok(HandlerOutput::None);
        }
        let digits =
            configured_optional_digits(self.id(), invocation.context.configuration, "digits")?;
        let value = invocation.value.ok_or_else(|| SemanticError::Handler {
            handler: self.id().to_owned(),
            message: "Vec3 formatter requires a value".to_owned(),
        })?;
        Ok(HandlerOutput::Text(format_vec3(value, digits)?))
    }
}

struct FormatAngleDegrees;

impl SemanticHandler for FormatAngleDegrees {
    fn id(&self) -> &'static str {
        "format.angle_degrees"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase == HandlerPhase::ParseEditValue {
            let Some(FieldValue::String(value)) = invocation.value else {
                return Err(SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: "angle edit parsing requires text".to_owned(),
                });
            };
            return Ok(HandlerOutput::Value(FieldValue::Float(
                parse_angle_degrees(value)?,
            )));
        }
        let Some(FieldValue::Float(value)) = invocation.value else {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "angle formatter requires a floating-point value".to_owned(),
            });
        };
        if matches!(
            invocation.phase,
            HandlerPhase::Display | HandlerPhase::Summary
        ) {
            return Ok(HandlerOutput::Text(format_angle_degrees(*value)));
        }
        Ok(HandlerOutput::Text(format_numeric_component(
            self.id(),
            invocation.value.expect("float value checked above"),
            None,
        )?))
    }
}

struct FormatTimestampDate;

impl SemanticHandler for FormatTimestampDate {
    fn id(&self) -> &'static str {
        "format.timestamp_date"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase == HandlerPhase::ParseEditValue {
            let Some(FieldValue::String(value)) = invocation.value else {
                return Err(timestamp_date_error(
                    "timestamp edit parsing requires hexadecimal text",
                ));
            };
            return Ok(HandlerOutput::Value(FieldValue::Bytes(
                std::borrow::Cow::Owned(parse_fixed_hex_bytes(value, 2)?),
            )));
        }
        let Some(FieldValue::Bytes(value)) = invocation.value else {
            return Err(timestamp_date_error(
                "timestamp formatter requires a byte-array value",
            ));
        };
        if value.len() != 2 {
            return Err(timestamp_date_error(
                "timestamp formatter requires exactly two bytes",
            ));
        }
        if matches!(
            invocation.phase,
            HandlerPhase::Display | HandlerPhase::Summary
        ) {
            return Ok(HandlerOutput::Text(format_timestamp_date(value)));
        }
        Ok(HandlerOutput::Text(format_hex_bytes(value)))
    }
}

struct FormatGeographicCoordinate;

impl SemanticHandler for FormatGeographicCoordinate {
    fn id(&self) -> &'static str {
        "format.geographic_coordinate"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        let latitude = coordinate_is_latitude(invocation.context.binding)?;
        if invocation.phase == HandlerPhase::ParseEditValue {
            let Some(FieldValue::String(value)) = invocation.value else {
                return Err(coordinate_error("coordinate edit parsing requires text"));
            };
            return Ok(HandlerOutput::Value(FieldValue::Float(
                parse_geographic_coordinate(value, latitude)?,
            )));
        }
        let Some(FieldValue::Float(value)) = invocation.value else {
            return Err(coordinate_error(
                "coordinate formatter requires a floating-point value",
            ));
        };
        if matches!(
            invocation.phase,
            HandlerPhase::Display | HandlerPhase::Summary
        ) {
            return Ok(HandlerOutput::Text(format_geographic_coordinate(
                *value, latitude,
            )));
        }
        Ok(HandlerOutput::Text(format_numeric_component(
            self.id(),
            invocation.value.expect("float value checked above"),
            None,
        )?))
    }
}

struct FormatScriptSummary;

impl SemanticHandler for FormatScriptSummary {
    fn id(&self) -> &'static str {
        "format.script_summary"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::Summary {
            return Ok(HandlerOutput::None);
        }
        let value = invocation.value.ok_or_else(|| SemanticError::Handler {
            handler: self.id().to_owned(),
            message: "script summary requires a value".to_owned(),
        })?;
        Ok(HandlerOutput::Text(format_script_summary(value)?))
    }
}

struct FormatItemSummary {
    resolver: Option<Arc<dyn FormLinkResolver>>,
}

impl SemanticHandler for FormatItemSummary {
    fn id(&self) -> &'static str {
        "format.item_summary"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::Summary {
            return Ok(HandlerOutput::None);
        }
        let value = invocation.value.ok_or_else(|| SemanticError::Handler {
            handler: self.id().to_owned(),
            message: "item summary requires a value".to_owned(),
        })?;
        let Some(text) = format_item_summary(
            value,
            self.resolver.as_deref(),
            handler_record_context(&invocation.context),
        )?
        else {
            return Ok(HandlerOutput::None);
        };
        Ok(HandlerOutput::Text(text))
    }
}

struct FormatFactionRelation {
    resolver: Option<Arc<dyn FormLinkResolver>>,
}

impl SemanticHandler for FormatFactionRelation {
    fn id(&self) -> &'static str {
        "format.faction_relation"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::Summary {
            return Ok(HandlerOutput::None);
        }
        let value = invocation.value.ok_or_else(|| SemanticError::Handler {
            handler: self.id().to_owned(),
            message: "faction relation summary requires a value".to_owned(),
        })?;
        let Some(text) = format_faction_relation(
            value,
            self.resolver.as_deref(),
            handler_record_context(&invocation.context),
        )?
        else {
            return Ok(HandlerOutput::None);
        };
        Ok(HandlerOutput::Text(text))
    }
}

struct FormatObjectProperty {
    resolver: Option<Arc<dyn FormLinkResolver>>,
}

struct FormatCrowdProperty {
    resolver: Option<Arc<dyn FormLinkResolver>>,
}

impl SemanticHandler for FormatObjectProperty {
    fn id(&self) -> &'static str {
        "format.object_property"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::Summary {
            return Ok(HandlerOutput::None);
        }
        let value = invocation.value.ok_or_else(|| SemanticError::Handler {
            handler: self.id().to_owned(),
            message: "object property summary requires a value".to_owned(),
        })?;
        let Some(text) = format_object_property(
            value,
            self.resolver.as_deref(),
            handler_record_context(&invocation.context),
        )?
        else {
            return Ok(HandlerOutput::None);
        };
        Ok(HandlerOutput::Text(text))
    }
}

impl SemanticHandler for FormatCrowdProperty {
    fn id(&self) -> &'static str {
        "format.crowd_property"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::Summary {
            return Ok(HandlerOutput::None);
        }
        let value = invocation.value.ok_or_else(|| SemanticError::Handler {
            handler: self.id().to_owned(),
            message: "crowd property summary requires a value".to_owned(),
        })?;
        let Some(text) = format_linked_float_property(
            value,
            self.resolver.as_deref(),
            handler_record_context(&invocation.context),
            self.id(),
        )?
        else {
            return Ok(HandlerOutput::None);
        };
        Ok(HandlerOutput::Text(text))
    }
}

struct FormatVmadObjectAlias {
    resolver: Option<Arc<dyn FormLinkResolver>>,
}

struct ResolveVmadObjectAliasLink {
    resolver: Option<Arc<dyn FormLinkResolver>>,
}

struct ResolveQuestAliasLink {
    resolver: Option<Arc<dyn FormLinkResolver>>,
}

struct ResolveLegendaryFilterMod {
    resolver: Option<Arc<dyn FormLinkResolver>>,
}

struct ResolveNpcFaceEntry {
    resolver: Option<Arc<dyn FormLinkResolver>>,
}

struct FormatCtdaQuestStage {
    resolver: Option<Arc<dyn FormLinkResolver>>,
}

struct FormatCtdaVariableName {
    resolver: Option<Arc<dyn FormLinkResolver>>,
}

struct FormatCtdaQuestObjective {
    resolver: Option<Arc<dyn FormLinkResolver>>,
}

struct OverlayCtdaQuest {
    resolver: Option<Arc<dyn FormLinkResolver>>,
}

struct FormatCtdaContextQuestStage {
    resolver: Option<Arc<dyn FormLinkResolver>>,
}

struct FormatCtdaConditionAlias {
    resolver: Option<Arc<dyn FormLinkResolver>>,
}

struct FormatCtdaStringParameter;

impl SemanticHandler for FormatCtdaStringParameter {
    fn id(&self) -> &'static str {
        "format.ctda_string_parameter"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        let string_path =
            configured_text(self.id(), invocation.context.configuration, "string_path")?;
        if invocation.phase == HandlerPhase::ParseEditValue {
            let Some(FieldValue::String(value)) = invocation.value else {
                return Err(ctda_string_parameter_error(
                    "condition string edit parsing requires text",
                ));
            };
            return Ok(HandlerOutput::ParsedValue {
                value: FieldValue::UInt(0),
                mutations: vec![HandlerMutation::SynchronizePresence {
                    path: string_path.to_owned(),
                    occurrence: 0,
                    present: true,
                    value: OwnedFieldValue::String(value.to_string()),
                }],
            });
        }
        let raw = callback_integer(
            invocation.value.ok_or_else(|| {
                ctda_string_parameter_error(
                    "condition string parameter formatting requires an integer",
                )
            })?,
            self.id(),
        )?;
        let text = match invocation.phase {
            HandlerPhase::Display
            | HandlerPhase::Summary
            | HandlerPhase::SortKey
            | HandlerPhase::EditValue
            | HandlerPhase::NativeValue => invocation
                .value_scope
                .and_then(|scope| scoped_string(scope, string_path))
                .unwrap_or_default()
                .to_owned(),
            HandlerPhase::Validation | HandlerPhase::ReferenceResolution => String::new(),
            _ => raw.to_string(),
        };
        Ok(HandlerOutput::Text(text))
    }
}

impl SemanticHandler for FormatCtdaConditionAlias {
    fn id(&self) -> &'static str {
        "format.ctda_condition_alias"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase == HandlerPhase::ParseEditValue {
            let Some(FieldValue::String(value)) = invocation.value else {
                return Err(ctda_condition_alias_error(
                    "condition alias edit parsing requires text",
                ));
            };
            return Ok(HandlerOutput::Value(FieldValue::Int(parse_vmad_alias(
                value,
                invocation.context.game,
            ))));
        }
        let raw = i64::try_from(callback_integer(
            invocation.value.ok_or_else(|| {
                ctda_condition_alias_error("condition alias formatting requires an integer")
            })?,
            self.id(),
        )?)
        .map_err(|_| ctda_condition_alias_error("condition alias exceeds i64"))?;
        if invocation.phase == HandlerPhase::SortKey {
            return Ok(HandlerOutput::Text(format!("{:08X}", raw as u64)));
        }
        let source = handler_record_context(&invocation.context);
        let Some(record) = invocation.source_record else {
            return Ok(HandlerOutput::Text(String::new()));
        };
        let resolver = self.resolver.as_deref();
        let quest = if invocation.context.record_signature == Signature(*b"QUST") {
            resolver.and_then(|resolver| {
                resolver.resolve_form_id(source, invocation.context.form_id, &[Signature(*b"QUST")])
            })
        } else if invocation.context.record_signature == Signature(*b"SCEN") {
            resolve_condition_quest_subrecord(record, Signature(*b"PNAM"), source, resolver)?
        } else if invocation.context.record_signature == Signature(*b"PACK") {
            resolve_condition_quest_subrecord(record, Signature(*b"QNAM"), source, resolver)?
        } else if invocation.context.record_signature == Signature(*b"INFO") {
            resolver.and_then(|resolver| resolver.resolve_info_condition_quest(source, record))
        } else {
            return Ok(HandlerOutput::Text(format_unresolved_condition_alias(
                raw,
                invocation.phase,
            )));
        };
        if invocation.context.record_signature == Signature(*b"INFO") && quest.is_none() {
            return Ok(HandlerOutput::Text(String::new()));
        }
        Ok(HandlerOutput::Text(format_resolved_quest_alias(
            raw,
            invocation.phase,
            invocation.context.game,
            quest.as_ref(),
        )))
    }
}

impl SemanticHandler for FormatCtdaQuestStage {
    fn id(&self) -> &'static str {
        "format.ctda_quest_stage"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase == HandlerPhase::ParseEditValue {
            let Some(FieldValue::String(value)) = invocation.value else {
                return Err(ctda_quest_stage_error(
                    "condition quest-stage edit parsing requires text",
                ));
            };
            return Ok(HandlerOutput::Value(FieldValue::Int(parse_prefixed_i32(
                value,
            )?)));
        }
        let stage = i64::try_from(callback_integer(
            invocation.value.ok_or_else(|| {
                ctda_quest_stage_error("condition quest-stage formatting requires an integer")
            })?,
            self.id(),
        )?)
        .map_err(|_| ctda_quest_stage_error("condition quest stage exceeds i64"))?;
        let quest_path =
            configured_text(self.id(), invocation.context.configuration, "quest_path")?;
        let quest = invocation
            .value_scope
            .and_then(|scope| scoped_form_id(scope, quest_path))
            .and_then(|(form_id, targets)| {
                self.resolver.as_deref().and_then(|resolver| {
                    resolver.resolve_form_id(
                        handler_record_context(&invocation.context),
                        form_id,
                        targets,
                    )
                })
            });
        Ok(HandlerOutput::Text(format_ctda_quest_stage(
            stage,
            invocation.phase,
            quest.as_ref(),
        )))
    }
}

impl SemanticHandler for FormatCtdaVariableName {
    fn id(&self) -> &'static str {
        "format.ctda_variable_name"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase == HandlerPhase::ParseEditValue {
            let Some(FieldValue::String(input)) = invocation.value else {
                return Err(ctda_variable_name_error(
                    "condition variable edit parsing requires text",
                ));
            };
            if let Ok(value) = parse_delphi_integer(input, self.id()) {
                if value != 0 {
                    return Ok(HandlerOutput::Value(FieldValue::Int(value)));
                }
            }
            let record =
                resolve_ctda_parameter_link(&invocation, self.resolver.as_deref(), self.id())?
                    .ok_or_else(|| {
                        ctda_variable_name_error(
                            "\"Parameter #1\" does not reference a valid main record",
                        )
                    })?;
            let metadata = record.script_variables().ok_or_else(|| {
                ctda_variable_name_error(
                    "FormID resolver did not supply legacy script-variable metadata",
                )
            })?;
            let variables = match metadata {
                ScriptVariableMetadata::MissingReference => {
                    return Err(ctda_variable_name_error(format!(
                        "\"{}\" does not contain a SCRI subrecord",
                        record.short_name()
                    )))
                }
                ScriptVariableMetadata::InvalidReference => {
                    return Err(ctda_variable_name_error(format!(
                        "\"{}\" does not have a valid script",
                        record.short_name()
                    )))
                }
                ScriptVariableMetadata::Resolved { variables, .. } => variables,
            };
            let name = input.trim();
            let variable = variables
                .iter()
                .find(|variable| variable.name().eq_ignore_ascii_case(name))
                .ok_or_else(|| {
                    ctda_variable_name_error(format!(
                        "Variable \"{input}\" was not found in \"{}\"",
                        record.short_name()
                    ))
                })?;
            return Ok(HandlerOutput::Value(FieldValue::Int(variable.index())));
        }
        let value = i64::try_from(callback_integer(
            invocation.value.ok_or_else(|| {
                ctda_variable_name_error("condition variable formatting requires an integer")
            })?,
            self.id(),
        )?)
        .map_err(|_| ctda_variable_name_error("condition variable index exceeds i64"))?;
        if invocation.phase == HandlerPhase::SortKey {
            return Ok(HandlerOutput::Text(format!("{:08X}", value as u64)));
        }
        let record = resolve_ctda_parameter_link(&invocation, self.resolver.as_deref(), self.id())?;
        Ok(HandlerOutput::Text(format_ctda_variable_name(
            value,
            invocation.phase,
            record.as_ref(),
        )?))
    }
}

impl SemanticHandler for FormatCtdaQuestObjective {
    fn id(&self) -> &'static str {
        "format.ctda_quest_objective"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase == HandlerPhase::ParseEditValue {
            let Some(FieldValue::String(input)) = invocation.value else {
                return Err(ctda_quest_objective_error(
                    "condition quest-objective edit parsing requires text",
                ));
            };
            let value = parse_prefixed_u32(input, self.id())?;
            return Ok(HandlerOutput::Value(FieldValue::Int(i64::from(value))));
        }
        let value = i64::try_from(callback_integer(
            invocation.value.ok_or_else(|| {
                ctda_quest_objective_error(
                    "condition quest-objective formatting requires an integer",
                )
            })?,
            self.id(),
        )?)
        .map_err(|_| ctda_quest_objective_error("condition quest objective exceeds i64"))?;
        if invocation.phase == HandlerPhase::SortKey {
            return Ok(HandlerOutput::Text(format!("{:08X}", value as u64)));
        }
        let quest = resolve_ctda_parameter_link(&invocation, self.resolver.as_deref(), self.id())?;
        Ok(HandlerOutput::Text(format_ctda_quest_objective(
            value,
            invocation.phase,
            quest.as_ref(),
        )))
    }
}

impl SemanticHandler for OverlayCtdaQuest {
    fn id(&self) -> &'static str {
        "overlay.ctda_quest"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        let raw = callback_form_id(
            invocation.value.ok_or_else(|| {
                ctda_condition_quest_error(self.id(), "quest overlay requires a FormID")
            })?,
            self.id(),
        )?;
        if raw != FormId::NULL
            || !matches!(
                invocation.phase,
                HandlerPhase::Display
                    | HandlerPhase::Summary
                    | HandlerPhase::SortKey
                    | HandlerPhase::Validation
                    | HandlerPhase::ReferenceResolution
            )
        {
            return Ok(HandlerOutput::None);
        }
        let Some(quest_form_id) =
            resolve_ctda_condition_quest(self.id(), &invocation, self.resolver.as_deref())?
        else {
            return Ok(HandlerOutput::None);
        };
        let source = handler_record_context(&invocation.context);
        let targets = [Signature(*b"QUST")];
        let resolved = self
            .resolver
            .as_deref()
            .and_then(|resolver| resolver.resolve_form_id(source, quest_form_id, &targets));
        match invocation.phase {
            HandlerPhase::Display => Ok(resolved.map_or_else(
                || HandlerOutput::Value(quest_form_id_value(quest_form_id)),
                |link| HandlerOutput::Text(link.value().to_owned()),
            )),
            HandlerPhase::Summary => Ok(resolved.map_or_else(
                || HandlerOutput::Value(quest_form_id_value(quest_form_id)),
                |link| HandlerOutput::Text(link.short_name().to_owned()),
            )),
            HandlerPhase::SortKey => Ok(HandlerOutput::Text(format!("{:08X}", quest_form_id.0))),
            HandlerPhase::Validation => Ok(HandlerOutput::Text(resolved.map_or_else(
                || "<Warning: Could not resolve Quest>".to_owned(),
                |_| String::new(),
            ))),
            HandlerPhase::ReferenceResolution if resolved.is_some() => {
                Ok(HandlerOutput::Link(SemanticLink::Record {
                    form_id: quest_form_id,
                }))
            }
            HandlerPhase::ReferenceResolution => Ok(HandlerOutput::None),
            _ => Ok(HandlerOutput::None),
        }
    }
}

impl SemanticHandler for FormatCtdaContextQuestStage {
    fn id(&self) -> &'static str {
        "format.ctda_context_quest_stage"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase == HandlerPhase::ParseEditValue {
            let Some(FieldValue::String(value)) = invocation.value else {
                return Err(ctda_condition_quest_error(
                    self.id(),
                    "context quest-stage edit parsing requires text",
                ));
            };
            return Ok(HandlerOutput::Value(FieldValue::UInt(
                parse_prefixed_u32(value, self.id())?.into(),
            )));
        }
        let stage = i64::try_from(callback_integer(
            invocation.value.ok_or_else(|| {
                ctda_condition_quest_error(
                    self.id(),
                    "context quest-stage formatting requires an integer",
                )
            })?,
            self.id(),
        )?)
        .map_err(|_| ctda_condition_quest_error(self.id(), "context quest stage exceeds i64"))?;
        let quest_form_id =
            resolve_ctda_condition_quest(self.id(), &invocation, self.resolver.as_deref())?;
        let source = handler_record_context(&invocation.context);
        let targets = [Signature(*b"QUST")];
        let quest = quest_form_id.and_then(|form_id| {
            self.resolver
                .as_deref()
                .and_then(|resolver| resolver.resolve_form_id(source, form_id, &targets))
        });
        Ok(HandlerOutput::Text(format_ctda_context_quest_stage(
            stage,
            invocation.phase,
            quest.as_ref(),
        )))
    }
}

impl SemanticHandler for ResolveVmadObjectAliasLink {
    fn id(&self) -> &'static str {
        "resolve.vmad_object_alias"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::ReferenceResolution {
            return Ok(HandlerOutput::None);
        }
        let raw = i64::try_from(callback_integer(
            invocation.value.ok_or_else(|| {
                vmad_alias_link_error("VMAD object-alias link resolution requires an integer")
            })?,
            self.id(),
        )?)
        .map_err(|_| vmad_alias_link_error("VMAD object alias exceeds i64"))?;
        if raw < 0 {
            return Ok(HandlerOutput::None);
        }
        let form_id_path =
            configured_text(self.id(), invocation.context.configuration, "form_id_path")?;
        let Some((quest_form_id, targets)) = invocation
            .value_scope
            .and_then(|scope| scoped_form_id(scope, form_id_path))
        else {
            return Ok(HandlerOutput::None);
        };
        let Some(quest) = self.resolver.as_deref().and_then(|resolver| {
            resolver.resolve_form_id(
                handler_record_context(&invocation.context),
                quest_form_id,
                targets,
            )
        }) else {
            return Ok(HandlerOutput::None);
        };
        let Some(aliases) = quest.quest_aliases() else {
            return Ok(HandlerOutput::None);
        };
        if aliases.iter().all(|alias| alias.index() != raw) {
            return Ok(HandlerOutput::None);
        }
        Ok(HandlerOutput::Link(SemanticLink::QuestAlias {
            quest_form_id,
            alias_index: raw,
        }))
    }
}

impl SemanticHandler for ResolveQuestAliasLink {
    fn id(&self) -> &'static str {
        "resolve.quest_alias"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::ReferenceResolution {
            return Ok(HandlerOutput::None);
        }
        let alias_index = i64::try_from(callback_integer(
            invocation.value.ok_or_else(|| {
                indexed_record_error(self.id(), "quest alias link requires an integer")
            })?,
            self.id(),
        )?)
        .map_err(|_| indexed_record_error(self.id(), "quest alias index exceeds i64"))?;
        if alias_index < 0 {
            return Ok(HandlerOutput::None);
        }
        let source = handler_record_context(&invocation.context);
        let quest_form_id =
            match configured_text(self.id(), invocation.context.configuration, "quest_source")? {
                "source_record" => invocation.context.form_id,
                "sibling_quest" => {
                    let Some(scope) = invocation.value_scope else {
                        return Ok(HandlerOutput::None);
                    };
                    let active =
                        scoped_named_value_by_name(scope, "Bethkit Active Repeat Occurrence")
                            .map_or(scope, |field| &field.value);
                    let Some(quest) = scoped_named_value_by_name(active, "Quest") else {
                        return Ok(HandlerOutput::None);
                    };
                    callback_form_id(&quest.value, self.id())?
                }
                value => {
                    return Err(indexed_record_error(
                        self.id(),
                        format!("unknown quest source {value:?}"),
                    ));
                }
            };
        let targets = [Signature(*b"QUST")];
        let Some(quest) = self
            .resolver
            .as_deref()
            .and_then(|resolver| resolver.resolve_form_id(source, quest_form_id, &targets))
        else {
            return Ok(HandlerOutput::None);
        };
        let Some(aliases) = quest.quest_aliases() else {
            return Ok(HandlerOutput::None);
        };
        if aliases.iter().all(|alias| alias.index() != alias_index) {
            return Ok(HandlerOutput::None);
        }
        Ok(HandlerOutput::Link(SemanticLink::QuestAlias {
            quest_form_id,
            alias_index,
        }))
    }
}

impl SemanticHandler for ResolveLegendaryFilterMod {
    fn id(&self) -> &'static str {
        "resolve.legendary_filter_mod"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::ReferenceResolution {
            return Ok(HandlerOutput::None);
        }
        let Some(filter_index) = invocation.array_indices.last().copied() else {
            return Ok(HandlerOutput::None);
        };
        let Some(scope) = invocation.value_scope else {
            return Ok(HandlerOutput::None);
        };
        let (filters_path, mods_path) =
            legendary_filter_paths(self.id(), &invocation.context.binding.path)?;
        let filter = scoped_array_element(scope, &filters_path, filter_index);
        let Some(FieldValue::Struct(filter_fields)) = filter else {
            return Ok(HandlerOutput::None);
        };
        let base_slot = condition_field(filter_fields, &["Star Slot"])
            .map(|field| callback_integer(&field.value, self.id()))
            .transpose()?;
        let Some(base_slot) = base_slot else {
            return Ok(HandlerOutput::None);
        };
        let mod_offset = callback_integer(
            invocation.value.ok_or_else(|| {
                indexed_record_error(self.id(), "legendary filter link requires an integer")
            })?,
            self.id(),
        )?;
        let Some(mod_offset) = usize::try_from(mod_offset).ok() else {
            return Ok(HandlerOutput::None);
        };
        let Some(mods) = scoped_named_value(scope, &mods_path) else {
            return Ok(HandlerOutput::None);
        };
        let FieldValue::Array(mod_values) = &mods.value else {
            return Err(indexed_record_error(
                self.id(),
                "Legendary Mods target is not an array",
            ));
        };
        let first = mod_values.iter().position(|value| {
            let FieldValue::Struct(fields) = value else {
                return false;
            };
            condition_field(fields, &["Star Slot"]).is_some_and(|field| {
                callback_integer(&field.value, self.id()).ok() == Some(base_slot)
            })
        });
        let Some(target) = first
            .and_then(|first| first.checked_add(mod_offset))
            .and_then(|index| mod_values.get(index))
        else {
            return Ok(HandlerOutput::None);
        };
        let FieldValue::Struct(fields) = target else {
            return Ok(HandlerOutput::None);
        };
        let Some(form_id) = condition_field(fields, &["Legendary Modifier"])
            .map(|field| callback_form_id(&field.value, self.id()))
            .transpose()?
        else {
            return Ok(HandlerOutput::None);
        };
        let source = handler_record_context(&invocation.context);
        if self
            .resolver
            .as_deref()
            .and_then(|resolver| resolver.resolve_form_id(source, form_id, &[]))
            .is_none()
        {
            return Ok(HandlerOutput::None);
        }
        Ok(HandlerOutput::Link(SemanticLink::Record { form_id }))
    }
}

impl SemanticHandler for ResolveNpcFaceEntry {
    fn id(&self) -> &'static str {
        "resolve.npc_face_entry"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::ReferenceResolution {
            return Ok(HandlerOutput::None);
        }
        let index = i64::try_from(callback_integer(
            invocation.value.ok_or_else(|| {
                indexed_record_error(self.id(), "NPC face entry requires an integer")
            })?,
            self.id(),
        )?)
        .map_err(|_| indexed_record_error(self.id(), "NPC face entry index exceeds i64"))?;
        let kind = match configured_text(self.id(), invocation.context.configuration, "entry_kind")?
        {
            "face_dial" => NpcFaceEntryKind::FaceDial,
            "face_morph_phenotype" => NpcFaceEntryKind::FaceMorphPhenotype,
            value => {
                return Err(indexed_record_error(
                    self.id(),
                    format!("unknown NPC face entry kind {value:?}"),
                ));
            }
        };
        let Some(entry) = self.resolver.as_deref().and_then(|resolver| {
            resolver.resolve_npc_face_entry(
                handler_record_context(&invocation.context),
                kind,
                index,
            )
        }) else {
            return Ok(HandlerOutput::None);
        };
        Ok(HandlerOutput::Link(SemanticLink::ExternalElement {
            record_form_id: entry.record_form_id(),
            path: entry.path().to_owned(),
            array_indices: entry.array_indices().to_vec(),
        }))
    }
}

fn legendary_filter_paths(handler: &str, path: &str) -> Result<(String, String)> {
    let components: Vec<&str> = path.split('/').collect();
    let Some(index) = components.iter().position(|component| {
        component
            .split_once(':')
            .is_some_and(|(_, name)| matches!(name, "Include Filters" | "Exclude Filters"))
    }) else {
        return Err(indexed_record_error(
            handler,
            format!("legendary filter path has no filter array: {path:?}"),
        ));
    };
    let filters = components[..=index].join("/");
    let mut mods = components[..index].to_vec();
    mods.push("11:Legendary Mods");
    Ok((filters, mods.join("/")))
}

fn scoped_array_element<'a>(
    scope: &'a FieldValue<'static>,
    path: &str,
    index: usize,
) -> Option<&'a FieldValue<'static>> {
    scoped_named_value(scope, path).and_then(|field| match &field.value {
        FieldValue::Array(values) => values.get(index),
        _ => None,
    })
}

fn format_ctda_quest_stage(
    stage: i64,
    phase: HandlerPhase,
    quest: Option<&FormLinkInfo>,
) -> String {
    if phase == HandlerPhase::SortKey {
        return format!("{:08X}", stage as u64);
    }
    if stage < 0 {
        return match phase {
            HandlerPhase::Display | HandlerPhase::Summary => format!("{stage} NONE"),
            HandlerPhase::EditValue => stage.to_string(),
            HandlerPhase::Validation => String::new(),
            _ => String::new(),
        };
    }
    let unresolved = match phase {
        HandlerPhase::Display => {
            format!("{stage} <Warning: Could not resolve Quest in Parameter #1>")
        }
        HandlerPhase::Summary | HandlerPhase::EditValue => stage.to_string(),
        HandlerPhase::Validation => "<Warning: Could not resolve Quest in Parameter #1>".to_owned(),
        _ => String::new(),
    };
    let Some(quest) = quest else {
        return unresolved;
    };
    let Some(stages) = quest.quest_stages() else {
        return match phase {
            HandlerPhase::Display => format!(
                "{stage} <Warning: \"{}\" is not a Quest record>",
                quest.short_name()
            ),
            HandlerPhase::Summary | HandlerPhase::EditValue => stage.to_string(),
            HandlerPhase::Validation => {
                format!(
                    "<Warning: \"{}\" is not a Quest record>",
                    quest.short_name()
                )
            }
            _ => unresolved,
        };
    };
    let Some(found) = stages.iter().find(|candidate| candidate.index() == stage) else {
        return match phase {
            HandlerPhase::Display => format!(
                "{stage} <Warning: Quest Stage not found in \"{}\">",
                quest.value()
            ),
            HandlerPhase::Summary | HandlerPhase::EditValue => stage.to_string(),
            HandlerPhase::Validation => {
                format!("<Warning: Quest Stage not found in \"{}\">", quest.value())
            }
            _ => unresolved,
        };
    };
    match phase {
        HandlerPhase::Display | HandlerPhase::Summary | HandlerPhase::EditValue => {
            format_quest_stage_label(found)
        }
        HandlerPhase::Validation => String::new(),
        _ => unresolved,
    }
}

fn resolve_ctda_parameter_link(
    invocation: &HandlerInvocation<'_>,
    resolver: Option<&dyn FormLinkResolver>,
    handler: &str,
) -> Result<Option<FormLinkInfo>> {
    let parameter_path =
        configured_text(handler, invocation.context.configuration, "parameter_path")?;
    Ok(invocation
        .value_scope
        .and_then(|scope| scoped_form_id(scope, parameter_path))
        .and_then(|(form_id, targets)| {
            resolver.and_then(|resolver| {
                resolver.resolve_form_id(
                    handler_record_context(&invocation.context),
                    form_id,
                    targets,
                )
            })
        }))
}

fn format_ctda_variable_name(
    value: i64,
    phase: HandlerPhase,
    record: Option<&FormLinkInfo>,
) -> Result<String> {
    let unresolved = match phase {
        HandlerPhase::Display => {
            format!("{value} <Warning: Could not resolve Parameter 1>")
        }
        HandlerPhase::Summary | HandlerPhase::EditValue => value.to_string(),
        HandlerPhase::Validation => "<Warning: Could not resolve Parameter 1>".to_owned(),
        _ => String::new(),
    };
    let Some(record) = record else {
        return Ok(unresolved);
    };
    let metadata = record.script_variables().ok_or_else(|| {
        ctda_variable_name_error("FormID resolver did not supply legacy script-variable metadata")
    })?;
    let warning = match metadata {
        ScriptVariableMetadata::MissingReference => {
            format!(
                "\"{}\" does not contain a SCRI subrecord",
                record.short_name()
            )
        }
        ScriptVariableMetadata::InvalidReference => {
            format!("\"{}\" does not have a valid script", record.short_name())
        }
        ScriptVariableMetadata::Resolved {
            script_name,
            variables,
        } => {
            if let Some(variable) = variables.iter().find(|variable| variable.index() == value) {
                return Ok(match phase {
                    HandlerPhase::Display | HandlerPhase::Summary | HandlerPhase::EditValue => {
                        variable.name().to_owned()
                    }
                    HandlerPhase::Validation => String::new(),
                    _ => unresolved,
                });
            }
            format!("Variable Index not found in \"{script_name}\"")
        }
    };
    Ok(match phase {
        HandlerPhase::Display => format!("{value} <Warning: {warning}>"),
        HandlerPhase::Summary | HandlerPhase::EditValue => value.to_string(),
        HandlerPhase::Validation => format!("<Warning: {warning}>"),
        _ => unresolved,
    })
}

fn format_ctda_quest_objective(
    value: i64,
    phase: HandlerPhase,
    quest: Option<&FormLinkInfo>,
) -> String {
    let unresolved = match phase {
        HandlerPhase::Display => {
            format!("{value} <Warning: Could not resolve Parameter 1>")
        }
        HandlerPhase::Summary | HandlerPhase::EditValue => value.to_string(),
        HandlerPhase::Validation => "<Warning: Could not resolve Parameter 1>".to_owned(),
        _ => String::new(),
    };
    let Some(quest) = quest else {
        return unresolved;
    };
    let Some(objectives) = quest.quest_objectives() else {
        return match phase {
            HandlerPhase::Display => format!(
                "{value} <Warning: \"{}\" is not a Quest record>",
                quest.short_name()
            ),
            HandlerPhase::Summary | HandlerPhase::EditValue => value.to_string(),
            HandlerPhase::Validation => {
                format!(
                    "<Warning: \"{}\" is not a Quest record>",
                    quest.short_name()
                )
            }
            _ => unresolved,
        };
    };
    if let Some(objective) = objectives
        .iter()
        .find(|objective| objective.index() == value)
    {
        return match phase {
            HandlerPhase::Display | HandlerPhase::Summary | HandlerPhase::EditValue => {
                format_indexed_label(objective.index(), objective.display_text())
            }
            HandlerPhase::Validation => String::new(),
            _ => unresolved,
        };
    }
    match phase {
        HandlerPhase::Display => format!(
            "{value} <Warning: Quest Objective not found in \"{}\">",
            quest.value()
        ),
        HandlerPhase::Summary | HandlerPhase::EditValue => value.to_string(),
        HandlerPhase::Validation => {
            format!(
                "<Warning: Quest Objective not found in \"{}\">",
                quest.value()
            )
        }
        _ => unresolved,
    }
}

fn format_quest_stage_label(stage: &QuestStageInfo) -> String {
    format_indexed_label(stage.index(), stage.log_entry())
}

fn format_indexed_label(index: i64, label: &str) -> String {
    let mut text = index.to_string();
    while text.len() < 3 {
        text.insert(0, '0');
    }
    let label = label.trim();
    if !label.is_empty() {
        text.push(' ');
        text.push_str(label);
    }
    text
}

fn format_ctda_context_quest_stage(
    stage: i64,
    phase: HandlerPhase,
    quest: Option<&FormLinkInfo>,
) -> String {
    if phase == HandlerPhase::SortKey {
        return format!("{:08X}", stage as u64);
    }
    let unresolved = match phase {
        HandlerPhase::Display => format!("{stage} <Warning: Could not resolve Quest>"),
        HandlerPhase::Summary | HandlerPhase::EditValue => stage.to_string(),
        HandlerPhase::Validation => "<Warning: Could not resolve Quest>".to_owned(),
        _ => String::new(),
    };
    let Some(quest) = quest else {
        return unresolved;
    };
    let Some(stages) = quest.quest_stages() else {
        return unresolved;
    };
    if let Some(entry) = stages.iter().find(|entry| entry.index() == stage) {
        return match phase {
            HandlerPhase::Display | HandlerPhase::Summary | HandlerPhase::EditValue => {
                format_quest_stage_label(entry)
            }
            HandlerPhase::Validation => String::new(),
            _ => String::new(),
        };
    }
    match phase {
        HandlerPhase::Display => format!(
            "{stage} <Warning: Quest Stage/Objective not found in \"{}\">",
            quest.value()
        ),
        HandlerPhase::Summary | HandlerPhase::EditValue => stage.to_string(),
        HandlerPhase::Validation => format!(
            "<Warning: Quest Stage/Objective not found in \"{}\">",
            quest.value()
        ),
        _ => String::new(),
    }
}

fn resolve_ctda_condition_quest(
    handler: &str,
    invocation: &HandlerInvocation<'_>,
    resolver: Option<&dyn FormLinkResolver>,
) -> Result<Option<FormId>> {
    let source = configured_text(handler, invocation.context.configuration, "quest_source")?;
    if source == "none" {
        return Ok(None);
    }
    if source == "record_form_id" {
        return Ok(
            (invocation.context.form_id != FormId::NULL).then_some(invocation.context.form_id)
        );
    }
    let record = require_source_record(handler, invocation)?;
    if source == "parent" {
        return Ok(resolver.and_then(|resolver| {
            resolver.resolve_condition_quest_form_id(
                handler_record_context(&invocation.context),
                record,
            )
        }));
    }
    if source != "subrecord" {
        return Err(ctda_condition_quest_error(
            handler,
            format!("unknown quest source {source:?}"),
        ));
    }
    let signature =
        configured_signature(handler, invocation.context.configuration, "quest_signature")?;
    let direct = record
        .get(signature)?
        .map(|subrecord| subrecord.as_u32().map(FormId))
        .transpose()?;
    if direct.is_some() {
        return Ok(direct);
    }
    if configured_optional_bool(handler, invocation.context.configuration, "parent_fallback")?
        .unwrap_or(false)
    {
        return Ok(resolver.and_then(|resolver| {
            resolver.resolve_condition_quest_form_id(
                handler_record_context(&invocation.context),
                record,
            )
        }));
    }
    Ok(None)
}

fn quest_form_id_value(form_id: FormId) -> FieldValue<'static> {
    FieldValue::FormId {
        value: form_id,
        targets: vec![Signature(*b"QUST")],
    }
}

fn callback_form_id(value: &FieldValue<'_>, handler: &str) -> Result<FormId> {
    match value {
        FieldValue::FormId { value, .. } => Ok(*value),
        _ => callback_u32(callback_integer(value, handler)?, handler).map(FormId),
    }
}

fn parse_prefixed_i32(value: &str) -> Result<i64> {
    let value = value.trim();
    let end = value
        .char_indices()
        .take_while(|(_, value)| *value == '-' || value.is_ascii_digit())
        .map(|(index, value)| index + value.len_utf8())
        .last()
        .unwrap_or(0);
    value[..end]
        .parse::<i32>()
        .map(i64::from)
        .map_err(|error| ctda_quest_stage_error(format!("invalid quest stage: {error}")))
}

fn parse_prefixed_u32(value: &str, handler: &str) -> Result<u32> {
    let value = value.trim();
    let end = value
        .char_indices()
        .take_while(|(_, value)| value.is_ascii_digit())
        .map(|(index, value)| index + value.len_utf8())
        .last()
        .unwrap_or(0);
    value[..end].parse::<u32>().map_err(|error| {
        ctda_condition_quest_error(handler, format!("invalid quest stage: {error}"))
    })
}

fn ctda_quest_stage_error(message: impl Into<String>) -> SemanticError {
    SemanticError::Handler {
        handler: "format.ctda_quest_stage".to_owned(),
        message: message.into(),
    }
}

fn ctda_variable_name_error(message: impl Into<String>) -> SemanticError {
    SemanticError::Handler {
        handler: "format.ctda_variable_name".to_owned(),
        message: message.into(),
    }
}

fn ctda_quest_objective_error(message: impl Into<String>) -> SemanticError {
    SemanticError::Handler {
        handler: "format.ctda_quest_objective".to_owned(),
        message: message.into(),
    }
}

fn ctda_condition_quest_error(handler: &str, message: impl Into<String>) -> SemanticError {
    SemanticError::Handler {
        handler: handler.to_owned(),
        message: message.into(),
    }
}

fn resolve_condition_quest_subrecord(
    record: &Record,
    signature: Signature,
    source: HandlerRecordContext,
    resolver: Option<&dyn FormLinkResolver>,
) -> Result<Option<FormLinkInfo>> {
    let subrecords = record.subrecords()?;
    let Some(subrecord) = subrecords
        .iter()
        .find(|subrecord| subrecord.signature == signature)
    else {
        return Ok(None);
    };
    let bytes: [u8; 4] = subrecord
        .as_bytes()
        .get(..4)
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| {
            ctda_condition_alias_error(format!(
                "{signature} quest reference is shorter than four bytes"
            ))
        })?;
    let form_id = FormId(u32::from_le_bytes(bytes));
    Ok(resolver
        .and_then(|resolver| resolver.resolve_form_id(source, form_id, &[Signature(*b"QUST")])))
}

fn scoped_string<'a>(value: &'a FieldValue<'static>, target_path: &str) -> Option<&'a str> {
    match value {
        FieldValue::Struct(values) => {
            for value in values {
                if value.path == target_path {
                    if let FieldValue::String(value) = &value.value {
                        return Some(value.as_ref());
                    }
                    return None;
                }
                if let Some(found) = scoped_string(&value.value, target_path) {
                    return Some(found);
                }
            }
            None
        }
        FieldValue::Array(values) => values
            .iter()
            .find_map(|value| scoped_string(value, target_path)),
        _ => None,
    }
}

fn scoped_named_value<'a>(
    value: &'a FieldValue<'static>,
    target_path: &str,
) -> Option<&'a crate::NamedValue<'static>> {
    match value {
        FieldValue::Struct(values) => values.iter().find_map(|value| {
            (value.path == target_path)
                .then_some(value)
                .or_else(|| scoped_named_value(&value.value, target_path))
        }),
        FieldValue::Array(values) => values
            .iter()
            .find_map(|value| scoped_named_value(value, target_path)),
        _ => None,
    }
}

fn scoped_named_value_by_name<'a>(
    value: &'a FieldValue<'static>,
    target_name: &str,
) -> Option<&'a crate::NamedValue<'static>> {
    match value {
        FieldValue::Struct(values) => values.iter().find_map(|value| {
            (value.name == target_name)
                .then_some(value)
                .or_else(|| scoped_named_value_by_name(&value.value, target_name))
        }),
        FieldValue::Array(values) => values
            .iter()
            .find_map(|value| scoped_named_value_by_name(value, target_name)),
        _ => None,
    }
}

fn format_unresolved_condition_alias(raw: i64, phase: HandlerPhase) -> String {
    match phase {
        HandlerPhase::Display | HandlerPhase::Summary | HandlerPhase::EditValue => raw.to_string(),
        _ => String::new(),
    }
}

fn ctda_condition_alias_error(message: impl Into<String>) -> SemanticError {
    SemanticError::Handler {
        handler: "format.ctda_condition_alias".to_owned(),
        message: message.into(),
    }
}

fn ctda_string_parameter_error(message: impl Into<String>) -> SemanticError {
    SemanticError::Handler {
        handler: "format.ctda_string_parameter".to_owned(),
        message: message.into(),
    }
}

impl SemanticHandler for FormatVmadObjectAlias {
    fn id(&self) -> &'static str {
        "format.vmad_object_alias"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase == HandlerPhase::ParseEditValue {
            let Some(FieldValue::String(value)) = invocation.value else {
                return Err(vmad_alias_error(
                    "VMAD object-alias edit parsing requires text",
                ));
            };
            return Ok(HandlerOutput::Value(FieldValue::Int(parse_vmad_alias(
                value,
                invocation.context.game,
            ))));
        }
        let raw = i64::try_from(callback_integer(
            invocation.value.ok_or_else(|| {
                vmad_alias_error("VMAD object-alias formatting requires an integer")
            })?,
            self.id(),
        )?)
        .map_err(|_| vmad_alias_error("VMAD object alias exceeds i64"))?;
        let form_id_path =
            configured_text(self.id(), invocation.context.configuration, "form_id_path")?;
        let Some((form_id, targets)) = invocation
            .value_scope
            .and_then(|scope| scoped_form_id(scope, form_id_path))
        else {
            return Ok(HandlerOutput::Text(String::new()));
        };
        Ok(HandlerOutput::Text(format_vmad_object_alias(
            raw,
            invocation.phase,
            invocation.context.game,
            form_id,
            targets,
            self.resolver.as_deref(),
            handler_record_context(&invocation.context),
        )?))
    }
}

fn scoped_form_id<'a>(
    value: &'a FieldValue<'static>,
    target_path: &str,
) -> Option<(FormId, &'a [Signature])> {
    match value {
        FieldValue::Struct(values) => {
            for value in values {
                if value.path == target_path {
                    if let FieldValue::FormId { value, targets } = &value.value {
                        return Some((*value, targets));
                    }
                    return None;
                }
                if let Some(found) = scoped_form_id(&value.value, target_path) {
                    return Some(found);
                }
            }
            None
        }
        FieldValue::Array(values) => values
            .iter()
            .find_map(|value| scoped_form_id(value, target_path)),
        _ => None,
    }
}

fn format_vmad_object_alias(
    raw: i64,
    phase: HandlerPhase,
    game: SchemaGame,
    quest_form_id: FormId,
    targets: &[Signature],
    resolver: Option<&dyn FormLinkResolver>,
    source: HandlerRecordContext,
) -> Result<String> {
    if phase == HandlerPhase::SortKey {
        return Ok(format!("{:08X}", raw as u64));
    }
    let link =
        resolver.and_then(|resolver| resolver.resolve_form_id(source, quest_form_id, targets));
    Ok(format_resolved_quest_alias(raw, phase, game, link.as_ref()))
}

fn format_resolved_quest_alias(
    raw: i64,
    phase: HandlerPhase,
    game: SchemaGame,
    link: Option<&FormLinkInfo>,
) -> String {
    let mut result =
        unresolved_vmad_alias(raw, phase, game).expect("alias fallback formatting is infallible");
    if vmad_alias_is_sentinel(raw, game)
        && !matches!(
            phase,
            HandlerPhase::NativeValue | HandlerPhase::ParseEditValue
        )
    {
        return result;
    }
    let Some(link) = link else {
        return result;
    };
    let Some(aliases) = link.quest_aliases() else {
        return match phase {
            HandlerPhase::Display => format!(
                "{raw} <Warning: \"{}\" is not a Quest record>",
                link.short_name()
            ),
            HandlerPhase::Summary => raw.to_string(),
            HandlerPhase::Validation => {
                format!("<Warning: \"{}\" is not a Quest record>", link.short_name())
            }
            _ => result,
        };
    };
    if let Some(alias) = aliases.iter().find(|alias| alias.index() == raw) {
        let include_index = phase != HandlerPhase::Summary
            || !matches!(
                game,
                SchemaGame::SkyrimLe | SchemaGame::SkyrimSe | SchemaGame::SkyrimVr
            );
        result = if include_index {
            format_vmad_alias_label(alias)
        } else {
            alias.editor_id().to_owned()
        };
        if phase == HandlerPhase::Validation {
            result.clear();
        }
        return result;
    }
    match phase {
        HandlerPhase::Display => format!(
            "{raw} <Warning: Quest Alias not found in \"{}\">",
            link.value()
        ),
        HandlerPhase::Summary => raw.to_string(),
        HandlerPhase::Validation => {
            format!("<Warning: Quest Alias not found in \"{}\">", link.value())
        }
        _ => result,
    }
}

fn unresolved_vmad_alias(raw: i64, phase: HandlerPhase, game: SchemaGame) -> Result<String> {
    let player_sentinel = matches!(
        game,
        SchemaGame::Fallout4
            | SchemaGame::Fallout4Vr
            | SchemaGame::Fallout76
            | SchemaGame::Starfield
    );
    let none_summary_empty = matches!(
        game,
        SchemaGame::SkyrimLe | SchemaGame::SkyrimSe | SchemaGame::SkyrimVr | SchemaGame::Starfield
    );
    let text = match phase {
        HandlerPhase::Display if raw == -1 => "None".to_owned(),
        HandlerPhase::Summary if raw == -1 && none_summary_empty => String::new(),
        HandlerPhase::Summary if raw == -1 => "None".to_owned(),
        HandlerPhase::Display | HandlerPhase::Summary if raw == -2 && player_sentinel => {
            "Player".to_owned()
        }
        HandlerPhase::Display => format!("{raw} <Warning: Could not resolve alias>"),
        HandlerPhase::Summary => raw.to_string(),
        HandlerPhase::EditValue if raw == -1 => "None".to_owned(),
        HandlerPhase::EditValue => raw.to_string(),
        HandlerPhase::Validation if raw == -1 || (raw == -2 && player_sentinel) => String::new(),
        HandlerPhase::Validation => "<Warning: Could not resolve alias>".to_owned(),
        HandlerPhase::NativeValue => String::new(),
        HandlerPhase::DecodeNormalize
        | HandlerPhase::ParseEditValue
        | HandlerPhase::UnionSelection
        | HandlerPhase::ArrayCount
        | HandlerPhase::ArrayElementInclusion
        | HandlerPhase::DefaultValue
        | HandlerPhase::AfterLoad
        | HandlerPhase::AfterSet
        | HandlerPhase::ReferenceResolution
        | HandlerPhase::Conflict
        | HandlerPhase::RecordMetadata
        | HandlerPhase::Removability
        | HandlerPhase::SortKey => String::new(),
    };
    Ok(text)
}

fn vmad_alias_is_sentinel(raw: i64, game: SchemaGame) -> bool {
    raw == -1
        || raw == -2
            && matches!(
                game,
                SchemaGame::Fallout4
                    | SchemaGame::Fallout4Vr
                    | SchemaGame::Fallout76
                    | SchemaGame::Starfield
            )
}

fn format_vmad_alias_label(alias: &QuestAliasInfo) -> String {
    let mut text = alias.index().to_string();
    while text.len() < 3 {
        text.insert(0, '0');
    }
    if !alias.editor_id().is_empty() {
        text.push(' ');
        text.push_str(alias.editor_id());
    }
    text
}

fn parse_vmad_alias(value: &str, game: SchemaGame) -> i64 {
    if value == "None" {
        return -1;
    }
    if value == "Player"
        && matches!(
            game,
            SchemaGame::Fallout4
                | SchemaGame::Fallout4Vr
                | SchemaGame::Fallout76
                | SchemaGame::Starfield
        )
    {
        return -2;
    }
    let value = value.trim();
    let end = value
        .char_indices()
        .take_while(|(_, value)| *value == '-' || value.is_ascii_digit())
        .map(|(index, value)| index + value.len_utf8())
        .last()
        .unwrap_or(0);
    value[..end].parse::<i32>().map(i64::from).unwrap_or(-1)
}

fn vmad_alias_error(message: impl Into<String>) -> SemanticError {
    SemanticError::Handler {
        handler: "format.vmad_object_alias".to_owned(),
        message: message.into(),
    }
}

fn vmad_alias_link_error(message: impl Into<String>) -> SemanticError {
    SemanticError::Handler {
        handler: "resolve.vmad_object_alias".to_owned(),
        message: message.into(),
    }
}

struct FormatLandscapePosition;

impl SemanticHandler for FormatLandscapePosition {
    fn id(&self) -> &'static str {
        "format.landscape_position"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase == HandlerPhase::ParseEditValue {
            let Some(FieldValue::String(value)) = invocation.value else {
                return Err(landscape_position_error(
                    "landscape position edit parsing requires text",
                ));
            };
            let value = parse_delphi_integer(value, self.id())?;
            return Ok(if value < 0 {
                HandlerOutput::Value(FieldValue::Int(value))
            } else {
                HandlerOutput::Value(FieldValue::UInt(value as u64))
            });
        }

        let value = i64::try_from(callback_integer(
            invocation.value.ok_or_else(|| {
                landscape_position_error("landscape position formatting requires an integer")
            })?,
            self.id(),
        )?)
        .map_err(|_| landscape_position_error("landscape position exceeds i64"))?;
        let row = value / 17;
        let column = value % 17;
        let text = match invocation.phase {
            HandlerPhase::Display | HandlerPhase::Summary => {
                format!("{value} -> {row}:{column}")
            }
            HandlerPhase::SortKey => format!("{row:02X}{column:02X}"),
            HandlerPhase::EditValue | HandlerPhase::NativeValue => value.to_string(),
            HandlerPhase::Validation if !(0..=288).contains(&value) => {
                format!("<Out of range: {value}>")
            }
            HandlerPhase::Validation => String::new(),
            _ => return Ok(HandlerOutput::None),
        };
        Ok(HandlerOutput::Text(text))
    }
}

struct FormatClimateMoons;

impl SemanticHandler for FormatClimateMoons {
    fn id(&self) -> &'static str {
        "format.climate_moons"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase == HandlerPhase::ParseEditValue {
            let Some(FieldValue::String(value)) = invocation.value else {
                return Err(climate_moons_error(
                    "climate moon edit parsing requires text",
                ));
            };
            let value = parse_delphi_integer(value, self.id())?;
            return Ok(if value < 0 {
                HandlerOutput::Value(FieldValue::Int(value))
            } else {
                HandlerOutput::Value(FieldValue::UInt(value as u64))
            });
        }

        let value = i64::try_from(callback_integer(
            invocation.value.ok_or_else(|| {
                climate_moons_error("climate moon formatting requires an integer")
            })?,
            self.id(),
        )?)
        .map_err(|_| climate_moons_error("climate moon value exceeds i64"))?;
        let text = match invocation.phase {
            HandlerPhase::Display | HandlerPhase::Summary => {
                format_climate_moons(value, invocation.context.game)
            }
            HandlerPhase::SortKey => format!("{value:02X}"),
            HandlerPhase::EditValue | HandlerPhase::NativeValue => value.to_string(),
            HandlerPhase::Validation => String::new(),
            _ => return Ok(HandlerOutput::None),
        };
        Ok(HandlerOutput::Text(text))
    }
}

struct FormatClimateTime;

impl SemanticHandler for FormatClimateTime {
    fn id(&self) -> &'static str {
        "format.climate_time"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase == HandlerPhase::ParseEditValue {
            return parse_integer_handler_value(
                invocation.value,
                self.id(),
                "climate time edit parsing requires text",
            );
        }

        let value = i64::try_from(callback_integer(
            invocation
                .value
                .ok_or_else(|| climate_time_error("climate time formatting requires an integer"))?,
            self.id(),
        )?)
        .map_err(|_| climate_time_error("climate time value exceeds i64"))?;
        let text = match invocation.phase {
            HandlerPhase::Display | HandlerPhase::Summary => format_climate_time(value),
            HandlerPhase::SortKey => format!("{value:04X}"),
            HandlerPhase::EditValue | HandlerPhase::NativeValue | HandlerPhase::Validation => {
                String::new()
            }
            _ => return Ok(HandlerOutput::None),
        };
        Ok(HandlerOutput::Text(text))
    }
}

struct FormatAlocTime;

impl SemanticHandler for FormatAlocTime {
    fn id(&self) -> &'static str {
        "format.aloc_time"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase == HandlerPhase::ParseEditValue {
            return parse_integer_handler_value(
                invocation.value,
                self.id(),
                "media location time edit parsing requires text",
            );
        }

        let value = i64::try_from(callback_integer(
            invocation.value.ok_or_else(|| {
                aloc_time_error("media location time formatting requires an integer")
            })?,
            self.id(),
        )?)
        .map_err(|_| aloc_time_error("media location time value exceeds i64"))?;
        let text = match invocation.phase {
            HandlerPhase::Display | HandlerPhase::Summary => format_aloc_time(value),
            HandlerPhase::SortKey => format!("{value:04X}"),
            HandlerPhase::EditValue | HandlerPhase::NativeValue | HandlerPhase::Validation => {
                String::new()
            }
            _ => return Ok(HandlerOutput::None),
        };
        Ok(HandlerOutput::Text(text))
    }
}

struct FormatIdleAnimationGroup;

impl SemanticHandler for FormatIdleAnimationGroup {
    fn id(&self) -> &'static str {
        "format.idle_animation_group"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase == HandlerPhase::ParseEditValue {
            return parse_integer_handler_value(
                invocation.value,
                self.id(),
                "idle animation edit parsing requires text",
            );
        }
        let value = i64::try_from(callback_integer(
            invocation.value.ok_or_else(|| {
                integer_formatter_error(self.id(), "idle animation formatting requires an integer")
            })?,
            self.id(),
        )?)
        .map_err(|_| integer_formatter_error(self.id(), "idle animation value exceeds i64"))?;
        let legacy_fallout = matches!(
            invocation.context.game,
            SchemaGame::Fallout3 | SchemaGame::FalloutNv
        );
        let masked = value & if legacy_fallout { !0xC0 } else { !0x80 };
        let name = idle_animation_group_name(masked, legacy_fallout);
        let text = match invocation.phase {
            HandlerPhase::Display | HandlerPhase::Summary => {
                let mut text = name.map_or_else(|| format!("<Unknown: {masked}>"), str::to_owned);
                if value & 0x80 == 0 {
                    text.push_str(", Must return a file");
                }
                text
            }
            HandlerPhase::SortKey => format!("{value:02X}"),
            HandlerPhase::EditValue | HandlerPhase::NativeValue => value.to_string(),
            HandlerPhase::Validation => {
                name.map_or_else(|| format!("<Unknown: {masked}>"), |_| String::new())
            }
            _ => return Ok(HandlerOutput::None),
        };
        Ok(HandlerOutput::Text(text))
    }
}

struct FormatWeatherClassification;

impl SemanticHandler for FormatWeatherClassification {
    fn id(&self) -> &'static str {
        "format.weather_classification"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase == HandlerPhase::ParseEditValue {
            return parse_integer_handler_value(
                invocation.value,
                self.id(),
                "weather classification edit parsing requires text",
            );
        }
        let value = i64::try_from(callback_integer(
            invocation.value.ok_or_else(|| {
                integer_formatter_error(
                    self.id(),
                    "weather classification formatting requires an integer",
                )
            })?,
            self.id(),
        )?)
        .map_err(|_| {
            integer_formatter_error(self.id(), "weather classification value exceeds i64")
        })?;
        let masked = value & !192;
        let name = weather_classification_name(masked, invocation.context.game);
        let text = match invocation.phase {
            HandlerPhase::Display | HandlerPhase::Summary => {
                name.map_or_else(|| format!("<Unknown: {masked}>"), str::to_owned)
            }
            HandlerPhase::SortKey => format!("{value:02X}"),
            HandlerPhase::EditValue | HandlerPhase::NativeValue => value.to_string(),
            HandlerPhase::Validation => {
                name.map_or_else(|| format!("<Unknown: {masked}>"), |_| String::new())
            }
            _ => return Ok(HandlerOutput::None),
        };
        Ok(HandlerOutput::Text(text))
    }
}

struct FixedHexIntegerFormatter;

struct ScaledInt4Formatter;

struct HideFfffFormatter;

struct CloudSpeedFormatter;

impl SemanticHandler for CloudSpeedFormatter {
    fn id(&self) -> &'static str {
        "format.cloud_speed"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase == HandlerPhase::ParseEditValue {
            let Some(FieldValue::String(input)) = invocation.value else {
                return Err(integer_formatter_error(
                    self.id(),
                    "cloud speed edit parsing requires text",
                ));
            };
            let parsed = input.trim().parse::<f64>().map_err(|error| {
                integer_formatter_error(
                    self.id(),
                    format!("invalid cloud speed edit value {input:?}: {error}"),
                )
            })?;
            let scaled = parsed * 10.0 * 127.0 + 127.0;
            if !scaled.is_finite() || scaled < i64::MIN as f64 {
                return Err(integer_formatter_error(
                    self.id(),
                    "cloud speed edit value exceeds i64",
                ));
            }
            let value = (scaled.round_ties_even() as i64).min(254);
            return Ok(if value < 0 {
                HandlerOutput::Value(FieldValue::Int(value))
            } else {
                HandlerOutput::Value(FieldValue::UInt(value as u64))
            });
        }

        let value = i64::try_from(callback_integer(
            invocation.value.ok_or_else(|| {
                integer_formatter_error(self.id(), "cloud speed formatting requires an integer")
            })?,
            self.id(),
        )?)
        .map_err(|_| integer_formatter_error(self.id(), "cloud speed value exceeds i64"))?;
        let text = match invocation.phase {
            HandlerPhase::Display | HandlerPhase::Summary | HandlerPhase::EditValue => {
                format!("{:.4}", (value - 127) as f64 / 1_270.0)
            }
            HandlerPhase::Validation => String::new(),
            _ => return Ok(HandlerOutput::None),
        };
        Ok(HandlerOutput::Text(text))
    }
}

impl SemanticHandler for HideFfffFormatter {
    fn id(&self) -> &'static str {
        "format.hide_ffff"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        let value = u64::try_from(callback_integer(
            invocation.value.ok_or_else(|| {
                integer_formatter_error(self.id(), "FFFF formatting requires an integer")
            })?,
            self.id(),
        )?)
        .map_err(|_| integer_formatter_error(self.id(), "FFFF value must be non-negative"))?;
        let text = match invocation.phase {
            HandlerPhase::Display | HandlerPhase::Summary if value == 0xffff => "None".to_owned(),
            HandlerPhase::Display | HandlerPhase::Summary => value.to_string(),
            HandlerPhase::SortKey => format!("{value:04X}"),
            _ => return Ok(HandlerOutput::None),
        };
        Ok(HandlerOutput::Text(text))
    }
}

impl SemanticHandler for ScaledInt4Formatter {
    fn id(&self) -> &'static str {
        "format.scaled_int4"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase == HandlerPhase::ParseEditValue {
            let Some(FieldValue::String(input)) = invocation.value else {
                return Err(integer_formatter_error(
                    self.id(),
                    "scaled integer edit parsing requires text",
                ));
            };
            let parsed = input.trim().parse::<f64>().map_err(|error| {
                integer_formatter_error(
                    self.id(),
                    format!("invalid scaled integer edit value {input:?}: {error}"),
                )
            })?;
            let scaled = parsed * 10_000.0;
            if !scaled.is_finite() || scaled < i64::MIN as f64 || scaled > i64::MAX as f64 {
                return Err(integer_formatter_error(
                    self.id(),
                    "scaled integer edit value exceeds i64",
                ));
            }
            let value = scaled.round_ties_even() as i64;
            return Ok(if value < 0 {
                HandlerOutput::Value(FieldValue::Int(value))
            } else {
                HandlerOutput::Value(FieldValue::UInt(value as u64))
            });
        }

        let value = i64::try_from(callback_integer(
            invocation.value.ok_or_else(|| {
                integer_formatter_error(self.id(), "scaled integer formatting requires an integer")
            })?,
            self.id(),
        )?)
        .map_err(|_| integer_formatter_error(self.id(), "scaled integer value exceeds i64"))?;
        let fixed = format!("{:.4}", value as f64 / 10_000.0);
        let text = match invocation.phase {
            HandlerPhase::Display | HandlerPhase::Summary | HandlerPhase::EditValue => fixed,
            HandlerPhase::SortKey => {
                let padded = format!("{fixed:0>22}");
                format!("{}{padded}", if value < 0 { '-' } else { '+' })
            }
            HandlerPhase::Validation => String::new(),
            HandlerPhase::NativeValue => value.to_string(),
            _ => return Ok(HandlerOutput::None),
        };
        Ok(HandlerOutput::Text(text))
    }
}

impl SemanticHandler for FixedHexIntegerFormatter {
    fn id(&self) -> &'static str {
        "format.fixed_hex_integer"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase == HandlerPhase::ParseEditValue {
            if invocation
                .context
                .configuration
                .get("plain_hex_edit")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
            {
                return parse_plain_hex_handler_value(invocation.value, self.id());
            }
            return parse_integer_handler_value(
                invocation.value,
                self.id(),
                "fixed hexadecimal edit parsing requires text",
            );
        }
        let value = u64::try_from(callback_integer(
            invocation.value.ok_or_else(|| {
                integer_formatter_error(
                    self.id(),
                    "fixed hexadecimal formatting requires an integer",
                )
            })?,
            self.id(),
        )?)
        .map_err(|_| {
            integer_formatter_error(self.id(), "fixed hexadecimal value must be non-negative")
        })?;
        let width = invocation
            .context
            .configuration
            .get("width")
            .and_then(serde_json::Value::as_u64)
            .and_then(|width| usize::try_from(width).ok())
            .filter(|width| (1..=16).contains(width))
            .ok_or_else(|| {
                integer_formatter_error(
                    self.id(),
                    "fixed hexadecimal width must be between 1 and 16",
                )
            })?;
        let hexadecimal = format!("{value:0width$X}");
        let text = match invocation.phase {
            HandlerPhase::Display | HandlerPhase::Summary | HandlerPhase::SortKey => hexadecimal,
            HandlerPhase::EditValue => {
                let prefix = invocation
                    .context
                    .configuration
                    .get("edit_prefix")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                format!("{prefix}{hexadecimal}")
            }
            HandlerPhase::NativeValue => value.to_string(),
            HandlerPhase::Validation => String::new(),
            _ => return Ok(HandlerOutput::None),
        };
        Ok(HandlerOutput::Text(text))
    }
}

struct NextObjectIdFormatter {
    resolver: Option<Arc<dyn NextObjectIdResolver>>,
}

impl SemanticHandler for NextObjectIdFormatter {
    fn id(&self) -> &'static str {
        "format.next_object_id"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase == HandlerPhase::ParseEditValue {
            let Some(FieldValue::String(input)) = invocation.value else {
                return Err(integer_formatter_error(
                    self.id(),
                    "next object ID edit parsing requires text",
                ));
            };
            let value = if input.trim() == "?" {
                self.resolver
                    .as_deref()
                    .and_then(|resolver| {
                        resolver.next_object_id(handler_record_context(&invocation.context))
                    })
                    .unwrap_or(2048)
            } else {
                let parsed = parse_delphi_integer(input, self.id())?;
                u32::try_from(parsed).map_err(|_| {
                    integer_formatter_error(
                        self.id(),
                        "next object ID edit value must fit an unsigned 32-bit integer",
                    )
                })?
            };
            return Ok(HandlerOutput::Value(FieldValue::UInt(u64::from(value))));
        }
        let value = u32::try_from(callback_integer(
            invocation.value.ok_or_else(|| {
                integer_formatter_error(self.id(), "next object ID formatting requires an integer")
            })?,
            self.id(),
        )?)
        .map_err(|_| {
            integer_formatter_error(
                self.id(),
                "next object ID value must fit an unsigned 32-bit integer",
            )
        })?;
        let hexadecimal = format!("{value:08X}");
        let text = match invocation.phase {
            HandlerPhase::Display | HandlerPhase::SortKey => hexadecimal,
            HandlerPhase::EditValue => format!("${hexadecimal}"),
            HandlerPhase::Summary | HandlerPhase::NativeValue | HandlerPhase::Validation => {
                String::new()
            }
            _ => return Ok(HandlerOutput::None),
        };
        Ok(HandlerOutput::Text(text))
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

struct WorldspaceOffsetColumnCount;

impl SemanticHandler for WorldspaceOffsetColumnCount {
    fn id(&self) -> &'static str {
        "array.worldspace_offset_columns"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::ArrayCount {
            return Ok(HandlerOutput::None);
        }
        let Some(min_x) = worldspace_bound_x(&invocation, Signature(*b"NAM0"), self.id())? else {
            return Ok(HandlerOutput::Integer(0));
        };
        let Some(max_x) = worldspace_bound_x(&invocation, Signature(*b"NAM9"), self.id())? else {
            return Ok(HandlerOutput::Integer(0));
        };
        let count = max_x.wrapping_sub(min_x).wrapping_add(1) as u32;
        Ok(HandlerOutput::Integer(i64::from(count)))
    }
}

struct OblivionPathGridConnectionCount;

impl SemanticHandler for OblivionPathGridConnectionCount {
    fn id(&self) -> &'static str {
        "array.oblivion_path_grid_connections"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::ArrayCount {
            return Ok(HandlerOutput::None);
        }
        let point_index =
            invocation
                .array_indices
                .last()
                .copied()
                .ok_or_else(|| SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: "path-grid connection count requires its outer array index".to_owned(),
                })?;
        let points = source_subrecord_bytes(&invocation, Signature(*b"PGRP"), self.id())?
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "path-grid record has no preceding PGRP points".to_owned(),
            })?;
        let count_offset = point_index
            .checked_mul(16)
            .and_then(|offset| offset.checked_add(12))
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "path-grid point offset overflowed".to_owned(),
            })?;
        let count = points
            .get(count_offset)
            .copied()
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: format!(
                    "path-grid point {point_index} has no connection-count byte in PGRP"
                ),
            })?;
        Ok(HandlerOutput::Integer(i64::from(count)))
    }
}

struct StarSlotArrayElementInclusion;

impl SemanticHandler for StarSlotArrayElementInclusion {
    fn id(&self) -> &'static str {
        "array.star_slot_matches_outer_index"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::ArrayElementInclusion {
            return Ok(HandlerOutput::None);
        }
        let outer_index =
            invocation
                .array_indices
                .last()
                .copied()
                .ok_or_else(|| SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: "star-slot inclusion requires its outer array index".to_owned(),
                })?;
        let bytes = match invocation.value {
            Some(FieldValue::Bytes(bytes)) => bytes.as_ref(),
            _ => {
                return Err(SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: "star-slot inclusion requires payload bytes".to_owned(),
                });
            }
        };
        let star_slot = bytes
            .get(..4)
            .and_then(|bytes| bytes.try_into().ok())
            .map(u32::from_le_bytes)
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "star-slot element is shorter than four bytes".to_owned(),
            })?;
        let include = usize::try_from(star_slot).is_ok_and(|slot| slot == outer_index);
        Ok(HandlerOutput::Integer(i64::from(include)))
    }
}

struct StarSlotDefaultValue;

impl SemanticHandler for StarSlotDefaultValue {
    fn id(&self) -> &'static str {
        "default.star_slot_outer_index"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::DefaultValue {
            return Ok(HandlerOutput::None);
        }
        let outer_index_position =
            invocation
                .array_indices
                .len()
                .checked_sub(2)
                .ok_or_else(|| SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: "star-slot default requires outer and inner array indices".to_owned(),
                })?;
        let outer_index = invocation.array_indices[outer_index_position];
        let value = i64::try_from(outer_index).map_err(|_| SemanticError::Handler {
            handler: self.id().to_owned(),
            message: "star-slot outer array index exceeds i64".to_owned(),
        })?;
        Ok(HandlerOutput::Value(FieldValue::Enumeration {
            value,
            name: None,
        }))
    }
}

fn worldspace_bound_x(
    invocation: &HandlerInvocation<'_>,
    signature: Signature,
    handler: &str,
) -> Result<Option<i32>> {
    let Some(bytes) = source_subrecord_bytes(invocation, signature, handler)? else {
        return Ok(None);
    };
    let raw = bytes
        .get(..4)
        .and_then(|bytes| bytes.try_into().ok())
        .map(f32::from_le_bytes)
        .ok_or_else(|| SemanticError::Handler {
            handler: handler.to_owned(),
            message: format!("{signature} worldspace bound is shorter than four bytes"),
        })?;
    if !raw.is_finite() {
        return Ok(None);
    }
    let scale = if invocation.context.game == SchemaGame::Starfield {
        1.0 / 100.0
    } else {
        1.0 / 4096.0
    };
    let scaled = float_from_raw(f64::from(raw), scale, 6).round_ties_even();
    if scaled < f64::from(i32::MIN) || scaled > f64::from(i32::MAX) {
        return Err(SemanticError::Handler {
            handler: handler.to_owned(),
            message: format!("{signature} worldspace X bound exceeds i32"),
        });
    }
    Ok(Some(scaled as i32))
}

struct SelectCtdaParameter {
    table: Option<Arc<ConditionFunctionTable>>,
}

struct SelectCoedOwner {
    resolver: Option<Arc<dyn FormLinkResolver>>,
}

struct SelectNoteData;

struct SelectSoundDescriptorData;

struct SelectAudioEffectData;

struct SelectStarfieldComponentData;

struct SelectStarfieldComponentDat2;

struct SelectOblivionObmeEfitParameter;

struct SelectOblivionObmeEfixParameter;

struct SelectGameSettingValue;

struct SelectLegacyNoteVoice;

struct SelectPackageInputValue;

struct SelectMorrowindGlobalValue;

struct SelectOblivionMiscActorValue;

struct SelectPerkEffectData;

struct SelectPerkEntryPointData;

struct SelectPerkEpf3;

struct SelectRecordFlag;

struct SelectBoneModifierType;

struct SelectEmptyString;

impl SemanticHandler for SelectCtdaParameter {
    fn id(&self) -> &'static str {
        "select.ctda_parameter"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::UnionSelection {
            return Ok(HandlerOutput::None);
        }
        let table = self.table.as_deref().ok_or_else(|| {
            ctda_parameter_error("schema package has no condition-function table")
        })?;
        let bytes = match invocation.value {
            Some(FieldValue::Bytes(value)) => value.as_ref(),
            _ => {
                return Err(ctda_parameter_error(
                    "union selection requires payload bytes",
                ))
            }
        };
        let parameter = invocation
            .context
            .configuration
            .get("parameter")
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .filter(|value| (1..=3).contains(value))
            .ok_or_else(|| ctda_parameter_error("parameter must be 1, 2, or 3"))?;
        let function_index = read_configured_integer(
            invocation.context.configuration,
            "function",
            bytes,
            self.id(),
        )?;
        let Ok(function_index) = i32::try_from(function_index) else {
            return Ok(HandlerOutput::Integer(0));
        };
        let function = table
            .functions()
            .binary_search_by_key(&function_index, |function| function.index())
            .ok()
            .and_then(|index| table.functions().get(index));
        let Some(function) = function else {
            return Ok(HandlerOutput::Integer(0));
        };
        let parameter_index = parameter - 1;
        let mut variant = function.parameter_variants()[parameter_index];
        if function.aliasable_parameters()[parameter_index] {
            let flags = read_configured_integer(
                invocation.context.configuration,
                "type",
                bytes,
                self.id(),
            )?;
            let run_on = read_configured_integer(
                invocation.context.configuration,
                "run_on",
                bytes,
                self.id(),
            )?;
            variant = select_ctda_flag_variant(
                invocation.context.game,
                function.name(),
                flags,
                run_on,
                variant,
                table,
            )?;
        }
        Ok(HandlerOutput::Integer(i64::from(variant)))
    }
}

impl SemanticHandler for SelectCoedOwner {
    fn id(&self) -> &'static str {
        "select.coed_owner"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::UnionSelection {
            return Ok(HandlerOutput::None);
        }
        let bytes = match invocation.value {
            Some(FieldValue::Bytes(value)) => value.as_ref(),
            _ => {
                return Err(coed_owner_error(
                    "owner union selection requires payload bytes",
                ))
            }
        };
        let offset = invocation
            .context
            .configuration
            .get("owner_offset")
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| coed_owner_error("owner_offset is missing or invalid"))?;
        let end = offset
            .checked_add(4)
            .ok_or_else(|| coed_owner_error("owner offset overflowed"))?;
        let form_id = bytes
            .get(offset..end)
            .and_then(|value| value.try_into().ok())
            .map(u32::from_le_bytes)
            .map(FormId)
            .ok_or_else(|| coed_owner_error("owner FormID exceeds the callback payload"))?;
        let Some(record) = self.resolver.as_deref().and_then(|resolver| {
            resolver.resolve_form_id(
                handler_record_context(&invocation.context),
                form_id,
                &[Signature(*b"NPC_"), Signature(*b"FACT")],
            )
        }) else {
            return Ok(HandlerOutput::Integer(0));
        };
        let signature = record.signature().ok_or_else(|| {
            coed_owner_error("FormID resolver did not supply the owner record signature")
        })?;
        let selected = if signature == Signature(*b"NPC_") {
            1
        } else if signature == Signature(*b"FACT") {
            2
        } else {
            0
        };
        Ok(HandlerOutput::Integer(selected))
    }
}

impl SemanticHandler for SelectNoteData {
    fn id(&self) -> &'static str {
        "select.note_data"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::UnionSelection {
            return Ok(HandlerOutput::None);
        }
        let Some(bytes) = source_subrecord_bytes(&invocation, Signature(*b"DNAM"), self.id())?
        else {
            return Ok(HandlerOutput::Integer(0));
        };
        let value = *bytes.first().ok_or_else(|| SemanticError::Handler {
            handler: self.id().to_owned(),
            message: "NOTE DNAM payload is empty".to_owned(),
        })?;
        let selected = match value {
            0 => 1,
            1 => 2,
            3 => 3,
            _ => 0,
        };
        Ok(HandlerOutput::Integer(selected))
    }
}

impl SemanticHandler for SelectSoundDescriptorData {
    fn id(&self) -> &'static str {
        "select.sound_descriptor_data"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::UnionSelection {
            return Ok(HandlerOutput::None);
        }
        let Some(bytes) = source_subrecord_bytes(&invocation, Signature(*b"CNAM"), self.id())?
        else {
            return Ok(HandlerOutput::Integer(0));
        };
        let value = bytes
            .get(..4)
            .and_then(|bytes| bytes.try_into().ok())
            .map(u32::from_le_bytes)
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "SNDR CNAM payload is shorter than four bytes".to_owned(),
            })?;
        Ok(HandlerOutput::Integer(i64::from(value == 0xED15_7AE3)))
    }
}

impl SemanticHandler for SelectAudioEffectData {
    fn id(&self) -> &'static str {
        "select.audio_effect_data"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::UnionSelection {
            return Ok(HandlerOutput::None);
        }
        let Some(bytes) = source_subrecord_bytes(&invocation, Signature(*b"KNAM"), self.id())?
        else {
            return Ok(HandlerOutput::Integer(0));
        };
        let value = bytes
            .get(..4)
            .and_then(|bytes| bytes.try_into().ok())
            .map(u32::from_le_bytes)
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "AECH KNAM payload is shorter than four bytes".to_owned(),
            })?;
        let selected = match value {
            0xEF57_5F7F => 1,
            0x1883_7B4F => 2,
            _ => 0,
        };
        Ok(HandlerOutput::Integer(selected))
    }
}

impl SemanticHandler for SelectStarfieldComponentData {
    fn id(&self) -> &'static str {
        "select.starfield_component_data"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::UnionSelection {
            return Ok(HandlerOutput::None);
        }
        let component = source_subrecord_text(&invocation, Signature(*b"BFCB"), self.id())?;
        let selected = match component {
            Some("BGSStarDataComponent_Component") => 1,
            Some("BGSOrbitedDataComponent_Component") => 2,
            Some("BGSOrbitalDataComponent_Component") => 3,
            Some("BGSBlockEditorMetaData_Component") => 4,
            Some("UniqueOverlayList_Component") => 5,
            Some("UniquePatternPlacementInfo_Component") => 6,
            _ => 0,
        };
        Ok(HandlerOutput::Integer(selected))
    }
}

impl SemanticHandler for SelectStarfieldComponentDat2 {
    fn id(&self) -> &'static str {
        "select.starfield_component_dat2"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::UnionSelection {
            return Ok(HandlerOutput::None);
        }
        let component = source_subrecord_text(&invocation, Signature(*b"BFCB"), self.id())?;
        Ok(HandlerOutput::Integer(i64::from(
            component == Some("BlockHeightAdjustment_Component"),
        )))
    }
}

impl SemanticHandler for SelectOblivionObmeEfitParameter {
    fn id(&self) -> &'static str {
        "select.oblivion_obme_efit_parameter"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        select_oblivion_obme_parameter(invocation, 4, self.id())
    }
}

impl SemanticHandler for SelectOblivionObmeEfixParameter {
    fn id(&self) -> &'static str {
        "select.oblivion_obme_efix_parameter"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        select_oblivion_obme_parameter(invocation, 5, self.id())
    }
}

fn select_oblivion_obme_parameter(
    invocation: HandlerInvocation<'_>,
    parameter_offset: usize,
    handler: &str,
) -> Result<HandlerOutput> {
    if invocation.phase != HandlerPhase::UnionSelection {
        return Ok(HandlerOutput::None);
    }
    let selected = source_subrecord_bytes(&invocation, Signature(*b"EFME"), handler)?
        .and_then(|bytes| bytes.get(parameter_offset))
        .copied()
        .unwrap_or_default();
    Ok(HandlerOutput::Integer(i64::from(selected)))
}

impl SemanticHandler for SelectGameSettingValue {
    fn id(&self) -> &'static str {
        "select.game_setting_value"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::UnionSelection {
            return Ok(HandlerOutput::None);
        }
        let editor_id =
            source_subrecord_text(&invocation, Signature(*b"EDID"), self.id())?.unwrap_or_default();
        let selected = match editor_id.as_bytes().first() {
            Some(b's') => 0,
            Some(b'f') => 2,
            Some(b'b') => 3,
            Some(b'u') => 4,
            _ => 1,
        };
        Ok(HandlerOutput::Integer(selected))
    }
}

impl SemanticHandler for SelectLegacyNoteVoice {
    fn id(&self) -> &'static str {
        "select.legacy_note_voice"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::UnionSelection {
            return Ok(HandlerOutput::None);
        }
        let note_type = source_subrecord_bytes(&invocation, Signature(*b"DATA"), self.id())?
            .and_then(|bytes| bytes.first())
            .copied();
        Ok(HandlerOutput::Integer(i64::from(note_type == Some(3))))
    }
}

impl SemanticHandler for SelectPackageInputValue {
    fn id(&self) -> &'static str {
        "select.package_input_value"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::UnionSelection {
            return Ok(HandlerOutput::None);
        }
        let input_type =
            source_subrecord_text(&invocation, Signature(*b"ANAM"), self.id())?.unwrap_or_default();
        let selected = match input_type {
            "Bool" => 1,
            "Int" => 2,
            "Float" | "ObjectList" => 3,
            _ => 0,
        };
        Ok(HandlerOutput::Integer(selected))
    }
}

impl SemanticHandler for SelectMorrowindGlobalValue {
    fn id(&self) -> &'static str {
        "select.morrowind_global_value"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::UnionSelection {
            return Ok(HandlerOutput::None);
        }
        let value_type = source_subrecord_bytes(&invocation, Signature(*b"FNAM"), self.id())?
            .and_then(|bytes| bytes.first())
            .copied();
        let selected = match value_type {
            Some(b'l') => 1,
            Some(b'f') => 2,
            _ => 0,
        };
        Ok(HandlerOutput::Integer(selected))
    }
}

impl SemanticHandler for SelectOblivionMiscActorValue {
    fn id(&self) -> &'static str {
        "select.oblivion_misc_actor_value"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::UnionSelection {
            return Ok(HandlerOutput::None);
        }
        let flags = invocation
            .source_record
            .map(|record| record.header.flags)
            .or_else(|| invocation.source_writable_record.map(|record| record.flags))
            .unwrap_or_else(RecordFlags::empty);
        Ok(HandlerOutput::Integer(i64::from(
            flags.bits() & 0x0000_00c0 == 0x0000_00c0,
        )))
    }
}

impl SemanticHandler for SelectPerkEffectData {
    fn id(&self) -> &'static str {
        "select.perk_effect_data"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::UnionSelection {
            return Ok(HandlerOutput::None);
        }
        let effect_type = source_subrecord_bytes(&invocation, Signature(*b"PRKE"), self.id())?
            .and_then(|bytes| bytes.first())
            .copied()
            .unwrap_or_default();
        Ok(HandlerOutput::Integer(i64::from(effect_type)))
    }
}

impl SemanticHandler for SelectPerkEntryPointData {
    fn id(&self) -> &'static str {
        "select.perk_entry_point_data"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::UnionSelection {
            return Ok(HandlerOutput::None);
        }
        let mut selected = source_subrecord_bytes(&invocation, Signature(*b"EPFT"), self.id())?
            .and_then(|bytes| bytes.first())
            .copied()
            .unwrap_or_default();
        if selected == 2 {
            let function = source_subrecord_bytes(&invocation, Signature(*b"DATA"), self.id())?
                .and_then(|bytes| bytes.get(1))
                .copied()
                .unwrap_or_default();
            selected = match invocation.context.game {
                SchemaGame::Fallout3 | SchemaGame::FalloutNv if function == 5 => 5,
                SchemaGame::SkyrimLe
                | SchemaGame::SkyrimSe
                | SchemaGame::SkyrimVr
                | SchemaGame::Fallout4
                | SchemaGame::Fallout4Vr
                | SchemaGame::Fallout76
                | SchemaGame::Starfield
                    if matches!(function, 5 | 12 | 13 | 14) =>
                {
                    8
                }
                _ => selected,
            };
        }
        Ok(HandlerOutput::Integer(i64::from(selected)))
    }
}

impl SemanticHandler for SelectPerkEpf3 {
    fn id(&self) -> &'static str {
        "select.perk_epf3"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::UnionSelection {
            return Ok(HandlerOutput::None);
        }
        let parameter_type = source_subrecord_bytes(&invocation, Signature(*b"EPFT"), self.id())?
            .and_then(|bytes| bytes.first())
            .copied();
        Ok(HandlerOutput::Integer(i64::from(parameter_type == Some(8))))
    }
}

impl SemanticHandler for SelectRecordFlag {
    fn id(&self) -> &'static str {
        "select.record_flag"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::UnionSelection {
            return Ok(HandlerOutput::None);
        }
        let mask = invocation
            .context
            .configuration
            .get("mask")
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .filter(|value| *value != 0)
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "record-flag selector requires a nonzero u32 mask".to_owned(),
            })?;
        let flags = invocation
            .source_record
            .map(|record| record.header.flags)
            .or_else(|| invocation.source_writable_record.map(|record| record.flags))
            .unwrap_or_else(RecordFlags::empty);
        Ok(HandlerOutput::Integer(i64::from(
            flags.bits() & mask == mask,
        )))
    }
}

impl SemanticHandler for SelectBoneModifierType {
    fn id(&self) -> &'static str {
        "select.bone_modifier_type"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::UnionSelection {
            return Ok(HandlerOutput::None);
        }
        let path = configured_text(self.id(), invocation.context.configuration, "path")?;
        let type_name = invocation
            .value_scope
            .and_then(|scope| scoped_string(scope, path))
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: format!("bone-modifier selector cannot resolve sibling field {path}"),
            })?;
        let selected = if type_name.eq_ignore_ascii_case("LookAtChain") {
            1
        } else if type_name.eq_ignore_ascii_case("MorphDriver") {
            2
        } else if type_name.eq_ignore_ascii_case("PoseDeformer") {
            3
        } else if type_name.eq_ignore_ascii_case("SpringBone") {
            4
        } else {
            0
        };
        Ok(HandlerOutput::Integer(selected))
    }
}

impl SemanticHandler for SelectEmptyString {
    fn id(&self) -> &'static str {
        "select.empty_string"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::UnionSelection {
            return Ok(HandlerOutput::None);
        }
        let path = configured_text(self.id(), invocation.context.configuration, "path")?;
        let value = invocation
            .value_scope
            .and_then(|scope| scoped_string(scope, path))
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: format!("string selector cannot resolve sibling field {path}"),
            })?;
        Ok(HandlerOutput::Integer(i64::from(value.is_empty())))
    }
}

fn source_subrecord_text<'a>(
    invocation: &HandlerInvocation<'a>,
    signature: Signature,
    handler: &str,
) -> Result<Option<&'a str>> {
    let Some(bytes) = source_subrecord_bytes(invocation, signature, handler)? else {
        return Ok(None);
    };
    let bytes = bytes.split(|byte| *byte == 0).next().unwrap_or_default();
    std::str::from_utf8(bytes)
        .map(Some)
        .map_err(|error| SemanticError::Handler {
            handler: handler.to_owned(),
            message: format!("{signature} text is not valid UTF-8: {error}"),
        })
}

fn source_subrecord_bytes<'a>(
    invocation: &HandlerInvocation<'a>,
    signature: Signature,
    handler: &str,
) -> Result<Option<&'a [u8]>> {
    if let Some(record) = invocation.source_record {
        let subrecords = record.subrecords()?;
        let end = invocation
            .source_subrecord_index
            .unwrap_or(subrecords.len())
            .min(subrecords.len());
        return Ok(subrecords[..end]
            .iter()
            .rev()
            .find(|subrecord| subrecord.signature == signature)
            .map(bethkit_core::SubRecord::as_bytes));
    }
    if let Some(record) = invocation.source_writable_record {
        let end = invocation
            .source_subrecord_index
            .unwrap_or(record.subrecords.len())
            .min(record.subrecords.len());
        return Ok(record.subrecords[..end]
            .iter()
            .rev()
            .find(|subrecord| subrecord.signature == signature)
            .map(|subrecord| subrecord.data.as_slice()));
    }
    Err(SemanticError::Handler {
        handler: handler.to_owned(),
        message: "subrecord-dependent union requires its source record".to_owned(),
    })
}

fn select_ctda_flag_variant(
    game: SchemaGame,
    function_name: &str,
    flags: i64,
    run_on: i64,
    base_variant: u16,
    table: &ConditionFunctionTable,
) -> Result<u16> {
    let Some(alias_variant) = table.alias_variant() else {
        return Ok(base_variant);
    };
    let packdata_variant = table
        .packdata_variant()
        .ok_or_else(|| ctda_parameter_error("condition table has no packdata variant"))?;
    if flags & 0x02 != 0 {
        let preserves_current_package = matches!(
            game,
            SchemaGame::Fallout4
                | SchemaGame::Fallout4Vr
                | SchemaGame::Fallout76
                | SchemaGame::Starfield
        ) && run_on == 5
            && function_name == "GetIsCurrentPackage";
        if !preserves_current_package
            || (game == SchemaGame::Starfield && run_on == 14 && function_name == "GetDistance")
        {
            return Ok(alias_variant);
        }
        return Ok(base_variant);
    }
    if flags & 0x08 != 0 {
        return Ok(packdata_variant);
    }
    Ok(base_variant)
}

fn read_configured_integer(
    configuration: &serde_json::Value,
    prefix: &str,
    bytes: &[u8],
    handler: &str,
) -> Result<i64> {
    let offset = configuration
        .get(format!("{prefix}_offset"))
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| configured_integer_error(handler, prefix, "offset is missing"))?;
    let width = configuration
        .get(format!("{prefix}_width"))
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .filter(|value| (1..=8).contains(value))
        .ok_or_else(|| configured_integer_error(handler, prefix, "width is invalid"))?;
    let signed = configuration
        .get(format!("{prefix}_signed"))
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| configured_integer_error(handler, prefix, "signed flag is missing"))?;
    let byte_order = configuration
        .get(format!("{prefix}_byte_order"))
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| configured_integer_error(handler, prefix, "byte order is missing"))?;
    let end = offset
        .checked_add(width)
        .ok_or_else(|| configured_integer_error(handler, prefix, "range overflowed"))?;
    let data = bytes.get(offset..end).ok_or_else(|| {
        configured_integer_error(handler, prefix, "range exceeds the callback payload")
    })?;
    let mut storage = [0_u8; 8];
    let value = match byte_order {
        "little" => {
            storage[..width].copy_from_slice(data);
            u64::from_le_bytes(storage)
        }
        "big" => {
            storage[8 - width..].copy_from_slice(data);
            u64::from_be_bytes(storage)
        }
        _ => {
            return Err(configured_integer_error(
                handler,
                prefix,
                "byte order is invalid",
            ))
        }
    };
    if !signed {
        return i64::try_from(value)
            .map_err(|_| configured_integer_error(handler, prefix, "value exceeds i64"));
    }
    let bits = width * 8;
    let signed_value = if bits == 64 || value & (1_u64 << (bits - 1)) == 0 {
        value
    } else {
        value | (!0_u64 << bits)
    };
    Ok(signed_value as i64)
}

fn configured_integer_error(handler: &str, prefix: &str, message: &str) -> SemanticError {
    SemanticError::Handler {
        handler: handler.to_owned(),
        message: format!("{prefix} integer {message}"),
    }
}

fn ctda_parameter_error(message: impl Into<String>) -> SemanticError {
    SemanticError::Handler {
        handler: "select.ctda_parameter".to_owned(),
        message: message.into(),
    }
}

fn coed_owner_error(message: impl Into<String>) -> SemanticError {
    SemanticError::Handler {
        handler: "select.coed_owner".to_owned(),
        message: message.into(),
    }
}

struct FormatCtdaCondition {
    table: Option<Arc<ConditionFunctionTable>>,
    resolver: Option<Arc<dyn FormLinkResolver>>,
}

struct FormatBlueprintComponentSummary {
    resolver: Option<Arc<dyn FormLinkResolver>>,
}

struct ResolveBlueprintComponent;

struct FormatIndexedRecordName {
    resolver: Option<Arc<dyn FormLinkResolver>>,
}

struct ResolveIndexedRecord {
    resolver: Option<Arc<dyn FormLinkResolver>>,
}

struct FormatAvmdEntryReference {
    resolver: Option<Arc<dyn FormLinkResolver>>,
}

struct ResolveAvmdEntryReference {
    resolver: Option<Arc<dyn FormLinkResolver>>,
}

struct FormatSnapNodeSummary {
    resolver: Option<Arc<dyn FormLinkResolver>>,
}

struct ResolveSnapNode {
    resolver: Option<Arc<dyn FormLinkResolver>>,
}

struct ResolveLocalArrayElement;

impl SemanticHandler for FormatIndexedRecordName {
    fn id(&self) -> &'static str {
        "format.indexed_record_name"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::Display {
            return Ok(HandlerOutput::None);
        }
        let index = configured_text(self.id(), invocation.context.configuration, "index")?;
        let key = record_index_key(
            invocation.value.ok_or_else(|| {
                indexed_record_error(self.id(), "index lookup requires a scalar value")
            })?,
            self.id(),
        )?;
        let Some(record) = self.resolver.as_deref().and_then(|resolver| {
            resolver.resolve_record_index(handler_record_context(&invocation.context), index, &key)
        }) else {
            return Ok(HandlerOutput::None);
        };
        Ok(HandlerOutput::Text(record.link().value().to_owned()))
    }
}

impl SemanticHandler for ResolveIndexedRecord {
    fn id(&self) -> &'static str {
        "resolve.indexed_record"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::ReferenceResolution {
            return Ok(HandlerOutput::None);
        }
        let index = configured_text(self.id(), invocation.context.configuration, "index")?;
        let key = record_index_key(
            invocation.value.ok_or_else(|| {
                indexed_record_error(self.id(), "index lookup requires a scalar value")
            })?,
            self.id(),
        )?;
        let Some(record) = self.resolver.as_deref().and_then(|resolver| {
            resolver.resolve_record_index(handler_record_context(&invocation.context), index, &key)
        }) else {
            return Ok(HandlerOutput::None);
        };
        Ok(HandlerOutput::Link(SemanticLink::Record {
            form_id: record.form_id(),
        }))
    }
}

impl SemanticHandler for FormatAvmdEntryReference {
    fn id(&self) -> &'static str {
        "format.avmd_entry_reference"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::Display {
            return Ok(HandlerOutput::None);
        }
        let Some(record) = resolve_avmd_entry_reference(&invocation, self.resolver.as_deref())?
        else {
            return Ok(HandlerOutput::None);
        };
        Ok(HandlerOutput::Text(record.link().value().to_owned()))
    }
}

impl SemanticHandler for ResolveAvmdEntryReference {
    fn id(&self) -> &'static str {
        "resolve.avmd_entry_reference"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::ReferenceResolution {
            return Ok(HandlerOutput::None);
        }
        let Some(record) = resolve_avmd_entry_reference(&invocation, self.resolver.as_deref())?
        else {
            return Ok(HandlerOutput::None);
        };
        Ok(HandlerOutput::Link(SemanticLink::Record {
            form_id: record.form_id(),
        }))
    }
}

impl SemanticHandler for FormatSnapNodeSummary {
    fn id(&self) -> &'static str {
        "format.snap_node_summary"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::Display {
            return Ok(HandlerOutput::None);
        }
        let Some(node) = resolve_snap_node(&invocation, self.resolver.as_deref())? else {
            return Ok(HandlerOutput::None);
        };
        if node.summary().is_empty() {
            return Ok(HandlerOutput::None);
        }
        let text = if node.containing_record_name().is_empty() {
            node.summary().to_owned()
        } else {
            format!("{} on {}", node.summary(), node.containing_record_name())
        };
        Ok(HandlerOutput::Text(text))
    }
}

impl SemanticHandler for ResolveSnapNode {
    fn id(&self) -> &'static str {
        "resolve.snap_node"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::ReferenceResolution {
            return Ok(HandlerOutput::None);
        }
        let Some(node) = resolve_snap_node(&invocation, self.resolver.as_deref())? else {
            return Ok(HandlerOutput::None);
        };
        Ok(HandlerOutput::Link(SemanticLink::ExternalElement {
            record_form_id: node.record_form_id(),
            path: node.path().to_owned(),
            array_indices: node.array_indices().to_vec(),
        }))
    }
}

fn resolve_snap_node(
    invocation: &HandlerInvocation<'_>,
    resolver: Option<&dyn FormLinkResolver>,
) -> Result<Option<ResolvedElementInfo>> {
    let handler = if invocation.phase == HandlerPhase::ReferenceResolution {
        "resolve.snap_node"
    } else {
        "format.snap_node_summary"
    };
    let node_id = i64::try_from(callback_integer(
        invocation
            .value
            .ok_or_else(|| indexed_record_error(handler, "snap node requires an integer"))?,
        handler,
    )?)
    .map_err(|_| indexed_record_error(handler, "snap node identifier exceeds i64"))?;
    let reference = match snap_node_reference_path(handler, &invocation.context)? {
        Some(path) => {
            let scope = invocation.value_scope.ok_or_else(|| {
                indexed_record_error(handler, "linked snap node requires sibling scope")
            })?;
            let active = scoped_named_value_by_name(scope, "Bethkit Active Repeat Occurrence")
                .map_or(scope, |field| &field.value);
            let Some((form_id, _)) = scoped_form_id(active, path) else {
                return Ok(None);
            };
            Some(form_id)
        }
        None => None,
    };
    Ok(resolver.and_then(|resolver| {
        resolver.resolve_snap_node(
            handler_record_context(&invocation.context),
            reference,
            node_id,
        )
    }))
}

fn snap_node_reference_path(
    handler: &str,
    context: &HandlerContext<'_>,
) -> Result<Option<&'static str>> {
    match context.binding.path.as_str() {
        "CELL/17:Ship Blueprint Snap Links/payload/element/2:Parent Node" => Ok(Some(
            "CELL/17:Ship Blueprint Snap Links/payload/element/0:Parent Reference",
        )),
        "CELL/17:Ship Blueprint Snap Links/payload/element/3:Linked Node" => Ok(Some(
            "CELL/17:Ship Blueprint Snap Links/payload/element/1:Linked Reference",
        )),
        "REFR/25:Snap Links/payload/element/1:Links/element/0:Parent Node" => Ok(None),
        "REFR/25:Snap Links/payload/element/1:Links/element/1:Linked Node" => Ok(Some(
            "REFR/25:Snap Links/payload/element/0:Linked Reference",
        )),
        path => Err(indexed_record_error(
            handler,
            format!("unsupported snap node binding path {path:?}"),
        )),
    }
}

struct FormatNavmeshEdge {
    resolver: Option<Arc<dyn FormLinkResolver>>,
}

struct FormatNavmeshVertex;

struct ResolveNavmeshEdge {
    resolver: Option<Arc<dyn FormLinkResolver>>,
}

struct NavmeshEdgeState {
    external: bool,
    triangle_index: Option<usize>,
    triangles_path: String,
    local_triangle_count: usize,
    external_navmesh: Option<ResolvedNavmeshInfo>,
}

impl SemanticHandler for FormatNavmeshVertex {
    fn id(&self) -> &'static str {
        "format.navmesh_vertex"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase == HandlerPhase::ParseEditValue {
            let Some(FieldValue::String(input)) = invocation.value else {
                return Err(indexed_record_error(
                    self.id(),
                    "vertex edit parsing requires text",
                ));
            };
            let parsed = parse_delphi_integer(input, self.id()).unwrap_or(0);
            return Ok(HandlerOutput::Value(FieldValue::Int(parsed)));
        }
        let raw = callback_integer(
            invocation
                .value
                .ok_or_else(|| indexed_record_error(self.id(), "vertex requires an integer"))?,
            self.id(),
        )?;
        let vertex = navmesh_vertex(&invocation, raw)?;
        let text = match invocation.phase {
            HandlerPhase::Display | HandlerPhase::Summary => {
                let mut text = raw.to_string();
                if let Some(vertex) = vertex {
                    let [x, y, z] = navmesh_vertex_coordinates(vertex)?;
                    text.push_str(&format!(
                        " ({}, {}, {})",
                        format_navmesh_vertex_float(x),
                        format_navmesh_vertex_float(y),
                        format_navmesh_vertex_float(z)
                    ));
                }
                text
            }
            HandlerPhase::SortKey => {
                let Some(vertex) = vertex else {
                    return Ok(HandlerOutput::Text(format!("{:04X}", raw as i32 as u32)));
                };
                if matches!(
                    invocation.context.game,
                    SchemaGame::SkyrimLe | SchemaGame::SkyrimSe | SchemaGame::SkyrimVr
                ) {
                    navmesh_vertex_coordinates(vertex)?
                        .map(format_navmesh_vertex_sort_key)
                        .join("|")
                } else {
                    String::new()
                }
            }
            HandlerPhase::EditValue => raw.to_string(),
            _ => return Ok(HandlerOutput::None),
        };
        Ok(HandlerOutput::Text(text))
    }
}

impl SemanticHandler for FormatNavmeshEdge {
    fn id(&self) -> &'static str {
        "format.navmesh_edge"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase == HandlerPhase::ParseEditValue {
            let input = match invocation.value {
                Some(FieldValue::String(value)) => value.trim(),
                _ => {
                    return Err(indexed_record_error(
                        self.id(),
                        "edge edit parsing requires text",
                    ));
                }
            };
            let parsed = if input.is_empty() || input.eq_ignore_ascii_case("None") {
                -1
            } else {
                parse_delphi_integer(input, self.id()).unwrap_or(0)
            };
            return Ok(HandlerOutput::Value(FieldValue::Int(parsed)));
        }
        let raw = callback_integer(
            invocation
                .value
                .ok_or_else(|| indexed_record_error(self.id(), "edge requires an integer"))?,
            self.id(),
        )?;
        let state = navmesh_edge_state(&invocation, self.resolver.as_deref())?;
        let text = match invocation.phase {
            HandlerPhase::Display | HandlerPhase::Summary => {
                format_navmesh_edge_display(raw, state.as_ref())
            }
            HandlerPhase::SortKey => format_navmesh_edge_sort_key(
                raw,
                state.as_ref(),
                self.resolver.as_deref(),
                &invocation,
            ),
            HandlerPhase::EditValue => {
                if raw < 0 {
                    String::new()
                } else {
                    raw.to_string()
                }
            }
            _ => return Ok(HandlerOutput::None),
        };
        Ok(HandlerOutput::Text(text))
    }
}

fn navmesh_vertex<'a>(
    invocation: &'a HandlerInvocation<'_>,
    raw: i128,
) -> Result<Option<&'a FieldValue<'static>>> {
    let vertices_path = navmesh_vertices_path(&invocation.context.binding.path)?;
    let Some(index) = usize::try_from(raw).ok() else {
        return Ok(None);
    };
    let Some(vertices) = invocation
        .value_scope
        .and_then(|scope| scoped_named_value(scope, &vertices_path))
    else {
        return Ok(None);
    };
    let FieldValue::Array(values) = &vertices.value else {
        return Err(indexed_record_error(
            "format.navmesh_vertex",
            "Vertices target is not an array",
        ));
    };
    Ok(values.get(index))
}

fn navmesh_vertices_path(path: &str) -> Result<String> {
    let components: Vec<&str> = path.split('/').collect();
    let Some(index) = components.iter().position(|component| {
        component
            .split_once(':')
            .is_some_and(|(_, name)| name == "Triangles")
    }) else {
        return Err(indexed_record_error(
            "format.navmesh_vertex",
            format!("NAVM vertex binding path has no Triangles array: {path:?}"),
        ));
    };
    let mut vertices = components[..index].to_vec();
    vertices.push("2:Vertices");
    Ok(vertices.join("/"))
}

fn navmesh_vertex_coordinates(vertex: &FieldValue<'_>) -> Result<[f64; 3]> {
    let FieldValue::Struct(fields) = vertex else {
        return Err(indexed_record_error(
            "format.navmesh_vertex",
            "selected Vertex is not a struct",
        ));
    };
    let coordinate = |name: &str| {
        condition_field(fields, &[name])
            .and_then(|field| match field.value {
                FieldValue::Float(value) => Some(value),
                _ => None,
            })
            .ok_or_else(|| {
                indexed_record_error(
                    "format.navmesh_vertex",
                    format!("selected Vertex has no floating-point {name} coordinate"),
                )
            })
    };
    Ok([coordinate("X")?, coordinate("Y")?, coordinate("Z")?])
}

fn format_navmesh_vertex_float(value: f64) -> String {
    if value.is_nan() {
        "NaN".to_owned()
    } else if value.is_infinite() && value.is_sign_positive() {
        "Inf".to_owned()
    } else if value.is_infinite() {
        "-Inf".to_owned()
    } else if value == f64::from(f32::MAX) {
        "Default".to_owned()
    } else if value == f64::from(-f32::MAX) {
        "Min".to_owned()
    } else {
        format!("{value:.6}")
    }
}

fn format_navmesh_vertex_sort_key(value: f64) -> String {
    if value.is_nan() {
        return " ".repeat(40);
    }
    if value.is_infinite() {
        return if value.is_sign_positive() {
            "+".repeat(40)
        } else {
            "-".repeat(40)
        };
    }
    if value == f64::from(f32::MAX) {
        return format!("+{}", "9".repeat(39));
    }
    if value == f64::from(-f32::MAX) {
        return format!("-{}", "9".repeat(39));
    }
    let value = if value.abs() <= f64::from(f32::from_bits(1)) {
        0.0
    } else {
        value
    };
    let magnitude = format!("{:.6}", value.abs());
    let padding = "0".repeat(39_usize.saturating_sub(magnitude.len()));
    let sign = if value < 0.0 { '-' } else { '+' };
    format!("{sign}{padding}{magnitude}")
}

impl SemanticHandler for ResolveNavmeshEdge {
    fn id(&self) -> &'static str {
        "resolve.navmesh_edge"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::ReferenceResolution {
            return Ok(HandlerOutput::None);
        }
        if callback_integer(
            invocation
                .value
                .ok_or_else(|| indexed_record_error(self.id(), "edge requires an integer"))?,
            self.id(),
        )? < 0
        {
            return Ok(HandlerOutput::None);
        }
        let Some(state) = navmesh_edge_state(&invocation, self.resolver.as_deref())? else {
            return Ok(HandlerOutput::None);
        };
        let Some(triangle_index) = state.triangle_index else {
            return Ok(HandlerOutput::None);
        };
        if state.external {
            let Some(navmesh) = state.external_navmesh else {
                return Ok(HandlerOutput::None);
            };
            if triangle_index >= navmesh.triangle_count() {
                return Ok(HandlerOutput::None);
            }
            return Ok(HandlerOutput::Link(SemanticLink::ExternalElement {
                record_form_id: navmesh.record_form_id(),
                path: format!("{}/element", navmesh.triangles_path()),
                array_indices: vec![triangle_index],
            }));
        }
        if triangle_index >= state.local_triangle_count {
            return Ok(HandlerOutput::None);
        }
        Ok(HandlerOutput::Link(SemanticLink::Element {
            path: format!("{}/element", state.triangles_path),
            array_indices: vec![triangle_index],
        }))
    }
}

fn format_navmesh_edge_display(raw: i128, state: Option<&NavmeshEdgeState>) -> String {
    if raw < 0 {
        return "None".to_owned();
    }
    let mut text = raw.to_string();
    let Some((triangle_index, navmesh)) =
        state.and_then(|state| Some((state.triangle_index?, state.external_navmesh.as_ref()?)))
    else {
        return text;
    };
    text.push_str(&format!(" (#{triangle_index} in {})", navmesh.name()));
    text
}

fn format_navmesh_edge_sort_key(
    raw: i128,
    state: Option<&NavmeshEdgeState>,
    resolver: Option<&dyn FormLinkResolver>,
    invocation: &HandlerInvocation<'_>,
) -> String {
    let raw_hex = format!("{:04X}", raw as i32 as u32);
    let Some(state) = state else {
        return format!("00000000{raw_hex}");
    };
    if state.external {
        let Some(navmesh) = state.external_navmesh.as_ref() else {
            return format!("00000000{raw_hex}");
        };
        let Some(triangle_index) = state.triangle_index else {
            return format!("00000000{raw_hex}");
        };
        return format!("{:08X}{:04X}", navmesh.load_order_form_id(), triangle_index);
    }
    let source = resolver
        .and_then(|resolver| {
            resolver.source_load_order_form_id(handler_record_context(&invocation.context))
        })
        .unwrap_or(0);
    format!("{source:08X}{raw_hex}")
}

fn navmesh_edge_state(
    invocation: &HandlerInvocation<'_>,
    resolver: Option<&dyn FormLinkResolver>,
) -> Result<Option<NavmeshEdgeState>> {
    let edge = navmesh_edge_number(&invocation.context.binding.path)?;
    let (triangles_path, edge_links_path) =
        navmesh_edge_array_paths(&invocation.context.binding.path)?;
    let Some(scope) = invocation.value_scope else {
        return Ok(None);
    };
    let Some(active_index) = invocation.array_indices.last().copied() else {
        return Ok(None);
    };
    let Some(triangles) = scoped_named_value(scope, &triangles_path) else {
        return Ok(None);
    };
    let FieldValue::Array(triangle_values) = &triangles.value else {
        return Err(indexed_record_error(
            "format.navmesh_edge",
            "Triangles target is not an array",
        ));
    };
    let Some(FieldValue::Struct(active_triangle)) = triangle_values.get(active_index) else {
        return Ok(None);
    };
    let flags = condition_field(active_triangle, &["Flags"])
        .map(|field| callback_integer(&field.value, "format.navmesh_edge"))
        .transpose()?
        .unwrap_or(0);
    let external = flags & (1_i128 << edge) != 0;
    let raw = callback_integer(
        invocation.value.ok_or_else(|| {
            indexed_record_error("format.navmesh_edge", "edge requires an integer")
        })?,
        "format.navmesh_edge",
    )?;
    if !external {
        return Ok(Some(NavmeshEdgeState {
            external: false,
            triangle_index: usize::try_from(raw).ok(),
            triangles_path,
            local_triangle_count: triangle_values.len(),
            external_navmesh: None,
        }));
    }
    let Some(edge_link_index) = usize::try_from(raw).ok() else {
        return Ok(Some(NavmeshEdgeState {
            external: true,
            triangle_index: None,
            triangles_path,
            local_triangle_count: triangle_values.len(),
            external_navmesh: None,
        }));
    };
    let edge_link =
        scoped_named_value(scope, &edge_links_path).and_then(|field| match &field.value {
            FieldValue::Array(values) => values.get(edge_link_index),
            _ => None,
        });
    let Some(FieldValue::Struct(fields)) = edge_link else {
        return Ok(Some(NavmeshEdgeState {
            external: true,
            triangle_index: None,
            triangles_path,
            local_triangle_count: triangle_values.len(),
            external_navmesh: None,
        }));
    };
    let triangle_index = condition_field(fields, &["Triangle Index", "Triangle"])
        .map(|field| callback_integer(&field.value, "format.navmesh_edge"))
        .transpose()?
        .and_then(|value| usize::try_from(value).ok());
    let navmesh_form_id = condition_field(fields, &["Mesh", "Navmesh"])
        .map(|field| callback_form_id(&field.value, "format.navmesh_edge"))
        .transpose()?;
    let external_navmesh = navmesh_form_id.and_then(|form_id| {
        resolver.and_then(|resolver| {
            resolver.resolve_navmesh(handler_record_context(&invocation.context), form_id)
        })
    });
    Ok(Some(NavmeshEdgeState {
        external: true,
        triangle_index,
        triangles_path,
        local_triangle_count: triangle_values.len(),
        external_navmesh,
    }))
}

fn navmesh_edge_number(path: &str) -> Result<u32> {
    let name = path.rsplit('/').next().unwrap_or(path);
    match name.split_once(':').map_or(name, |(_, name)| name) {
        "Edge 0-1" => Ok(0),
        "Edge 1-2" => Ok(1),
        "Edge 2-0" => Ok(2),
        _ => Err(indexed_record_error(
            "format.navmesh_edge",
            format!("unsupported NAVM edge binding path {path:?}"),
        )),
    }
}

fn navmesh_edge_array_paths(path: &str) -> Result<(String, String)> {
    let components: Vec<&str> = path.split('/').collect();
    let Some(index) = components.iter().position(|component| {
        component
            .split_once(':')
            .is_some_and(|(_, name)| name == "Triangles")
    }) else {
        return Err(indexed_record_error(
            "format.navmesh_edge",
            format!("NAVM edge binding path has no Triangles array: {path:?}"),
        ));
    };
    let triangles_path = components[..=index].join("/");
    let mut edge_links = components[..index].to_vec();
    edge_links.push("4:Edge Links");
    Ok((triangles_path, edge_links.join("/")))
}

impl SemanticHandler for ResolveLocalArrayElement {
    fn id(&self) -> &'static str {
        "resolve.local_array_element"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::ReferenceResolution {
            return Ok(HandlerOutput::None);
        }
        let index = callback_integer(
            invocation.value.ok_or_else(|| {
                indexed_record_error(self.id(), "array element link requires an integer")
            })?,
            self.id(),
        )?;
        if index < 0 {
            return Ok(HandlerOutput::None);
        }
        let index = usize::try_from(index)
            .map_err(|_| indexed_record_error(self.id(), "array element index exceeds usize"))?;
        let (array_path, element_path) = local_array_target_paths(self.id(), &invocation.context)?;
        let scope = invocation.value_scope.ok_or_else(|| {
            indexed_record_error(self.id(), "array element link requires record scope")
        })?;
        let Some(array) = scoped_named_value(scope, &array_path) else {
            return Ok(HandlerOutput::None);
        };
        let FieldValue::Array(elements) = &array.value else {
            return Err(indexed_record_error(
                self.id(),
                format!("configured target {array_path:?} is not an array"),
            ));
        };
        if index >= elements.len() {
            return Ok(HandlerOutput::None);
        }
        Ok(HandlerOutput::Link(SemanticLink::Element {
            path: element_path,
            array_indices: vec![index],
        }))
    }
}

fn local_array_target_paths(
    handler: &str,
    context: &HandlerContext<'_>,
) -> Result<(String, String)> {
    if let Some(array_path) =
        configured_optional_text(handler, context.configuration, "target_path")?
    {
        return Ok((array_path.to_owned(), format!("{array_path}/element")));
    }
    let source_container = configured_text(handler, context.configuration, "source_container")?;
    let target_segment = configured_text(handler, context.configuration, "target_segment")?;
    let components: Vec<&str> = context.binding.path.split('/').collect();
    let Some(source_index) = components.iter().position(|component| {
        component
            .split_once(':')
            .is_some_and(|(_, name)| name == source_container)
    }) else {
        return Err(indexed_record_error(
            handler,
            format!(
                "binding path {:?} has no {source_container:?} container",
                context.binding.path
            ),
        ));
    };
    let mut array_path = components[..source_index].join("/");
    if !array_path.is_empty() {
        array_path.push('/');
    }
    array_path.push_str(target_segment);
    let element_path = format!("{array_path}/element");
    Ok((array_path, element_path))
}

fn resolve_avmd_entry_reference(
    invocation: &HandlerInvocation<'_>,
    resolver: Option<&dyn FormLinkResolver>,
) -> Result<Option<IndexedRecordInfo>> {
    let handler = if invocation.phase == HandlerPhase::ReferenceResolution {
        "resolve.avmd_entry_reference"
    } else {
        "format.avmd_entry_reference"
    };
    let mode = configured_text(handler, invocation.context.configuration, "mode")?;
    let type_path = configured_text(handler, invocation.context.configuration, "type_path")?;
    let value_path = configured_text(handler, invocation.context.configuration, "value_path")?;
    let scope = invocation
        .value_scope
        .ok_or_else(|| indexed_record_error(handler, "AVMD lookup requires record scope"))?;
    let record_type = scoped_named_value(scope, type_path)
        .map(|field| &field.value)
        .ok_or_else(|| indexed_record_error(handler, "AVMD Type field is missing"))?;
    if !matches!(
        record_type,
        FieldValue::Enumeration {
            name: Some(name),
            ..
        } if name == "Complex Group"
    ) {
        return Ok(None);
    }
    let active = scoped_named_value_by_name(scope, "Bethkit Active Repeat Occurrence")
        .map(|field| &field.value)
        .ok_or_else(|| indexed_record_error(handler, "active AVMD entry scope is missing"))?;
    let value = invocation
        .value
        .ok_or_else(|| indexed_record_error(handler, "AVMD lookup requires a string value"))?;
    let FieldValue::String(value) = value else {
        return Err(indexed_record_error(
            handler,
            "AVMD lookup value is not a string",
        ));
    };
    let candidates: Vec<(&str, String)> = match mode {
        "name" => {
            if scoped_named_value(active, value_path).is_some() {
                return Ok(None);
            }
            ["simple_group", "complex_group", "modulation"]
                .into_iter()
                .map(|index| (index, value.to_string()))
                .collect()
        }
        "value" => {
            let Some((prefix, key)) = value.split_once('_') else {
                return Ok(None);
            };
            if key.is_empty() {
                return Ok(None);
            }
            let index = match prefix {
                "SimpleGroup" => "simple_group",
                "ComplexGroup" => "complex_group",
                "Modulation" => "modulation",
                _ => return Ok(None),
            };
            vec![(index, key.to_owned())]
        }
        _ => {
            return Err(indexed_record_error(
                handler,
                format!("unknown AVMD entry mode {mode:?}"),
            ))
        }
    };
    let source = handler_record_context(&invocation.context);
    Ok(resolver.and_then(|resolver| {
        candidates.into_iter().find_map(|(index, key)| {
            resolver.resolve_record_index(source, index, &RecordIndexKeyValue::Text(key))
        })
    }))
}

fn record_index_key(value: &FieldValue<'_>, handler: &str) -> Result<RecordIndexKeyValue> {
    match value {
        FieldValue::String(value) => Ok(RecordIndexKeyValue::Text(value.to_string())),
        FieldValue::Int(value) => Ok(RecordIndexKeyValue::Integer(*value)),
        FieldValue::UInt(value) => i64::try_from(*value)
            .map(RecordIndexKeyValue::Integer)
            .map_err(|_| indexed_record_error(handler, "index key exceeds i64")),
        _ => Err(indexed_record_error(
            handler,
            "index key is not a string or integer",
        )),
    }
}

fn indexed_record_error(handler: &str, message: impl Into<String>) -> SemanticError {
    SemanticError::Handler {
        handler: handler.to_owned(),
        message: message.into(),
    }
}

impl SemanticHandler for FormatBlueprintComponentSummary {
    fn id(&self) -> &'static str {
        "format.blueprint_component_summary"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::Display {
            return Ok(HandlerOutput::None);
        }
        let value = invocation.value.ok_or_else(|| {
            blueprint_component_error(self.id(), "component formatting requires an integer")
        })?;
        let part_id = callback_integer(value, self.id())?;
        if part_id < 0 {
            return Ok(HandlerOutput::None);
        }
        let Some(scope) = invocation.value_scope else {
            return Err(blueprint_component_error(
                self.id(),
                "component formatting requires the decoded record scope",
            ));
        };
        let Some(found) = find_blueprint_component(scope, part_id) else {
            return Ok(HandlerOutput::None);
        };
        let summary = format_blueprint_component(
            found.fields,
            self.resolver.as_deref(),
            handler_record_context(&invocation.context),
        )?;
        if summary.is_empty() {
            Ok(HandlerOutput::None)
        } else {
            Ok(HandlerOutput::Text(summary))
        }
    }
}

impl SemanticHandler for ResolveBlueprintComponent {
    fn id(&self) -> &'static str {
        "resolve.blueprint_component"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::ReferenceResolution {
            return Ok(HandlerOutput::None);
        }
        let value = invocation.value.ok_or_else(|| {
            blueprint_component_error(self.id(), "component resolution requires an integer")
        })?;
        let part_id = callback_integer(value, self.id())?;
        if part_id < 0 {
            return Ok(HandlerOutput::None);
        }
        let Some(scope) = invocation.value_scope else {
            return Err(blueprint_component_error(
                self.id(),
                "component resolution requires the decoded record scope",
            ));
        };
        let Some(found) = find_blueprint_component(scope, part_id) else {
            return Ok(HandlerOutput::None);
        };
        Ok(HandlerOutput::Link(SemanticLink::Element {
            path: found.path,
            array_indices: found.array_indices,
        }))
    }
}

struct BlueprintComponent<'a> {
    fields: &'a [crate::NamedValue<'static>],
    path: String,
    array_indices: Vec<usize>,
}

fn find_blueprint_component<'a>(
    scope: &'a FieldValue<'static>,
    part_id: i128,
) -> Option<BlueprintComponent<'a>> {
    fn visit<'a>(
        value: &'a FieldValue<'static>,
        part_id: i128,
        array_indices: &mut Vec<usize>,
    ) -> Option<BlueprintComponent<'a>> {
        match value {
            FieldValue::Struct(fields) => {
                let part = condition_field(fields, &["Part ID"]);
                let is_blueprint_item = condition_field(fields, &["Base Item"]).is_some()
                    && condition_field(fields, &["Position/Rotation"]).is_some();
                if is_blueprint_item
                    && part.is_some_and(|field| {
                        callback_integer(&field.value, "resolve.blueprint_component")
                            .is_ok_and(|value| value == part_id)
                    })
                {
                    let part = part?;
                    let path = part
                        .path
                        .rsplit_once('/')
                        .map_or_else(|| part.path.clone(), |(parent, _)| parent.to_owned());
                    return Some(BlueprintComponent {
                        fields,
                        path,
                        array_indices: array_indices.clone(),
                    });
                }
                fields
                    .iter()
                    .find_map(|field| visit(&field.value, part_id, array_indices))
            }
            FieldValue::Array(values) => {
                for (index, value) in values.iter().enumerate() {
                    array_indices.push(index);
                    let found = visit(value, part_id, array_indices);
                    array_indices.pop();
                    if found.is_some() {
                        return found;
                    }
                }
                None
            }
            _ => None,
        }
    }

    visit(scope, part_id, &mut Vec::new())
}

fn format_blueprint_component(
    fields: &[crate::NamedValue<'_>],
    resolver: Option<&dyn FormLinkResolver>,
    source: HandlerRecordContext,
) -> Result<String> {
    let part_id = condition_integer(fields, "Part ID")?;
    let mut members = vec![format!("[{part_id}]")];
    if let Some(base_item) = condition_field(fields, &["Base Item"]) {
        members.push(format_blueprint_form_id(
            &base_item.value,
            resolver,
            source,
        )?);
    }
    if let Some(position_rotation) = condition_field(fields, &["Position/Rotation"]) {
        members.push(format_blueprint_position_rotation(
            &position_rotation.value,
        )?);
    }
    if let Some(construction) = condition_field(fields, &["Construction Object"]) {
        if !matches!(
            construction.value,
            FieldValue::FormId {
                value: FormId::NULL,
                ..
            }
        ) {
            members.push(format_blueprint_form_id(
                &construction.value,
                resolver,
                source,
            )?);
        }
    }
    Ok(members
        .into_iter()
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join(" "))
}

fn format_blueprint_form_id(
    value: &FieldValue<'_>,
    resolver: Option<&dyn FormLinkResolver>,
    source: HandlerRecordContext,
) -> Result<String> {
    let FieldValue::FormId { value, targets } = value else {
        return Err(blueprint_component_error(
            "format.blueprint_component_summary",
            "blueprint item link is not a FormID",
        ));
    };
    Ok(resolver
        .and_then(|resolver| resolver.resolve_form_id(source, *value, targets))
        .map_or_else(
            || format!("{:08X}", value.0),
            |record| record.value().to_owned(),
        ))
}

fn format_blueprint_position_rotation(value: &FieldValue<'_>) -> Result<String> {
    let fields = struct_fields(value, "format.blueprint_component_summary")?;
    let position = condition_field(fields, &["Position"]).ok_or_else(|| {
        blueprint_component_error(
            "format.blueprint_component_summary",
            "blueprint item has no Position field",
        )
    })?;
    let rotation = condition_field(fields, &["Rotation"]).ok_or_else(|| {
        blueprint_component_error(
            "format.blueprint_component_summary",
            "blueprint item has no Rotation field",
        )
    })?;
    Ok(format!(
        "Pos:{} Rot:{}",
        format_vec3(&position.value, Some(6))?,
        format_vec3(&rotation.value, Some(4))?
    ))
}

fn blueprint_component_error(handler: &str, message: impl Into<String>) -> SemanticError {
    SemanticError::Handler {
        handler: handler.to_owned(),
        message: message.into(),
    }
}

impl SemanticHandler for FormatCtdaCondition {
    fn id(&self) -> &'static str {
        "format.ctda_condition"
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
        ) {
            return Ok(HandlerOutput::None);
        }
        let value = invocation
            .value
            .ok_or_else(|| ctda_condition_error("condition formatting requires a value"))?;
        let text = format_ctda_condition(
            value,
            invocation.context.game,
            self.table.as_deref(),
            self.resolver.as_deref(),
            handler_record_context(&invocation.context),
        )?;
        Ok(HandlerOutput::Text(text))
    }
}

fn format_ctda_condition(
    value: &FieldValue<'_>,
    game: SchemaGame,
    table: Option<&ConditionFunctionTable>,
    resolver: Option<&dyn FormLinkResolver>,
    source: HandlerRecordContext,
) -> Result<String> {
    let fields = find_ctda_fields(value)
        .ok_or_else(|| ctda_condition_error("condition value has no CTDA payload"))?;
    let condition_type = condition_integer(fields, "Type")?;
    let function = condition_field(fields, &["Function"])
        .ok_or_else(|| ctda_condition_error("condition has no Function field"))?;
    let function_index = callback_integer(&function.value, "format.ctda_condition")
        .ok()
        .and_then(|value| i32::try_from(value).ok());
    let function_text = condition_function_text(&function.value, function_index, game, table)?;

    let run_on = condition_field(fields, &["Run On"]);
    let reference = condition_field(fields, &["Reference"]);
    let mut text = String::new();
    if let (Some(run_on), Some(reference)) = (run_on, reference) {
        if condition_parameter_is_visible(reference) {
            let mut run_on_value =
                i64::try_from(callback_integer(&run_on.value, "format.ctda_condition")?)
                    .map_err(|_| ctda_condition_error("Run On value exceeds i64"))?;
            if game == SchemaGame::FalloutNv && matches!(function_index, Some(106 | 285)) {
                run_on_value = 0;
            }
            if run_on_value == 2 {
                text.push('(');
                text.push_str(&condition_value_text(&reference.value, resolver, source)?);
                text.push(')');
            } else {
                text.push_str(
                    &condition_value_text(&run_on.value, resolver, source)?.replace(' ', ""),
                );
            }
        }
    }
    if text.is_empty() {
        text.push_str(if condition_type & 0x02 == 0 {
            "Subject"
        } else {
            "Target"
        });
    }
    text.push('.');
    text.push_str(&function_text);

    if let Some(parameter_1) = condition_field(fields, &["Parameter #1", "Param #1"]) {
        if condition_parameter_is_visible(parameter_1) {
            text.push('(');
            text.push_str(&condition_value_text(&parameter_1.value, resolver, source)?);
            if let Some(parameter_2) = condition_field(fields, &["Parameter #2", "Param #2"]) {
                if condition_parameter_is_visible(parameter_2) {
                    text.push_str(", ");
                    text.push_str(&condition_value_text(&parameter_2.value, resolver, source)?);
                }
            }
            text.push(')');
        }
    }

    text.push_str(match condition_type & 0xE0 {
        0x00 => " = ",
        0x20 => " <> ",
        0x40 => " > ",
        0x60 => " >= ",
        0x80 => " < ",
        0xA0 => " <= ",
        _ => "",
    });
    let comparison = condition_field(fields, &["Comparison Value"])
        .ok_or_else(|| ctda_condition_error("condition has no Comparison Value field"))?;
    text.push_str(&condition_value_text(&comparison.value, resolver, source)?);

    if let Some((index, count)) = condition_repeat_position(value) {
        if index + 1 < count {
            text.push_str(if condition_type & 0x01 == 0 {
                " AND"
            } else {
                " OR"
            });
        }
    }
    Ok(text)
}

fn find_ctda_fields<'a>(value: &'a FieldValue<'a>) -> Option<&'a [crate::NamedValue<'a>]> {
    let FieldValue::Struct(fields) = value else {
        return None;
    };
    if condition_field(fields, &["Type"]).is_some()
        && condition_field(fields, &["Function"]).is_some()
    {
        return Some(fields);
    }
    fields
        .iter()
        .find_map(|field| find_ctda_fields(&field.value))
}

fn condition_field<'a>(
    fields: &'a [crate::NamedValue<'a>],
    names: &[&str],
) -> Option<&'a crate::NamedValue<'a>> {
    fields
        .iter()
        .find(|field| names.iter().any(|name| field.name == *name))
}

fn condition_integer(fields: &[crate::NamedValue<'_>], name: &str) -> Result<i64> {
    let field = condition_field(fields, &[name])
        .ok_or_else(|| ctda_condition_error(format!("condition has no {name} field")))?;
    i64::try_from(callback_integer(&field.value, "format.ctda_condition")?)
        .map_err(|_| ctda_condition_error(format!("{name} value exceeds i64")))
}

fn condition_function_text(
    value: &FieldValue<'_>,
    index: Option<i32>,
    game: SchemaGame,
    table: Option<&ConditionFunctionTable>,
) -> Result<String> {
    if let FieldValue::String(value) = value {
        return Ok(value.to_string());
    }
    let index = index.ok_or_else(|| ctda_condition_error("Function is not an integer"))?;
    if let Some(function) = table.and_then(|table| {
        table
            .functions()
            .binary_search_by_key(&index, |function| function.index())
            .ok()
            .and_then(|position| table.functions().get(position))
    }) {
        return Ok(function.name().to_owned());
    }
    if game == SchemaGame::FalloutNv {
        Ok(format!("<Unknown: {index}>"))
    } else {
        Ok(index.to_string())
    }
}

fn condition_parameter_is_visible(value: &crate::NamedValue<'_>) -> bool {
    let selected = value.effective_path.as_deref().unwrap_or(&value.path);
    let selected = selected.to_ascii_lowercase();
    !selected.ends_with(":none")
        && !selected.ends_with(":unused")
        && !selected.contains("(unused)")
        && !matches!(value.value, FieldValue::Absent)
}

fn condition_value_text(
    value: &FieldValue<'_>,
    resolver: Option<&dyn FormLinkResolver>,
    source: HandlerRecordContext,
) -> Result<String> {
    match value {
        FieldValue::String(value) => Ok(value.to_string()),
        FieldValue::Int(value) => Ok(value.to_string()),
        FieldValue::UInt(value) => Ok(value.to_string()),
        FieldValue::Float(value) => Ok(format_delphi_general(*value, 6)),
        FieldValue::Enumeration {
            name: Some(name), ..
        } => Ok(name.clone()),
        FieldValue::Enumeration { value, name: None } => Ok(value.to_string()),
        FieldValue::Flags { value, .. } => Ok(value.to_string()),
        FieldValue::FormId { value, targets } => Ok(resolver
            .and_then(|resolver| resolver.resolve_form_id(source, *value, targets))
            .map_or_else(
                || format!("{:08X}", value.0),
                |record| record.value().to_owned(),
            )),
        _ => Err(ctda_condition_error(
            "condition field is not a scalar summary value",
        )),
    }
}

fn condition_repeat_position(value: &FieldValue<'_>) -> Option<(usize, usize)> {
    let FieldValue::Struct(fields) = value else {
        return None;
    };
    let marker = condition_field(fields, &["Bethkit Repeat Position"])?;
    let FieldValue::Struct(position) = &marker.value else {
        return None;
    };
    let index = condition_field(position, &["Index"])
        .and_then(|field| callback_integer(&field.value, "format.ctda_condition").ok())
        .and_then(|value| usize::try_from(value).ok())?;
    let count = condition_field(position, &["Count"])
        .and_then(|field| callback_integer(&field.value, "format.ctda_condition").ok())
        .and_then(|value| usize::try_from(value).ok())?;
    Some((index, count))
}

fn ctda_condition_error(message: impl Into<String>) -> SemanticError {
    SemanticError::Handler {
        handler: "format.ctda_condition".to_owned(),
        message: message.into(),
    }
}

struct CtdaFunctionFormatter {
    table: Option<Arc<ConditionFunctionTable>>,
}

impl SemanticHandler for CtdaFunctionFormatter {
    fn id(&self) -> &'static str {
        "format.ctda_function"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        let table = self
            .table
            .as_deref()
            .ok_or_else(|| ctda_function_error("schema package has no condition-function table"))?;
        if invocation.phase == HandlerPhase::ParseEditValue {
            let Some(FieldValue::String(input)) = invocation.value else {
                return Err(ctda_function_error(
                    "condition-function edit parsing requires text",
                ));
            };
            let value = table
                .functions()
                .iter()
                .find(|function| function.name().eq_ignore_ascii_case(input))
                .map_or_else(
                    || parse_delphi_integer(input, self.id()),
                    |function| Ok(i64::from(function.index())),
                )?;
            return Ok(HandlerOutput::Value(FieldValue::Int(value)));
        }

        let value = i64::try_from(callback_integer(
            invocation
                .value
                .ok_or_else(|| ctda_function_error("formatting requires an integer value"))?,
            self.id(),
        )?)
        .map_err(|_| ctda_function_error("condition-function value exceeds i64"))?;
        let function = i32::try_from(value)
            .ok()
            .and_then(|value| {
                table
                    .functions()
                    .binary_search_by_key(&value, |function| function.index())
                    .ok()
            })
            .and_then(|index| table.functions().get(index));
        let text = match invocation.phase {
            HandlerPhase::Display => function.map_or_else(
                || format!("<Unknown: {value}>"),
                |function| function.name().to_owned(),
            ),
            HandlerPhase::Summary => function.map_or_else(
                || {
                    if invocation.context.game == SchemaGame::FalloutNv {
                        format!("<Unknown: {value}>")
                    } else {
                        value.to_string()
                    }
                },
                |function| function.name().to_owned(),
            ),
            HandlerPhase::EditValue => {
                function.map_or_else(|| value.to_string(), |function| function.name().to_owned())
            }
            HandlerPhase::SortKey => format!("{:08X}", value as u64),
            HandlerPhase::NativeValue => String::new(),
            HandlerPhase::Validation => {
                function.map_or_else(|| format!("<Unknown: {value}>"), |_| String::new())
            }
            _ => return Ok(HandlerOutput::None),
        };
        Ok(HandlerOutput::Text(text))
    }
}

fn ctda_function_error(message: impl Into<String>) -> SemanticError {
    SemanticError::Handler {
        handler: "format.ctda_function".to_owned(),
        message: message.into(),
    }
}

struct LegacyCtdaAfterLoad;

impl SemanticHandler for LegacyCtdaAfterLoad {
    fn id(&self) -> &'static str {
        "migrate.legacy_ctda_run_on"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::AfterLoad {
            return Ok(HandlerOutput::None);
        }
        if !matches!(
            invocation.context.game,
            SchemaGame::Fallout3 | SchemaGame::FalloutNv
        ) {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "legacy CTDA migration is only valid for Fallout 3 and Fallout NV"
                    .to_owned(),
            });
        }
        let record = invocation
            .source_writable_record
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "legacy CTDA migration requires a writable record".to_owned(),
            })?;
        let index = invocation
            .source_subrecord_index
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "legacy CTDA migration requires a source subrecord".to_owned(),
            })?;
        let subrecord = record
            .subrecords
            .get(index)
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: format!("source subrecord index {index} is out of bounds"),
            })?;
        if subrecord.signature != Signature(*b"CTDA") {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: format!(
                    "legacy CTDA migration received {} instead of CTDA",
                    subrecord.signature
                ),
            });
        }
        let Some(type_flags) = subrecord.data.first().copied() else {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "legacy CTDA payload is empty".to_owned(),
            });
        };
        if type_flags & 0x02 == 0 {
            return Ok(HandlerOutput::None);
        }
        if subrecord.data.len() != 20 && subrecord.data.len() < 24 {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: format!(
                    "legacy CTDA payload has unsupported size {}",
                    subrecord.data.len()
                ),
            });
        }
        let mut data = subrecord.data.clone();
        if data.len() == 20 {
            data.resize(28, 0);
        }
        data[0] &= !0x02;
        data[20..24].copy_from_slice(&1_u32.to_le_bytes());
        Ok(HandlerOutput::SubrecordPayload(data))
    }
}

struct LegacyEfitAfterLoad {
    resolver: Option<Arc<dyn FormLinkResolver>>,
}

impl SemanticHandler for LegacyEfitAfterLoad {
    fn id(&self) -> &'static str {
        "migrate.legacy_efit_actor_value"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::AfterLoad {
            return Ok(HandlerOutput::None);
        }
        if !matches!(
            invocation.context.game,
            SchemaGame::Fallout3 | SchemaGame::FalloutNv
        ) {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "legacy EFIT migration is only valid for Fallout 3 and Fallout NV"
                    .to_owned(),
            });
        }
        let record = invocation
            .source_writable_record
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "legacy EFIT migration requires a writable record".to_owned(),
            })?;
        if record.flags.contains(RecordFlags::DELETED) {
            return Ok(HandlerOutput::None);
        }
        let index = invocation
            .source_subrecord_index
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "legacy EFIT migration requires a source subrecord".to_owned(),
            })?;
        let efit = record
            .subrecords
            .get(index)
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: format!("source subrecord index {index} is out of bounds"),
            })?;
        if efit.signature != Signature(*b"EFIT") || efit.data.len() != 20 {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: format!(
                    "legacy EFIT migration requires a 20-byte EFIT payload, got {} bytes of {}",
                    efit.data.len(),
                    efit.signature
                ),
            });
        }
        let Some(efid) = index
            .checked_sub(1)
            .and_then(|efid_index| record.subrecords.get(efid_index))
            .filter(|subrecord| subrecord.signature == Signature(*b"EFID"))
        else {
            return Ok(HandlerOutput::None);
        };
        let Ok(efid_bytes) = <[u8; 4]>::try_from(efid.data.as_slice()) else {
            return Ok(HandlerOutput::None);
        };
        let Some(actor_value) = self
            .resolver
            .as_deref()
            .and_then(|resolver| {
                resolver.resolve_form_id(
                    HandlerRecordContext::new(
                        invocation.context.record_signature,
                        invocation.context.form_id,
                        invocation.context.form_version,
                        invocation.context.game,
                    ),
                    FormId(u32::from_le_bytes(efid_bytes)),
                    &[Signature(*b"MGEF")],
                )
            })
            .and_then(|link| link.magic_effect_actor_value())
        else {
            return Ok(HandlerOutput::None);
        };
        let actor_value = i32::try_from(actor_value).map_err(|_| SemanticError::Handler {
            handler: self.id().to_owned(),
            message: format!("resolved magic-effect actor value {actor_value} exceeds i32"),
        })?;
        let bytes = actor_value.to_le_bytes();
        if efit.data[16..20] == bytes {
            return Ok(HandlerOutput::None);
        }
        let mut data = efit.data.clone();
        data[16..20].copy_from_slice(&bytes);
        Ok(HandlerOutput::SubrecordPayload(data))
    }
}

struct VerifyModernEfitAfterLoad;

impl SemanticHandler for VerifyModernEfitAfterLoad {
    fn id(&self) -> &'static str {
        "verify.inert_modern_efit_after_load"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::AfterLoad {
            return Ok(HandlerOutput::None);
        }
        if !matches!(
            invocation.context.game,
            SchemaGame::SkyrimLe
                | SchemaGame::SkyrimSe
                | SchemaGame::SkyrimVr
                | SchemaGame::Fallout4
                | SchemaGame::Fallout4Vr
                | SchemaGame::Fallout76
        ) {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "modern inert EFIT callback is not valid for this game".to_owned(),
            });
        }
        let expected_size = invocation
            .context
            .configuration
            .get("expected_payload_size")
            .and_then(serde_json::Value::as_u64)
            .and_then(|size| usize::try_from(size).ok());
        let record = invocation
            .source_writable_record
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "modern inert EFIT verifier requires a writable record".to_owned(),
            })?;
        let index = invocation
            .source_subrecord_index
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "modern inert EFIT verifier requires a source subrecord".to_owned(),
            })?;
        let efit = record
            .subrecords
            .get(index)
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: format!("source subrecord index {index} is out of bounds"),
            })?;
        if efit.signature != Signature(*b"EFIT") {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: format!(
                    "modern inert EFIT verifier expected EFIT, got {}",
                    efit.signature
                ),
            });
        }
        if let Some(expected_size) = expected_size {
            if efit.data.len() != expected_size {
                return Err(SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: format!(
                        "modern inert EFIT verifier expected {expected_size} bytes, got {}",
                        efit.data.len()
                    ),
                });
            }
        }
        Ok(HandlerOutput::None)
    }
}

struct EmbeddedScriptAfterLoad;

impl SemanticHandler for EmbeddedScriptAfterLoad {
    fn id(&self) -> &'static str {
        "migrate.embedded_script_type"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::AfterLoad {
            return Ok(HandlerOutput::None);
        }
        if !matches!(
            invocation.context.game,
            SchemaGame::Fallout3 | SchemaGame::FalloutNv
        ) {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message:
                    "embedded-script load migration is only valid for Fallout 3 and Fallout NV"
                        .to_owned(),
            });
        }
        if invocation.context.binding.callback_id != "def.after_load"
            || !invocation
                .context
                .binding
                .path
                .ends_with(":Embedded Script")
        {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "embedded-script migration requires an Embedded Script load binding"
                    .to_owned(),
            });
        }
        let anchor_path_suffix = configured_text(
            self.id(),
            invocation.context.configuration,
            "anchor_path_suffix",
        )?;
        if anchor_path_suffix != "/0:Basic Script Data" {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "embedded-script migration requires the materialized SCHR anchor"
                    .to_owned(),
            });
        }
        let record = invocation
            .source_writable_record
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "embedded-script load migration requires a writable record".to_owned(),
            })?;
        let index = invocation
            .source_subrecord_index
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "embedded-script load migration requires a source subrecord".to_owned(),
            })?;
        let schr = record
            .subrecords
            .get(index)
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: format!("source subrecord index {index} is out of bounds"),
            })?;
        if schr.signature != Signature(*b"SCHR") || schr.data.len() != 20 {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: format!(
                    "embedded-script load migration requires a 20-byte SCHR payload, got {} \
                     bytes of {}",
                    schr.data.len(),
                    schr.signature
                ),
            });
        }
        if u16::from_le_bytes([schr.data[16], schr.data[17]]) != 1 {
            return Ok(HandlerOutput::None);
        }
        let mut data = schr.data.clone();
        data[16..18].copy_from_slice(&0_u16.to_le_bytes());
        Ok(HandlerOutput::SubrecordPayload(data))
    }
}

struct OblivionEfitAfterLoad {
    resolver: Option<Arc<dyn FormLinkResolver>>,
}

impl SemanticHandler for OblivionEfitAfterLoad {
    fn id(&self) -> &'static str {
        "migrate.oblivion_efit_actor_value"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::AfterLoad {
            return Ok(HandlerOutput::None);
        }
        if invocation.context.game != SchemaGame::Oblivion {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "Oblivion EFIT migration is only valid for Oblivion".to_owned(),
            });
        }
        let record = invocation
            .source_writable_record
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "Oblivion EFIT migration requires a writable record".to_owned(),
            })?;
        let index = invocation
            .source_subrecord_index
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "Oblivion EFIT migration requires a source subrecord".to_owned(),
            })?;
        let efit = record
            .subrecords
            .get(index)
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: format!("source subrecord index {index} is out of bounds"),
            })?;
        if efit.signature != Signature(*b"EFIT") || efit.data.len() != 24 {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: format!(
                    "Oblivion EFIT migration requires a 24-byte payload, got {} bytes of {}",
                    efit.data.len(),
                    efit.signature
                ),
            });
        }
        let code = u32::from_le_bytes(
            efit.data[..4]
                .try_into()
                .expect("four-byte EFIT code slice must convert"),
        );
        let Some(effect) = self.resolver.as_deref().and_then(|resolver| {
            resolver.resolve_magic_effect_code(
                HandlerRecordContext::new(
                    invocation.context.record_signature,
                    invocation.context.form_id,
                    invocation.context.form_version,
                    invocation.context.game,
                ),
                code,
            )
        }) else {
            return Ok(HandlerOutput::None);
        };
        if effect.magic_effect_flags().unwrap_or_default() & 0x0100_0000 == 0 {
            return Ok(HandlerOutput::None);
        }
        let Some(associated_item) = effect.magic_effect_associated_item() else {
            return Ok(HandlerOutput::None);
        };
        let actor_value = i32::try_from(associated_item).map_err(|_| SemanticError::Handler {
            handler: self.id().to_owned(),
            message: format!("resolved associated item {associated_item} exceeds i32"),
        })?;
        let bytes = actor_value.to_le_bytes();
        if efit.data[20..24] == bytes {
            return Ok(HandlerOutput::None);
        }
        let mut data = efit.data.clone();
        data[20..24].copy_from_slice(&bytes);
        Ok(HandlerOutput::SubrecordPayload(data))
    }
}

struct VerifyOblivionEfixAfterLoad;

impl SemanticHandler for VerifyOblivionEfixAfterLoad {
    fn id(&self) -> &'static str {
        "verify.inert_oblivion_efix_after_load"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::AfterLoad {
            return Ok(HandlerOutput::None);
        }
        if invocation.context.game != SchemaGame::Oblivion {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "Oblivion EFIX verifier is only valid for Oblivion".to_owned(),
            });
        }
        let record = invocation
            .source_writable_record
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "Oblivion EFIX verifier requires a writable record".to_owned(),
            })?;
        let index = invocation
            .source_subrecord_index
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "Oblivion EFIX verifier requires a source subrecord".to_owned(),
            })?;
        let efix = record
            .subrecords
            .get(index)
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: format!("source subrecord index {index} is out of bounds"),
            })?;
        if efix.signature != Signature(*b"EFIX") {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: format!("Oblivion EFIX verifier received {}", efix.signature),
            });
        }
        Ok(HandlerOutput::None)
    }
}

struct RemoveOrphanedKeywordArrayAfterLoad;

impl SemanticHandler for RemoveOrphanedKeywordArrayAfterLoad {
    fn id(&self) -> &'static str {
        "migrate.remove_orphaned_keyword_array"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::AfterLoad {
            return Ok(HandlerOutput::None);
        }
        let game = invocation.context.game;
        let signature = invocation.context.record_signature;
        let supported_game = matches!(
            game,
            SchemaGame::SkyrimLe
                | SchemaGame::SkyrimSe
                | SchemaGame::SkyrimVr
                | SchemaGame::Fallout4
                | SchemaGame::Fallout4Vr
                | SchemaGame::Fallout76
        );
        let supported_record = matches!(
            signature,
            Signature([b'A', b'L', b'C', b'H'])
                | Signature([b'A', b'M', b'M', b'O'])
                | Signature([b'A', b'R', b'M', b'O'])
                | Signature([b'M', b'I', b'S', b'C'])
        ) || (signature == Signature(*b"NPC_")
            && matches!(
                game,
                SchemaGame::SkyrimLe | SchemaGame::SkyrimSe | SchemaGame::SkyrimVr
            ));
        if !supported_game || !supported_record {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: format!(
                    "orphaned keyword migration is not valid for {signature} in {game:?}"
                ),
            });
        }
        if invocation.source_subrecord_index.is_some() {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "orphaned keyword migration requires a record-level binding".to_owned(),
            });
        }
        let record = invocation
            .source_writable_record
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "orphaned keyword migration requires a writable record".to_owned(),
            })?;
        if record.flags.contains(RecordFlags::DELETED)
            || record
                .subrecords
                .iter()
                .any(|subrecord| subrecord.signature == Signature(*b"KSIZ"))
            || !record
                .subrecords
                .iter()
                .any(|subrecord| subrecord.signature == Signature(*b"KWDA"))
        {
            return Ok(HandlerOutput::None);
        }
        let path = invocation
            .context
            .configuration
            .get("keyword_path")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "orphaned keyword migration requires keyword_path".to_owned(),
            })?;
        let expected_prefix = format!("{signature}/");
        if !path.starts_with(&expected_prefix) {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: format!("keyword path {path} is outside {signature}"),
            });
        }
        Ok(HandlerOutput::Mutations(vec![HandlerMutation::Remove {
            path: path.to_owned(),
            occurrence: 0,
        }]))
    }
}

struct VerifyInertBodyTemplateAfterLoad;

impl SemanticHandler for VerifyInertBodyTemplateAfterLoad {
    fn id(&self) -> &'static str {
        "verify.inert_body_template_after_load"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::AfterLoad {
            return Ok(HandlerOutput::None);
        }
        let signature = invocation.context.record_signature;
        let valid_record = (signature == Signature(*b"ARMA")
            && matches!(
                invocation.context.game,
                SchemaGame::SkyrimLe
                    | SchemaGame::SkyrimSe
                    | SchemaGame::SkyrimVr
                    | SchemaGame::Fallout4
                    | SchemaGame::Fallout4Vr
                    | SchemaGame::Fallout76
            ))
            || (signature == Signature(*b"RACE")
                && matches!(
                    invocation.context.game,
                    SchemaGame::SkyrimLe | SchemaGame::SkyrimSe | SchemaGame::SkyrimVr
                ));
        if !valid_record || invocation.context.binding.path != signature.to_string() {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "disabled body-template migration requires a guarded ARMA or Skyrim RACE \
                          root binding"
                    .to_owned(),
            });
        }
        if invocation.source_subrecord_index.is_some()
            || invocation.source_writable_record.is_none()
        {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "disabled body-template migration requires a writable record-level source"
                    .to_owned(),
            });
        }
        Ok(HandlerOutput::None)
    }
}

struct MessageAfterLoad;

impl SemanticHandler for MessageAfterLoad {
    fn id(&self) -> &'static str {
        "migrate.message_display_time"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::AfterLoad {
            return Ok(HandlerOutput::None);
        }
        if invocation.context.record_signature != Signature(*b"MESG")
            || !matches!(
                invocation.context.game,
                SchemaGame::SkyrimLe
                    | SchemaGame::SkyrimSe
                    | SchemaGame::SkyrimVr
                    | SchemaGame::Fallout3
                    | SchemaGame::FalloutNv
                    | SchemaGame::Fallout4
                    | SchemaGame::Fallout4Vr
                    | SchemaGame::Fallout76
            )
        {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "message load migration is not valid for this record and game".to_owned(),
            });
        }
        if invocation.source_subrecord_index.is_some() {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "message load migration requires a record-level binding".to_owned(),
            });
        }
        let record = invocation
            .source_writable_record
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "message load migration requires a writable record".to_owned(),
            })?;
        if record.flags.contains(RecordFlags::DELETED) {
            return Ok(HandlerOutput::None);
        }
        let flags =
            record
                .subrecords
                .iter()
                .find(|subrecord| subrecord.signature == Signature(*b"DNAM"))
                .map(|subrecord| {
                    let bytes: [u8; 4] = subrecord.data.as_slice().try_into().map_err(|_| {
                        SemanticError::Handler {
                            handler: self.id().to_owned(),
                            message: format!(
                                "message DNAM requires a 4-byte payload, got {}",
                                subrecord.data.len()
                            ),
                        }
                    })?;
                    Ok::<u32, SemanticError>(u32::from_le_bytes(bytes))
                })
                .transpose()?;
        let has_display_time = record
            .subrecords
            .iter()
            .any(|subrecord| subrecord.signature == Signature(*b"TNAM"));
        let is_message_box = flags.unwrap_or_default() & 1 != 0;
        if is_message_box != has_display_time {
            return Ok(HandlerOutput::None);
        }
        let flags_path =
            configured_text(self.id(), invocation.context.configuration, "flags_path")?;
        let display_time_path = configured_text(
            self.id(),
            invocation.context.configuration,
            "display_time_path",
        )?;
        let expected_prefix = "MESG/";
        if !flags_path.starts_with(expected_prefix)
            || !display_time_path.starts_with(expected_prefix)
        {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "message migration paths must be inside MESG".to_owned(),
            });
        }
        let mutation = if is_message_box {
            HandlerMutation::Remove {
                path: display_time_path.to_owned(),
                occurrence: 0,
            }
        } else if let Some(flags) = flags {
            HandlerMutation::Set {
                path: flags_path.to_owned(),
                occurrence: 0,
                value: OwnedFieldValue::UInt(u64::from(flags | 1)),
            }
        } else {
            HandlerMutation::Insert {
                path: flags_path.to_owned(),
                value: OwnedFieldValue::UInt(1),
            }
        };
        Ok(HandlerOutput::Mutations(vec![mutation]))
    }
}

struct DefaultObjectArrayAfterLoad;

impl SemanticHandler for DefaultObjectArrayAfterLoad {
    fn id(&self) -> &'static str {
        "migrate.remove_empty_default_objects"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::AfterLoad {
            return Ok(HandlerOutput::None);
        }
        if invocation.context.record_signature != Signature(*b"DOBJ")
            || !matches!(
                invocation.context.game,
                SchemaGame::SkyrimLe
                    | SchemaGame::SkyrimSe
                    | SchemaGame::SkyrimVr
                    | SchemaGame::Fallout4
                    | SchemaGame::Fallout4Vr
                    | SchemaGame::Fallout76
                    | SchemaGame::Starfield
            )
        {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "default-object cleanup is not valid for this record and game".to_owned(),
            });
        }
        let record = invocation
            .source_writable_record
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "default-object cleanup requires a writable record".to_owned(),
            })?;
        let index = invocation
            .source_subrecord_index
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "default-object cleanup requires a source subrecord".to_owned(),
            })?;
        let objects = record
            .subrecords
            .get(index)
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: format!("source subrecord index {index} is out of bounds"),
            })?;
        if objects.signature != Signature(*b"DNAM") || objects.data.len() % 8 != 0 {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: format!(
                    "default-object cleanup requires an 8-byte DNAM entry array, got {} \
                     bytes of {}",
                    objects.data.len(),
                    objects.signature
                ),
            });
        }
        let mut data = Vec::with_capacity(objects.data.len());
        for entry in objects.data.chunks_exact(8) {
            let use_code = u32::from_le_bytes(
                entry[..4]
                    .try_into()
                    .expect("four-byte use code must convert"),
            );
            if use_code != 0 {
                data.extend_from_slice(entry);
            }
        }
        if data.len() == objects.data.len() {
            Ok(HandlerOutput::None)
        } else {
            Ok(HandlerOutput::SubrecordPayload(data))
        }
    }
}

struct SkyrimWeaponAfterLoad;

impl SemanticHandler for SkyrimWeaponAfterLoad {
    fn id(&self) -> &'static str {
        "migrate.skyrim_weapon_flags"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::AfterLoad {
            return Ok(HandlerOutput::None);
        }
        if invocation.context.record_signature != Signature(*b"WEAP")
            || !matches!(
                invocation.context.game,
                SchemaGame::SkyrimLe | SchemaGame::SkyrimSe | SchemaGame::SkyrimVr
            )
        {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "weapon flag cleanup is only valid for Skyrim WEAP records".to_owned(),
            });
        }
        let record = invocation
            .source_writable_record
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "weapon flag cleanup requires a writable record".to_owned(),
            })?;
        if record.flags.contains(RecordFlags::DELETED) {
            return Ok(HandlerOutput::None);
        }
        let index = invocation
            .source_subrecord_index
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "weapon flag cleanup requires a source subrecord".to_owned(),
            })?;
        let data = record
            .subrecords
            .get(index)
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: format!("source subrecord index {index} is out of bounds"),
            })?;
        if data.signature != Signature(*b"DNAM") || data.data.len() != 100 {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: format!(
                    "Skyrim weapon cleanup requires a 100-byte DNAM payload, got {} bytes of {}",
                    data.data.len(),
                    data.signature
                ),
            });
        }
        let flags = u16::from_le_bytes(
            data.data[12..14]
                .try_into()
                .expect("two-byte weapon flags must convert"),
        );
        let flags2 = u32::from_le_bytes(
            data.data[40..44]
                .try_into()
                .expect("four-byte weapon flags2 must convert"),
        );
        let normalized_flags = flags & !0x0040;
        let normalized_flags2 = flags2 & !0x0000_0100;
        if normalized_flags == flags && normalized_flags2 == flags2 {
            return Ok(HandlerOutput::None);
        }
        let mut normalized = data.data.clone();
        normalized[12..14].copy_from_slice(&normalized_flags.to_le_bytes());
        normalized[40..44].copy_from_slice(&normalized_flags2.to_le_bytes());
        Ok(HandlerOutput::SubrecordPayload(normalized))
    }
}

struct LightAfterLoad;

impl SemanticHandler for LightAfterLoad {
    fn id(&self) -> &'static str {
        "migrate.light_defaults"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::AfterLoad {
            return Ok(HandlerOutput::None);
        }
        let game = invocation.context.game;
        if invocation.context.record_signature != Signature(*b"LIGH")
            || !matches!(
                game,
                SchemaGame::Oblivion
                    | SchemaGame::Fallout3
                    | SchemaGame::FalloutNv
                    | SchemaGame::SkyrimLe
                    | SchemaGame::SkyrimSe
                    | SchemaGame::SkyrimVr
                    | SchemaGame::Fallout4
                    | SchemaGame::Fallout4Vr
                    | SchemaGame::Fallout76
            )
        {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "light default migration is not valid for this record and game".to_owned(),
            });
        }
        if invocation.source_subrecord_index.is_some() {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "light default migration requires a record-level binding".to_owned(),
            });
        }
        let record = invocation
            .source_writable_record
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "light default migration requires a writable record".to_owned(),
            })?;
        if record.flags.contains(RecordFlags::DELETED) {
            return Ok(HandlerOutput::None);
        }
        let data_path = configured_text(self.id(), invocation.context.configuration, "data_path")?;
        if !data_path.starts_with("LIGH/") {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "light DATA path must be inside LIGH".to_owned(),
            });
        }
        let mut mutations = Vec::with_capacity(2);
        if let Some(data) = record
            .subrecords
            .iter()
            .find(|subrecord| subrecord.signature == Signature(*b"DATA"))
        {
            let valid_length = match game {
                SchemaGame::Oblivion | SchemaGame::Fallout3 | SchemaGame::FalloutNv => {
                    data.data.len() == 32
                }
                SchemaGame::SkyrimLe | SchemaGame::SkyrimSe | SchemaGame::SkyrimVr => {
                    data.data.len() == 48
                }
                SchemaGame::Fallout4 | SchemaGame::Fallout4Vr => data.data.len() == 64,
                SchemaGame::Fallout76 => data.data.len() >= 64,
                _ => false,
            };
            if !valid_length {
                return Err(SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: format!(
                        "light DATA has invalid length {} for {game:?}",
                        data.data.len()
                    ),
                });
            }
            let falloff = f32::from_le_bytes(
                data.data[16..20]
                    .try_into()
                    .expect("four-byte light falloff must convert"),
            );
            let fov = f32::from_le_bytes(
                data.data[20..24]
                    .try_into()
                    .expect("four-byte light FOV must convert"),
            );
            let normalize_falloff = extended_same_value_zero(falloff);
            let normalize_fov = extended_same_value_zero(fov);
            if normalize_falloff || normalize_fov {
                let mut normalized = data.data.clone();
                if normalize_falloff {
                    normalized[16..20].copy_from_slice(&1.0_f32.to_le_bytes());
                }
                if normalize_fov {
                    normalized[20..24].copy_from_slice(&90.0_f32.to_le_bytes());
                }
                mutations.push(HandlerMutation::ReplacePayload {
                    path: data_path.to_owned(),
                    occurrence: 0,
                    data: normalized,
                });
            }
        }
        if let Some(fade_path) = invocation
            .context
            .configuration
            .get("fade_path")
            .and_then(serde_json::Value::as_str)
        {
            if !fade_path.starts_with("LIGH/") {
                return Err(SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: "light fade path must be inside LIGH".to_owned(),
                });
            }
            if !record
                .subrecords
                .iter()
                .any(|subrecord| subrecord.signature == Signature(*b"FNAM"))
            {
                mutations.push(HandlerMutation::Insert {
                    path: fade_path.to_owned(),
                    value: OwnedFieldValue::Float(1.0),
                });
            }
        }
        if mutations.is_empty() {
            Ok(HandlerOutput::None)
        } else {
            Ok(HandlerOutput::Mutations(mutations))
        }
    }
}

struct SkyrimCellAfterLoad;

impl SemanticHandler for SkyrimCellAfterLoad {
    fn id(&self) -> &'static str {
        "migrate.skyrim_cell_after_load"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::AfterLoad {
            return Ok(HandlerOutput::None);
        }
        if invocation.context.record_signature != Signature(*b"CELL")
            || invocation.context.binding.path != "CELL"
            || !matches!(
                invocation.context.game,
                SchemaGame::SkyrimLe | SchemaGame::SkyrimSe | SchemaGame::SkyrimVr
            )
        {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "Skyrim cell migration requires a guarded CELL root binding".to_owned(),
            });
        }
        if invocation.source_subrecord_index.is_some() {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "Skyrim cell migration requires a record-level binding".to_owned(),
            });
        }
        let record = invocation
            .source_writable_record
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "Skyrim cell migration requires a writable record".to_owned(),
            })?;
        if record.flags.contains(RecordFlags::DELETED) || record.subrecords.is_empty() {
            return Ok(HandlerOutput::None);
        }
        let data_path = invocation
            .context
            .configuration
            .get("data_path")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "Skyrim cell migration requires data_path".to_owned(),
            })?;
        let water_height_path = invocation
            .context
            .configuration
            .get("water_height_path")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "Skyrim cell migration requires water_height_path".to_owned(),
            })?;
        if !data_path.starts_with("CELL/") || !water_height_path.starts_with("CELL/") {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "Skyrim cell migration paths must be inside CELL".to_owned(),
            });
        }
        let data = record
            .subrecords
            .iter()
            .find(|subrecord| subrecord.signature == Signature(*b"DATA"));
        let water_height = record
            .subrecords
            .iter()
            .find(|subrecord| subrecord.signature == Signature(*b"XCLW"));
        let mut mutations = Vec::new();
        if let Some(data) = data {
            if data.data.len() == 1 {
                mutations.push(HandlerMutation::ReplacePayload {
                    path: data_path.to_owned(),
                    occurrence: 0,
                    data: vec![data.data[0], 0],
                });
            }
            if water_height.is_none() && data.data.first().is_some_and(|flags| flags & 0x02 != 0) {
                mutations.push(HandlerMutation::Insert {
                    path: water_height_path.to_owned(),
                    value: OwnedFieldValue::Float(f64::from(f32::MAX)),
                });
            }
        }
        if water_height.is_some_and(|subrecord| {
            subrecord.data.as_slice() == f32::from_bits(0xff7f_ffff).to_le_bytes()
        }) {
            mutations.push(HandlerMutation::ReplacePayload {
                path: water_height_path.to_owned(),
                occurrence: 0,
                data: 0.0_f32.to_le_bytes().to_vec(),
            });
        }
        if mutations.is_empty() {
            Ok(HandlerOutput::None)
        } else {
            Ok(HandlerOutput::Mutations(mutations))
        }
    }
}

struct FalloutCellAfterLoad;

impl SemanticHandler for FalloutCellAfterLoad {
    fn id(&self) -> &'static str {
        "migrate.fallout_cell_after_load"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::AfterLoad {
            return Ok(HandlerOutput::None);
        }
        if invocation.context.record_signature != Signature(*b"CELL")
            || invocation.context.binding.path != "CELL"
            || !matches!(
                invocation.context.game,
                SchemaGame::Fallout3 | SchemaGame::FalloutNv
            )
        {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "Fallout cell migration requires a guarded CELL root binding".to_owned(),
            });
        }
        if invocation.source_subrecord_index.is_some() {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "Fallout cell migration requires a record-level binding".to_owned(),
            });
        }
        let record = invocation
            .source_writable_record
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "Fallout cell migration requires a writable record".to_owned(),
            })?;
        if record.flags.contains(RecordFlags::DELETED) || record.subrecords.is_empty() {
            return Ok(HandlerOutput::None);
        }
        let _data_path = required_cell_path(&invocation, self.id(), "data_path")?;
        let water_height_path = required_cell_path(&invocation, self.id(), "water_height_path")?;
        let water_noise_path = required_cell_path(&invocation, self.id(), "water_noise_path")?;
        let has_water = record
            .subrecords
            .iter()
            .find(|subrecord| subrecord.signature == Signature(*b"DATA"))
            .and_then(|subrecord| subrecord.data.first())
            .is_some_and(|flags| flags & 0x02 != 0);
        if !has_water {
            return Ok(HandlerOutput::None);
        }
        let mut mutations = Vec::new();
        if !record
            .subrecords
            .iter()
            .any(|subrecord| subrecord.signature == Signature(*b"XCLW"))
        {
            mutations.push(HandlerMutation::Insert {
                path: water_height_path.to_owned(),
                value: OwnedFieldValue::Float(f64::from(f32::MAX)),
            });
        }
        if !record
            .subrecords
            .iter()
            .any(|subrecord| subrecord.signature == Signature(*b"XNAM"))
        {
            mutations.push(HandlerMutation::Insert {
                path: water_noise_path.to_owned(),
                value: OwnedFieldValue::String(String::new()),
            });
        }
        if mutations.is_empty() {
            Ok(HandlerOutput::None)
        } else {
            Ok(HandlerOutput::Mutations(mutations))
        }
    }
}

struct OblivionCellAfterLoad {
    resolver: Option<Arc<dyn FormLinkResolver>>,
}

impl SemanticHandler for OblivionCellAfterLoad {
    fn id(&self) -> &'static str {
        "migrate.oblivion_cell_after_load"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::AfterLoad {
            return Ok(HandlerOutput::None);
        }
        if invocation.context.game != SchemaGame::Oblivion
            || invocation.context.record_signature != Signature(*b"CELL")
            || invocation.context.binding.path != "CELL"
        {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "Oblivion cell migration requires a guarded CELL root binding".to_owned(),
            });
        }
        if invocation.source_subrecord_index.is_some() {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "Oblivion cell migration requires a record-level binding".to_owned(),
            });
        }
        for (key, expected) in [
            ("data_path", "CELL/2:Flags"),
            ("grid_path", "CELL/3:Grid"),
            ("lighting_path", "CELL/4:Lighting"),
        ] {
            let actual = invocation
                .context
                .configuration
                .get(key)
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: format!("Oblivion cell migration requires {key}"),
                })?;
            if actual != expected {
                return Err(SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: format!(
                        "Oblivion cell migration requires materialized {key} {expected}"
                    ),
                });
            }
        }
        let record = invocation
            .source_writable_record
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "Oblivion cell migration requires a writable record".to_owned(),
            })?;
        if record.flags.contains(RecordFlags::DELETED) || record.subrecords.is_empty() {
            return Ok(HandlerOutput::None);
        }
        let Some(data) = record
            .subrecords
            .iter()
            .find(|subrecord| subrecord.signature == Signature(*b"DATA"))
        else {
            return Ok(HandlerOutput::None);
        };
        let Some(flags) = data.data.first().copied() else {
            return Ok(HandlerOutput::None);
        };
        if flags & 0x01 != 0 {
            if record
                .subrecords
                .iter()
                .any(|subrecord| subrecord.signature == Signature(*b"XCLL"))
            {
                return Ok(HandlerOutput::None);
            }
            return Ok(HandlerOutput::Mutations(vec![
                HandlerMutation::InsertPayload {
                    path: "CELL/4:Lighting".to_owned(),
                    data: vec![0; 36],
                },
            ]));
        }

        let mut mutations = Vec::new();
        if !record
            .subrecords
            .iter()
            .any(|subrecord| subrecord.signature == Signature(*b"XCLC"))
        {
            mutations.push(HandlerMutation::InsertPayload {
                path: "CELL/3:Grid".to_owned(),
                data: vec![0; 8],
            });
        }
        let parent_group_type = self.resolver.as_ref().and_then(|resolver| {
            resolver.source_parent_group_type(handler_record_context(&invocation.context))
        });
        if flags & 0x02 == 0 && parent_group_type == Some(1) {
            let mut normalized = data.data.clone();
            normalized[0] |= 0x02;
            mutations.push(HandlerMutation::ReplacePayload {
                path: "CELL/2:Flags".to_owned(),
                occurrence: 0,
                data: normalized,
            });
        }
        if mutations.is_empty() {
            Ok(HandlerOutput::None)
        } else {
            Ok(HandlerOutput::Mutations(mutations))
        }
    }
}

struct OblivionPathGridAfterLoad;

impl SemanticHandler for OblivionPathGridAfterLoad {
    fn id(&self) -> &'static str {
        "migrate.oblivion_path_grid_after_load"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::AfterLoad {
            return Ok(HandlerOutput::None);
        }
        if invocation.context.game != SchemaGame::Oblivion
            || invocation.context.record_signature != Signature(*b"PGRD")
            || invocation.context.binding.path != "PGRD"
        {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "Oblivion path-grid migration requires a guarded PGRD root binding"
                    .to_owned(),
            });
        }
        if invocation.source_subrecord_index.is_some() {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "Oblivion path-grid migration requires a record-level binding".to_owned(),
            });
        }
        verify_oblivion_path_grid_configuration(&invocation, self.id())?;
        let record = invocation
            .source_writable_record
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "Oblivion path-grid migration requires a writable record".to_owned(),
            })?;
        if record.flags.contains(RecordFlags::DELETED) || record.subrecords.is_empty() {
            return Ok(HandlerOutput::None);
        }
        let Some(points) = record
            .subrecords
            .iter()
            .find(|subrecord| subrecord.signature == Signature(*b"PGRP"))
        else {
            return Ok(HandlerOutput::None);
        };
        let point_count = points.data.len() / 16;
        let mut mutations = Vec::new();
        if !record
            .subrecords
            .iter()
            .any(|subrecord| subrecord.signature == Signature(*b"PGAG"))
        {
            mutations.push(HandlerMutation::InsertPayload {
                path: "PGRD/2:Unknown".to_owned(),
                data: vec![0; point_count.saturating_add(7) / 8],
            });
        }
        if !record.flags.contains(RecordFlags::COMPRESSED) {
            mutations.push(HandlerMutation::SetRecordFlags {
                path: "PGRD".to_owned(),
                flags: record.flags | RecordFlags::COMPRESSED,
            });
        }
        let connections = record
            .subrecords
            .iter()
            .find(|subrecord| subrecord.signature == Signature(*b"PGRR"));
        if let Some(connections) = connections {
            if let Some((normalized_points, normalized_connections)) =
                normalize_oblivion_point_connections(&points.data, &connections.data)
            {
                mutations.push(HandlerMutation::ReplacePayload {
                    path: "PGRD/1:Points".to_owned(),
                    occurrence: 0,
                    data: normalized_points,
                });
                mutations.push(HandlerMutation::ReplacePayload {
                    path: "PGRD/3:Point-to-Point Connections".to_owned(),
                    occurrence: 0,
                    data: normalized_connections,
                });
            }
        }
        if mutations.is_empty() {
            Ok(HandlerOutput::None)
        } else {
            Ok(HandlerOutput::Mutations(mutations))
        }
    }
}

fn verify_oblivion_path_grid_configuration(
    invocation: &HandlerInvocation<'_>,
    handler: &str,
) -> Result<()> {
    for (key, expected) in [
        ("points_path", serde_json::json!("PGRD/1:Points")),
        ("auxiliary_path", serde_json::json!("PGRD/2:Unknown")),
        (
            "connections_path",
            serde_json::json!("PGRD/3:Point-to-Point Connections"),
        ),
        ("point_size", serde_json::json!(16)),
        ("connection_count_offset", serde_json::json!(12)),
    ] {
        if invocation.context.configuration.get(key) != Some(&expected) {
            return Err(SemanticError::Handler {
                handler: handler.to_owned(),
                message: format!(
                    "Oblivion path-grid migration requires materialized {key} {expected}"
                ),
            });
        }
    }
    Ok(())
}

fn normalize_oblivion_point_connections(
    points: &[u8],
    connections: &[u8],
) -> Option<(Vec<u8>, Vec<u8>)> {
    if !points.len().is_multiple_of(16) || !connections.len().is_multiple_of(2) {
        return None;
    }
    let expected_connections = points
        .chunks_exact(16)
        .map(|point| usize::from(point[12]))
        .sum::<usize>();
    if expected_connections.saturating_mul(2) != connections.len() {
        return None;
    }
    let mut normalized_points = points.to_vec();
    let mut normalized_connections = Vec::with_capacity(connections.len());
    let mut source_offset = 0;
    let mut changed = false;
    for (index, point) in points.chunks_exact(16).enumerate() {
        let count = usize::from(point[12]);
        let end = source_offset + count * 2;
        let connection_group = &connections[source_offset..end];
        let retained_len = connection_group
            .chunks_exact(2)
            .rposition(|connection| connection != [0xff, 0xff])
            .map_or(0, |last| last + 1);
        if retained_len != count {
            changed = true;
            normalized_points[index * 16 + 12] =
                u8::try_from(retained_len).expect("retained PGRR count fits its source byte");
        }
        normalized_connections.extend_from_slice(&connection_group[..retained_len * 2]);
        source_offset = end;
    }
    changed.then_some((normalized_points, normalized_connections))
}

struct OblivionInterCellConnectionsAfterLoad;

impl SemanticHandler for OblivionInterCellConnectionsAfterLoad {
    fn id(&self) -> &'static str {
        "migrate.oblivion_inter_cell_connections_after_load"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::AfterLoad {
            return Ok(HandlerOutput::None);
        }
        if invocation.context.game != SchemaGame::Oblivion
            || invocation.context.record_signature != Signature(*b"PGRD")
            || invocation.context.binding.path != "PGRD/4:Inter-Cell Connections/payload"
        {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "Oblivion inter-cell cleanup requires a guarded PGRI payload binding"
                    .to_owned(),
            });
        }
        for (key, expected) in [
            ("entry_size", 16),
            ("point_offset", 0),
            ("x_offset", 4),
            ("y_offset", 8),
            ("z_offset", 12),
        ] {
            if invocation
                .context
                .configuration
                .get(key)
                .and_then(serde_json::Value::as_u64)
                != Some(expected)
            {
                return Err(SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: format!(
                        "Oblivion inter-cell cleanup requires materialized {key} {expected}"
                    ),
                });
            }
        }
        let record = invocation
            .source_writable_record
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "Oblivion inter-cell cleanup requires a writable record".to_owned(),
            })?;
        let index = invocation
            .source_subrecord_index
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "Oblivion inter-cell cleanup requires a source subrecord".to_owned(),
            })?;
        let connections = record
            .subrecords
            .get(index)
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: format!("source subrecord index {index} is out of bounds"),
            })?;
        if connections.signature != Signature(*b"PGRI") {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: format!(
                    "Oblivion inter-cell cleanup requires PGRI, got {}",
                    connections.signature
                ),
            });
        }
        let Some(normalized) = deduplicate_oblivion_inter_cell_connections(&connections.data)
        else {
            return Ok(HandlerOutput::None);
        };
        Ok(HandlerOutput::SubrecordPayload(normalized))
    }
}

fn deduplicate_oblivion_inter_cell_connections(data: &[u8]) -> Option<Vec<u8>> {
    if !data.len().is_multiple_of(16) {
        return None;
    }
    let entries = data.chunks_exact(16).collect::<Vec<_>>();
    let mut keys = BTreeSet::new();
    let mut keep = vec![true; entries.len()];
    for (index, entry) in entries.iter().enumerate().rev() {
        let key = (
            u16::from_le_bytes(entry[..2].try_into().expect("two-byte PGRI point")),
            oblivion_path_grid_float_sort_key(&entry[4..8]),
            oblivion_path_grid_float_sort_key(&entry[8..12]),
            oblivion_path_grid_float_sort_key(&entry[12..16]),
        );
        if !keys.insert(key) {
            keep[index] = false;
        }
    }
    if keep.iter().all(|retain| *retain) {
        return None;
    }
    Some(
        entries
            .into_iter()
            .zip(keep)
            .filter_map(|(entry, retain)| retain.then_some(entry))
            .flatten()
            .copied()
            .collect(),
    )
}

fn oblivion_path_grid_float_sort_key(bytes: &[u8]) -> String {
    let value = f32::from_le_bytes(bytes.try_into().expect("four-byte PGRI coordinate"));
    if value.is_nan() {
        return "nan".to_owned();
    }
    if value == f32::INFINITY {
        return "+inf".to_owned();
    }
    if value == f32::NEG_INFINITY {
        return "-inf".to_owned();
    }
    let normalized = if value == 0.0 || value.is_subnormal() {
        0.0
    } else {
        value
    };
    format!("{normalized:.6}")
}

fn required_cell_path<'a>(
    invocation: &'a HandlerInvocation<'_>,
    handler: &str,
    key: &str,
) -> Result<&'a str> {
    let path = invocation
        .context
        .configuration
        .get(key)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| SemanticError::Handler {
            handler: handler.to_owned(),
            message: format!("cell migration requires {key}"),
        })?;
    if !path.starts_with("CELL/") {
        return Err(SemanticError::Handler {
            handler: handler.to_owned(),
            message: format!("cell migration {key} must be inside CELL"),
        });
    }
    Ok(path)
}

struct LegacyEffectShaderAfterLoad;

impl SemanticHandler for LegacyEffectShaderAfterLoad {
    fn id(&self) -> &'static str {
        "migrate.legacy_effect_shader_birth_ratios"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::AfterLoad {
            return Ok(HandlerOutput::None);
        }
        if invocation.context.record_signature != Signature(*b"EFSH")
            || invocation.context.binding.path != "EFSH"
            || !matches!(
                invocation.context.game,
                SchemaGame::Fallout3 | SchemaGame::FalloutNv
            )
        {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "effect-shader migration requires a guarded EFSH root binding".to_owned(),
            });
        }
        if invocation.source_subrecord_index.is_some() {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "effect-shader migration requires a record-level binding".to_owned(),
            });
        }
        let record = invocation
            .source_writable_record
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "effect-shader migration requires a writable record".to_owned(),
            })?;
        if record.flags.contains(RecordFlags::DELETED) || record.subrecords.is_empty() {
            return Ok(HandlerOutput::None);
        }
        let data_path = invocation
            .context
            .configuration
            .get("data_path")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "effect-shader migration requires data_path".to_owned(),
            })?;
        if !data_path.starts_with("EFSH/") {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "effect-shader DATA path must be inside EFSH".to_owned(),
            });
        }
        let Some(data) = record
            .subrecords
            .iter()
            .find(|subrecord| subrecord.signature == Signature(*b"DATA"))
        else {
            return Ok(HandlerOutput::None);
        };
        let mut replacement = data.data.clone();
        let mut changed = false;
        for offset in [124_usize, 128] {
            let Some(bytes) = replacement.get(offset..offset + 4) else {
                continue;
            };
            let value = f32::from_le_bytes(
                bytes
                    .try_into()
                    .expect("four-byte effect-shader slice was checked above"),
            );
            if value != 0.0 && value <= 1.0 {
                replacement[offset..offset + 4].copy_from_slice(&(value * 78.0).to_le_bytes());
                changed = true;
            }
        }
        if !changed {
            return Ok(HandlerOutput::None);
        }
        Ok(HandlerOutput::Mutations(vec![
            HandlerMutation::ReplacePayload {
                path: data_path.to_owned(),
                occurrence: 0,
                data: replacement,
            },
        ]))
    }
}

struct LegacyFactionAfterLoad;

impl SemanticHandler for LegacyFactionAfterLoad {
    fn id(&self) -> &'static str {
        "migrate.legacy_faction_after_load"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::AfterLoad {
            return Ok(HandlerOutput::None);
        }
        if invocation.context.record_signature != Signature(*b"FACT")
            || invocation.context.binding.path != "FACT"
            || !matches!(
                invocation.context.game,
                SchemaGame::Fallout3 | SchemaGame::FalloutNv
            )
        {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "faction migration requires a guarded FACT root binding".to_owned(),
            });
        }
        if invocation.source_subrecord_index.is_some() {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "faction migration requires a record-level binding".to_owned(),
            });
        }
        let record = invocation
            .source_writable_record
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "faction migration requires a writable record".to_owned(),
            })?;
        if record.flags.contains(RecordFlags::DELETED) || record.subrecords.is_empty() {
            return Ok(HandlerOutput::None);
        }
        let unused_path = invocation
            .context
            .configuration
            .get("unused_path")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "faction migration requires unused_path".to_owned(),
            })?;
        if unused_path != "FACT/4:Unused" {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "faction migration requires the materialized CNAM path".to_owned(),
            });
        }
        if !record
            .subrecords
            .iter()
            .any(|subrecord| subrecord.signature == Signature(*b"CNAM"))
        {
            return Ok(HandlerOutput::None);
        }
        Ok(HandlerOutput::Mutations(vec![
            HandlerMutation::RemoveFirstBySignature {
                path: "FACT".to_owned(),
                signature: Signature(*b"CNAM"),
            },
        ]))
    }
}

struct LegacyWaterAfterLoad;

impl SemanticHandler for LegacyWaterAfterLoad {
    fn id(&self) -> &'static str {
        "migrate.legacy_water_after_load"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::AfterLoad {
            return Ok(HandlerOutput::None);
        }
        if invocation.context.record_signature != Signature(*b"WATR")
            || invocation.context.binding.path != "WATR"
            || !matches!(
                invocation.context.game,
                SchemaGame::Fallout3 | SchemaGame::FalloutNv
            )
        {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "water migration requires a guarded WATR root binding".to_owned(),
            });
        }
        if invocation.source_subrecord_index.is_some() {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "water migration requires a record-level binding".to_owned(),
            });
        }
        let record = invocation
            .source_writable_record
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "water migration requires a writable record".to_owned(),
            })?;
        if record.flags.contains(RecordFlags::DELETED) || record.subrecords.is_empty() {
            return Ok(HandlerOutput::None);
        }
        let required_path = |key: &str, expected: &str| -> Result<&str> {
            let path = invocation
                .context
                .configuration
                .get(key)
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: format!("water migration requires {key}"),
                })?;
            if path != expected {
                return Err(SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: format!("water migration {key} does not match the materialized path"),
                });
            }
            Ok(path)
        };
        let damage_path = required_path("damage_path", "WATR/8:Damage")?;
        let new_visual_path = required_path("new_visual_path", "WATR/9:Visual Data/0:Visual Data")?;
        let old_visual_path = required_path("old_visual_path", "WATR/9:Visual Data/1:Visual Data")?;
        if record
            .subrecords
            .iter()
            .any(|subrecord| subrecord.signature == Signature(*b"DNAM"))
        {
            return Ok(HandlerOutput::None);
        }
        let Some(old_visual) = record.subrecords.iter().find(|subrecord| {
            subrecord.signature == Signature(*b"DATA") && subrecord.data.len() == 186
        }) else {
            return Ok(HandlerOutput::None);
        };
        let damage = old_visual.data[184..186].to_vec();
        let mut new_visual = vec![0_u8; 196];
        new_visual[..184].copy_from_slice(&old_visual.data[..184]);
        new_visual[184..188].copy_from_slice(&1.0_f32.to_le_bytes());
        new_visual[188..192].copy_from_slice(&0.5_f32.to_le_bytes());
        new_visual[192..196].copy_from_slice(&0.25_f32.to_le_bytes());
        let mut mutations = vec![HandlerMutation::Remove {
            path: old_visual_path.to_owned(),
            occurrence: 0,
        }];
        if record.subrecords.iter().any(|subrecord| {
            subrecord.signature == Signature(*b"DATA") && subrecord.data.len() == 2
        }) {
            mutations.push(HandlerMutation::ReplacePayload {
                path: damage_path.to_owned(),
                occurrence: 0,
                data: damage,
            });
        } else {
            mutations.push(HandlerMutation::InsertPayload {
                path: damage_path.to_owned(),
                data: damage,
            });
        }
        mutations.push(HandlerMutation::InsertPayload {
            path: new_visual_path.to_owned(),
            data: new_visual,
        });
        Ok(HandlerOutput::Mutations(mutations))
    }
}

struct OblivionReferenceAfterLoad;

impl SemanticHandler for OblivionReferenceAfterLoad {
    fn id(&self) -> &'static str {
        "migrate.oblivion_reference_after_load"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::AfterLoad {
            return Ok(HandlerOutput::None);
        }
        let (expected_root, expected_path) = match invocation.context.record_signature {
            Signature(signature) if signature == *b"ACHR" => ("ACHR", "ACHR/2:Unused/0:Unused"),
            Signature(signature) if signature == *b"REFR" => ("REFR", "REFR/11:Unused/0:Unused"),
            _ => {
                return Err(SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: "Oblivion reference migration requires ACHR or REFR".to_owned(),
                });
            }
        };
        if invocation.context.binding.path != expected_root
            || invocation.context.game != SchemaGame::Oblivion
        {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "Oblivion reference migration requires a guarded root binding".to_owned(),
            });
        }
        if invocation.source_subrecord_index.is_some() {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "Oblivion reference migration requires a record-level binding".to_owned(),
            });
        }
        let record = invocation
            .source_writable_record
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "Oblivion reference migration requires a writable record".to_owned(),
            })?;
        if record.subrecords.is_empty() {
            return Ok(HandlerOutput::None);
        }
        let unused_path = invocation
            .context
            .configuration
            .get("unused_path")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "Oblivion reference migration requires unused_path".to_owned(),
            })?;
        if unused_path != expected_path {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "Oblivion reference migration path does not match the record".to_owned(),
            });
        }
        if !record
            .subrecords
            .iter()
            .any(|subrecord| subrecord.signature == Signature(*b"XPCI"))
        {
            return Ok(HandlerOutput::None);
        }
        Ok(HandlerOutput::Mutations(vec![
            HandlerMutation::RemoveFirstBySignature {
                path: invocation.context.binding.path.to_owned(),
                signature: Signature(*b"XPCI"),
            },
        ]))
    }
}

struct OblivionLeveledListAfterLoad;

impl SemanticHandler for OblivionLeveledListAfterLoad {
    fn id(&self) -> &'static str {
        "migrate.oblivion_leveled_list_after_load"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::AfterLoad {
            return Ok(HandlerOutput::None);
        }
        let root = invocation.context.record_signature.to_string();
        if invocation.context.game != SchemaGame::Oblivion
            || !matches!(root.as_str(), "LVLC" | "LVLI" | "LVSP")
            || invocation.context.binding.path != root
        {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "Oblivion leveled-list migration requires a guarded root binding"
                    .to_owned(),
            });
        }
        if invocation.source_subrecord_index.is_some() {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "Oblivion leveled-list migration requires a record-level binding"
                    .to_owned(),
            });
        }
        let record = invocation
            .source_writable_record
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "Oblivion leveled-list migration requires a writable record".to_owned(),
            })?;
        if record.flags.contains(RecordFlags::DELETED) || record.subrecords.is_empty() {
            return Ok(HandlerOutput::None);
        }
        let configured_path = |key: &str, suffix: &str| -> Result<&str> {
            let path = invocation
                .context
                .configuration
                .get(key)
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: format!("Oblivion leveled-list migration requires {key}"),
                })?;
            if path != format!("{root}/{suffix}") {
                return Err(SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: format!(
                        "Oblivion leveled-list {key} does not match the guarded record"
                    ),
                });
            }
            Ok(path)
        };
        let chance_path = configured_path("chance_path", "1:Chance none")?;
        let flags_path = configured_path("flags_path", "2:Flags")?;
        let mut mutations = Vec::new();
        if record
            .subrecords
            .iter()
            .any(|subrecord| subrecord.signature == Signature(*b"DATA"))
        {
            mutations.push(HandlerMutation::RemoveFirstBySignature {
                path: root,
                signature: Signature(*b"DATA"),
            });
        }
        let chance = record
            .subrecords
            .iter()
            .find(|subrecord| subrecord.signature == Signature(*b"LVLD"));
        if let Some(chance) = chance.filter(|subrecord| {
            subrecord
                .data
                .first()
                .is_some_and(|value| value & 0x80 != 0)
        }) {
            let mut data = chance.data.clone();
            data[0] &= 0x7f;
            mutations.push(HandlerMutation::ReplacePayload {
                path: chance_path.to_owned(),
                occurrence: 0,
                data,
            });
            if let Some(flags) = record
                .subrecords
                .iter()
                .find(|subrecord| subrecord.signature == Signature(*b"LVLF"))
            {
                let mut data = flags.data.clone();
                if data.is_empty() {
                    data.push(1);
                } else {
                    data[0] |= 1;
                }
                mutations.push(HandlerMutation::ReplacePayload {
                    path: flags_path.to_owned(),
                    occurrence: 0,
                    data,
                });
            } else {
                mutations.push(HandlerMutation::InsertPayload {
                    path: flags_path.to_owned(),
                    data: vec![1],
                });
            }
        }
        if mutations.is_empty() {
            Ok(HandlerOutput::None)
        } else {
            Ok(HandlerOutput::Mutations(mutations))
        }
    }
}

struct OblivionMagicEffectAfterLoad {
    resolver: Option<Arc<dyn FormLinkResolver>>,
}

impl SemanticHandler for OblivionMagicEffectAfterLoad {
    fn id(&self) -> &'static str {
        "migrate.oblivion_magic_effect_after_load"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::AfterLoad {
            return Ok(HandlerOutput::None);
        }
        if invocation.context.game != SchemaGame::Oblivion
            || invocation.context.record_signature != Signature(*b"MGEF")
            || invocation.context.binding.path != "MGEF"
        {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "Oblivion magic-effect migration requires a guarded MGEF root binding"
                    .to_owned(),
            });
        }
        if invocation.source_subrecord_index.is_some() {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "Oblivion magic-effect migration requires a record-level binding"
                    .to_owned(),
            });
        }
        for (key, expected) in [
            ("code_path", "MGEF/0:Magic Effect Code"),
            ("data_path", "MGEF/7:Data"),
            ("flags_path", "MGEF/7:Data/payload/0:Flags"),
        ] {
            let actual = invocation
                .context
                .configuration
                .get(key)
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: format!("Oblivion magic-effect migration requires {key}"),
                })?;
            if actual != expected {
                return Err(SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: format!(
                        "Oblivion magic-effect migration requires materialized {key} {expected}"
                    ),
                });
            }
        }
        let record = invocation
            .source_writable_record
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "Oblivion magic-effect migration requires a writable record".to_owned(),
            })?;
        if record.flags.contains(RecordFlags::DELETED) || record.subrecords.is_empty() {
            return Ok(HandlerOutput::None);
        }
        let source_file = self.resolver.as_ref().and_then(|resolver| {
            resolver.source_file_name(handler_record_context(&invocation.context))
        });
        if !source_file.is_some_and(|name| name.eq_ignore_ascii_case("Oblivion.esm")) {
            return Ok(HandlerOutput::None);
        }
        let Some(editor_id) = record
            .subrecords
            .iter()
            .find(|subrecord| subrecord.signature == Signature(*b"EDID"))
            .map(|subrecord| {
                let end = subrecord
                    .data
                    .iter()
                    .position(|byte| *byte == 0)
                    .unwrap_or(subrecord.data.len());
                &subrecord.data[..end]
            })
        else {
            return Ok(HandlerOutput::None);
        };
        let Some(data) = record
            .subrecords
            .iter()
            .find(|subrecord| subrecord.signature == Signature(*b"DATA"))
        else {
            return Ok(HandlerOutput::None);
        };
        let Some(flags) = data.data.get(..4) else {
            return Ok(HandlerOutput::None);
        };
        let mut normalized = u32::from_le_bytes(flags.try_into().expect("four-byte MGEF flags"));
        if [b"RSFI", b"RSFR", b"RSPA", b"RSSH"]
            .iter()
            .any(|expected| editor_id.eq_ignore_ascii_case(*expected))
        {
            normalized |= 0x0000_0008;
        } else if editor_id.eq_ignore_ascii_case(b"REAN") {
            normalized &= !0x0002_0000;
        } else {
            return Ok(HandlerOutput::None);
        }
        if normalized.to_le_bytes() == flags {
            return Ok(HandlerOutput::None);
        }
        let mut payload = data.data.clone();
        payload[..4].copy_from_slice(&normalized.to_le_bytes());
        Ok(HandlerOutput::Mutations(vec![
            HandlerMutation::ReplacePayload {
                path: "MGEF/7:Data".to_owned(),
                occurrence: 0,
                data: payload,
            },
        ]))
    }
}

struct LegacyNpcAfterLoad;

impl SemanticHandler for LegacyNpcAfterLoad {
    fn id(&self) -> &'static str {
        "migrate.legacy_npc_after_load"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::AfterLoad {
            return Ok(HandlerOutput::None);
        }
        if invocation.context.record_signature != Signature(*b"NPC_")
            || invocation.context.binding.path != "NPC_"
            || !matches!(
                invocation.context.game,
                SchemaGame::Fallout3 | SchemaGame::FalloutNv
            )
        {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "legacy NPC migration requires a guarded NPC_ root binding".to_owned(),
            });
        }
        if invocation.source_subrecord_index.is_some() {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "legacy NPC migration requires a record-level binding".to_owned(),
            });
        }
        let record = invocation
            .source_writable_record
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "legacy NPC migration requires a writable record".to_owned(),
            })?;
        if record.flags.contains(RecordFlags::DELETED) || record.subrecords.is_empty() {
            return Ok(HandlerOutput::None);
        }
        let value_path = invocation
            .context
            .configuration
            .get("value_path")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "legacy NPC migration requires value_path".to_owned(),
            })?;
        if value_path != "NPC_/30:Unknown" {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "legacy NPC migration requires the materialized NAM5 path".to_owned(),
            });
        }
        let Some(value) = record
            .subrecords
            .iter()
            .find(|subrecord| subrecord.signature == Signature(*b"NAM5"))
        else {
            return Ok(HandlerOutput::None);
        };
        let Some(bytes) = value.data.get(..2) else {
            return Ok(HandlerOutput::None);
        };
        let native = u16::from_le_bytes(
            bytes
                .try_into()
                .expect("two-byte NPC value slice was checked above"),
        );
        if native <= 255 {
            return Ok(HandlerOutput::None);
        }
        let mut data = value.data.clone();
        data[..2].copy_from_slice(&255_u16.to_le_bytes());
        Ok(HandlerOutput::Mutations(vec![
            HandlerMutation::ReplacePayload {
                path: value_path.to_owned(),
                occurrence: 0,
                data,
            },
        ]))
    }
}

struct LegacyInfoAfterLoad;

impl SemanticHandler for LegacyInfoAfterLoad {
    fn id(&self) -> &'static str {
        "migrate.legacy_info_after_load"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::AfterLoad {
            return Ok(HandlerOutput::None);
        }
        if invocation.context.record_signature != Signature(*b"INFO")
            || invocation.context.binding.path != "INFO"
            || !matches!(
                invocation.context.game,
                SchemaGame::Fallout3 | SchemaGame::FalloutNv
            )
        {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "legacy INFO migration requires a guarded INFO root binding".to_owned(),
            });
        }
        if invocation.source_subrecord_index.is_some() {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "legacy INFO migration requires a record-level binding".to_owned(),
            });
        }
        let record = invocation
            .source_writable_record
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "legacy INFO migration requires a writable record".to_owned(),
            })?;
        if record.flags.contains(RecordFlags::DELETED) || record.subrecords.is_empty() {
            return Ok(HandlerOutput::None);
        }
        let (expected_unused_sound_path, expected_speech_challenge_path) =
            match invocation.context.game {
                SchemaGame::Fallout3 => ("INFO/11:Unused", "INFO/15:Speech Challenge"),
                SchemaGame::FalloutNv => ("INFO/12:Unused", "INFO/16:Speech Challenge"),
                _ => unreachable!("legacy INFO game guard was checked above"),
            };
        for (key, expected) in [
            ("data_path", "INFO/0:DATA"),
            ("unused_sound_path", expected_unused_sound_path),
            ("speech_challenge_path", expected_speech_challenge_path),
        ] {
            let actual = invocation
                .context
                .configuration
                .get(key)
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: format!("legacy INFO migration requires {key}"),
                })?;
            if actual != expected {
                return Err(SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: format!(
                        "legacy INFO migration requires materialized {key} {expected}"
                    ),
                });
            }
        }

        let data = record
            .subrecords
            .iter()
            .find(|subrecord| subrecord.signature == Signature(*b"DATA"));
        let flags = data
            .and_then(|subrecord| subrecord.data.get(2))
            .copied()
            .unwrap_or_default();
        let mut mutations = Vec::new();
        if flags & 0x80 == 0
            && record
                .subrecords
                .iter()
                .any(|subrecord| subrecord.signature == Signature(*b"DNAM"))
        {
            mutations.push(HandlerMutation::RemoveFirstBySignature {
                path: "INFO".to_owned(),
                signature: Signature(*b"DNAM"),
            });
        }
        if record
            .subrecords
            .iter()
            .any(|subrecord| subrecord.signature == Signature(*b"SNDD"))
        {
            mutations.push(HandlerMutation::RemoveFirstBySignature {
                path: "INFO".to_owned(),
                signature: Signature(*b"SNDD"),
            });
        }
        if let Some(data) = data.filter(|subrecord| subrecord.data.first() == Some(&3)) {
            let mut payload = data.data.clone();
            payload[0] = 0;
            mutations.push(HandlerMutation::ReplacePayload {
                path: "INFO/0:DATA".to_owned(),
                occurrence: 0,
                data: payload,
            });
        }
        if mutations.is_empty() {
            Ok(HandlerOutput::None)
        } else {
            Ok(HandlerOutput::Mutations(mutations))
        }
    }
}

struct LegacySoundAfterLoad;

impl SemanticHandler for LegacySoundAfterLoad {
    fn id(&self) -> &'static str {
        "migrate.legacy_sound_after_load"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::AfterLoad {
            return Ok(HandlerOutput::None);
        }
        if invocation.context.record_signature != Signature(*b"SOUN")
            || invocation.context.binding.path != "SOUN"
            || !matches!(
                invocation.context.game,
                SchemaGame::Fallout3 | SchemaGame::FalloutNv
            )
        {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "legacy sound migration requires a guarded SOUN root binding".to_owned(),
            });
        }
        if invocation.source_subrecord_index.is_some() {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "legacy sound migration requires a record-level binding".to_owned(),
            });
        }
        let record = invocation
            .source_writable_record
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "legacy sound migration requires a writable record".to_owned(),
            })?;
        if record.flags.contains(RecordFlags::DELETED) || record.subrecords.is_empty() {
            return Ok(HandlerOutput::None);
        }
        let base = match invocation.context.game {
            SchemaGame::Fallout3 => 3,
            SchemaGame::FalloutNv => 4,
            _ => unreachable!("legacy sound game guard was checked above"),
        };
        let expected_paths = [
            (
                "new_data_path",
                format!("SOUN/{base}:Sound Data/0:Sound Data"),
            ),
            (
                "old_data_path",
                format!("SOUN/{base}:Sound Data/1:Sound Data"),
            ),
            ("curve_path", format!("SOUN/{}:Attenuation Curve", base + 1)),
            (
                "reverb_path",
                format!("SOUN/{}:Reverb Attenuation Control", base + 2),
            ),
            ("priority_path", format!("SOUN/{}:Priority", base + 3)),
        ];
        for (key, expected) in &expected_paths {
            let actual = invocation
                .context
                .configuration
                .get(key)
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: format!("legacy sound migration requires {key}"),
                })?;
            if actual != expected {
                return Err(SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: format!(
                        "legacy sound migration requires materialized {key} {expected}"
                    ),
                });
            }
        }
        if record
            .subrecords
            .iter()
            .any(|subrecord| subrecord.signature == Signature(*b"SNDD"))
        {
            return Ok(HandlerOutput::None);
        }
        let Some(old_data) = record
            .subrecords
            .iter()
            .find(|subrecord| subrecord.signature == Signature(*b"SNDX"))
        else {
            return Ok(HandlerOutput::None);
        };
        if old_data.data.len() != 12 {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: format!(
                    "legacy sound migration requires 12-byte SNDX, got {} bytes",
                    old_data.data.len()
                ),
            });
        }
        let mut data = vec![0; 36];
        data[..12].copy_from_slice(&old_data.data);
        for (offset, value) in [(12, 100_i16), (14, 50), (16, 20), (18, 5), (20, 0)] {
            data[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
        }
        data[22..24].copy_from_slice(&80_i16.to_le_bytes());
        data[24..28].copy_from_slice(&128_i32.to_le_bytes());
        copy_legacy_sound_value(record, Signature(*b"ANAM"), 10, &mut data, 12, self.id())?;
        copy_legacy_sound_value(record, Signature(*b"GNAM"), 2, &mut data, 22, self.id())?;
        copy_legacy_sound_value(record, Signature(*b"HNAM"), 4, &mut data, 24, self.id())?;

        let mut mutations = vec![HandlerMutation::RemoveFirstBySignature {
            path: "SOUN".to_owned(),
            signature: Signature(*b"SNDX"),
        }];
        for signature in [*b"ANAM", *b"GNAM", *b"HNAM"] {
            if record
                .subrecords
                .iter()
                .any(|subrecord| subrecord.signature == Signature(signature))
            {
                mutations.push(HandlerMutation::RemoveFirstBySignature {
                    path: "SOUN".to_owned(),
                    signature: Signature(signature),
                });
            }
        }
        mutations.push(HandlerMutation::InsertPayload {
            path: expected_paths[0].1.clone(),
            data,
        });
        Ok(HandlerOutput::Mutations(mutations))
    }
}

fn copy_legacy_sound_value(
    record: &WritableRecord,
    signature: Signature,
    size: usize,
    target: &mut [u8],
    offset: usize,
    handler: &str,
) -> Result<()> {
    let Some(source) = record
        .subrecords
        .iter()
        .find(|subrecord| subrecord.signature == signature)
    else {
        return Ok(());
    };
    if source.data.len() != size {
        return Err(SemanticError::Handler {
            handler: handler.to_owned(),
            message: format!(
                "legacy sound migration requires {size}-byte {signature}, got {} bytes",
                source.data.len()
            ),
        });
    }
    target[offset..offset + size].copy_from_slice(&source.data);
    Ok(())
}

struct LegacyWeaponAfterLoad;

impl SemanticHandler for LegacyWeaponAfterLoad {
    fn id(&self) -> &'static str {
        "migrate.legacy_weapon_after_load"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::AfterLoad {
            return Ok(HandlerOutput::None);
        }
        if invocation.context.record_signature != Signature(*b"WEAP")
            || invocation.context.binding.path != "WEAP"
            || !matches!(
                invocation.context.game,
                SchemaGame::Fallout3 | SchemaGame::FalloutNv
            )
        {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "legacy weapon migration requires a guarded WEAP root binding".to_owned(),
            });
        }
        if invocation.source_subrecord_index.is_some() {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "legacy weapon migration requires a record-level binding".to_owned(),
            });
        }
        let expected_data_path = match invocation.context.game {
            SchemaGame::Fallout3 => "WEAP/31:DNAM",
            SchemaGame::FalloutNv => "WEAP/51:DNAM",
            _ => unreachable!("legacy weapon game guard was checked above"),
        };
        for (key, expected) in [
            ("data_path", expected_data_path),
            (
                "animation_multiplier_path",
                &format!("{expected_data_path}/payload/1:Animation Multiplier"),
            ),
            (
                "attack_multiplier_path",
                &format!("{expected_data_path}/payload/21:Animation Attack Multiplier"),
            ),
        ] {
            let actual = invocation
                .context
                .configuration
                .get(key)
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: format!("legacy weapon migration requires {key}"),
                })?;
            if actual != expected {
                return Err(SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: format!(
                        "legacy weapon migration requires materialized {key} {expected}"
                    ),
                });
            }
        }
        let record = invocation
            .source_writable_record
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "legacy weapon migration requires a writable record".to_owned(),
            })?;
        if record.flags.contains(RecordFlags::DELETED) || record.subrecords.is_empty() {
            return Ok(HandlerOutput::None);
        }
        let Some(data) = record
            .subrecords
            .iter()
            .find(|subrecord| subrecord.signature == Signature(*b"DNAM"))
        else {
            return Ok(HandlerOutput::None);
        };
        let mut normalized = data.data.clone();
        let mut changed = false;
        for offset in [4, 60] {
            let Some(bytes) = data.data.get(offset..offset + 4) else {
                continue;
            };
            let value = f32::from_le_bytes(
                bytes
                    .try_into()
                    .expect("four-byte weapon multiplier must convert"),
            );
            if value == 0.0 {
                normalized[offset..offset + 4].copy_from_slice(&1.0_f32.to_le_bytes());
                changed = true;
            }
        }
        if !changed {
            return Ok(HandlerOutput::None);
        }
        Ok(HandlerOutput::Mutations(vec![
            HandlerMutation::ReplacePayload {
                path: expected_data_path.to_owned(),
                occurrence: 0,
                data: normalized,
            },
        ]))
    }
}

struct LegacyPackageAfterLoad;

impl SemanticHandler for LegacyPackageAfterLoad {
    fn id(&self) -> &'static str {
        "migrate.legacy_package_after_load"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::AfterLoad {
            return Ok(HandlerOutput::None);
        }
        if invocation.context.record_signature != Signature(*b"PACK")
            || invocation.context.binding.path != "PACK"
            || !matches!(
                invocation.context.game,
                SchemaGame::Fallout3 | SchemaGame::FalloutNv
            )
        {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "legacy package migration requires a guarded PACK root binding".to_owned(),
            });
        }
        if invocation.source_subrecord_index.is_some() {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "legacy package migration requires a record-level binding".to_owned(),
            });
        }
        let expected_paths = [
            ("general_path", "PACK/1:General"),
            ("type_path", "PACK/1:General/payload/1:Type"),
            ("locations_path", "PACK/2:Locations"),
            ("location_path", "PACK/2:Locations/0:Location 1"),
            (
                "location_type_path",
                "PACK/2:Locations/0:Location 1/payload/0:Type",
            ),
            ("target_path", "PACK/4:Target 1"),
            ("eat_marker_path", "PACK/8:Eat Marker"),
            (
                "follow_radius_path",
                "PACK/10:Follow - Start Location - Trigger Radius",
            ),
            ("patrol_flags_path", "PACK/11:Patrol Flags"),
        ];
        for &(key, expected) in &expected_paths {
            let actual = invocation
                .context
                .configuration
                .get(key)
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: format!("legacy package migration requires {key}"),
                })?;
            if actual != expected {
                return Err(SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: format!(
                        "legacy package migration requires materialized {key} {expected}"
                    ),
                });
            }
        }
        let record = invocation
            .source_writable_record
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "legacy package migration requires a writable record".to_owned(),
            })?;
        if record.flags.contains(RecordFlags::DELETED) || record.subrecords.is_empty() {
            return Ok(HandlerOutput::None);
        }
        let Some(general) = record
            .subrecords
            .iter()
            .find(|subrecord| subrecord.signature == Signature(*b"PKDT"))
        else {
            return Ok(HandlerOutput::None);
        };
        let Some(package_type) = general.data.get(4).copied() else {
            return Ok(HandlerOutput::None);
        };
        let has_signature = |signature| {
            record
                .subrecords
                .iter()
                .any(|subrecord| subrecord.signature == Signature(signature))
        };
        let schedule_index = record
            .subrecords
            .iter()
            .position(|subrecord| subrecord.signature == Signature(*b"PSDT"));
        let has_locations = has_signature(*b"PLDT")
            || schedule_index.is_some_and(|schedule| {
                record.subrecords[..schedule]
                    .iter()
                    .any(|subrecord| subrecord.signature == Signature(*b"PLD2"))
            });
        let mut mutations = Vec::new();
        match package_type {
            0 => push_package_insert(
                &mut mutations,
                record,
                "PACK/4:Target 1",
                *b"PTDT",
                vec![0; 16],
            ),
            1 => push_package_insert(
                &mut mutations,
                record,
                "PACK/10:Follow - Start Location - Trigger Radius",
                *b"PKFD",
                vec![0; 4],
            ),
            3 => {
                push_package_insert(
                    &mut mutations,
                    record,
                    "PACK/4:Target 1",
                    *b"PTDT",
                    vec![0; 16],
                );
                push_package_insert(
                    &mut mutations,
                    record,
                    "PACK/8:Eat Marker",
                    *b"PKED",
                    Vec::new(),
                );
            }
            4 if !has_locations => {
                let mut data = vec![0; 12];
                data[..4].copy_from_slice(&3_i32.to_le_bytes());
                mutations.push(HandlerMutation::InsertPayload {
                    path: "PACK/2:Locations/0:Location 1".to_owned(),
                    data,
                });
            }
            13 => {
                if !has_locations {
                    let mut data = vec![0; 12];
                    data[..4].copy_from_slice(&6_i32.to_le_bytes());
                    mutations.push(HandlerMutation::InsertPayload {
                        path: "PACK/2:Locations/0:Location 1".to_owned(),
                        data,
                    });
                }
                push_package_insert(
                    &mut mutations,
                    record,
                    "PACK/11:Patrol Flags",
                    *b"PKPT",
                    vec![0; 2],
                );
            }
            _ => {}
        }
        if mutations.is_empty() {
            Ok(HandlerOutput::None)
        } else {
            Ok(HandlerOutput::Mutations(mutations))
        }
    }
}

fn push_package_insert(
    mutations: &mut Vec<HandlerMutation>,
    record: &WritableRecord,
    path: &str,
    signature: [u8; 4],
    data: Vec<u8>,
) {
    if record
        .subrecords
        .iter()
        .all(|subrecord| subrecord.signature != Signature(signature))
    {
        mutations.push(HandlerMutation::InsertPayload {
            path: path.to_owned(),
            data,
        });
    }
}

struct FalloutLeveledListAfterLoad;

impl SemanticHandler for FalloutLeveledListAfterLoad {
    fn id(&self) -> &'static str {
        "migrate.fallout_leveled_list_after_load"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::AfterLoad {
            return Ok(HandlerOutput::None);
        }
        let root = invocation.context.record_signature.to_string();
        let expected = match (invocation.context.game, invocation.context.record_signature) {
            (SchemaGame::Fallout4 | SchemaGame::Fallout4Vr, Signature(signature))
                if signature == *b"LVLN" =>
            {
                (
                    "LVLN/7:Leveled List Entries",
                    "LVLN/7:Leveled List Entries/repeat/0:Leveled List Entry/0:Base Data",
                    Some(10_u64),
                )
            }
            (SchemaGame::Fallout4 | SchemaGame::Fallout4Vr, Signature(signature))
                if signature == *b"LVLI" =>
            {
                (
                    "LVLI/7:Leveled List Entries",
                    "LVLI/7:Leveled List Entries/repeat/0:Leveled List Entry/0:Base Data",
                    Some(10_u64),
                )
            }
            (SchemaGame::Fallout76, Signature(signature)) if signature == *b"LVLN" => (
                "LVLN/12:Leveled List Entries",
                "LVLN/12:Leveled List Entries/repeat/0:Leveled List Entry/0:LVLO",
                Some(10_u64),
            ),
            (SchemaGame::Fallout76, Signature(signature)) if signature == *b"LVLI" => (
                "LVLI/19:Leveled List Entries",
                "LVLI/19:Leveled List Entries/repeat/0:Leveled List Entry/0:LVLO",
                Some(10_u64),
            ),
            (SchemaGame::Fallout76, Signature(signature)) if signature == *b"LVLP" => (
                "LVLP/7:Leveled List Entries",
                "LVLP/7:Leveled List Entries/repeat/0:Leveled List Entry/0:Reference",
                None,
            ),
            _ => {
                return Err(SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: "Fallout leveled-list migration requires a guarded record".to_owned(),
                });
            }
        };
        if invocation.context.binding.path != root {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "Fallout leveled-list migration requires a root binding".to_owned(),
            });
        }
        if invocation.source_subrecord_index.is_some() {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "Fallout leveled-list migration requires a record-level binding"
                    .to_owned(),
            });
        }
        let configured_path = |key: &str, expected_path: &str| -> Result<()> {
            let actual = invocation
                .context
                .configuration
                .get(key)
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: format!("Fallout leveled-list migration requires {key}"),
                })?;
            if actual != expected_path {
                return Err(SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: format!(
                        "Fallout leveled-list migration requires materialized {key} \
                         {expected_path}"
                    ),
                });
            }
            Ok(())
        };
        configured_path("entries_path", expected.0)?;
        configured_path("entry_path", expected.1)?;
        let modern_form_version = invocation
            .context
            .configuration
            .get("modern_form_version")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "Fallout leveled-list migration requires modern_form_version".to_owned(),
            })?;
        if modern_form_version != 69 {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "Fallout leveled-list migration requires form-version boundary 69"
                    .to_owned(),
            });
        }
        let configured_offset = invocation
            .context
            .configuration
            .get("chance_none_offset")
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "Fallout leveled-list migration requires chance_none_offset".to_owned(),
            })?;
        let offset_matches = match expected.2 {
            Some(offset) => configured_offset.as_u64() == Some(offset),
            None => configured_offset.is_null(),
        };
        if !offset_matches {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "Fallout leveled-list migration has a mismatched Chance None layout"
                    .to_owned(),
            });
        }
        if u64::from(invocation.context.form_version) >= modern_form_version {
            return Ok(HandlerOutput::None);
        }
        let record = invocation
            .source_writable_record
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "Fallout leveled-list migration requires a writable record".to_owned(),
            })?;
        if record.flags.contains(RecordFlags::DELETED) || record.subrecords.is_empty() {
            return Ok(HandlerOutput::None);
        }
        let Some(chance_none_offset) = expected.2.map(|offset| offset as usize) else {
            return Ok(HandlerOutput::None);
        };
        let mut mutations = Vec::new();
        let mut occurrence = 0;
        for subrecord in &record.subrecords {
            if subrecord.signature != Signature(*b"LVLO") {
                continue;
            }
            let mut data = subrecord.data.clone();
            let needs_extension = data.len() <= chance_none_offset;
            if needs_extension {
                data.resize(chance_none_offset + 1, 0);
            }
            if needs_extension || data[chance_none_offset] != 0 {
                data[chance_none_offset] = 0;
                mutations.push(HandlerMutation::ReplacePayload {
                    path: expected.1.to_owned(),
                    occurrence,
                    data,
                });
            }
            occurrence += 1;
        }
        if mutations.is_empty() {
            Ok(HandlerOutput::None)
        } else {
            Ok(HandlerOutput::Mutations(mutations))
        }
    }
}

struct FalloutNpcAfterLoad {
    resolver: Option<Arc<dyn FormLinkResolver>>,
}

impl SemanticHandler for FalloutNpcAfterLoad {
    fn id(&self) -> &'static str {
        "migrate.fallout_npc_after_load"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::AfterLoad {
            return Ok(HandlerOutput::None);
        }
        if invocation.context.record_signature != Signature(*b"NPC_")
            || invocation.context.binding.path != "NPC_"
        {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "Fallout NPC migration requires a guarded NPC_ root binding".to_owned(),
            });
        }
        if invocation.source_subrecord_index.is_some() {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "Fallout NPC migration requires a record-level binding".to_owned(),
            });
        }
        let expected_paths = match invocation.context.game {
            SchemaGame::Fallout4 | SchemaGame::Fallout4Vr => [
                ("keyword_count_path", "NPC_/36:Keyword Count"),
                ("keywords_path", "NPC_/37:Keywords"),
                ("morph_keys_path", "NPC_/65:Morph Keys"),
                ("morph_values_path", "NPC_/66:Morph Values"),
            ],
            SchemaGame::Fallout76 => [
                ("keyword_count_path", "NPC_/42:Keywords/0:Keyword Count"),
                ("keywords_path", "NPC_/42:Keywords/1:Keywords"),
                ("morph_keys_path", "NPC_/71:Morph Keys"),
                ("morph_values_path", "NPC_/72:Morph Values"),
            ],
            _ => {
                return Err(SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: "Fallout NPC migration requires a guarded game".to_owned(),
                });
            }
        };
        for &(key, expected) in &expected_paths {
            let actual = invocation
                .context
                .configuration
                .get(key)
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: format!("Fallout NPC migration requires {key}"),
                })?;
            if actual != expected {
                return Err(SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: format!(
                        "Fallout NPC migration requires materialized {key} {expected}"
                    ),
                });
            }
        }
        let record = invocation
            .source_writable_record
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "Fallout NPC migration requires a writable record".to_owned(),
            })?;
        if record.flags.contains(RecordFlags::DELETED) || record.subrecords.is_empty() {
            return Ok(HandlerOutput::None);
        }
        let mut mutations = Vec::new();
        let has_keyword_count = record
            .subrecords
            .iter()
            .any(|subrecord| subrecord.signature == Signature(*b"KSIZ"));
        if !has_keyword_count
            && record
                .subrecords
                .iter()
                .any(|subrecord| subrecord.signature == Signature(*b"KWDA"))
        {
            mutations.push(HandlerMutation::RemoveFirstBySignature {
                path: "NPC_".to_owned(),
                signature: Signature(*b"KWDA"),
            });
        }
        let master_keys = self.resolver.as_ref().and_then(|resolver| {
            resolver.source_master_morph_keys(handler_record_context(&invocation.context))
        });
        if let Some(master_keys) = master_keys {
            append_fallout_npc_morph_mutations(
                &mut mutations,
                record,
                &master_keys,
                expected_paths[2].1,
                expected_paths[3].1,
            );
        }
        if mutations.is_empty() {
            Ok(HandlerOutput::None)
        } else {
            Ok(HandlerOutput::Mutations(mutations))
        }
    }
}

fn append_fallout_npc_morph_mutations(
    mutations: &mut Vec<HandlerMutation>,
    record: &WritableRecord,
    master_keys: &[u32],
    keys_path: &str,
    values_path: &str,
) {
    let Some(keys) = record
        .subrecords
        .iter()
        .find(|subrecord| subrecord.signature == Signature(*b"MSDK"))
    else {
        return;
    };
    let Some(values) = record
        .subrecords
        .iter()
        .find(|subrecord| subrecord.signature == Signature(*b"MSDV"))
    else {
        return;
    };
    if keys.data.len() % 4 != 0
        || values.data.len() % 4 != 0
        || keys.data.len() != values.data.len()
        || keys.data.len() / 4 < master_keys.len()
    {
        return;
    }
    let current_keys = keys
        .data
        .chunks_exact(4)
        .map(|bytes| u32::from_le_bytes(bytes.try_into().expect("four-byte morph key")))
        .collect::<Vec<_>>();
    let mut master_positions = BTreeMap::new();
    for (index, key) in master_keys.iter().copied().enumerate() {
        if master_positions.insert(key, index).is_some() {
            return;
        }
    }
    let mut next_unknown = master_keys.len();
    let mut needs_sort = false;
    let mut sort_orders = Vec::with_capacity(current_keys.len());
    for (index, key) in current_keys.iter().enumerate() {
        if let Some(order) = master_positions.get(key).copied() {
            needs_sort |= order != index;
            sort_orders.push(order);
        } else {
            sort_orders.push(next_unknown);
            next_unknown += 1;
        }
    }
    if !needs_sort || next_unknown != current_keys.len() {
        return;
    }
    let mut indices = (0..current_keys.len()).collect::<Vec<_>>();
    indices.sort_by_key(|index| sort_orders[*index]);
    let reorder = |data: &[u8]| {
        let mut reordered = Vec::with_capacity(data.len());
        for index in &indices {
            reordered.extend_from_slice(&data[index * 4..index * 4 + 4]);
        }
        reordered
    };
    mutations.push(HandlerMutation::ReplacePayload {
        path: keys_path.to_owned(),
        occurrence: 0,
        data: reorder(&keys.data),
    });
    mutations.push(HandlerMutation::ReplacePayload {
        path: values_path.to_owned(),
        occurrence: 0,
        data: reorder(&values.data),
    });
}

struct LegacyMagicEffectAfterLoad;

impl SemanticHandler for LegacyMagicEffectAfterLoad {
    fn id(&self) -> &'static str {
        "migrate.legacy_magic_effect_after_load"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::AfterLoad {
            return Ok(HandlerOutput::None);
        }
        if invocation.context.record_signature != Signature(*b"MGEF")
            || invocation.context.binding.path != "MGEF"
            || !matches!(
                invocation.context.game,
                SchemaGame::Fallout3 | SchemaGame::FalloutNv
            )
        {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "legacy MGEF migration requires a guarded MGEF root binding".to_owned(),
            });
        }
        if invocation.source_subrecord_index.is_some() {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "legacy MGEF migration requires a record-level binding".to_owned(),
            });
        }
        let record = invocation
            .source_writable_record
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "legacy MGEF migration requires a writable record".to_owned(),
            })?;
        if record.flags.contains(RecordFlags::DELETED) || record.subrecords.is_empty() {
            return Ok(HandlerOutput::None);
        }
        for (key, expected) in [
            ("data_path", "MGEF/5:Data"),
            ("archetype_path", "MGEF/5:Data/payload/17:Archtype"),
            ("actor_value_path", "MGEF/5:Data/payload/18:Actor Value"),
        ] {
            let actual = invocation
                .context
                .configuration
                .get(key)
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: format!("legacy MGEF migration requires {key}"),
                })?;
            if actual != expected {
                return Err(SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: format!(
                        "legacy MGEF migration requires materialized {key} {expected}"
                    ),
                });
            }
        }
        let Some(data) = record
            .subrecords
            .iter()
            .find(|subrecord| subrecord.signature == Signature(*b"DATA"))
        else {
            return Ok(HandlerOutput::None);
        };
        let (Some(archetype_bytes), Some(actor_value_bytes)) =
            (data.data.get(64..68), data.data.get(68..72))
        else {
            return Ok(HandlerOutput::None);
        };
        let archetype = u32::from_le_bytes(
            archetype_bytes
                .try_into()
                .expect("four-byte MGEF archetype slice was checked above"),
        );
        let actor_value = i32::from_le_bytes(
            actor_value_bytes
                .try_into()
                .expect("four-byte MGEF actor-value slice was checked above"),
        );
        let Some(expected) = legacy_magic_effect_actor_value(invocation.context.game, archetype)
        else {
            return Ok(HandlerOutput::None);
        };
        if actor_value == expected {
            return Ok(HandlerOutput::None);
        }
        let mut payload = data.data.clone();
        payload[68..72].copy_from_slice(&expected.to_le_bytes());
        Ok(HandlerOutput::Mutations(vec![
            HandlerMutation::ReplacePayload {
                path: "MGEF/5:Data".to_owned(),
                occurrence: 0,
                data: payload,
            },
        ]))
    }
}

fn legacy_magic_effect_actor_value(game: SchemaGame, archetype: u32) -> Option<i32> {
    match archetype {
        1 | 2 | 3 | 13 | 16 | 17 | 18 | 19 | 30 | 31 | 32 | 33 => Some(-1),
        11 => Some(48),
        12 => Some(49),
        24 => Some(47),
        35 if game == SchemaGame::FalloutNv => Some(-1),
        36 if game == SchemaGame::FalloutNv => Some(51),
        _ => None,
    }
}

struct SkyrimReferenceAfterLoad;

impl SemanticHandler for SkyrimReferenceAfterLoad {
    fn id(&self) -> &'static str {
        "migrate.skyrim_reference_after_load"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::AfterLoad {
            return Ok(HandlerOutput::None);
        }
        if invocation.context.record_signature != Signature(*b"REFR")
            || invocation.context.binding.path != "REFR"
            || !matches!(
                invocation.context.game,
                SchemaGame::SkyrimLe | SchemaGame::SkyrimSe | SchemaGame::SkyrimVr
            )
        {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "Skyrim reference migration requires a guarded REFR root binding"
                    .to_owned(),
            });
        }
        if invocation.source_subrecord_index.is_some() {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "Skyrim reference migration requires a record-level binding".to_owned(),
            });
        }
        let record = invocation
            .source_writable_record
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "Skyrim reference migration requires a writable record".to_owned(),
            })?;
        if record.flags.contains(RecordFlags::DELETED) || record.subrecords.is_empty() {
            return Ok(HandlerOutput::None);
        }
        for (key, expected) in [
            ("lock_path", "REFR/37:Lock Data"),
            ("lock_level_path", "REFR/37:Lock Data/payload/0:Level"),
            ("portal_path", "REFR/8:Room Portal (unused)"),
        ] {
            let actual = invocation
                .context
                .configuration
                .get(key)
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: format!("Skyrim reference migration requires {key}"),
                })?;
            if actual != expected {
                return Err(SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: format!(
                        "Skyrim reference migration requires materialized {key} {expected}"
                    ),
                });
            }
        }
        let Some(lock) = record
            .subrecords
            .iter()
            .find(|subrecord| subrecord.signature == Signature(*b"XLOC"))
        else {
            return Ok(HandlerOutput::None);
        };
        let mut mutations = Vec::new();
        if lock.data.first() == Some(&0) {
            let mut payload = lock.data.clone();
            payload[0] = 1;
            mutations.push(HandlerMutation::ReplacePayload {
                path: "REFR/37:Lock Data".to_owned(),
                occurrence: 0,
                data: payload,
            });
        }
        if record
            .subrecords
            .iter()
            .any(|subrecord| subrecord.signature == Signature(*b"XPTL"))
        {
            mutations.push(HandlerMutation::RemoveFirstBySignature {
                path: "REFR".to_owned(),
                signature: Signature(*b"XPTL"),
            });
        }
        if mutations.is_empty() {
            Ok(HandlerOutput::None)
        } else {
            Ok(HandlerOutput::Mutations(mutations))
        }
    }
}

struct FalloutReferenceAfterLoad {
    resolver: Option<Arc<dyn FormLinkResolver>>,
}

impl SemanticHandler for FalloutReferenceAfterLoad {
    fn id(&self) -> &'static str {
        "migrate.fallout_reference_after_load"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::AfterLoad {
            return Ok(HandlerOutput::None);
        }
        if invocation.context.record_signature != Signature(*b"REFR")
            || invocation.context.binding.path != "REFR"
        {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "Fallout reference migration requires a guarded REFR root binding"
                    .to_owned(),
            });
        }
        if invocation.source_subrecord_index.is_some() {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "Fallout reference migration requires a record-level binding".to_owned(),
            });
        }
        let expected_paths = match invocation.context.game {
            SchemaGame::Fallout3 => Some((
                "legacy",
                [
                    ("unused_path", "REFR/1:Unused"),
                    ("base_path", "REFR/2:Base"),
                    ("ammo_path", "REFR/25:Ammo"),
                    ("ammo_type_path", "REFR/25:Ammo/0:Type"),
                    ("ammo_count_path", "REFR/25:Ammo/1:Count"),
                ],
            )),
            SchemaGame::FalloutNv => Some((
                "legacy",
                [
                    ("unused_path", "REFR/1:Unused"),
                    ("base_path", "REFR/2:Base"),
                    ("ammo_path", "REFR/26:Ammo"),
                    ("ammo_type_path", "REFR/26:Ammo/0:Type"),
                    ("ammo_count_path", "REFR/26:Ammo/1:Count"),
                ],
            )),
            _ => None,
        };
        if let Some((mode, paths)) = expected_paths {
            verify_fallout_reference_configuration(&invocation, mode, &paths)?;
            return self.invoke_legacy(invocation, paths[2].1);
        }
        let lock_path = match invocation.context.game {
            SchemaGame::Fallout4 | SchemaGame::Fallout4Vr => "REFR/39:Lock Data",
            SchemaGame::Fallout76 => "REFR/44:Lock Data",
            _ => {
                return Err(SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: "Fallout reference migration requires a guarded game".to_owned(),
                });
            }
        };
        let level_path = format!("{lock_path}/payload/0:Level");
        let paths = [
            ("lock_path", lock_path),
            ("lock_level_path", level_path.as_str()),
        ];
        verify_fallout_reference_configuration(&invocation, "lock", &paths)?;
        self.invoke_lock(invocation, lock_path)
    }
}

impl FalloutReferenceAfterLoad {
    fn invoke_legacy(
        &self,
        invocation: HandlerInvocation<'_>,
        ammo_path: &str,
    ) -> Result<HandlerOutput> {
        let record = invocation
            .source_writable_record
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "Fallout reference migration requires a writable record".to_owned(),
            })?;
        if record.flags.contains(RecordFlags::DELETED) || record.subrecords.is_empty() {
            return Ok(HandlerOutput::None);
        }
        let mut mutations = Vec::new();
        if record
            .subrecords
            .iter()
            .any(|subrecord| subrecord.signature == Signature(*b"RCLR"))
        {
            mutations.push(HandlerMutation::RemoveFirstBySignature {
                path: "REFR".to_owned(),
                signature: Signature(*b"RCLR"),
            });
        }
        let has_ammo = record
            .subrecords
            .iter()
            .any(|subrecord| subrecord.signature == Signature(*b"XAMT"));
        if has_ammo {
            let base_form_id = record
                .subrecords
                .iter()
                .find(|subrecord| subrecord.signature == Signature(*b"NAME"))
                .and_then(|subrecord| subrecord.data.get(..4))
                .map(|bytes| FormId(u32::from_le_bytes(bytes.try_into().expect("four bytes"))));
            let base_signature = base_form_id
                .and_then(|form_id| {
                    self.resolver.as_ref().and_then(|resolver| {
                        resolver.resolve_form_id(
                            handler_record_context(&invocation.context),
                            form_id,
                            &[],
                        )
                    })
                })
                .and_then(|link| link.signature());
            if base_signature.is_some_and(|signature| signature != Signature(*b"WEAP")) {
                mutations.push(HandlerMutation::RemoveFirstBySignature {
                    path: ammo_path.to_owned(),
                    signature: Signature(*b"XAMT"),
                });
                if record
                    .subrecords
                    .iter()
                    .any(|subrecord| subrecord.signature == Signature(*b"XAMC"))
                {
                    mutations.push(HandlerMutation::RemoveFirstBySignature {
                        path: ammo_path.to_owned(),
                        signature: Signature(*b"XAMC"),
                    });
                }
            }
        }
        if mutations.is_empty() {
            Ok(HandlerOutput::None)
        } else {
            Ok(HandlerOutput::Mutations(mutations))
        }
    }

    fn invoke_lock(
        &self,
        invocation: HandlerInvocation<'_>,
        lock_path: &str,
    ) -> Result<HandlerOutput> {
        let record = invocation
            .source_writable_record
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "Fallout reference migration requires a writable record".to_owned(),
            })?;
        if record.flags.contains(RecordFlags::DELETED) || record.subrecords.is_empty() {
            return Ok(HandlerOutput::None);
        }
        let Some(lock) = record
            .subrecords
            .iter()
            .find(|subrecord| subrecord.signature == Signature(*b"XLOC"))
        else {
            return Ok(HandlerOutput::None);
        };
        if lock.data.first() != Some(&0) {
            return Ok(HandlerOutput::None);
        }
        let mut data = lock.data.clone();
        data[0] = 1;
        Ok(HandlerOutput::Mutations(vec![
            HandlerMutation::ReplacePayload {
                path: lock_path.to_owned(),
                occurrence: 0,
                data,
            },
        ]))
    }
}

fn verify_fallout_reference_configuration(
    invocation: &HandlerInvocation<'_>,
    expected_mode: &str,
    expected_paths: &[(&str, &str)],
) -> Result<()> {
    let handler = "migrate.fallout_reference_after_load";
    let mode = invocation
        .context
        .configuration
        .get("mode")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| SemanticError::Handler {
            handler: handler.to_owned(),
            message: "Fallout reference migration requires mode".to_owned(),
        })?;
    if mode != expected_mode {
        return Err(SemanticError::Handler {
            handler: handler.to_owned(),
            message: format!("Fallout reference migration requires {expected_mode} mode"),
        });
    }
    for (key, expected) in expected_paths {
        let actual = invocation
            .context
            .configuration
            .get(key)
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| SemanticError::Handler {
                handler: handler.to_owned(),
                message: format!("Fallout reference migration requires {key}"),
            })?;
        if actual != *expected {
            return Err(SemanticError::Handler {
                handler: handler.to_owned(),
                message: format!(
                    "Fallout reference migration requires materialized {key} {expected}"
                ),
            });
        }
    }
    Ok(())
}

struct FalloutSceneBehaviorAfterLoad;

impl SemanticHandler for FalloutSceneBehaviorAfterLoad {
    fn id(&self) -> &'static str {
        "migrate.fallout_scene_behavior_after_load"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::AfterLoad {
            return Ok(HandlerOutput::None);
        }
        let path = invocation.context.binding.path.as_str();
        let expected_offset = match path {
            "SCEN/8:Actor Behavior Settings/payload/2:Player Dialogue" => 8,
            "SCEN/8:Actor Behavior Settings/payload/3:Observe Combat" => 12,
            _ => {
                return Err(SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: "Fallout scene behavior migration requires a guarded SCEN field"
                        .to_owned(),
                });
            }
        };
        if invocation.context.record_signature != Signature(*b"SCEN")
            || !matches!(
                invocation.context.game,
                SchemaGame::Fallout4 | SchemaGame::Fallout4Vr
            )
        {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "Fallout scene behavior migration is only valid for FO4 SCEN records"
                    .to_owned(),
            });
        }
        let configured_offset = invocation
            .context
            .configuration
            .get("field_offset")
            .and_then(serde_json::Value::as_u64)
            .and_then(|offset| usize::try_from(offset).ok())
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "Fallout scene behavior migration requires field_offset".to_owned(),
            })?;
        if configured_offset != expected_offset {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: format!(
                    "Fallout scene behavior field {path} requires byte offset {expected_offset}"
                ),
            });
        }
        let record = invocation
            .source_writable_record
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "Fallout scene behavior migration requires a writable record".to_owned(),
            })?;
        let index = invocation
            .source_subrecord_index
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "Fallout scene behavior migration requires a source subrecord".to_owned(),
            })?;
        let behavior = record
            .subrecords
            .get(index)
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: format!("source subrecord index {index} is out of bounds"),
            })?;
        if behavior.signature != Signature(*b"VNAM") {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: format!(
                    "Fallout scene behavior migration requires VNAM, got {}",
                    behavior.signature
                ),
            });
        }
        let Some(bytes) = behavior
            .data
            .get(configured_offset..configured_offset.saturating_add(4))
        else {
            return Ok(HandlerOutput::None);
        };
        let value = u32::from_le_bytes(
            bytes
                .try_into()
                .expect("four-byte scene behavior value must convert"),
        );
        if value <= 3 {
            return Ok(HandlerOutput::None);
        }
        let mut normalized = behavior.data.clone();
        normalized[configured_offset..configured_offset + 4].copy_from_slice(&3_u32.to_le_bytes());
        Ok(HandlerOutput::SubrecordPayload(normalized))
    }
}

struct RemoveOffsetDataAfterLoad {
    enabled: bool,
}

impl SemanticHandler for RemoveOffsetDataAfterLoad {
    fn id(&self) -> &'static str {
        "migrate.remove_offset_data"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::AfterLoad {
            return Ok(HandlerOutput::None);
        }
        if invocation.context.record_signature != Signature(*b"TES4")
            || invocation.context.binding.path != "TES4"
            || !matches!(
                invocation.context.game,
                SchemaGame::Oblivion
                    | SchemaGame::Fallout3
                    | SchemaGame::FalloutNv
                    | SchemaGame::SkyrimLe
                    | SchemaGame::SkyrimSe
                    | SchemaGame::SkyrimVr
                    | SchemaGame::Fallout4
                    | SchemaGame::Fallout4Vr
                    | SchemaGame::Fallout76
                    | SchemaGame::Starfield
            )
        {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "offset-data removal is only valid for modern TES4 headers".to_owned(),
            });
        }
        if invocation.source_subrecord_index.is_some() {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "offset-data removal requires a record-level binding".to_owned(),
            });
        }
        let record = invocation
            .source_writable_record
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "offset-data removal requires a writable record".to_owned(),
            })?;
        if !self.enabled
            || !record
                .subrecords
                .iter()
                .any(|subrecord| subrecord.signature == Signature(*b"OFST"))
        {
            return Ok(HandlerOutput::None);
        }
        Ok(HandlerOutput::Mutations(vec![
            HandlerMutation::RemoveFirstBySignature {
                path: "TES4".to_owned(),
                signature: Signature(*b"OFST"),
            },
        ]))
    }
}

struct RemoveWorldspaceOffsetDataAfterLoad {
    enabled: bool,
}

impl SemanticHandler for RemoveWorldspaceOffsetDataAfterLoad {
    fn id(&self) -> &'static str {
        "migrate.remove_worldspace_offset_data"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::AfterLoad {
            return Ok(HandlerOutput::None);
        }
        if invocation.context.record_signature != Signature(*b"WRLD")
            || invocation.context.binding.path != "WRLD"
            || !matches!(
                invocation.context.game,
                SchemaGame::Oblivion
                    | SchemaGame::Fallout3
                    | SchemaGame::FalloutNv
                    | SchemaGame::SkyrimLe
                    | SchemaGame::SkyrimSe
                    | SchemaGame::SkyrimVr
                    | SchemaGame::Fallout4
                    | SchemaGame::Fallout4Vr
                    | SchemaGame::Fallout76
                    | SchemaGame::Starfield
            )
        {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "worldspace offset cleanup requires a modern WRLD record".to_owned(),
            });
        }
        if invocation.source_subrecord_index.is_some() {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "worldspace offset cleanup requires a record-level binding".to_owned(),
            });
        }
        let record = invocation
            .source_writable_record
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "worldspace offset cleanup requires a writable record".to_owned(),
            })?;
        if !self.enabled
            || !record
                .subrecords
                .iter()
                .any(|subrecord| subrecord.signature == Signature(*b"OFST"))
        {
            return Ok(HandlerOutput::None);
        }
        Ok(HandlerOutput::Mutations(vec![
            HandlerMutation::RemoveFirstBySignature {
                path: "WRLD".to_owned(),
                signature: Signature(*b"OFST"),
            },
        ]))
    }
}

struct WorldspaceAfterLoad {
    remove_offset_data: bool,
    source_file_load_order: Option<u32>,
}

impl SemanticHandler for WorldspaceAfterLoad {
    fn id(&self) -> &'static str {
        "migrate.worldspace_after_load"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::AfterLoad {
            return Ok(HandlerOutput::None);
        }
        if invocation.context.record_signature != Signature(*b"WRLD")
            || invocation.context.binding.path != "WRLD"
            || !matches!(
                invocation.context.game,
                SchemaGame::SkyrimLe
                    | SchemaGame::SkyrimSe
                    | SchemaGame::SkyrimVr
                    | SchemaGame::Fallout4
                    | SchemaGame::Fallout4Vr
                    | SchemaGame::Fallout76
                    | SchemaGame::Starfield
            )
        {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "worldspace migration requires a supported WRLD record".to_owned(),
            });
        }
        if invocation.source_subrecord_index.is_some() {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "worldspace migration requires a record-level binding".to_owned(),
            });
        }
        let record = invocation
            .source_writable_record
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "worldspace migration requires a writable record".to_owned(),
            })?;
        let mut mutations = Vec::new();
        let has_signature = |signature| {
            record
                .subrecords
                .iter()
                .any(|subrecord| subrecord.signature == signature)
        };
        if self.remove_offset_data
            && invocation.context.game == SchemaGame::Starfield
            && has_signature(Signature(*b"OFST"))
        {
            mutations.push(HandlerMutation::RemoveFirstBySignature {
                path: "WRLD".to_owned(),
                signature: Signature(*b"OFST"),
            });
        }
        if has_signature(Signature(*b"RNAM")) {
            let remove_rnam = match invocation.context.game {
                SchemaGame::SkyrimLe | SchemaGame::Fallout4 | SchemaGame::Fallout4Vr => true,
                SchemaGame::SkyrimSe | SchemaGame::SkyrimVr => {
                    self.source_file_load_order
                        .ok_or_else(|| SemanticError::Handler {
                            handler: self.id().to_owned(),
                            message:
                                "Skyrim large-reference cleanup requires the source-file load \
                                  order"
                                    .to_owned(),
                        })?
                        == 0
                }
                _ => false,
            };
            if remove_rnam {
                mutations.push(HandlerMutation::RemoveFirstBySignature {
                    path: "WRLD".to_owned(),
                    signature: Signature(*b"RNAM"),
                });
            }
        }
        let remove_clsz = match invocation.context.game {
            SchemaGame::Fallout4 | SchemaGame::Fallout4Vr => true,
            SchemaGame::Starfield => self.remove_offset_data,
            _ => false,
        };
        if remove_clsz && has_signature(Signature(*b"CLSZ")) {
            mutations.push(HandlerMutation::RemoveFirstBySignature {
                path: "WRLD".to_owned(),
                signature: Signature(*b"CLSZ"),
            });
        }
        if mutations.is_empty() {
            Ok(HandlerOutput::None)
        } else {
            Ok(HandlerOutput::Mutations(mutations))
        }
    }
}

struct RegionPointOrderAfterLoad;

impl SemanticHandler for RegionPointOrderAfterLoad {
    fn id(&self) -> &'static str {
        "migrate.region_point_order"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::AfterLoad {
            return Ok(HandlerOutput::None);
        }
        let game = invocation.context.game;
        if invocation.context.record_signature != Signature(*b"REGN")
            || !invocation.context.binding.path.starts_with("REGN/")
            || !matches!(
                game,
                SchemaGame::Oblivion
                    | SchemaGame::Fallout3
                    | SchemaGame::FalloutNv
                    | SchemaGame::SkyrimLe
                    | SchemaGame::SkyrimSe
                    | SchemaGame::SkyrimVr
                    | SchemaGame::Fallout4
                    | SchemaGame::Fallout4Vr
                    | SchemaGame::Fallout76
                    | SchemaGame::Starfield
            )
        {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "region point ordering is only valid for modern REGN records".to_owned(),
            });
        }
        let record = invocation
            .source_writable_record
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "region point ordering requires a writable record".to_owned(),
            })?;
        let index = invocation
            .source_subrecord_index
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "region point ordering requires a source subrecord".to_owned(),
            })?;
        let points = record
            .subrecords
            .get(index)
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: format!("source subrecord index {index} is out of bounds"),
            })?;
        if points.signature != Signature(*b"RPLD") || points.data.len() % 8 != 0 {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: format!(
                    "region point ordering requires 8-byte RPLD points, got {} bytes of {}",
                    points.data.len(),
                    points.signature
                ),
            });
        }
        if points.data.len() <= 8 {
            return Ok(HandlerOutput::None);
        }
        let first = region_point(&points.data[..8]);
        let last = region_point(&points.data[points.data.len() - 8..]);
        let display_round = game != SchemaGame::Oblivion;
        let first_x = region_comparison_value(first.0, display_round);
        let last_x = region_comparison_value(last.0, display_round);
        let reverse = match system_math_single_compare(first_x, last_x) {
            std::cmp::Ordering::Equal => {
                let first_y = region_comparison_value(first.1, display_round);
                let last_y = region_comparison_value(last.1, display_round);
                system_math_single_compare(first_y, last_y) == std::cmp::Ordering::Greater
            }
            std::cmp::Ordering::Greater => true,
            std::cmp::Ordering::Less => false,
        };
        if !reverse {
            return Ok(HandlerOutput::None);
        }
        let normalized = points
            .data
            .chunks_exact(8)
            .rev()
            .flat_map(|point| point.iter().copied())
            .collect();
        Ok(HandlerOutput::SubrecordPayload(normalized))
    }
}

fn region_point(data: &[u8]) -> (f32, f32) {
    let x = f32::from_le_bytes(
        data[..4]
            .try_into()
            .expect("four-byte region X coordinate must convert"),
    );
    let y = f32::from_le_bytes(
        data[4..8]
            .try_into()
            .expect("four-byte region Y coordinate must convert"),
    );
    (x, y)
}

fn region_comparison_value(value: f32, display_round: bool) -> f32 {
    if display_round {
        crate::value::float_from_raw(f64::from(value), 1.0, 6) as f32
    } else {
        value
    }
}

fn system_math_single_compare(left: f32, right: f32) -> std::cmp::Ordering {
    const SINGLE_RESOLUTION: f64 = 0.0001;
    let left = f64::from(left);
    let right = f64::from(right);
    let tolerance = (left.abs().min(right.abs()) * SINGLE_RESOLUTION).max(SINGLE_RESOLUTION);
    if (left - right).abs() <= tolerance {
        std::cmp::Ordering::Equal
    } else if left < right {
        std::cmp::Ordering::Less
    } else {
        // Delphi's FUCOMPP path returns GreaterThanValue for unordered operands.
        std::cmp::Ordering::Greater
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
        let configuration = invocation.context.configuration;
        let run_on_path = configured_text(self.id(), configuration, "run_on_path")?;
        if run_on_path != invocation.context.binding.path {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "CTDA Run On configuration does not match its binding".to_owned(),
            });
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
        let reference_run_on = i128::from(configured_u64(
            self.id(),
            configuration,
            "reference_run_on",
        )?);
        if old_value == Some(new_value) || new_value == reference_run_on {
            return Ok(HandlerOutput::None);
        }
        Ok(HandlerOutput::Mutations(vec![HandlerMutation::Set {
            path: configured_text(self.id(), configuration, "reference_path")?.to_owned(),
            occurrence: 0,
            value: OwnedFieldValue::UInt(configured_u64(
                self.id(),
                configuration,
                "null_reference",
            )?),
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
        let configuration = invocation.context.configuration;
        let type_path = configured_text(self.id(), configuration, "type_path")?;
        if type_path != invocation.context.binding.path {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "CTDA Type configuration does not match its binding".to_owned(),
            });
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
        let mut mutations = Vec::new();
        let global_value_mask = configured_u64(self.id(), configuration, "global_value_mask")?;
        if (old_value & global_value_mask) != (new_value & global_value_mask) {
            mutations.push(HandlerMutation::Set {
                path: configured_text(self.id(), configuration, "comparison_path")?.to_owned(),
                occurrence: 0,
                value: OwnedFieldValue::UInt(configured_u64(
                    self.id(),
                    configuration,
                    "comparison_default",
                )?),
            });
        }
        let legacy_use_global = configured_bool(self.id(), configuration, "legacy_use_global")?;
        if legacy_use_global {
            let legacy_mask = configured_u64(self.id(), configuration, "legacy_use_global_mask")?;
            if new_value & legacy_mask == 0 {
                return if mutations.is_empty() {
                    Ok(HandlerOutput::None)
                } else {
                    Ok(HandlerOutput::Mutations(mutations))
                };
            }
            mutations.push(HandlerMutation::Set {
                path: configured_text(self.id(), configuration, "run_on_path")?.to_owned(),
                occurrence: 0,
                value: OwnedFieldValue::UInt(configured_u64(
                    self.id(),
                    configuration,
                    "subject_run_on",
                )?),
            });
            mutations.push(HandlerMutation::Set {
                path: type_path.to_owned(),
                occurrence: 0,
                value: OwnedFieldValue::UInt(new_value & !legacy_mask),
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

struct GameSettingEditorIdAfterSet;

impl SemanticHandler for GameSettingEditorIdAfterSet {
    fn id(&self) -> &'static str {
        "edit.game_setting_editor_id"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::AfterSet {
            return Ok(HandlerOutput::None);
        }
        let configuration = invocation.context.configuration;
        let editor_id_path = configured_text(self.id(), configuration, "editor_id_path")?;
        if editor_id_path != invocation.context.binding.path {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "game-setting editor-ID configuration does not match its binding"
                    .to_owned(),
            });
        }
        let Some(FieldValue::String(new_value)) = invocation.value else {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "game-setting editor-ID updates require a string value".to_owned(),
            });
        };
        let Some(FieldValue::String(old_value)) = invocation.old_value else {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "game-setting editor-ID updates require the previous string value"
                    .to_owned(),
            });
        };
        if old_value == new_value || old_value.chars().next() == new_value.chars().next() {
            return Ok(HandlerOutput::None);
        }
        let data_path = configured_text(self.id(), configuration, "data_path")?.to_owned();
        Ok(HandlerOutput::Mutations(vec![
            HandlerMutation::Remove {
                path: data_path.clone(),
                occurrence: 0,
            },
            HandlerMutation::InsertDefault { path: data_path },
        ]))
    }
}

struct PerkEffectTypeAfterSet;

impl SemanticHandler for PerkEffectTypeAfterSet {
    fn id(&self) -> &'static str {
        "edit.perk_effect_type"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::AfterSet {
            return Ok(HandlerOutput::None);
        }
        let configuration = invocation.context.configuration;
        let type_path = configured_text(self.id(), configuration, "type_path")?;
        if type_path != invocation.context.binding.path {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "perk effect type configuration does not match its binding path"
                    .to_owned(),
            });
        }
        let new_type = invocation
            .value
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "perk effect type updates require a new value".to_owned(),
            })
            .and_then(|value| callback_integer(value, self.id()))?;
        let old_type = invocation
            .old_value
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "perk effect type updates require the previous value".to_owned(),
            })
            .and_then(|value| callback_integer(value, self.id()))?;
        if old_type == new_type {
            return Ok(HandlerOutput::None);
        }
        let data_path = configured_text(self.id(), configuration, "data_path")?;
        let conditions_path = configured_text(self.id(), configuration, "conditions_path")?;
        let parameters_path = configured_text(self.id(), configuration, "parameters_path")?;
        let mut mutations = vec![
            HandlerMutation::ResetToDefault {
                path: data_path.to_owned(),
                occurrence: 0,
            },
            HandlerMutation::RemoveContainer {
                path: conditions_path.to_owned(),
            },
            HandlerMutation::RemoveContainer {
                path: parameters_path.to_owned(),
            },
        ];
        let entry_point_type = i128::from(configured_i64(
            self.id(),
            configuration,
            "entry_point_type",
        )?);
        if new_type == entry_point_type {
            mutations.push(HandlerMutation::InsertDefault {
                path: configured_text(self.id(), configuration, "parameter_type_path")?.to_owned(),
            });
            mutations.push(HandlerMutation::Set {
                path: configured_text(self.id(), configuration, "function_path")?.to_owned(),
                occurrence: 0,
                value: OwnedFieldValue::Int(configured_i64(
                    self.id(),
                    configuration,
                    "entry_point_function",
                )?),
            });
        }
        Ok(HandlerOutput::Mutations(mutations))
    }
}

struct MagicEffectAssocItemAfterSet;

impl SemanticHandler for MagicEffectAssocItemAfterSet {
    fn id(&self) -> &'static str {
        "edit.magic_effect_assoc_item"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::AfterSet {
            return Ok(HandlerOutput::None);
        }
        let assoc_item_path = configured_text(
            self.id(),
            invocation.context.configuration,
            "assoc_item_path",
        )?;
        if assoc_item_path != invocation.context.binding.path {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "associated-item configuration does not match its binding path".to_owned(),
            });
        }
        let Some(FieldValue::FormId { value, .. }) = invocation.value else {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "associated-item updates require a FormID value".to_owned(),
            });
        };
        if value.0 == 0 {
            return Ok(HandlerOutput::None);
        }
        let archetype_path = configured_text(
            self.id(),
            invocation.context.configuration,
            "archetype_path",
        )?;
        let unset_archetype = configured_i64(
            self.id(),
            invocation.context.configuration,
            "unset_archetype",
        )?;
        let generic_archetype = configured_i64(
            self.id(),
            invocation.context.configuration,
            "generic_archetype",
        )?;
        Ok(HandlerOutput::Mutations(vec![
            HandlerMutation::SetIfEqual {
                path: archetype_path.to_owned(),
                occurrence: 0,
                expected: OwnedFieldValue::Int(unset_archetype),
                value: OwnedFieldValue::Int(generic_archetype),
            },
        ]))
    }
}

struct PackageInputTypeAfterSet;

impl SemanticHandler for PackageInputTypeAfterSet {
    fn id(&self) -> &'static str {
        "edit.package_input_type"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::AfterSet {
            return Ok(HandlerOutput::None);
        }
        let type_path = configured_text(self.id(), invocation.context.configuration, "type_path")?;
        if type_path != invocation.context.binding.path {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "package-input configuration does not match its binding path".to_owned(),
            });
        }
        let Some(FieldValue::String(new_value)) = invocation.value else {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "package-input updates require a new string value".to_owned(),
            });
        };
        let Some(FieldValue::String(old_value)) = invocation.old_value else {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "package-input updates require the previous string value".to_owned(),
            });
        };
        if old_value == new_value {
            return Ok(HandlerOutput::None);
        }
        let record = invocation
            .source_writable_record
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "package-input updates require the writable source record".to_owned(),
            })?;
        let index = invocation
            .source_subrecord_index
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "package-input updates require the source ANAM index".to_owned(),
            })?;
        let type_signature = configured_signature(
            self.id(),
            invocation.context.configuration,
            "type_signature",
        )?;
        let value_signature = configured_signature(
            self.id(),
            invocation.context.configuration,
            "value_signature",
        )?;
        if record
            .subrecords
            .get(index)
            .map(|subrecord| subrecord.signature)
            != Some(type_signature)
        {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "package-input callback source is not its materialized type".to_owned(),
            });
        }
        let value_present = record
            .subrecords
            .get(index.saturating_add(1))
            .is_some_and(|subrecord| subrecord.signature == value_signature);
        let value_path =
            configured_text(self.id(), invocation.context.configuration, "value_path")?;
        let value_types =
            configured_text_array(self.id(), invocation.context.configuration, "value_types")?;
        let requires_value = value_types.contains(&new_value.as_ref());
        let mutation = match (requires_value, value_present) {
            (true, true) => HandlerMutation::ResetToDefault {
                path: value_path.to_owned(),
                occurrence: 0,
            },
            (true, false) => HandlerMutation::InsertDefault {
                path: value_path.to_owned(),
            },
            (false, true) => HandlerMutation::Remove {
                path: value_path.to_owned(),
                occurrence: 0,
            },
            (false, false) => return Ok(HandlerOutput::None),
        };
        Ok(HandlerOutput::Mutations(vec![mutation]))
    }
}

struct QuestScriptNameAfterSet;

impl SemanticHandler for QuestScriptNameAfterSet {
    fn id(&self) -> &'static str {
        "edit.quest_script_name"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::AfterSet {
            return Ok(HandlerOutput::None);
        }
        let name_path = configured_text(self.id(), invocation.context.configuration, "name_path")?;
        if name_path != invocation.context.binding.path {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "quest script-name configuration does not match its binding".to_owned(),
            });
        }
        let Some(FieldValue::String(new_value)) = invocation.value else {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "quest script-name update requires a new string".to_owned(),
            });
        };
        let Some(FieldValue::String(old_value)) = invocation.old_value else {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "quest script-name update requires the previous string".to_owned(),
            });
        };
        if old_value == new_value || old_value.is_empty() == new_value.is_empty() {
            return Ok(HandlerOutput::None);
        }
        let script_path =
            configured_text(self.id(), invocation.context.configuration, "script_path")?;
        Ok(HandlerOutput::Mutations(vec![
            HandlerMutation::ResetToDefault {
                path: script_path.to_owned(),
                occurrence: 0,
            },
        ]))
    }
}

fn require_legacy_perk_invocation<'a>(
    handler: &str,
    invocation: &'a HandlerInvocation<'_>,
) -> Result<(&'a WritableRecord, usize)> {
    if invocation.context.record_signature != Signature(*b"PERK")
        || !matches!(
            invocation.context.game,
            SchemaGame::Fallout3 | SchemaGame::FalloutNv
        )
    {
        return Err(SemanticError::Handler {
            handler: handler.to_owned(),
            message: "legacy PERK callback requires a guarded Fallout 3 or New Vegas binding"
                .to_owned(),
        });
    }
    let record = invocation
        .source_writable_record
        .ok_or_else(|| SemanticError::Handler {
            handler: handler.to_owned(),
            message: "legacy PERK callback requires the writable source record".to_owned(),
        })?;
    let index = invocation
        .source_subrecord_index
        .ok_or_else(|| SemanticError::Handler {
            handler: handler.to_owned(),
            message: "legacy PERK callback requires its source subrecord index".to_owned(),
        })?;
    if index >= record.subrecords.len() {
        return Err(SemanticError::Handler {
            handler: handler.to_owned(),
            message: "legacy PERK source subrecord index is out of bounds".to_owned(),
        });
    }
    Ok((record, index))
}

fn legacy_perk_callback_i64(value: &FieldValue<'_>, handler: &str) -> Result<i64> {
    i64::try_from(callback_integer(value, handler)?).map_err(|_| SemanticError::Handler {
        handler: handler.to_owned(),
        message: "legacy PERK callback value exceeds i64".to_owned(),
    })
}

fn legacy_perk_effect_range(
    handler: &str,
    record: &WritableRecord,
    source_index: usize,
) -> Result<std::ops::Range<usize>> {
    let start = (0..=source_index)
        .rev()
        .find(|index| record.subrecords[*index].signature == Signature(*b"PRKE"))
        .ok_or_else(|| SemanticError::Handler {
            handler: handler.to_owned(),
            message: "legacy PERK callback has no preceding PRKE header".to_owned(),
        })?;
    let end = (source_index.saturating_add(1)..record.subrecords.len())
        .find(|index| record.subrecords[*index].signature == Signature(*b"PRKE"))
        .unwrap_or(record.subrecords.len());
    Ok(start..end)
}

fn legacy_perk_parameter_type(
    handler: &str,
    configuration: &serde_json::Value,
    record: &WritableRecord,
    range: &std::ops::Range<usize>,
) -> Result<i64> {
    let Some(subrecord) = record.subrecords[range.clone()]
        .iter()
        .find(|subrecord| subrecord.signature == Signature(*b"EPFT"))
    else {
        return Ok(0);
    };
    read_configured_integer(configuration, "parameter_type", &subrecord.data, handler)
}

fn legacy_perk_parameter_mutations(
    handler: &str,
    configuration: &serde_json::Value,
    record: &WritableRecord,
    range: &std::ops::Range<usize>,
    parameter_type: i64,
) -> Result<Vec<HandlerMutation>> {
    let mut mutations = Vec::new();
    for (key, signature) in [
        ("parameter_data_path", Signature(*b"EPFD")),
        ("button_label_path", Signature(*b"EPF2")),
        ("script_flags_path", Signature(*b"EPF3")),
    ] {
        if record.subrecords[range.clone()]
            .iter()
            .any(|subrecord| subrecord.signature == signature)
        {
            mutations.push(HandlerMutation::Remove {
                path: configured_text(handler, configuration, key)?.to_owned(),
                occurrence: 0,
            });
        }
    }
    mutations.push(HandlerMutation::RemoveContainer {
        path: configured_text(handler, configuration, "embedded_script_path")?.to_owned(),
    });
    match parameter_type {
        1..=3 => mutations.push(HandlerMutation::InsertDefault {
            path: configured_text(handler, configuration, "parameter_data_path")?.to_owned(),
        }),
        4 => {
            for key in [
                "button_label_path",
                "script_flags_path",
                "script_header_path",
            ] {
                mutations.push(HandlerMutation::InsertDefault {
                    path: configured_text(handler, configuration, key)?.to_owned(),
                });
            }
        }
        _ => {}
    }
    Ok(mutations)
}

fn legacy_perk_set_parameter_type(
    handler: &str,
    configuration: &serde_json::Value,
    record: &WritableRecord,
    range: &std::ops::Range<usize>,
    old_function: i64,
    new_function: i64,
) -> Result<Vec<HandlerMutation>> {
    let parameter_types = configured_u8_array(handler, configuration, "function_parameter_types")?;
    let new_index = usize::try_from(new_function)
        .ok()
        .filter(|index| *index < parameter_types.len());
    let Some(new_index) = new_index else {
        return Ok(Vec::new());
    };
    let new_parameter_type = i64::from(parameter_types[new_index]);
    let old_parameter_type = legacy_perk_parameter_type(handler, configuration, record, range)?;
    let parameter_type_path =
        configured_text(handler, configuration, "parameter_type_path")?.to_owned();
    let parameter_type_present = record.subrecords[range.clone()]
        .iter()
        .any(|subrecord| subrecord.signature == Signature(*b"EPFT"));
    let mut mutations = vec![if parameter_type_present {
        HandlerMutation::Set {
            path: parameter_type_path,
            occurrence: 0,
            value: OwnedFieldValue::Int(new_parameter_type),
        }
    } else {
        HandlerMutation::SynchronizePresence {
            path: parameter_type_path,
            occurrence: 0,
            present: true,
            value: OwnedFieldValue::Int(new_parameter_type),
        }
    }];
    let force_rebuild = old_function != new_function && matches!(new_function, 4 | 5);
    if old_parameter_type != new_parameter_type || force_rebuild {
        mutations.extend(legacy_perk_parameter_mutations(
            handler,
            configuration,
            record,
            range,
            new_parameter_type,
        )?);
    }
    Ok(mutations)
}

struct LegacyPerkEntryPointAfterSet;

impl SemanticHandler for LegacyPerkEntryPointAfterSet {
    fn id(&self) -> &'static str {
        "edit.legacy_perk_entry_point"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::AfterSet {
            return Ok(HandlerOutput::None);
        }
        let configuration = invocation.context.configuration;
        if configured_text(self.id(), configuration, "binding_path")?
            != invocation.context.binding.path
        {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "legacy PERK entry-point configuration does not match its binding"
                    .to_owned(),
            });
        }
        let new_entry_point = invocation
            .value
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "legacy PERK entry-point callback requires a new value".to_owned(),
            })
            .and_then(|value| legacy_perk_callback_i64(value, self.id()))?;
        let old_entry_point = invocation
            .old_value
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "legacy PERK entry-point callback requires the previous value".to_owned(),
            })
            .and_then(|value| legacy_perk_callback_i64(value, self.id()))?;
        if old_entry_point == new_entry_point {
            return Ok(HandlerOutput::None);
        }
        let (record, source_index) = require_legacy_perk_invocation(self.id(), &invocation)?;
        let range = legacy_perk_effect_range(self.id(), record, source_index)?;
        let entry_conditions =
            configured_u8_array(self.id(), configuration, "entry_point_conditions")?;
        let entry_function_types =
            configured_u8_array(self.id(), configuration, "entry_point_function_types")?;
        if entry_conditions.len() != entry_function_types.len() {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "legacy PERK entry-point metadata lengths differ".to_owned(),
            });
        }
        let old_index = usize::try_from(old_entry_point)
            .ok()
            .filter(|index| *index < entry_conditions.len())
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: format!("legacy PERK entry point {old_entry_point} is out of range"),
            })?;
        let new_index = usize::try_from(new_entry_point)
            .ok()
            .filter(|index| *index < entry_conditions.len())
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: format!("legacy PERK entry point {new_entry_point} is out of range"),
            })?;
        let function_types = configured_u8_array(self.id(), configuration, "function_types")?;
        let current_function = read_configured_integer(
            configuration,
            "function",
            &record.subrecords[source_index].data,
            self.id(),
        )?;
        let required_function_type = entry_function_types[new_index];
        let current_matches = usize::try_from(current_function)
            .ok()
            .and_then(|index| function_types.get(index))
            .is_some_and(|function_type| *function_type == required_function_type);
        let selected_function = if current_matches {
            current_function
        } else {
            function_types
                .iter()
                .position(|function_type| *function_type == required_function_type)
                .and_then(|index| i64::try_from(index).ok())
                .ok_or_else(|| SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: format!(
                        "legacy PERK has no function for type {required_function_type}"
                    ),
                })?
        };
        let condition_slots = configured_u8_matrix(self.id(), configuration, "condition_slots", 3)?;
        let old_condition = usize::from(entry_conditions[old_index]);
        let new_condition = usize::from(entry_conditions[new_index]);
        let old_slots =
            condition_slots
                .get(old_condition)
                .ok_or_else(|| SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: "legacy PERK old condition metadata is out of range".to_owned(),
                })?;
        let new_slots =
            condition_slots
                .get(new_condition)
                .ok_or_else(|| SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: "legacy PERK new condition metadata is out of range".to_owned(),
                })?;
        let new_count = new_slots.iter().take_while(|slot| **slot != 0).count();
        let mut mutations = Vec::new();
        if selected_function != current_function {
            mutations.push(HandlerMutation::Set {
                path: configured_text(self.id(), configuration, "function_path")?.to_owned(),
                occurrence: 0,
                value: OwnedFieldValue::Int(selected_function),
            });
            mutations.extend(legacy_perk_set_parameter_type(
                self.id(),
                configuration,
                record,
                &range,
                current_function,
                selected_function,
            )?);
        }
        mutations.push(HandlerMutation::Set {
            path: configured_text(self.id(), configuration, "condition_count_path")?.to_owned(),
            occurrence: 0,
            value: OwnedFieldValue::Int(i64::try_from(new_count).map_err(|_| {
                SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: "legacy PERK condition count exceeds i64".to_owned(),
                }
            })?),
        });
        let mut condition_indices = Vec::new();
        for subrecord in &record.subrecords[range.clone()] {
            if subrecord.signature == Signature(*b"PRKC") {
                condition_indices.push(read_configured_integer(
                    configuration,
                    "condition_index",
                    &subrecord.data,
                    self.id(),
                )?);
            }
        }
        let condition_item_path = configured_text(self.id(), configuration, "condition_item_path")?;
        for (occurrence, condition_index) in condition_indices.iter().enumerate().rev() {
            let remove = usize::try_from(*condition_index).map_or(true, |index| {
                index >= new_count
                    || (index == 2 && old_slots[1] != new_slots[1])
                    || (index == 3 && old_slots[2] != new_slots[2])
            });
            if remove {
                mutations.push(HandlerMutation::RemoveContainerOccurrence {
                    path: condition_item_path.to_owned(),
                    occurrence,
                });
            }
        }
        Ok(HandlerOutput::Mutations(mutations))
    }
}

struct LegacyPerkFunctionAfterSet;

impl SemanticHandler for LegacyPerkFunctionAfterSet {
    fn id(&self) -> &'static str {
        "edit.legacy_perk_function"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::AfterSet {
            return Ok(HandlerOutput::None);
        }
        let configuration = invocation.context.configuration;
        if configured_text(self.id(), configuration, "binding_path")?
            != invocation.context.binding.path
        {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "legacy PERK function configuration does not match its binding".to_owned(),
            });
        }
        let new_function = invocation
            .value
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "legacy PERK function callback requires a new value".to_owned(),
            })
            .and_then(|value| legacy_perk_callback_i64(value, self.id()))?;
        let old_function = invocation
            .old_value
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "legacy PERK function callback requires the previous value".to_owned(),
            })
            .and_then(|value| legacy_perk_callback_i64(value, self.id()))?;
        let (record, source_index) = require_legacy_perk_invocation(self.id(), &invocation)?;
        let range = legacy_perk_effect_range(self.id(), record, source_index)?;
        let mutations = legacy_perk_set_parameter_type(
            self.id(),
            configuration,
            record,
            &range,
            old_function,
            new_function,
        )?;
        if mutations.is_empty() {
            Ok(HandlerOutput::None)
        } else {
            Ok(HandlerOutput::Mutations(mutations))
        }
    }
}

struct LegacyPerkParameterTypeAfterSet;

impl SemanticHandler for LegacyPerkParameterTypeAfterSet {
    fn id(&self) -> &'static str {
        "edit.legacy_perk_parameter_type"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::AfterSet {
            return Ok(HandlerOutput::None);
        }
        let configuration = invocation.context.configuration;
        if configured_text(self.id(), configuration, "binding_path")?
            != invocation.context.binding.path
        {
            return Err(SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "legacy PERK EPFT configuration does not match its binding".to_owned(),
            });
        }
        let new_parameter_type = invocation
            .value
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "legacy PERK EPFT callback requires a new value".to_owned(),
            })
            .and_then(|value| legacy_perk_callback_i64(value, self.id()))?;
        let old_parameter_type = invocation
            .old_value
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "legacy PERK EPFT callback requires the previous value".to_owned(),
            })
            .and_then(|value| legacy_perk_callback_i64(value, self.id()))?;
        if old_parameter_type == new_parameter_type || !(0..=4).contains(&new_parameter_type) {
            return Ok(HandlerOutput::None);
        }
        let (record, source_index) = require_legacy_perk_invocation(self.id(), &invocation)?;
        let range = legacy_perk_effect_range(self.id(), record, source_index)?;
        Ok(HandlerOutput::Mutations(legacy_perk_parameter_mutations(
            self.id(),
            configuration,
            record,
            &range,
            new_parameter_type,
        )?))
    }
}

struct HeadPartsAfterSet;

impl SemanticHandler for HeadPartsAfterSet {
    fn id(&self) -> &'static str {
        "edit.head_parts"
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
                message: "head-parts callback requires a writable repeat scope".to_owned(),
            })?;
        let configuration = invocation.context.configuration;
        let index_signature = configured_signature(self.id(), configuration, "index_signature")?;
        let ear_value = u32::from(configured_byte(self.id(), configuration, "ear_value")?);
        let Some(index) = record
            .subrecords
            .iter()
            .find(|subrecord| subrecord.signature == index_signature)
        else {
            return Ok(HandlerOutput::None);
        };
        let index_bytes: [u8; 4] = index
            .data
            .get(..4)
            .and_then(|bytes| bytes.try_into().ok())
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "head-parts index payload is shorter than four bytes".to_owned(),
            })?;
        if u32::from_le_bytes(index_bytes) != ear_value {
            return Ok(HandlerOutput::None);
        }

        let model_fields = configured_subrecord_paths(self.id(), configuration, "model_fields")?;
        let icon_signatures =
            configured_signature_list(self.id(), configuration, "icon_signatures")?;
        let model_present = model_fields.iter().any(|(_, signature)| {
            record
                .subrecords
                .iter()
                .any(|subrecord| subrecord.signature == *signature)
        });
        let icon_present = icon_signatures.iter().any(|signature| {
            record
                .subrecords
                .iter()
                .any(|subrecord| subrecord.signature == *signature)
        });
        if !model_present || !icon_present {
            return Ok(HandlerOutput::None);
        }

        let mutations = model_fields
            .into_iter()
            .rev()
            .filter(|(_, signature)| {
                record
                    .subrecords
                    .iter()
                    .any(|subrecord| subrecord.signature == *signature)
            })
            .map(|(path, _)| HandlerMutation::Remove {
                path,
                occurrence: 0,
            })
            .collect();
        Ok(HandlerOutput::Mutations(mutations))
    }
}

fn configured_subrecord_paths(
    handler: &str,
    configuration: &serde_json::Value,
    key: &str,
) -> Result<Vec<(String, Signature)>> {
    let values = configuration
        .get(key)
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| SemanticError::Handler {
            handler: handler.to_owned(),
            message: format!("callback configuration `{key}` must be an array"),
        })?;
    values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let path = value
                .get("path")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| SemanticError::Handler {
                    handler: handler.to_owned(),
                    message: format!("callback configuration `{key}[{index}].path` is missing"),
                })?;
            let signature = value
                .get("signature")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| SemanticError::Handler {
                    handler: handler.to_owned(),
                    message: format!(
                        "callback configuration `{key}[{index}].signature` is missing"
                    ),
                })?;
            Ok((
                path.to_owned(),
                parse_configured_signature(
                    handler,
                    &format!("{key}[{index}].signature"),
                    signature,
                )?,
            ))
        })
        .collect()
}

fn configured_signature_list(
    handler: &str,
    configuration: &serde_json::Value,
    key: &str,
) -> Result<Vec<Signature>> {
    let values = configuration
        .get(key)
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| SemanticError::Handler {
            handler: handler.to_owned(),
            message: format!("callback configuration `{key}` must be an array"),
        })?;
    values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let signature = value.as_str().ok_or_else(|| SemanticError::Handler {
                handler: handler.to_owned(),
                message: format!("callback configuration `{key}[{index}]` must be a string"),
            })?;
            parse_configured_signature(handler, &format!("{key}[{index}]"), signature)
        })
        .collect()
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
                expected: OwnedFieldValue::Int(0),
                value: OwnedFieldValue::Int(0xff),
            },
        ]))
    }
}

struct MagicEffectArchetypeAfterSet;

impl SemanticHandler for MagicEffectArchetypeAfterSet {
    fn id(&self) -> &'static str {
        "edit.magic_effect_archetype"
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
        let new_value = i64::try_from(callback_integer(value, self.id())?).map_err(|_| {
            SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "magic-effect archetype exceeds i64".to_owned(),
            }
        })?;
        let old_value = invocation
            .old_value
            .map(|value| callback_integer(value, self.id()))
            .transpose()?
            .map(i64::try_from)
            .transpose()
            .map_err(|_| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "previous magic-effect archetype exceeds i64".to_owned(),
            })?
            .unwrap_or(new_value);
        if old_value == new_value || old_value >= 0xff || new_value >= 0xff {
            return Ok(HandlerOutput::None);
        }
        let configuration = invocation.context.configuration;
        let assoc_item_path = configured_text(self.id(), configuration, "assoc_item_path")?;
        let actor_value_path = configured_text(self.id(), configuration, "actor_value_path")?;
        let actor_value = magic_effect_actor_value(invocation.context.game, new_value);
        let mut mutations = vec![
            HandlerMutation::Set {
                path: assoc_item_path.to_owned(),
                occurrence: 0,
                value: OwnedFieldValue::Bytes(vec![0; 4]),
            },
            HandlerMutation::Set {
                path: actor_value_path.to_owned(),
                occurrence: 0,
                value: magic_effect_actor_value_owned(
                    invocation.context.game,
                    invocation.context.form_version,
                    actor_value,
                )?,
            },
        ];
        if let Some(second_actor_value_path) =
            configured_optional_text(self.id(), configuration, "second_actor_value_path")?
        {
            mutations.push(HandlerMutation::Set {
                path: second_actor_value_path.to_owned(),
                occurrence: 0,
                value: magic_effect_actor_value_owned(
                    invocation.context.game,
                    invocation.context.form_version,
                    -1,
                )?,
            });
        }
        if let Some(second_av_weight_path) =
            configured_optional_text(self.id(), configuration, "second_av_weight_path")?
        {
            mutations.push(HandlerMutation::Set {
                path: second_av_weight_path.to_owned(),
                occurrence: 0,
                value: OwnedFieldValue::Float(0.0),
            });
        }
        Ok(HandlerOutput::Mutations(mutations))
    }
}

fn magic_effect_actor_value(game: SchemaGame, archetype: i64) -> i64 {
    match game {
        SchemaGame::Fallout3 => match archetype {
            11 => 48,
            12 => 49,
            24 => 47,
            _ => -1,
        },
        SchemaGame::FalloutNv => match archetype {
            11 => 48,
            12 => 49,
            24 => 47,
            36 => 51,
            _ => -1,
        },
        _ => match archetype {
            6 | 8 => 0,
            7 | 24 | 38 | 42 => 1,
            11 => 54,
            21 => 53,
            _ => -1,
        },
    }
}

fn magic_effect_actor_value_owned(
    game: SchemaGame,
    form_version: u16,
    value: i64,
) -> Result<OwnedFieldValue> {
    let raw = value as u32;
    match game {
        SchemaGame::SkyrimLe
        | SchemaGame::SkyrimSe
        | SchemaGame::SkyrimVr
        | SchemaGame::Fallout3
        | SchemaGame::FalloutNv => Ok(OwnedFieldValue::Int(value)),
        SchemaGame::Fallout4 | SchemaGame::Fallout4Vr => Ok(OwnedFieldValue::FormId(FormId(raw))),
        SchemaGame::Fallout76 if form_version < 77 => Ok(OwnedFieldValue::UInt(u64::from(raw))),
        SchemaGame::Fallout76 => Ok(OwnedFieldValue::FormId(FormId(raw))),
        _ => Err(SemanticError::Handler {
            handler: "edit.magic_effect_archetype".to_owned(),
            message: format!("unsupported game {}", game.slug()),
        }),
    }
}

struct ResetSiblingDefault;

impl SemanticHandler for ResetSiblingDefault {
    fn id(&self) -> &'static str {
        "edit.reset_sibling_default"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        if invocation.phase != HandlerPhase::AfterSet {
            return Ok(HandlerOutput::None);
        }
        let path = configured_text(self.id(), invocation.context.configuration, "target_path")?;
        Ok(HandlerOutput::Mutations(vec![
            HandlerMutation::ResetToDefault {
                path: path.to_owned(),
                occurrence: 0,
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
            if invocation.context.game == SchemaGame::Oblivion {
                let value =
                    u64::try_from(parse_delphi_integer(value, self.id())?).map_err(|_| {
                        SemanticError::Handler {
                            handler: self.id().to_owned(),
                            message: "Oblivion CTDA Type must be non-negative".to_owned(),
                        }
                    })?;
                return Ok(HandlerOutput::Value(FieldValue::UInt(value)));
            }
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
            SchemaGame::Fallout3 | SchemaGame::FalloutNv | SchemaGame::Oblivion
        );
        let text = match invocation.phase {
            HandlerPhase::Display | HandlerPhase::Summary => {
                format_ctda_type_display(value, legacy)
            }
            HandlerPhase::SortKey => format!("{value:02X}"),
            HandlerPhase::EditValue if invocation.context.game == SchemaGame::Oblivion => {
                value.to_string()
            }
            HandlerPhase::EditValue => format_ctda_type_edit_value(
                value,
                match invocation.context.game {
                    SchemaGame::Fallout3 => 6,
                    _ => 8,
                },
                legacy,
            ),
            HandlerPhase::NativeValue => value.to_string(),
            HandlerPhase::Validation => validate_ctda_type(value, legacy),
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
                .map_or_else(|| parse_delphi_integer(input, self.id()), Ok)?;
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
            HandlerPhase::Validation => name.map_or_else(
                || match configuration_string(
                    invocation.context.configuration,
                    "unknown_validation",
                    self.id(),
                ) {
                    Ok("angle") => Ok(format!("<Unknown: {value}>")),
                    Ok("none") => Ok(String::new()),
                    Ok(policy) => Err(SemanticError::Handler {
                        handler: self.id().to_owned(),
                        message: format!("unsupported unknown validation policy {policy:?}"),
                    }),
                    Err(error) => Err(error),
                },
                |_| Ok(String::new()),
            )?,
            _ => return Ok(HandlerOutput::None),
        };
        Ok(HandlerOutput::Text(text))
    }
}

struct EventFunctionMemberFormatter;

impl SemanticHandler for EventFunctionMemberFormatter {
    fn id(&self) -> &'static str {
        "format.event_function_member"
    }

    fn version(&self) -> u32 {
        1
    }

    fn invoke(&self, invocation: HandlerInvocation<'_>) -> Result<HandlerOutput> {
        let values = event_function_member_values(invocation.context.configuration, self.id())?;
        if invocation.phase == HandlerPhase::ParseEditValue {
            let FieldValue::String(input) =
                invocation.value.ok_or_else(|| SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: "event function/member parsing requires text".to_owned(),
                })?
            else {
                return Err(SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: "event function/member parsing requires text".to_owned(),
                });
            };
            let Some((function, member)) = input.split_once(':') else {
                return Ok(HandlerOutput::Value(FieldValue::UInt(0)));
            };
            let function = parse_event_component(function, &values.functions, self.id())?;
            let member = parse_event_component(member, &values.members, self.id())?;
            let packed = member
                .checked_shl(16)
                .and_then(|member| member.checked_add(function))
                .ok_or_else(|| SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: "event function/member edit value overflowed i64".to_owned(),
                })?;
            return Ok(HandlerOutput::Value(if packed < 0 {
                FieldValue::Int(packed)
            } else {
                FieldValue::UInt(packed as u64)
            }));
        }

        let raw = callback_integer(
            invocation.value.ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "event function/member formatting requires an integer".to_owned(),
            })?,
            self.id(),
        )?;
        let packed = callback_u32(raw, self.id())?;
        let function = i64::from((packed & 0xFFFF) as u16);
        let member = i64::from((packed >> 16) as u16);
        let function_name = event_component_name(function, &values.functions);
        let member_name = event_component_name(member, &values.members);
        let text = match invocation.phase {
            HandlerPhase::Display | HandlerPhase::Summary | HandlerPhase::EditValue => {
                format!(
                    "{}:{}",
                    function_name.map_or_else(|| function.to_string(), str::to_owned),
                    member_name.map_or_else(|| member.to_string(), str::to_owned)
                )
            }
            HandlerPhase::SortKey => format!("{packed:08X}"),
            HandlerPhase::NativeValue => String::new(),
            HandlerPhase::Validation => {
                let function_error = function_name.map_or_else(
                    || format!("EventFunction<Unknown: {function}>"),
                    |_| String::new(),
                );
                let member_error = member_name.map_or_else(
                    || format!("EventMember<Unknown: {member}>"),
                    |_| String::new(),
                );
                if function_error.is_empty() && member_error.is_empty() {
                    String::new()
                } else {
                    format!("{function_error}:{member_error}")
                }
            }
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

struct SynchronizeContainerCountsAfterSet;

impl SemanticHandler for SynchronizeContainerCountsAfterSet {
    fn id(&self) -> &'static str {
        "edit.sync_container_counts"
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
            message: "container counter synchronization requires a structural value".to_owned(),
        })?;
        let counters = invocation
            .context
            .configuration
            .get("counters")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: "container counter synchronization requires a counters array".to_owned(),
            })?;
        let mut mutations = Vec::with_capacity(counters.len());
        for counter in counters {
            let counter_path = configured_text(self.id(), counter, "counter_path")?;
            let value_path = configured_text(self.id(), counter, "value_path")?;
            if counter_path == value_path {
                return Err(SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: "container counter and value paths must differ".to_owned(),
                });
            }
            let counter_value =
                scoped_named_value(value, counter_path).ok_or_else(|| SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: format!("container counter path {counter_path:?} is absent"),
                })?;
            let array_value =
                scoped_named_value(value, value_path).ok_or_else(|| SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: format!("container value path {value_path:?} is absent"),
                })?;
            let FieldValue::Array(values) = &array_value.value else {
                return Err(SemanticError::Handler {
                    handler: self.id().to_owned(),
                    message: format!("container value path {value_path:?} is not an array"),
                });
            };
            let count = u64::try_from(values.len()).map_err(|_| SemanticError::Handler {
                handler: self.id().to_owned(),
                message: format!("container value path {value_path:?} exceeds u64"),
            })?;
            if callback_integer(&counter_value.value, self.id())? == i128::from(count) {
                continue;
            }
            mutations.push(HandlerMutation::Set {
                path: counter_path.to_owned(),
                occurrence: 0,
                value: OwnedFieldValue::UInt(count),
            });
        }
        Ok(HandlerOutput::Mutations(mutations))
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

struct EventFunctionMemberValues<'a> {
    functions: Vec<(i64, &'a str)>,
    members: Vec<(i64, &'a str)>,
}

fn event_function_member_values<'a>(
    configuration: &'a serde_json::Value,
    handler: &str,
) -> Result<EventFunctionMemberValues<'a>> {
    let values = configuration
        .get("values")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| SemanticError::Handler {
            handler: handler.to_owned(),
            message: "event function/member formatter requires a values array".to_owned(),
        })?;
    let mut functions = Vec::new();
    let mut members = Vec::new();
    for entry in values {
        let value = entry
            .get("value")
            .and_then(serde_json::Value::as_i64)
            .ok_or_else(|| SemanticError::Handler {
                handler: handler.to_owned(),
                message: "event function/member value must be an i64".to_owned(),
            })?;
        if !(i64::from(i32::MIN)..=i64::from(u32::MAX)).contains(&value) {
            return Err(SemanticError::Handler {
                handler: handler.to_owned(),
                message: "event function/member value exceeds 32 bits".to_owned(),
            });
        }
        let name = entry
            .get("name")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| SemanticError::Handler {
                handler: handler.to_owned(),
                message: "event function/member name must be text".to_owned(),
            })?;
        let (function_name, member_name) =
            name.split_once(':').ok_or_else(|| SemanticError::Handler {
                handler: handler.to_owned(),
                message: format!("event function/member name {name:?} has no separator"),
            })?;
        let packed = value as u32;
        insert_event_component(
            &mut functions,
            i64::from((packed & 0xFFFF) as u16),
            function_name,
            handler,
        )?;
        insert_event_component(
            &mut members,
            i64::from((packed >> 16) as u16),
            member_name,
            handler,
        )?;
    }
    if functions.is_empty() || members.is_empty() {
        return Err(SemanticError::Handler {
            handler: handler.to_owned(),
            message: "event function/member values must not be empty".to_owned(),
        });
    }
    Ok(EventFunctionMemberValues { functions, members })
}

fn insert_event_component<'a>(
    values: &mut Vec<(i64, &'a str)>,
    value: i64,
    name: &'a str,
    handler: &str,
) -> Result<()> {
    if name.is_empty() || parse_delphi_integer(name, handler).is_ok() {
        return Ok(());
    }
    if let Some((_, existing_name)) = values.iter().find(|(candidate, _)| *candidate == value) {
        if *existing_name != name {
            return Err(SemanticError::Handler {
                handler: handler.to_owned(),
                message: format!(
                    "event component {value} has conflicting names {existing_name:?} and {name:?}"
                ),
            });
        }
        return Ok(());
    }
    if let Some((existing_value, _)) = values
        .iter()
        .find(|(_, existing_name)| existing_name.eq_ignore_ascii_case(name))
    {
        if *existing_value != value {
            return Err(SemanticError::Handler {
                handler: handler.to_owned(),
                message: format!(
                    "event component {name:?} has conflicting values {existing_value} and {value}"
                ),
            });
        }
        return Ok(());
    }
    values.push((value, name));
    Ok(())
}

fn event_component_name<'a>(value: i64, values: &[(i64, &'a str)]) -> Option<&'a str> {
    values
        .iter()
        .find(|(candidate, _)| *candidate == value)
        .map(|(_, name)| *name)
}

fn parse_event_component(input: &str, values: &[(i64, &str)], handler: &str) -> Result<i64> {
    values
        .iter()
        .find(|(_, name)| name.eq_ignore_ascii_case(input))
        .map(|(value, _)| *value)
        .map_or_else(|| parse_delphi_integer(input, handler), Ok)
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

fn validate_ctda_type(value: u64, legacy: bool) -> String {
    let operator_mask = if legacy { 0xF0 } else { 0xE0 };
    let mut result = match value & operator_mask {
        0x00 | 0x20 | 0x40 | 0x60 | 0x80 | 0xA0 => String::new(),
        _ => "<Unknown Compare operator>".to_owned(),
    };
    if legacy && value & 0x08 != 0 {
        result.push_str(" / <Unknown: 3>");
    }
    result
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

fn callback_u32(value: i128, handler: &str) -> Result<u32> {
    if (0..=i128::from(u32::MAX)).contains(&value) {
        return Ok(value as u32);
    }
    if (i128::from(i32::MIN)..0).contains(&value) {
        return Ok(value as i32 as u32);
    }
    Err(SemanticError::Handler {
        handler: handler.to_owned(),
        message: "callback value exceeds 32 bits".to_owned(),
    })
}

fn parse_delphi_integer(value: &str, handler: &str) -> Result<i64> {
    let value = value.trim();
    let (negative, value) = if let Some(value) = value.strip_prefix('-') {
        (true, value)
    } else {
        (false, value.strip_prefix('+').unwrap_or(value))
    };
    let (radix, digits) = value
        .strip_prefix('$')
        .map_or_else(
            || {
                value
                    .strip_prefix("0x")
                    .or_else(|| value.strip_prefix("0X"))
                    .map(|digits| (16, digits))
            },
            |digits| Some((16, digits)),
        )
        .unwrap_or((10, value));
    let magnitude = u64::from_str_radix(digits, radix).map_err(|error| SemanticError::Handler {
        handler: handler.to_owned(),
        message: format!("invalid integer edit value {value:?}: {error}"),
    })?;
    if negative {
        let maximum = i64::MAX as u64 + 1;
        if magnitude > maximum {
            return Err(SemanticError::Handler {
                handler: handler.to_owned(),
                message: format!("integer edit value -{value} is below i64"),
            });
        }
        if magnitude == maximum {
            Ok(i64::MIN)
        } else {
            Ok(-(magnitude as i64))
        }
    } else {
        i64::try_from(magnitude).map_err(|_| SemanticError::Handler {
            handler: handler.to_owned(),
            message: format!("integer edit value {value} exceeds i64"),
        })
    }
}

fn landscape_position_error(message: impl Into<String>) -> SemanticError {
    SemanticError::Handler {
        handler: "format.landscape_position".to_owned(),
        message: message.into(),
    }
}

fn format_climate_moons(value: i64, game: SchemaGame) -> String {
    let legacy_fallout = matches!(game, SchemaGame::Fallout3 | SchemaGame::FalloutNv);
    let masser_mask = if legacy_fallout { 128 } else { 64 };
    let secunda_mask = if legacy_fallout { 64 } else { 128 };
    let prefix = match (value & masser_mask != 0, value & secunda_mask != 0) {
        (true, true) => "Masser, Secunda",
        (true, false) => "Masser",
        (false, true) => "Secunda",
        (false, false) => "No Moon",
    };
    format!("{prefix} / {}", value % 64)
}

fn climate_moons_error(message: impl Into<String>) -> SemanticError {
    SemanticError::Handler {
        handler: "format.climate_moons".to_owned(),
        message: message.into(),
    }
}

fn format_climate_time(value: i64) -> String {
    if !(0..144).contains(&value) {
        return value.to_string();
    }
    let hours = value / 6;
    let minutes = value % 6 * 10;
    format!("{hours:02}:{minutes:02}:00")
}

fn climate_time_error(message: impl Into<String>) -> SemanticError {
    SemanticError::Handler {
        handler: "format.climate_time".to_owned(),
        message: message.into(),
    }
}

fn format_aloc_time(value: i64) -> String {
    let seconds = value.rem_euclid(256) * 86_400 / 256;
    let hours = seconds / 3_600;
    let minutes = seconds % 3_600 / 60;
    let seconds = seconds % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}")
}

fn aloc_time_error(message: impl Into<String>) -> SemanticError {
    SemanticError::Handler {
        handler: "format.aloc_time".to_owned(),
        message: message.into(),
    }
}

fn parse_integer_handler_value(
    value: Option<&FieldValue<'static>>,
    handler: &str,
    missing_message: &str,
) -> Result<HandlerOutput> {
    let Some(FieldValue::String(value)) = value else {
        return Err(integer_formatter_error(handler, missing_message));
    };
    let value = parse_delphi_integer(value, handler)?;
    Ok(if value < 0 {
        HandlerOutput::Value(FieldValue::Int(value))
    } else {
        HandlerOutput::Value(FieldValue::UInt(value as u64))
    })
}

fn parse_plain_hex_handler_value(
    value: Option<&FieldValue<'static>>,
    handler: &str,
) -> Result<HandlerOutput> {
    let Some(FieldValue::String(value)) = value else {
        return Err(integer_formatter_error(
            handler,
            "plain hexadecimal edit parsing requires text",
        ));
    };
    // xEdit's wbHexStrToInt prefers the first space over a colon and returns zero
    // instead of rejecting malformed user input.
    let digits = value
        .find(' ')
        .or_else(|| value.find(':'))
        .map_or(value.as_ref(), |index| &value[..index]);
    let parsed = u64::from_str_radix(digits, 16)
        .ok()
        .and_then(|value| i64::try_from(value).ok())
        .unwrap_or(0);
    Ok(HandlerOutput::Value(FieldValue::UInt(parsed as u64)))
}

fn idle_animation_group_name(value: i64, legacy_fallout: bool) -> Option<&'static str> {
    if legacy_fallout {
        match value {
            0 => Some("Idle"),
            1 => Some("Movement"),
            2 => Some("Left Arm"),
            3 => Some("Left Hand"),
            4 => Some("Weapon"),
            5 => Some("Weapon Up"),
            6 => Some("Weapon Down"),
            7 => Some("Special Idle"),
            20 => Some("Whole Body"),
            21 => Some("Upper Body"),
            _ => None,
        }
    } else {
        match value {
            0 => Some("Lower Body"),
            1 => Some("Left Arm"),
            2 => Some("Left Hand"),
            3 => Some("Right Arm"),
            4 => Some("Special Idle"),
            5 => Some("Whole Body"),
            6 => Some("Upper Body"),
            _ => None,
        }
    }
}

fn weather_classification_name(value: i64, game: SchemaGame) -> Option<&'static str> {
    match value {
        0 => Some("None"),
        1 => Some("Pleasant"),
        2 => Some("Cloudy"),
        3 if game == SchemaGame::Oblivion => Some("Unknown 3"),
        4 => Some("Rainy"),
        8 => Some("Snow"),
        _ => None,
    }
}

fn integer_formatter_error(handler: &str, message: impl Into<String>) -> SemanticError {
    SemanticError::Handler {
        handler: handler.to_owned(),
        message: message.into(),
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

fn format_rgb(
    value: &FieldValue<'_>,
    include_alpha: bool,
    digits: Option<usize>,
    alpha_digits: Option<usize>,
) -> Result<String> {
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
        .enumerate()
        .map(|(index, component)| {
            let component_digits = if index == 3 {
                alpha_digits.or(digits)
            } else {
                digits
            };
            format_numeric_component("format.rgb", &component.value, component_digits)
        })
        .collect::<Result<_>>()?;
    Ok(format!(
        "{}({})",
        if include_alpha { "RGBA" } else { "RGB" },
        formatted.join(", ")
    ))
}

fn format_vec3(value: &FieldValue<'_>, digits: Option<usize>) -> Result<String> {
    let FieldValue::Struct(components) = value else {
        return Err(SemanticError::Handler {
            handler: "format.vec3".to_owned(),
            message: "Vec3 formatter requires a struct value".to_owned(),
        });
    };
    if components.len() < 3 {
        return Err(SemanticError::Handler {
            handler: "format.vec3".to_owned(),
            message: format!(
                "Vec3 formatter requires three components, got {}",
                components.len()
            ),
        });
    }
    let formatted = components[..3]
        .iter()
        .map(|component| format_numeric_component("format.vec3", &component.value, digits))
        .collect::<Result<Vec<_>>>()?;
    Ok(format!("({})", formatted.join(", ")))
}

fn format_angle_degrees(value: f64) -> String {
    let mut degrees = value.to_degrees();
    while degrees > 360.0 {
        degrees -= 360.0;
    }
    while degrees < -360.0 {
        degrees += 360.0;
    }
    let formatted = if degrees == 0.0 {
        "0".to_owned()
    } else {
        degrees.to_string()
    };
    format!("{formatted}\u{00B0}")
}

fn parse_angle_degrees(value: &str) -> Result<f64> {
    let Some(degrees) = value.strip_suffix('\u{00B0}') else {
        return Err(SemanticError::Handler {
            handler: "format.angle_degrees".to_owned(),
            message: "angle edit value must end with a degree symbol".to_owned(),
        });
    };
    let degrees = degrees.parse::<f64>().map_err(|_| SemanticError::Handler {
        handler: "format.angle_degrees".to_owned(),
        message: "angle edit value must contain a number".to_owned(),
    })?;
    if !(-360.0..=360.0).contains(&degrees) {
        return Err(SemanticError::Handler {
            handler: "format.angle_degrees".to_owned(),
            message: "angle edit value must be between -360 and 360 degrees".to_owned(),
        });
    }
    Ok(degrees.to_radians())
}

fn format_timestamp_date(value: &[u8]) -> String {
    let mut packed = u16::from_le_bytes([value[0], value[1]]);
    if packed == 0 {
        return "None".to_owned();
    }
    let day = packed & 0x1F;
    packed >>= 5;
    let month = packed & 0x0F;
    packed >>= 4;
    let year = 2000 + (packed & 0x7F);
    format!("{year:04}-{month:02}-{day:02}")
}

fn format_hex_bytes(value: &[u8]) -> String {
    value
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn parse_fixed_hex_bytes(value: &str, length: usize) -> Result<Vec<u8>> {
    let digits: String = value
        .chars()
        .filter(|character| !matches!(character, ' ' | ',' | ';'))
        .collect();
    if !digits.len().is_multiple_of(2)
        || !digits
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return Err(timestamp_date_error(
            "timestamp edit value must contain complete hexadecimal byte pairs",
        ));
    }
    let mut bytes = digits
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let text = std::str::from_utf8(pair).map_err(|_| {
                timestamp_date_error("timestamp edit value contains invalid hexadecimal text")
            })?;
            u8::from_str_radix(text, 16).map_err(|_| {
                timestamp_date_error("timestamp edit value contains invalid hexadecimal text")
            })
        })
        .collect::<Result<Vec<_>>>()?;
    bytes.resize(length, 0);
    bytes.truncate(length);
    Ok(bytes)
}

fn timestamp_date_error(message: impl Into<String>) -> SemanticError {
    SemanticError::Handler {
        handler: "format.timestamp_date".to_owned(),
        message: message.into(),
    }
}

fn coordinate_is_latitude(binding: &CallbackBinding) -> Result<bool> {
    if binding.path.ends_with(":Latitude") {
        Ok(true)
    } else if binding.path.ends_with(":Longitude") {
        Ok(false)
    } else {
        Err(coordinate_error(
            "coordinate binding path must end with Latitude or Longitude",
        ))
    }
}

fn format_geographic_coordinate(value: f64, latitude: bool) -> String {
    let full = if latitude { 180.0 } else { 360.0 };
    let half = full / 2.0;
    let mut coordinate = value.to_degrees();
    while coordinate > half {
        coordinate -= full;
    }
    while coordinate < -half {
        coordinate += full;
    }

    let mut degrees = coordinate.trunc() as i32;
    let minutes_fraction = (coordinate - f64::from(degrees)).abs() * 60.0;
    let mut minutes = minutes_fraction.trunc() as i32;
    let mut seconds = ((minutes_fraction - f64::from(minutes)) * 60.0).round_ties_even() as i32;
    if seconds == 60 {
        seconds = 0;
        minutes += 1;
        if minutes == 60 {
            degrees += 1;
            minutes = 0;
        }
    }

    let direction = match (latitude, coordinate >= 0.0) {
        (true, true) => 'N',
        (true, false) => 'S',
        (false, true) => 'E',
        (false, false) => 'W',
    };
    format!("{}\u{00B0}{minutes}'{seconds}\"{direction}", degrees.abs())
}

fn parse_geographic_coordinate(value: &str, latitude: bool) -> Result<f64> {
    if value.chars().count() < 7 {
        return Err(coordinate_error("coordinate edit value is too short"));
    }
    let degree_position = value
        .find('\u{00B0}')
        .ok_or_else(|| coordinate_error("coordinate edit value is missing the degree symbol"))?;
    let minute_position = value
        .find('\'')
        .ok_or_else(|| coordinate_error("coordinate edit value is missing the minute symbol"))?;
    let second_position = value
        .find('"')
        .ok_or_else(|| coordinate_error("coordinate edit value is missing the second symbol"))?;
    if !(degree_position < minute_position && minute_position < second_position) {
        return Err(coordinate_error(
            "coordinate edit value symbols are out of order",
        ));
    }
    let direction = value
        .chars()
        .next_back()
        .ok_or_else(|| coordinate_error("coordinate edit value is empty"))?;
    let (negative_direction, positive_direction) = if latitude { ('S', 'N') } else { ('W', 'E') };
    if direction != negative_direction && direction != positive_direction {
        return Err(coordinate_error(
            "coordinate edit value has an invalid direction",
        ));
    }

    let degrees = parse_coordinate_part(&value[..degree_position], "degrees")?;
    let minutes = parse_coordinate_part(
        &value[degree_position + '\u{00B0}'.len_utf8()..minute_position],
        "minutes",
    )?;
    let seconds = parse_coordinate_part(&value[minute_position + 1..second_position], "seconds")?;
    let half = if latitude { 90 } else { 180 };
    if degrees < 0
        || degrees > half
        || (degrees == half && (minutes > 0 || seconds > 0))
        || !(0..=59).contains(&minutes)
        || !(0..=59).contains(&seconds)
    {
        return Err(coordinate_error(
            "coordinate edit value exceeds its geographic bounds",
        ));
    }
    let mut decimal = f64::from(degrees) + f64::from(minutes) / 60.0 + f64::from(seconds) / 3600.0;
    if direction == negative_direction || decimal == 180.0 {
        decimal = -decimal;
    }
    Ok(decimal.to_radians())
}

fn parse_coordinate_part(value: &str, name: &str) -> Result<i32> {
    value
        .parse::<i32>()
        .map_err(|_| coordinate_error(format!("coordinate edit value contains invalid {name}")))
}

fn coordinate_error(message: impl Into<String>) -> SemanticError {
    SemanticError::Handler {
        handler: "format.geographic_coordinate".to_owned(),
        message: message.into(),
    }
}

fn format_script_summary(value: &FieldValue<'_>) -> Result<String> {
    if !matches!(value, FieldValue::Struct(_)) {
        return Err(SemanticError::Handler {
            handler: "format.script_summary".to_owned(),
            message: "script summary requires a struct value".to_owned(),
        });
    }
    let compiled = find_compiled_script(value);
    let source = find_script_source(value);
    if !compiled {
        return Ok(if source.is_some() {
            "<Source not compiled>".to_owned()
        } else {
            "<Empty>".to_owned()
        });
    }
    let Some(source) = source else {
        return Ok("<Source missing>".to_owned());
    };
    let lines: Vec<&str> = source
        .split(['\r', '\n'])
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with(';'))
        .collect();
    match lines.as_slice() {
        [] => Ok("<Source missing>".to_owned()),
        [line] => Ok((*line).to_owned()),
        _ => Ok(format!("<{} lines>", lines.len())),
    }
}

fn find_compiled_script(value: &FieldValue<'_>) -> bool {
    match value {
        FieldValue::Struct(values) => values.iter().any(|field| {
            matches!(field.value, FieldValue::Bytes(_))
                && field.name.to_ascii_lowercase().contains("compiled")
                || find_compiled_script(&field.value)
        }),
        FieldValue::Array(values) => values.iter().any(find_compiled_script),
        _ => false,
    }
}

fn find_script_source<'a>(value: &'a FieldValue<'a>) -> Option<&'a str> {
    match value {
        FieldValue::Struct(values) => values.iter().find_map(|field| {
            if let FieldValue::String(source) = &field.value {
                if field.name.to_ascii_lowercase().contains("source") {
                    return Some(source.as_ref());
                }
            }
            find_script_source(&field.value)
        }),
        FieldValue::Array(values) => values.iter().find_map(find_script_source),
        _ => None,
    }
}

fn handler_record_context(context: &HandlerContext<'_>) -> HandlerRecordContext {
    HandlerRecordContext::new(
        context.record_signature,
        context.form_id,
        context.form_version,
        context.game,
    )
    .with_plugin_localized(context.plugin_localized)
}

fn format_item_summary(
    value: &FieldValue<'_>,
    resolver: Option<&dyn FormLinkResolver>,
    source: HandlerRecordContext,
) -> Result<Option<String>> {
    let mut fields = struct_fields(value, "format.item_summary")?;
    if let Some(crate::NamedValue {
        value: FieldValue::Struct(nested),
        ..
    }) = fields.first()
    {
        fields = nested;
    }
    let [item, count, ..] = fields else {
        return Err(summary_error(
            "format.item_summary",
            "item summary requires item and count fields",
        ));
    };
    let FieldValue::FormId { value, targets } = &item.value else {
        return Err(summary_error(
            "format.item_summary",
            "item summary requires a FormID as its first field",
        ));
    };
    let count = callback_integer(&count.value, "format.item_summary")?;
    let Some(link) =
        resolver.and_then(|resolver| resolver.resolve_form_id(source, *value, targets))
    else {
        return Ok(None);
    };
    Ok(Some(format!("{count}x {}", link.short_name())))
}

fn format_faction_relation(
    value: &FieldValue<'_>,
    resolver: Option<&dyn FormLinkResolver>,
    source: HandlerRecordContext,
) -> Result<Option<String>> {
    let fields = struct_fields(value, "format.faction_relation")?;
    let [faction, modifier, remaining @ ..] = fields else {
        return Err(summary_error(
            "format.faction_relation",
            "faction relation requires faction and modifier fields",
        ));
    };
    let FieldValue::FormId { value, targets } = &faction.value else {
        return Err(summary_error(
            "format.faction_relation",
            "faction relation requires a FormID as its first field",
        ));
    };
    let Some(link) =
        resolver.and_then(|resolver| resolver.resolve_form_id(source, *value, targets))
    else {
        return Ok(None);
    };
    if source.game == SchemaGame::Oblivion {
        let modifier = callback_integer(&modifier.value, "format.faction_relation")?;
        let prefix = if modifier >= 0 { "+" } else { "" };
        return Ok(Some(format!("{prefix}{modifier} {}", link.value())));
    }
    let reaction = remaining.first().ok_or_else(|| {
        summary_error(
            "format.faction_relation",
            "modern faction relation requires a combat reaction field",
        )
    })?;
    Ok(Some(format!(
        "{} {}",
        summary_scalar(&reaction.value, "format.faction_relation")?,
        link.value()
    )))
}

fn format_object_property(
    value: &FieldValue<'_>,
    resolver: Option<&dyn FormLinkResolver>,
    source: HandlerRecordContext,
) -> Result<Option<String>> {
    format_linked_float_property(value, resolver, source, "format.object_property")
}

fn format_linked_float_property(
    value: &FieldValue<'_>,
    resolver: Option<&dyn FormLinkResolver>,
    source: HandlerRecordContext,
    handler: &str,
) -> Result<Option<String>> {
    let fields = struct_fields(value, handler)?;
    let [linked_value, property_value, remaining @ ..] = fields else {
        return Err(summary_error(
            handler,
            "linked property requires form and value fields",
        ));
    };
    let FieldValue::FormId { value, targets } = &linked_value.value else {
        return Err(summary_error(
            handler,
            "linked property requires a FormID as its first field",
        ));
    };
    let FieldValue::Float(property_value) = property_value.value else {
        return Err(summary_error(
            handler,
            "linked property requires a floating-point second field",
        ));
    };
    let Some(link) =
        resolver.and_then(|resolver| resolver.resolve_form_id(source, *value, targets))
    else {
        return Ok(None);
    };
    let Some(editor_id) = link.editor_id() else {
        return Ok(None);
    };
    let mut text = format!("{editor_id} = {}", format_delphi_general(property_value, 5));
    if matches!(source.game, SchemaGame::Fallout76 | SchemaGame::Starfield) {
        if let Some(crate::NamedValue {
            value: FieldValue::FormId { value, targets },
            ..
        }) = remaining.first()
        {
            if let Some(curve) =
                resolver.and_then(|resolver| resolver.resolve_form_id(source, *value, targets))
            {
                text.push_str(" {Curve Table: ");
                text.push_str(curve.short_name());
                text.push('}');
            }
        }
    }
    Ok(Some(text))
}

fn struct_fields<'a>(
    value: &'a FieldValue<'a>,
    handler: &str,
) -> Result<&'a [crate::NamedValue<'a>]> {
    match value {
        FieldValue::Struct(fields) => Ok(fields),
        _ => Err(summary_error(handler, "summary requires a struct value")),
    }
}

fn summary_scalar(value: &FieldValue<'_>, handler: &str) -> Result<String> {
    match value {
        FieldValue::Int(value) => Ok(value.to_string()),
        FieldValue::UInt(value) => Ok(value.to_string()),
        FieldValue::Enumeration {
            name: Some(name), ..
        } => Ok(name.clone()),
        FieldValue::Enumeration { value, name: None } => Ok(value.to_string()),
        FieldValue::String(value) => Ok(value.to_string()),
        _ => Err(summary_error(
            handler,
            "summary field is not a scalar display value",
        )),
    }
}

fn summary_error(handler: &str, message: impl Into<String>) -> SemanticError {
    SemanticError::Handler {
        handler: handler.to_owned(),
        message: message.into(),
    }
}

fn format_numeric_component(
    handler: &str,
    value: &FieldValue<'_>,
    digits: Option<usize>,
) -> Result<String> {
    match value {
        FieldValue::Int(value) => Ok(value.to_string()),
        FieldValue::UInt(value) => Ok(value.to_string()),
        FieldValue::Float(value) if value.is_nan() => Ok("NaN".to_owned()),
        FieldValue::Float(value) if value.is_infinite() && value.is_sign_positive() => {
            Ok("+Inf".to_owned())
        }
        FieldValue::Float(value) if value.is_infinite() => Ok("-Inf".to_owned()),
        FieldValue::Float(value) => {
            let Some(digits) = digits else {
                return Ok(if *value == 0.0 {
                    "0".to_owned()
                } else {
                    value.to_string()
                });
            };
            let mut formatted = format!("{value:.digits$}");
            if formatted.contains('.') {
                while formatted.ends_with('0') {
                    formatted.pop();
                }
                if formatted.ends_with('.') {
                    formatted.pop();
                }
            }
            if formatted == "-0" {
                formatted = "0".to_owned();
            }
            Ok(formatted)
        }
        _ => Err(SemanticError::Handler {
            handler: handler.to_owned(),
            message: "formatter components must be numeric values".to_owned(),
        }),
    }
}

fn format_delphi_general(value: f64, precision: usize) -> String {
    if value.is_nan() {
        return "NAN".to_owned();
    }
    if value.is_infinite() {
        return if value.is_sign_negative() {
            "-INF".to_owned()
        } else {
            "INF".to_owned()
        };
    }

    let negative = value.is_sign_negative();
    let value = value.abs();
    let fractional_digits = precision.saturating_sub(1);
    let scientific = format!("{value:.fractional_digits$e}");
    let (mantissa, exponent) = scientific
        .split_once('e')
        .expect("Rust scientific float formatting always contains an exponent");
    let exponent: i32 = exponent
        .parse()
        .expect("Rust scientific float formatting always has an integer exponent");
    let mut digits: String = mantissa
        .chars()
        .filter(|character| *character != '.')
        .collect();
    while digits.len() > 1 && digits.ends_with('0') {
        digits.pop();
    }

    let decimal_position = exponent + 1;
    let use_exponent = decimal_position > precision as i32 || decimal_position < -3;
    let mut formatted = if use_exponent {
        let mut formatted = String::new();
        formatted.push(digits.remove(0));
        if !digits.is_empty() {
            formatted.push('.');
            formatted.push_str(&digits);
        }
        formatted.push('E');
        if exponent < 0 {
            formatted.push('-');
        }
        formatted.push_str(&format!("{:03}", exponent.unsigned_abs()));
        formatted
    } else if decimal_position > 0 {
        let decimal_position = decimal_position as usize;
        if digits.len() <= decimal_position {
            digits.push_str(&"0".repeat(decimal_position - digits.len()));
        } else {
            digits.insert(decimal_position, '.');
        }
        digits
    } else {
        format!("0.{}{}", "0".repeat((-decimal_position) as usize), digits)
    };
    if negative {
        formatted.insert(0, '-');
    }
    formatted
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

fn extended_same_value_zero(value: f32) -> bool {
    const EXTENDED_RESOLUTION: f64 = 1.0e-16;
    f64::from(value).abs() <= EXTENDED_RESOLUTION
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use super::*;

    type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;

    struct TestResourceHashResolver;

    impl ResourceHashResolver for TestResourceHashResolver {
        fn resolve_file_hash(&self, hash: u64) -> Option<String> {
            (hash == 0x1234).then(|| "textures/example.dds".to_owned())
        }

        fn resolve_folder_hash(&self, hash: u64) -> Option<String> {
            (hash == 0x5678).then(|| "textures/example".to_owned())
        }
    }

    struct TestNextObjectIdResolver;

    impl NextObjectIdResolver for TestNextObjectIdResolver {
        fn next_object_id(&self, _source: HandlerRecordContext) -> Option<u32> {
            Some(0x1235)
        }
    }

    struct TestFormLinkResolver;

    impl FormLinkResolver for TestFormLinkResolver {
        fn resolve_form_id(
            &self,
            _source: HandlerRecordContext,
            form_id: FormId,
            _targets: &[Signature],
        ) -> Option<FormLinkInfo> {
            match form_id {
                FormId(0x1234) => Some(
                    FormLinkInfo::new("[00001234] Example Faction", "Example Item [MISC:00001234]")
                        .with_signature(Signature(*b"FACT"))
                        .with_editor_id("ExampleActorValue"),
                ),
                FormId(0x2468) => Some(
                    FormLinkInfo::new("[00002468] Example Actor", "Example Actor [NPC_:00002468]")
                        .with_signature(Signature(*b"NPC_"))
                        .with_editor_id("ExampleActor"),
                ),
                FormId(0x4567) => Some(
                    FormLinkInfo::new(
                        "Example Curve [CURV:00004567]",
                        "Example Curve [CURV:00004567]",
                    )
                    .with_signature(Signature(*b"CURV")),
                ),
                FormId(0x6789) => Some(
                    FormLinkInfo::new(
                        "Example Effect [MGEF:00006789]",
                        "Example Effect [MGEF:00006789]",
                    )
                    .with_signature(Signature(*b"MGEF"))
                    .with_magic_effect_actor_value(48),
                ),
                FormId(0x7777) => Some(
                    FormLinkInfo::new(
                        "Example Weapon [WEAP:00007777]",
                        "Example Weapon [WEAP:00007777]",
                    )
                    .with_signature(Signature(*b"WEAP")),
                ),
                FormId(0x3456) => Some(
                    FormLinkInfo::new(
                        "Example Terminal [TERM:00003456]",
                        "Example Terminal [TERM:00003456]",
                    )
                    .with_signature(Signature(*b"TERM"))
                    .with_script_variables(ScriptVariableMetadata::resolved(
                        "ExampleScript [SCPT:00007890]",
                        vec![
                            ScriptVariableInfo::new(2, "FirstVariable"),
                            ScriptVariableInfo::new(7, "TargetVariable"),
                        ],
                    )),
                ),
                FormId(0x5678) => Some(
                    FormLinkInfo::new(
                        "Example Quest [QUST:00005678]",
                        "Example Quest [QUST:00005678]",
                    )
                    .with_signature(Signature(*b"QUST"))
                    .with_quest_aliases(vec![
                        QuestAliasInfo::new(7, "Target"),
                        QuestAliasInfo::new(12, ""),
                    ])
                    .with_quest_stages(vec![
                        QuestStageInfo::new(10, " First objective "),
                        QuestStageInfo::new(20, ""),
                    ])
                    .with_quest_objectives(vec![
                        QuestObjectiveInfo::new(10, " Reach the target "),
                        QuestObjectiveInfo::new(20, ""),
                    ]),
                ),
                _ => None,
            }
        }

        fn resolve_magic_effect_code(
            &self,
            _source: HandlerRecordContext,
            code: u32,
        ) -> Option<FormLinkInfo> {
            (code == u32::from_le_bytes(*b"ABCD")).then(|| {
                FormLinkInfo::new(
                    "Example Effect [MGEF:00006789]",
                    "Example Effect [MGEF:00006789]",
                )
                .with_signature(Signature(*b"MGEF"))
                .with_magic_effect_metadata(0x0100_0000, 42)
            })
        }

        fn source_master_morph_keys(&self, _source: HandlerRecordContext) -> Option<Vec<u32>> {
            Some(vec![20, 10])
        }

        fn source_file_name(&self, _source: HandlerRecordContext) -> Option<String> {
            Some("Oblivion.esm".to_owned())
        }

        fn source_parent_group_type(&self, _source: HandlerRecordContext) -> Option<u32> {
            Some(1)
        }

        fn resolve_record_index(
            &self,
            _source: HandlerRecordContext,
            index: &str,
            key: &RecordIndexKeyValue,
        ) -> Option<IndexedRecordInfo> {
            match (index, key) {
                ("simple_group", RecordIndexKeyValue::Text(key)) if key == "ExampleColor" => {
                    Some(IndexedRecordInfo::new(
                        FormId(0x1234),
                        FormLinkInfo::new(
                            "Example Color [AVMD:00001234]",
                            "Example Color [AVMD:00001234]",
                        ),
                    ))
                }
                ("collision_layer", RecordIndexKeyValue::Integer(7)) => {
                    Some(IndexedRecordInfo::new(
                        FormId(0x2468),
                        FormLinkInfo::new(
                            "Example Collision [COLL:00002468]",
                            "Example Collision [COLL:00002468]",
                        ),
                    ))
                }
                ("complex_group", RecordIndexKeyValue::Text(key)) if key == "EntryName" => {
                    Some(IndexedRecordInfo::new(
                        FormId(0x3456),
                        FormLinkInfo::new(
                            "Complex Entry [AVMD:00003456]",
                            "Complex Entry [AVMD:00003456]",
                        ),
                    ))
                }
                ("modulation", RecordIndexKeyValue::Text(key)) if key == "EntryValue" => {
                    Some(IndexedRecordInfo::new(
                        FormId(0x4567),
                        FormLinkInfo::new(
                            "Modulation Entry [AVMD:00004567]",
                            "Modulation Entry [AVMD:00004567]",
                        ),
                    ))
                }
                _ => None,
            }
        }

        fn resolve_snap_node(
            &self,
            _source: HandlerRecordContext,
            reference_form_id: Option<FormId>,
            node_id: i64,
        ) -> Option<ResolvedElementInfo> {
            (node_id == 7 && matches!(reference_form_id, None | Some(FormId(0x2468)))).then(|| {
                ResolvedElementInfo::new(
                    FormId(0x5678),
                    "STMP/2:Nodes/payload/element",
                    vec![3],
                    "[7] Example Node",
                    "Example Template [STMP:00005678]",
                )
            })
        }

        fn source_load_order_form_id(&self, _source: HandlerRecordContext) -> Option<u32> {
            Some(0x0100_1234)
        }

        fn resolve_navmesh(
            &self,
            _source: HandlerRecordContext,
            form_id: FormId,
        ) -> Option<ResolvedNavmeshInfo> {
            (form_id == FormId(0x2468)).then(|| {
                ResolvedNavmeshInfo::new(
                    FormId(0x2468),
                    0x0200_2468,
                    "Target Navmesh [NAVM:02002468]",
                    "NAVM/0:Navigation Mesh/payload/3:Triangles",
                    4,
                )
            })
        }

        fn resolve_npc_face_entry(
            &self,
            _source: HandlerRecordContext,
            kind: NpcFaceEntryKind,
            index: i64,
        ) -> Option<ResolvedElementInfo> {
            (index == 7).then(|| {
                let (path, summary) = match kind {
                    NpcFaceEntryKind::FaceDial => (
                        "RACE/Chargen and Skintones/Male/Chargen/Face Dials/element",
                        "007 Jaw Width",
                    ),
                    NpcFaceEntryKind::FaceMorphPhenotype => (
                        "RACE/Chargen and Skintones/Male/Chargen/Face Morph Phenotypes/element",
                        "007 Athletic",
                    ),
                };
                ResolvedElementInfo::new(
                    FormId(0x2468),
                    path,
                    vec![3],
                    summary,
                    "Example Race [RACE:00002468]",
                )
            })
        }

        fn resolve_condition_quest_form_id(
            &self,
            _source: HandlerRecordContext,
            _record: &Record,
        ) -> Option<FormId> {
            Some(FormId(0x5678))
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
            effective_path: None,
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
            component("Alpha", FieldValue::Float(0.123_456_7)),
        ]);

        assert_eq!(format_rgb(&rgb, false, None, None)?, "RGB(12, 34, 56)");
        assert_eq!(
            format_rgb(&rgba, true, Some(0), None)?,
            "RGBA(12, 34, 56, 0)"
        );
        assert_eq!(
            format_rgb(&rgba, true, Some(0), Some(6))?,
            "RGBA(12, 34, 56, 0.123457)"
        );
        let binding = test_metadata_binding(
            "def.value_transform",
            "format.rgb",
            serde_json::json!({"include_alpha": false}),
        );
        let display = FormatRgb.invoke(HandlerInvocation {
            context: HandlerContext {
                binding: &binding,
                record_signature: Signature(*b"TEST"),
                form_id: FormId::NULL,
                form_version: 0,
                game: SchemaGame::SkyrimSe,
                plugin_localized: false,
                configuration: match &binding.implementation {
                    CallbackImplementation::BuiltIn { operation } => &operation.configuration,
                    _ => unreachable!("test binding is built-in"),
                },
            },
            phase: HandlerPhase::Display,
            value: Some(&rgb),
            old_value: None,
            value_scope: None,
            source_record: None,
            source_writable_record: None,
            source_subrecord_index: None,
            array_indices: &[],
        })?;
        assert!(matches!(display, HandlerOutput::None));
        Ok(())
    }

    /// Matches xEdit's fixed precision and special-value summaries for Vec3 fields.
    #[test]
    fn vec3_formatter_matches_xedit_component_summaries() -> Result<()> {
        let component = |name: &str, value: FieldValue<'static>| crate::NamedValue {
            node_id: bethkit_schema::SchemaNodeId(1),
            path: format!("TEST/{name}"),
            effective_path: None,
            name: name.to_owned(),
            span: crate::ByteSpan { start: 0, end: 4 },
            value,
        };
        let vector = FieldValue::Struct(vec![
            component("X", FieldValue::Float(1.234_567_8)),
            component("Y", FieldValue::Float(-0.0)),
            component("Z", FieldValue::Float(f64::INFINITY)),
        ]);

        assert_eq!(format_vec3(&vector, Some(6))?, "(1.234568, 0, +Inf)");
        Ok(())
    }

    /// Matches xEdit's Starfield angle display, wrapping, and editable-value conversion.
    #[test]
    fn angle_formatter_matches_xedit_degrees() -> Result<()> {
        assert_eq!(format_angle_degrees(std::f64::consts::PI), "180\u{00B0}");
        assert_eq!(
            format_angle_degrees(std::f64::consts::TAU * 2.5),
            "180\u{00B0}"
        );
        assert_eq!(
            format_angle_degrees(-std::f64::consts::TAU * 2.5),
            "-180\u{00B0}"
        );
        assert!(
            (parse_angle_degrees("-90\u{00B0}")? + std::f64::consts::FRAC_PI_2).abs()
                < f64::EPSILON
        );
        assert!(parse_angle_degrees("361\u{00B0}").is_err());
        assert!(parse_angle_degrees("90").is_err());
        Ok(())
    }

    /// Matches xEdit's packed date display and fixed byte-array edit behavior.
    #[test]
    fn timestamp_formatter_matches_xedit_date() -> Result<()> {
        let packed = (24_u16 << 9) | (7_u16 << 5) | 27_u16;
        let bytes = packed.to_le_bytes();

        assert_eq!(format_timestamp_date(&bytes), "2024-07-27");
        assert_eq!(format_timestamp_date(&[0, 0]), "None");
        assert_eq!(format_hex_bytes(&bytes), "FB 30");
        assert_eq!(parse_fixed_hex_bytes("FB, 30", 2)?, bytes);
        assert_eq!(parse_fixed_hex_bytes("01", 2)?, [1, 0]);
        assert_eq!(parse_fixed_hex_bytes("01 02 03", 2)?, [1, 2]);
        assert!(parse_fixed_hex_bytes("1", 2).is_err());
        Ok(())
    }

    /// Matches xEdit's Starfield latitude and longitude DMS conversion.
    #[test]
    fn geographic_formatter_matches_xedit_coordinates() -> Result<()> {
        assert_eq!(
            format_geographic_coordinate(51.5_f64.to_radians(), true),
            "51\u{00B0}30'0\"N"
        );
        assert_eq!(
            format_geographic_coordinate((-122.25_f64).to_radians(), false),
            "122\u{00B0}15'0\"W"
        );
        assert_eq!(
            format_geographic_coordinate(270.0_f64.to_radians(), false),
            "90\u{00B0}0'0\"W"
        );
        assert!(
            (parse_geographic_coordinate("51\u{00B0}30'0\"N", true)? - 51.5_f64.to_radians()).abs()
                < f64::EPSILON
        );
        assert_eq!(
            parse_geographic_coordinate("180\u{00B0}0'0\"E", false)?,
            (-180.0_f64).to_radians()
        );
        assert!(parse_geographic_coordinate("91\u{00B0}0'0\"N", true).is_err());
        Ok(())
    }

    /// Matches xEdit's source/compiled-state and meaningful-line script summaries.
    #[test]
    fn script_summary_matches_xedit_states() -> Result<()> {
        let field = |name: &str, value: FieldValue<'static>| crate::NamedValue {
            node_id: bethkit_schema::SchemaNodeId(1),
            path: format!("TEST/{name}"),
            effective_path: None,
            name: name.to_owned(),
            span: crate::ByteSpan { start: 0, end: 0 },
            value,
        };
        let source_only = FieldValue::Struct(vec![field(
            "Script Source",
            FieldValue::String(std::borrow::Cow::Borrowed("set x to 1")),
        )]);
        let compiled_only = FieldValue::Struct(vec![field(
            "Compiled Script",
            FieldValue::Bytes(std::borrow::Cow::Borrowed(&[])),
        )]);
        let complete = FieldValue::Struct(vec![
            field(
                "Compiled Script",
                FieldValue::Bytes(std::borrow::Cow::Borrowed(&[1])),
            ),
            field(
                "Script Source",
                FieldValue::String(std::borrow::Cow::Borrowed(
                    "; comment\r\n set x to 1 \r\n\r\nset y to 2",
                )),
            ),
        ]);

        assert_eq!(
            format_script_summary(&FieldValue::Struct(vec![]))?,
            "<Empty>"
        );
        assert_eq!(
            format_script_summary(&source_only)?,
            "<Source not compiled>"
        );
        assert_eq!(format_script_summary(&compiled_only)?, "<Source missing>");
        assert_eq!(format_script_summary(&complete)?, "<2 lines>");
        Ok(())
    }

    /// Matches xEdit's count and short-name item summary.
    #[test]
    fn item_summary_uses_resolved_short_name() -> TestResult {
        let field = |name: &str, value: FieldValue<'static>| crate::NamedValue {
            node_id: bethkit_schema::SchemaNodeId(1),
            path: format!("TEST/{name}"),
            effective_path: None,
            name: name.to_owned(),
            span: crate::ByteSpan { start: 0, end: 0 },
            value,
        };
        let value = FieldValue::Struct(vec![field(
            "CNTO",
            FieldValue::Struct(vec![
                field(
                    "Item",
                    FieldValue::FormId {
                        value: FormId(0x1234),
                        targets: vec![Signature(*b"MISC")],
                    },
                ),
                field("Count", FieldValue::Int(3)),
            ]),
        )]);
        let source =
            HandlerRecordContext::new(Signature(*b"CONT"), FormId::NULL, 0, SchemaGame::SkyrimSe);

        assert_eq!(
            format_item_summary(&value, Some(&TestFormLinkResolver), source)?,
            Some("3x Example Item [MISC:00001234]".to_owned())
        );
        Ok(())
    }

    /// Resolves and summarizes Starfield blueprint component part identifiers.
    #[test]
    fn blueprint_component_handlers_match_xedit() -> TestResult {
        // given
        let field = |name: &str, value: FieldValue<'static>| crate::NamedValue {
            node_id: bethkit_schema::SchemaNodeId(1),
            path: format!("TEST/Blue Print Components/element/{name}"),
            effective_path: None,
            name: name.to_owned(),
            span: crate::ByteSpan { start: 0, end: 0 },
            value,
        };
        let vector = |name: &str, values: [f64; 3]| {
            field(
                name,
                FieldValue::Struct(
                    ["X", "Y", "Z"]
                        .into_iter()
                        .zip(values)
                        .map(|(name, value)| field(name, FieldValue::Float(value)))
                        .collect(),
                ),
            )
        };
        let item = FieldValue::Struct(vec![
            field(
                "Base Item",
                FieldValue::FormId {
                    value: FormId(0x1234),
                    targets: vec![Signature(*b"GBFM")],
                },
            ),
            field(
                "Construction Object",
                FieldValue::FormId {
                    value: FormId::NULL,
                    targets: vec![Signature(*b"COBJ")],
                },
            ),
            field(
                "Position/Rotation",
                FieldValue::Struct(vec![
                    vector("Position", [1.0, 2.0, 3.0]),
                    vector("Rotation", [90.0, 0.0, -45.0]),
                ]),
            ),
            field("Part ID", FieldValue::UInt(7)),
        ]);
        let scope = FieldValue::Array(vec![item]);
        let found =
            find_blueprint_component(&scope, 7).ok_or("expected blueprint component resolution")?;
        let source =
            HandlerRecordContext::new(Signature(*b"TEST"), FormId::NULL, 0, SchemaGame::Starfield);

        // when
        let summary =
            format_blueprint_component(found.fields, Some(&TestFormLinkResolver), source)?;

        // then
        assert_eq!(
            summary,
            "[7] [00001234] Example Faction Pos:(1, 2, 3) Rot:(90, 0, -45)"
        );
        assert_eq!(found.array_indices, vec![0]);
        assert!(find_blueprint_component(&scope, -1).is_none());
        assert!(find_blueprint_component(&scope, 8).is_none());
        Ok(())
    }

    /// Resolves Starfield strings and integers through xEdit named record indexes.
    #[test]
    fn indexed_record_handlers_match_xedit() -> TestResult {
        // given
        let format_binding = test_metadata_binding(
            "def.value_transform",
            "format.indexed_record_name",
            serde_json::json!({ "index": "simple_group" }),
        );
        let link_binding = test_metadata_binding(
            "value.links_to",
            "resolve.indexed_record",
            serde_json::json!({ "index": "collision_layer" }),
        );
        let mut handlers = SemanticHandlerRegistry::builtin();
        handlers.set_form_link_resolver(Arc::new(TestFormLinkResolver));
        let source =
            HandlerRecordContext::new(Signature(*b"TEST"), FormId::NULL, 0, SchemaGame::Starfield);
        let color = FieldValue::String(Cow::Borrowed("ExampleColor"));
        let collision = FieldValue::UInt(7);

        // when / then
        assert!(matches!(
            handlers.invoke(
                &format_binding,
                source,
                HandlerPhase::Display,
                Some(&color),
                None,
            )?,
            HandlerOutput::Text(value) if value == "Example Color [AVMD:00001234]"
        ));
        assert!(matches!(
            handlers.invoke(
                &link_binding,
                source,
                HandlerPhase::ReferenceResolution,
                Some(&collision),
                None,
            )?,
            HandlerOutput::Link(SemanticLink::Record {
                form_id: FormId(0x2468),
            })
        ));
        assert!(matches!(
            handlers.invoke(
                &format_binding,
                source,
                HandlerPhase::Summary,
                Some(&color),
                None,
            )?,
            HandlerOutput::None
        ));
        Ok(())
    }

    /// Resolves AVMD entry names and prefixed values with their local entry context.
    #[test]
    fn avmd_entry_reference_handlers_match_xedit() -> TestResult {
        // given
        let type_path = "AVMD/2:Type";
        let value_path = "AVMD/6:Entries/repeat/0:Entry/1:Value";
        let field = |path: &str, name: &str, value: FieldValue<'static>| crate::NamedValue {
            node_id: bethkit_schema::SchemaNodeId(1),
            path: path.to_owned(),
            effective_path: None,
            name: name.to_owned(),
            span: crate::ByteSpan { start: 0, end: 0 },
            value,
        };
        let active = field(
            "",
            "Bethkit Active Repeat Occurrence",
            FieldValue::Struct(vec![field(
                "AVMD/6:Entries/repeat/0:Entry/0:Name",
                "Name",
                FieldValue::String(Cow::Borrowed("EntryName")),
            )]),
        );
        let scope = FieldValue::Struct(vec![
            active,
            field(
                type_path,
                "Type",
                FieldValue::Enumeration {
                    value: 2,
                    name: Some("Complex Group".to_owned()),
                },
            ),
        ]);
        let name_binding = test_metadata_binding(
            "def.value_transform",
            "format.avmd_entry_reference",
            serde_json::json!({
                "mode": "name",
                "type_path": type_path,
                "value_path": value_path
            }),
        );
        let value_binding = test_metadata_binding(
            "value.links_to",
            "resolve.avmd_entry_reference",
            serde_json::json!({
                "mode": "value",
                "type_path": type_path,
                "value_path": value_path
            }),
        );
        let mut handlers = SemanticHandlerRegistry::builtin();
        handlers.set_form_link_resolver(Arc::new(TestFormLinkResolver));
        let source =
            HandlerRecordContext::new(Signature(*b"AVMD"), FormId::NULL, 0, SchemaGame::Starfield);
        let name = FieldValue::String(Cow::Borrowed("EntryName"));
        let value = FieldValue::String(Cow::Borrowed("Modulation_EntryValue"));

        // when / then
        assert!(matches!(
            handlers.invoke_with_value_scope(
                &name_binding,
                source,
                HandlerPhase::Display,
                Some(&name),
                None,
                Some(&scope),
            )?,
            HandlerOutput::Text(text) if text == "Complex Entry [AVMD:00003456]"
        ));
        assert!(matches!(
            handlers.invoke_with_value_scope(
                &value_binding,
                source,
                HandlerPhase::ReferenceResolution,
                Some(&value),
                None,
                Some(&scope),
            )?,
            HandlerOutput::Link(SemanticLink::Record {
                form_id: FormId(0x4567),
            })
        ));
        Ok(())
    }

    /// Resolves Starfield snap node IDs and formats their linked element summaries.
    #[test]
    fn snap_node_handlers_match_xedit() -> TestResult {
        // given
        let path = "REFR/25:Snap Links/payload/element/1:Links/element/1:Linked Node";
        let reference_path = "REFR/25:Snap Links/payload/element/0:Linked Reference";
        let mut format_binding = test_metadata_binding(
            "def.value_transform",
            "format.snap_node_summary",
            serde_json::json!({}),
        );
        format_binding.path = path.to_owned();
        let scope = FieldValue::Struct(vec![
            crate::NamedValue {
                node_id: bethkit_schema::SchemaNodeId(u32::MAX),
                path: String::new(),
                effective_path: None,
                name: "Bethkit Active Repeat Occurrence".to_owned(),
                span: crate::ByteSpan { start: 0, end: 0 },
                value: FieldValue::Struct(vec![
                    crate::NamedValue {
                        node_id: bethkit_schema::SchemaNodeId(1),
                        path: reference_path.to_owned(),
                        effective_path: None,
                        name: "Linked Reference".to_owned(),
                        span: crate::ByteSpan { start: 0, end: 4 },
                        value: FieldValue::FormId {
                            value: FormId(0x2468),
                            targets: vec![Signature(*b"REFR")],
                        },
                    },
                    snap_reference_value(
                        2,
                        "CELL/17:Ship Blueprint Snap Links/payload/element/0:Parent Reference",
                    ),
                    snap_reference_value(
                        3,
                        "CELL/17:Ship Blueprint Snap Links/payload/element/1:Linked Reference",
                    ),
                ]),
            },
            crate::NamedValue {
                node_id: bethkit_schema::SchemaNodeId(u32::MAX),
                path: String::new(),
                effective_path: None,
                name: "Bethkit Structural Record Scope".to_owned(),
                span: crate::ByteSpan { start: 0, end: 0 },
                value: FieldValue::Struct(vec![crate::NamedValue {
                    node_id: bethkit_schema::SchemaNodeId(2),
                    path: reference_path.to_owned(),
                    effective_path: None,
                    name: "Linked Reference".to_owned(),
                    span: crate::ByteSpan { start: 4, end: 8 },
                    value: FieldValue::FormId {
                        value: FormId(0x9999),
                        targets: vec![Signature(*b"REFR")],
                    },
                }]),
            },
        ]);
        let mut handlers = SemanticHandlerRegistry::builtin();
        handlers.set_form_link_resolver(Arc::new(TestFormLinkResolver));
        let source = HandlerRecordContext::new(
            Signature(*b"REFR"),
            FormId(0x1111),
            0,
            SchemaGame::Starfield,
        );
        let node = FieldValue::UInt(7);

        // when / then
        assert!(matches!(
            handlers.invoke_with_value_scope(
                &format_binding,
                source,
                HandlerPhase::Display,
                Some(&node),
                None,
                Some(&scope),
            )?,
            HandlerOutput::Text(text)
                if text
                    == "[7] Example Node on Example Template [STMP:00005678]"
        ));
        for (path, needs_scope) in [
            (
                "CELL/17:Ship Blueprint Snap Links/payload/element/2:Parent Node",
                true,
            ),
            (
                "CELL/17:Ship Blueprint Snap Links/payload/element/3:Linked Node",
                true,
            ),
            (
                "REFR/25:Snap Links/payload/element/1:Links/element/0:Parent Node",
                false,
            ),
            (
                "REFR/25:Snap Links/payload/element/1:Links/element/1:Linked Node",
                true,
            ),
        ] {
            let mut link_binding =
                test_metadata_binding("value.links_to", "resolve.snap_node", serde_json::json!({}));
            link_binding.path = path.to_owned();
            assert!(matches!(
                handlers.invoke_with_value_scope(
                    &link_binding,
                    source,
                    HandlerPhase::ReferenceResolution,
                    Some(&node),
                    None,
                    needs_scope.then_some(&scope),
                )?,
                HandlerOutput::Link(SemanticLink::ExternalElement {
                    record_form_id: FormId(0x5678),
                    path,
                    array_indices,
                }) if path == "STMP/2:Nodes/payload/element" && array_indices == vec![3]
            ));
        }
        Ok(())
    }

    /// Matches local and external xEdit navigation-mesh edge semantics.
    #[test]
    fn navmesh_edge_handlers_match_xedit() -> TestResult {
        // given
        let triangles_path = "NAVM/0:Navigation Mesh/payload/3:Triangles";
        let edge_path = "NAVM/0:Navigation Mesh/payload/3:Triangles/element/3:Edge 0-1";
        let edge_links_path = "NAVM/0:Navigation Mesh/payload/4:Edge Links";
        let field = |path: &str, name: &str, value: FieldValue<'static>| crate::NamedValue {
            node_id: bethkit_schema::SchemaNodeId(1),
            path: path.to_owned(),
            effective_path: None,
            name: name.to_owned(),
            span: crate::ByteSpan { start: 0, end: 0 },
            value,
        };
        let triangle = |flags: u64| {
            FieldValue::Struct(vec![field(
                &format!("{triangles_path}/element/0:Flags"),
                "Flags",
                FieldValue::Flags {
                    value: flags,
                    active: Vec::new(),
                },
            )])
        };
        let scope = FieldValue::Struct(vec![
            field(
                triangles_path,
                "Triangles",
                FieldValue::Array(vec![triangle(0), triangle(1)]),
            ),
            field(
                edge_links_path,
                "Edge Links",
                FieldValue::Array(vec![FieldValue::Struct(vec![
                    field(
                        &format!("{edge_links_path}/element/1:Mesh"),
                        "Mesh",
                        FieldValue::FormId {
                            value: FormId(0x2468),
                            targets: vec![Signature(*b"NAVM")],
                        },
                    ),
                    field(
                        &format!("{edge_links_path}/element/2:Triangle Index"),
                        "Triangle Index",
                        FieldValue::UInt(2),
                    ),
                ])]),
            ),
        ]);
        let mut format_binding = test_metadata_binding(
            "integer.formatter",
            "format.navmesh_edge",
            serde_json::json!({}),
        );
        format_binding.path = edge_path.to_owned();
        let mut link_binding = test_metadata_binding(
            "value.links_to",
            "resolve.navmesh_edge",
            serde_json::json!({}),
        );
        link_binding.path = edge_path.to_owned();
        let mut handlers = SemanticHandlerRegistry::builtin();
        handlers.set_form_link_resolver(Arc::new(TestFormLinkResolver));
        let source =
            HandlerRecordContext::new(Signature(*b"NAVM"), FormId(0x1234), 0, SchemaGame::Fallout4);
        let edge = FieldValue::UInt(0);
        let local_index = [0];
        let external_index = [1];

        // when / then
        for (indices, phase, expected) in [
            (local_index.as_slice(), HandlerPhase::Display, "0"),
            (
                local_index.as_slice(),
                HandlerPhase::SortKey,
                "010012340000",
            ),
            (
                external_index.as_slice(),
                HandlerPhase::Display,
                "0 (#2 in Target Navmesh [NAVM:02002468])",
            ),
            (
                external_index.as_slice(),
                HandlerPhase::SortKey,
                "020024680002",
            ),
        ] {
            assert!(matches!(
                handlers.invoke_with_records(
                    &format_binding,
                    source,
                    HandlerInvocationAccess {
                        source: HandlerRecordSource::None,
                        value_scope: Some(&scope),
                        source_subrecord_index: None,
                        array_indices: indices,
                    },
                    phase,
                    Some(&edge),
                    None,
                )?,
                HandlerOutput::Text(text) if text == expected
            ));
        }
        assert!(matches!(
            handlers.invoke_with_records(
                &link_binding,
                source,
                HandlerInvocationAccess {
                    source: HandlerRecordSource::None,
                    value_scope: Some(&scope),
                    source_subrecord_index: None,
                    array_indices: &local_index,
                },
                HandlerPhase::ReferenceResolution,
                Some(&edge),
                None,
            )?,
            HandlerOutput::Link(SemanticLink::Element {
                path,
                array_indices,
            }) if path == format!("{triangles_path}/element") && array_indices == vec![0]
        ));
        assert!(matches!(
            handlers.invoke_with_records(
                &link_binding,
                source,
                HandlerInvocationAccess {
                    source: HandlerRecordSource::None,
                    value_scope: Some(&scope),
                    source_subrecord_index: None,
                    array_indices: &external_index,
                },
                HandlerPhase::ReferenceResolution,
                Some(&edge),
                None,
            )?,
            HandlerOutput::Link(SemanticLink::ExternalElement {
                record_form_id: FormId(0x2468),
                path,
                array_indices,
            }) if path == "NAVM/0:Navigation Mesh/payload/3:Triangles/element"
                && array_indices == vec![2]
        ));
        for (input, expected) in [("", -1), ("None", -1), ("garbage", 0), ("17", 17)] {
            let input = FieldValue::String(Cow::Borrowed(input));
            assert!(matches!(
                handlers.invoke(
                    &format_binding,
                    source,
                    HandlerPhase::ParseEditValue,
                    Some(&input),
                    None,
                )?,
                HandlerOutput::Value(FieldValue::Int(value)) if value == expected
            ));
        }
        let none = FieldValue::Int(-1);
        assert!(matches!(
            handlers.invoke(
                &format_binding,
                source,
                HandlerPhase::EditValue,
                Some(&none),
                None,
            )?,
            HandlerOutput::Text(text) if text.is_empty()
        ));
        Ok(())
    }

    /// Matches xEdit navigation-mesh vertex display, sorting, and edit parsing.
    #[test]
    fn navmesh_vertex_formatter_matches_xedit() -> TestResult {
        // given
        let vertices_path = "NAVM/0:Navigation Mesh/payload/2:Vertices";
        let vertex_path = "NAVM/0:Navigation Mesh/payload/3:Triangles/element/0:Vertex 0";
        let field = |path: &str, name: &str, value: FieldValue<'static>| crate::NamedValue {
            node_id: bethkit_schema::SchemaNodeId(1),
            path: path.to_owned(),
            effective_path: None,
            name: name.to_owned(),
            span: crate::ByteSpan { start: 0, end: 0 },
            value,
        };
        let scope = FieldValue::Struct(vec![field(
            vertices_path,
            "Vertices",
            FieldValue::Array(vec![FieldValue::Struct(vec![
                field(
                    &format!("{vertices_path}/element/0:X"),
                    "X",
                    FieldValue::Float(1.25),
                ),
                field(
                    &format!("{vertices_path}/element/1:Y"),
                    "Y",
                    FieldValue::Float(-2.5),
                ),
                field(
                    &format!("{vertices_path}/element/2:Z"),
                    "Z",
                    FieldValue::Float(0.0),
                ),
            ])]),
        )]);
        let mut binding = test_metadata_binding(
            "integer.formatter",
            "format.navmesh_vertex",
            serde_json::json!({}),
        );
        binding.path = vertex_path.to_owned();
        let handlers = SemanticHandlerRegistry::builtin();
        let skyrim =
            HandlerRecordContext::new(Signature(*b"NAVM"), FormId(0x1234), 0, SchemaGame::SkyrimSe);
        let fallout =
            HandlerRecordContext::new(Signature(*b"NAVM"), FormId(0x1234), 0, SchemaGame::Fallout4);
        let valid = FieldValue::UInt(0);
        let invalid = FieldValue::UInt(2);
        let access = || HandlerInvocationAccess {
            source: HandlerRecordSource::None,
            value_scope: Some(&scope),
            source_subrecord_index: None,
            array_indices: &[],
        };

        // when / then
        assert!(matches!(
            handlers.invoke_with_records(
                &binding,
                skyrim,
                access(),
                HandlerPhase::Display,
                Some(&valid),
                None,
            )?,
            HandlerOutput::Text(text) if text == "0 (1.250000, -2.500000, 0.000000)"
        ));
        let expected_sort_key = format!(
            "+{}1.250000|-{}2.500000|+{}0.000000",
            "0".repeat(31),
            "0".repeat(31),
            "0".repeat(31)
        );
        assert!(matches!(
            handlers.invoke_with_records(
                &binding,
                skyrim,
                access(),
                HandlerPhase::SortKey,
                Some(&valid),
                None,
            )?,
            HandlerOutput::Text(text) if text == expected_sort_key
        ));
        assert!(matches!(
            handlers.invoke_with_records(
                &binding,
                fallout,
                access(),
                HandlerPhase::SortKey,
                Some(&valid),
                None,
            )?,
            HandlerOutput::Text(text) if text.is_empty()
        ));
        assert!(matches!(
            handlers.invoke_with_records(
                &binding,
                fallout,
                access(),
                HandlerPhase::SortKey,
                Some(&invalid),
                None,
            )?,
            HandlerOutput::Text(text) if text == "0002"
        ));
        for (input, expected) in [("17", 17), ("garbage", 0), ("", 0)] {
            let input = FieldValue::String(Cow::Borrowed(input));
            assert!(matches!(
                handlers.invoke(
                    &binding,
                    fallout,
                    HandlerPhase::ParseEditValue,
                    Some(&input),
                    None,
                )?,
                HandlerOutput::Value(FieldValue::Int(value)) if value == expected
            ));
        }
        Ok(())
    }

    fn snap_reference_value(node_id: u32, path: &str) -> crate::NamedValue<'static> {
        crate::NamedValue {
            node_id: bethkit_schema::SchemaNodeId(node_id),
            path: path.to_owned(),
            effective_path: None,
            name: "Reference".to_owned(),
            span: crate::ByteSpan { start: 0, end: 4 },
            value: FieldValue::FormId {
                value: FormId(0x2468),
                targets: vec![Signature(*b"REFR")],
            },
        }
    }

    /// Resolves only valid non-negative indexes into a configured local record array.
    #[test]
    fn local_array_element_handler_resolves_bounded_indexes() -> TestResult {
        // given
        let array_path = "ACTI/41:Navmesh Geometry/payload/variants/1:Navmesh Geometry/2:Vertices";
        let element_path =
            "ACTI/41:Navmesh Geometry/payload/variants/1:Navmesh Geometry/2:Vertices/element";
        let mut binding = test_metadata_binding(
            "value.links_to",
            "resolve.local_array_element",
            serde_json::json!({
                "source_container": "Triangles",
                "target_segment": "2:Vertices"
            }),
        );
        binding.path = concat!(
            "ACTI/41:Navmesh Geometry/payload/variants/1:Navmesh Geometry/",
            "3:Triangles/element/0:Vertex 0"
        )
        .to_owned();
        let scope = FieldValue::Struct(vec![crate::NamedValue {
            node_id: bethkit_schema::SchemaNodeId(1),
            path: array_path.to_owned(),
            effective_path: None,
            name: "Vertices".to_owned(),
            span: crate::ByteSpan { start: 0, end: 24 },
            value: FieldValue::Array(vec![
                FieldValue::Bytes(std::borrow::Cow::Borrowed(&[0; 12])),
                FieldValue::Bytes(std::borrow::Cow::Borrowed(&[1; 12])),
            ]),
        }]);
        let handlers = SemanticHandlerRegistry::builtin();
        let source =
            HandlerRecordContext::new(Signature(*b"NAVM"), FormId(0x1234), 0, SchemaGame::SkyrimSe);

        // when / then
        assert!(matches!(
            handlers.invoke_with_value_scope(
                &binding,
                source,
                HandlerPhase::ReferenceResolution,
                Some(&FieldValue::Int(1)),
                None,
                Some(&scope),
            )?,
            HandlerOutput::Link(SemanticLink::Element {
                path,
                array_indices,
            }) if path == element_path && array_indices == vec![1]
        ));
        for index in [-1, 2] {
            assert!(matches!(
                handlers.invoke_with_value_scope(
                    &binding,
                    source,
                    HandlerPhase::ReferenceResolution,
                    Some(&FieldValue::Int(index)),
                    None,
                    Some(&scope),
                )?,
                HandlerOutput::None
            ));
        }
        for (path, configuration, array_path) in [
            (
                concat!(
                    "ACTI/23:Navmesh Geometry/payload/9:Navmesh Grid/",
                    "9:NavMesh Grid Arrays/element/element"
                ),
                serde_json::json!({
                    "source_container": "Navmesh Grid",
                    "target_segment": "3:Triangles"
                }),
                "ACTI/23:Navmesh Geometry/payload/3:Triangles",
            ),
            (
                "NAVM/4:PreCut Map Entries/payload/element/1:Triangles/element",
                serde_json::json!({
                    "target_path": "NAVM/1:Navmesh Geometry/payload/3:Triangles"
                }),
                "NAVM/1:Navmesh Geometry/payload/3:Triangles",
            ),
        ] {
            let mut binding = test_metadata_binding(
                "value.links_to",
                "resolve.local_array_element",
                configuration,
            );
            binding.path = path.to_owned();
            let scope = FieldValue::Struct(vec![crate::NamedValue {
                node_id: bethkit_schema::SchemaNodeId(2),
                path: array_path.to_owned(),
                effective_path: None,
                name: "Triangles".to_owned(),
                span: crate::ByteSpan { start: 0, end: 4 },
                value: FieldValue::Array(vec![FieldValue::UInt(0)]),
            }]);
            assert!(matches!(
                handlers.invoke_with_value_scope(
                    &binding,
                    source,
                    HandlerPhase::ReferenceResolution,
                    Some(&FieldValue::Int(0)),
                    None,
                    Some(&scope),
                )?,
                HandlerOutput::Link(SemanticLink::Element {
                    path,
                    array_indices,
                }) if path == format!("{array_path}/element") && array_indices == vec![0]
            ));
        }
        Ok(())
    }

    /// Resolves VMAD object aliases through their sibling quest FormID.
    #[test]
    fn vmad_object_alias_formatter_matches_xedit_scope_and_game_policies(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let form_id_path = "TEST/Object/FormID";
        let binding = test_metadata_binding(
            "integer.formatter",
            "format.vmad_object_alias",
            serde_json::json!({ "form_id_path": form_id_path }),
        );
        let scope = FieldValue::Struct(vec![crate::NamedValue {
            node_id: bethkit_schema::SchemaNodeId(1),
            path: form_id_path.to_owned(),
            effective_path: None,
            name: "FormID".to_owned(),
            span: crate::ByteSpan { start: 0, end: 4 },
            value: FieldValue::FormId {
                value: FormId(0x5678),
                targets: vec![Signature(*b"QUST")],
            },
        }]);
        let alias = FieldValue::Int(7);
        let unknown = FieldValue::Int(8);
        let none = FieldValue::Int(-1);
        let mut handlers = SemanticHandlerRegistry::builtin();
        handlers.set_form_link_resolver(Arc::new(TestFormLinkResolver));
        let skyrim =
            HandlerRecordContext::new(Signature(*b"TEST"), FormId::NULL, 0, SchemaGame::SkyrimSe);
        let fallout =
            HandlerRecordContext::new(Signature(*b"TEST"), FormId::NULL, 0, SchemaGame::Fallout4);

        // when / then
        assert!(matches!(
            handlers.invoke_with_value_scope(
                &binding,
                skyrim,
                HandlerPhase::Display,
                Some(&alias),
                None,
                Some(&scope),
            )?,
            HandlerOutput::Text(text) if text == "007 Target"
        ));
        assert!(matches!(
            handlers.invoke_with_value_scope(
                &binding,
                skyrim,
                HandlerPhase::Summary,
                Some(&alias),
                None,
                Some(&scope),
            )?,
            HandlerOutput::Text(text) if text == "Target"
        ));
        assert!(matches!(
            handlers.invoke_with_value_scope(
                &binding,
                fallout,
                HandlerPhase::Summary,
                Some(&alias),
                None,
                Some(&scope),
            )?,
            HandlerOutput::Text(text) if text == "007 Target"
        ));
        assert!(matches!(
            handlers.invoke_with_value_scope(
                &binding,
                skyrim,
                HandlerPhase::Display,
                Some(&unknown),
                None,
                Some(&scope),
            )?,
            HandlerOutput::Text(text)
                if text
                    == "8 <Warning: Quest Alias not found in \"Example Quest \
                        [QUST:00005678]\">"
        ));
        assert!(matches!(
            handlers.invoke_with_value_scope(
                &binding,
                skyrim,
                HandlerPhase::Summary,
                Some(&none),
                None,
                Some(&scope),
            )?,
            HandlerOutput::Text(text) if text.is_empty()
        ));
        assert!(matches!(
            handlers.invoke_with_value_scope(
                &binding,
                fallout,
                HandlerPhase::Summary,
                Some(&none),
                None,
                Some(&scope),
            )?,
            HandlerOutput::Text(text) if text == "None"
        ));
        assert!(matches!(
            handlers.invoke(
                &binding,
                skyrim,
                HandlerPhase::Display,
                Some(&alias),
                None,
            )?,
            HandlerOutput::Text(text) if text.is_empty()
        ));
        let edit = FieldValue::String(Cow::Borrowed("007 Target"));
        assert!(matches!(
            handlers.invoke(
                &binding,
                skyrim,
                HandlerPhase::ParseEditValue,
                Some(&edit),
                None,
            )?,
            HandlerOutput::Value(FieldValue::Int(7))
        ));
        let player = FieldValue::String(Cow::Borrowed("Player"));
        assert!(matches!(
            handlers.invoke(
                &binding,
                fallout,
                HandlerPhase::ParseEditValue,
                Some(&player),
                None,
            )?,
            HandlerOutput::Value(FieldValue::Int(-2))
        ));
        assert!(matches!(
            handlers.invoke(
                &binding,
                skyrim,
                HandlerPhase::ParseEditValue,
                Some(&player),
                None,
            )?,
            HandlerOutput::Value(FieldValue::Int(-1))
        ));
        let link_binding = test_metadata_binding(
            "value.links_to",
            "resolve.vmad_object_alias",
            serde_json::json!({ "form_id_path": form_id_path }),
        );
        assert!(matches!(
            handlers.invoke_with_value_scope(
                &link_binding,
                fallout,
                HandlerPhase::ReferenceResolution,
                Some(&alias),
                None,
                Some(&scope),
            )?,
            HandlerOutput::Link(SemanticLink::QuestAlias {
                quest_form_id: FormId(0x5678),
                alias_index: 7,
            })
        ));
        assert!(matches!(
            handlers.invoke_with_value_scope(
                &link_binding,
                fallout,
                HandlerPhase::ReferenceResolution,
                Some(&unknown),
                None,
                Some(&scope),
            )?,
            HandlerOutput::None
        ));
        assert!(matches!(
            handlers.invoke_with_value_scope(
                &link_binding,
                fallout,
                HandlerPhase::ReferenceResolution,
                Some(&none),
                None,
                Some(&scope),
            )?,
            HandlerOutput::None
        ));
        Ok(())
    }

    /// Resolves same-quest and external Starfield quest aliases.
    #[test]
    fn quest_alias_link_handler_matches_xedit() -> TestResult {
        // given
        let mut handlers = SemanticHandlerRegistry::builtin();
        handlers.set_form_link_resolver(Arc::new(TestFormLinkResolver));
        let source = HandlerRecordContext::new(
            Signature(*b"QUST"),
            FormId(0x5678),
            0,
            SchemaGame::Starfield,
        );
        let alias = FieldValue::Int(7);
        let same_quest = test_metadata_binding(
            "value.links_to",
            "resolve.quest_alias",
            serde_json::json!({ "quest_source": "source_record" }),
        );
        let external_quest = test_metadata_binding(
            "value.links_to",
            "resolve.quest_alias",
            serde_json::json!({ "quest_source": "sibling_quest" }),
        );
        let quest = |form_id| crate::NamedValue {
            node_id: bethkit_schema::SchemaNodeId(1),
            path: "QUST/Alias Type/External Alias Reference/0:Quest".to_owned(),
            effective_path: None,
            name: "Quest".to_owned(),
            span: crate::ByteSpan { start: 0, end: 4 },
            value: FieldValue::FormId {
                value: FormId(form_id),
                targets: vec![Signature(*b"QUST")],
            },
        };
        let scope = FieldValue::Struct(vec![
            crate::NamedValue {
                node_id: bethkit_schema::SchemaNodeId(u32::MAX),
                path: String::new(),
                effective_path: None,
                name: "Bethkit Active Repeat Occurrence".to_owned(),
                span: crate::ByteSpan { start: 0, end: 0 },
                value: FieldValue::Struct(vec![quest(0x5678)]),
            },
            quest(0x9999),
        ]);

        // when / then
        for (binding, value_scope) in [(&same_quest, None), (&external_quest, Some(&scope))] {
            assert!(matches!(
                handlers.invoke_with_value_scope(
                    binding,
                    source,
                    HandlerPhase::ReferenceResolution,
                    Some(&alias),
                    None,
                    value_scope,
                )?,
                HandlerOutput::Link(SemanticLink::QuestAlias {
                    quest_form_id: FormId(0x5678),
                    alias_index: 7,
                })
            ));
        }
        let unknown = FieldValue::Int(99);
        assert!(matches!(
            handlers.invoke(
                &same_quest,
                source,
                HandlerPhase::ReferenceResolution,
                Some(&unknown),
                None,
            )?,
            HandlerOutput::None
        ));
        Ok(())
    }

    /// Resolves Fallout 76 legendary-filter offsets through their base star slot.
    #[test]
    fn legendary_filter_link_handler_matches_xedit() -> TestResult {
        // given
        let filters_path = "LGDI/12:Include Filters";
        let mods_path = "LGDI/11:Legendary Mods";
        let named = |path: &str, name: &str, value: FieldValue<'static>| crate::NamedValue {
            node_id: bethkit_schema::SchemaNodeId(1),
            path: path.to_owned(),
            effective_path: None,
            name: name.to_owned(),
            span: crate::ByteSpan { start: 0, end: 0 },
            value,
        };
        let filter = |slot| {
            FieldValue::Struct(vec![named(
                &format!("{filters_path}/element/0:Star Slot"),
                "Star Slot",
                FieldValue::UInt(slot),
            )])
        };
        let legendary_mod = |slot, form_id| {
            FieldValue::Struct(vec![
                named(
                    &format!("{mods_path}/element/0:Star Slot"),
                    "Star Slot",
                    FieldValue::UInt(slot),
                ),
                named(
                    &format!("{mods_path}/element/1:Legendary Modifier"),
                    "Legendary Modifier",
                    FieldValue::FormId {
                        value: FormId(form_id),
                        targets: Vec::new(),
                    },
                ),
            ])
        };
        let scope = FieldValue::Struct(vec![
            named(
                filters_path,
                "Include Filters",
                FieldValue::Array(vec![filter(1), filter(2)]),
            ),
            named(
                mods_path,
                "Legendary Mods",
                FieldValue::Array(vec![
                    legendary_mod(1, 0x9999),
                    legendary_mod(2, 0x9999),
                    legendary_mod(3, 0x1234),
                ]),
            ),
        ]);
        let mut binding = test_metadata_binding(
            "value.links_to",
            "resolve.legendary_filter_mod",
            serde_json::json!({}),
        );
        binding.path = format!("{filters_path}/element/1:Referenced Mod");
        let mut handlers = SemanticHandlerRegistry::builtin();
        handlers.set_form_link_resolver(Arc::new(TestFormLinkResolver));
        let source = HandlerRecordContext::new(
            Signature(*b"LGDI"),
            FormId(0x5678),
            0,
            SchemaGame::Fallout76,
        );
        let offset = FieldValue::UInt(1);
        let filter_index = [1];

        // when / then
        assert!(matches!(
            handlers.invoke_with_records(
                &binding,
                source,
                HandlerInvocationAccess {
                    source: HandlerRecordSource::None,
                    value_scope: Some(&scope),
                    source_subrecord_index: None,
                    array_indices: &filter_index,
                },
                HandlerPhase::ReferenceResolution,
                Some(&offset),
                None,
            )?,
            HandlerOutput::Link(SemanticLink::Record {
                form_id: FormId(0x1234),
            })
        ));
        Ok(())
    }

    /// Resolves Starfield NPC face indexes through effective race metadata.
    #[test]
    fn npc_face_entry_link_handler_matches_xedit() -> TestResult {
        // given
        let mut handlers = SemanticHandlerRegistry::builtin();
        handlers.set_form_link_resolver(Arc::new(TestFormLinkResolver));
        let source = HandlerRecordContext::new(
            Signature(*b"NPC_"),
            FormId(0x5678),
            0,
            SchemaGame::Starfield,
        );
        let index = FieldValue::UInt(7);

        // when / then
        for (entry_kind, expected_path) in [
            (
                "face_dial",
                "RACE/Chargen and Skintones/Male/Chargen/Face Dials/element",
            ),
            (
                "face_morph_phenotype",
                "RACE/Chargen and Skintones/Male/Chargen/Face Morph Phenotypes/element",
            ),
        ] {
            let binding = test_metadata_binding(
                "value.links_to",
                "resolve.npc_face_entry",
                serde_json::json!({ "entry_kind": entry_kind }),
            );
            assert!(matches!(
                handlers.invoke(
                    &binding,
                    source,
                    HandlerPhase::ReferenceResolution,
                    Some(&index),
                    None,
                )?,
                HandlerOutput::Link(SemanticLink::ExternalElement {
                    record_form_id: FormId(0x2468),
                    path,
                    array_indices,
                }) if path == expected_path && array_indices == vec![3]
            ));
        }
        Ok(())
    }

    /// Formats CTDA quest stages through the quest selected in parameter one.
    #[test]
    fn ctda_quest_stage_formatter_matches_xedit(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let quest_path = "TEST/CTDA/Parameter #1";
        let binding = test_metadata_binding(
            "integer.formatter",
            "format.ctda_quest_stage",
            serde_json::json!({ "quest_path": quest_path }),
        );
        let scope = FieldValue::Struct(vec![crate::NamedValue {
            node_id: bethkit_schema::SchemaNodeId(1),
            path: quest_path.to_owned(),
            effective_path: None,
            name: "Parameter #1".to_owned(),
            span: crate::ByteSpan { start: 0, end: 4 },
            value: FieldValue::FormId {
                value: FormId(0x5678),
                targets: vec![Signature(*b"QUST")],
            },
        }]);
        let mut handlers = SemanticHandlerRegistry::builtin();
        handlers.set_form_link_resolver(Arc::new(TestFormLinkResolver));
        let record =
            HandlerRecordContext::new(Signature(*b"TEST"), FormId::NULL, 0, SchemaGame::Starfield);
        let stage = FieldValue::Int(10);
        let missing = FieldValue::Int(30);
        let none = FieldValue::Int(-1);

        // when / then
        assert!(matches!(
            handlers.invoke_with_value_scope(
                &binding,
                record,
                HandlerPhase::Display,
                Some(&stage),
                None,
                Some(&scope),
            )?,
            HandlerOutput::Text(text) if text == "010 First objective"
        ));
        assert!(matches!(
            handlers.invoke_with_value_scope(
                &binding,
                record,
                HandlerPhase::Display,
                Some(&missing),
                None,
                Some(&scope),
            )?,
            HandlerOutput::Text(text)
                if text
                    == "30 <Warning: Quest Stage not found in \
                        \"Example Quest [QUST:00005678]\">"
        ));
        assert!(matches!(
            handlers.invoke_with_value_scope(
                &binding,
                record,
                HandlerPhase::Summary,
                Some(&none),
                None,
                Some(&scope),
            )?,
            HandlerOutput::Text(text) if text == "-1 NONE"
        ));
        assert!(matches!(
            handlers.invoke_with_value_scope(
                &binding,
                record,
                HandlerPhase::EditValue,
                Some(&none),
                None,
                Some(&scope),
            )?,
            HandlerOutput::Text(text) if text == "-1"
        ));
        let edit = FieldValue::String(Cow::Borrowed("010 First objective"));
        assert!(matches!(
            handlers.invoke(
                &binding,
                record,
                HandlerPhase::ParseEditValue,
                Some(&edit),
                None,
            )?,
            HandlerOutput::Value(FieldValue::Int(10))
        ));
        Ok(())
    }

    /// Resolves legacy CTDA variable indices through parameter one's effective script.
    #[test]
    fn ctda_variable_name_formatter_matches_xedit() -> TestResult {
        // given
        let parameter_path = "TEST/CTDA/Parameter #1";
        let binding = test_metadata_binding(
            "integer.formatter",
            "format.ctda_variable_name",
            serde_json::json!({ "parameter_path": parameter_path }),
        );
        let scope = FieldValue::Struct(vec![crate::NamedValue {
            node_id: bethkit_schema::SchemaNodeId(1),
            path: parameter_path.to_owned(),
            effective_path: None,
            name: "Parameter #1".to_owned(),
            span: crate::ByteSpan { start: 0, end: 4 },
            value: FieldValue::FormId {
                value: FormId(0x3456),
                targets: vec![],
            },
        }]);
        let mut handlers = SemanticHandlerRegistry::builtin();
        handlers.set_form_link_resolver(Arc::new(TestFormLinkResolver));
        let record =
            HandlerRecordContext::new(Signature(*b"TEST"), FormId::NULL, 0, SchemaGame::FalloutNv);

        // when / then
        assert!(matches!(
            handlers.invoke_with_value_scope(
                &binding,
                record,
                HandlerPhase::Display,
                Some(&FieldValue::Int(7)),
                None,
                Some(&scope),
            )?,
            HandlerOutput::Text(text) if text == "TargetVariable"
        ));
        assert!(matches!(
            handlers.invoke_with_value_scope(
                &binding,
                record,
                HandlerPhase::Display,
                Some(&FieldValue::Int(9)),
                None,
                Some(&scope),
            )?,
            HandlerOutput::Text(text)
                if text
                    == "9 <Warning: Variable Index not found in \
                        \"ExampleScript [SCPT:00007890]\">"
        ));
        let edit = FieldValue::String(Cow::Borrowed(" targetvariable "));
        assert!(matches!(
            handlers.invoke_with_value_scope(
                &binding,
                record,
                HandlerPhase::ParseEditValue,
                Some(&edit),
                None,
                Some(&scope),
            )?,
            HandlerOutput::Value(FieldValue::Int(7))
        ));
        assert!(matches!(
            handlers.invoke(
                &binding,
                record,
                HandlerPhase::SortKey,
                Some(&FieldValue::Int(-1)),
                None,
            )?,
            HandlerOutput::Text(text) if text == "FFFFFFFFFFFFFFFF"
        ));
        Ok(())
    }

    /// Resolves Fallout New Vegas CTDA objective indices through parameter one.
    #[test]
    fn ctda_quest_objective_formatter_matches_xedit() -> TestResult {
        // given
        let parameter_path = "TEST/CTDA/Parameter #1";
        let binding = test_metadata_binding(
            "integer.formatter",
            "format.ctda_quest_objective",
            serde_json::json!({ "parameter_path": parameter_path }),
        );
        let scope = FieldValue::Struct(vec![crate::NamedValue {
            node_id: bethkit_schema::SchemaNodeId(1),
            path: parameter_path.to_owned(),
            effective_path: None,
            name: "Parameter #1".to_owned(),
            span: crate::ByteSpan { start: 0, end: 4 },
            value: FieldValue::FormId {
                value: FormId(0x5678),
                targets: vec![Signature(*b"QUST")],
            },
        }]);
        let mut handlers = SemanticHandlerRegistry::builtin();
        handlers.set_form_link_resolver(Arc::new(TestFormLinkResolver));
        let record =
            HandlerRecordContext::new(Signature(*b"TEST"), FormId::NULL, 0, SchemaGame::FalloutNv);

        // when / then
        assert!(matches!(
            handlers.invoke_with_value_scope(
                &binding,
                record,
                HandlerPhase::Summary,
                Some(&FieldValue::Int(10)),
                None,
                Some(&scope),
            )?,
            HandlerOutput::Text(text) if text == "010 Reach the target"
        ));
        assert!(matches!(
            handlers.invoke_with_value_scope(
                &binding,
                record,
                HandlerPhase::Validation,
                Some(&FieldValue::Int(30)),
                None,
                Some(&scope),
            )?,
            HandlerOutput::Text(text)
                if text
                    == "<Warning: Quest Objective not found in \
                        \"Example Quest [QUST:00005678]\">"
        ));
        let edit = FieldValue::String(Cow::Borrowed("020 Optional objective"));
        assert!(matches!(
            handlers.invoke(
                &binding,
                record,
                HandlerPhase::ParseEditValue,
                Some(&edit),
                None,
            )?,
            HandlerOutput::Value(FieldValue::Int(20))
        ));
        Ok(())
    }

    /// Resolves implicit CTDA quests for overlays and parameter-one stage values.
    #[test]
    fn ctda_condition_quest_handlers_match_xedit(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let configuration = serde_json::json!({
            "quest_source": "subrecord",
            "quest_signature": "PNAM",
            "parent_fallback": true
        });
        let overlay = test_metadata_binding(
            "integer.overlay",
            "overlay.ctda_quest",
            configuration.clone(),
        );
        let stage = test_metadata_binding(
            "integer.formatter",
            "format.ctda_context_quest_stage",
            configuration,
        );
        let scene = test_record(*b"SCEN", &[(*b"PNAM", 0x5678_u32.to_le_bytes().to_vec())])?;
        let mut handlers = SemanticHandlerRegistry::builtin();
        handlers.set_form_link_resolver(Arc::new(TestFormLinkResolver));
        let record =
            HandlerRecordContext::new(Signature(*b"SCEN"), FormId::NULL, 0, SchemaGame::Fallout76);
        let null_quest = FieldValue::FormId {
            value: FormId::NULL,
            targets: vec![Signature(*b"QUST")],
        };
        let stage_ten = FieldValue::UInt(10);

        // when / then
        assert!(matches!(
            handlers.invoke_with_source_record(
                &overlay,
                record,
                Some(&scene),
                HandlerPhase::Display,
                Some(&null_quest),
                None,
            )?,
            HandlerOutput::Text(text) if text == "Example Quest [QUST:00005678]"
        ));
        assert!(matches!(
            handlers.invoke_with_source_record(
                &overlay,
                record,
                Some(&scene),
                HandlerPhase::ReferenceResolution,
                Some(&null_quest),
                None,
            )?,
            HandlerOutput::Link(SemanticLink::Record {
                form_id: FormId(0x5678)
            })
        ));
        assert!(matches!(
            handlers.invoke_with_source_record(
                &stage,
                record,
                Some(&scene),
                HandlerPhase::Display,
                Some(&stage_ten),
                None,
            )?,
            HandlerOutput::Text(text) if text == "010 First objective"
        ));
        let edit = FieldValue::String(Cow::Borrowed("020 Later objective"));
        assert!(matches!(
            handlers.invoke(
                &stage,
                record,
                HandlerPhase::ParseEditValue,
                Some(&edit),
                None,
            )?,
            HandlerOutput::Value(FieldValue::UInt(20))
        ));
        Ok(())
    }

    /// Resolves CTDA aliases through the quest context inherited by a scene.
    #[test]
    fn ctda_condition_alias_formatter_matches_xedit(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let binding = test_metadata_binding(
            "integer.formatter",
            "format.ctda_condition_alias",
            serde_json::Value::Null,
        );
        let scene = test_record(*b"SCEN", &[(*b"PNAM", 0x5678_u32.to_le_bytes().to_vec())])?;
        let mut handlers = SemanticHandlerRegistry::builtin();
        handlers.set_form_link_resolver(Arc::new(TestFormLinkResolver));
        let record =
            HandlerRecordContext::new(Signature(*b"SCEN"), FormId::NULL, 0, SchemaGame::Starfield);
        let alias = FieldValue::Int(7);
        let missing = FieldValue::Int(8);

        // when / then
        assert!(matches!(
            handlers.invoke_with_source_record(
                &binding,
                record,
                Some(&scene),
                HandlerPhase::Display,
                Some(&alias),
                None,
            )?,
            HandlerOutput::Text(text) if text == "007 Target"
        ));
        assert!(matches!(
            handlers.invoke_with_source_record(
                &binding,
                record,
                Some(&scene),
                HandlerPhase::Validation,
                Some(&missing),
                None,
            )?,
            HandlerOutput::Text(text)
                if text
                    == "<Warning: Quest Alias not found in \
                        \"Example Quest [QUST:00005678]\">"
        ));
        let player = FieldValue::String(Cow::Borrowed("Player"));
        assert!(matches!(
            handlers.invoke(
                &binding,
                record,
                HandlerPhase::ParseEditValue,
                Some(&player),
                None,
            )?,
            HandlerOutput::Value(FieldValue::Int(-2))
        ));
        Ok(())
    }

    /// Reads and transactionally updates CTDA string parameter subrecords.
    #[test]
    fn ctda_string_parameter_formatter_matches_xedit(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let string_path = "TEST/Condition/CIS1";
        let binding = test_metadata_binding(
            "integer.formatter",
            "format.ctda_string_parameter",
            serde_json::json!({ "string_path": string_path }),
        );
        let scope = FieldValue::Struct(vec![crate::NamedValue {
            node_id: bethkit_schema::SchemaNodeId(1),
            path: string_path.to_owned(),
            effective_path: None,
            name: "Parameter #1".to_owned(),
            span: crate::ByteSpan { start: 0, end: 5 },
            value: FieldValue::String(Cow::Borrowed("Hello")),
        }]);
        let handlers = SemanticHandlerRegistry::builtin();
        let record =
            HandlerRecordContext::new(Signature(*b"TEST"), FormId::NULL, 0, SchemaGame::Starfield);
        let value = FieldValue::UInt(0);

        // when / then
        assert!(matches!(
            handlers.invoke_with_value_scope(
                &binding,
                record,
                HandlerPhase::Display,
                Some(&value),
                None,
                Some(&scope),
            )?,
            HandlerOutput::Text(text) if text == "Hello"
        ));
        let edit = FieldValue::String(Cow::Borrowed("Updated"));
        assert!(matches!(
            handlers.invoke(
                &binding,
                record,
                HandlerPhase::ParseEditValue,
                Some(&edit),
                None,
            )?,
            HandlerOutput::ParsedValue {
                value: FieldValue::UInt(0),
                mutations,
            } if mutations
                == vec![HandlerMutation::SynchronizePresence {
                    path: string_path.to_owned(),
                    occurrence: 0,
                    present: true,
                    value: OwnedFieldValue::String("Updated".to_owned()),
                }]
        ));
        Ok(())
    }

    /// Matches xEdit's Oblivion and modern faction-relation summaries.
    #[test]
    fn faction_relation_summary_uses_resolved_value(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let field = |name: &str, value: FieldValue<'static>| crate::NamedValue {
            node_id: bethkit_schema::SchemaNodeId(1),
            path: format!("TEST/{name}"),
            effective_path: None,
            name: name.to_owned(),
            span: crate::ByteSpan { start: 0, end: 0 },
            value,
        };
        let faction = field(
            "Faction",
            FieldValue::FormId {
                value: FormId(0x1234),
                targets: vec![Signature(*b"FACT")],
            },
        );
        let value = FieldValue::Struct(vec![
            faction.clone(),
            field("Modifier", FieldValue::Int(2)),
            field(
                "Group Combat Reaction",
                FieldValue::Enumeration {
                    value: 2,
                    name: Some("Enemy".to_owned()),
                },
            ),
        ]);
        let modern =
            HandlerRecordContext::new(Signature(*b"FACT"), FormId::NULL, 0, SchemaGame::SkyrimSe);
        let oblivion =
            HandlerRecordContext::new(Signature(*b"FACT"), FormId::NULL, 0, SchemaGame::Oblivion);

        assert_eq!(
            format_faction_relation(&value, Some(&TestFormLinkResolver), modern)?,
            Some("Enemy [00001234] Example Faction".to_owned())
        );
        assert_eq!(
            format_faction_relation(
                &FieldValue::Struct(vec![faction, field("Modifier", FieldValue::Int(2))]),
                Some(&TestFormLinkResolver),
                oblivion,
            )?,
            Some("+2 [00001234] Example Faction".to_owned())
        );
        Ok(())
    }

    /// Matches xEdit's resolved actor-value object-property summary.
    #[test]
    fn object_property_summary_uses_editor_id_and_delphi_general_format(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let field = |name: &str, value: FieldValue<'static>| crate::NamedValue {
            node_id: bethkit_schema::SchemaNodeId(1),
            path: format!("TEST/{name}"),
            effective_path: None,
            name: name.to_owned(),
            span: crate::ByteSpan { start: 0, end: 0 },
            value,
        };
        let value = FieldValue::Struct(vec![
            field(
                "Actor Value",
                FieldValue::FormId {
                    value: FormId(0x1234),
                    targets: vec![Signature(*b"AVIF")],
                },
            ),
            field("Value", FieldValue::Float(12.345_67)),
        ]);
        let source =
            HandlerRecordContext::new(Signature(*b"ACTI"), FormId::NULL, 0, SchemaGame::Fallout4);

        // when
        let result = format_object_property(&value, Some(&TestFormLinkResolver), source)?;

        // then
        assert_eq!(result, Some("ExampleActorValue = 12.346".to_owned()));
        Ok(())
    }

    /// Matches xEdit's Starfield crowd-property summary including curve metadata.
    #[test]
    fn crowd_property_summary_includes_curve_table() -> TestResult {
        let field = |name: &str, value: FieldValue<'static>| crate::NamedValue {
            node_id: bethkit_schema::SchemaNodeId(1),
            path: format!("TEST/{name}"),
            effective_path: None,
            name: name.to_owned(),
            span: crate::ByteSpan { start: 0, end: 0 },
            value,
        };
        let value = FieldValue::Struct(vec![
            field(
                "Actor",
                FieldValue::FormId {
                    value: FormId(0x2468),
                    targets: vec![Signature(*b"NPC_")],
                },
            ),
            field("Value", FieldValue::Float(0.125)),
            field(
                "Curve Table",
                FieldValue::FormId {
                    value: FormId(0x4567),
                    targets: vec![Signature(*b"CURV")],
                },
            ),
        ]);
        let source =
            HandlerRecordContext::new(Signature(*b"ACHR"), FormId::NULL, 0, SchemaGame::Starfield);

        assert_eq!(
            format_linked_float_property(
                &value,
                Some(&TestFormLinkResolver),
                source,
                "format.crowd_property",
            )?,
            Some("ExampleActor = 0.125 {Curve Table: Example Curve [CURV:00004567]}".to_owned())
        );
        Ok(())
    }

    /// Matches xEdit's `wbAtxtPosition` formatting, editing, and range checking.
    #[test]
    fn landscape_position_formatter_matches_xedit_grid() -> TestResult {
        // given
        let binding = test_metadata_binding(
            "integer.formatter",
            "format.landscape_position",
            serde_json::json!({}),
        );
        let handlers = SemanticHandlerRegistry::builtin();
        let context =
            HandlerRecordContext::new(Signature(*b"LAND"), FormId::NULL, 0, SchemaGame::SkyrimSe);

        // when / then
        for (value, display, sort_key) in [
            (0, "0 -> 0:0", "0000"),
            (16, "16 -> 0:16", "0010"),
            (17, "17 -> 1:0", "0100"),
            (288, "288 -> 16:16", "1010"),
        ] {
            let value = FieldValue::UInt(value);
            assert!(matches!(
                handlers.invoke(
                    &binding,
                    context,
                    HandlerPhase::Display,
                    Some(&value),
                    None,
                )?,
                HandlerOutput::Text(text) if text == display
            ));
            assert!(matches!(
                handlers.invoke(
                    &binding,
                    context,
                    HandlerPhase::SortKey,
                    Some(&value),
                    None,
                )?,
                HandlerOutput::Text(text) if text == sort_key
            ));
        }

        let valid = FieldValue::UInt(288);
        let invalid = FieldValue::UInt(289);
        assert!(matches!(
            handlers.invoke(
                &binding,
                context,
                HandlerPhase::Validation,
                Some(&valid),
                None,
            )?,
            HandlerOutput::Text(text) if text.is_empty()
        ));
        assert!(matches!(
            handlers.invoke(
                &binding,
                context,
                HandlerPhase::Validation,
                Some(&invalid),
                None,
            )?,
            HandlerOutput::Text(text) if text == "<Out of range: 289>"
        ));
        assert!(runs_during_validation(&binding));

        let edit = FieldValue::String(Cow::Borrowed("$0120"));
        assert!(matches!(
            handlers.invoke(
                &binding,
                context,
                HandlerPhase::ParseEditValue,
                Some(&edit),
                None,
            )?,
            HandlerOutput::Value(FieldValue::UInt(288))
        ));
        Ok(())
    }

    /// Matches the xEdit climate moon masks for modern and legacy Fallout games.
    #[test]
    fn climate_moon_formatter_matches_xedit_game_masks() -> TestResult {
        // given
        let binding = test_metadata_binding(
            "integer.formatter",
            "format.climate_moons",
            serde_json::json!({}),
        );
        let handlers = SemanticHandlerRegistry::builtin();
        let value = FieldValue::UInt(64);

        // when / then
        for (game, expected) in [
            (SchemaGame::SkyrimSe, "Masser / 0"),
            (SchemaGame::Fallout4, "Masser / 0"),
            (SchemaGame::Fallout3, "Secunda / 0"),
            (SchemaGame::FalloutNv, "Secunda / 0"),
        ] {
            let context = HandlerRecordContext::new(Signature(*b"CLMT"), FormId::NULL, 0, game);
            assert!(matches!(
                handlers.invoke(
                    &binding,
                    context,
                    HandlerPhase::Display,
                    Some(&value),
                    None,
                )?,
                HandlerOutput::Text(text) if text == expected
            ));
        }

        let both = FieldValue::UInt(255);
        let context =
            HandlerRecordContext::new(Signature(*b"CLMT"), FormId::NULL, 0, SchemaGame::SkyrimSe);
        assert!(matches!(
            handlers.invoke(
                &binding,
                context,
                HandlerPhase::Summary,
                Some(&both),
                None,
            )?,
            HandlerOutput::Text(text) if text == "Masser, Secunda / 63"
        ));
        assert!(matches!(
            handlers.invoke(
                &binding,
                context,
                HandlerPhase::SortKey,
                Some(&both),
                None,
            )?,
            HandlerOutput::Text(text) if text == "FF"
        ));
        Ok(())
    }

    /// Matches xEdit's six-values-per-hour climate time formatter.
    #[test]
    fn climate_time_formatter_matches_xedit_units_and_fallback() -> TestResult {
        // given
        let binding = test_metadata_binding(
            "integer.formatter",
            "format.climate_time",
            serde_json::json!({}),
        );
        let handlers = SemanticHandlerRegistry::builtin();
        let context =
            HandlerRecordContext::new(Signature(*b"CLMT"), FormId::NULL, 0, SchemaGame::SkyrimSe);

        // when / then
        for (value, expected) in [(0, "00:00:00"), (39, "06:30:00"), (143, "23:50:00")] {
            assert!(matches!(
                handlers.invoke(
                    &binding,
                    context,
                    HandlerPhase::Display,
                    Some(&FieldValue::UInt(value)),
                    None,
                )?,
                HandlerOutput::Text(text) if text == expected
            ));
        }
        assert!(matches!(
            handlers.invoke(
                &binding,
                context,
                HandlerPhase::Summary,
                Some(&FieldValue::UInt(144)),
                None,
            )?,
            HandlerOutput::Text(text) if text == "144"
        ));
        assert!(matches!(
            handlers.invoke(
                &binding,
                context,
                HandlerPhase::SortKey,
                Some(&FieldValue::UInt(39)),
                None,
            )?,
            HandlerOutput::Text(text) if text == "0027"
        ));
        Ok(())
    }

    /// Matches xEdit's 1/256-day media location time formatter.
    #[test]
    fn aloc_time_formatter_matches_xedit_units_and_sorting() -> TestResult {
        // given
        let binding = test_metadata_binding(
            "integer.formatter",
            "format.aloc_time",
            serde_json::json!({}),
        );
        let handlers = SemanticHandlerRegistry::builtin();
        let context =
            HandlerRecordContext::new(Signature(*b"ALOC"), FormId::NULL, 0, SchemaGame::FalloutNv);

        // when / then
        for (value, expected) in [
            (0, "00:00:00"),
            (64, "06:00:00"),
            (128, "12:00:00"),
            (192, "18:00:00"),
            (256, "00:00:00"),
        ] {
            assert!(matches!(
                handlers.invoke(
                    &binding,
                    context,
                    HandlerPhase::Display,
                    Some(&FieldValue::UInt(value)),
                    None,
                )?,
                HandlerOutput::Text(text) if text == expected
            ));
        }
        assert!(matches!(
            handlers.invoke(
                &binding,
                context,
                HandlerPhase::SortKey,
                Some(&FieldValue::UInt(255)),
                None,
            )?,
            HandlerOutput::Text(text) if text == "00FF"
        ));
        Ok(())
    }

    /// Matches the legacy xEdit idle-group names, flags, and validation.
    #[test]
    fn idle_animation_formatter_matches_xedit_games() -> TestResult {
        let binding = test_metadata_binding(
            "integer.formatter",
            "format.idle_animation_group",
            serde_json::json!({}),
        );
        let handlers = SemanticHandlerRegistry::builtin();
        for (game, value, expected) in [
            (SchemaGame::Oblivion, 5, "Whole Body, Must return a file"),
            (SchemaGame::Fallout3, 5, "Weapon Up, Must return a file"),
            (SchemaGame::FalloutNv, 0x95, "Upper Body"),
        ] {
            let context = HandlerRecordContext::new(Signature(*b"IDLE"), FormId::NULL, 0, game);
            let value = FieldValue::UInt(value);
            assert!(matches!(
                handlers.invoke(
                    &binding,
                    context,
                    HandlerPhase::Display,
                    Some(&value),
                    None,
                )?,
                HandlerOutput::Text(text) if text == expected
            ));
        }

        let invalid = FieldValue::UInt(22);
        let context =
            HandlerRecordContext::new(Signature(*b"IDLE"), FormId::NULL, 0, SchemaGame::Fallout3);
        assert!(matches!(
            handlers.invoke(
                &binding,
                context,
                HandlerPhase::Validation,
                Some(&invalid),
                None,
            )?,
            HandlerOutput::Text(text) if text == "<Unknown: 22>"
        ));
        Ok(())
    }

    /// Matches Oblivion's extra weather class and Fallout's stricter validation.
    #[test]
    fn weather_classification_formatter_matches_xedit_games() -> TestResult {
        let binding = test_metadata_binding(
            "integer.formatter",
            "format.weather_classification",
            serde_json::json!({}),
        );
        let handlers = SemanticHandlerRegistry::builtin();
        let value = FieldValue::UInt(3);
        for (game, expected_display, expected_check) in [
            (SchemaGame::Oblivion, "Unknown 3", ""),
            (SchemaGame::Fallout3, "<Unknown: 3>", "<Unknown: 3>"),
            (SchemaGame::FalloutNv, "<Unknown: 3>", "<Unknown: 3>"),
        ] {
            let context = HandlerRecordContext::new(Signature(*b"WTHR"), FormId::NULL, 0, game);
            assert!(matches!(
                handlers.invoke(
                    &binding,
                    context,
                    HandlerPhase::Display,
                    Some(&value),
                    None,
                )?,
                HandlerOutput::Text(text) if text == expected_display
            ));
            assert!(matches!(
                handlers.invoke(
                    &binding,
                    context,
                    HandlerPhase::Validation,
                    Some(&value),
                    None,
                )?,
                HandlerOutput::Text(text) if text == expected_check
            ));
        }
        Ok(())
    }

    /// Matches xEdit's fixed-width Morrowind object-index text and edit prefix.
    #[test]
    fn fixed_hex_integer_formatter_matches_morrowind_index() -> TestResult {
        let binding = test_metadata_binding(
            "integer.formatter",
            "format.fixed_hex_integer",
            serde_json::json!({ "width": 8, "edit_prefix": "$" }),
        );
        let handlers = SemanticHandlerRegistry::builtin();
        let context =
            HandlerRecordContext::new(Signature(*b"REFR"), FormId::NULL, 0, SchemaGame::Morrowind);
        let value = FieldValue::UInt(0x1234);
        assert!(matches!(
            handlers.invoke(
                &binding,
                context,
                HandlerPhase::Display,
                Some(&value),
                None,
            )?,
            HandlerOutput::Text(text) if text == "00001234"
        ));
        assert!(matches!(
            handlers.invoke(
                &binding,
                context,
                HandlerPhase::EditValue,
                Some(&value),
                None,
            )?,
            HandlerOutput::Text(text) if text == "$00001234"
        ));
        let edit = FieldValue::String(Cow::Borrowed("$00001234"));
        assert!(matches!(
            handlers.invoke(
                &binding,
                context,
                HandlerPhase::ParseEditValue,
                Some(&edit),
                None,
            )?,
            HandlerOutput::Value(FieldValue::UInt(0x1234))
        ));
        Ok(())
    }

    /// Matches xEdit's plain hexadecimal formatter and forgiving edit parser.
    #[test]
    fn fixed_hex_integer_formatter_matches_wb_hex_str_to_int() -> TestResult {
        let binding = test_metadata_binding(
            "integer.formatter",
            "format.fixed_hex_integer",
            serde_json::json!({ "width": 8, "plain_hex_edit": true }),
        );
        let handlers = SemanticHandlerRegistry::builtin();
        let context =
            HandlerRecordContext::new(Signature(*b"RACE"), FormId::NULL, 0, SchemaGame::Fallout4);
        let value = FieldValue::UInt(0x89AB_CDEF);
        assert!(matches!(
            handlers.invoke(
                &binding,
                context,
                HandlerPhase::EditValue,
                Some(&value),
                None,
            )?,
            HandlerOutput::Text(text) if text == "89ABCDEF"
        ));
        for (input, expected) in [
            ("0000ABCD", 0xABCD),
            ("0000ABCD ignored", 0xABCD),
            ("0000ABCD:ignored", 0xABCD),
            ("$0000ABCD", 0),
            ("not-hex", 0),
        ] {
            let edit = FieldValue::String(Cow::Borrowed(input));
            assert!(matches!(
                handlers.invoke(
                    &binding,
                    context,
                    HandlerPhase::ParseEditValue,
                    Some(&edit),
                    None,
                )?,
                HandlerOutput::Value(FieldValue::UInt(value)) if value == expected
            ));
        }
        let edit = FieldValue::String(Cow::Borrowed("0000ABCD:ignored later"));
        assert!(matches!(
            handlers.invoke(
                &binding,
                context,
                HandlerPhase::ParseEditValue,
                Some(&edit),
                None,
            )?,
            HandlerOutput::Value(FieldValue::UInt(0))
        ));
        Ok(())
    }

    /// Matches xEdit's scaled four-decimal formatter and edit parser.
    #[test]
    fn scaled_int4_formatter_matches_xedit() -> TestResult {
        // given
        let binding = test_metadata_binding(
            "integer.formatter",
            "format.scaled_int4",
            serde_json::json!({}),
        );
        let handlers = SemanticHandlerRegistry::builtin();
        let context =
            HandlerRecordContext::new(Signature(*b"REGN"), FormId::NULL, 0, SchemaGame::Oblivion);

        // when / then
        for (value, expected) in [
            (0_i64, "0.0000"),
            (1, "0.0001"),
            (10_000, "1.0000"),
            (-12_345, "-1.2345"),
        ] {
            assert!(matches!(
                handlers.invoke(
                    &binding,
                    context,
                    HandlerPhase::Display,
                    Some(&FieldValue::Int(value)),
                    None,
                )?,
                HandlerOutput::Text(text) if text == expected
            ));
        }
        assert!(matches!(
            handlers.invoke(
                &binding,
                context,
                HandlerPhase::SortKey,
                Some(&FieldValue::Int(-12_345)),
                None,
            )?,
            HandlerOutput::Text(text) if text == "-000000000000000-1.2345"
        ));
        for (input, expected) in [("1.23445", 12_344_i64), ("1.23455", 12_346)] {
            assert!(matches!(
                handlers.invoke(
                    &binding,
                    context,
                    HandlerPhase::ParseEditValue,
                    Some(&FieldValue::String(input.into())),
                    None,
                )?,
                HandlerOutput::Value(FieldValue::UInt(value)) if value == expected as u64
            ));
        }
        Ok(())
    }

    /// Matches xEdit's hidden-FFFF display and sort formatting.
    #[test]
    fn hide_ffff_formatter_matches_xedit() -> TestResult {
        // given
        let binding = test_metadata_binding(
            "integer.formatter",
            "format.hide_ffff",
            serde_json::json!({}),
        );
        let handlers = SemanticHandlerRegistry::builtin();
        let context =
            HandlerRecordContext::new(Signature(*b"REGN"), FormId::NULL, 0, SchemaGame::Oblivion);

        // when / then
        for (value, phase, expected) in [
            (0xffff_u64, HandlerPhase::Display, "None"),
            (42, HandlerPhase::Summary, "42"),
            (42, HandlerPhase::SortKey, "002A"),
        ] {
            assert!(matches!(
                handlers.invoke(
                    &binding,
                    context,
                    phase,
                    Some(&FieldValue::UInt(value)),
                    None,
                )?,
                HandlerOutput::Text(text) if text == expected
            ));
        }
        Ok(())
    }

    /// Matches xEdit's fixed cloud-speed display, parsing, and upper clamp.
    #[test]
    fn cloud_speed_formatter_matches_xedit() -> TestResult {
        // given
        let binding = test_metadata_binding(
            "integer.formatter",
            "format.cloud_speed",
            serde_json::json!({}),
        );
        let handlers = SemanticHandlerRegistry::builtin();
        let context =
            HandlerRecordContext::new(Signature(*b"WTHR"), FormId::NULL, 0, SchemaGame::SkyrimSe);

        // when / then
        for (value, expected) in [
            (0_u64, "-0.1000"),
            (127, "0.0000"),
            (254, "0.1000"),
            (255, "0.1008"),
        ] {
            assert!(matches!(
                handlers.invoke(
                    &binding,
                    context,
                    HandlerPhase::Display,
                    Some(&FieldValue::UInt(value)),
                    None,
                )?,
                HandlerOutput::Text(text) if text == expected
            ));
        }
        for (input, expected) in [
            ("-0.1", 0_i64),
            ("0", 127),
            ("0.1", 254),
            ("1", 254),
            ("-0.2", -127),
        ] {
            let output = handlers.invoke(
                &binding,
                context,
                HandlerPhase::ParseEditValue,
                Some(&FieldValue::String(input.into())),
                None,
            )?;
            let actual = match output {
                HandlerOutput::Value(FieldValue::Int(value)) => value,
                HandlerOutput::Value(FieldValue::UInt(value)) => {
                    i64::try_from(value).expect("cloud speed test values fit in a signed integer")
                }
                _ => panic!("cloud speed parser did not return an integer value"),
            };
            assert_eq!(actual, expected);
        }
        Ok(())
    }

    /// Matches xEdit's plugin-header object-ID display and `?` edit behavior.
    #[test]
    fn next_object_id_formatter_matches_xedit() -> TestResult {
        // given
        let binding = test_metadata_binding(
            "integer.formatter",
            "format.next_object_id",
            serde_json::json!({}),
        );
        let context =
            HandlerRecordContext::new(Signature(*b"TES4"), FormId::NULL, 0, SchemaGame::SkyrimSe);
        let value = FieldValue::UInt(0x1234);
        let mut handlers = SemanticHandlerRegistry::builtin();
        handlers.set_next_object_id_resolver(Arc::new(TestNextObjectIdResolver));

        // when / then
        for (phase, expected) in [
            (HandlerPhase::Display, "00001234"),
            (HandlerPhase::SortKey, "00001234"),
            (HandlerPhase::EditValue, "$00001234"),
            (HandlerPhase::Summary, ""),
            (HandlerPhase::NativeValue, ""),
        ] {
            assert!(matches!(
                handlers.invoke(&binding, context, phase, Some(&value), None)?,
                HandlerOutput::Text(text) if text == expected
            ));
        }
        let automatic = FieldValue::String(Cow::Borrowed(" ? "));
        assert!(matches!(
            handlers.invoke(
                &binding,
                context,
                HandlerPhase::ParseEditValue,
                Some(&automatic),
                None,
            )?,
            HandlerOutput::Value(FieldValue::UInt(0x1235))
        ));
        let fallback = SemanticHandlerRegistry::builtin();
        assert!(matches!(
            fallback.invoke(
                &binding,
                context,
                HandlerPhase::ParseEditValue,
                Some(&automatic),
                None,
            )?,
            HandlerOutput::Value(FieldValue::UInt(2048))
        ));
        Ok(())
    }

    /// Matches Delphi's five-significant-digit `ffGeneral` thresholds and special values.
    #[test]
    fn delphi_general_formatter_matches_xedit_boundaries(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let cases = [
            (0.0, "0"),
            (-0.0, "-0"),
            (99_999.0, "99999"),
            (100_000.0, "1E005"),
            (0.000_1, "0.0001"),
            (0.000_01, "1E-005"),
            (12_300_000_000.0, "1.23E010"),
            (f64::INFINITY, "INF"),
            (f64::NEG_INFINITY, "-INF"),
        ];

        // when / then
        for (value, expected) in cases {
            assert_eq!(format_delphi_general(value, 5), expected);
        }
        assert_eq!(format_delphi_general(f64::NAN, 5), "NAN");
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
                    == concat!(
                        "Play_Test {00112233-4455-6677-8899-AABBCCDDEEFF} ",
                        "\"\\Events\\Default Work Unit\\",
                        "Play_Test_With_A_Long_Object_Path_123456789\""
                    )
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
            effective_path: None,
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
            effective_path: None,
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

    /// Selects COED owner data from the resolved owner record signature.
    #[test]
    fn coed_owner_selector_matches_xedit_links() -> TestResult {
        // given
        let binding = test_metadata_binding(
            "union.select",
            "select.coed_owner",
            serde_json::json!({ "owner_offset": 0 }),
        );
        let context =
            HandlerRecordContext::new(Signature(*b"CONT"), FormId::NULL, 0, SchemaGame::Fallout4);
        let mut handlers = SemanticHandlerRegistry::builtin();
        handlers.set_form_link_resolver(Arc::new(TestFormLinkResolver));

        // when / then
        for (form_id, expected) in [(0x2468_u32, 1_i64), (0x1234, 2), (0x9999, 0)] {
            let payload = FieldValue::Bytes(Cow::Owned(form_id.to_le_bytes().to_vec()));
            assert!(matches!(
                handlers.invoke(
                    &binding,
                    context,
                    HandlerPhase::UnionSelection,
                    Some(&payload),
                    None,
                )?,
                HandlerOutput::Integer(selected) if selected == expected
            ));
        }
        Ok(())
    }

    /// Selects NOTE payload layouts from the record-level DNAM type.
    #[test]
    fn note_data_selector_matches_xedit_type_mapping() -> TestResult {
        // given
        let binding =
            test_metadata_binding("union.select", "select.note_data", serde_json::json!({}));
        let handlers = SemanticHandlerRegistry::builtin();
        let context =
            HandlerRecordContext::new(Signature(*b"NOTE"), FormId::NULL, 0, SchemaGame::Fallout4);

        // when / then
        for (note_type, expected) in [(0_u8, 1_i64), (1, 2), (2, 0), (3, 3), (255, 0)] {
            let record = test_record(*b"NOTE", &[(*b"DNAM", vec![note_type])])?;
            assert!(matches!(
                handlers.invoke_with_source_record(
                    &binding,
                    context,
                    Some(&record),
                    HandlerPhase::UnionSelection,
                    None,
                    None,
                )?,
                HandlerOutput::Integer(selected) if selected == expected
            ));
        }
        let record = test_record(*b"NOTE", &[])?;
        assert!(matches!(
            handlers.invoke_with_source_record(
                &binding,
                context,
                Some(&record),
                HandlerPhase::UnionSelection,
                None,
                None,
            )?,
            HandlerOutput::Integer(0)
        ));
        Ok(())
    }

    /// Selects the AutoWeapon SNDR layout from read-only and writable records.
    #[test]
    fn sound_descriptor_selector_matches_xedit_type_mapping() -> TestResult {
        // given
        let binding = test_metadata_binding(
            "union.select",
            "select.sound_descriptor_data",
            serde_json::json!({}),
        );
        let handlers = SemanticHandlerRegistry::builtin();
        let context =
            HandlerRecordContext::new(Signature(*b"SNDR"), FormId::NULL, 0, SchemaGame::Fallout76);
        let auto_weapon = test_record(
            *b"SNDR",
            &[(*b"CNAM", 0xED15_7AE3_u32.to_le_bytes().to_vec())],
        )?;

        // when / then
        assert!(matches!(
            handlers.invoke_with_source_record(
                &binding,
                context,
                Some(&auto_weapon),
                HandlerPhase::UnionSelection,
                None,
                None,
            )?,
            HandlerOutput::Integer(1)
        ));
        let standard = WritableRecord {
            signature: Signature(*b"SNDR"),
            flags: RecordFlags::empty(),
            form_id: FormId::NULL,
            form_version: 0,
            subrecords: vec![bethkit_core::WritableSubRecord {
                signature: Signature(*b"CNAM"),
                data: 0x1EEF_540A_u32.to_le_bytes().to_vec(),
            }],
        };
        assert!(matches!(
            handlers.invoke_with_writable_record(
                &binding,
                context,
                &standard,
                HandlerPhase::UnionSelection,
                None,
                None,
            )?,
            HandlerOutput::Integer(0)
        ));
        Ok(())
    }

    /// Selects each repeated AECH payload from its nearest preceding KNAM type.
    #[test]
    fn audio_effect_selector_uses_exact_subrecord_position() -> TestResult {
        // given
        let binding = test_metadata_binding(
            "union.select",
            "select.audio_effect_data",
            serde_json::json!({}),
        );
        let handlers = SemanticHandlerRegistry::builtin();
        let context =
            HandlerRecordContext::new(Signature(*b"AECH"), FormId::NULL, 0, SchemaGame::Fallout4);
        let record = test_record(
            *b"AECH",
            &[
                (*b"KNAM", 0x8648_04BE_u32.to_le_bytes().to_vec()),
                (*b"DNAM", vec![0; 20]),
                (*b"KNAM", 0x1883_7B4F_u32.to_le_bytes().to_vec()),
                (*b"DNAM", vec![0; 16]),
            ],
        )?;

        // when / then
        for (source_index, expected) in [(1_usize, 0_i64), (3, 2)] {
            assert!(matches!(
                handlers.invoke_with_subrecord(
                    &binding,
                    context,
                    HandlerSubrecordSource::ReadOnly {
                        record: &record,
                        index: source_index,
                    },
                    HandlerPhase::UnionSelection,
                    None,
                    None,
                )?,
                HandlerOutput::Integer(selected) if selected == expected
            ));
        }
        Ok(())
    }

    /// Selects Starfield component payloads from each repeat-local BFCB name.
    #[test]
    fn starfield_component_selectors_use_exact_subrecord_position() -> TestResult {
        // given
        let data_binding = test_metadata_binding(
            "union.select",
            "select.starfield_component_data",
            serde_json::json!({}),
        );
        let dat2_binding = test_metadata_binding(
            "union.select",
            "select.starfield_component_dat2",
            serde_json::json!({}),
        );
        let handlers = SemanticHandlerRegistry::builtin();
        let context =
            HandlerRecordContext::new(Signature(*b"PLAN"), FormId::NULL, 0, SchemaGame::Starfield);
        let record = test_record(
            *b"PLAN",
            &[
                (*b"BFCB", b"BGSStarDataComponent_Component\0".to_vec()),
                (*b"DATA", vec![0; 8]),
                (*b"BFCB", b"UniqueOverlayList_Component\0".to_vec()),
                (*b"DATA", vec![0; 8]),
                (*b"BFCB", b"BlockHeightAdjustment_Component\0".to_vec()),
                (*b"DAT2", vec![0; 8]),
            ],
        )?;

        // when / then
        for (binding, source_index, expected) in [
            (&data_binding, 1_usize, 1_i64),
            (&data_binding, 3, 5),
            (&dat2_binding, 5, 1),
        ] {
            assert!(matches!(
                handlers.invoke_with_subrecord(
                    binding,
                    context,
                    HandlerSubrecordSource::ReadOnly {
                        record: &record,
                        index: source_index,
                    },
                    HandlerPhase::UnionSelection,
                    None,
                    None,
                )?,
                HandlerOutput::Integer(selected) if selected == expected
            ));
        }
        Ok(())
    }

    /// Selects each Oblivion OBME parameter from its repeat-local EFME metadata.
    #[test]
    fn oblivion_obme_parameter_selectors_use_exact_subrecord_position() -> TestResult {
        // given
        let efit_binding = test_metadata_binding(
            "union.select",
            "select.oblivion_obme_efit_parameter",
            serde_json::json!({}),
        );
        let efix_binding = test_metadata_binding(
            "union.select",
            "select.oblivion_obme_efix_parameter",
            serde_json::json!({}),
        );
        let handlers = SemanticHandlerRegistry::builtin();
        let context =
            HandlerRecordContext::new(Signature(*b"ALCH"), FormId::NULL, 0, SchemaGame::Oblivion);
        let record = test_record(
            *b"ALCH",
            &[
                (*b"EFME", vec![0, 0, 0, 0, 1, 2]),
                (*b"EFIT", vec![0; 24]),
                (*b"EFIX", vec![0; 20]),
                (*b"EFME", vec![0, 0, 0, 0, 3, 1]),
                (*b"EFIT", vec![0; 24]),
                (*b"EFIX", vec![0; 20]),
            ],
        )?;

        // when / then
        for (binding, source_index, expected) in [
            (&efit_binding, 1_usize, 1_i64),
            (&efix_binding, 2, 2),
            (&efit_binding, 4, 3),
            (&efix_binding, 5, 1),
        ] {
            assert!(matches!(
                handlers.invoke_with_subrecord(
                    binding,
                    context,
                    HandlerSubrecordSource::ReadOnly {
                        record: &record,
                        index: source_index,
                    },
                    HandlerPhase::UnionSelection,
                    None,
                    None,
                )?,
                HandlerOutput::Integer(selected) if selected == expected
            ));
        }
        Ok(())
    }

    /// Selects GMST value layouts from the first editor-ID character.
    #[test]
    fn game_setting_value_selector_matches_all_xedit_type_codes() -> TestResult {
        // given
        let binding = test_metadata_binding(
            "union.select",
            "select.game_setting_value",
            serde_json::json!({}),
        );
        let handlers = SemanticHandlerRegistry::builtin();
        let context =
            HandlerRecordContext::new(Signature(*b"GMST"), FormId::NULL, 0, SchemaGame::Fallout76);

        // when / then
        for (editor_id, expected) in [
            ("sSetting", 0_i64),
            ("iSetting", 1),
            ("fSetting", 2),
            ("bSetting", 3),
            ("uSetting", 4),
            ("xSetting", 1),
            ("", 1),
        ] {
            let record = test_record(
                *b"GMST",
                &[
                    (*b"EDID", [editor_id.as_bytes(), &[0]].concat()),
                    (*b"DATA", vec![0; 4]),
                ],
            )?;
            assert!(matches!(
                handlers.invoke_with_subrecord(
                    &binding,
                    context,
                    HandlerSubrecordSource::ReadOnly {
                        record: &record,
                        index: 1,
                    },
                    HandlerPhase::UnionSelection,
                    None,
                    None,
                )?,
                HandlerOutput::Integer(selected) if selected == expected
            ));
        }
        Ok(())
    }

    /// Selects Fallout 3 and New Vegas NOTE voice references from DATA.
    #[test]
    fn legacy_note_voice_selector_matches_xedit_type_mapping() -> TestResult {
        // given
        let binding = test_metadata_binding(
            "union.select",
            "select.legacy_note_voice",
            serde_json::json!({}),
        );
        let handlers = SemanticHandlerRegistry::builtin();
        let context =
            HandlerRecordContext::new(Signature(*b"NOTE"), FormId::NULL, 0, SchemaGame::Fallout3);

        // when / then
        for (note_type, expected) in [(0_u8, 0_i64), (1, 0), (2, 0), (3, 1), (255, 0)] {
            let record = test_record(
                *b"NOTE",
                &[
                    (*b"DATA", vec![note_type]),
                    (*b"TNAM", vec![0; 4]),
                    (*b"SNAM", vec![0; 4]),
                ],
            )?;
            for source_index in [1_usize, 2] {
                assert!(matches!(
                    handlers.invoke_with_subrecord(
                        &binding,
                        context,
                        HandlerSubrecordSource::ReadOnly {
                            record: &record,
                            index: source_index,
                        },
                        HandlerPhase::UnionSelection,
                        None,
                        None,
                    )?,
                    HandlerOutput::Integer(selected) if selected == expected
                ));
            }
        }
        Ok(())
    }

    /// Selects package input layouts from the repeat-local ANAM type.
    #[test]
    fn package_input_value_selector_matches_xedit_type_mapping() -> TestResult {
        // given
        let binding = test_metadata_binding(
            "union.select",
            "select.package_input_value",
            serde_json::json!({}),
        );
        let handlers = SemanticHandlerRegistry::builtin();
        let context =
            HandlerRecordContext::new(Signature(*b"PACK"), FormId::NULL, 0, SchemaGame::SkyrimSe);

        // when / then
        for (input_type, expected) in [
            ("Bool", 1_i64),
            ("Int", 2),
            ("Float", 3),
            ("ObjectList", 3),
            ("Target", 0),
            ("", 0),
        ] {
            let record = test_record(
                *b"PACK",
                &[
                    (*b"ANAM", [input_type.as_bytes(), &[0]].concat()),
                    (*b"CNAM", vec![0; 4]),
                ],
            )?;
            assert!(matches!(
                handlers.invoke_with_subrecord(
                    &binding,
                    context,
                    HandlerSubrecordSource::ReadOnly {
                        record: &record,
                        index: 1,
                    },
                    HandlerPhase::UnionSelection,
                    None,
                    None,
                )?,
                HandlerOutput::Integer(selected) if selected == expected
            ));
        }
        Ok(())
    }

    /// Selects Morrowind global layouts from the FNAM type byte.
    #[test]
    fn morrowind_global_value_selector_matches_xedit_type_mapping() -> TestResult {
        // given
        let binding = test_metadata_binding(
            "union.select",
            "select.morrowind_global_value",
            serde_json::json!({}),
        );
        let handlers = SemanticHandlerRegistry::builtin();
        let context =
            HandlerRecordContext::new(Signature(*b"GLOB"), FormId::NULL, 0, SchemaGame::Morrowind);

        // when / then
        for (value_type, expected) in [(b's', 0_i64), (b'l', 1), (b'f', 2), (b'x', 0)] {
            let record = test_record(
                *b"GLOB",
                &[(*b"FNAM", vec![value_type]), (*b"FLTV", vec![0; 4])],
            )?;
            assert!(matches!(
                handlers.invoke_with_subrecord(
                    &binding,
                    context,
                    HandlerSubrecordSource::ReadOnly {
                        record: &record,
                        index: 1,
                    },
                    HandlerPhase::UnionSelection,
                    None,
                    None,
                )?,
                HandlerOutput::Integer(selected) if selected == expected
            ));
        }
        Ok(())
    }

    /// Selects Oblivion MISC actor-value layouts from record flags.
    #[test]
    fn oblivion_misc_actor_value_selector_requires_both_xedit_flags() -> TestResult {
        // given
        let binding = test_metadata_binding(
            "union.select",
            "select.oblivion_misc_actor_value",
            serde_json::json!({}),
        );
        let handlers = SemanticHandlerRegistry::builtin();
        let context =
            HandlerRecordContext::new(Signature(*b"MISC"), FormId::NULL, 0, SchemaGame::Oblivion);

        // when / then
        for (flags, expected) in [(0_u32, 0_i64), (0x40, 0), (0x80, 0), (0xc0, 1)] {
            let mut record = test_record(*b"MISC", &[(*b"DATA", vec![0; 8])])?;
            record.header.flags = RecordFlags::from_bits_retain(flags);
            assert!(matches!(
                handlers.invoke_with_subrecord(
                    &binding,
                    context,
                    HandlerSubrecordSource::ReadOnly {
                        record: &record,
                        index: 0,
                    },
                    HandlerPhase::UnionSelection,
                    None,
                    None,
                )?,
                HandlerOutput::Integer(selected) if selected == expected
            ));
        }
        Ok(())
    }

    /// Selects each repeated PERK effect layout from its nearest preceding PRKE type.
    #[test]
    fn perk_effect_data_selector_uses_repeat_local_type() -> TestResult {
        // given
        let binding = test_metadata_binding(
            "union.select",
            "select.perk_effect_data",
            serde_json::json!({}),
        );
        let handlers = SemanticHandlerRegistry::builtin();
        let context =
            HandlerRecordContext::new(Signature(*b"PERK"), FormId::NULL, 0, SchemaGame::SkyrimSe);
        let record = test_record(
            *b"PERK",
            &[
                (*b"PRKE", vec![0, 1]),
                (*b"DATA", vec![0; 8]),
                (*b"PRKE", vec![2, 3]),
                (*b"DATA", vec![0; 8]),
                (*b"PRKE", vec![1, 4]),
                (*b"DATA", vec![0; 8]),
            ],
        )?;

        // when / then
        for (source_index, expected) in [(1_usize, 0_i64), (3, 2), (5, 1)] {
            assert!(matches!(
                handlers.invoke_with_subrecord(
                    &binding,
                    context,
                    HandlerSubrecordSource::ReadOnly {
                        record: &record,
                        index: source_index,
                    },
                    HandlerPhase::UnionSelection,
                    None,
                    None,
                )?,
                HandlerOutput::Integer(selected) if selected == expected
            ));
        }
        Ok(())
    }

    /// Selects repeated PERK entry-point data with each game's xEdit remapping.
    #[test]
    fn perk_entry_point_data_selector_matches_xedit_remapping() -> TestResult {
        // given
        let binding = test_metadata_binding(
            "union.select",
            "select.perk_entry_point_data",
            serde_json::json!({}),
        );
        let handlers = SemanticHandlerRegistry::builtin();

        // when / then
        for (game, function, expected) in [
            (SchemaGame::Fallout3, 5_u8, 5_i64),
            (SchemaGame::FalloutNv, 12, 2),
            (SchemaGame::SkyrimLe, 5, 8),
            (SchemaGame::SkyrimSe, 12, 8),
            (SchemaGame::Fallout4, 13, 8),
            (SchemaGame::Fallout76, 14, 8),
            (SchemaGame::Starfield, 4, 2),
        ] {
            let context = HandlerRecordContext::new(Signature(*b"PERK"), FormId::NULL, 0, game);
            let record = test_record(
                *b"PERK",
                &[
                    (*b"DATA", vec![0, function, 0]),
                    (*b"EPFT", vec![2]),
                    (*b"EPFD", vec![0; 8]),
                ],
            )?;
            assert!(matches!(
                handlers.invoke_with_subrecord(
                    &binding,
                    context,
                    HandlerSubrecordSource::ReadOnly {
                        record: &record,
                        index: 2,
                    },
                    HandlerPhase::UnionSelection,
                    None,
                    None,
                )?,
                HandlerOutput::Integer(selected) if selected == expected
            ));
        }

        let context =
            HandlerRecordContext::new(Signature(*b"PERK"), FormId::NULL, 0, SchemaGame::SkyrimSe);
        let record = test_record(
            *b"PERK",
            &[
                (*b"DATA", vec![0, 5, 0]),
                (*b"EPFT", vec![2]),
                (*b"EPFD", vec![0; 8]),
                (*b"DATA", vec![0, 4, 0]),
                (*b"EPFT", vec![7]),
                (*b"EPFD", vec![0; 4]),
            ],
        )?;
        assert!(matches!(
            handlers.invoke_with_subrecord(
                &binding,
                context,
                HandlerSubrecordSource::ReadOnly {
                    record: &record,
                    index: 5,
                },
                HandlerPhase::UnionSelection,
                None,
                None,
            )?,
            HandlerOutput::Integer(7)
        ));
        Ok(())
    }

    /// Selects both Fallout 76 EPF3 payloads from their repeat-local EPFT type.
    #[test]
    fn perk_epf3_selector_uses_repeat_local_type() -> TestResult {
        // given
        let binding =
            test_metadata_binding("union.select", "select.perk_epf3", serde_json::json!({}));
        let handlers = SemanticHandlerRegistry::builtin();
        let context =
            HandlerRecordContext::new(Signature(*b"PERK"), FormId::NULL, 0, SchemaGame::Fallout76);
        let record = test_record(
            *b"PERK",
            &[
                (*b"EPFT", vec![4]),
                (*b"EPF3", vec![0; 4]),
                (*b"EPFT", vec![8]),
                (*b"EPF3", vec![0; 4]),
                (*b"EPF3", vec![0; 4]),
            ],
        )?;

        // when / then
        for (source_index, expected) in [(1_usize, 0_i64), (3, 1), (4, 1)] {
            assert!(matches!(
                handlers.invoke_with_subrecord(
                    &binding,
                    context,
                    HandlerSubrecordSource::ReadOnly {
                        record: &record,
                        index: source_index,
                    },
                    HandlerPhase::UnionSelection,
                    None,
                    None,
                )?,
                HandlerOutput::Integer(selected) if selected == expected
            ));
        }
        Ok(())
    }

    /// Selects record-flag variants for both read-only and writable records.
    #[test]
    fn record_flag_selector_matches_xedit_mask() -> TestResult {
        // given
        let binding = test_metadata_binding(
            "union.select",
            "select.record_flag",
            serde_json::json!({ "mask": 64 }),
        );
        let handlers = SemanticHandlerRegistry::builtin();
        let context =
            HandlerRecordContext::new(Signature(*b"INFO"), FormId::NULL, 0, SchemaGame::Fallout4);
        let mut record = test_record(*b"INFO", &[(*b"ENAM", vec![0; 2])])?;
        record.header.flags = RecordFlags::from_bits_retain(0x40);
        let writable = WritableRecord {
            signature: Signature(*b"INFO"),
            flags: RecordFlags::empty(),
            form_id: FormId::NULL,
            form_version: 0,
            subrecords: Vec::new(),
        };

        // when / then
        assert!(matches!(
            handlers.invoke_with_source_record(
                &binding,
                context,
                Some(&record),
                HandlerPhase::UnionSelection,
                None,
                None,
            )?,
            HandlerOutput::Integer(1)
        ));
        assert!(matches!(
            handlers.invoke_with_writable_record(
                &binding,
                context,
                &writable,
                HandlerPhase::UnionSelection,
                None,
                None,
            )?,
            HandlerOutput::Integer(0)
        ));
        Ok(())
    }

    /// Selects every Starfield bone-modifier payload from its prefixed type name.
    #[test]
    fn bone_modifier_selector_matches_xedit_type_names() -> TestResult {
        // given
        let path = "BMOD/data/0:Type";
        let binding = test_metadata_binding(
            "union.select",
            "select.bone_modifier_type",
            serde_json::json!({ "path": path }),
        );
        let handlers = SemanticHandlerRegistry::builtin();
        let context =
            HandlerRecordContext::new(Signature(*b"BMOD"), FormId::NULL, 0, SchemaGame::Starfield);

        // when / then
        for (type_name, expected) in [
            ("LookAtChain", 1_i64),
            ("morphdriver", 2),
            ("PoseDeformer", 3),
            ("SPRINGBONE", 4),
            ("Unknown", 0),
        ] {
            let scope = FieldValue::Struct(vec![crate::NamedValue {
                node_id: bethkit_schema::SchemaNodeId(1),
                path: path.to_owned(),
                effective_path: None,
                name: "Type".to_owned(),
                span: crate::ByteSpan { start: 0, end: 0 },
                value: FieldValue::String(Cow::Borrowed(type_name)),
            }]);
            assert!(matches!(
                handlers.invoke_with_value_scope(
                    &binding,
                    context,
                    HandlerPhase::UnionSelection,
                    None,
                    None,
                    Some(&scope),
                )?,
                HandlerOutput::Integer(selected) if selected == expected
            ));
        }
        assert!(handlers
            .invoke(&binding, context, HandlerPhase::UnionSelection, None, None,)
            .is_err());
        Ok(())
    }

    /// Selects the empty-string variant from the exact configured sibling path.
    #[test]
    fn empty_string_selector_requires_the_configured_scope() -> TestResult {
        // given
        let path = "TEST/data/0:ScriptName";
        let binding = test_metadata_binding(
            "union.select",
            "select.empty_string",
            serde_json::json!({ "path": path }),
        );
        let handlers = SemanticHandlerRegistry::builtin();
        let context =
            HandlerRecordContext::new(Signature(*b"TEST"), FormId::NULL, 0, SchemaGame::Fallout4);
        let scope = |value: &'static str| {
            FieldValue::Struct(vec![crate::NamedValue {
                node_id: bethkit_schema::SchemaNodeId(1),
                path: path.to_owned(),
                effective_path: None,
                name: "ScriptName".to_owned(),
                span: crate::ByteSpan { start: 0, end: 0 },
                value: FieldValue::String(Cow::Borrowed(value)),
            }])
        };

        // when / then
        for (value, expected) in [("", 1_i64), ("QuestScript", 0)] {
            assert!(matches!(
                handlers.invoke_with_value_scope(
                    &binding,
                    context,
                    HandlerPhase::UnionSelection,
                    None,
                    None,
                    Some(&scope(value)),
                )?,
                HandlerOutput::Integer(selected) if selected == expected
            ));
        }
        assert!(handlers
            .invoke(&binding, context, HandlerPhase::UnionSelection, None, None,)
            .is_err());
        Ok(())
    }

    /// Selects Starfield CTDA parameter variants from the exported xEdit table.
    #[test]
    fn ctda_parameter_selector_uses_function_table_and_flags() -> Result<()> {
        // given
        let binding = CallbackBinding {
            path: "TEST/0:CTDA/payload/5:Parameter #1".to_owned(),
            callback_id: "union.select".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-ctda-parameter".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "select.ctda_parameter".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({
                        "parameter": 1,
                        "type_offset": 0,
                        "type_width": 1,
                        "type_signed": false,
                        "type_byte_order": "little",
                        "function_offset": 8,
                        "function_width": 2,
                        "function_signed": false,
                        "function_byte_order": "little",
                        "run_on_offset": 20,
                        "run_on_width": 4,
                        "run_on_signed": false,
                        "run_on_byte_order": "little"
                    }),
                },
            },
        };
        let table = ConditionFunctionTable::new(
            Some(9),
            Some(39),
            vec![
                bethkit_schema::ConditionFunction::new(
                    1,
                    "GetDistance",
                    "",
                    [36, 1, 1],
                    [true, false, false],
                ),
                bethkit_schema::ConditionFunction::new(
                    2,
                    "GetIsCurrentPackage",
                    "",
                    [38, 1, 1],
                    [true, false, false],
                ),
            ],
        );
        let mut handlers = SemanticHandlerRegistry::builtin();
        handlers.set_condition_function_table(Arc::new(table));
        let record =
            HandlerRecordContext::new(Signature(*b"TEST"), FormId::NULL, 0, SchemaGame::Starfield);
        let select = |function: u16, flags: u8, run_on: u32| -> Result<i64> {
            let mut payload = vec![0_u8; 24];
            payload[0] = flags;
            payload[8..10].copy_from_slice(&function.to_le_bytes());
            payload[20..24].copy_from_slice(&run_on.to_le_bytes());
            let value = FieldValue::Bytes(Cow::Owned(payload));
            match handlers.invoke(
                &binding,
                record,
                HandlerPhase::UnionSelection,
                Some(&value),
                None,
            )? {
                HandlerOutput::Integer(value) => Ok(value),
                _ => Err(ctda_parameter_error(
                    "test selector returned a non-integer value",
                )),
            }
        };

        // when / then
        assert_eq!(select(1, 0, 0)?, 36);
        assert_eq!(select(1, 0x02, 14)?, 9);
        assert_eq!(select(1, 0x08, 0)?, 39);
        assert_eq!(select(2, 0x02, 5)?, 38);
        assert_eq!(select(2, 0x0A, 5)?, 38);
        assert_eq!(select(999, 0, 0)?, 0);
        Ok(())
    }

    /// Formats and parses CTDA function identifiers exactly like the xEdit tables.
    #[test]
    fn ctda_function_formatter_uses_exported_names_and_game_policies() -> Result<()> {
        // given
        let binding = CallbackBinding {
            path: "TEST/0:CTDA/payload/3:Function".to_owned(),
            callback_id: "integer.formatter".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-ctda-function".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "format.ctda_function".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({}),
                },
            },
        };
        let table = ConditionFunctionTable::new(
            Some(9),
            Some(39),
            vec![bethkit_schema::ConditionFunction::new(
                1,
                "GetDistance",
                "",
                [36, 1, 1],
                [true, false, false],
            )],
        );
        let mut handlers = SemanticHandlerRegistry::builtin();
        handlers.set_condition_function_table(Arc::new(table));
        let skyrim =
            HandlerRecordContext::new(Signature(*b"TEST"), FormId::NULL, 0, SchemaGame::SkyrimSe);
        let fallout_nv =
            HandlerRecordContext::new(Signature(*b"TEST"), FormId::NULL, 0, SchemaGame::FalloutNv);
        let known = FieldValue::UInt(1);
        let unknown = FieldValue::UInt(999);
        let text = |record, phase, value: &FieldValue<'static>| -> Result<String> {
            match handlers.invoke(&binding, record, phase, Some(value), None)? {
                HandlerOutput::Text(value) => Ok(value),
                _ => Err(ctda_function_error(
                    "test formatter returned a non-text value",
                )),
            }
        };

        // when / then
        assert_eq!(text(skyrim, HandlerPhase::Display, &known)?, "GetDistance");
        assert_eq!(
            text(skyrim, HandlerPhase::Display, &unknown)?,
            "<Unknown: 999>"
        );
        assert_eq!(text(skyrim, HandlerPhase::Summary, &unknown)?, "999");
        assert_eq!(
            text(fallout_nv, HandlerPhase::Summary, &unknown)?,
            "<Unknown: 999>"
        );
        assert_eq!(text(skyrim, HandlerPhase::SortKey, &unknown)?, "000003E7");
        assert_eq!(text(skyrim, HandlerPhase::EditValue, &unknown)?, "999");
        assert_eq!(text(skyrim, HandlerPhase::Validation, &known)?, "");
        assert_eq!(
            text(skyrim, HandlerPhase::Validation, &unknown)?,
            "<Unknown: 999>"
        );

        let named_edit = FieldValue::String(Cow::Borrowed("getdistance"));
        assert!(matches!(
            handlers.invoke(
                &binding,
                skyrim,
                HandlerPhase::ParseEditValue,
                Some(&named_edit),
                None,
            )?,
            HandlerOutput::Value(FieldValue::Int(1))
        ));
        let numeric_edit = FieldValue::String(Cow::Borrowed("$10"));
        assert!(matches!(
            handlers.invoke(
                &binding,
                skyrim,
                HandlerPhase::ParseEditValue,
                Some(&numeric_edit),
                None,
            )?,
            HandlerOutput::Value(FieldValue::Int(16))
        ));
        Ok(())
    }

    /// Formats complete CTDA condition summaries and repeat connectors like xEdit.
    #[test]
    fn ctda_condition_summary_matches_xedit() -> Result<()> {
        // given
        let field = |name: &str, value: FieldValue<'static>, effective_path: Option<&str>| {
            crate::NamedValue {
                node_id: bethkit_schema::SchemaNodeId(1),
                path: format!("TEST/Conditions/CTDA/{name}"),
                effective_path: effective_path.map(str::to_owned),
                name: name.to_owned(),
                span: crate::ByteSpan { start: 0, end: 0 },
                value,
            }
        };
        let position = FieldValue::Struct(vec![
            field("Index", FieldValue::UInt(0), None),
            field("Count", FieldValue::UInt(2), None),
        ]);
        let ctda = FieldValue::Struct(vec![
            field("Type", FieldValue::UInt(0x40), None),
            field(
                "Comparison Value",
                FieldValue::Float(1.25),
                Some("TEST/Comparison/variants/0:Float"),
            ),
            field("Function", FieldValue::UInt(1), None),
            field(
                "Parameter #1",
                FieldValue::UInt(7),
                Some("TEST/Parameter1/variants/2:Integer"),
            ),
            field(
                "Parameter #2",
                FieldValue::Bytes(Cow::Borrowed(&[0; 4])),
                Some("TEST/Parameter2/variants/1:None"),
            ),
            field(
                "Run On",
                FieldValue::Enumeration {
                    value: 2,
                    name: Some("Reference".to_owned()),
                },
                None,
            ),
            field(
                "Reference",
                FieldValue::FormId {
                    value: FormId(0x1234),
                    targets: vec![Signature(*b"REFR")],
                },
                Some("TEST/Reference/variants/1:Reference"),
            ),
        ]);
        let value = FieldValue::Struct(vec![
            field("CTDA", ctda, None),
            field("Bethkit Repeat Position", position, None),
        ]);
        let table = ConditionFunctionTable::new(
            None,
            None,
            vec![bethkit_schema::ConditionFunction::new(
                1,
                "GetDistance",
                "",
                [0, 0, 0],
                [false, false, false],
            )],
        );
        let source =
            HandlerRecordContext::new(Signature(*b"TEST"), FormId::NULL, 0, SchemaGame::SkyrimSe);

        // when
        let summary = format_ctda_condition(
            &value,
            SchemaGame::SkyrimSe,
            Some(&table),
            Some(&TestFormLinkResolver),
            source,
        )?;

        // then
        assert_eq!(
            summary,
            "([00001234] Example Faction).GetDistance(7) > 1.25 AND"
        );
        assert!(format_ctda_condition(
            &FieldValue::UInt(0),
            SchemaGame::SkyrimSe,
            Some(&table),
            None,
            source,
        )
        .is_err());
        Ok(())
    }

    /// Migrates modern-sized legacy CTDA payloads without changing unrelated bytes.
    #[test]
    fn legacy_ctda_after_load_preserves_existing_reference_bytes() -> Result<()> {
        let binding = CallbackBinding {
            path: "TEST/0:CTDA".to_owned(),
            callback_id: "def.after_load".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-ctda-after-load".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "migrate.legacy_ctda_run_on".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({}),
                },
            },
        };
        let mut data = (0_u8..28).collect::<Vec<_>>();
        data[0] = 0x43;
        let record = WritableRecord {
            signature: Signature(*b"TEST"),
            flags: RecordFlags::empty(),
            form_id: FormId::NULL,
            form_version: 0,
            subrecords: vec![bethkit_core::WritableSubRecord {
                signature: Signature(*b"CTDA"),
                data: data.clone(),
            }],
        };
        let output = SemanticHandlerRegistry::builtin().invoke_with_records(
            &binding,
            HandlerRecordContext::new(Signature(*b"TEST"), FormId::NULL, 0, SchemaGame::Fallout3),
            HandlerInvocationAccess::writable_subrecord_with_scope(&record, 0, None),
            HandlerPhase::AfterLoad,
            None,
            None,
        )?;

        let HandlerOutput::SubrecordPayload(migrated) = output else {
            return Err(SemanticError::Handler {
                handler: "migrate.legacy_ctda_run_on".to_owned(),
                message: "legacy CTDA migration did not return a payload".to_owned(),
            });
        };
        assert_eq!(migrated.len(), 28);
        assert_eq!(migrated[0], 0x41);
        assert_eq!(&migrated[1..20], &data[1..20]);
        assert_eq!(&migrated[20..24], &1_u32.to_le_bytes());
        assert_eq!(&migrated[24..], &data[24..]);

        let mut normalized = record;
        normalized.subrecords[0].data[0] = 0x41;
        let unchanged = SemanticHandlerRegistry::builtin().invoke_with_records(
            &binding,
            HandlerRecordContext::new(Signature(*b"TEST"), FormId::NULL, 0, SchemaGame::Fallout3),
            HandlerInvocationAccess::writable_subrecord_with_scope(&normalized, 0, None),
            HandlerPhase::AfterLoad,
            None,
            None,
        )?;
        assert!(matches!(unchanged, HandlerOutput::None));
        Ok(())
    }

    /// Synchronizes legacy EFIT actor values through the resolved sibling EFID.
    #[test]
    fn legacy_efit_after_load_uses_resolved_magic_effect_actor_value() -> Result<()> {
        let binding = CallbackBinding {
            path: "TEST/0:Effect/1:EFIT".to_owned(),
            callback_id: "def.after_load".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-efit-after-load".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "migrate.legacy_efit_actor_value".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({}),
                },
            },
        };
        let mut original_efit = (0_u8..20).collect::<Vec<_>>();
        original_efit[16..20].copy_from_slice(&(-1_i32).to_le_bytes());
        let record = WritableRecord {
            signature: Signature(*b"TEST"),
            flags: RecordFlags::empty(),
            form_id: FormId(0x1111),
            form_version: 0,
            subrecords: vec![
                bethkit_core::WritableSubRecord {
                    signature: Signature(*b"EFID"),
                    data: 0x6789_u32.to_le_bytes().to_vec(),
                },
                bethkit_core::WritableSubRecord {
                    signature: Signature(*b"EFIT"),
                    data: original_efit.clone(),
                },
            ],
        };
        let mut handlers = SemanticHandlerRegistry::builtin();
        handlers.set_form_link_resolver(Arc::new(TestFormLinkResolver));
        let output = handlers.invoke_with_records(
            &binding,
            HandlerRecordContext::new(
                Signature(*b"TEST"),
                FormId(0x1111),
                0,
                SchemaGame::FalloutNv,
            ),
            HandlerInvocationAccess::writable_subrecord_with_scope(&record, 1, None),
            HandlerPhase::AfterLoad,
            None,
            None,
        )?;

        let HandlerOutput::SubrecordPayload(migrated) = output else {
            return Err(SemanticError::Handler {
                handler: "migrate.legacy_efit_actor_value".to_owned(),
                message: "legacy EFIT migration did not return a payload".to_owned(),
            });
        };
        assert_eq!(&migrated[..16], &original_efit[..16]);
        assert_eq!(&migrated[16..20], &48_i32.to_le_bytes());

        let unresolved = SemanticHandlerRegistry::builtin().invoke_with_records(
            &binding,
            HandlerRecordContext::new(
                Signature(*b"TEST"),
                FormId(0x1111),
                0,
                SchemaGame::FalloutNv,
            ),
            HandlerInvocationAccess::writable_subrecord_with_scope(&record, 1, None),
            HandlerPhase::AfterLoad,
            None,
            None,
        )?;
        assert!(matches!(unresolved, HandlerOutput::None));
        Ok(())
    }

    /// Synchronizes Oblivion EFIT actor values through the resolved MGEF effect code.
    #[test]
    fn oblivion_efit_after_load_uses_resolved_magic_effect_metadata() -> Result<()> {
        let binding = CallbackBinding {
            path: "TEST/0:Effect/1:EFIT".to_owned(),
            callback_id: "def.after_load".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-oblivion-efit-after-load".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "migrate.oblivion_efit_actor_value".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({}),
                },
            },
        };
        let mut original_efit = (0_u8..24).collect::<Vec<_>>();
        original_efit[..4].copy_from_slice(b"ABCD");
        original_efit[20..24].copy_from_slice(&(-1_i32).to_le_bytes());
        let record = WritableRecord {
            signature: Signature(*b"TEST"),
            flags: RecordFlags::empty(),
            form_id: FormId(0x1111),
            form_version: 0,
            subrecords: vec![bethkit_core::WritableSubRecord {
                signature: Signature(*b"EFIT"),
                data: original_efit.clone(),
            }],
        };
        let mut handlers = SemanticHandlerRegistry::builtin();
        handlers.set_form_link_resolver(Arc::new(TestFormLinkResolver));
        let source =
            HandlerRecordContext::new(Signature(*b"TEST"), FormId(0x1111), 0, SchemaGame::Oblivion);
        let output = handlers.invoke_with_records(
            &binding,
            source,
            HandlerInvocationAccess::writable_subrecord_with_scope(&record, 0, None),
            HandlerPhase::AfterLoad,
            None,
            None,
        )?;

        let HandlerOutput::SubrecordPayload(migrated) = output else {
            return Err(SemanticError::Handler {
                handler: "migrate.oblivion_efit_actor_value".to_owned(),
                message: "Oblivion EFIT migration did not return a payload".to_owned(),
            });
        };
        assert_eq!(&migrated[..20], &original_efit[..20]);
        assert_eq!(&migrated[20..24], &42_i32.to_le_bytes());

        let unresolved = SemanticHandlerRegistry::builtin().invoke_with_records(
            &binding,
            source,
            HandlerInvocationAccess::writable_subrecord_with_scope(&record, 0, None),
            HandlerPhase::AfterLoad,
            None,
            None,
        )?;
        assert!(matches!(unresolved, HandlerOutput::None));
        Ok(())
    }

    /// Verifies that Oblivion EFIX cannot expose the EFIT members used by the callback.
    #[test]
    fn oblivion_efix_after_load_verifier_rejects_signature_drift() -> Result<()> {
        let binding = CallbackBinding {
            path: "TEST/0:Effect/1:EFIX".to_owned(),
            callback_id: "def.after_load".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-oblivion-efix-after-load".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "verify.inert_oblivion_efix_after_load".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({}),
                },
            },
        };
        let source =
            HandlerRecordContext::new(Signature(*b"TEST"), FormId(0x1111), 0, SchemaGame::Oblivion);
        let record = |signature| WritableRecord {
            signature: Signature(*b"TEST"),
            flags: RecordFlags::empty(),
            form_id: FormId(0x1111),
            form_version: 0,
            subrecords: vec![bethkit_core::WritableSubRecord {
                signature,
                data: vec![0x5a; 20],
            }],
        };
        let valid = record(Signature(*b"EFIX"));
        let output = SemanticHandlerRegistry::builtin().invoke_with_records(
            &binding,
            source,
            HandlerInvocationAccess::writable_subrecord_with_scope(&valid, 0, None),
            HandlerPhase::AfterLoad,
            None,
            None,
        )?;
        assert!(matches!(output, HandlerOutput::None));

        let drifted = record(Signature(*b"EFIT"));
        let error = SemanticHandlerRegistry::builtin()
            .invoke_with_records(
                &binding,
                source,
                HandlerInvocationAccess::writable_subrecord_with_scope(&drifted, 0, None),
                HandlerPhase::AfterLoad,
                None,
                None,
            )
            .expect_err("an EFIT subrecord must fail the EFIX verifier");
        assert!(error.to_string().contains("received EFIT"));
        Ok(())
    }

    /// Removes the first orphaned keyword array only when its KSIZ counter is absent.
    #[test]
    fn orphaned_keyword_after_load_matches_xedit_record_cleanup() -> Result<()> {
        let binding = CallbackBinding {
            path: "MISC".to_owned(),
            callback_id: "def.after_load".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-orphaned-keyword-after-load".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "migrate.remove_orphaned_keyword_array".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({
                        "keyword_path": "MISC/10:Keywords",
                    }),
                },
            },
        };
        let source =
            HandlerRecordContext::new(Signature(*b"MISC"), FormId(0x1111), 0, SchemaGame::SkyrimSe);
        let record = |flags, signatures: &[[u8; 4]]| WritableRecord {
            signature: Signature(*b"MISC"),
            flags,
            form_id: FormId(0x1111),
            form_version: 0,
            subrecords: signatures
                .iter()
                .map(|signature| bethkit_core::WritableSubRecord {
                    signature: Signature(*signature),
                    data: vec![0x5a; 4],
                })
                .collect(),
        };
        let orphaned = record(RecordFlags::empty(), &[*b"KWDA"]);
        let output = SemanticHandlerRegistry::builtin().invoke_with_writable_record(
            &binding,
            source,
            &orphaned,
            HandlerPhase::AfterLoad,
            None,
            None,
        )?;
        assert!(matches!(
            output,
            HandlerOutput::Mutations(mutations)
                if mutations
                    == [HandlerMutation::Remove {
                        path: "MISC/10:Keywords".to_owned(),
                        occurrence: 0,
                    }]
        ));

        let counted = record(RecordFlags::empty(), &[*b"KSIZ", *b"KWDA"]);
        let output = SemanticHandlerRegistry::builtin().invoke_with_writable_record(
            &binding,
            source,
            &counted,
            HandlerPhase::AfterLoad,
            None,
            None,
        )?;
        assert!(matches!(output, HandlerOutput::None));

        let deleted = record(RecordFlags::DELETED, &[*b"KWDA"]);
        let output = SemanticHandlerRegistry::builtin().invoke_with_writable_record(
            &binding,
            source,
            &deleted,
            HandlerPhase::AfterLoad,
            None,
            None,
        )?;
        assert!(matches!(output, HandlerOutput::None));
        Ok(())
    }

    /// Verifies the unconditional early exit in guarded ARMA and Skyrim RACE migrations.
    #[test]
    fn body_template_after_load_verifier_rejects_unguarded_records() -> Result<()> {
        let binding = CallbackBinding {
            path: "ARMA".to_owned(),
            callback_id: "def.after_load".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-body-template-after-load".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "verify.inert_body_template_after_load".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({}),
                },
            },
        };
        let record = |signature| WritableRecord {
            signature,
            flags: RecordFlags::empty(),
            form_id: FormId(0x1111),
            form_version: 0,
            subrecords: Vec::new(),
        };
        let valid = record(Signature(*b"ARMA"));
        let output = SemanticHandlerRegistry::builtin().invoke_with_writable_record(
            &binding,
            HandlerRecordContext::new(Signature(*b"ARMA"), FormId(0x1111), 0, SchemaGame::SkyrimSe),
            &valid,
            HandlerPhase::AfterLoad,
            None,
            None,
        )?;
        assert!(matches!(output, HandlerOutput::None));

        let mut race_binding = binding.clone();
        race_binding.path = "RACE".to_owned();
        let race = record(Signature(*b"RACE"));
        let output = SemanticHandlerRegistry::builtin().invoke_with_writable_record(
            &race_binding,
            HandlerRecordContext::new(Signature(*b"RACE"), FormId(0x1111), 0, SchemaGame::SkyrimSe),
            &race,
            HandlerPhase::AfterLoad,
            None,
            None,
        )?;
        assert!(matches!(output, HandlerOutput::None));
        let error = SemanticHandlerRegistry::builtin()
            .invoke_with_writable_record(
                &race_binding,
                HandlerRecordContext::new(
                    Signature(*b"RACE"),
                    FormId(0x1111),
                    0,
                    SchemaGame::Fallout4,
                ),
                &race,
                HandlerPhase::AfterLoad,
                None,
                None,
            )
            .expect_err("a Fallout 4 RACE record must fail the Skyrim verifier");
        assert!(error.to_string().contains("guarded ARMA or Skyrim RACE"));

        let drifted = record(Signature(*b"ARMO"));
        let error = SemanticHandlerRegistry::builtin()
            .invoke_with_writable_record(
                &binding,
                HandlerRecordContext::new(
                    Signature(*b"ARMO"),
                    FormId(0x1111),
                    0,
                    SchemaGame::SkyrimSe,
                ),
                &drifted,
                HandlerPhase::AfterLoad,
                None,
                None,
            )
            .expect_err("an ARMO record must fail the ARMA verifier");
        assert!(error.to_string().contains("guarded ARMA or Skyrim RACE"));
        Ok(())
    }

    /// Reconciles MESG message-box flags and display-time presence exactly like xEdit.
    #[test]
    fn message_after_load_matches_xedit_truth_table() -> Result<()> {
        let binding = CallbackBinding {
            path: "MESG".to_owned(),
            callback_id: "def.after_load".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-message-after-load".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "migrate.message_display_time".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({
                        "flags_path": "MESG/5:Flags",
                        "display_time_path": "MESG/6:Display Time",
                    }),
                },
            },
        };
        let source =
            HandlerRecordContext::new(Signature(*b"MESG"), FormId(0x1111), 0, SchemaGame::SkyrimSe);
        let record = |flags: Option<u32>, display_time: bool| {
            let mut subrecords = Vec::new();
            if let Some(flags) = flags {
                subrecords.push(bethkit_core::WritableSubRecord {
                    signature: Signature(*b"DNAM"),
                    data: flags.to_le_bytes().to_vec(),
                });
            }
            if display_time {
                subrecords.push(bethkit_core::WritableSubRecord {
                    signature: Signature(*b"TNAM"),
                    data: 10_u32.to_le_bytes().to_vec(),
                });
            }
            WritableRecord {
                signature: Signature(*b"MESG"),
                flags: RecordFlags::empty(),
                form_id: FormId(0x1111),
                form_version: 0,
                subrecords,
            }
        };
        let invoke = |record: &WritableRecord| {
            SemanticHandlerRegistry::builtin().invoke_with_writable_record(
                &binding,
                source,
                record,
                HandlerPhase::AfterLoad,
                None,
                None,
            )
        };

        let message_box_with_time = record(Some(1), true);
        assert!(matches!(
            invoke(&message_box_with_time)?,
            HandlerOutput::Mutations(mutations)
                if mutations
                    == [HandlerMutation::Remove {
                        path: "MESG/6:Display Time".to_owned(),
                        occurrence: 0,
                    }]
        ));

        let ordinary_without_time = record(Some(2), false);
        assert!(matches!(
            invoke(&ordinary_without_time)?,
            HandlerOutput::Mutations(mutations)
                if mutations
                    == [HandlerMutation::Set {
                        path: "MESG/5:Flags".to_owned(),
                        occurrence: 0,
                        value: OwnedFieldValue::UInt(3),
                    }]
        ));

        let missing_both = record(None, false);
        assert!(matches!(
            invoke(&missing_both)?,
            HandlerOutput::Mutations(mutations)
                if mutations
                    == [HandlerMutation::Insert {
                        path: "MESG/5:Flags".to_owned(),
                        value: OwnedFieldValue::UInt(1),
                    }]
        ));

        assert!(matches!(
            invoke(&record(Some(1), false))?,
            HandlerOutput::None
        ));
        assert!(matches!(
            invoke(&record(Some(0), true))?,
            HandlerOutput::None
        ));
        Ok(())
    }

    /// Removes only zero-use DOBJ entries while retaining every byte of other entries.
    #[test]
    fn default_object_after_load_removes_empty_entries() -> Result<()> {
        let binding = CallbackBinding {
            path: "DOBJ/1:Objects/payload".to_owned(),
            callback_id: "def.after_load".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-default-object-after-load".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "migrate.remove_empty_default_objects".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({}),
                },
            },
        };
        let source =
            HandlerRecordContext::new(Signature(*b"DOBJ"), FormId(0x1111), 0, SchemaGame::SkyrimSe);
        let retained_one = [1_u32.to_le_bytes(), 0x1234_u32.to_le_bytes()].concat();
        let removed = [0_u32.to_le_bytes(), 0x5678_u32.to_le_bytes()].concat();
        let retained_two = [2_u32.to_le_bytes(), 0x9abc_u32.to_le_bytes()].concat();
        let record = WritableRecord {
            signature: Signature(*b"DOBJ"),
            flags: RecordFlags::empty(),
            form_id: FormId(0x1111),
            form_version: 0,
            subrecords: vec![bethkit_core::WritableSubRecord {
                signature: Signature(*b"DNAM"),
                data: [
                    retained_one.as_slice(),
                    removed.as_slice(),
                    retained_two.as_slice(),
                ]
                .concat(),
            }],
        };

        let output = SemanticHandlerRegistry::builtin().invoke_with_records(
            &binding,
            source,
            HandlerInvocationAccess::writable_subrecord_with_scope(&record, 0, None),
            HandlerPhase::AfterLoad,
            None,
            None,
        )?;

        assert!(matches!(
            output,
            HandlerOutput::SubrecordPayload(data)
                if data == [retained_one.as_slice(), retained_two.as_slice()].concat()
        ));
        Ok(())
    }

    /// Clears only the two Creation Kit noise bits from Skyrim weapon data.
    #[test]
    fn skyrim_weapon_after_load_clears_iron_sights_flags() -> Result<()> {
        let binding = CallbackBinding {
            path: "WEAP".to_owned(),
            callback_id: "def.after_load".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-skyrim-weapon-after-load".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "migrate.skyrim_weapon_flags".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({
                        "anchor_path_suffix": "/29:Data",
                    }),
                },
            },
        };
        let mut data = (0_u8..100).collect::<Vec<_>>();
        data[12..14].copy_from_slice(&0x00c1_u16.to_le_bytes());
        data[40..44].copy_from_slice(&0x1234_0181_u32.to_le_bytes());
        let record = WritableRecord {
            signature: Signature(*b"WEAP"),
            flags: RecordFlags::empty(),
            form_id: FormId(0x1111),
            form_version: 0,
            subrecords: vec![bethkit_core::WritableSubRecord {
                signature: Signature(*b"DNAM"),
                data,
            }],
        };

        let output = SemanticHandlerRegistry::builtin().invoke_with_records(
            &binding,
            HandlerRecordContext::new(Signature(*b"WEAP"), FormId(0x1111), 0, SchemaGame::SkyrimSe),
            HandlerInvocationAccess::writable_subrecord_with_scope(&record, 0, None),
            HandlerPhase::AfterLoad,
            None,
            None,
        )?;

        let HandlerOutput::SubrecordPayload(normalized) = output else {
            return Err(SemanticError::Handler {
                handler: "migrate.skyrim_weapon_flags".to_owned(),
                message: "Skyrim weapon cleanup did not return a payload".to_owned(),
            });
        };
        let mut expected = record.subrecords[0].data.clone();
        expected[12..14].copy_from_slice(&0x0081_u16.to_le_bytes());
        expected[40..44].copy_from_slice(&0x1234_0081_u32.to_le_bytes());
        assert_eq!(normalized, expected);
        Ok(())
    }

    /// Restores xEdit's DATA and FNAM defaults for modern light records.
    #[test]
    fn light_after_load_normalizes_data_and_inserts_fade() -> Result<()> {
        assert!(extended_same_value_zero(1.0e-17_f32));
        assert!(!extended_same_value_zero(1.0e-15_f32));
        assert!(!extended_same_value_zero(f32::NAN));
        let binding = CallbackBinding {
            path: "LIGH".to_owned(),
            callback_id: "def.after_load".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-light-after-load".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "migrate.light_defaults".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({
                        "data_path": "LIGH/7:DATA",
                        "fade_path": "LIGH/8:Fade value",
                    }),
                },
            },
        };
        let mut data = (0_u8..48).collect::<Vec<_>>();
        data[16..20].copy_from_slice(&0.0_f32.to_le_bytes());
        data[20..24].copy_from_slice(&(-0.0_f32).to_le_bytes());
        let record = WritableRecord {
            signature: Signature(*b"LIGH"),
            flags: RecordFlags::empty(),
            form_id: FormId(0x1111),
            form_version: 0,
            subrecords: vec![bethkit_core::WritableSubRecord {
                signature: Signature(*b"DATA"),
                data: data.clone(),
            }],
        };

        let output = SemanticHandlerRegistry::builtin().invoke_with_writable_record(
            &binding,
            HandlerRecordContext::new(Signature(*b"LIGH"), FormId(0x1111), 0, SchemaGame::SkyrimSe),
            &record,
            HandlerPhase::AfterLoad,
            None,
            None,
        )?;

        let mut normalized = data;
        normalized[16..20].copy_from_slice(&1.0_f32.to_le_bytes());
        normalized[20..24].copy_from_slice(&90.0_f32.to_le_bytes());
        assert!(matches!(
            output,
            HandlerOutput::Mutations(mutations)
                if mutations
                    == [
                        HandlerMutation::ReplacePayload {
                            path: "LIGH/7:DATA".to_owned(),
                            occurrence: 0,
                            data: normalized,
                        },
                        HandlerMutation::Insert {
                            path: "LIGH/8:Fade value".to_owned(),
                            value: OwnedFieldValue::Float(1.0),
                        },
                    ]
        ));
        Ok(())
    }

    /// Leaves FO76's stale FNAM branch inert and preserves its unknown DATA tail.
    #[test]
    fn light_after_load_preserves_fo76_tail_without_fnam() -> Result<()> {
        let binding = CallbackBinding {
            path: "LIGH".to_owned(),
            callback_id: "def.after_load".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-fo76-light-after-load".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "migrate.light_defaults".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({
                        "data_path": "LIGH/14:DATA",
                    }),
                },
            },
        };
        let mut data = (0_u8..72).collect::<Vec<_>>();
        data[16..20].copy_from_slice(&0.0_f32.to_le_bytes());
        data[20..24].copy_from_slice(&45.0_f32.to_le_bytes());
        let record = WritableRecord {
            signature: Signature(*b"LIGH"),
            flags: RecordFlags::empty(),
            form_id: FormId(0x1111),
            form_version: 0,
            subrecords: vec![bethkit_core::WritableSubRecord {
                signature: Signature(*b"DATA"),
                data: data.clone(),
            }],
        };

        let output = SemanticHandlerRegistry::builtin().invoke_with_writable_record(
            &binding,
            HandlerRecordContext::new(
                Signature(*b"LIGH"),
                FormId(0x1111),
                0,
                SchemaGame::Fallout76,
            ),
            &record,
            HandlerPhase::AfterLoad,
            None,
            None,
        )?;

        let mut normalized = data.clone();
        normalized[16..20].copy_from_slice(&1.0_f32.to_le_bytes());
        assert!(matches!(
            output,
            HandlerOutput::Mutations(mutations)
                if mutations
                    == [HandlerMutation::ReplacePayload {
                        path: "LIGH/14:DATA".to_owned(),
                        occurrence: 0,
                        data: normalized,
                    }]
        ));
        assert_eq!(&data[64..], &(64_u8..72).collect::<Vec<_>>());
        Ok(())
    }

    /// Expands legacy CELL flags and normalizes Skyrim water-height sentinel values.
    #[test]
    fn skyrim_cell_after_load_matches_xedit_water_migration() -> Result<()> {
        let binding = CallbackBinding {
            path: "CELL".to_owned(),
            callback_id: "def.after_load".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-skyrim-cell-after-load".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "migrate.skyrim_cell_after_load".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({
                        "data_path": "CELL/2:Flags",
                        "water_height_path": "CELL/9:Water Height",
                    }),
                },
            },
        };
        let record = |flags, subrecords: Vec<([u8; 4], Vec<u8>)>| WritableRecord {
            signature: Signature(*b"CELL"),
            flags,
            form_id: FormId(0x1111),
            form_version: 0,
            subrecords: subrecords
                .into_iter()
                .map(|(signature, data)| bethkit_core::WritableSubRecord {
                    signature: Signature(signature),
                    data,
                })
                .collect(),
        };
        let invoke = |record: &WritableRecord| {
            SemanticHandlerRegistry::builtin().invoke_with_writable_record(
                &binding,
                HandlerRecordContext::new(
                    Signature(*b"CELL"),
                    FormId(0x1111),
                    0,
                    SchemaGame::SkyrimSe,
                ),
                record,
                HandlerPhase::AfterLoad,
                None,
                None,
            )
        };

        // when / then
        assert!(matches!(
            invoke(&record(RecordFlags::empty(), vec![(*b"DATA", vec![0x02])]))?,
            HandlerOutput::Mutations(mutations)
                if mutations == [
                    HandlerMutation::ReplacePayload {
                        path: "CELL/2:Flags".to_owned(),
                        occurrence: 0,
                        data: vec![0x02, 0],
                    },
                    HandlerMutation::Insert {
                        path: "CELL/9:Water Height".to_owned(),
                        value: OwnedFieldValue::Float(f64::from(f32::MAX)),
                    },
                ]
        ));
        assert!(matches!(
            invoke(&record(
                RecordFlags::empty(),
                vec![
                    (*b"DATA", vec![0, 0]),
                    (
                        *b"XCLW",
                        f32::from_bits(0xff7f_ffff).to_le_bytes().to_vec()
                    ),
                ]
            ))?,
            HandlerOutput::Mutations(mutations)
                if mutations == [HandlerMutation::ReplacePayload {
                    path: "CELL/9:Water Height".to_owned(),
                    occurrence: 0,
                    data: 0.0_f32.to_le_bytes().to_vec(),
                }]
        ));
        assert!(matches!(
            invoke(&record(
                RecordFlags::empty(),
                vec![
                    (*b"DATA", vec![0, 0]),
                    (*b"XCLW", 42.0_f32.to_le_bytes().to_vec()),
                ]
            ))?,
            HandlerOutput::None
        ));
        assert!(matches!(
            invoke(&record(RecordFlags::DELETED, vec![(*b"DATA", vec![0x02])]))?,
            HandlerOutput::None
        ));
        Ok(())
    }

    /// Adds Fallout CELL water defaults only when the record's water flag is set.
    #[test]
    fn fallout_cell_after_load_matches_xedit_water_defaults() -> Result<()> {
        let binding = CallbackBinding {
            path: "CELL".to_owned(),
            callback_id: "def.after_load".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-fallout-cell-after-load".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "migrate.fallout_cell_after_load".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({
                        "data_path": "CELL/2:Flags",
                        "water_height_path": "CELL/7:Water Height",
                        "water_noise_path": "CELL/8:Water Noise Texture",
                    }),
                },
            },
        };
        let record = |flags, subrecords: Vec<([u8; 4], Vec<u8>)>| WritableRecord {
            signature: Signature(*b"CELL"),
            flags,
            form_id: FormId(0x1111),
            form_version: 0,
            subrecords: subrecords
                .into_iter()
                .map(|(signature, data)| bethkit_core::WritableSubRecord {
                    signature: Signature(signature),
                    data,
                })
                .collect(),
        };
        let invoke = |game, record: &WritableRecord| {
            SemanticHandlerRegistry::builtin().invoke_with_writable_record(
                &binding,
                HandlerRecordContext::new(Signature(*b"CELL"), FormId(0x1111), 0, game),
                record,
                HandlerPhase::AfterLoad,
                None,
                None,
            )
        };

        assert!(matches!(
            invoke(
                SchemaGame::Fallout3,
                &record(RecordFlags::empty(), vec![(*b"DATA", vec![0x02])])
            )?,
            HandlerOutput::Mutations(mutations)
                if mutations == [
                    HandlerMutation::Insert {
                        path: "CELL/7:Water Height".to_owned(),
                        value: OwnedFieldValue::Float(f64::from(f32::MAX)),
                    },
                    HandlerMutation::Insert {
                        path: "CELL/8:Water Noise Texture".to_owned(),
                        value: OwnedFieldValue::String(String::new()),
                    },
                ]
        ));
        assert!(matches!(
            invoke(
                SchemaGame::FalloutNv,
                &record(
                    RecordFlags::empty(),
                    vec![
                        (*b"DATA", vec![0x02]),
                        (*b"XCLW", 7.5_f32.to_le_bytes().to_vec()),
                    ]
                )
            )?,
            HandlerOutput::Mutations(mutations)
                if mutations == [HandlerMutation::Insert {
                    path: "CELL/8:Water Noise Texture".to_owned(),
                    value: OwnedFieldValue::String(String::new()),
                }]
        ));
        assert!(matches!(
            invoke(
                SchemaGame::Fallout3,
                &record(RecordFlags::empty(), vec![(*b"DATA", vec![0x01])])
            )?,
            HandlerOutput::None
        ));
        assert!(matches!(
            invoke(
                SchemaGame::FalloutNv,
                &record(RecordFlags::DELETED, vec![(*b"DATA", vec![0x02])])
            )?,
            HandlerOutput::None
        ));
        Ok(())
    }

    /// Adds Oblivion CELL structural defaults and exterior group metadata.
    #[test]
    fn oblivion_cell_after_load_matches_xedit_structure() -> Result<()> {
        let binding = CallbackBinding {
            path: "CELL".to_owned(),
            callback_id: "def.after_load".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-oblivion-cell-after-load".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "migrate.oblivion_cell_after_load".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({
                        "data_path": "CELL/2:Flags",
                        "grid_path": "CELL/3:Grid",
                        "lighting_path": "CELL/4:Lighting",
                    }),
                },
            },
        };
        let record = |flags, subrecords: Vec<([u8; 4], Vec<u8>)>| WritableRecord {
            signature: Signature(*b"CELL"),
            flags,
            form_id: FormId(0x1111),
            form_version: 0,
            subrecords: subrecords
                .into_iter()
                .map(|(signature, data)| bethkit_core::WritableSubRecord {
                    signature: Signature(signature),
                    data,
                })
                .collect(),
        };
        let mut registry = SemanticHandlerRegistry::builtin();
        registry.set_form_link_resolver(Arc::new(TestFormLinkResolver));
        let invoke = |record: &WritableRecord| {
            registry.invoke_with_writable_record(
                &binding,
                HandlerRecordContext::new(
                    Signature(*b"CELL"),
                    FormId(0x1111),
                    0,
                    SchemaGame::Oblivion,
                ),
                record,
                HandlerPhase::AfterLoad,
                None,
                None,
            )
        };

        assert!(matches!(
            invoke(&record(
                RecordFlags::empty(),
                vec![(*b"DATA", vec![0x01, 0xaa])]
            ))?,
            HandlerOutput::Mutations(mutations)
                if mutations == [HandlerMutation::InsertPayload {
                    path: "CELL/4:Lighting".to_owned(),
                    data: vec![0; 36],
                }]
        ));
        assert!(matches!(
            invoke(&record(
                RecordFlags::empty(),
                vec![(*b"DATA", vec![0x40, 0xaa])]
            ))?,
            HandlerOutput::Mutations(mutations)
                if mutations == [
                    HandlerMutation::InsertPayload {
                        path: "CELL/3:Grid".to_owned(),
                        data: vec![0; 8],
                    },
                    HandlerMutation::ReplacePayload {
                        path: "CELL/2:Flags".to_owned(),
                        occurrence: 0,
                        data: vec![0x42, 0xaa],
                    },
                ]
        ));
        assert!(matches!(
            invoke(&record(
                RecordFlags::empty(),
                vec![(*b"DATA", vec![0x42]), (*b"XCLC", vec![0; 8]),]
            ))?,
            HandlerOutput::None
        ));
        assert!(matches!(
            invoke(&record(RecordFlags::DELETED, vec![(*b"DATA", vec![0x01])]))?,
            HandlerOutput::None
        ));
        Ok(())
    }

    /// Repairs Oblivion PGRD auxiliary data, compression, and trailing sentinels.
    #[test]
    fn oblivion_path_grid_after_load_matches_xedit_repairs() -> Result<()> {
        let binding = CallbackBinding {
            path: "PGRD".to_owned(),
            callback_id: "def.after_load".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-oblivion-pgrd-after-load".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "migrate.oblivion_path_grid_after_load".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({
                        "points_path": "PGRD/1:Points",
                        "auxiliary_path": "PGRD/2:Unknown",
                        "connections_path": "PGRD/3:Point-to-Point Connections",
                        "point_size": 16,
                        "connection_count_offset": 12,
                    }),
                },
            },
        };
        let mut points = (0_u8..32).collect::<Vec<_>>();
        points[12] = 3;
        points[28] = 2;
        let connections = [1_i16, -1, -1, 2, -1]
            .into_iter()
            .flat_map(i16::to_le_bytes)
            .collect::<Vec<_>>();
        let record = WritableRecord {
            signature: Signature(*b"PGRD"),
            flags: RecordFlags::empty(),
            form_id: FormId(0x1111),
            form_version: 0,
            subrecords: vec![
                bethkit_core::WritableSubRecord {
                    signature: Signature(*b"PGRP"),
                    data: points.clone(),
                },
                bethkit_core::WritableSubRecord {
                    signature: Signature(*b"PGRR"),
                    data: connections,
                },
            ],
        };

        let output = SemanticHandlerRegistry::builtin().invoke_with_writable_record(
            &binding,
            HandlerRecordContext::new(Signature(*b"PGRD"), FormId(0x1111), 0, SchemaGame::Oblivion),
            &record,
            HandlerPhase::AfterLoad,
            None,
            None,
        )?;

        points[12] = 1;
        points[28] = 1;
        assert!(matches!(
            output,
            HandlerOutput::Mutations(mutations)
                if mutations == [
                    HandlerMutation::InsertPayload {
                        path: "PGRD/2:Unknown".to_owned(),
                        data: vec![0],
                    },
                    HandlerMutation::SetRecordFlags {
                        path: "PGRD".to_owned(),
                        flags: RecordFlags::COMPRESSED,
                    },
                    HandlerMutation::ReplacePayload {
                        path: "PGRD/1:Points".to_owned(),
                        occurrence: 0,
                        data: points,
                    },
                    HandlerMutation::ReplacePayload {
                        path: "PGRD/3:Point-to-Point Connections".to_owned(),
                        occurrence: 0,
                        data: [1_i16, 2]
                            .into_iter()
                            .flat_map(i16::to_le_bytes)
                            .collect(),
                    },
                ]
        ));
        Ok(())
    }

    /// Keeps the last Oblivion PGRI entry for each xEdit structural sort key.
    #[test]
    fn oblivion_inter_cell_after_load_matches_xedit_deduplication() -> Result<()> {
        let binding = CallbackBinding {
            path: "PGRD/4:Inter-Cell Connections/payload".to_owned(),
            callback_id: "def.after_load".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-oblivion-pgri-after-load".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "migrate.oblivion_inter_cell_connections_after_load".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({
                        "entry_size": 16,
                        "point_offset": 0,
                        "x_offset": 4,
                        "y_offset": 8,
                        "z_offset": 12,
                    }),
                },
            },
        };
        let entry = |point: u16, unused: [u8; 2], x: f32, y: f32, z: f32| {
            [
                point.to_le_bytes().as_slice(),
                unused.as_slice(),
                x.to_le_bytes().as_slice(),
                y.to_le_bytes().as_slice(),
                z.to_le_bytes().as_slice(),
            ]
            .concat()
        };
        let first = entry(7, [0xaa, 0xbb], 1.0, -0.0, 3.0);
        let unique = entry(8, [0xcc, 0xdd], 2.0, 4.0, 6.0);
        let retained = entry(7, [0x11, 0x22], 1.0, 0.0, 3.0);
        let record = WritableRecord {
            signature: Signature(*b"PGRD"),
            flags: RecordFlags::empty(),
            form_id: FormId(0x1111),
            form_version: 0,
            subrecords: vec![bethkit_core::WritableSubRecord {
                signature: Signature(*b"PGRI"),
                data: [first, unique.clone(), retained.clone()].concat(),
            }],
        };

        let output = SemanticHandlerRegistry::builtin().invoke_with_records(
            &binding,
            HandlerRecordContext::new(Signature(*b"PGRD"), FormId(0x1111), 0, SchemaGame::Oblivion),
            HandlerInvocationAccess::writable_subrecord_with_scope(&record, 0, None),
            HandlerPhase::AfterLoad,
            None,
            None,
        )?;

        assert!(matches!(
            output,
            HandlerOutput::SubrecordPayload(data)
                if data == [unique, retained].concat()
        ));
        Ok(())
    }

    /// Scales only legacy EFSH particle birth ratios in xEdit's migration range.
    #[test]
    fn legacy_effect_shader_after_load_matches_xedit_ratio_migration() -> Result<()> {
        let binding = CallbackBinding {
            path: "EFSH".to_owned(),
            callback_id: "def.after_load".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-legacy-efsh-after-load".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "migrate.legacy_effect_shader_birth_ratios".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({
                        "data_path": "EFSH/4:DATA",
                    }),
                },
            },
        };
        let record = |payload: Vec<u8>| WritableRecord {
            signature: Signature(*b"EFSH"),
            flags: RecordFlags::empty(),
            form_id: FormId(0x1111),
            form_version: 0,
            subrecords: vec![bethkit_core::WritableSubRecord {
                signature: Signature(*b"DATA"),
                data: payload,
            }],
        };
        let invoke = |game, record: &WritableRecord| {
            SemanticHandlerRegistry::builtin().invoke_with_writable_record(
                &binding,
                HandlerRecordContext::new(Signature(*b"EFSH"), FormId(0x1111), 0, game),
                record,
                HandlerPhase::AfterLoad,
                None,
                None,
            )
        };
        let mut payload: Vec<u8> = (0_u16..140).map(|value| value as u8).collect();
        payload[124..128].copy_from_slice(&0.5_f32.to_le_bytes());
        payload[128..132].copy_from_slice(&(-0.25_f32).to_le_bytes());

        let output = invoke(SchemaGame::Fallout3, &record(payload.clone()))?;

        let mut expected = payload;
        expected[124..128].copy_from_slice(&39.0_f32.to_le_bytes());
        expected[128..132].copy_from_slice(&(-19.5_f32).to_le_bytes());
        assert!(matches!(
            output,
            HandlerOutput::Mutations(mutations)
                if mutations == [HandlerMutation::ReplacePayload {
                    path: "EFSH/4:DATA".to_owned(),
                    occurrence: 0,
                    data: expected,
                }]
        ));

        let mut unchanged = vec![0_u8; 140];
        unchanged[124..128].copy_from_slice(&0.0_f32.to_le_bytes());
        unchanged[128..132].copy_from_slice(&f32::NAN.to_le_bytes());
        assert!(matches!(
            invoke(SchemaGame::FalloutNv, &record(unchanged))?,
            HandlerOutput::None
        ));
        assert!(matches!(
            invoke(SchemaGame::Fallout3, &record(vec![0_u8; 128]))?,
            HandlerOutput::None
        ));
        Ok(())
    }

    /// Removes only the first legacy FACT CNAM from valid non-deleted records.
    #[test]
    fn legacy_faction_after_load_matches_xedit_unused_cleanup() -> Result<()> {
        let binding = CallbackBinding {
            path: "FACT".to_owned(),
            callback_id: "def.after_load".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-legacy-fact-after-load".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "migrate.legacy_faction_after_load".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({
                        "unused_path": "FACT/4:Unused",
                    }),
                },
            },
        };
        let record = |flags, subrecords: Vec<([u8; 4], Vec<u8>)>| WritableRecord {
            signature: Signature(*b"FACT"),
            flags,
            form_id: FormId(0x1111),
            form_version: 0,
            subrecords: subrecords
                .into_iter()
                .map(|(signature, data)| bethkit_core::WritableSubRecord {
                    signature: Signature(signature),
                    data,
                })
                .collect(),
        };
        let invoke = |game, record: &WritableRecord| {
            SemanticHandlerRegistry::builtin().invoke_with_writable_record(
                &binding,
                HandlerRecordContext::new(Signature(*b"FACT"), FormId(0x1111), 0, game),
                record,
                HandlerPhase::AfterLoad,
                None,
                None,
            )
        };

        assert!(matches!(
            invoke(
                SchemaGame::Fallout3,
                &record(
                    RecordFlags::empty(),
                    vec![
                        (*b"EDID", b"Faction\0".to_vec()),
                        (*b"CNAM", 1.0_f32.to_le_bytes().to_vec()),
                        (*b"CNAM", 2.0_f32.to_le_bytes().to_vec()),
                    ],
                ),
            )?,
            HandlerOutput::Mutations(mutations)
                if mutations == [HandlerMutation::RemoveFirstBySignature {
                    path: "FACT".to_owned(),
                    signature: Signature(*b"CNAM"),
                }]
        ));
        assert!(matches!(
            invoke(
                SchemaGame::FalloutNv,
                &record(
                    RecordFlags::empty(),
                    vec![(*b"EDID", b"Faction\0".to_vec())],
                ),
            )?,
            HandlerOutput::None
        ));
        assert!(matches!(
            invoke(
                SchemaGame::FalloutNv,
                &record(
                    RecordFlags::DELETED,
                    vec![(*b"CNAM", 1.0_f32.to_le_bytes().to_vec())],
                ),
            )?,
            HandlerOutput::None
        ));
        Ok(())
    }

    /// Converts legacy WATR visual DATA into DNAM while preserving copied bytes.
    #[test]
    fn legacy_water_after_load_matches_xedit_visual_migration() -> Result<()> {
        let binding = CallbackBinding {
            path: "WATR".to_owned(),
            callback_id: "def.after_load".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-legacy-watr-after-load".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "migrate.legacy_water_after_load".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({
                        "damage_path": "WATR/8:Damage",
                        "new_visual_path": "WATR/9:Visual Data/0:Visual Data",
                        "old_visual_path": "WATR/9:Visual Data/1:Visual Data",
                    }),
                },
            },
        };
        let record = |subrecords: Vec<([u8; 4], Vec<u8>)>| WritableRecord {
            signature: Signature(*b"WATR"),
            flags: RecordFlags::empty(),
            form_id: FormId(0x1111),
            form_version: 0,
            subrecords: subrecords
                .into_iter()
                .map(|(signature, data)| bethkit_core::WritableSubRecord {
                    signature: Signature(signature),
                    data,
                })
                .collect(),
        };
        let invoke = |game, record: &WritableRecord| {
            SemanticHandlerRegistry::builtin().invoke_with_writable_record(
                &binding,
                HandlerRecordContext::new(Signature(*b"WATR"), FormId(0x1111), 0, game),
                record,
                HandlerPhase::AfterLoad,
                None,
                None,
            )
        };
        let mut old_visual: Vec<u8> = (0_u16..186).map(|value| value as u8).collect();
        old_visual[184..186].copy_from_slice(&0x1234_u16.to_le_bytes());

        let output = invoke(
            SchemaGame::Fallout3,
            &record(vec![
                (*b"DATA", 0xffff_u16.to_le_bytes().to_vec()),
                (*b"DATA", old_visual.clone()),
            ]),
        )?;

        let mut new_visual = vec![0_u8; 196];
        new_visual[..184].copy_from_slice(&old_visual[..184]);
        new_visual[184..188].copy_from_slice(&1.0_f32.to_le_bytes());
        new_visual[188..192].copy_from_slice(&0.5_f32.to_le_bytes());
        new_visual[192..196].copy_from_slice(&0.25_f32.to_le_bytes());
        assert!(matches!(
            output,
            HandlerOutput::Mutations(mutations)
                if mutations == [
                    HandlerMutation::Remove {
                        path: "WATR/9:Visual Data/1:Visual Data".to_owned(),
                        occurrence: 0,
                    },
                    HandlerMutation::ReplacePayload {
                        path: "WATR/8:Damage".to_owned(),
                        occurrence: 0,
                        data: 0x1234_u16.to_le_bytes().to_vec(),
                    },
                    HandlerMutation::InsertPayload {
                        path: "WATR/9:Visual Data/0:Visual Data".to_owned(),
                        data: new_visual,
                    },
                ]
        ));
        assert!(matches!(
            invoke(
                SchemaGame::FalloutNv,
                &record(vec![
                    (*b"DATA", old_visual.clone()),
                    (*b"DNAM", vec![0_u8; 196]),
                ]),
            )?,
            HandlerOutput::None
        ));
        assert!(matches!(
            invoke(
                SchemaGame::FalloutNv,
                &record(vec![(*b"DATA", vec![0_u8; 185])]),
            )?,
            HandlerOutput::None
        ));
        Ok(())
    }

    /// Removes only the first Oblivion reference XPCI, including on deleted records.
    #[test]
    fn oblivion_reference_after_load_matches_xedit_unused_cleanup() -> Result<()> {
        let binding = |path: &str, unused_path: &str| CallbackBinding {
            path: path.to_owned(),
            callback_id: "def.after_load".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-oblivion-reference-after-load".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "migrate.oblivion_reference_after_load".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({
                        "unused_path": unused_path,
                    }),
                },
            },
        };
        let record = |signature, flags, subrecords: Vec<([u8; 4], Vec<u8>)>| WritableRecord {
            signature: Signature(signature),
            flags,
            form_id: FormId(0x1111),
            form_version: 0,
            subrecords: subrecords
                .into_iter()
                .map(|(signature, data)| bethkit_core::WritableSubRecord {
                    signature: Signature(signature),
                    data,
                })
                .collect(),
        };
        let registry = SemanticHandlerRegistry::builtin();
        let invoke = |binding: &CallbackBinding, record: &WritableRecord| {
            registry.invoke_with_writable_record(
                binding,
                HandlerRecordContext::new(
                    record.signature,
                    FormId(0x1111),
                    0,
                    SchemaGame::Oblivion,
                ),
                record,
                HandlerPhase::AfterLoad,
                None,
                None,
            )
        };

        let achr = binding("ACHR", "ACHR/2:Unused/0:Unused");
        assert!(matches!(
            invoke(
                &achr,
                &record(
                    *b"ACHR",
                    RecordFlags::DELETED,
                    vec![
                        (*b"XPCI", vec![1, 0, 0, 0]),
                        (*b"XPCI", vec![2, 0, 0, 0]),
                    ],
                ),
            )?,
            HandlerOutput::Mutations(mutations)
                if mutations == [HandlerMutation::RemoveFirstBySignature {
                    path: "ACHR".to_owned(),
                    signature: Signature(*b"XPCI"),
                }]
        ));
        let refr = binding("REFR", "REFR/11:Unused/0:Unused");
        assert!(matches!(
            invoke(
                &refr,
                &record(
                    *b"REFR",
                    RecordFlags::empty(),
                    vec![(*b"EDID", b"Reference\0".to_vec())],
                ),
            )?,
            HandlerOutput::None
        ));
        Ok(())
    }

    /// Migrates Oblivion leveled-list chance flags and removes one legacy DATA.
    #[test]
    fn oblivion_leveled_list_after_load_matches_xedit_migration() -> Result<()> {
        let binding = |root: &str| CallbackBinding {
            path: root.to_owned(),
            callback_id: "def.after_load".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-oblivion-lvl-after-load".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "migrate.oblivion_leveled_list_after_load".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({
                        "chance_path": format!("{root}/1:Chance none"),
                        "flags_path": format!("{root}/2:Flags"),
                    }),
                },
            },
        };
        let record = |signature, flags, subrecords: Vec<([u8; 4], Vec<u8>)>| WritableRecord {
            signature: Signature(signature),
            flags,
            form_id: FormId(0x1111),
            form_version: 0,
            subrecords: subrecords
                .into_iter()
                .map(|(signature, data)| bethkit_core::WritableSubRecord {
                    signature: Signature(signature),
                    data,
                })
                .collect(),
        };
        let registry = SemanticHandlerRegistry::builtin();
        let invoke = |binding: &CallbackBinding, record: &WritableRecord| {
            registry.invoke_with_writable_record(
                binding,
                HandlerRecordContext::new(
                    record.signature,
                    FormId(0x1111),
                    0,
                    SchemaGame::Oblivion,
                ),
                record,
                HandlerPhase::AfterLoad,
                None,
                None,
            )
        };

        let lvli = binding("LVLI");
        assert!(matches!(
            invoke(
                &lvli,
                &record(
                    *b"LVLI",
                    RecordFlags::empty(),
                    vec![
                        (*b"LVLD", vec![0x92, 0xaa]),
                        (*b"LVLF", vec![0x02, 0xbb]),
                        (*b"DATA", vec![0xcc]),
                        (*b"DATA", vec![0xdd]),
                    ],
                ),
            )?,
            HandlerOutput::Mutations(mutations)
                if mutations == [
                    HandlerMutation::RemoveFirstBySignature {
                        path: "LVLI".to_owned(),
                        signature: Signature(*b"DATA"),
                    },
                    HandlerMutation::ReplacePayload {
                        path: "LVLI/1:Chance none".to_owned(),
                        occurrence: 0,
                        data: vec![0x12, 0xaa],
                    },
                    HandlerMutation::ReplacePayload {
                        path: "LVLI/2:Flags".to_owned(),
                        occurrence: 0,
                        data: vec![0x03, 0xbb],
                    },
                ]
        ));
        let lvlc = binding("LVLC");
        assert!(matches!(
            invoke(
                &lvlc,
                &record(
                    *b"LVLC",
                    RecordFlags::empty(),
                    vec![(*b"LVLD", vec![0x80])],
                ),
            )?,
            HandlerOutput::Mutations(mutations)
                if mutations == [
                    HandlerMutation::ReplacePayload {
                        path: "LVLC/1:Chance none".to_owned(),
                        occurrence: 0,
                        data: vec![0],
                    },
                    HandlerMutation::InsertPayload {
                        path: "LVLC/2:Flags".to_owned(),
                        data: vec![1],
                    },
                ]
        ));
        assert!(matches!(
            invoke(
                &binding("LVSP"),
                &record(*b"LVSP", RecordFlags::DELETED, vec![(*b"LVLD", vec![0x80])],),
            )?,
            HandlerOutput::None
        ));
        Ok(())
    }

    /// Clamps legacy NPC NAM5 to 255 while preserving malformed trailing bytes.
    #[test]
    fn legacy_npc_after_load_matches_xedit_clamp() -> Result<()> {
        let binding = CallbackBinding {
            path: "NPC_".to_owned(),
            callback_id: "def.after_load".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-legacy-npc-after-load".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "migrate.legacy_npc_after_load".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({
                        "value_path": "NPC_/30:Unknown",
                    }),
                },
            },
        };
        let record = |flags, data: Vec<u8>| WritableRecord {
            signature: Signature(*b"NPC_"),
            flags,
            form_id: FormId(0x1111),
            form_version: 0,
            subrecords: vec![bethkit_core::WritableSubRecord {
                signature: Signature(*b"NAM5"),
                data,
            }],
        };
        let registry = SemanticHandlerRegistry::builtin();
        let invoke = |game, record: &WritableRecord| {
            registry.invoke_with_writable_record(
                &binding,
                HandlerRecordContext::new(Signature(*b"NPC_"), FormId(0x1111), 0, game),
                record,
                HandlerPhase::AfterLoad,
                None,
                None,
            )
        };

        assert!(matches!(
            invoke(
                SchemaGame::Fallout3,
                &record(RecordFlags::empty(), vec![0x34, 0x12, 0xaa]),
            )?,
            HandlerOutput::Mutations(mutations)
                if mutations == [HandlerMutation::ReplacePayload {
                    path: "NPC_/30:Unknown".to_owned(),
                    occurrence: 0,
                    data: vec![0xff, 0x00, 0xaa],
                }]
        ));
        assert!(matches!(
            invoke(
                SchemaGame::FalloutNv,
                &record(RecordFlags::empty(), 255_u16.to_le_bytes().to_vec()),
            )?,
            HandlerOutput::None
        ));
        assert!(matches!(
            invoke(
                SchemaGame::FalloutNv,
                &record(RecordFlags::DELETED, 256_u16.to_le_bytes().to_vec()),
            )?,
            HandlerOutput::None
        ));
        Ok(())
    }

    /// Cleans legacy INFO fields and normalizes Persuasion to Topic.
    #[test]
    fn legacy_info_after_load_matches_xedit_cleanup() -> Result<()> {
        let binding = |game| {
            let (unused_sound_path, speech_challenge_path) = match game {
                SchemaGame::Fallout3 => ("INFO/11:Unused", "INFO/15:Speech Challenge"),
                SchemaGame::FalloutNv => ("INFO/12:Unused", "INFO/16:Speech Challenge"),
                _ => panic!("test only supports legacy Fallout games"),
            };
            CallbackBinding {
                path: "INFO".to_owned(),
                callback_id: "def.after_load".to_owned(),
                callback_slot: None,
                implementation_fingerprint: "test-legacy-info-after-load".to_owned(),
                implementation: CallbackImplementation::BuiltIn {
                    operation: bethkit_schema::BuiltInOperation {
                        id: "migrate.legacy_info_after_load".to_owned(),
                        minimum_version: 1,
                        configuration: serde_json::json!({
                            "data_path": "INFO/0:DATA",
                            "unused_sound_path": unused_sound_path,
                            "speech_challenge_path": speech_challenge_path,
                        }),
                    },
                },
            }
        };
        let record = |flags, subrecords| WritableRecord {
            signature: Signature(*b"INFO"),
            flags,
            form_id: FormId(0x1111),
            form_version: 0,
            subrecords,
        };
        let subrecord = |signature, data| bethkit_core::WritableSubRecord {
            signature: Signature(signature),
            data,
        };
        let registry = SemanticHandlerRegistry::builtin();
        let invoke = |game, binding: &CallbackBinding, record: &WritableRecord| {
            registry.invoke_with_writable_record(
                binding,
                HandlerRecordContext::new(Signature(*b"INFO"), FormId(0x1111), 0, game),
                record,
                HandlerPhase::AfterLoad,
                None,
                None,
            )
        };

        let fallout_3_binding = binding(SchemaGame::Fallout3);
        let fallout_3 = record(
            RecordFlags::empty(),
            vec![
                subrecord(*b"DATA", vec![3, 9, 0, 7]),
                subrecord(*b"DNAM", vec![1]),
                subrecord(*b"DNAM", vec![2]),
                subrecord(*b"SNDD", vec![3]),
                subrecord(*b"SNDD", vec![4]),
            ],
        );
        assert!(matches!(
            invoke(SchemaGame::Fallout3, &fallout_3_binding, &fallout_3)?,
            HandlerOutput::Mutations(mutations)
                if mutations == [
                    HandlerMutation::RemoveFirstBySignature {
                        path: "INFO".to_owned(),
                        signature: Signature(*b"DNAM"),
                    },
                    HandlerMutation::RemoveFirstBySignature {
                        path: "INFO".to_owned(),
                        signature: Signature(*b"SNDD"),
                    },
                    HandlerMutation::ReplacePayload {
                        path: "INFO/0:DATA".to_owned(),
                        occurrence: 0,
                        data: vec![0, 9, 0, 7],
                    },
                ]
        ));

        let fallout_nv_binding = binding(SchemaGame::FalloutNv);
        let fallout_nv = record(
            RecordFlags::empty(),
            vec![
                subrecord(*b"DATA", vec![3, 9, 0x80, 7]),
                subrecord(*b"DNAM", vec![1]),
                subrecord(*b"SNDD", vec![2]),
            ],
        );
        assert!(matches!(
            invoke(SchemaGame::FalloutNv, &fallout_nv_binding, &fallout_nv)?,
            HandlerOutput::Mutations(mutations)
                if mutations == [
                    HandlerMutation::RemoveFirstBySignature {
                        path: "INFO".to_owned(),
                        signature: Signature(*b"SNDD"),
                    },
                    HandlerMutation::ReplacePayload {
                        path: "INFO/0:DATA".to_owned(),
                        occurrence: 0,
                        data: vec![0, 9, 0x80, 7],
                    },
                ]
        ));

        let deleted = record(
            RecordFlags::DELETED,
            vec![subrecord(*b"DATA", vec![3, 0, 0, 0])],
        );
        assert!(matches!(
            invoke(SchemaGame::FalloutNv, &fallout_nv_binding, &deleted)?,
            HandlerOutput::None
        ));
        Ok(())
    }

    /// Migrates legacy SOUN fields into xEdit's complete SNDD layout.
    #[test]
    fn legacy_sound_after_load_matches_xedit_migration() -> Result<()> {
        let binding = |game| {
            let base = if game == SchemaGame::Fallout3 { 3 } else { 4 };
            CallbackBinding {
                path: "SOUN".to_owned(),
                callback_id: "def.after_load".to_owned(),
                callback_slot: None,
                implementation_fingerprint: "test-legacy-sound-after-load".to_owned(),
                implementation: CallbackImplementation::BuiltIn {
                    operation: bethkit_schema::BuiltInOperation {
                        id: "migrate.legacy_sound_after_load".to_owned(),
                        minimum_version: 1,
                        configuration: serde_json::json!({
                            "new_data_path": format!(
                                "SOUN/{base}:Sound Data/0:Sound Data"
                            ),
                            "old_data_path": format!(
                                "SOUN/{base}:Sound Data/1:Sound Data"
                            ),
                            "curve_path": format!(
                                "SOUN/{}:Attenuation Curve",
                                base + 1
                            ),
                            "reverb_path": format!(
                                "SOUN/{}:Reverb Attenuation Control",
                                base + 2
                            ),
                            "priority_path": format!("SOUN/{}:Priority", base + 3),
                        }),
                    },
                },
            }
        };
        let subrecord = |signature, data| bethkit_core::WritableSubRecord {
            signature: Signature(signature),
            data,
        };
        let record = |flags, subrecords| WritableRecord {
            signature: Signature(*b"SOUN"),
            flags,
            form_id: FormId(0x1111),
            form_version: 0,
            subrecords,
        };
        let registry = SemanticHandlerRegistry::builtin();
        let invoke = |game, record: &WritableRecord| {
            registry.invoke_with_writable_record(
                &binding(game),
                HandlerRecordContext::new(Signature(*b"SOUN"), FormId(0x1111), 0, game),
                record,
                HandlerPhase::AfterLoad,
                None,
                None,
            )
        };
        let old_data = (0_u8..12).collect::<Vec<_>>();
        let curve = [9_i16, 8, 7, 6, 5]
            .into_iter()
            .flat_map(i16::to_le_bytes)
            .collect::<Vec<_>>();
        let migrated = invoke(
            SchemaGame::FalloutNv,
            &record(
                RecordFlags::empty(),
                vec![
                    subrecord(*b"SNDX", old_data.clone()),
                    subrecord(*b"ANAM", curve.clone()),
                    subrecord(*b"GNAM", (-7_i16).to_le_bytes().to_vec()),
                    subrecord(*b"HNAM", (-9_i32).to_le_bytes().to_vec()),
                ],
            ),
        )?;
        let HandlerOutput::Mutations(mutations) = migrated else {
            return Err(SemanticError::Handler {
                handler: "test".to_owned(),
                message: "legacy sound migration did not return mutations".to_owned(),
            });
        };
        assert_eq!(mutations.len(), 5);
        let HandlerMutation::InsertPayload { path, data } = &mutations[4] else {
            return Err(SemanticError::Handler {
                handler: "test".to_owned(),
                message: "legacy sound migration did not insert SNDD last".to_owned(),
            });
        };
        assert_eq!(path, "SOUN/4:Sound Data/0:Sound Data");
        assert_eq!(&data[..12], old_data);
        assert_eq!(&data[12..22], curve);
        assert_eq!(&data[22..24], &(-7_i16).to_le_bytes());
        assert_eq!(&data[24..28], &(-9_i32).to_le_bytes());
        assert_eq!(&data[28..], &[0; 8]);

        let defaults = invoke(
            SchemaGame::Fallout3,
            &record(
                RecordFlags::empty(),
                vec![subrecord(*b"SNDX", old_data.clone())],
            ),
        )?;
        let HandlerOutput::Mutations(default_mutations) = defaults else {
            return Err(SemanticError::Handler {
                handler: "test".to_owned(),
                message: "legacy sound defaults did not return mutations".to_owned(),
            });
        };
        let HandlerMutation::InsertPayload {
            path,
            data: default_data,
        } = &default_mutations[1]
        else {
            return Err(SemanticError::Handler {
                handler: "test".to_owned(),
                message: "legacy sound defaults did not insert SNDD".to_owned(),
            });
        };
        assert_eq!(path, "SOUN/3:Sound Data/0:Sound Data");
        assert_eq!(&default_data[..12], old_data);
        assert_eq!(
            &default_data[12..22],
            [100_i16, 50, 20, 5, 0]
                .into_iter()
                .flat_map(i16::to_le_bytes)
                .collect::<Vec<_>>()
        );
        assert_eq!(&default_data[22..24], &80_i16.to_le_bytes());
        assert_eq!(&default_data[24..28], &128_i32.to_le_bytes());
        assert_eq!(&default_data[28..], &[0; 8]);

        assert!(matches!(
            invoke(
                SchemaGame::Fallout3,
                &record(
                    RecordFlags::empty(),
                    vec![
                        subrecord(*b"SNDX", old_data.clone()),
                        subrecord(*b"SNDD", vec![0; 36]),
                    ],
                ),
            )?,
            HandlerOutput::None
        ));
        assert!(matches!(
            invoke(
                SchemaGame::Fallout3,
                &record(
                    RecordFlags::DELETED,
                    vec![subrecord(*b"SNDX", old_data.clone())],
                ),
            )?,
            HandlerOutput::None
        ));
        assert!(matches!(
            invoke(
                SchemaGame::FalloutNv,
                &record(
                    RecordFlags::empty(),
                    vec![
                        subrecord(*b"SNDX", old_data),
                        subrecord(*b"ANAM", vec![0; 8]),
                    ],
                ),
            ),
            Err(SemanticError::Handler { message, .. })
                if message.contains("10-byte ANAM")
        ));
        Ok(())
    }

    /// Normalizes only zero legacy WEAP multipliers at their materialized offsets.
    #[test]
    fn legacy_weapon_after_load_matches_xedit_defaults() -> Result<()> {
        let binding = |game| {
            let data_path = if game == SchemaGame::Fallout3 {
                "WEAP/31:DNAM"
            } else {
                "WEAP/51:DNAM"
            };
            CallbackBinding {
                path: "WEAP".to_owned(),
                callback_id: "def.after_load".to_owned(),
                callback_slot: None,
                implementation_fingerprint: "test-legacy-weapon-after-load".to_owned(),
                implementation: CallbackImplementation::BuiltIn {
                    operation: bethkit_schema::BuiltInOperation {
                        id: "migrate.legacy_weapon_after_load".to_owned(),
                        minimum_version: 1,
                        configuration: serde_json::json!({
                            "data_path": data_path,
                            "animation_multiplier_path": format!(
                                "{data_path}/payload/1:Animation Multiplier"
                            ),
                            "attack_multiplier_path": format!(
                                "{data_path}/payload/21:Animation Attack Multiplier"
                            ),
                        }),
                    },
                },
            }
        };
        let record = |flags, data| WritableRecord {
            signature: Signature(*b"WEAP"),
            flags,
            form_id: FormId(0x1111),
            form_version: 0,
            subrecords: vec![bethkit_core::WritableSubRecord {
                signature: Signature(*b"DNAM"),
                data,
            }],
        };
        let invoke = |game, record: &WritableRecord| {
            SemanticHandlerRegistry::builtin().invoke_with_writable_record(
                &binding(game),
                HandlerRecordContext::new(Signature(*b"WEAP"), FormId(0x1111), 0, game),
                record,
                HandlerPhase::AfterLoad,
                None,
                None,
            )
        };
        let mut original = (0_u8..140).collect::<Vec<_>>();
        original[4..8].copy_from_slice(&0.0_f32.to_le_bytes());
        original[60..64].copy_from_slice(&(-0.0_f32).to_le_bytes());
        let migrated = invoke(
            SchemaGame::Fallout3,
            &record(RecordFlags::empty(), original.clone()),
        )?;
        let HandlerOutput::Mutations(mutations) = migrated else {
            return Err(SemanticError::Handler {
                handler: "test".to_owned(),
                message: "legacy weapon migration did not return a replacement".to_owned(),
            });
        };
        let [HandlerMutation::ReplacePayload {
            path,
            occurrence,
            data,
        }] = mutations.as_slice()
        else {
            return Err(SemanticError::Handler {
                handler: "test".to_owned(),
                message: "legacy weapon migration returned unexpected mutations".to_owned(),
            });
        };
        assert_eq!(path, "WEAP/31:DNAM");
        assert_eq!(*occurrence, 0);
        assert_eq!(&data[..4], &original[..4]);
        assert_eq!(&data[4..8], &1.0_f32.to_le_bytes());
        assert_eq!(&data[8..60], &original[8..60]);
        assert_eq!(&data[60..64], &1.0_f32.to_le_bytes());
        assert_eq!(&data[64..], &original[64..]);

        let mut ordinary = original;
        ordinary[4..8].copy_from_slice(&2.5_f32.to_le_bytes());
        ordinary[60..64].copy_from_slice(&f32::NAN.to_le_bytes());
        assert!(matches!(
            invoke(
                SchemaGame::FalloutNv,
                &record(RecordFlags::empty(), ordinary),
            )?,
            HandlerOutput::None
        ));

        let mut short = (0_u8..8).collect::<Vec<_>>();
        short[4..8].copy_from_slice(&0.0_f32.to_le_bytes());
        let short_output = invoke(
            SchemaGame::FalloutNv,
            &record(RecordFlags::empty(), short.clone()),
        )?;
        assert!(matches!(
            short_output,
            HandlerOutput::Mutations(mutations)
                if matches!(
                    mutations.as_slice(),
                    [HandlerMutation::ReplacePayload { path, data, .. }]
                        if path == "WEAP/51:DNAM"
                            && data[..4] == short[..4]
                            && data[4..8] == 1.0_f32.to_le_bytes()
                )
        ));
        assert!(matches!(
            invoke(
                SchemaGame::Fallout3,
                &record(RecordFlags::DELETED, vec![0; 140]),
            )?,
            HandlerOutput::None
        ));
        Ok(())
    }

    /// Adds the exact type-specific legacy PACK members created by xEdit.
    #[test]
    fn legacy_package_after_load_matches_xedit_type_defaults() -> Result<()> {
        let binding = CallbackBinding {
            path: "PACK".to_owned(),
            callback_id: "def.after_load".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-legacy-package-after-load".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "migrate.legacy_package_after_load".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({
                        "general_path": "PACK/1:General",
                        "type_path": "PACK/1:General/payload/1:Type",
                        "locations_path": "PACK/2:Locations",
                        "location_path": "PACK/2:Locations/0:Location 1",
                        "location_type_path":
                            "PACK/2:Locations/0:Location 1/payload/0:Type",
                        "target_path": "PACK/4:Target 1",
                        "eat_marker_path": "PACK/8:Eat Marker",
                        "follow_radius_path":
                            "PACK/10:Follow - Start Location - Trigger Radius",
                        "patrol_flags_path": "PACK/11:Patrol Flags",
                    }),
                },
            },
        };
        let subrecord = |signature, data| bethkit_core::WritableSubRecord {
            signature: Signature(signature),
            data,
        };
        let record = |package_type, before_schedule: Vec<_>, after_schedule: Vec<_>| {
            let mut general = vec![0; 12];
            general[4] = package_type;
            let mut subrecords = vec![subrecord(*b"PKDT", general)];
            subrecords.extend(before_schedule);
            subrecords.push(subrecord(*b"PSDT", vec![0; 8]));
            subrecords.extend(after_schedule);
            WritableRecord {
                signature: Signature(*b"PACK"),
                flags: RecordFlags::empty(),
                form_id: FormId(0x1111),
                form_version: 0,
                subrecords,
            }
        };
        let registry = SemanticHandlerRegistry::builtin();
        let invoke = |game, record: &WritableRecord| {
            registry.invoke_with_writable_record(
                &binding,
                HandlerRecordContext::new(Signature(*b"PACK"), FormId(0x1111), 0, game),
                record,
                HandlerPhase::AfterLoad,
                None,
                None,
            )
        };
        let inserts = |output| -> Result<Vec<(String, Vec<u8>)>> {
            let HandlerOutput::Mutations(mutations) = output else {
                return Err(SemanticError::Handler {
                    handler: "test".to_owned(),
                    message: "legacy package migration did not return insertions".to_owned(),
                });
            };
            mutations
                .into_iter()
                .map(|mutation| match mutation {
                    HandlerMutation::InsertPayload { path, data } => Ok((path, data)),
                    _ => Err(SemanticError::Handler {
                        handler: "test".to_owned(),
                        message: "legacy package migration returned a non-insertion".to_owned(),
                    }),
                })
                .collect()
        };

        assert_eq!(
            inserts(invoke(
                SchemaGame::Fallout3,
                &record(0, Vec::new(), Vec::new())
            )?)?,
            vec![("PACK/4:Target 1".to_owned(), vec![0; 16])]
        );
        assert_eq!(
            inserts(invoke(
                SchemaGame::FalloutNv,
                &record(1, Vec::new(), Vec::new())
            )?)?,
            vec![(
                "PACK/10:Follow - Start Location - Trigger Radius".to_owned(),
                vec![0; 4],
            )]
        );
        assert_eq!(
            inserts(invoke(
                SchemaGame::Fallout3,
                &record(3, Vec::new(), Vec::new())
            )?)?,
            vec![
                ("PACK/4:Target 1".to_owned(), vec![0; 16]),
                ("PACK/8:Eat Marker".to_owned(), Vec::new()),
            ]
        );
        let sleep = inserts(invoke(
            SchemaGame::FalloutNv,
            &record(4, Vec::new(), Vec::new()),
        )?)?;
        assert_eq!(sleep.len(), 1);
        assert_eq!(sleep[0].0, "PACK/2:Locations/0:Location 1");
        assert_eq!(&sleep[0].1[..4], &3_i32.to_le_bytes());
        assert_eq!(&sleep[0].1[4..], &[0; 8]);

        let patrol = inserts(invoke(
            SchemaGame::Fallout3,
            &record(13, Vec::new(), Vec::new()),
        )?)?;
        assert_eq!(patrol.len(), 2);
        assert_eq!(patrol[0].0, "PACK/2:Locations/0:Location 1");
        assert_eq!(&patrol[0].1[..4], &6_i32.to_le_bytes());
        assert_eq!(&patrol[0].1[4..], &[0; 8]);
        assert_eq!(patrol[1], ("PACK/11:Patrol Flags".to_owned(), vec![0; 2]));

        assert!(matches!(
            invoke(SchemaGame::Fallout3, &record(12, Vec::new(), Vec::new()),)?,
            HandlerOutput::None
        ));
        assert!(matches!(
            invoke(
                SchemaGame::Fallout3,
                &record(0, vec![subrecord(*b"PTDT", vec![0xaa; 16])], Vec::new(),),
            )?,
            HandlerOutput::None
        ));
        assert!(matches!(
            invoke(
                SchemaGame::FalloutNv,
                &record(4, vec![subrecord(*b"PLD2", vec![0xaa; 12])], Vec::new(),),
            )?,
            HandlerOutput::None
        ));
        let late_location = inserts(invoke(
            SchemaGame::FalloutNv,
            &record(4, Vec::new(), vec![subrecord(*b"PLD2", vec![0xaa; 12])]),
        )?)?;
        assert_eq!(late_location[0].0, "PACK/2:Locations/0:Location 1");
        Ok(())
    }

    /// Clears old Fallout leveled-entry Chance None bytes below form version 69.
    #[test]
    fn fallout_leveled_list_after_load_matches_xedit_versioned_cleanup() -> Result<()> {
        let cases = [
            (
                SchemaGame::Fallout4,
                *b"LVLN",
                "LVLN/7:Leveled List Entries",
                "LVLN/7:Leveled List Entries/repeat/0:Leveled List Entry/0:Base Data",
                Some(10_u64),
            ),
            (
                SchemaGame::Fallout4Vr,
                *b"LVLN",
                "LVLN/7:Leveled List Entries",
                "LVLN/7:Leveled List Entries/repeat/0:Leveled List Entry/0:Base Data",
                Some(10_u64),
            ),
            (
                SchemaGame::Fallout4,
                *b"LVLI",
                "LVLI/7:Leveled List Entries",
                "LVLI/7:Leveled List Entries/repeat/0:Leveled List Entry/0:Base Data",
                Some(10_u64),
            ),
            (
                SchemaGame::Fallout4Vr,
                *b"LVLI",
                "LVLI/7:Leveled List Entries",
                "LVLI/7:Leveled List Entries/repeat/0:Leveled List Entry/0:Base Data",
                Some(10_u64),
            ),
            (
                SchemaGame::Fallout76,
                *b"LVLN",
                "LVLN/12:Leveled List Entries",
                "LVLN/12:Leveled List Entries/repeat/0:Leveled List Entry/0:LVLO",
                Some(10_u64),
            ),
            (
                SchemaGame::Fallout76,
                *b"LVLI",
                "LVLI/19:Leveled List Entries",
                "LVLI/19:Leveled List Entries/repeat/0:Leveled List Entry/0:LVLO",
                Some(10_u64),
            ),
            (
                SchemaGame::Fallout76,
                *b"LVLP",
                "LVLP/7:Leveled List Entries",
                "LVLP/7:Leveled List Entries/repeat/0:Leveled List Entry/0:Reference",
                None,
            ),
        ];
        let registry = SemanticHandlerRegistry::builtin();
        for (game, signature, entries_path, entry_path, chance_none_offset) in cases {
            let binding = CallbackBinding {
                path: Signature(signature).to_string(),
                callback_id: "def.after_load".to_owned(),
                callback_slot: None,
                implementation_fingerprint: "test-fallout-leveled-list-after-load".to_owned(),
                implementation: CallbackImplementation::BuiltIn {
                    operation: bethkit_schema::BuiltInOperation {
                        id: "migrate.fallout_leveled_list_after_load".to_owned(),
                        minimum_version: 1,
                        configuration: serde_json::json!({
                            "entries_path": entries_path,
                            "entry_path": entry_path,
                            "chance_none_offset": chance_none_offset,
                            "modern_form_version": 69,
                        }),
                    },
                },
            };
            let mut changed = vec![0xaa; 14];
            changed[10] = 73;
            let unchanged = vec![0_u8; 12];
            let record = |form_version, flags| WritableRecord {
                signature: Signature(signature),
                flags,
                form_id: FormId(0x1111),
                form_version,
                subrecords: vec![
                    bethkit_core::WritableSubRecord {
                        signature: Signature(*b"LVLO"),
                        data: changed.clone(),
                    },
                    bethkit_core::WritableSubRecord {
                        signature: Signature(*b"COED"),
                        data: vec![0xbb; 12],
                    },
                    bethkit_core::WritableSubRecord {
                        signature: Signature(*b"LVLO"),
                        data: unchanged.clone(),
                    },
                ],
            };
            let invoke = |record: &WritableRecord| {
                registry.invoke_with_writable_record(
                    &binding,
                    HandlerRecordContext::new(
                        Signature(signature),
                        FormId(0x1111),
                        record.form_version,
                        game,
                    ),
                    record,
                    HandlerPhase::AfterLoad,
                    None,
                    None,
                )
            };

            if chance_none_offset.is_none() {
                assert!(matches!(
                    invoke(&record(68, RecordFlags::empty()))?,
                    HandlerOutput::None
                ));
            } else {
                let HandlerOutput::Mutations(mutations) =
                    invoke(&record(68, RecordFlags::empty()))?
                else {
                    return Err(SemanticError::Handler {
                        handler: "test".to_owned(),
                        message: "Fallout leveled-list migration returned no mutation".to_owned(),
                    });
                };
                assert!(matches!(
                    mutations.as_slice(),
                    [HandlerMutation::ReplacePayload {
                        path,
                        occurrence: 0,
                        data,
                    }] if path == entry_path
                        && data.len() == changed.len()
                        && data[..10] == changed[..10]
                        && data[10] == 0
                        && data[11..] == changed[11..]
                ));
            }
            assert!(matches!(
                invoke(&record(69, RecordFlags::empty()))?,
                HandlerOutput::None
            ));
            assert!(matches!(
                invoke(&record(68, RecordFlags::DELETED))?,
                HandlerOutput::None
            ));
        }

        let binding = CallbackBinding {
            path: "LVLI".to_owned(),
            callback_id: "def.after_load".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-short-fallout-leveled-list".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "migrate.fallout_leveled_list_after_load".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({
                        "entries_path": "LVLI/7:Leveled List Entries",
                        "entry_path":
                            "LVLI/7:Leveled List Entries/repeat/0:Leveled List Entry/0:Base Data",
                        "chance_none_offset": 10,
                        "modern_form_version": 69,
                    }),
                },
            },
        };
        let record = WritableRecord {
            signature: Signature(*b"LVLI"),
            flags: RecordFlags::empty(),
            form_id: FormId(0x1111),
            form_version: 68,
            subrecords: vec![bethkit_core::WritableSubRecord {
                signature: Signature(*b"LVLO"),
                data: vec![0xcc; 8],
            }],
        };
        assert!(matches!(
            registry.invoke_with_writable_record(
                &binding,
                HandlerRecordContext::new(
                    Signature(*b"LVLI"),
                    FormId(0x1111),
                    68,
                    SchemaGame::Fallout4,
                ),
                &record,
                HandlerPhase::AfterLoad,
                None,
                None,
            )?,
            HandlerOutput::Mutations(mutations)
                if matches!(
                    mutations.as_slice(),
                    [HandlerMutation::ReplacePayload { data, .. }]
                        if data == &[0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0, 0, 0]
                )
        ));
        Ok(())
    }

    /// Normalizes legacy MGEF actor values for every xEdit archetype branch.
    #[test]
    fn legacy_magic_effect_after_load_matches_xedit_mapping() -> Result<()> {
        for archetype in [1, 2, 3, 13, 16, 17, 18, 19, 30, 31, 32, 33] {
            assert_eq!(
                legacy_magic_effect_actor_value(SchemaGame::Fallout3, archetype),
                Some(-1)
            );
            assert_eq!(
                legacy_magic_effect_actor_value(SchemaGame::FalloutNv, archetype),
                Some(-1)
            );
        }
        for (archetype, actor_value) in [(11, 48), (12, 49), (24, 47)] {
            assert_eq!(
                legacy_magic_effect_actor_value(SchemaGame::Fallout3, archetype),
                Some(actor_value)
            );
            assert_eq!(
                legacy_magic_effect_actor_value(SchemaGame::FalloutNv, archetype),
                Some(actor_value)
            );
        }
        assert_eq!(
            legacy_magic_effect_actor_value(SchemaGame::Fallout3, 35),
            None
        );
        assert_eq!(
            legacy_magic_effect_actor_value(SchemaGame::Fallout3, 36),
            None
        );
        assert_eq!(
            legacy_magic_effect_actor_value(SchemaGame::FalloutNv, 35),
            Some(-1)
        );
        assert_eq!(
            legacy_magic_effect_actor_value(SchemaGame::FalloutNv, 36),
            Some(51)
        );
        assert_eq!(
            legacy_magic_effect_actor_value(SchemaGame::FalloutNv, 34),
            None
        );

        let binding = CallbackBinding {
            path: "MGEF".to_owned(),
            callback_id: "def.after_load".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-legacy-mgef-after-load".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "migrate.legacy_magic_effect_after_load".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({
                        "data_path": "MGEF/5:Data",
                        "archetype_path": "MGEF/5:Data/payload/17:Archtype",
                        "actor_value_path": "MGEF/5:Data/payload/18:Actor Value",
                    }),
                },
            },
        };
        let mut data = vec![0xaa; 76];
        data[64..68].copy_from_slice(&36_u32.to_le_bytes());
        data[68..72].copy_from_slice(&(-9_i32).to_le_bytes());
        let record = |data| WritableRecord {
            signature: Signature(*b"MGEF"),
            flags: RecordFlags::empty(),
            form_id: FormId(0x1111),
            form_version: 0,
            subrecords: vec![bethkit_core::WritableSubRecord {
                signature: Signature(*b"DATA"),
                data,
            }],
        };
        let registry = SemanticHandlerRegistry::builtin();
        let invoke = |game, record: &WritableRecord| {
            registry.invoke_with_writable_record(
                &binding,
                HandlerRecordContext::new(Signature(*b"MGEF"), FormId(0x1111), 0, game),
                record,
                HandlerPhase::AfterLoad,
                None,
                None,
            )
        };

        let output = invoke(SchemaGame::FalloutNv, &record(data.clone()))?;
        let HandlerOutput::Mutations(mutations) = output else {
            return Err(SemanticError::Handler {
                handler: "test".to_owned(),
                message: "legacy MGEF callback did not return its replacement".to_owned(),
            });
        };
        let [HandlerMutation::ReplacePayload {
            path,
            occurrence,
            data: migrated,
        }] = mutations.as_slice()
        else {
            return Err(SemanticError::Handler {
                handler: "test".to_owned(),
                message: "legacy MGEF callback returned unexpected mutations".to_owned(),
            });
        };
        assert_eq!(path, "MGEF/5:Data");
        assert_eq!(*occurrence, 0);
        assert_eq!(&migrated[..68], &data[..68]);
        assert_eq!(&migrated[68..72], &51_i32.to_le_bytes());
        assert_eq!(&migrated[72..], &data[72..]);
        assert!(matches!(
            invoke(SchemaGame::Fallout3, &record(data))?,
            HandlerOutput::None
        ));
        assert!(matches!(
            invoke(SchemaGame::FalloutNv, &record(vec![0xaa; 71]))?,
            HandlerOutput::None
        ));
        Ok(())
    }

    /// Normalizes Skyrim lock level zero and removes one obsolete room portal.
    #[test]
    fn skyrim_reference_after_load_matches_xedit_cleanup() -> Result<()> {
        let binding = CallbackBinding {
            path: "REFR".to_owned(),
            callback_id: "def.after_load".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-skyrim-reference-after-load".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "migrate.skyrim_reference_after_load".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({
                        "lock_path": "REFR/37:Lock Data",
                        "lock_level_path": "REFR/37:Lock Data/payload/0:Level",
                        "portal_path": "REFR/8:Room Portal (unused)",
                    }),
                },
            },
        };
        let subrecord = |signature, data| bethkit_core::WritableSubRecord {
            signature: Signature(signature),
            data,
        };
        let record = |flags, subrecords| WritableRecord {
            signature: Signature(*b"REFR"),
            flags,
            form_id: FormId(0x1111),
            form_version: 44,
            subrecords,
        };
        let registry = SemanticHandlerRegistry::builtin();
        let invoke = |game, record: &WritableRecord| {
            registry.invoke_with_writable_record(
                &binding,
                HandlerRecordContext::new(Signature(*b"REFR"), FormId(0x1111), 44, game),
                record,
                HandlerPhase::AfterLoad,
                None,
                None,
            )
        };
        let original = (0_u8..20).collect::<Vec<_>>();
        let migrated = invoke(
            SchemaGame::SkyrimSe,
            &record(
                RecordFlags::empty(),
                vec![
                    subrecord(*b"XPTL", vec![1]),
                    subrecord(*b"XPTL", vec![2]),
                    subrecord(*b"XLOC", original.clone()),
                ],
            ),
        )?;
        assert!(matches!(
            migrated,
            HandlerOutput::Mutations(mutations)
                if mutations == [
                    HandlerMutation::ReplacePayload {
                        path: "REFR/37:Lock Data".to_owned(),
                        occurrence: 0,
                        data: [vec![1], original[1..].to_vec()].concat(),
                    },
                    HandlerMutation::RemoveFirstBySignature {
                        path: "REFR".to_owned(),
                        signature: Signature(*b"XPTL"),
                    },
                ]
        ));

        let nonzero = invoke(
            SchemaGame::SkyrimVr,
            &record(
                RecordFlags::empty(),
                vec![subrecord(*b"XPTL", vec![1]), subrecord(*b"XLOC", vec![25])],
            ),
        )?;
        assert!(matches!(
            nonzero,
            HandlerOutput::Mutations(mutations)
                if mutations == [HandlerMutation::RemoveFirstBySignature {
                    path: "REFR".to_owned(),
                    signature: Signature(*b"XPTL"),
                }]
        ));
        assert!(matches!(
            invoke(
                SchemaGame::SkyrimLe,
                &record(RecordFlags::empty(), vec![subrecord(*b"XPTL", vec![1])],),
            )?,
            HandlerOutput::None
        ));
        assert!(matches!(
            invoke(
                SchemaGame::SkyrimLe,
                &record(
                    RecordFlags::DELETED,
                    vec![subrecord(*b"XPTL", vec![1]), subrecord(*b"XLOC", vec![0]),],
                ),
            )?,
            HandlerOutput::None
        ));
        Ok(())
    }

    /// Matches legacy ammo cleanup and modern lock normalization for Fallout references.
    #[test]
    fn fallout_reference_after_load_matches_xedit_cleanup() -> Result<()> {
        let legacy_binding = |game| {
            let ammo_index = if game == SchemaGame::Fallout3 { 25 } else { 26 };
            CallbackBinding {
                path: "REFR".to_owned(),
                callback_id: "def.after_load".to_owned(),
                callback_slot: None,
                implementation_fingerprint: "test-legacy-fallout-reference".to_owned(),
                implementation: CallbackImplementation::BuiltIn {
                    operation: bethkit_schema::BuiltInOperation {
                        id: "migrate.fallout_reference_after_load".to_owned(),
                        minimum_version: 1,
                        configuration: serde_json::json!({
                            "mode": "legacy",
                            "unused_path": "REFR/1:Unused",
                            "base_path": "REFR/2:Base",
                            "ammo_path": format!("REFR/{ammo_index}:Ammo"),
                            "ammo_type_path": format!("REFR/{ammo_index}:Ammo/0:Type"),
                            "ammo_count_path": format!("REFR/{ammo_index}:Ammo/1:Count"),
                        }),
                    },
                },
            }
        };
        let lock_binding = |game| {
            let lock_index = if game == SchemaGame::Fallout76 {
                44
            } else {
                39
            };
            CallbackBinding {
                path: "REFR".to_owned(),
                callback_id: "def.after_load".to_owned(),
                callback_slot: None,
                implementation_fingerprint: "test-modern-fallout-reference".to_owned(),
                implementation: CallbackImplementation::BuiltIn {
                    operation: bethkit_schema::BuiltInOperation {
                        id: "migrate.fallout_reference_after_load".to_owned(),
                        minimum_version: 1,
                        configuration: serde_json::json!({
                            "mode": "lock",
                            "lock_path": format!("REFR/{lock_index}:Lock Data"),
                            "lock_level_path":
                                format!("REFR/{lock_index}:Lock Data/payload/0:Level"),
                        }),
                    },
                },
            }
        };
        let subrecord = |signature, data| bethkit_core::WritableSubRecord {
            signature: Signature(signature),
            data,
        };
        let record = |flags, subrecords| WritableRecord {
            signature: Signature(*b"REFR"),
            flags,
            form_id: FormId(0x1111),
            form_version: 44,
            subrecords,
        };
        let mut registry = SemanticHandlerRegistry::builtin();
        registry.set_form_link_resolver(Arc::new(TestFormLinkResolver));
        let invoke = |game, binding: &CallbackBinding, record: &WritableRecord| {
            registry.invoke_with_writable_record(
                binding,
                HandlerRecordContext::new(Signature(*b"REFR"), FormId(0x1111), 44, game),
                record,
                HandlerPhase::AfterLoad,
                None,
                None,
            )
        };

        for game in [SchemaGame::Fallout3, SchemaGame::FalloutNv] {
            let binding = legacy_binding(game);
            let ammo_path = if game == SchemaGame::Fallout3 {
                "REFR/25:Ammo"
            } else {
                "REFR/26:Ammo"
            };
            let cleaned = invoke(
                game,
                &binding,
                &record(
                    RecordFlags::empty(),
                    vec![
                        subrecord(*b"RCLR", vec![1]),
                        subrecord(*b"RCLR", vec![2]),
                        subrecord(*b"NAME", 0x1234_u32.to_le_bytes().to_vec()),
                        subrecord(*b"XAMT", 0x2222_u32.to_le_bytes().to_vec()),
                        subrecord(*b"XAMC", 7_i32.to_le_bytes().to_vec()),
                    ],
                ),
            )?;
            assert!(matches!(
                cleaned,
                HandlerOutput::Mutations(mutations)
                    if mutations == [
                        HandlerMutation::RemoveFirstBySignature {
                            path: "REFR".to_owned(),
                            signature: Signature(*b"RCLR"),
                        },
                        HandlerMutation::RemoveFirstBySignature {
                            path: ammo_path.to_owned(),
                            signature: Signature(*b"XAMT"),
                        },
                        HandlerMutation::RemoveFirstBySignature {
                            path: ammo_path.to_owned(),
                            signature: Signature(*b"XAMC"),
                        },
                    ]
            ));

            let weapon = invoke(
                game,
                &binding,
                &record(
                    RecordFlags::empty(),
                    vec![
                        subrecord(*b"NAME", 0x7777_u32.to_le_bytes().to_vec()),
                        subrecord(*b"XAMT", vec![0; 4]),
                        subrecord(*b"XAMC", vec![0; 4]),
                    ],
                ),
            )?;
            assert!(matches!(weapon, HandlerOutput::None));
        }

        for game in [
            SchemaGame::Fallout4,
            SchemaGame::Fallout4Vr,
            SchemaGame::Fallout76,
        ] {
            let binding = lock_binding(game);
            let lock_path = if game == SchemaGame::Fallout76 {
                "REFR/44:Lock Data"
            } else {
                "REFR/39:Lock Data"
            };
            let original = (0_u8..20).collect::<Vec<_>>();
            let normalized = invoke(
                game,
                &binding,
                &record(
                    RecordFlags::empty(),
                    vec![subrecord(*b"XLOC", original.clone())],
                ),
            )?;
            assert!(matches!(
                normalized,
                HandlerOutput::Mutations(mutations)
                    if mutations == [HandlerMutation::ReplacePayload {
                        path: lock_path.to_owned(),
                        occurrence: 0,
                        data: [vec![1], original[1..].to_vec()].concat(),
                    }]
            ));
            assert!(matches!(
                invoke(
                    game,
                    &binding,
                    &record(RecordFlags::empty(), vec![subrecord(*b"XLOC", vec![25])]),
                )?,
                HandlerOutput::None
            ));
            assert!(matches!(
                invoke(
                    game,
                    &binding,
                    &record(RecordFlags::DELETED, vec![subrecord(*b"XLOC", vec![0])]),
                )?,
                HandlerOutput::None
            ));
        }
        Ok(())
    }

    /// Removes orphaned keywords and aligns NPC morph pairs to the master key order.
    #[test]
    fn fallout_npc_after_load_matches_xedit_morph_order() -> Result<()> {
        let cases = [
            (
                SchemaGame::Fallout4,
                "NPC_/36:Keyword Count",
                "NPC_/37:Keywords",
                "NPC_/65:Morph Keys",
                "NPC_/66:Morph Values",
            ),
            (
                SchemaGame::Fallout4Vr,
                "NPC_/36:Keyword Count",
                "NPC_/37:Keywords",
                "NPC_/65:Morph Keys",
                "NPC_/66:Morph Values",
            ),
            (
                SchemaGame::Fallout76,
                "NPC_/42:Keywords/0:Keyword Count",
                "NPC_/42:Keywords/1:Keywords",
                "NPC_/71:Morph Keys",
                "NPC_/72:Morph Values",
            ),
        ];
        let subrecord = |signature, data| bethkit_core::WritableSubRecord {
            signature: Signature(signature),
            data,
        };
        let record = |flags, subrecords| WritableRecord {
            signature: Signature(*b"NPC_"),
            flags,
            form_id: FormId(0x1111),
            form_version: 131,
            subrecords,
        };
        let mut registry = SemanticHandlerRegistry::builtin();
        registry.set_form_link_resolver(Arc::new(TestFormLinkResolver));
        for (game, keyword_count_path, keywords_path, keys_path, values_path) in cases {
            let binding = CallbackBinding {
                path: "NPC_".to_owned(),
                callback_id: "def.after_load".to_owned(),
                callback_slot: None,
                implementation_fingerprint: "test-fallout-npc-after-load".to_owned(),
                implementation: CallbackImplementation::BuiltIn {
                    operation: bethkit_schema::BuiltInOperation {
                        id: "migrate.fallout_npc_after_load".to_owned(),
                        minimum_version: 1,
                        configuration: serde_json::json!({
                            "keyword_count_path": keyword_count_path,
                            "keywords_path": keywords_path,
                            "morph_keys_path": keys_path,
                            "morph_values_path": values_path,
                        }),
                    },
                },
            };
            let invoke = |record: &WritableRecord| {
                registry.invoke_with_writable_record(
                    &binding,
                    HandlerRecordContext::new(Signature(*b"NPC_"), FormId(0x1111), 131, game),
                    record,
                    HandlerPhase::AfterLoad,
                    None,
                    None,
                )
            };
            let keys = [10_u32, 30, 20]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>();
            let values = [1.0_f32, 3.0, 2.0]
                .into_iter()
                .flat_map(f32::to_le_bytes)
                .collect::<Vec<_>>();
            let migrated = invoke(&record(
                RecordFlags::empty(),
                vec![
                    subrecord(*b"KWDA", vec![0xaa; 8]),
                    subrecord(*b"MSDK", keys),
                    subrecord(*b"MSDV", values),
                ],
            ))?;
            let expected_keys = [20_u32, 10, 30]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>();
            let expected_values = [2.0_f32, 1.0, 3.0]
                .into_iter()
                .flat_map(f32::to_le_bytes)
                .collect::<Vec<_>>();
            assert!(matches!(
                migrated,
                HandlerOutput::Mutations(mutations)
                    if mutations == [
                        HandlerMutation::RemoveFirstBySignature {
                            path: "NPC_".to_owned(),
                            signature: Signature(*b"KWDA"),
                        },
                        HandlerMutation::ReplacePayload {
                            path: keys_path.to_owned(),
                            occurrence: 0,
                            data: expected_keys,
                        },
                        HandlerMutation::ReplacePayload {
                            path: values_path.to_owned(),
                            occurrence: 0,
                            data: expected_values,
                        },
                    ]
            ));

            let ordered_keys = [20_u32, 10, 30]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>();
            assert!(matches!(
                invoke(&record(
                    RecordFlags::empty(),
                    vec![
                        subrecord(*b"KSIZ", 1_u32.to_le_bytes().to_vec()),
                        subrecord(*b"KWDA", vec![0xaa; 4]),
                        subrecord(*b"MSDK", ordered_keys),
                        subrecord(*b"MSDV", vec![0; 12]),
                    ],
                ))?,
                HandlerOutput::None
            ));
            assert!(matches!(
                invoke(&record(
                    RecordFlags::DELETED,
                    vec![subrecord(*b"KWDA", vec![0xaa; 4])],
                ))?,
                HandlerOutput::None
            ));
        }
        Ok(())
    }

    /// Normalizes only Oblivion.esm's hard-coded magic-effect flag exceptions.
    #[test]
    fn oblivion_magic_effect_after_load_matches_xedit_flags() -> Result<()> {
        let binding = CallbackBinding {
            path: "MGEF".to_owned(),
            callback_id: "def.after_load".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-oblivion-mgef-after-load".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "migrate.oblivion_magic_effect_after_load".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({
                        "code_path": "MGEF/0:Magic Effect Code",
                        "data_path": "MGEF/7:Data",
                        "flags_path": "MGEF/7:Data/payload/0:Flags",
                    }),
                },
            },
        };
        let record = |editor_id: &[u8], flags: u32| WritableRecord {
            signature: Signature(*b"MGEF"),
            flags: RecordFlags::empty(),
            form_id: FormId(0x1111),
            form_version: 0,
            subrecords: vec![
                bethkit_core::WritableSubRecord {
                    signature: Signature(*b"EDID"),
                    data: [editor_id, b"\0"].concat(),
                },
                bethkit_core::WritableSubRecord {
                    signature: Signature(*b"DATA"),
                    data: [flags.to_le_bytes().as_slice(), &[0xaa, 0xbb]].concat(),
                },
            ],
        };
        let mut registry = SemanticHandlerRegistry::builtin();
        registry.set_form_link_resolver(Arc::new(TestFormLinkResolver));
        let invoke = |record: &WritableRecord| {
            registry.invoke_with_writable_record(
                &binding,
                HandlerRecordContext::new(
                    Signature(*b"MGEF"),
                    FormId(0x1111),
                    0,
                    SchemaGame::Oblivion,
                ),
                record,
                HandlerPhase::AfterLoad,
                None,
                None,
            )
        };
        for editor_id in [b"RSFI", b"RSFR", b"RSPA", b"RSSH"] {
            assert!(matches!(
                invoke(&record(editor_id, 0x4000_0000))?,
                HandlerOutput::Mutations(mutations)
                    if mutations == [HandlerMutation::ReplacePayload {
                        path: "MGEF/7:Data".to_owned(),
                        occurrence: 0,
                        data: vec![0x08, 0x00, 0x00, 0x40, 0xaa, 0xbb],
                    }]
            ));
        }
        assert!(matches!(
            invoke(&record(b"REAN", 0x4002_0008))?,
            HandlerOutput::Mutations(mutations)
                if mutations == [HandlerMutation::ReplacePayload {
                    path: "MGEF/7:Data".to_owned(),
                    occurrence: 0,
                    data: vec![0x08, 0x00, 0x00, 0x40, 0xaa, 0xbb],
                }]
        ));
        assert!(matches!(
            invoke(&record(b"ABCD", 0x4002_0008))?,
            HandlerOutput::None
        ));
        assert!(matches!(
            invoke(&record(b"RSFI", 0x4000_0008))?,
            HandlerOutput::None
        ));
        Ok(())
    }

    /// Clamps both FO4 SCEN behavior fields without changing neighboring VNAM bytes.
    #[test]
    fn fallout_scene_behavior_after_load_matches_xedit_clamp() -> Result<()> {
        let binding = |path: &str, field_offset| CallbackBinding {
            path: path.to_owned(),
            callback_id: "def.after_load".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-fo4-scen-behavior-after-load".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "migrate.fallout_scene_behavior_after_load".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({ "field_offset": field_offset }),
                },
            },
        };
        let record = |data| WritableRecord {
            signature: Signature(*b"SCEN"),
            flags: RecordFlags::empty(),
            form_id: FormId(0x1111),
            form_version: 131,
            subrecords: vec![bethkit_core::WritableSubRecord {
                signature: Signature(*b"VNAM"),
                data,
            }],
        };
        let invoke = |game, binding: &CallbackBinding, record: &WritableRecord| {
            SemanticHandlerRegistry::builtin().invoke_with_records(
                binding,
                HandlerRecordContext::new(Signature(*b"SCEN"), FormId(0x1111), 131, game),
                HandlerInvocationAccess::writable_subrecord_with_scope(record, 0, None),
                HandlerPhase::AfterLoad,
                None,
                None,
            )
        };
        let player_path = "SCEN/8:Actor Behavior Settings/payload/2:Player Dialogue";
        let observe_path = "SCEN/8:Actor Behavior Settings/payload/3:Observe Combat";
        let mut original = (0_u8..20).collect::<Vec<_>>();
        original[8..12].copy_from_slice(&4_u32.to_le_bytes());
        original[12..16].copy_from_slice(&u32::MAX.to_le_bytes());

        let player = invoke(
            SchemaGame::Fallout4,
            &binding(player_path, 8),
            &record(original.clone()),
        )?;
        let HandlerOutput::SubrecordPayload(player) = player else {
            return Err(SemanticError::Handler {
                handler: "test".to_owned(),
                message: "player-dialogue clamp did not return a payload".to_owned(),
            });
        };
        assert_eq!(&player[..8], &original[..8]);
        assert_eq!(&player[8..12], &3_u32.to_le_bytes());
        assert_eq!(&player[12..], &original[12..]);

        let observe = invoke(
            SchemaGame::Fallout4Vr,
            &binding(observe_path, 12),
            &record(original.clone()),
        )?;
        let HandlerOutput::SubrecordPayload(observe) = observe else {
            return Err(SemanticError::Handler {
                handler: "test".to_owned(),
                message: "observe-combat clamp did not return a payload".to_owned(),
            });
        };
        assert_eq!(&observe[..12], &original[..12]);
        assert_eq!(&observe[12..16], &3_u32.to_le_bytes());
        assert_eq!(&observe[16..], &original[16..]);

        let mut valid = original;
        valid[8..12].copy_from_slice(&3_u32.to_le_bytes());
        assert!(matches!(
            invoke(
                SchemaGame::Fallout4,
                &binding(player_path, 8),
                &record(valid),
            )?,
            HandlerOutput::None
        ));
        assert!(matches!(
            invoke(
                SchemaGame::Fallout4,
                &binding(observe_path, 12),
                &record(vec![0xaa; 15]),
            )?,
            HandlerOutput::None
        ));
        Ok(())
    }

    /// Removes OFST by raw signature and honors xEdit's disable switch.
    #[test]
    fn offset_data_after_load_matches_xedit_option() -> Result<()> {
        let binding = CallbackBinding {
            path: "TES4".to_owned(),
            callback_id: "def.after_load".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-offset-data-after-load".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "migrate.remove_offset_data".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({}),
                },
            },
        };
        let record = WritableRecord {
            signature: Signature(*b"TES4"),
            flags: RecordFlags::empty(),
            form_id: FormId::NULL,
            form_version: 0,
            subrecords: vec![bethkit_core::WritableSubRecord {
                signature: Signature(*b"OFST"),
                data: vec![1, 2, 3, 4],
            }],
        };
        let source =
            HandlerRecordContext::new(Signature(*b"TES4"), FormId::NULL, 0, SchemaGame::Fallout76);

        let output = SemanticHandlerRegistry::builtin().invoke_with_writable_record(
            &binding,
            source,
            &record,
            HandlerPhase::AfterLoad,
            None,
            None,
        )?;

        assert!(matches!(
            output,
            HandlerOutput::Mutations(mutations)
                if mutations
                    == [HandlerMutation::RemoveFirstBySignature {
                        path: "TES4".to_owned(),
                        signature: Signature(*b"OFST"),
                    }]
        ));

        let mut disabled = SemanticHandlerRegistry::builtin();
        disabled.set_remove_offset_data(false);
        assert!(matches!(
            disabled.invoke_with_writable_record(
                &binding,
                source,
                &record,
                HandlerPhase::AfterLoad,
                None,
                None,
            )?,
            HandlerOutput::None
        ));
        Ok(())
    }

    /// Removes the first generic WRLD OFST across every xEdit game that defines WRLD.
    #[test]
    fn worldspace_offset_data_after_load_matches_xedit_option() -> Result<()> {
        let binding = CallbackBinding {
            path: "WRLD".to_owned(),
            callback_id: "record.after_load".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-worldspace-offset-data".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "migrate.remove_worldspace_offset_data".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({}),
                },
            },
        };
        let record = WritableRecord {
            signature: Signature(*b"WRLD"),
            flags: RecordFlags::empty(),
            form_id: FormId::NULL,
            form_version: 0,
            subrecords: vec![
                bethkit_core::WritableSubRecord {
                    signature: Signature(*b"OFST"),
                    data: vec![1],
                },
                bethkit_core::WritableSubRecord {
                    signature: Signature(*b"OFST"),
                    data: vec![2],
                },
            ],
        };
        let handlers = SemanticHandlerRegistry::builtin();

        // when / then
        for game in [
            SchemaGame::Oblivion,
            SchemaGame::Fallout3,
            SchemaGame::FalloutNv,
            SchemaGame::SkyrimLe,
            SchemaGame::SkyrimSe,
            SchemaGame::SkyrimVr,
            SchemaGame::Fallout4,
            SchemaGame::Fallout4Vr,
            SchemaGame::Fallout76,
            SchemaGame::Starfield,
        ] {
            let source = HandlerRecordContext::new(Signature(*b"WRLD"), FormId::NULL, 0, game);
            assert!(matches!(
                handlers.invoke_with_writable_record(
                    &binding,
                    source,
                    &record,
                    HandlerPhase::AfterLoad,
                    None,
                    None,
                )?,
                HandlerOutput::Mutations(mutations)
                    if mutations == [HandlerMutation::RemoveFirstBySignature {
                        path: "WRLD".to_owned(),
                        signature: Signature(*b"OFST"),
                    }]
            ));
        }
        let mut disabled = SemanticHandlerRegistry::builtin();
        disabled.set_remove_offset_data(false);
        assert!(matches!(
            disabled.invoke_with_writable_record(
                &binding,
                HandlerRecordContext::new(
                    Signature(*b"WRLD"),
                    FormId::NULL,
                    0,
                    SchemaGame::Fallout3,
                ),
                &record,
                HandlerPhase::AfterLoad,
                None,
                None,
            )?,
            HandlerOutput::None
        ));
        Ok(())
    }

    /// Applies each WRLD cleanup with xEdit's game, option, and source-file conditions.
    #[test]
    fn worldspace_after_load_matches_xedit_cleanup() -> Result<()> {
        fn invoke(
            handlers: &SemanticHandlerRegistry,
            game: SchemaGame,
            source_file_load_order: Option<u32>,
            signatures: &[[u8; 4]],
        ) -> Result<HandlerOutput> {
            let binding = CallbackBinding {
                path: "WRLD".to_owned(),
                callback_id: "def.after_load".to_owned(),
                callback_slot: None,
                implementation_fingerprint: "test-worldspace-after-load".to_owned(),
                implementation: CallbackImplementation::BuiltIn {
                    operation: bethkit_schema::BuiltInOperation {
                        id: "migrate.worldspace_after_load".to_owned(),
                        minimum_version: 1,
                        configuration: serde_json::json!({}),
                    },
                },
            };
            let record = WritableRecord {
                signature: Signature(*b"WRLD"),
                flags: RecordFlags::empty(),
                form_id: FormId(0x3c),
                form_version: 0,
                subrecords: signatures
                    .iter()
                    .map(|signature| bethkit_core::WritableSubRecord {
                        signature: Signature(*signature),
                        data: vec![1],
                    })
                    .collect(),
            };
            let context = HandlerRecordContext::new(Signature(*b"WRLD"), FormId(0x3c), 0, game);
            let mut handlers = handlers.clone();
            if let Some(load_order) = source_file_load_order {
                handlers.set_worldspace_source_file_load_order(load_order);
            }
            handlers.invoke_with_writable_record(
                &binding,
                context,
                &record,
                HandlerPhase::AfterLoad,
                None,
                None,
            )
        }

        // given / when / then
        let handlers = SemanticHandlerRegistry::builtin();
        assert!(matches!(
            invoke(
                &handlers,
                SchemaGame::Fallout4,
                None,
                &[*b"OFST", *b"RNAM", *b"CLSZ"]
            )?,
            HandlerOutput::Mutations(mutations)
                if mutations == [
                    HandlerMutation::RemoveFirstBySignature {
                        path: "WRLD".to_owned(),
                        signature: Signature(*b"RNAM"),
                    },
                    HandlerMutation::RemoveFirstBySignature {
                        path: "WRLD".to_owned(),
                        signature: Signature(*b"CLSZ"),
                    },
                ]
        ));
        assert!(matches!(
            invoke(
                &handlers,
                SchemaGame::SkyrimSe,
                Some(0),
                &[*b"RNAM"]
            )?,
            HandlerOutput::Mutations(mutations)
                if mutations == [HandlerMutation::RemoveFirstBySignature {
                    path: "WRLD".to_owned(),
                    signature: Signature(*b"RNAM"),
                }]
        ));
        assert!(matches!(
            invoke(&handlers, SchemaGame::SkyrimSe, Some(1), &[*b"RNAM"])?,
            HandlerOutput::None
        ));
        assert!(matches!(
            invoke(&handlers, SchemaGame::SkyrimSe, None, &[*b"RNAM"]),
            Err(SemanticError::Handler { message, .. })
                if message.contains("source-file load order")
        ));
        assert!(matches!(
            invoke(
                &handlers,
                SchemaGame::Fallout76,
                None,
                &[*b"OFST", *b"RNAM", *b"CLSZ", *b"NAM0", *b"NAM9"]
            )?,
            HandlerOutput::None
        ));
        let mut disabled = SemanticHandlerRegistry::builtin();
        disabled.set_remove_offset_data(false);
        assert!(matches!(
            invoke(
                &disabled,
                SchemaGame::Starfield,
                None,
                &[*b"OFST", *b"CLSZ"]
            )?,
            HandlerOutput::None
        ));
        assert!(matches!(
            invoke(
                &handlers,
                SchemaGame::Starfield,
                None,
                &[*b"OFST", *b"OFST", *b"CLSZ"]
            )?,
            HandlerOutput::Mutations(mutations)
                if mutations.len() == 2
                    && mutations[0] == HandlerMutation::RemoveFirstBySignature {
                        path: "WRLD".to_owned(),
                        signature: Signature(*b"OFST"),
                    }
                    && mutations[1] == HandlerMutation::RemoveFirstBySignature {
                        path: "WRLD".to_owned(),
                        signature: Signature(*b"CLSZ"),
                    }
        ));
        Ok(())
    }

    /// Reverses REGN points with xEdit's Single comparison and game-specific reads.
    #[test]
    fn region_point_after_load_matches_xedit_ordering() -> Result<()> {
        fn point(x: f32, y: f32) -> Vec<u8> {
            [x.to_le_bytes(), y.to_le_bytes()].concat()
        }

        fn invoke(game: SchemaGame, data: Vec<u8>) -> Result<HandlerOutput> {
            let binding = CallbackBinding {
                path: "REGN/3:Region Areas/repeat/0:Region Area/1:Region Point List Data"
                    .to_owned(),
                callback_id: "def.after_load".to_owned(),
                callback_slot: None,
                implementation_fingerprint: "test-region-points-after-load".to_owned(),
                implementation: CallbackImplementation::BuiltIn {
                    operation: bethkit_schema::BuiltInOperation {
                        id: "migrate.region_point_order".to_owned(),
                        minimum_version: 1,
                        configuration: serde_json::json!({}),
                    },
                },
            };
            let record = WritableRecord {
                signature: Signature(*b"REGN"),
                flags: RecordFlags::empty(),
                form_id: FormId(0x1111),
                form_version: 0,
                subrecords: vec![bethkit_core::WritableSubRecord {
                    signature: Signature(*b"RPLD"),
                    data,
                }],
            };
            SemanticHandlerRegistry::builtin().invoke_with_records(
                &binding,
                HandlerRecordContext::new(Signature(*b"REGN"), FormId(0x1111), 0, game),
                HandlerInvocationAccess::writable_subrecord_with_scope(&record, 0, None),
                HandlerPhase::AfterLoad,
                None,
                None,
            )
        }

        let first = point(3.0, 9.0);
        let middle = point(2.0, f32::from_bits(0x7fc0_1234));
        let last = point(1.0, 2.0);
        let output = invoke(
            SchemaGame::SkyrimSe,
            [first.as_slice(), middle.as_slice(), last.as_slice()].concat(),
        )?;
        assert!(matches!(
            output,
            HandlerOutput::SubrecordPayload(data)
                if data == [last.as_slice(), middle.as_slice(), first.as_slice()].concat()
        ));

        let rounded_first = point(0.000_100_4, 0.0);
        let rounded_last = point(0.0, 1.0);
        let boundary = [rounded_first.as_slice(), rounded_last.as_slice()].concat();
        assert!(matches!(
            invoke(SchemaGame::Oblivion, boundary.clone())?,
            HandlerOutput::SubrecordPayload(_)
        ));
        assert!(matches!(
            invoke(SchemaGame::SkyrimSe, boundary)?,
            HandlerOutput::None
        ));
        assert_eq!(
            system_math_single_compare(f32::NAN, 0.0),
            std::cmp::Ordering::Greater
        );
        assert_eq!(
            system_math_single_compare(1.0, 1.000_05),
            std::cmp::Ordering::Equal
        );
        Ok(())
    }

    /// Verifies the fixed modern EFIT layout that makes xEdit's stale setter inert.
    #[test]
    fn modern_efit_after_load_verifier_rejects_layout_drift() -> Result<()> {
        let binding = CallbackBinding {
            path: "TEST/0:Effect/1:EFIT".to_owned(),
            callback_id: "def.after_load".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-modern-efit-after-load".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "verify.inert_modern_efit_after_load".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({ "expected_payload_size": 12 }),
                },
            },
        };
        let source =
            HandlerRecordContext::new(Signature(*b"TEST"), FormId(0x1111), 0, SchemaGame::SkyrimSe);
        let record = |size| WritableRecord {
            signature: Signature(*b"TEST"),
            flags: RecordFlags::empty(),
            form_id: FormId(0x1111),
            form_version: 0,
            subrecords: vec![bethkit_core::WritableSubRecord {
                signature: Signature(*b"EFIT"),
                data: vec![0x5a; size],
            }],
        };
        let valid = record(12);
        let output = SemanticHandlerRegistry::builtin().invoke_with_records(
            &binding,
            source,
            HandlerInvocationAccess::writable_subrecord_with_scope(&valid, 0, None),
            HandlerPhase::AfterLoad,
            None,
            None,
        )?;
        assert!(matches!(output, HandlerOutput::None));

        let drifted = record(20);
        let error = SemanticHandlerRegistry::builtin()
            .invoke_with_records(
                &binding,
                source,
                HandlerInvocationAccess::writable_subrecord_with_scope(&drifted, 0, None),
                HandlerPhase::AfterLoad,
                None,
                None,
            )
            .expect_err("a legacy-sized EFIT must fail the modern verifier");
        assert!(error.to_string().contains("expected 12 bytes"));

        let mut versioned_binding = binding;
        let CallbackImplementation::BuiltIn { operation } = &mut versioned_binding.implementation
        else {
            return Err(SemanticError::Handler {
                handler: "verify.inert_modern_efit_after_load".to_owned(),
                message: "test binding is not built in".to_owned(),
            });
        };
        operation.configuration = serde_json::json!({});
        let versioned = record(24);
        let output = SemanticHandlerRegistry::builtin().invoke_with_records(
            &versioned_binding,
            HandlerRecordContext::new(
                Signature(*b"TEST"),
                FormId(0x1111),
                166,
                SchemaGame::Fallout76,
            ),
            HandlerInvocationAccess::writable_subrecord_with_scope(&versioned, 0, None),
            HandlerPhase::AfterLoad,
            None,
            None,
        )?;
        assert!(matches!(output, HandlerOutput::None));
        Ok(())
    }

    /// Converts embedded Quest scripts to Object scripts without touching other SCHR bytes.
    #[test]
    fn embedded_script_after_load_migrates_only_type() -> Result<()> {
        let binding = CallbackBinding {
            path: "TEST/0:Embedded Script".to_owned(),
            callback_id: "def.after_load".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-embedded-script-after-load".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "migrate.embedded_script_type".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({
                        "anchor_path_suffix": "/0:Basic Script Data",
                    }),
                },
            },
        };
        let mut data = (0_u8..20).collect::<Vec<_>>();
        data[16..18].copy_from_slice(&1_u16.to_le_bytes());
        let record = WritableRecord {
            signature: Signature(*b"TEST"),
            flags: RecordFlags::empty(),
            form_id: FormId::NULL,
            form_version: 0,
            subrecords: vec![bethkit_core::WritableSubRecord {
                signature: Signature(*b"SCHR"),
                data: data.clone(),
            }],
        };
        let output = SemanticHandlerRegistry::builtin().invoke_with_records(
            &binding,
            HandlerRecordContext::new(Signature(*b"TEST"), FormId::NULL, 0, SchemaGame::Fallout3),
            HandlerInvocationAccess::writable_subrecord_with_scope(&record, 0, None),
            HandlerPhase::AfterLoad,
            None,
            None,
        )?;

        let HandlerOutput::SubrecordPayload(migrated) = output else {
            return Err(SemanticError::Handler {
                handler: "migrate.embedded_script_type".to_owned(),
                message: "embedded-script migration did not return a payload".to_owned(),
            });
        };
        assert_eq!(&migrated[..16], &data[..16]);
        assert_eq!(&migrated[16..18], &0_u16.to_le_bytes());
        assert_eq!(&migrated[18..], &data[18..]);

        let mut object_record = record;
        object_record.subrecords[0].data[16..18].copy_from_slice(&0_u16.to_le_bytes());
        let unchanged = SemanticHandlerRegistry::builtin().invoke_with_records(
            &binding,
            HandlerRecordContext::new(Signature(*b"TEST"), FormId::NULL, 0, SchemaGame::Fallout3),
            HandlerInvocationAccess::writable_subrecord_with_scope(&object_record, 0, None),
            HandlerPhase::AfterLoad,
            None,
            None,
        )?;
        assert!(matches!(unchanged, HandlerOutput::None));
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
                    configuration: serde_json::json!({
                        "run_on_path": "TEST/0:CTDA/payload/7:Run On",
                        "reference_path": "TEST/0:CTDA/payload/8:Reference",
                        "reference_run_on": 2,
                        "null_reference": 0
                    }),
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
        let binding = |legacy_use_global| CallbackBinding {
            path: "TEST/0:CTDA/payload/0:Type".to_owned(),
            callback_id: "def.after_set".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-ctda-type".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "edit.ctda_type".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({
                        "type_path": "TEST/0:CTDA/payload/0:Type",
                        "comparison_path": "TEST/0:CTDA/payload/2:Comparison Value",
                        "global_value_mask": 4,
                        "comparison_default": 0,
                        "legacy_use_global": legacy_use_global,
                        "run_on_path": "TEST/0:CTDA/payload/7:Run On",
                        "legacy_use_global_mask": 2,
                        "subject_run_on": 1
                    }),
                },
            },
        };
        let handlers = SemanticHandlerRegistry::builtin();
        let old = FieldValue::UInt(0);
        let modern = FieldValue::UInt(4);
        let modern_binding = binding(false);
        let modern_output = handlers.invoke(
            &modern_binding,
            HandlerRecordContext::new(Signature(*b"TEST"), FormId::NULL, 0, SchemaGame::Morrowind),
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
        let legacy_binding = binding(true);
        let legacy_output = handlers.invoke(
            &legacy_binding,
            HandlerRecordContext::new(Signature(*b"TEST"), FormId::NULL, 0, SchemaGame::Morrowind),
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

    /// Resets GMST DATA only when the editor-ID type prefix changes.
    #[test]
    fn game_setting_editor_id_resets_value_on_type_change() -> Result<()> {
        let binding = |data_path: &str| CallbackBinding {
            path: "GMST/0:Editor ID".to_owned(),
            callback_id: "def.after_set".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-game-setting-editor-id".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "edit.game_setting_editor_id".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({
                        "editor_id_path": "GMST/0:Editor ID",
                        "data_path": data_path
                    }),
                },
            },
        };
        let handlers = SemanticHandlerRegistry::builtin();
        let old = FieldValue::String(std::borrow::Cow::Borrowed("fExample"));
        let changed = FieldValue::String(std::borrow::Cow::Borrowed("iExample"));
        for data_path in ["GMST/1:Value", "GMST/2:Value"] {
            let output = handlers.invoke(
                &binding(data_path),
                HandlerRecordContext::new(
                    Signature(*b"TEST"),
                    FormId::NULL,
                    0,
                    SchemaGame::Morrowind,
                ),
                HandlerPhase::AfterSet,
                Some(&changed),
                Some(&old),
            )?;
            assert!(matches!(
                output,
                HandlerOutput::Mutations(mutations)
                    if mutations == [
                        HandlerMutation::Remove {
                            path: data_path.to_owned(),
                            occurrence: 0,
                        },
                        HandlerMutation::InsertDefault {
                            path: data_path.to_owned(),
                        }
                    ]
            ));
        }
        assert!(matches!(
            handlers.invoke(
                &binding("GMST/1:Value"),
                HandlerRecordContext::new(
                    Signature(*b"TEST"),
                    FormId::NULL,
                    0,
                    SchemaGame::Morrowind,
                ),
                HandlerPhase::AfterSet,
                Some(&FieldValue::String(std::borrow::Cow::Borrowed("fChanged"))),
                Some(&old),
            )?,
            HandlerOutput::None
        ));
        Ok(())
    }

    /// Rebuilds PERK effect data and clears dependent containers after a type change.
    #[test]
    fn perk_effect_type_rebuilds_dependent_state() -> Result<()> {
        let type_path = "PERK/8:Effects/repeat/0:Effect/0:Header/payload/0:Type";
        let data_path = "PERK/8:Effects/repeat/0:Effect/1:Effect Data";
        let conditions_path = "PERK/8:Effects/repeat/0:Effect/2:Perk Conditions";
        let parameters_path = "PERK/8:Effects/repeat/0:Effect/3:Function Parameters";
        let parameter_type_path = "PERK/8:Effects/repeat/0:Effect/3:Function Parameters/0:Type";
        let function_path =
            "PERK/8:Effects/repeat/0:Effect/1:Effect Data/payload/2:Entry Point/1:Function";
        let binding = CallbackBinding {
            path: type_path.to_owned(),
            callback_id: "def.after_set".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-perk-effect-type".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "edit.perk_effect_type".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({
                        "type_path": type_path,
                        "data_path": data_path,
                        "conditions_path": conditions_path,
                        "parameters_path": parameters_path,
                        "parameter_type_path": parameter_type_path,
                        "function_path": function_path,
                        "entry_point_type": 2,
                        "entry_point_function": 2
                    }),
                },
            },
        };
        let handlers = SemanticHandlerRegistry::builtin();
        let record =
            HandlerRecordContext::new(Signature(*b"TEST"), FormId::NULL, 0, SchemaGame::Morrowind);

        let output = handlers.invoke(
            &binding,
            record,
            HandlerPhase::AfterSet,
            Some(&FieldValue::Int(2)),
            Some(&FieldValue::Int(0)),
        )?;

        assert!(matches!(
            output,
            HandlerOutput::Mutations(mutations)
                if mutations == [
                    HandlerMutation::ResetToDefault {
                        path: data_path.to_owned(),
                        occurrence: 0,
                    },
                    HandlerMutation::RemoveContainer {
                        path: conditions_path.to_owned(),
                    },
                    HandlerMutation::RemoveContainer {
                        path: parameters_path.to_owned(),
                    },
                    HandlerMutation::InsertDefault {
                        path: parameter_type_path.to_owned(),
                    },
                    HandlerMutation::Set {
                        path: function_path.to_owned(),
                        occurrence: 0,
                        value: OwnedFieldValue::Int(2),
                    },
                ]
        ));
        assert!(matches!(
            handlers.invoke(
                &binding,
                record,
                HandlerPhase::AfterSet,
                Some(&FieldValue::Int(1)),
                Some(&FieldValue::Int(1)),
            )?,
            HandlerOutput::None
        ));
        assert!(matches!(
            handlers.invoke(
                &binding,
                record,
                HandlerPhase::AfterSet,
                Some(&FieldValue::Int(9)),
                Some(&FieldValue::Int(1)),
            )?,
            HandlerOutput::Mutations(mutations)
                if mutations.len() == 3
                    && matches!(
                        mutations.last(),
                        Some(HandlerMutation::RemoveContainer { path })
                            if path == parameters_path
                    )
        ));
        Ok(())
    }

    /// Reproduces the coupled legacy PERK entry-point, function, and EPFT setters.
    #[test]
    fn legacy_perk_after_set_rebuilds_coupled_state() -> Result<()> {
        let entry_path = "PERK/6:Effects/repeat/0:Effect/1:DATA/payload/0:Entry Point";
        let function_path = "PERK/6:Effects/repeat/0:Effect/1:DATA/payload/1:Function";
        let count_path = "PERK/6:Effects/repeat/0:Effect/1:DATA/payload/2:Count";
        let condition_item_path = "PERK/6:Effects/repeat/0:Effect/2:Conditions/repeat/0:Condition";
        let condition_index_path = concat!(
            "PERK/6:Effects/repeat/0:Effect/2:Conditions/repeat/",
            "0:Condition/0:Run On"
        );
        let parameter_type_path = "PERK/6:Effects/repeat/0:Effect/3:Parameters/0:Type";
        let parameter_data_path = "PERK/6:Effects/repeat/0:Effect/3:Parameters/1:Data";
        let button_label_path = "PERK/6:Effects/repeat/0:Effect/3:Parameters/2:Button";
        let script_flags_path = "PERK/6:Effects/repeat/0:Effect/3:Parameters/3:Flags";
        let embedded_script_path = "PERK/6:Effects/repeat/0:Effect/3:Parameters/4:Script";
        let script_header_path = "PERK/6:Effects/repeat/0:Effect/3:Parameters/4:Script/0:Header";
        let configuration = |binding_path: &str| {
            let mut value = serde_json::json!({
                "binding_path": binding_path,
                "function_path": function_path,
                "condition_count_path": count_path,
                "condition_item_path": condition_item_path,
                "condition_index_path": condition_index_path,
                "parameter_type_path": parameter_type_path,
                "parameter_data_path": parameter_data_path,
                "button_label_path": button_label_path,
                "script_flags_path": script_flags_path,
                "embedded_script_path": embedded_script_path,
                "script_header_path": script_header_path,
                "callback_offset": 0,
                "callback_width": 1,
                "callback_signed": false,
                "callback_byte_order": "little",
                "function_offset": 1,
                "function_width": 1,
                "function_signed": false,
                "function_byte_order": "little",
                "parameter_type_offset": 0,
                "parameter_type_width": 1,
                "parameter_type_signed": false,
                "parameter_type_byte_order": "little",
                "condition_index_offset": 0,
                "condition_index_width": 1,
                "condition_index_signed": false,
                "condition_index_byte_order": "little"
            });
            value["entry_point_conditions"] = serde_json::json!([
                3, 3, 3, 2, 1, 2, 7, 2, 3, 0, 0, 0, 0, 0, 4, 5, 6, 1, 0, 0, 0, 4, 0, 0, 0, 0, 0, 4,
                0, 0, 0, 0, 0, 0, 2, 3, 3
            ]);
            value["entry_point_function_types"] = serde_json::json!([
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 2,
                0, 0, 0, 0, 0, 0, 0, 0, 0
            ]);
            value["condition_slots"] = serde_json::json!([
                [1, 0, 0],
                [1, 2, 0],
                [1, 3, 0],
                [1, 3, 4],
                [1, 4, 0],
                [1, 5, 0],
                [1, 5, 6],
                [1, 5, 7]
            ]);
            value["function_types"] = serde_json::json!([3, 0, 0, 0, 0, 0, 3, 3, 1, 2]);
            value["function_parameter_types"] = serde_json::json!([0, 1, 1, 1, 2, 2, 0, 0, 3, 4]);
            value
        };
        let binding = |id: &str, path: &str| CallbackBinding {
            path: path.to_owned(),
            callback_id: "def.after_set".to_owned(),
            callback_slot: None,
            implementation_fingerprint: format!("test-{id}"),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: id.to_owned(),
                    minimum_version: 1,
                    configuration: configuration(path),
                },
            },
        };
        let subrecord = |signature, data| bethkit_core::WritableSubRecord { signature, data };
        let record = WritableRecord {
            signature: Signature(*b"PERK"),
            flags: RecordFlags::empty(),
            form_id: FormId::NULL,
            form_version: 0,
            subrecords: vec![
                subrecord(Signature(*b"PRKE"), vec![2, 0, 0]),
                subrecord(Signature(*b"DATA"), vec![0, 1, 3]),
                subrecord(Signature(*b"PRKC"), vec![0]),
                subrecord(Signature(*b"CTDA"), vec![0; 28]),
                subrecord(Signature(*b"PRKC"), vec![1]),
                subrecord(Signature(*b"CTDA"), vec![0; 28]),
                subrecord(Signature(*b"PRKC"), vec![2]),
                subrecord(Signature(*b"CTDA"), vec![0; 28]),
                subrecord(Signature(*b"PRKC"), vec![3]),
                subrecord(Signature(*b"CTDA"), vec![0; 28]),
                subrecord(Signature(*b"EPFT"), vec![1]),
                subrecord(Signature(*b"EPFD"), vec![0; 4]),
            ],
        };
        let handlers = SemanticHandlerRegistry::builtin();
        let context =
            HandlerRecordContext::new(Signature(*b"PERK"), FormId::NULL, 0, SchemaGame::Fallout3);
        let output = handlers.invoke_with_records(
            &binding("edit.legacy_perk_entry_point", entry_path),
            context,
            HandlerInvocationAccess::writable_subrecord_with_scope(&record, 1, None),
            HandlerPhase::AfterSet,
            Some(&FieldValue::Int(21)),
            Some(&FieldValue::Int(0)),
        )?;
        assert!(matches!(
            output,
            HandlerOutput::Mutations(mutations)
                if mutations == [
                    HandlerMutation::Set {
                        path: function_path.to_owned(),
                        occurrence: 0,
                        value: OwnedFieldValue::Int(8),
                    },
                    HandlerMutation::Set {
                        path: parameter_type_path.to_owned(),
                        occurrence: 0,
                        value: OwnedFieldValue::Int(3),
                    },
                    HandlerMutation::Remove {
                        path: parameter_data_path.to_owned(),
                        occurrence: 0,
                    },
                    HandlerMutation::RemoveContainer {
                        path: embedded_script_path.to_owned(),
                    },
                    HandlerMutation::InsertDefault {
                        path: parameter_data_path.to_owned(),
                    },
                    HandlerMutation::Set {
                        path: count_path.to_owned(),
                        occurrence: 0,
                        value: OwnedFieldValue::Int(2),
                    },
                    HandlerMutation::RemoveContainerOccurrence {
                        path: condition_item_path.to_owned(),
                        occurrence: 3,
                    },
                    HandlerMutation::RemoveContainerOccurrence {
                        path: condition_item_path.to_owned(),
                        occurrence: 2,
                    },
                ]
        ));

        let function_output = handlers.invoke_with_records(
            &binding("edit.legacy_perk_function", function_path),
            context,
            HandlerInvocationAccess::writable_subrecord_with_scope(&record, 1, None),
            HandlerPhase::AfterSet,
            Some(&FieldValue::Int(5)),
            Some(&FieldValue::Int(4)),
        )?;
        assert!(matches!(
            function_output,
            HandlerOutput::Mutations(mutations)
                if mutations == [
                    HandlerMutation::Set {
                        path: parameter_type_path.to_owned(),
                        occurrence: 0,
                        value: OwnedFieldValue::Int(2),
                    },
                    HandlerMutation::Remove {
                        path: parameter_data_path.to_owned(),
                        occurrence: 0,
                    },
                    HandlerMutation::RemoveContainer {
                        path: embedded_script_path.to_owned(),
                    },
                    HandlerMutation::InsertDefault {
                        path: parameter_data_path.to_owned(),
                    },
                ]
        ));

        let parameter_output = handlers.invoke_with_records(
            &binding("edit.legacy_perk_parameter_type", parameter_type_path),
            context,
            HandlerInvocationAccess::writable_subrecord_with_scope(&record, 10, None),
            HandlerPhase::AfterSet,
            Some(&FieldValue::Int(4)),
            Some(&FieldValue::Int(1)),
        )?;
        assert!(matches!(
            parameter_output,
            HandlerOutput::Mutations(mutations)
                if mutations == [
                    HandlerMutation::Remove {
                        path: parameter_data_path.to_owned(),
                        occurrence: 0,
                    },
                    HandlerMutation::RemoveContainer {
                        path: embedded_script_path.to_owned(),
                    },
                    HandlerMutation::InsertDefault {
                        path: button_label_path.to_owned(),
                    },
                    HandlerMutation::InsertDefault {
                        path: script_flags_path.to_owned(),
                    },
                    HandlerMutation::InsertDefault {
                        path: script_header_path.to_owned(),
                    },
                ]
        ));

        let mut new_vegas_configuration = configuration(entry_path);
        new_vegas_configuration["entry_point_conditions"] = serde_json::json!([
            3, 3, 3, 2, 1, 2, 7, 2, 3, 0, 0, 0, 0, 0, 4, 5, 6, 1, 0, 0, 0, 4, 0, 0, 0, 0, 0, 4, 0,
            0, 0, 0, 0, 0, 2, 3, 3, 2, 2, 2, 2, 0, 0, 2, 0, 0, 0, 0, 0, 2, 2, 0, 2, 2, 2, 0, 3, 2,
            3, 2, 2, 0, 3, 0, 3, 0, 0, 0, 0, 0, 0, 0, 2, 2
        ]);
        new_vegas_configuration["entry_point_function_types"] =
            serde_json::Value::Array((0..74).map(|_| serde_json::Value::from(0)).collect());
        new_vegas_configuration["entry_point_function_types"][21] = serde_json::Value::from(1);
        new_vegas_configuration["entry_point_function_types"][27] = serde_json::Value::from(2);
        new_vegas_configuration["function_types"] =
            serde_json::json!([3, 0, 0, 0, 0, 0, 0, 0, 1, 2]);
        let new_vegas_binding = CallbackBinding {
            path: entry_path.to_owned(),
            callback_id: "def.after_set".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-new-vegas-entry-point".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "edit.legacy_perk_entry_point".to_owned(),
                    minimum_version: 1,
                    configuration: new_vegas_configuration,
                },
            },
        };
        let new_vegas_record = WritableRecord {
            signature: Signature(*b"PERK"),
            flags: RecordFlags::empty(),
            form_id: FormId::NULL,
            form_version: 0,
            subrecords: vec![
                subrecord(Signature(*b"PRKE"), vec![2, 0, 0]),
                subrecord(Signature(*b"DATA"), vec![21, 6, 2]),
                subrecord(Signature(*b"EPFT"), vec![0]),
            ],
        };
        let new_vegas_output = handlers.invoke_with_records(
            &new_vegas_binding,
            HandlerRecordContext::new(Signature(*b"PERK"), FormId::NULL, 0, SchemaGame::FalloutNv),
            HandlerInvocationAccess::writable_subrecord_with_scope(&new_vegas_record, 1, None),
            HandlerPhase::AfterSet,
            Some(&FieldValue::Int(0)),
            Some(&FieldValue::Int(21)),
        )?;
        assert!(matches!(
            new_vegas_output,
            HandlerOutput::Mutations(mutations)
                if mutations == [HandlerMutation::Set {
                    path: count_path.to_owned(),
                    occurrence: 0,
                    value: OwnedFieldValue::Int(3),
                }]
        ));
        Ok(())
    }

    /// Removes an existing FO3 head-part model only for an ears part that has an icon.
    #[test]
    fn head_parts_remove_ears_model_with_icon(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let binding = CallbackBinding {
            path: "RACE/14:Head Data/1:Parts/repeat/0:Part".to_owned(),
            callback_id: "def.after_set".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-head-parts".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "edit.head_parts".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({
                        "index_signature": "INDX",
                        "ear_value": 1,
                        "model_fields": [
                            {
                                "path": "RACE/14:Head Data/1:Parts/repeat/0:Part/1:Model/0:File",
                                "signature": "MODL"
                            },
                            {
                                "path": "RACE/14:Head Data/1:Parts/repeat/0:Part/1:Model/1:Bounds",
                                "signature": "MODB"
                            },
                            {
                                "path": concat!(
                                    "RACE/14:Head Data/1:Parts/repeat/0:Part/",
                                    "1:Model/2:Textures"
                                ),
                                "signature": "MODT"
                            }
                        ],
                        "icon_signatures": ["ICON", "MICO"]
                    }),
                },
            },
        };
        let record = WritableRecord {
            signature: Signature(*b"RACE"),
            flags: RecordFlags::empty(),
            form_id: FormId::NULL,
            form_version: 0,
            subrecords: vec![
                bethkit_core::WritableSubRecord {
                    signature: Signature(*b"INDX"),
                    data: 1_u32.to_le_bytes().to_vec(),
                },
                bethkit_core::WritableSubRecord {
                    signature: Signature(*b"MODL"),
                    data: b"ears.nif\0".to_vec(),
                },
                bethkit_core::WritableSubRecord {
                    signature: Signature(*b"MODT"),
                    data: Vec::new(),
                },
                bethkit_core::WritableSubRecord {
                    signature: Signature(*b"ICON"),
                    data: b"ears.dds\0".to_vec(),
                },
            ],
        };
        let output = SemanticHandlerRegistry::builtin().invoke_with_writable_record(
            &binding,
            HandlerRecordContext::new(Signature(*b"RACE"), FormId::NULL, 0, SchemaGame::Fallout3),
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
                    [
                        HandlerMutation::Remove {
                            path: textures,
                            occurrence: 0,
                        },
                        HandlerMutation::Remove {
                            path: model,
                            occurrence: 0,
                        }
                    ] if textures.ends_with("/2:Textures") && model.ends_with("/0:File")
                )
        ));
        Ok(())
    }

    /// Rebuilds repeat-local package input values for every xEdit type transition.
    #[test]
    fn package_input_type_rebuilds_repeat_local_value() -> Result<()> {
        let type_path = "PACK/9:Package Data/0:Data Input Values/repeat/0:Value/0:Type";
        let value_path = "PACK/9:Package Data/0:Data Input Values/repeat/0:Value/1:Value";
        let binding = CallbackBinding {
            path: type_path.to_owned(),
            callback_id: "def.after_set".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-package-input-type".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "edit.package_input_type".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({
                        "type_path": type_path,
                        "value_path": value_path,
                        "type_signature": "ANAM",
                        "value_signature": "CNAM",
                        "value_types": ["Bool", "Int", "Float", "ObjectList"]
                    }),
                },
            },
        };
        let handlers = SemanticHandlerRegistry::builtin();
        let context =
            HandlerRecordContext::new(Signature(*b"TEST"), FormId::NULL, 0, SchemaGame::Morrowind);
        let subrecord = |signature, data| bethkit_core::WritableSubRecord { signature, data };
        let invoke = |subrecords: Vec<bethkit_core::WritableSubRecord>,
                      old_value: &'static str,
                      new_value: &'static str|
         -> Result<HandlerOutput> {
            let record = WritableRecord {
                signature: Signature(*b"PACK"),
                flags: RecordFlags::empty(),
                form_id: FormId::NULL,
                form_version: 0,
                subrecords,
            };
            handlers.invoke_with_records(
                &binding,
                context,
                HandlerInvocationAccess::writable_subrecord_with_scope(&record, 0, None),
                HandlerPhase::AfterSet,
                Some(&FieldValue::String(new_value.into())),
                Some(&FieldValue::String(old_value.into())),
            )
        };

        let reset = invoke(
            vec![
                subrecord(Signature(*b"ANAM"), b"Int\0".to_vec()),
                subrecord(Signature(*b"CNAM"), 42_u32.to_le_bytes().to_vec()),
            ],
            "Int",
            "Float",
        )?;
        assert!(matches!(
            reset,
            HandlerOutput::Mutations(mutations)
                if mutations == [HandlerMutation::ResetToDefault {
                    path: value_path.to_owned(),
                    occurrence: 0,
                }]
        ));

        let insert = invoke(
            vec![subrecord(Signature(*b"ANAM"), b"Target\0".to_vec())],
            "Target",
            "ObjectList",
        )?;
        assert!(matches!(
            insert,
            HandlerOutput::Mutations(mutations)
                if mutations == [HandlerMutation::InsertDefault {
                    path: value_path.to_owned(),
                }]
        ));

        let remove = invoke(
            vec![
                subrecord(Signature(*b"ANAM"), b"Bool\0".to_vec()),
                subrecord(Signature(*b"CNAM"), vec![1]),
            ],
            "Bool",
            "Target",
        )?;
        assert!(matches!(
            remove,
            HandlerOutput::Mutations(mutations)
                if mutations == [HandlerMutation::Remove {
                    path: value_path.to_owned(),
                    occurrence: 0,
                }]
        ));

        assert!(matches!(
            invoke(
                vec![subrecord(Signature(*b"ANAM"), b"Target\0".to_vec())],
                "Target",
                "Location",
            )?,
            HandlerOutput::None
        ));
        assert!(matches!(
            invoke(
                vec![
                    subrecord(Signature(*b"ANAM"), b"Int\0".to_vec()),
                    subrecord(Signature(*b"CNAM"), 42_u32.to_le_bytes().to_vec()),
                ],
                "Int",
                "Int",
            )?,
            HandlerOutput::None
        ));
        Ok(())
    }

    /// Resets quest fragment script data only when ScriptName crosses the empty boundary.
    #[test]
    fn quest_script_name_resets_script_on_presence_transition() -> Result<()> {
        let name_path = "QUST/1:VMAD/payload/3:Script Fragments/2:ScriptName";
        let script_path = "QUST/1:VMAD/payload/3:Script Fragments/3:Script";
        let binding = CallbackBinding {
            path: name_path.to_owned(),
            callback_id: "def.after_set".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-quest-script-name".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "edit.quest_script_name".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({
                        "name_path": name_path,
                        "script_path": script_path
                    }),
                },
            },
        };
        let handlers = SemanticHandlerRegistry::builtin();
        let context =
            HandlerRecordContext::new(Signature(*b"TEST"), FormId::NULL, 0, SchemaGame::Morrowind);
        let invoke = |old_value: &'static str, new_value: &'static str| {
            handlers.invoke(
                &binding,
                context,
                HandlerPhase::AfterSet,
                Some(&FieldValue::String(new_value.into())),
                Some(&FieldValue::String(old_value.into())),
            )
        };

        assert!(matches!(
            invoke("", "QuestScript")?,
            HandlerOutput::Mutations(mutations)
                if mutations == [HandlerMutation::ResetToDefault {
                    path: script_path.to_owned(),
                    occurrence: 0,
                }]
        ));
        assert!(matches!(
            invoke("QuestScript", "")?,
            HandlerOutput::Mutations(_)
        ));
        assert!(matches!(
            invoke("OldScript", "NewScript")?,
            HandlerOutput::None
        ));
        assert!(matches!(
            invoke("SameScript", "SameScript")?,
            HandlerOutput::None
        ));
        Ok(())
    }

    /// Protects a zero MGEF archetype when a non-null associated item is assigned.
    #[test]
    fn magic_effect_assoc_item_protects_unset_archetype() -> Result<()> {
        let handlers = SemanticHandlerRegistry::builtin();
        for (assoc_item_path, archetype_path) in [
            (
                "MGEF/5:Data/payload/2:Assoc. Item",
                "MGEF/5:Data/payload/17:Archtype",
            ),
            (
                "MGEF/6:Magic Effect Data/0:Data/payload/2:Assoc. Item",
                "MGEF/6:Magic Effect Data/0:Data/payload/16:Archtype",
            ),
            (
                "MGEF/5:Magic Effect Data/0:Data/payload/3:Assoc. Item",
                "MGEF/5:Magic Effect Data/0:Data/payload/17:Archetype",
            ),
        ] {
            let binding = CallbackBinding {
                path: assoc_item_path.to_owned(),
                callback_id: "def.after_set".to_owned(),
                callback_slot: None,
                implementation_fingerprint: "test-mgef-assoc-item".to_owned(),
                implementation: CallbackImplementation::BuiltIn {
                    operation: bethkit_schema::BuiltInOperation {
                        id: "edit.magic_effect_assoc_item".to_owned(),
                        minimum_version: 1,
                        configuration: serde_json::json!({
                            "assoc_item_path": assoc_item_path,
                            "archetype_path": archetype_path,
                            "unset_archetype": 0,
                            "generic_archetype": 0xff
                        }),
                    },
                },
            };
            let record = HandlerRecordContext::new(
                Signature(*b"TEST"),
                FormId::NULL,
                0,
                SchemaGame::Morrowind,
            );
            let protected = handlers.invoke(
                &binding,
                record,
                HandlerPhase::AfterSet,
                Some(&FieldValue::FormId {
                    value: FormId(0x1234),
                    targets: Vec::new(),
                }),
                Some(&FieldValue::FormId {
                    value: FormId::NULL,
                    targets: Vec::new(),
                }),
            )?;
            assert!(matches!(
                protected,
                HandlerOutput::Mutations(mutations)
                    if mutations == [HandlerMutation::SetIfEqual {
                        path: archetype_path.to_owned(),
                        occurrence: 0,
                        expected: OwnedFieldValue::Int(0),
                        value: OwnedFieldValue::Int(0xff),
                    }]
            ));

            let null = handlers.invoke(
                &binding,
                record,
                HandlerPhase::AfterSet,
                Some(&FieldValue::FormId {
                    value: FormId::NULL,
                    targets: Vec::new(),
                }),
                Some(&FieldValue::FormId {
                    value: FormId(0x1234),
                    targets: Vec::new(),
                }),
            )?;
            assert!(matches!(null, HandlerOutput::None));
        }
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
                        expected: OwnedFieldValue::Int(0),
                        value: OwnedFieldValue::Int(0xff),
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

    /// Resets MGEF dependent fields with each game's xEdit actor-value mapping.
    #[test]
    fn magic_effect_archetype_resets_dependent_fields() -> Result<()> {
        let binding = |configuration: serde_json::Value| CallbackBinding {
            path: "MGEF/data/archetype".to_owned(),
            callback_id: "def.after_set".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-mgef-archetype".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "edit.magic_effect_archetype".to_owned(),
                    minimum_version: 1,
                    configuration,
                },
            },
        };
        let handlers = SemanticHandlerRegistry::builtin();
        let modern_binding = binding(serde_json::json!({
            "assoc_item_path": "MGEF/data/assoc",
            "actor_value_path": "MGEF/data/actor",
            "second_actor_value_path": "MGEF/data/second_actor",
            "second_av_weight_path": "MGEF/data/second_weight"
        }));
        let modern_record =
            HandlerRecordContext::new(Signature(*b"MGEF"), FormId::NULL, 44, SchemaGame::SkyrimSe);
        let modern = handlers.invoke(
            &modern_binding,
            modern_record,
            HandlerPhase::AfterSet,
            Some(&FieldValue::Enumeration {
                value: 11,
                name: None,
            }),
            Some(&FieldValue::Enumeration {
                value: 0,
                name: None,
            }),
        )?;
        let HandlerOutput::Mutations(modern) = modern else {
            return Err(SemanticError::Handler {
                handler: "edit.magic_effect_archetype".to_owned(),
                message: "modern callback did not return mutations".to_owned(),
            });
        };
        assert!(matches!(
            modern.as_slice(),
            [
                HandlerMutation::Set {
                    value: OwnedFieldValue::Bytes(bytes),
                    ..
                },
                HandlerMutation::Set {
                    value: OwnedFieldValue::Int(54),
                    ..
                },
                HandlerMutation::Set {
                    value: OwnedFieldValue::Int(-1),
                    ..
                },
                HandlerMutation::Set {
                    value: OwnedFieldValue::Float(weight),
                    ..
                }
            ] if bytes == &[0, 0, 0, 0] && *weight == 0.0
        ));

        let legacy_binding = binding(serde_json::json!({
            "assoc_item_path": "MGEF/data/assoc",
            "actor_value_path": "MGEF/data/actor"
        }));
        let legacy_record =
            HandlerRecordContext::new(Signature(*b"MGEF"), FormId::NULL, 15, SchemaGame::FalloutNv);
        let legacy = handlers.invoke(
            &legacy_binding,
            legacy_record,
            HandlerPhase::AfterSet,
            Some(&FieldValue::Enumeration {
                value: 36,
                name: None,
            }),
            Some(&FieldValue::Enumeration {
                value: 0,
                name: None,
            }),
        )?;
        assert!(matches!(
            legacy,
            HandlerOutput::Mutations(mutations)
                if matches!(
                    mutations.as_slice(),
                    [
                        HandlerMutation::Set {
                            value: OwnedFieldValue::Bytes(_),
                            ..
                        },
                        HandlerMutation::Set {
                            value: OwnedFieldValue::Int(51),
                            ..
                        }
                    ]
                )
        ));

        let protected = handlers.invoke(
            &legacy_binding,
            legacy_record,
            HandlerPhase::AfterSet,
            Some(&FieldValue::Enumeration {
                value: 1,
                name: None,
            }),
            Some(&FieldValue::Enumeration {
                value: 255,
                name: None,
            }),
        )?;
        assert!(matches!(protected, HandlerOutput::None));
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

        let oblivion = FieldValue::UInt(0xA7);
        let oblivion_record =
            HandlerRecordContext::new(Signature(*b"TEST"), FormId::NULL, 0, SchemaGame::Oblivion);
        assert!(matches!(
            handlers.invoke(
                &binding,
                oblivion_record,
                HandlerPhase::Display,
                Some(&oblivion),
                None,
            )?,
            HandlerOutput::Text(text)
                if text == "Less than or equal to / Or, Run on target, Use global"
        ));
        assert!(matches!(
            handlers.invoke(
                &binding,
                oblivion_record,
                HandlerPhase::EditValue,
                Some(&oblivion),
                None,
            )?,
            HandlerOutput::Text(text) if text == "167"
        ));
        let oblivion_edit = FieldValue::String(Cow::Borrowed("167"));
        assert!(matches!(
            handlers.invoke(
                &binding,
                oblivion_record,
                HandlerPhase::ParseEditValue,
                Some(&oblivion_edit),
                None,
            )?,
            HandlerOutput::Value(FieldValue::UInt(0xA7))
        ));
        let invalid = FieldValue::UInt(0x18);
        assert!(matches!(
            handlers.invoke(
                &binding,
                oblivion_record,
                HandlerPhase::Validation,
                Some(&invalid),
                None,
            )?,
            HandlerOutput::Text(text)
                if text == "<Unknown Compare operator> / <Unknown: 3>"
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
                        "unknown_validation": "angle",
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
        assert!(matches!(
            handlers.invoke(
                &binding,
                record,
                HandlerPhase::Validation,
                Some(&known),
                None,
            )?,
            HandlerOutput::Text(text) if text.is_empty()
        ));
        assert!(matches!(
            handlers.invoke(
                &binding,
                record,
                HandlerPhase::Validation,
                Some(&unknown),
                None,
            )?,
            HandlerOutput::Text(text) if text == "<Unknown: 9>"
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
        let hexadecimal_edit = FieldValue::String(Cow::Borrowed("$7B"));
        assert!(matches!(
            handlers.invoke(
                &binding,
                record,
                HandlerPhase::ParseEditValue,
                Some(&hexadecimal_edit),
                None,
            )?,
            HandlerOutput::Value(FieldValue::Int(123))
        ));
        Ok(())
    }

    /// Matches xEdit's paired Starfield event function/member formatter.
    #[test]
    fn event_function_member_formatter_preserves_each_component() -> Result<()> {
        let binding = CallbackBinding {
            path: "TEST/value".to_owned(),
            callback_id: "integer.formatter".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-event-function-member".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "format.event_function_member".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({
                        "values": [
                            { "value": 0, "name": "GetIsID:None" },
                            { "value": 4, "name": "GetItemValue:None" },
                            { "value": 826671104, "name": "GetIsID:Form" },
                            { "value": -65536, "name": "GetIsID:-1" }
                        ]
                    }),
                },
            },
        };
        let handlers = SemanticHandlerRegistry::builtin();
        let record =
            HandlerRecordContext::new(Signature(*b"TEST"), FormId::NULL, 0, SchemaGame::Starfield);
        let known = FieldValue::UInt(0x3146_0004);
        for phase in [
            HandlerPhase::Display,
            HandlerPhase::Summary,
            HandlerPhase::EditValue,
        ] {
            assert!(matches!(
                handlers.invoke(&binding, record, phase, Some(&known), None)?,
                HandlerOutput::Text(text) if text == "GetItemValue:Form"
            ));
        }
        assert!(matches!(
            handlers.invoke(
                &binding,
                record,
                HandlerPhase::SortKey,
                Some(&known),
                None,
            )?,
            HandlerOutput::Text(text) if text == "31460004"
        ));
        assert!(matches!(
            handlers.invoke(
                &binding,
                record,
                HandlerPhase::Validation,
                Some(&known),
                None,
            )?,
            HandlerOutput::Text(text) if text.is_empty()
        ));

        let unknown = FieldValue::UInt(0x1234_0009);
        assert!(matches!(
            handlers.invoke(
                &binding,
                record,
                HandlerPhase::Display,
                Some(&unknown),
                None,
            )?,
            HandlerOutput::Text(text) if text == "9:4660"
        ));
        assert!(matches!(
            handlers.invoke(
                &binding,
                record,
                HandlerPhase::Validation,
                Some(&unknown),
                None,
            )?,
            HandlerOutput::Text(text)
                if text == "EventFunction<Unknown: 9>:EventMember<Unknown: 4660>"
        ));

        let named_edit = FieldValue::String(Cow::Borrowed("getitemvalue:form"));
        assert!(matches!(
            handlers.invoke(
                &binding,
                record,
                HandlerPhase::ParseEditValue,
                Some(&named_edit),
                None,
            )?,
            HandlerOutput::Value(FieldValue::UInt(0x3146_0004))
        ));
        let negative_edit = FieldValue::String(Cow::Borrowed("GetIsID:-1"));
        assert!(matches!(
            handlers.invoke(
                &binding,
                record,
                HandlerPhase::ParseEditValue,
                Some(&negative_edit),
                None,
            )?,
            HandlerOutput::Value(FieldValue::Int(-65_536))
        ));
        let no_separator = FieldValue::String(Cow::Borrowed("GetItemValue"));
        assert!(matches!(
            handlers.invoke(
                &binding,
                record,
                HandlerPhase::ParseEditValue,
                Some(&no_separator),
                None,
            )?,
            HandlerOutput::Value(FieldValue::UInt(0))
        ));

        let numeric_edit_value = FieldValue::UInt(0xFFFF_0000);
        assert!(matches!(
            handlers.invoke(
                &binding,
                record,
                HandlerPhase::Display,
                Some(&numeric_edit_value),
                None,
            )?,
            HandlerOutput::Text(text) if text == "GetIsID:65535"
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

    /// Synchronizes every mismatched array count in one structural container.
    #[test]
    fn synchronize_container_counts_updates_only_mismatched_fields() -> Result<()> {
        let binding = CallbackBinding {
            path: "OMOD/4:Data".to_owned(),
            callback_id: "def.after_set".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-container-counts".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "edit.sync_container_counts".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({
                        "counters": [
                            {
                                "counter_path": "OMOD/4:Data/payload/0:Include Count",
                                "value_path": "OMOD/4:Data/payload/2:Includes"
                            },
                            {
                                "counter_path": "OMOD/4:Data/payload/1:Property Count",
                                "value_path": "OMOD/4:Data/payload/3:Properties"
                            }
                        ]
                    }),
                },
            },
        };
        let named = |id, path: &str, value| crate::NamedValue {
            node_id: bethkit_schema::SchemaNodeId(id),
            path: path.to_owned(),
            effective_path: None,
            name: path.to_owned(),
            span: crate::ByteSpan { start: 0, end: 0 },
            value,
        };
        let value = FieldValue::Struct(vec![
            named(
                1,
                "OMOD/4:Data/payload/0:Include Count",
                FieldValue::UInt(2),
            ),
            named(
                2,
                "OMOD/4:Data/payload/1:Property Count",
                FieldValue::UInt(0),
            ),
            named(
                3,
                "OMOD/4:Data/payload/2:Includes",
                FieldValue::Array(vec![FieldValue::UInt(1), FieldValue::UInt(2)]),
            ),
            named(
                4,
                "OMOD/4:Data/payload/3:Properties",
                FieldValue::Array(vec![FieldValue::UInt(3)]),
            ),
        ]);

        let output = SemanticHandlerRegistry::builtin().invoke(
            &binding,
            HandlerRecordContext::new(Signature(*b"OMOD"), FormId::NULL, 0, SchemaGame::Fallout4),
            HandlerPhase::AfterSet,
            Some(&value),
            None,
        )?;

        assert!(matches!(
            output,
            HandlerOutput::Mutations(mutations)
                if mutations == [HandlerMutation::Set {
                    path: "OMOD/4:Data/payload/1:Property Count".to_owned(),
                    occurrence: 0,
                    value: OwnedFieldValue::UInt(1),
                }]
        ));
        Ok(())
    }

    /// Rejects a container-counter rule whose configured value is not an array.
    #[test]
    fn synchronize_container_counts_rejects_non_array_values() {
        let binding = CallbackBinding {
            path: "OMOD/4:Data".to_owned(),
            callback_id: "def.after_set".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "test-container-count-error".to_owned(),
            implementation: CallbackImplementation::BuiltIn {
                operation: bethkit_schema::BuiltInOperation {
                    id: "edit.sync_container_counts".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({
                        "counters": [{
                            "counter_path": "OMOD/4:Data/payload/0:Count",
                            "value_path": "OMOD/4:Data/payload/1:Values"
                        }]
                    }),
                },
            },
        };
        let value = FieldValue::Struct(vec![
            crate::NamedValue {
                node_id: bethkit_schema::SchemaNodeId(1),
                path: "OMOD/4:Data/payload/0:Count".to_owned(),
                effective_path: None,
                name: "Count".to_owned(),
                span: crate::ByteSpan { start: 0, end: 4 },
                value: FieldValue::UInt(0),
            },
            crate::NamedValue {
                node_id: bethkit_schema::SchemaNodeId(2),
                path: "OMOD/4:Data/payload/1:Values".to_owned(),
                effective_path: None,
                name: "Values".to_owned(),
                span: crate::ByteSpan { start: 4, end: 8 },
                value: FieldValue::UInt(1),
            },
        ]);

        let error = SemanticHandlerRegistry::builtin()
            .invoke(
                &binding,
                HandlerRecordContext::new(
                    Signature(*b"OMOD"),
                    FormId::NULL,
                    0,
                    SchemaGame::Fallout4,
                ),
                HandlerPhase::AfterSet,
                Some(&value),
                None,
            )
            .expect_err("non-array container values must fail");

        assert!(error.to_string().contains("is not an array"));
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

    /// Derives xEdit worldspace offset columns from NAM0 and NAM9 X bounds.
    #[test]
    fn worldspace_offset_column_count_matches_xedit_bounds() -> TestResult {
        // given
        let binding = test_metadata_binding(
            "array.count",
            "array.worldspace_offset_columns",
            serde_json::json!({}),
        );
        let handlers = SemanticHandlerRegistry::builtin();

        // when / then
        for (game, min_x, max_x, expected) in [
            (SchemaGame::SkyrimSe, -8192.0_f32, 4096.0_f32, 4_i64),
            (SchemaGame::SkyrimSe, 2048.0, 6144.0, 3),
            (SchemaGame::Starfield, -100.0, 200.0, 4),
        ] {
            let record = test_record(
                *b"WRLD",
                &[
                    (*b"NAM0", [min_x.to_le_bytes(), [0; 4]].concat()),
                    (*b"NAM9", [max_x.to_le_bytes(), [0; 4]].concat()),
                    (*b"OFST", vec![0; 16]),
                ],
            )?;
            let context = HandlerRecordContext::new(Signature(*b"WRLD"), FormId::NULL, 0, game);
            assert!(matches!(
                handlers.invoke_with_subrecord(
                    &binding,
                    context,
                    HandlerSubrecordSource::ReadOnly {
                        record: &record,
                        index: 2,
                    },
                    HandlerPhase::ArrayCount,
                    None,
                    None,
                )?,
                HandlerOutput::Integer(count) if count == expected
            ));
        }
        Ok(())
    }

    /// Reads Oblivion PGRR group sizes from each matching PGRP point.
    #[test]
    fn oblivion_path_grid_connection_counts_follow_outer_array_index() -> TestResult {
        // given
        let binding = test_metadata_binding(
            "array.count",
            "array.oblivion_path_grid_connections",
            serde_json::json!({}),
        );
        let mut points = vec![0_u8; 48];
        points[12] = 2;
        points[28] = 5;
        points[44] = 1;
        let record = test_record(*b"PGRD", &[(*b"PGRP", points), (*b"PGRR", vec![0; 16])])?;
        let context =
            HandlerRecordContext::new(Signature(*b"PGRD"), FormId::NULL, 0, SchemaGame::Oblivion);
        let handlers = SemanticHandlerRegistry::builtin();

        // when / then
        for (point_index, expected) in [2_i64, 5, 1].into_iter().enumerate() {
            let indices = [point_index];
            let output = handlers.invoke_with_records(
                &binding,
                context,
                HandlerInvocationAccess::read_only_subrecord_with_scope(&record, 1, None)
                    .with_array_indices(&indices),
                HandlerPhase::ArrayCount,
                None,
                None,
            )?;
            assert!(matches!(
                output,
                HandlerOutput::Integer(actual) if actual == expected
            ));
        }
        Ok(())
    }

    /// Includes Starfield LGDI elements only in their encoded star-slot group.
    #[test]
    fn star_slot_array_inclusion_matches_outer_array_index() -> TestResult {
        let binding = test_metadata_binding(
            "array.should_include",
            "array.star_slot_matches_outer_index",
            serde_json::json!({}),
        );
        let value = FieldValue::Bytes(Cow::Borrowed(&[2, 0, 0, 0, 99]));
        let indices = [2_usize];

        let output = StarSlotArrayElementInclusion.invoke(HandlerInvocation {
            context: HandlerContext {
                binding: &binding,
                record_signature: Signature(*b"LGDI"),
                form_id: FormId::NULL,
                form_version: 0,
                game: SchemaGame::Starfield,
                plugin_localized: false,
                configuration: match &binding.implementation {
                    CallbackImplementation::BuiltIn { operation } => &operation.configuration,
                    _ => unreachable!("test binding is built-in"),
                },
            },
            phase: HandlerPhase::ArrayElementInclusion,
            value: Some(&value),
            old_value: None,
            value_scope: None,
            source_record: None,
            source_writable_record: None,
            source_subrecord_index: None,
            array_indices: &indices,
        })?;

        assert!(matches!(output, HandlerOutput::Integer(1)));
        Ok(())
    }

    /// Initializes Starfield LGDI star slots from their outer group position.
    #[test]
    fn star_slot_default_uses_outer_array_index() -> TestResult {
        let binding = test_metadata_binding(
            "value.set_default",
            "default.star_slot_outer_index",
            serde_json::json!({}),
        );
        let indices = [3_usize, 7_usize];
        let value = FieldValue::Enumeration {
            value: 0,
            name: Some("First Star Slot".to_owned()),
        };

        let output = StarSlotDefaultValue.invoke(HandlerInvocation {
            context: HandlerContext {
                binding: &binding,
                record_signature: Signature(*b"LGDI"),
                form_id: FormId::NULL,
                form_version: 0,
                game: SchemaGame::Starfield,
                plugin_localized: false,
                configuration: match &binding.implementation {
                    CallbackImplementation::BuiltIn { operation } => &operation.configuration,
                    _ => unreachable!("test binding is built-in"),
                },
            },
            phase: HandlerPhase::DefaultValue,
            value: Some(&value),
            old_value: None,
            value_scope: None,
            source_record: None,
            source_writable_record: None,
            source_subrecord_index: None,
            array_indices: &indices,
        })?;

        assert!(matches!(
            output,
            HandlerOutput::Value(FieldValue::Enumeration { value: 3, .. })
        ));
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
                plugin_localized: false,
                configuration: match &binding.implementation {
                    CallbackImplementation::BuiltIn { operation } => &operation.configuration,
                    _ => unreachable!("test binding is built-in"),
                },
            },
            phase: HandlerPhase::Validation,
            value: None,
            old_value: None,
            value_scope: None,
            source_record: None,
            source_writable_record: None,
            source_subrecord_index: None,
            array_indices: &[],
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
                plugin_localized: false,
                configuration: match &binding.implementation {
                    CallbackImplementation::BuiltIn { operation } => &operation.configuration,
                    _ => unreachable!("test binding is built-in"),
                },
            },
            phase,
            value: Some(&value),
            old_value: None,
            value_scope: None,
            source_record: None,
            source_writable_record: None,
            source_subrecord_index: None,
            array_indices: &[],
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
                plugin_localized: false,
                configuration: match &binding.implementation {
                    CallbackImplementation::BuiltIn { operation } => &operation.configuration,
                    _ => unreachable!("test binding is built-in"),
                },
            },
            phase,
            value: Some(value),
            old_value: None,
            value_scope: None,
            source_record: None,
            source_writable_record: None,
            source_subrecord_index: None,
            array_indices: &[],
        })
    }

    const fn test_wwise_guid() -> [u8; 16] {
        [
            0x33, 0x22, 0x11, 0x00, 0x55, 0x44, 0x77, 0x66, 0x88, 0x99, 0xAA, 0xBB, 0xCC, 0xDD,
            0xEE, 0xFF,
        ]
    }
}
