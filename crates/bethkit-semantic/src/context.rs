// SPDX-License-Identifier: Apache-2.0
//!
//! Semantic runtime context tying packages to decoder implementations.

use std::sync::Arc;

use bethkit_core::Record;
use bethkit_schema::{CallbackImplementation, SchemaPackage, SchemaRegistry};

use crate::{
    DecoderRegistry, FieldValue, HandlerOutput, RecordEditor, RecordView, Result, SemanticError,
    SemanticHandlerRegistry,
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
                record.header.signature,
                record.header.form_id,
                record.header.form_version,
                Some(&handler_value),
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
}
