// SPDX-License-Identifier: Apache-2.0
//!
//! Semantic runtime context tying packages to decoder implementations.

use std::borrow::Cow;
use std::sync::Arc;

use bethkit_core::{FormId, Record};
use bethkit_schema::{
    CallbackBinding, CallbackImplementation, ConflictPriority, PrimitiveType, SchemaNodeKind,
    SchemaPackage, SchemaRegistry,
};

use crate::{value::handler_to_owned_value, OwnedFieldValue};
use crate::{
    DecoderRegistry, FieldValue, HandlerOutput, HandlerPhase, HandlerRecordContext, RecordEditor,
    RecordGridCell, RecordIndexKey, RecordView, Result, SemanticError, SemanticHandlerRegistry,
    ValueFormat,
};

/// Runtime context for schema-guided operations on one game mode.
pub struct SemanticContext {
    registry: SchemaRegistry,
    decoders: DecoderRegistry,
    handlers: SemanticHandlerRegistry,
}

impl SemanticContext {
    /// Creates a context and verifies all package decoder requirements.
    ///
    /// # Errors
    ///
    /// Returns [`crate::SemanticError::MissingDecoder`] when a required
    /// decoder is unavailable or too old.
    pub fn new(package: Arc<SchemaPackage>, decoders: DecoderRegistry) -> Result<Self> {
        Self::new_with_handlers(package, decoders, SemanticHandlerRegistry::builtin())
    }

    /// Creates a context with caller-provided decoder and handler registries.
    ///
    /// # Errors
    ///
    /// Returns [`crate::SemanticError::MissingDecoder`] or
    /// [`crate::SemanticError::MissingHandler`] when a package requirement
    /// is unavailable or too old.
    pub fn new_with_handlers(
        package: Arc<SchemaPackage>,
        decoders: DecoderRegistry,
        handlers: SemanticHandlerRegistry,
    ) -> Result<Self> {
        for requirement in &package.manifest().required_decoders {
            decoders.require(&requirement.id, requirement.minimum_version)?;
        }
        for requirement in &package.manifest().required_handlers {
            handlers.require(&requirement.id, requirement.minimum_version)?;
        }
        Ok(Self {
            registry: SchemaRegistry::new(package),
            decoders,
            handlers,
        })
    }

    /// Creates a read-only semantic view over a record.
    ///
    /// # Errors
    ///
    /// Returns [`crate::SemanticError::MissingRecordSchema`] when no schema
    /// exists for the record signature.
    pub fn view<'context, 'record>(
        &'context self,
        record: &'record Record,
        plugin_localized: bool,
    ) -> Result<RecordView<'context, 'record>> {
        RecordView::new(self, record, plugin_localized)
    }

    /// Creates a lossless semantic editor over a record.
    ///
    /// # Errors
    ///
    /// Returns [`crate::SemanticError`] when the record subrecords cannot be
    /// parsed or its schema is unavailable.
    pub fn edit(&self, record: &Record, plugin_localized: bool) -> Result<RecordEditor> {
        RecordEditor::new(self, record, plugin_localized)
    }

    /// Returns the package registry.
    pub fn registry(&self) -> &SchemaRegistry {
        &self.registry
    }

    /// Returns the custom decoder registry.
    pub fn decoders(&self) -> &DecoderRegistry {
        &self.decoders
    }

    /// Returns the semantic callback handler registry.
    pub fn handlers(&self) -> &SemanticHandlerRegistry {
        &self.handlers
    }

    /// Returns the effective xEdit conflict priority for an exact schema path.
    ///
    /// # Errors
    ///
    /// Returns [`SemanticError::MissingPath`] when the path is not part of the
    /// record schema, or [`SemanticError::Handler`] when a dynamic priority
    /// callback returns an invalid result.
    pub fn conflict_priority(&self, record: &Record, path: &str) -> Result<ConflictPriority> {
        self.conflict_priority_with_value(record, path, None)
    }

    /// Returns the effective xEdit conflict priority with a decoded value.
    ///
    /// Value-dependent callbacks, including ignore-empty rules, require this
    /// method instead of [`Self::conflict_priority`].
    ///
    /// # Errors
    ///
    /// Returns [`SemanticError::MissingPath`] when the path is not part of the
    /// record schema, or [`SemanticError::Handler`] when a dynamic priority
    /// callback rejects the value or returns an invalid result.
    pub fn conflict_priority_for_value(
        &self,
        record: &Record,
        path: &str,
        value: &FieldValue<'_>,
    ) -> Result<ConflictPriority> {
        let handler_value = value.to_handler_value();
        self.conflict_priority_with_value(record, path, Some(&handler_value))
    }

