// SPDX-License-Identifier: Apache-2.0
//!
//! Versioned semantic callback handlers and built-in xEdit operations.

use std::collections::BTreeMap;
use std::sync::Arc;

use bethkit_core::{FormId, Signature};
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
    /// Record index keys.
    IndexKeys(Vec<String>),
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
        registry.register(Arc::new(FormatRgb));
        registry.register(Arc::new(RemovableWhenZero));
        registry.register(Arc::new(ResourceHashFormatter { resolver: None }));
        registry.register(Arc::new(ModelInfoCounts));
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
        })
    }

    const fn test_wwise_guid() -> [u8; 16] {
        [
            0x33, 0x22, 0x11, 0x00, 0x55, 0x44, 0x77, 0x66, 0x88, 0x99, 0xAA, 0xBB, 0xCC, 0xDD,
            0xEE, 0xFF,
        ]
    }
}
