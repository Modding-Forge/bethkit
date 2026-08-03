// SPDX-License-Identifier: Apache-2.0
//!
//! Registry for complex schema decoders and encoders.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::{FieldValue, OwnedFieldValue, Result, SemanticError};

const XEDIT_DTINTEGER_ID: &str = "xedit.dtinteger";

struct ZeroWidthIntegerDecoder;

impl CustomDecoder for ZeroWidthIntegerDecoder {
    fn id(&self) -> &'static str {
        XEDIT_DTINTEGER_ID
    }

    fn version(&self) -> u32 {
        1
    }

    fn decode<'a>(&self, _payload: &'a [u8]) -> Result<DecodedPayload<'a>> {
        Ok(DecodedPayload {
            value: FieldValue::UInt(0),
            consumed: 0,
        })
    }

    fn encode(&self, value: &OwnedFieldValue) -> Result<Vec<u8>> {
        if value != &OwnedFieldValue::UInt(0) {
            return Err(SemanticError::Decoder {
                decoder: XEDIT_DTINTEGER_ID.to_owned(),
                message: "zero-width xEdit integer only accepts unsigned value 0".to_owned(),
            });
        }
        Ok(Vec::new())
    }
}

/// Value and exact byte consumption reported by a custom payload decoder.
pub struct DecodedPayload<'a> {
    /// Decoded semantic value.
    pub value: FieldValue<'a>,
    /// Number of input bytes consumed by the decoder.
    pub consumed: usize,
}

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
    fn decode<'a>(&self, payload: &'a [u8]) -> Result<DecodedPayload<'a>>;

    /// Encodes an owned semantic value.
    ///
    /// # Errors
    ///
    /// Returns [`SemanticError`] when the value is unsupported or invalid.
    fn encode(&self, value: &OwnedFieldValue) -> Result<Vec<u8>>;
}

/// Versioned custom-decoder registry.
#[derive(Clone, Default)]
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
        let mut registry = Self::new();
        registry.register(Arc::new(ZeroWidthIntegerDecoder));
        registry
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Decodes xEdit's zero-width integer without consuming following bytes.
    #[test]
    fn zero_width_integer_decoder_consumes_no_bytes(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let registry = DecoderRegistry::builtin();
        let decoder = registry
            .require(XEDIT_DTINTEGER_ID, 1)
            .expect("built-in zero-width integer decoder must be registered");

        // when
        let decoded = decoder.decode(&[0xAA, 0xBB])?;

        // then
        assert!(matches!(decoded.value, FieldValue::UInt(0)));
        assert_eq!(decoded.consumed, 0);
        Ok(())
    }

    /// Encodes only the single value representable by a zero-width integer.
    #[test]
    fn zero_width_integer_decoder_encodes_only_zero(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let registry = DecoderRegistry::builtin();
        let decoder = registry
            .require(XEDIT_DTINTEGER_ID, 1)
            .expect("built-in zero-width integer decoder must be registered");

        // when
        let encoded = decoder.encode(&OwnedFieldValue::UInt(0))?;

        // then
        assert!(encoded.is_empty());
        assert!(decoder.encode(&OwnedFieldValue::UInt(1)).is_err());
        Ok(())
    }
}
