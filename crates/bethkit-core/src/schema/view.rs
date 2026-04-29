// SPDX-License-Identifier: Apache-2.0
//!
//! Read-only, schema-guided view over a parsed [`Record`].
//!
//! [`RecordView`] pairs a live record with its [`RecordSchema`] to yield
//! typed [`FieldEntry`] values without copying the underlying bytes beyond
//! what the schema's type system requires.
//!
//! # Limitations
//!
//! - Only the **first** occurrence of each subrecord signature is exposed
//!   through [`RecordView::get_field`].  Use [`RecordView::fields`] for
//!   repeating subrecords.
//! - Union variants, deeply nested structs, and multi-level arrays produce
//!   [`FieldValue::Bytes`] as a fallback rather than panicking or returning
//!   an error; the caller can inspect the raw bytes if full decoding is
//!   required.

use std::fmt;

use crate::error::{CoreError, Result};
use crate::record::{Record, SubRecord};
use crate::schema::{ArrayCount, FieldDef, FieldType, RecordSchema, SubRecordDef};
use crate::types::{FormId, RecordFlags, Signature};


/// A decoded field value produced by a [`RecordView`].
#[derive(Debug)]
pub enum FieldValue<'a> {
    /// Any unsigned or signed integer narrowed to i64 for a uniform API.
    Int(i64),
    /// A 32-bit or 64-bit float widened to f64.
    Float(f64),
    /// An inline NUL-terminated string borrowed from the record data.
    Str(&'a str),
    /// A resolved FormID without a declared target-type constraint.
    FormId(FormId),
    /// A resolved FormID that the schema restricts to a specific set of
    /// record types.
    ///
    /// The `allowed` slice contains every [`Signature`] that the referenced
    /// record is permitted to have (e.g. `[ENCH, SPEL]` for an enchantment
    /// link). All entries point into static memory — no allocation.
    ///
    /// # Validation
    ///
    /// To check whether the value is a valid reference, resolve `raw` against
    /// a [`crate::PluginCache`] and verify that the winning record's signature
    /// is contained in `allowed`.
    FormIdTyped {
        /// The raw file-local FormID read from the subrecord.
        raw: FormId,
        /// The set of record-type signatures the referenced record must have.
        allowed: &'static [Signature],
    },
    /// Raw bytes for payloads that are not further decoded.
    Bytes(&'a [u8]),
    /// An enumeration value with an optional resolved name.
    Enum {
        /// The raw integer from the record.
        value: i64,
        /// The name of the variant, if it is a known value.
        name: Option<&'static str>,
    },
    /// A set of bit flags.
    Flags {
        /// The raw integer from the record.
        value: u64,
        /// Names of all bits that are set.
        active: Vec<&'static str>,
    },
    /// A fixed-layout struct decoded into a list of named fields.
    Struct(Vec<FieldEntry<'a>>),
    /// An array of homogeneous values.
    Array(Vec<FieldValue<'a>>),
    /// A localised string table ID (only when the record's LOCALIZED flag is
    /// set).
    LocalizedId(u32),
    /// The subrecord matching the field definition was absent from the record.
    Missing,
}

impl fmt::Display for FieldValue<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Int(v) => write!(f, "{v}"),
            Self::Float(v) => write!(f, "{v}"),
            Self::Str(s) => write!(f, "{s}"),
            Self::FormId(id) => write!(f, "{id}"),
            Self::FormIdTyped { raw, allowed } => {
                let names: Vec<String> = allowed.iter().map(|s| format!("{s}")).collect();
                write!(f, "{raw} (-> {})", names.join(" | "))
            }
            Self::Bytes(b) => write!(f, "<{} bytes>", b.len()),
            Self::Enum { value, name } => match name {
                Some(n) => write!(f, "{n} ({value})"),
                None => write!(f, "{value}"),
            },
            Self::Flags { value, active } => write!(f, "{:#x} [{}]", value, active.join(", ")),
            Self::Struct(fields) => {
                write!(f, "{{")?;
                for (i, fe) in fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}: {}", fe.name, fe.value)?;
                }
                write!(f, "}}")
            }
            Self::Array(items) => {
                write!(f, "[")?;
                for (i, v) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{v}")?;
                }
                write!(f, "]")
            }
            Self::LocalizedId(id) => write!(f, "<LSTRING:{id:#010x}>"),
            Self::Missing => write!(f, "<missing>"),
        }
    }
}


