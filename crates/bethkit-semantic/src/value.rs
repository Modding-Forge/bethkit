// SPDX-License-Identifier: Apache-2.0
//!
//! Decoded semantic values and byte provenance.

use std::borrow::Cow;

use bethkit_core::{FormId, Signature};
use bethkit_schema::SchemaNodeId;

use crate::Result;

/// Half-open byte range inside a subrecord payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteSpan {
    /// First byte included in the value.
    pub start: usize,
    /// First byte after the value.
    pub end: usize,
}

impl ByteSpan {
    /// Creates a checked byte span.
    ///
    /// Returns `None` when `end` precedes `start`.
    pub const fn new(start: usize, end: usize) -> Option<Self> {
        if end < start {
            None
        } else {
            Some(Self { start, end })
        }
    }

    /// Returns the number of bytes covered by the span.
    pub const fn len(self) -> usize {
        self.end - self.start
    }

    /// Returns whether the span contains no bytes.
    pub const fn is_empty(self) -> bool {
        self.start == self.end
    }
}

/// Provenance of a decoded field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldOrigin {
    /// Field was declared by the loaded schema package.
    Schema,
    /// Field is an unknown subrecord preserved by the lossless core.
    UnknownSubrecord,
    /// Field has a declared signature but appears outside its grammar position.
    UnmatchedKnownSubrecord,
    /// Field was decoded by a registered custom decoder.
    CustomDecoder,
}

/// A named nested value.
#[derive(Debug, Clone)]
pub struct NamedValue<'a> {
    /// Stable schema node identifier.
    pub node_id: SchemaNodeId,
    /// Stable schema path.
    pub path: String,
    /// Effective selected-node path when `path` is a dynamic union.
    pub effective_path: Option<String>,
    /// Human-readable name.
    pub name: String,
    /// Byte span inside the containing payload.
    pub span: ByteSpan,
    /// Decoded value.
    pub value: FieldValue<'a>,
}

/// A decoded value that borrows strings and bytes from the source record.
#[derive(Debug, Clone)]
pub enum FieldValue<'a> {
    /// Signed integer.
    Int(i64),
    /// Unsigned integer.
    UInt(u64),
    /// Floating-point value.
    Float(f64),
    /// Borrowed or handler-owned UTF-8 string.
    String(Cow<'a, str>),
    /// Raw file-local FormID and allowed target signatures.
    FormId {
        /// File-local FormID.
        value: FormId,
        /// Allowed target record types, empty when unrestricted.
        targets: Vec<Signature>,
    },
    /// Enumeration value and optional name.
    Enumeration {
        /// Raw integer value.
        value: i64,
        /// Name assigned by the schema when known.
        name: Option<String>,
    },
    /// Raw flags value and active names.
    Flags {
        /// Complete raw flags value.
        value: u64,
        /// Names of set bits.
        active: Vec<String>,
    },
    /// Borrowed or handler-owned bytes.
    Bytes(Cow<'a, [u8]>),
    /// Packed struct fields.
    Struct(Vec<NamedValue<'a>>),
    /// Homogeneous array values.
    Array(Vec<FieldValue<'a>>),
    /// Node was excluded by its condition.
    Absent,
}

impl FieldValue<'_> {
    pub(crate) fn to_handler_value(&self) -> FieldValue<'static> {
        match self {
            Self::Int(value) => FieldValue::Int(*value),
            Self::UInt(value) => FieldValue::UInt(*value),
            Self::Float(value) => FieldValue::Float(*value),
            Self::String(value) => FieldValue::String(Cow::Owned(value.to_string())),
            Self::FormId { value, targets } => FieldValue::FormId {
                value: *value,
                targets: targets.clone(),
            },
            Self::Enumeration { value, name } => FieldValue::Enumeration {
                value: *value,
                name: name.clone(),
            },
            Self::Flags { value, active } => FieldValue::Flags {
                value: *value,
                active: active.clone(),
            },
            Self::Bytes(value) => FieldValue::Bytes(Cow::Owned(value.to_vec())),
            Self::Struct(values) => FieldValue::Struct(
                values
                    .iter()
                    .map(|value| NamedValue {
                        node_id: value.node_id,
                        path: value.path.clone(),
                        effective_path: value.effective_path.clone(),
                        name: value.name.clone(),
                        span: value.span,
                        value: value.value.to_handler_value(),
                    })
                    .collect(),
            ),
            Self::Array(values) => {
                FieldValue::Array(values.iter().map(FieldValue::to_handler_value).collect())
            }
            Self::Absent => FieldValue::Absent,
        }
    }
}

impl FieldValue<'static> {
    pub(crate) fn into_record_value<'a>(self) -> FieldValue<'a> {
        match self {
            Self::Int(value) => FieldValue::Int(value),
            Self::UInt(value) => FieldValue::UInt(value),
            Self::Float(value) => FieldValue::Float(value),
            Self::String(value) => FieldValue::String(Cow::Owned(value.into_owned())),
            Self::FormId { value, targets } => FieldValue::FormId { value, targets },
            Self::Enumeration { value, name } => FieldValue::Enumeration { value, name },
            Self::Flags { value, active } => FieldValue::Flags { value, active },
            Self::Bytes(value) => FieldValue::Bytes(Cow::Owned(value.into_owned())),
            Self::Struct(values) => FieldValue::Struct(
                values
                    .into_iter()
                    .map(|value| NamedValue {
                        node_id: value.node_id,
                        path: value.path,
                        effective_path: value.effective_path,
                        name: value.name,
                        span: value.span,
                        value: value.value.into_record_value(),
                    })
                    .collect(),
            ),
            Self::Array(values) => FieldValue::Array(
                values
                    .into_iter()
                    .map(FieldValue::into_record_value)
                    .collect(),
            ),
            Self::Absent => FieldValue::Absent,
        }
    }
}

