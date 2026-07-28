// SPDX-License-Identifier: Apache-2.0
//!
//! Versioned semantic callback handlers and built-in xEdit operations.

use std::collections::BTreeMap;
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
    /// Reset a field occurrence to the schema-native default selected in the current edit context.
    ResetToDefault {
        /// Stable schema path.
        path: String,
        /// Zero-based occurrence.
        occurrence: usize,
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
        registry.register(Arc::new(CtdaRunOnAfterSet));
        registry.register(Arc::new(CtdaTypeAfterSet));
        registry.register(Arc::new(MessageDisplayTimeAfterSet));
        registry.register(Arc::new(FormListEditorIdAfterSet));
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

    /// Installs the load-order resolver used by FormID-dependent summaries.
    pub fn set_form_link_resolver(&mut self, resolver: Arc<dyn FormLinkResolver>) {
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
        self.register(Arc::new(SelectCoedOwner {
            resolver: Some(resolver),
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
        self.register(Arc::new(SelectCtdaParameter {
            table: Some(Arc::clone(&table)),
        }));
        self.register(Arc::new(CtdaFunctionFormatter { table: Some(table) }));
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
