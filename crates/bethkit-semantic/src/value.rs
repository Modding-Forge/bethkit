// SPDX-License-Identifier: Apache-2.0
//!
//! Decoded semantic values and byte provenance.

use bethkit_core::{FormId, Signature};
use bethkit_schema::SchemaNodeId;

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
    /// Field was decoded by a registered custom decoder.
    CustomDecoder,
}

/// A named nested value.
#[derive(Debug)]
pub struct NamedValue<'a> {
    /// Stable schema node identifier.
    pub node_id: SchemaNodeId,
    /// Stable schema path.
    pub path: String,
    /// Human-readable name.
    pub name: String,
    /// Byte span inside the containing payload.
    pub span: ByteSpan,
    /// Decoded value.
    pub value: FieldValue<'a>,
}

/// A decoded value that borrows strings and bytes from the source record.
#[derive(Debug)]
pub enum FieldValue<'a> {
    /// Signed integer.
    Int(i64),
    /// Unsigned integer.
    UInt(u64),
    /// Floating-point value.
    Float(f64),
    /// Borrowed UTF-8 string.
    String(&'a str),
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
    /// Borrowed bytes.
    Bytes(&'a [u8]),
    /// Packed struct fields.
    Struct(Vec<NamedValue<'a>>),
    /// Homogeneous array values.
    Array(Vec<FieldValue<'a>>),
    /// Node was excluded by its condition.
    Absent,
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
}
