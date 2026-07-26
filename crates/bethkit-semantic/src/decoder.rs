// SPDX-License-Identifier: Apache-2.0
//!
//! Registry for complex schema decoders and encoders.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::{FieldValue, OwnedFieldValue, Result, SemanticError};

/// Custom implementation for complex xEdit definitions.
pub trait CustomDecoder: Send + Sync {
    /// Stable identifier referenced by schema packages.
    fn id(&self) -> &'static str;

    /// Decoder implementation version.
    fn version(&self) -> u32;

    /// Decodes a payload.
    ///
    /// # Errors
    ///
    /// Returns [`SemanticError`] when the payload is malformed.
    fn decode<'a>(&self, payload: &'a [u8]) -> Result<FieldValue<'a>>;

    /// Encodes an owned semantic value.
    ///
    /// # Errors
    ///
    /// Returns [`SemanticError`] when the value is unsupported or invalid.
    fn encode(&self, value: &OwnedFieldValue) -> Result<Vec<u8>>;
}

/// Versioned custom-decoder registry.
#[derive(Default)]
pub struct DecoderRegistry {
    decoders: BTreeMap<String, Arc<dyn CustomDecoder>>,
}

impl DecoderRegistry {
    /// Creates an empty decoder registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a registry containing all built-in decoders.
    ///
    /// Complex decoder implementations are added as their differential test
    /// suites are completed.
    pub fn builtin() -> Self {
        Self::new()
    }

    /// Registers or replaces a custom decoder.
    pub fn register(&mut self, decoder: Arc<dyn CustomDecoder>) {
        self.decoders.insert(decoder.id().to_owned(), decoder);
    }

    /// Resolves a decoder satisfying a minimum implementation version.
    ///
    /// # Errors
    ///
    /// Returns [`SemanticError::MissingDecoder`] when the decoder is absent
    /// or older than the required version.
    pub fn require(&self, id: &str, minimum_version: u32) -> Result<&dyn CustomDecoder> {
        let decoder: &Arc<dyn CustomDecoder> = self
            .decoders
            .get(id)
            .ok_or_else(|| SemanticError::MissingDecoder(id.to_owned()))?;
        if decoder.version() < minimum_version {
            return Err(SemanticError::MissingDecoder(format!(
                "{id} version {minimum_version} or newer"
            )));
        }
        Ok(decoder.as_ref())
    }

    /// Returns a decoder by identifier.
    pub fn get(&self, id: &str) -> Option<&dyn CustomDecoder> {
        self.decoders.get(id).map(Arc::as_ref)
    }
}