/// A named field value within a [`RecordView`] or a nested struct.
#[derive(Debug)]
pub struct FieldEntry<'a> {
    /// Human-readable field name (from the schema).
    pub name: &'static str,
    /// The decoded value.
    pub value: FieldValue<'a>,
}


/// A read-only, schema-guided view over a parsed [`Record`].
///
/// Pairs a live record with its [`RecordSchema`] to provide typed field
/// access without modifying or copying the underlying record data beyond
/// what the schema requires.
pub struct RecordView<'a> {
    record: &'a Record,
    schema: &'static RecordSchema,
    /// Whether LString fields should be decoded as localised IDs.
    localized: bool,
}

impl<'a> RecordView<'a> {
    /// Creates a new view over `record` using `schema`.
    ///
    /// `plugin_localized` must be `true` when the containing plugin has its
    /// LOCALIZED flag set in the TES4 header record.  In Skyrim SE, the flag
    /// is stored at plugin-header level only; individual record headers do not
    /// repeat it, so passing `record.header.flags.contains(LOCALIZED)` alone
    /// is insufficient.
    ///
    /// When either `plugin_localized` or the per-record LOCALIZED flag is set,
    /// LString fields are decoded as [`FieldValue::LocalizedId`] rather than
    /// as inline strings.
    ///
    /// # Arguments
    ///
    /// * `record` - The record to view.
    /// * `schema` - The schema describing the record's subrecord layout.
    /// * `plugin_localized` - Whether the plugin that contains this record has
    ///   the LOCALIZED flag set.
    pub fn new(record: &'a Record, schema: &'static RecordSchema, plugin_localized: bool) -> Self {
        let localized =
            plugin_localized || record.header.flags.contains(RecordFlags::LOCALIZED);
        Self { record, schema, localized }
    }

