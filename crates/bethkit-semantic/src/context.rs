// SPDX-License-Identifier: Apache-2.0
//!
//! Semantic runtime context tying packages to decoder implementations.

use std::sync::Arc;

use bethkit_core::Record;
use bethkit_schema::{SchemaPackage, SchemaRegistry};

use crate::{DecoderRegistry, RecordEditor, RecordView, Result};

/// Runtime context for schema-guided operations on one game mode.
pub struct SemanticContext {
    registry: SchemaRegistry,
    decoders: DecoderRegistry,
}

impl SemanticContext {
    /// Creates a context and verifies all package decoder requirements.
    ///
    /// # Errors
    ///
    /// Returns [`crate::SemanticError::MissingDecoder`] when a required
    /// decoder is unavailable or too old.
    pub fn new(package: Arc<SchemaPackage>, decoders: DecoderRegistry) -> Result<Self> {
        for requirement in &package.manifest().required_decoders {
            decoders.require(&requirement.id, requirement.minimum_version)?;
        }
        Ok(Self {
            registry: SchemaRegistry::new(package),
            decoders,
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
}