/// Owned value accepted by [`crate::RecordEditor`].
#[derive(Debug, Clone, PartialEq)]
pub enum OwnedFieldValue {
    /// Signed integer.
    Int(i64),
    /// Unsigned integer.
    UInt(u64),
    /// Floating-point value.
    Float(f64),
    /// UTF-8 string.
    String(String),
    /// File-local FormID.
    FormId(FormId),
    /// Raw bytes.
    Bytes(Vec<u8>),
    /// Packed struct values in declaration order.
    Struct(Vec<OwnedFieldValue>),
    /// Homogeneous array values.
    Array(Vec<OwnedFieldValue>),
    /// Struct field omitted from an optional trailing suffix.
    Absent,
}

pub(crate) fn handler_to_owned_value(
    value: FieldValue<'static>,
    _path: &str,
) -> Result<OwnedFieldValue> {
    match value {
        FieldValue::Int(value) => Ok(OwnedFieldValue::Int(value)),
        FieldValue::UInt(value) => Ok(OwnedFieldValue::UInt(value)),
        FieldValue::Float(value) => Ok(OwnedFieldValue::Float(value)),
        FieldValue::String(value) => Ok(OwnedFieldValue::String(value.into_owned())),
        FieldValue::FormId { value, .. } => Ok(OwnedFieldValue::FormId(value)),
        FieldValue::Enumeration { value, .. } => Ok(OwnedFieldValue::Int(value)),
        FieldValue::Flags { value, .. } => Ok(OwnedFieldValue::UInt(value)),
        FieldValue::Bytes(value) => Ok(OwnedFieldValue::Bytes(value.into_owned())),
        FieldValue::Array(values) => values
            .into_iter()
            .map(|value| handler_to_owned_value(value, _path))
            .collect::<Result<Vec<_>>>()
            .map(OwnedFieldValue::Array),
        FieldValue::Struct(values) => values
            .into_iter()
            .map(|value| handler_to_owned_value(value.value, _path))
            .collect::<Result<Vec<_>>>()
            .map(OwnedFieldValue::Struct),
        FieldValue::Absent => Ok(OwnedFieldValue::Absent),
    }
}

pub(crate) fn float_from_raw(value: f64, scale: f64, digits: i32) -> f64 {
    round_float_to_digits(value * scale, digits)
}

pub(crate) fn float_to_raw(value: f64, scale: f64, digits: i32) -> f64 {
    round_float_to_digits(value, digits) / scale
}

fn round_float_to_digits(value: f64, digits: i32) -> f64 {
    if digits == i32::MIN || !value.is_finite() {
        return value;
    }
    let factor = 10.0_f64.powi(-digits);
    (value / factor).round_ties_even() * factor
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Preserves editable raw values for decoded enumerations and flags.
    #[test]
    fn handler_values_convert_enumerations_and_flags_to_owned_values(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        assert_eq!(
            handler_to_owned_value(
                FieldValue::Enumeration {
                    value: 255,
                    name: Some("Protected".to_owned()),
                },
                "TEST/enum",
            )?,
            OwnedFieldValue::Int(255)
        );
        assert_eq!(
            handler_to_owned_value(
                FieldValue::Flags {
                    value: 0x41,
                    active: vec!["First".to_owned(), "Seventh".to_owned()],
                },
                "TEST/flags",
            )?,
            OwnedFieldValue::UInt(0x41)
        );
        Ok(())
    }

    /// Matches Delphi's tie-to-even decimal rounding used by `RoundToEx`.
    #[test]
    fn float_rounding_uses_ties_to_even() -> std::result::Result<(), Box<dyn std::error::Error>> {
        assert_eq!(float_from_raw(2.5, 1.0, 0), 2.0);
        assert_eq!(float_from_raw(3.5, 1.0, 0), 4.0);
        assert!((float_from_raw(1.234_56, 1.0, 4) - 1.234_6).abs() < f64::EPSILON * 2.0);
        assert_eq!(float_from_raw(f64::INFINITY, 255.0, 4), f64::INFINITY);
        assert_eq!(float_from_raw(0.5, 255.0, 0), 128.0);
        assert_eq!(float_to_raw(128.0, 255.0, 0), 128.0 / 255.0);
        Ok(())
    }
}