    /// Returns all defined fields in schema order.
    ///
    /// Absent subrecords and subrecords that are shorter than the schema
    /// expects (version-truncated trailing fields) are both represented as
    /// [`FieldValue::Missing`].  Only genuine decoding failures — such as
    /// invalid UTF-8 in a ZString — propagate as errors.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError`] if subrecord raw parsing fails or a field
    /// contains data that cannot be decoded (e.g. invalid UTF-8 in a ZString).
    /// [`CoreError::UnexpectedEof`] is never returned; truncated subrecords
    /// are silently coerced to [`FieldValue::Missing`].
    pub fn fields(&self) -> Result<Vec<FieldEntry<'a>>> {
        let subrecords: &[SubRecord] = self.record.subrecords()?;
        let mut entries: Vec<FieldEntry<'a>> = Vec::with_capacity(self.schema.members.len());
        for def in self.schema.members {
            // NOTE: Bethesda plugins may omit optional trailing fields inside
            // a subrecord when an older or simpler version of the schema was
            // used. Treat those truncation errors the same as a missing
            // subrecord so callers get a uniform Missing value rather than an
            // error that would prevent decoding the rest of the record.
            let value: FieldValue<'a> = match self.decode_member(def, subrecords) {
                Ok(v) => v,
                Err(CoreError::UnexpectedEof { .. }) => FieldValue::Missing,
                Err(e) => return Err(e),
            };
            entries.push(FieldEntry { name: def.name, value });
        }
        Ok(entries)
    }

    /// Returns the first field matching `name`, if any.
    ///
    /// Absent subrecords and version-truncated subrecords both yield
    /// `Ok(Some(FieldEntry { value: FieldValue::Missing, .. }))`.
    ///
    /// # Arguments
    ///
    /// * `name` - The human-readable field name defined in the schema.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError`] on genuine decoding failures (e.g. invalid
    /// UTF-8).  [`CoreError::UnexpectedEof`] is coerced to
    /// [`FieldValue::Missing`] rather than propagated.
    pub fn get_field(&self, name: &str) -> Result<Option<FieldEntry<'a>>> {
        let subrecords: &[SubRecord] = self.record.subrecords()?;
        for def in self.schema.members {
            if def.name == name {
                // NOTE: Same truncation-tolerance as fields() — see there.
                let value: FieldValue<'a> = match self.decode_member(def, subrecords) {
                    Ok(v) => v,
                    Err(CoreError::UnexpectedEof { .. }) => FieldValue::Missing,
                    Err(e) => return Err(e),
                };
                return Ok(Some(FieldEntry { name: def.name, value }));
            }
        }
        Ok(None)
    }

    /// Decodes a single member against the subrecord list.
    fn decode_member(
        &self,
        def: &SubRecordDef,
        subrecords: &'a [SubRecord],
    ) -> Result<FieldValue<'a>> {
        if def.repeating {
            let matching: Vec<&'a SubRecord> =
                subrecords.iter().filter(|sr| sr.signature == def.sig).collect();
            if matching.is_empty() {
                return Ok(FieldValue::Missing);
            }
            let items: Result<Vec<FieldValue<'a>>> = matching
                .into_iter()
                .map(|sr| self.decode_field(def.field, sr.as_bytes()))
                .collect();
            return Ok(FieldValue::Array(items?));
        }

        match subrecords.iter().find(|sr| sr.signature == def.sig) {
            Some(sr) => self.decode_field(def.field, sr.as_bytes()),
            None => Ok(FieldValue::Missing),
        }
    }

    /// Decodes a single field from `data` according to `kind`.
    fn decode_field(&self, kind: FieldType, data: &'a [u8]) -> Result<FieldValue<'a>> {
        match kind {
            FieldType::UInt8 => {
                let v: u8 = data
                    .first()
                    .copied()
                    .ok_or(CoreError::UnexpectedEof { context: "UInt8 field" })?;
                Ok(FieldValue::Int(i64::from(v)))
            }
            FieldType::UInt16 => {
                if data.len() < 2 {
                    return Err(CoreError::UnexpectedEof { context: "UInt16 field" });
                }
                let v: u16 = u16::from_le_bytes([data[0], data[1]]);
                Ok(FieldValue::Int(i64::from(v)))
            }
            FieldType::UInt32 => {
                if data.len() < 4 {
                    return Err(CoreError::UnexpectedEof { context: "UInt32 field" });
                }
                let v: u32 = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
                Ok(FieldValue::Int(i64::from(v)))
            }
            FieldType::UInt64 => {
                if data.len() < 8 {
                    return Err(CoreError::UnexpectedEof { context: "UInt64 field" });
                }
                let v: u64 = u64::from_le_bytes(data[..8].try_into().expect("slice is 8 bytes"));
                // Truncate to i64 on overflow — callers that need the full
                // u64 range should use Bytes instead.
                Ok(FieldValue::Int(v as i64))
            }
            FieldType::Int8 => {
                let v: i8 = data
                    .first()
                    .copied()
                    .ok_or(CoreError::UnexpectedEof { context: "Int8 field" })? as i8;
                Ok(FieldValue::Int(i64::from(v)))
            }
            FieldType::Int16 => {
                if data.len() < 2 {
                    return Err(CoreError::UnexpectedEof { context: "Int16 field" });
                }
                let v: i16 = i16::from_le_bytes([data[0], data[1]]);
                Ok(FieldValue::Int(i64::from(v)))
            }
            FieldType::Int32 => {
                if data.len() < 4 {
                    return Err(CoreError::UnexpectedEof { context: "Int32 field" });
                }
                let v: i32 = i32::from_le_bytes([data[0], data[1], data[2], data[3]]);
                Ok(FieldValue::Int(i64::from(v)))
            }
            FieldType::Float32 => {
                if data.len() < 4 {
                    return Err(CoreError::UnexpectedEof { context: "Float32 field" });
                }
                let bits: u32 = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
                Ok(FieldValue::Float(f64::from(f32::from_bits(bits))))
            }
            FieldType::ZString => {
                let end: usize = data
                    .iter()
                    .position(|&b| b == 0)
                    .unwrap_or(data.len());
                let s: &str = std::str::from_utf8(&data[..end])
                    .map_err(|e| CoreError::InvalidEncoding(e.to_string()))?;
                Ok(FieldValue::Str(s))
            }
            FieldType::LString => {
                if self.localized {
                    if data.len() < 4 {
                        return Err(CoreError::UnexpectedEof { context: "LString ID" });
                    }
                    let id: u32 = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
                    Ok(FieldValue::LocalizedId(id))
                } else {
                    let end: usize = data
                        .iter()
                        .position(|&b| b == 0)
                        .unwrap_or(data.len());
                    let s: &str = std::str::from_utf8(&data[..end])
                        .map_err(|e| CoreError::InvalidEncoding(e.to_string()))?;
                    Ok(FieldValue::Str(s))
                }
            }
            FieldType::ByteArray => Ok(FieldValue::Bytes(data)),
            FieldType::FormId => {
                if data.len() < 4 {
                    return Err(CoreError::UnexpectedEof { context: "FormId field" });
                }
                let raw: u32 = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
                Ok(FieldValue::FormId(FormId(raw)))
            }
            FieldType::FormIdTyped(allowed) => {
                if data.len() < 4 {
                    return Err(CoreError::UnexpectedEof { context: "FormId field" });
                }
                let raw: u32 = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
                Ok(FieldValue::FormIdTyped { raw: FormId(raw), allowed })
            }
            FieldType::Enum(def) => {
                if data.len() < 4 {
                    return Err(CoreError::UnexpectedEof { context: "Enum field" });
                }
                let raw: u32 = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
                let value: i64 = raw as i64;
                let name: Option<&'static str> = def.name_of(value);
                Ok(FieldValue::Enum { value, name })
            }
            FieldType::Flags(def) => {
                if data.len() < 4 {
                    return Err(CoreError::UnexpectedEof { context: "Flags field" });
                }
                let raw: u32 = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
                let value: u64 = u64::from(raw);
                let active: Vec<&'static str> = def.active_names(value).collect();
                Ok(FieldValue::Flags { value, active })
            }
            FieldType::Struct(fields) => {
                let entries: Result<Vec<FieldEntry<'a>>> =
                    decode_struct_fields(fields, data, self.localized);
                Ok(FieldValue::Struct(entries?))
            }
            FieldType::Array { element, count } => {
                let values: Result<Vec<FieldValue<'a>>> =
                    decode_array(*element, count, data, self.localized);
                Ok(FieldValue::Array(values?))
            }
            FieldType::Union { decider, variants } => {
                let idx: usize = decider(data);
                if let Some(variant) = variants.get(idx) {
                    self.decode_field(*variant, data)
                } else {
                    Ok(FieldValue::Bytes(data))
                }
            }
            FieldType::Unused(_) => Ok(FieldValue::Bytes(data)),
        }
    }
}