    fn conflict_priority_with_value(
        &self,
        record: &Record,
        path: &str,
        value: Option<&FieldValue<'static>>,
    ) -> Result<ConflictPriority> {
        let node = self
            .registry
            .get_node(record.header.signature, path)
            .ok_or_else(|| SemanticError::MissingPath(path.to_owned()))?;
        let mut priority = node.conflict_priority;
        for binding in self
            .registry
            .package()
            .callback_bindings()
            .iter()
            .filter(|binding| {
                binding.path == path && binding.callback_id == "def.conflict_priority"
            })
        {
            if !matches!(
                binding.implementation,
                CallbackImplementation::BuiltIn { .. }
                    | CallbackImplementation::CustomHandler { .. }
            ) {
                continue;
            }
            priority = match self.handlers.invoke_with_source_record(
                binding,
                self.handler_record(record),
                Some(record),
                HandlerPhase::Conflict,
                value,
                None,
            )? {
                HandlerOutput::ConflictPriority(priority) => priority,
                _ => {
                    return Err(SemanticError::Handler {
                        handler: binding.callback_id.clone(),
                        message: "conflict-priority callback returned an invalid result".to_owned(),
                    });
                }
            };
        }
        Ok(priority)
    }

    /// Formats a typed value with the xEdit presentation callback bound to its path.
    ///
    /// The original value is never replaced. `None` means that the path has no
    /// executable value formatter.
    ///
    /// # Errors
    ///
    /// Returns [`SemanticError::Handler`] when the formatter rejects the value
    /// or returns a non-text result.
    pub fn format_value(
        &self,
        record: &Record,
        path: &str,
        value: &FieldValue<'_>,
    ) -> Result<Option<String>> {
        self.format_value_as(record, path, value, ValueFormat::Display)
    }

    /// Formats a typed value using one explicit xEdit presentation mode.
    ///
    /// The original value is never replaced. `None` means that the path has no
    /// executable formatter for the selected mode.
    ///
    /// # Errors
    ///
    /// Returns [`SemanticError::Handler`] when the formatter rejects the value
    /// or returns a non-text result.
    pub fn format_value_as(
        &self,
        record: &Record,
        path: &str,
        value: &FieldValue<'_>,
        format: ValueFormat,
    ) -> Result<Option<String>> {
        let mut handler_value = value.to_handler_value();
        let mut formatted = self.format_string_enumeration(record, path, value, format);
        if let Some(text) = &formatted {
            handler_value = FieldValue::String(Cow::Owned(text.clone()));
        }
        for binding in self
            .registry
            .package()
            .callback_bindings()
            .iter()
            .filter(|binding| {
                binding.path == path
                    && !crate::handler::is_validation_binding(binding)
                    && matches!(
                        binding.callback_id.as_str(),
                        "def.value_transform" | "integer.formatter" | "string.formatter"
                    )
            })
        {
            if !matches!(
                binding.implementation,
                CallbackImplementation::BuiltIn { .. }
                    | CallbackImplementation::CustomHandler { .. }
            ) {
                continue;
            }
            let output = self.handlers.invoke(
                binding,
                self.handler_record(record),
                format.into(),
                Some(&handler_value),
                None,
            )?;
            let text = match output {
                HandlerOutput::None => continue,
                HandlerOutput::Text(value) => value,
                _ => {
                    return Err(SemanticError::Handler {
                        handler: binding.callback_id.clone(),
                        message: "value formatter returned a non-text result".to_owned(),
                    });
                }
            };
            handler_value = FieldValue::String(Cow::Owned(text.clone()));
            formatted = Some(text);
        }
        Ok(formatted)
    }