/// Decodes a packed struct from `data` into a list of [`FieldEntry`] values.
fn decode_struct_fields<'a>(
    fields: &'static [FieldDef],
    data: &'a [u8],
    localized: bool,
) -> Result<Vec<FieldEntry<'a>>> {
    let mut offset: usize = 0;
    let mut entries: Vec<FieldEntry<'a>> = Vec::with_capacity(fields.len());

    for field in fields {
        let (value, consumed): (FieldValue<'a>, usize) =
            decode_one_field(field.kind, &data[offset..], localized)?;
        entries.push(FieldEntry { name: field.name, value });
        offset += consumed;
    }

    Ok(entries)
}

/// Decodes a single field value from `data` and returns `(value, bytes_consumed)`.
fn decode_one_field<'a>(
    kind: FieldType,
    data: &'a [u8],
    localized: bool,
) -> Result<(FieldValue<'a>, usize)> {
    match kind {
        FieldType::UInt8 => {
            let v: u8 = *data.first().ok_or(CoreError::UnexpectedEof { context: "UInt8" })?;
            Ok((FieldValue::Int(i64::from(v)), 1))
        }
        FieldType::UInt16 => {
            if data.len() < 2 {
                return Err(CoreError::UnexpectedEof { context: "UInt16" });
            }
            let v: u16 = u16::from_le_bytes([data[0], data[1]]);
            Ok((FieldValue::Int(i64::from(v)), 2))
        }
        FieldType::UInt32 => {
            if data.len() < 4 {
                return Err(CoreError::UnexpectedEof { context: "UInt32" });
            }
            let v: u32 = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
            Ok((FieldValue::Int(i64::from(v)), 4))
        }
        FieldType::UInt64 => {
            if data.len() < 8 {
                return Err(CoreError::UnexpectedEof { context: "UInt64" });
            }
            let v: u64 = u64::from_le_bytes(data[..8].try_into().expect("slice is 8 bytes"));
            Ok((FieldValue::Int(v as i64), 8))
        }
        FieldType::Int8 => {
            let v: i8 = *data.first().ok_or(CoreError::UnexpectedEof { context: "Int8" })? as i8;
            Ok((FieldValue::Int(i64::from(v)), 1))
        }
        FieldType::Int16 => {
            if data.len() < 2 {
                return Err(CoreError::UnexpectedEof { context: "Int16" });
            }
            let v: i16 = i16::from_le_bytes([data[0], data[1]]);
            Ok((FieldValue::Int(i64::from(v)), 2))
        }
        FieldType::Int32 => {
            if data.len() < 4 {
                return Err(CoreError::UnexpectedEof { context: "Int32" });
            }
            let v: i32 = i32::from_le_bytes([data[0], data[1], data[2], data[3]]);
            Ok((FieldValue::Int(i64::from(v)), 4))
        }
        FieldType::Float32 => {
            if data.len() < 4 {
                return Err(CoreError::UnexpectedEof { context: "Float32" });
            }
            let bits: u32 = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
            Ok((FieldValue::Float(f64::from(f32::from_bits(bits))), 4))
        }
        FieldType::ZString => {
            let end: usize = data.iter().position(|&b| b == 0).unwrap_or(data.len());
            let s: &str = std::str::from_utf8(&data[..end])
                .map_err(|e| CoreError::InvalidEncoding(e.to_string()))?;
            Ok((FieldValue::Str(s), end + 1))
        }
        FieldType::LString => {
            if localized {
                if data.len() < 4 {
                    return Err(CoreError::UnexpectedEof { context: "LString ID" });
                }
                let id: u32 = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
                Ok((FieldValue::LocalizedId(id), 4))
            } else {
                let end: usize = data.iter().position(|&b| b == 0).unwrap_or(data.len());
                let s: &str = std::str::from_utf8(&data[..end])
                    .map_err(|e| CoreError::InvalidEncoding(e.to_string()))?;
                Ok((FieldValue::Str(s), end + 1))
            }
        }
        FieldType::ByteArray => Ok((FieldValue::Bytes(data), data.len())),
        FieldType::FormId => {
            if data.len() < 4 {
                return Err(CoreError::UnexpectedEof { context: "FormId" });
            }
            let raw: u32 = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
            Ok((FieldValue::FormId(FormId(raw)), 4))
        }
        FieldType::FormIdTyped(allowed) => {
            if data.len() < 4 {
                return Err(CoreError::UnexpectedEof { context: "FormId" });
            }
            let raw: u32 = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
            Ok((FieldValue::FormIdTyped { raw: FormId(raw), allowed }, 4))
        }
        FieldType::Enum(def) => {
            if data.len() < 4 {
                return Err(CoreError::UnexpectedEof { context: "Enum" });
            }
            let raw: u32 = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
            let value: i64 = raw as i64;
            let name: Option<&'static str> = def.name_of(value);
            Ok((FieldValue::Enum { value, name }, 4))
        }
        FieldType::Flags(def) => {
            if data.len() < 4 {
                return Err(CoreError::UnexpectedEof { context: "Flags" });
            }
            let raw: u32 = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
            let value: u64 = u64::from(raw);
            let active: Vec<&'static str> = def.active_names(value).collect();
            Ok((FieldValue::Flags { value, active }, 4))
        }
        FieldType::Struct(fields) => {
            let entries: Result<Vec<FieldEntry<'a>>> =
                decode_struct_fields(fields, data, localized);
            let consumed: usize = struct_byte_size(fields);
            Ok((FieldValue::Struct(entries?), consumed))
        }
        FieldType::Array { element, count } => {
            let values: Result<Vec<FieldValue<'a>>> =
                decode_array(*element, count, data, localized);
            Ok((FieldValue::Array(values?), data.len()))
        }
        FieldType::Union { decider, variants } => {
            let idx: usize = decider(data);
            if let Some(variant) = variants.get(idx) {
                decode_one_field(*variant, data, localized)
            } else {
                Ok((FieldValue::Bytes(data), data.len()))
            }
        }
        FieldType::Unused(n) => Ok((FieldValue::Bytes(&data[..n as usize]), n as usize)),
    }
}

/// Decodes a homogeneous array from `data`.
fn decode_array<'a>(
    element: FieldType,
    count: ArrayCount,
    data: &'a [u8],
    localized: bool,
) -> Result<Vec<FieldValue<'a>>> {
    match count {
        ArrayCount::Fixed(n) => {
            let mut offset: usize = 0;
            let mut values: Vec<FieldValue<'a>> = Vec::with_capacity(n as usize);
            for _ in 0..n {
                let (v, consumed): (FieldValue<'a>, usize) =
                    decode_one_field(element, &data[offset..], localized)?;
                values.push(v);
                offset += consumed;
            }
            Ok(values)
        }
        ArrayCount::Remainder | ArrayCount::PrecedingSibling(_) => {
            // Consume all remaining bytes, parsing one element at a time.
            let mut offset: usize = 0;
            let mut values: Vec<FieldValue<'a>> = Vec::new();
            while offset < data.len() {
                let (v, consumed): (FieldValue<'a>, usize) =
                    decode_one_field(element, &data[offset..], localized)?;
                if consumed == 0 {
                    break;
                }
                values.push(v);
                offset += consumed;
            }
            Ok(values)
        }
    }
}

/// Returns the fixed byte width of a struct layout (best-effort; uses element
/// sizes of each field in order).
fn struct_byte_size(fields: &'static [FieldDef]) -> usize {
    fields.iter().map(|f| field_byte_size(f.kind)).sum()
}

/// Returns the byte size of a single field type (0 for variable-length).
fn field_byte_size(kind: FieldType) -> usize {
    match kind {
        FieldType::UInt8 | FieldType::Int8 => 1,
        FieldType::UInt16 | FieldType::Int16 => 2,
        FieldType::UInt32 | FieldType::Int32 | FieldType::Float32 => 4,
        FieldType::UInt64 => 8,
        FieldType::FormId | FieldType::FormIdTyped(_) => 4,
        FieldType::Enum(_) | FieldType::Flags(_) => 4,
        FieldType::Unused(n) => n as usize,
        FieldType::Struct(fields) => struct_byte_size(fields),
        // Variable-length types return 0; the caller should use Bytes.
        FieldType::ZString
        | FieldType::LString
        | FieldType::ByteArray
        | FieldType::Array { .. }
        | FieldType::Union { .. } => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{EnumDef, FlagsDef, FieldDef, FieldType, RecordSchema, SubRecordDef};
    use crate::types::Signature;

    fn make_u32_subrecord(sig: [u8; 4], value: u32) -> Vec<u8> {
        let mut buf: Vec<u8> = Vec::new();
        buf.extend_from_slice(&sig);
        buf.extend_from_slice(&(4u16).to_le_bytes()); // data_size
        buf.extend_from_slice(&value.to_le_bytes());
        buf
    }

    fn make_zstring_subrecord(sig: [u8; 4], s: &str) -> Vec<u8> {
        let payload: Vec<u8> = {
            let mut v: Vec<u8> = s.as_bytes().to_vec();
            v.push(0);
            v
        };
        let mut buf: Vec<u8> = Vec::new();
        buf.extend_from_slice(&sig);
        buf.extend_from_slice(&(payload.len() as u16).to_le_bytes());
        buf.extend_from_slice(&payload);
        buf
    }

    fn build_record_bytes(subrecords_data: &[u8]) -> Vec<u8> {
        // Full record: 24-byte header + subrecord data.
        let mut buf: Vec<u8> = vec![
            // sig
            b'T', b'E', b'S', b'T',
            // data_size
        ];
        let data_size: u32 = subrecords_data.len() as u32;
        buf.extend_from_slice(&data_size.to_le_bytes());
        // flags (none)
        buf.extend_from_slice(&0u32.to_le_bytes());
        // form_id
        buf.extend_from_slice(&0u32.to_le_bytes());
        // version_control
        buf.extend_from_slice(&0u32.to_le_bytes());
        // form_version
        buf.extend_from_slice(&0u16.to_le_bytes());
        // unknown
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(subrecords_data);
        buf
    }

    static TEST_SCHEMA: RecordSchema = RecordSchema {
        sig: Signature(*b"TEST"),
        name: "Test Record",
        members: &[
            SubRecordDef {
                sig: Signature(*b"EDID"),
                name: "Editor ID",
                required: true,
                repeating: false,
                field: FieldType::ZString,
            },
            SubRecordDef {
                sig: Signature(*b"DATA"),
                name: "Value",
                required: true,
                repeating: false,
                field: FieldType::UInt32,
            },
        ],
    };

    /// Verifies that RecordView decodes a ZString field correctly.
    #[test]
    fn record_view_zstring_field() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let ctx = crate::types::GameContext::sse();
        let mut sr_data: Vec<u8> = make_zstring_subrecord(*b"EDID", "TestEditorID");
        sr_data.extend(make_u32_subrecord(*b"DATA", 42));
        let record_bytes = build_record_bytes(&sr_data);
        let mut cursor = bethkit_io::SliceCursor::new(&record_bytes);
        let record = crate::record::Record::parse_header(&mut cursor, &ctx)?;
        let view = RecordView::new(&record, &TEST_SCHEMA, false);
        let entry = view.get_field("Editor ID")?.expect("Editor ID present");
        assert!(matches!(entry.value, FieldValue::Str("TestEditorID")));
        Ok(())
    }

    /// Verifies that RecordView decodes a UInt32 field correctly.
    #[test]
    fn record_view_uint32_field() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let ctx = crate::types::GameContext::sse();
        let mut sr_data: Vec<u8> = make_zstring_subrecord(*b"EDID", "test");
        sr_data.extend(make_u32_subrecord(*b"DATA", 1234));
        let record_bytes = build_record_bytes(&sr_data);
        let mut cursor = bethkit_io::SliceCursor::new(&record_bytes);
        let record = crate::record::Record::parse_header(&mut cursor, &ctx)?;
        let view = RecordView::new(&record, &TEST_SCHEMA, false);
        let entry = view.get_field("Value")?.expect("Value present");
        assert!(matches!(entry.value, FieldValue::Int(1234)));
        Ok(())
    }

    /// Verifies that a missing optional field yields FieldValue::Missing.
    #[test]
    fn record_view_missing_field() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let ctx = crate::types::GameContext::sse();
        let sr_data: Vec<u8> = make_zstring_subrecord(*b"EDID", "test");
        let record_bytes = build_record_bytes(&sr_data);
        let mut cursor = bethkit_io::SliceCursor::new(&record_bytes);
        let record = crate::record::Record::parse_header(&mut cursor, &ctx)?;
        let view = RecordView::new(&record, &TEST_SCHEMA, false);
        let entry = view.get_field("Value")?.expect("field entry returned");
        assert!(matches!(entry.value, FieldValue::Missing));
        Ok(())
    }

    /// Verifies that an enum field is decoded with a name when the value is known.
    #[test]
    fn record_view_enum_field() -> std::result::Result<(), Box<dyn std::error::Error>> {
        static MY_ENUM: EnumDef = EnumDef {
            name: "MyEnum",
            values: &[(0, "Zero"), (1, "One")],
        };
        static ENUM_SCHEMA: RecordSchema = RecordSchema {
            sig: Signature(*b"ENMT"),
            name: "Enum Test",
            members: &[SubRecordDef {
                sig: Signature(*b"EVAL"),
                name: "My Value",
                required: true,
                repeating: false,
                field: FieldType::Enum(&MY_ENUM),
            }],
        };
        let ctx = crate::types::GameContext::sse();
        let sr_data = make_u32_subrecord(*b"EVAL", 1);
        let record_bytes = build_record_bytes(&sr_data);
        let mut cursor = bethkit_io::SliceCursor::new(&record_bytes);
        let record = crate::record::Record::parse_header(&mut cursor, &ctx)?;
        let view = RecordView::new(&record, &ENUM_SCHEMA, false);
        let entry = view.get_field("My Value")?.expect("field present");
        match entry.value {
            FieldValue::Enum { value, name } => {
                assert_eq!(value, 1);
                assert_eq!(name, Some("One"));
            }
            other => panic!("expected Enum, got {other:?}"),
        }
        Ok(())
    }

    /// Verifies that a flags field lists active flag names.
    #[test]
    fn record_view_flags_field() -> std::result::Result<(), Box<dyn std::error::Error>> {
        static MY_FLAGS: FlagsDef = FlagsDef {
            name: "MyFlags",
            bits: &[(0, "Alpha"), (1, "Beta"), (2, "Gamma")],
        };
        static FLAGS_SCHEMA: RecordSchema = RecordSchema {
            sig: Signature(*b"FLGT"),
            name: "Flags Test",
            members: &[SubRecordDef {
                sig: Signature(*b"FVAL"),
                name: "My Flags",
                required: true,
                repeating: false,
                field: FieldType::Flags(&MY_FLAGS),
            }],
        };
        let ctx = crate::types::GameContext::sse();
        let sr_data = make_u32_subrecord(*b"FVAL", 0b101);
        let record_bytes = build_record_bytes(&sr_data);
        let mut cursor = bethkit_io::SliceCursor::new(&record_bytes);
        let record = crate::record::Record::parse_header(&mut cursor, &ctx)?;
        let view = RecordView::new(&record, &FLAGS_SCHEMA, false);
        let entry = view.get_field("My Flags")?.expect("field present");
        match entry.value {
            FieldValue::Flags { value, active } => {
                assert_eq!(value, 0b101);
                assert_eq!(active, ["Alpha", "Gamma"]);
            }
            other => panic!("expected Flags, got {other:?}"),
        }
        Ok(())
    }

    /// Verifies that a struct field is decoded into named sub-fields.
    #[test]
    fn record_view_struct_field() -> std::result::Result<(), Box<dyn std::error::Error>> {
        static STRUCT_FIELDS: [FieldDef; 2] = [
            FieldDef { name: "A", kind: FieldType::UInt16 },
            FieldDef { name: "B", kind: FieldType::UInt16 },
        ];
        static STRUCT_SCHEMA: RecordSchema = RecordSchema {
            sig: Signature(*b"STRT"),
            name: "Struct Test",
            members: &[SubRecordDef {
                sig: Signature(*b"SVAL"),
                name: "My Struct",
                required: true,
                repeating: false,
                field: FieldType::Struct(&STRUCT_FIELDS),
            }],
        };
        let ctx = crate::types::GameContext::sse();
        let payload: Vec<u8> = {
            let mut v = Vec::new();
            v.extend_from_slice(b"SVAL");
            v.extend_from_slice(&4u16.to_le_bytes());
            v.extend_from_slice(&10u16.to_le_bytes());
            v.extend_from_slice(&20u16.to_le_bytes());
            v
        };
        let record_bytes = build_record_bytes(&payload);
        let mut cursor = bethkit_io::SliceCursor::new(&record_bytes);
        let record = crate::record::Record::parse_header(&mut cursor, &ctx)?;
        let view = RecordView::new(&record, &STRUCT_SCHEMA, false);
        let entry = view.get_field("My Struct")?.expect("field present");
        match entry.value {
            FieldValue::Struct(fields) => {
                assert_eq!(fields.len(), 2);
                assert!(matches!(fields[0].value, FieldValue::Int(10)));
                assert!(matches!(fields[1].value, FieldValue::Int(20)));
            }
            other => panic!("expected Struct, got {other:?}"),
        }
        Ok(())
    }
}