    /// Parses text accepted by an xEdit edit control back to a typed value.
    ///
    /// `None` means that the path has no executable text-to-value transform.
    ///
    /// # Errors
    ///
    /// Returns [`SemanticError::Handler`] when a bound transform rejects the
    /// input or returns an invalid result.
    pub fn parse_edit_value(
        &self,
        record: &Record,
        path: &str,
        text: &str,
    ) -> Result<Option<OwnedFieldValue>> {
        let input = FieldValue::String(Cow::Owned(text.to_owned()));
        let mut parsed = self
            .string_enumeration(record, path)
            .map(|_| OwnedFieldValue::String(text.to_owned()));
        for binding in self
            .registry
            .package()
            .callback_bindings()
            .iter()
            .filter(|binding| {
                binding.path == path
                    && matches!(
                        binding.callback_id.as_str(),
                        "def.value_transform" | "integer.formatter" | "string.formatter"
                    )
                    && !crate::handler::is_validation_binding(binding)
            })
        {
            if !matches!(
                binding.implementation,
                CallbackImplementation::BuiltIn { .. }
                    | CallbackImplementation::CustomHandler { .. }
            ) {
                continue;
            }
            match self.handlers.invoke(
                binding,
                self.handler_record(record),
                HandlerPhase::ParseEditValue,
                Some(&input),
                None,
            )? {
                HandlerOutput::None => {}
                HandlerOutput::Value(value) => {
                    parsed = Some(handler_to_owned_value(value, path)?);
                }
                _ => {
                    return Err(SemanticError::Handler {
                        handler: binding.callback_id.clone(),
                        message: "edit parser returned a non-value result".to_owned(),
                    });
                }
            }
        }
        Ok(parsed)
    }

    fn string_enumeration<'a>(&'a self, record: &Record, path: &str) -> Option<&'a [String]> {
        let node = self.registry.get_node(record.header.signature, path)?;
        match &node.kind {
            SchemaNodeKind::Primitive {
                primitive: PrimitiveType::String { string },
            } if !string.allowed_values.is_empty() => Some(&string.allowed_values),
            _ => None,
        }
    }

    fn format_string_enumeration(
        &self,
        record: &Record,
        path: &str,
        value: &FieldValue<'_>,
        format: ValueFormat,
    ) -> Option<String> {
        let allowed_values = self.string_enumeration(record, path)?;
        let FieldValue::String(value) = value else {
            return None;
        };
        Some(format_string_enumeration_value(
            allowed_values,
            value,
            format,
        ))
    }

    /// Returns whether xEdit allows the value at an exact schema path to be removed.
    ///
    /// Values without a removability callback are removable by default.
    ///
    /// # Errors
    ///
    /// Returns [`SemanticError::Handler`] when a bound callback rejects the
    /// value or returns a non-boolean result.
    pub fn is_removable(
        &self,
        record: &Record,
        path: &str,
        value: &FieldValue<'_>,
    ) -> Result<bool> {
        let handler_value = value.to_handler_value();
        let mut removable = true;
        for binding in self
            .registry
            .package()
            .callback_bindings()
            .iter()
            .filter(|binding| binding.path == path && binding.callback_id == "def.is_removeable")
        {
            if !matches!(
                binding.implementation,
                CallbackImplementation::BuiltIn { .. }
                    | CallbackImplementation::CustomHandler { .. }
            ) {
                continue;
            }
            removable = match self.handlers.invoke(
                binding,
                self.handler_record(record),
                HandlerPhase::Removability,
                Some(&handler_value),
                None,
            )? {
                HandlerOutput::Boolean(value) => value,
                _ => {
                    return Err(SemanticError::Handler {
                        handler: binding.callback_id.clone(),
                        message: "removability callback returned a non-boolean result".to_owned(),
                    });
                }
            };
        }
        Ok(removable)
    }

    /// Returns xEdit's dynamic sorting decision for a subrecord-array path.
    ///
    /// `None` means the schema path has no dynamic `is_sorted` callback; the
    /// caller must then use the array's static schema ordering metadata.
    ///
    /// # Errors
    ///
    /// Returns [`SemanticError::Handler`] when the callback is not executable,
    /// is bound more than once, or returns a non-boolean result.
    pub fn array_is_sorted(&self, record: &Record, path: &str) -> Result<Option<bool>> {
        let mut decision = None;
        for binding in self
            .registry
            .package()
            .callback_bindings()
            .iter()
            .filter(|binding| {
                binding.path == path && binding.callback_id == "subrecord_array.is_sorted"
            })
        {
            if decision.is_some() {
                return Err(SemanticError::Handler {
                    handler: binding.callback_id.clone(),
                    message: "array sorting callback is bound more than once".to_owned(),
                });
            }
            if !matches!(
                binding.implementation,
                CallbackImplementation::BuiltIn { .. }
                    | CallbackImplementation::CustomHandler { .. }
            ) {
                return Err(SemanticError::Handler {
                    handler: binding.callback_id.clone(),
                    message: "array sorting callback is not executable".to_owned(),
                });
            }
            decision = Some(
                match self.handlers.invoke_with_source_record(
                    binding,
                    self.handler_record(record),
                    Some(record),
                    HandlerPhase::RecordMetadata,
                    None,
                    None,
                )? {
                    HandlerOutput::Boolean(value) => value,
                    _ => {
                        return Err(SemanticError::Handler {
                            handler: binding.callback_id.clone(),
                            message: "array sorting callback returned a non-boolean result"
                                .to_owned(),
                        });
                    }
                },
            );
        }
        Ok(decision)
    }

    /// Returns xEdit's dynamic grid coordinates for a main record.
    ///
    /// `None` means the record has no grid callback or the callback reports
    /// that the record has no exterior grid position.
    ///
    /// # Errors
    ///
    /// Returns [`SemanticError::Handler`] when the callback is duplicated,
    /// not executable, or returns an invalid result.
    pub fn record_grid_cell(&self, record: &Record) -> Result<Option<RecordGridCell>> {
        let Some(binding) = self.record_metadata_binding(record, "record.grid_cell")? else {
            return Ok(None);
        };
        match self.invoke_record_metadata(record, binding)? {
            HandlerOutput::GridCell(value) => Ok(Some(value)),
            HandlerOutput::None => Ok(None),
            _ => Err(invalid_record_metadata_output(
                binding,
                "grid-cell callback returned a non-grid result",
            )),
        }
    }

    /// Returns xEdit's dynamic FormID for a main record.
    ///
    /// `None` means the record has no FormID callback or the callback cannot
    /// derive a FormID from the current record data.
    ///
    /// # Errors
    ///
    /// Returns [`SemanticError::Handler`] when the callback is duplicated,
    /// not executable, or returns an invalid result.
    pub fn record_form_id(&self, record: &Record) -> Result<Option<FormId>> {
        let Some(binding) = self.record_metadata_binding(record, "record.form_id")? else {
            return Ok(None);
        };
        match self.invoke_record_metadata(record, binding)? {
            HandlerOutput::FormId(value) => Ok(Some(value)),
            HandlerOutput::None => Ok(None),
            _ => Err(invalid_record_metadata_output(
                binding,
                "FormID callback returned a non-FormID result",
            )),
        }
    }

    /// Returns xEdit's dynamic identity string for a main record.
    ///
    /// `None` means the record has no identity callback.
    ///
    /// # Errors
    ///
    /// Returns [`SemanticError::Handler`] when the callback is duplicated,
    /// not executable, or returns an invalid result.
    pub fn record_identity(&self, record: &Record) -> Result<Option<String>> {
        let Some(binding) = self.record_metadata_binding(record, "record.identity")? else {
            return Ok(None);
        };
        match self.invoke_record_metadata(record, binding)? {
            HandlerOutput::Text(value) => Ok(Some(value)),
            _ => Err(invalid_record_metadata_output(
                binding,
                "identity callback returned a non-text result",
            )),
        }
    }

    /// Returns xEdit's dynamic editor ID for a main record.
    ///
    /// `None` means the record has no custom editor-ID getter.
    ///
    /// # Errors
    ///
    /// Returns [`SemanticError::Handler`] when the callback is duplicated,
    /// not executable, or returns an invalid result.
    pub fn record_editor_id(&self, record: &Record) -> Result<Option<String>> {
        let Some(binding) = self.record_metadata_binding(record, "record.get_editor_id")? else {
            return Ok(None);
        };
        match self.invoke_record_metadata(record, binding)? {
            HandlerOutput::Text(value) => Ok(Some(value)),
            _ => Err(invalid_record_metadata_output(
                binding,
                "editor-ID callback returned a non-text result",
            )),
        }
    }

    /// Returns xEdit's dynamic named index keys for a main record.
    ///
    /// `None` means the record has no custom index-key callback. An empty
    /// vector means the callback ran but the current record contributes no key.
    ///
    /// # Errors
    ///
    /// Returns [`SemanticError::Handler`] when the callback is duplicated,
    /// not executable, or returns an invalid result.
    pub fn record_index_keys(&self, record: &Record) -> Result<Option<Vec<RecordIndexKey>>> {
        let Some(binding) = self.record_metadata_binding(record, "record.index_keys")? else {
            return Ok(None);
        };
        match self.invoke_record_metadata(record, binding)? {
            HandlerOutput::IndexKeys(value) => Ok(Some(value)),
            _ => Err(invalid_record_metadata_output(
                binding,
                "index-key callback returned an invalid result",
            )),
        }
    }

    pub(crate) fn apply_normalizers<'a>(
        &self,
        path: &str,
        record: &Record,
        mut value: FieldValue<'a>,
    ) -> Result<FieldValue<'a>> {
        for binding in self
            .registry
            .package()
            .callback_bindings()
            .iter()
            .filter(|binding| binding.path == path && binding.callback_id == "float.normalizer")
        {
            if !matches!(
                binding.implementation,
                CallbackImplementation::BuiltIn { .. }
                    | CallbackImplementation::CustomHandler { .. }
            ) {
                continue;
            }
            let handler_value = value.to_handler_value();
            if matches!(&handler_value, FieldValue::Float(value) if !value.is_finite()) {
                continue;
            }
            value = match self.handlers.invoke(
                binding,
                self.handler_record(record),
                HandlerPhase::DecodeNormalize,
                Some(&handler_value),
                None,
            )? {
                HandlerOutput::Value(transformed) => transformed.into_record_value(),
                _ => {
                    return Err(SemanticError::Handler {
                        handler: binding.callback_id.clone(),
                        message: "value callback returned a non-value result".to_owned(),
                    });
                }
            };
        }
        Ok(value)
    }

    fn handler_record(&self, record: &Record) -> HandlerRecordContext {
        HandlerRecordContext::new(
            record.header.signature,
            record.header.form_id,
            record.header.form_version,
            self.registry.package().manifest().game,
        )
    }

    fn record_metadata_binding<'a>(
        &'a self,
        record: &Record,
        callback_id: &str,
    ) -> Result<Option<&'a CallbackBinding>> {
        let record_path = record.header.signature.to_string();
        let mut bindings = self
            .registry
            .package()
            .callback_bindings()
            .iter()
            .filter(|binding| binding.path == record_path && binding.callback_id == callback_id);
        let Some(binding) = bindings.next() else {
            return Ok(None);
        };
        if bindings.next().is_some() {
            return Err(SemanticError::Handler {
                handler: callback_id.to_owned(),
                message: "record metadata callback is bound more than once".to_owned(),
            });
        }
        if !matches!(
            binding.implementation,
            CallbackImplementation::BuiltIn { .. } | CallbackImplementation::CustomHandler { .. }
        ) {
            return Err(SemanticError::Handler {
                handler: callback_id.to_owned(),
                message: "record metadata callback is not executable".to_owned(),
            });
        }
        Ok(Some(binding))
    }

    fn invoke_record_metadata(
        &self,
        record: &Record,
        binding: &CallbackBinding,
    ) -> Result<HandlerOutput> {
        self.handlers.invoke_with_source_record(
            binding,
            self.handler_record(record),
            Some(record),
            HandlerPhase::RecordMetadata,
            None,
            None,
        )
    }
}

fn invalid_record_metadata_output(binding: &CallbackBinding, message: &str) -> SemanticError {
    SemanticError::Handler {
        handler: binding.callback_id.clone(),
        message: message.to_owned(),
    }
}

fn format_string_enumeration_value(
    allowed_values: &[String],
    value: &str,
    format: ValueFormat,
) -> String {
    match format {
        ValueFormat::Display | ValueFormat::Summary
            if !value.is_empty() && !allowed_values.iter().any(|allowed| allowed == value) =>
        {
            format!("<Unknown: {value}>")
        }
        ValueFormat::SortKey => value.to_uppercase(),
        ValueFormat::Display
        | ValueFormat::Summary
        | ValueFormat::EditValue
        | ValueFormat::NativeValue => value.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Matches xEdit's string-enumeration behavior across presentation modes.
    #[test]
    fn string_enumeration_formats_known_and_unknown_values() {
        let allowed = vec!["BGSActivityTracker".to_owned()];

        assert_eq!(
            format_string_enumeration_value(&allowed, "BGSActivityTracker", ValueFormat::Display),
            "BGSActivityTracker"
        );
        assert_eq!(
            format_string_enumeration_value(&allowed, "FutureComponent", ValueFormat::Summary),
            "<Unknown: FutureComponent>"
        );
        assert_eq!(
            format_string_enumeration_value(&allowed, "FutureComponent", ValueFormat::EditValue),
            "FutureComponent"
        );
        assert_eq!(
            format_string_enumeration_value(&allowed, "mixedCase", ValueFormat::SortKey),
            "MIXEDCASE"
        );
        assert_eq!(
            format_string_enumeration_value(&allowed, "", ValueFormat::Display),
            ""
        );
    }
}
