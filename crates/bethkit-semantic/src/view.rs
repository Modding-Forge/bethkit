// SPDX-License-Identifier: Apache-2.0
//!
//! Ordered schema-guided views over parsed records.

use std::borrow::Cow;

use bethkit_core::{FormId, Record, Signature, SubRecord};
use bethkit_schema::{
    ArrayCount, ByteOrder, EvalContext, EvalValue, IntegerType, PrimitiveType, SchemaNode,
    SchemaNodeKind, SchemaRecord, StringType,
};

use crate::{
    grammar::interpret, ByteSpan, Diagnostic, DiagnosticCode, DiagnosticSeverity, FieldOrigin,
    FieldValue, NamedValue, Result, SemanticContext, SemanticError, ValidationReport,
};

/// One decoded top-level record field.
#[derive(Debug)]
pub struct Field<'a> {
    /// Stable schema node identifier.
    pub node_id: bethkit_schema::SchemaNodeId,
    /// Stable schema path.
    pub path: String,
    /// Human-readable name.
    pub name: String,
    /// Source subrecord signature.
    pub subrecord_signature: Signature,
    /// Zero-based occurrence of the signature.
    pub occurrence: usize,
    /// Byte span inside the subrecord payload.
    pub span: ByteSpan,
    /// Field provenance.
    pub origin: FieldOrigin,
    /// Decoded field value.
    pub value: FieldValue<'a>,
}

/// Read-only semantic view over one parsed record.
pub struct RecordView<'context, 'record> {
    context: &'context SemanticContext,
    record: &'record Record,
    schema: &'context SchemaRecord,
    localized: bool,
}

impl<'context, 'record> RecordView<'context, 'record> {
    pub(crate) fn new(
        context: &'context SemanticContext,
        record: &'record Record,
        plugin_localized: bool,
    ) -> Result<Self> {
        let schema: &SchemaRecord =
            context
                .registry()
                .get(record.header.signature)
                .ok_or_else(|| {
                    SemanticError::MissingRecordSchema(record.header.signature.to_string())
                })?;
        Ok(Self {
            context,
            record,
            schema,
            localized: plugin_localized,
        })
    }

    /// Returns the schema used by this view.
    pub fn schema(&self) -> &SchemaRecord {
        self.schema
    }

    /// Decodes top-level subrecords in their source order.
    ///
    /// Unknown and out-of-order known subrecords are preserved as borrowed
    /// bytes with distinct [`FieldOrigin`] values.
    ///
    /// # Errors
    ///
    /// Returns [`SemanticError`] when subrecord parsing, conditions, primitive
    /// decoding, or a custom decoder fails.
    pub fn fields(&self) -> Result<Vec<Field<'record>>> {
        let subrecords: &'record [SubRecord] = self.record.subrecords()?;
        let grammar = interpret(
            &self.schema.root,
            self.record.header.signature,
            self.record.header.form_version,
            subrecords,
        )?;
        let mut occurrences: std::collections::BTreeMap<Signature, usize> =
            std::collections::BTreeMap::new();
        let mut fields: Vec<Field<'record>> = Vec::with_capacity(subrecords.len());

        for (index, subrecord) in subrecords.iter().enumerate() {
            let occurrence: usize = *occurrences
                .entry(subrecord.signature)
                .and_modify(|value| *value += 1)
                .or_insert(0);
            let definition = grammar.assignments[index];
            match definition {
                Some(node) => {
                    let SchemaNodeKind::Subrecord { payload, .. } = &node.kind else {
                        unreachable!("definition was filtered to subrecord nodes");
                    };
                    let data: &'record [u8] = subrecord.as_bytes();
                    let value: FieldValue<'record> = self.decode_node(payload, data, data, 0)?;
                    fields.push(Field {
                        node_id: node.id,
                        path: node.path.clone(),
                        name: node.name.clone(),
                        subrecord_signature: subrecord.signature,
                        occurrence,
                        span: ByteSpan {
                            start: 0,
                            end: data.len(),
                        },
                        origin: match &payload.kind {
                            SchemaNodeKind::Custom { .. } => FieldOrigin::CustomDecoder,
                            _ => FieldOrigin::Schema,
                        },
                        value,
                    });
                }
                None => {
                    let data: &'record [u8] = subrecord.as_bytes();
                    let declared = grammar.declared_signatures.contains(&subrecord.signature);
                    fields.push(Field {
                        node_id: bethkit_schema::SchemaNodeId(u32::MAX),
                        path: format!(
                            "{}.{}.{}",
                            if declared { "unmatched" } else { "unknown" },
                            subrecord.signature,
                            occurrence
                        ),
                        name: if declared {
                            "Out-of-order known subrecord".to_owned()
                        } else {
                            "Unknown subrecord".to_owned()
                        },
                        subrecord_signature: subrecord.signature,
                        occurrence,
                        span: ByteSpan {
                            start: 0,
                            end: data.len(),
                        },
                        origin: if declared {
                            FieldOrigin::UnmatchedKnownSubrecord
                        } else {
                            FieldOrigin::UnknownSubrecord
                        },
                        value: FieldValue::Bytes(Cow::Borrowed(data)),
                    });
                }
            }
        }
        Ok(fields)
    }

    /// Validates required fields, duplicate constraints, unknown subrecords,
    /// payload decoding, and complete byte coverage.
    pub fn validate(&self) -> ValidationReport {
        let mut report = ValidationReport::new();
        let definitions: Vec<&SchemaNode> = top_level_subrecords(&self.schema.root);
        let subrecords: &[SubRecord] = match self.record.subrecords() {
            Ok(value) => value,
            Err(error) => {
                report.push(self.diagnostic(
                    DiagnosticSeverity::Error,
                    DiagnosticCode::InvalidPayload,
                    error.to_string(),
                    None,
                    None,
                ));
                return report;
            }
        };

        let grammar = match interpret(
            &self.schema.root,
            self.record.header.signature,
            self.record.header.form_version,
            subrecords,
        ) {
            Ok(value) => value,
            Err(error) => {
                report.push(self.diagnostic(
                    DiagnosticSeverity::Error,
                    DiagnosticCode::InvalidPayload,
                    error.to_string(),
                    None,
                    None,
                ));
                return report;
            }
        };
        for definition in &definitions {
            let matched = grammar
                .assignments
                .iter()
                .flatten()
                .any(|assigned| assigned.id == definition.id);
            if definition.required && !matched {
                report.push(self.diagnostic(
                    DiagnosticSeverity::Error,
                    DiagnosticCode::MissingRequired,
                    format!("required field {} is absent", definition.path),
                    Some(definition),
                    None,
                ));
            }
        }

        match self.fields() {
            Ok(fields) => {
                for field in fields {
                    if field.origin == FieldOrigin::UnknownSubrecord {
                        report.push(self.diagnostic(
                            DiagnosticSeverity::Warning,
                            DiagnosticCode::UnknownSubrecord,
                            format!(
                                "subrecord {} is not declared by the package",
                                field.subrecord_signature
                            ),
                            None,
                            Some(field.span),
                        ));
                    } else if field.origin == FieldOrigin::UnmatchedKnownSubrecord {
                        report.push(self.diagnostic(
                            DiagnosticSeverity::Error,
                            DiagnosticCode::InvalidOrder,
                            format!(
                                "subrecord {} appears outside its schema position",
                                field.subrecord_signature
                            ),
                            None,
                            Some(field.span),
                        ));
                    }
                }
            }
            Err(error) => {
                report.push(self.diagnostic(
                    DiagnosticSeverity::Error,
                    DiagnosticCode::InvalidPayload,
                    error.to_string(),
                    None,
                    None,
                ));
            }
        }
        report
    }

    fn decode_node<'a>(
        &self,
        node: &SchemaNode,
        payload: &'a [u8],
        current: &'a [u8],
        offset: usize,
    ) -> Result<FieldValue<'a>> {
        if let Some(condition) = &node.condition {
            let context = EvalContext {
                payload,
                form_version: self.record.header.form_version,
                record_signature: self.record.header.signature.into(),
            };
            if condition.evaluate(&context, 1024)? != EvalValue::Bool(true) {
                return Ok(FieldValue::Absent);
            }
        }

        let decoded = match &node.kind {
            SchemaNodeKind::Primitive { primitive } => {
                decode_primitive(primitive, current, self.localized, &node.path)
            }
            SchemaNodeKind::Struct { fields } => {
                let mut values: Vec<NamedValue<'a>> = Vec::with_capacity(fields.len());
                let mut cursor: usize = 0;
                for field in fields {
                    let remaining: &'a [u8] =
                        current.get(cursor..).ok_or_else(|| SemanticError::Decode {
                            path: field.path.clone(),
                            message: "struct cursor exceeded payload".to_owned(),
                        })?;
                    let value: FieldValue<'a> =
                        self.decode_node(field, payload, remaining, offset + cursor)?;
                    let consumed: usize = fixed_node_size(field).unwrap_or(remaining.len());
                    values.push(NamedValue {
                        node_id: field.id,
                        path: field.path.clone(),
                        name: field.name.clone(),
                        span: ByteSpan {
                            start: offset + cursor,
                            end: offset + cursor + consumed,
                        },
                        value,
                    });
                    cursor = cursor
                        .checked_add(consumed)
                        .ok_or_else(|| SemanticError::Decode {
                            path: field.path.clone(),
                            message: "struct cursor overflowed".to_owned(),
                        })?;
                }
                if cursor != current.len() {
                    return Err(SemanticError::Decode {
                        path: node.path.clone(),
                        message: format!(
                            "{} payload bytes were not consumed",
                            current.len().saturating_sub(cursor)
                        ),
                    });
                }
                Ok(FieldValue::Struct(values))
            }
            SchemaNodeKind::Array { element, count } => {
                let element_size: usize =
                    fixed_node_size(element).ok_or_else(|| SemanticError::Decode {
                        path: node.path.clone(),
                        message: "variable-size array requires a custom decoder".to_owned(),
                    })?;
                if element_size == 0 {
                    return Err(SemanticError::Decode {
                        path: node.path.clone(),
                        message: "array element size must not be zero".to_owned(),
                    });
                }
                let element_count: usize = match count {
                    ArrayCount::Fixed { count } => *count as usize,
                    ArrayCount::Remainder => current.len() / element_size,
                    ArrayCount::Expression { expression } => {
                        let context = EvalContext {
                            payload,
                            form_version: self.record.header.form_version,
                            record_signature: self.record.header.signature.into(),
                        };
                        let value: i64 = match expression.evaluate(&context, 1024)? {
                            EvalValue::Int(value) if value >= 0 => value,
                            _ => {
                                return Err(SemanticError::Decode {
                                    path: node.path.clone(),
                                    message: "array count expression is not non-negative"
                                        .to_owned(),
                                });
                            }
                        };
                        usize::try_from(value).map_err(|_| SemanticError::Decode {
                            path: node.path.clone(),
                            message: "array count exceeds platform size".to_owned(),
                        })?
                    }
                };
                let expected: usize = element_count.checked_mul(element_size).ok_or_else(|| {
                    SemanticError::Decode {
                        path: node.path.clone(),
                        message: "array byte length overflowed".to_owned(),
                    }
                })?;
                if expected != current.len() {
                    return Err(SemanticError::Decode {
                        path: node.path.clone(),
                        message: format!(
                            "array expects {expected} bytes, payload has {}",
                            current.len()
                        ),
                    });
                }
                let mut values: Vec<FieldValue<'a>> = Vec::with_capacity(element_count);
                for index in 0..element_count {
                    let start: usize = index * element_size;
                    let end: usize = start + element_size;
                    values.push(self.decode_node(
                        element,
                        payload,
                        &current[start..end],
                        offset + start,
                    )?);
                }
                Ok(FieldValue::Array(values))
            }
            SchemaNodeKind::Union { selector, variants } => {
                let context = EvalContext {
                    payload,
                    form_version: self.record.header.form_version,
                    record_signature: self.record.header.signature.into(),
                };
                let index: usize = match selector.evaluate(&context, 1024)? {
                    EvalValue::Int(value) if value >= 0 => {
                        usize::try_from(value).map_err(|_| SemanticError::Decode {
                            path: node.path.clone(),
                            message: "union selector exceeds platform size".to_owned(),
                        })?
                    }
                    _ => {
                        return Err(SemanticError::Decode {
                            path: node.path.clone(),
                            message: "union selector is not a non-negative integer".to_owned(),
                        });
                    }
                };
                let variant: &SchemaNode =
                    variants.get(index).ok_or_else(|| SemanticError::Decode {
                        path: node.path.clone(),
                        message: format!("union variant {index} does not exist"),
                    })?;
                self.decode_node(variant, payload, current, offset)
            }
            SchemaNodeKind::Custom { decoder, .. } => self
                .context
                .decoders()
                .get(decoder)
                .ok_or_else(|| SemanticError::MissingDecoder(decoder.clone()))?
                .decode(current),
            SchemaNodeKind::Compressed { .. } => Err(SemanticError::Decode {
                path: node.path.clone(),
                message: "compressed nodes require a registered custom decoder".to_owned(),
            }),
            SchemaNodeKind::Reference { target } => Err(SemanticError::Decode {
                path: node.path.clone(),
                message: format!("unresolved schema reference {target}"),
            }),
            SchemaNodeKind::Sequence { .. }
            | SchemaNodeKind::Choice { .. }
            | SchemaNodeKind::Repeat { .. }
            | SchemaNodeKind::Subrecord { .. } => Err(SemanticError::Decode {
                path: node.path.clone(),
                message: "container node cannot decode a payload directly".to_owned(),
            }),
        }?;
        self.context
            .apply_value_callbacks(&node.path, self.record, decoded)
    }

    fn diagnostic(
        &self,
        severity: DiagnosticSeverity,
        code: DiagnosticCode,
        message: String,
        node: Option<&SchemaNode>,
        span: Option<ByteSpan>,
    ) -> Diagnostic {
        Diagnostic {
            severity,
            code,
            message,
            record_signature: self.record.header.signature,
            form_id: self.record.header.form_id,
            node_id: node.map(|value| value.id),
            path: node.map(|value| value.path.clone()),
            span,
        }
    }
}

fn top_level_subrecords(root: &SchemaNode) -> Vec<&SchemaNode> {
    fn collect<'a>(node: &'a SchemaNode, output: &mut Vec<&'a SchemaNode>) {
        match &node.kind {
            SchemaNodeKind::Subrecord { .. } => output.push(node),
            SchemaNodeKind::Sequence { children } => {
                for child in children {
                    collect(child, output);
                }
            }
            SchemaNodeKind::Choice { alternatives } => {
                for alternative in alternatives {
                    collect(alternative, output);
                }
            }
            SchemaNodeKind::Repeat { child, .. } => collect(child, output),
            _ => {}
        }
    }

    let mut output: Vec<&SchemaNode> = Vec::new();
    collect(root, &mut output);
    output
}

fn decode_primitive<'a>(
    primitive: &PrimitiveType,
    data: &'a [u8],
    localized: bool,
    path: &str,
) -> Result<FieldValue<'a>> {
    match primitive {
        PrimitiveType::Integer { integer } => decode_integer(*integer, data, path),
        PrimitiveType::Float { width, byte_order } => decode_float(*width, *byte_order, data, path),
        PrimitiveType::String { string } => decode_string(string, data, localized, path),
        PrimitiveType::Bytes { length } => {
            if let Some(expected) = length {
                if data.len() != *expected as usize {
                    return Err(decode_length_error(path, *expected as usize, data.len()));
                }
            }
            Ok(FieldValue::Bytes(Cow::Borrowed(data)))
        }
        PrimitiveType::FormId { targets } => {
            if data.len() != 4 {
                return Err(decode_length_error(path, 4, data.len()));
            }
            let raw: u32 = u32::from_le_bytes(data.try_into().expect("FormID length was checked"));
            Ok(FieldValue::FormId {
                value: FormId(raw),
                targets: targets.iter().copied().map(Signature::from).collect(),
            })
        }
        PrimitiveType::Enumeration { integer, values } => {
            let raw: i64 = integer_as_i64(*integer, data, path)?;
            let name: Option<String> = values
                .iter()
                .find(|(value, _)| *value == raw)
                .map(|(_, name)| name.clone());
            Ok(FieldValue::Enumeration { value: raw, name })
        }
        PrimitiveType::Flags { integer, bits } => {
            let raw: u64 = integer_as_u64(*integer, data, path)?;
            let active: Vec<String> = bits
                .iter()
                .filter(|(bit, _)| *bit < 64 && raw & (1_u64 << bit) != 0)
                .map(|(_, name)| name.clone())
                .collect();
            Ok(FieldValue::Flags { value: raw, active })
        }
        PrimitiveType::Unused { length } => {
            if data.len() != *length as usize {
                return Err(decode_length_error(path, *length as usize, data.len()));
            }
            Ok(FieldValue::Bytes(Cow::Borrowed(data)))
        }
    }
}

fn decode_integer<'a>(integer: IntegerType, data: &'a [u8], path: &str) -> Result<FieldValue<'a>> {
    if integer.signed {
        integer_as_i64(integer, data, path).map(FieldValue::Int)
    } else {
        integer_as_u64(integer, data, path).map(FieldValue::UInt)
    }
}

fn integer_as_u64(integer: IntegerType, data: &[u8], path: &str) -> Result<u64> {
    if data.len() != integer.width as usize {
        return Err(decode_length_error(
            path,
            integer.width as usize,
            data.len(),
        ));
    }
    let value: u64 = match (integer.width, integer.byte_order) {
        (1, _) => u64::from(data[0]),
        (2, ByteOrder::LittleEndian) => u64::from(u16::from_le_bytes([data[0], data[1]])),
        (2, ByteOrder::BigEndian) => u64::from(u16::from_be_bytes([data[0], data[1]])),
        (4, ByteOrder::LittleEndian) => u64::from(u32::from_le_bytes(
            data.try_into().expect("integer length was checked"),
        )),
        (4, ByteOrder::BigEndian) => u64::from(u32::from_be_bytes(
            data.try_into().expect("integer length was checked"),
        )),
        (8, ByteOrder::LittleEndian) => {
            u64::from_le_bytes(data.try_into().expect("integer length was checked"))
        }
        (8, ByteOrder::BigEndian) => {
            u64::from_be_bytes(data.try_into().expect("integer length was checked"))
        }
        _ => {
            return Err(SemanticError::Decode {
                path: path.to_owned(),
                message: format!("unsupported integer width {}", integer.width),
            });
        }
    };
    Ok(value)
}

fn integer_as_i64(integer: IntegerType, data: &[u8], path: &str) -> Result<i64> {
    if !integer.signed {
        return i64::try_from(integer_as_u64(integer, data, path)?).map_err(|_| {
            SemanticError::Decode {
                path: path.to_owned(),
                message: "unsigned integer exceeds i64".to_owned(),
            }
        });
    }
    if data.len() != integer.width as usize {
        return Err(decode_length_error(
            path,
            integer.width as usize,
            data.len(),
        ));
    }
    let value: i64 = match (integer.width, integer.byte_order) {
        (1, _) => i64::from(data[0] as i8),
        (2, ByteOrder::LittleEndian) => i64::from(i16::from_le_bytes([data[0], data[1]])),
        (2, ByteOrder::BigEndian) => i64::from(i16::from_be_bytes([data[0], data[1]])),
        (4, ByteOrder::LittleEndian) => i64::from(i32::from_le_bytes(
            data.try_into().expect("integer length was checked"),
        )),
        (4, ByteOrder::BigEndian) => i64::from(i32::from_be_bytes(
            data.try_into().expect("integer length was checked"),
        )),
        (8, ByteOrder::LittleEndian) => {
            i64::from_le_bytes(data.try_into().expect("integer length was checked"))
        }
        (8, ByteOrder::BigEndian) => {
            i64::from_be_bytes(data.try_into().expect("integer length was checked"))
        }
        _ => {
            return Err(SemanticError::Decode {
                path: path.to_owned(),
                message: format!("unsupported integer width {}", integer.width),
            });
        }
    };
    Ok(value)
}

fn decode_float<'a>(
    width: u8,
    byte_order: ByteOrder,
    data: &'a [u8],
    path: &str,
) -> Result<FieldValue<'a>> {
    if data.len() != width as usize {
        return Err(decode_length_error(path, width as usize, data.len()));
    }
    let value: f64 = match (width, byte_order) {
        (4, ByteOrder::LittleEndian) => f64::from(f32::from_le_bytes(
            data.try_into().expect("float length was checked"),
        )),
        (4, ByteOrder::BigEndian) => f64::from(f32::from_be_bytes(
            data.try_into().expect("float length was checked"),
        )),
        (8, ByteOrder::LittleEndian) => {
            f64::from_le_bytes(data.try_into().expect("float length was checked"))
        }
        (8, ByteOrder::BigEndian) => {
            f64::from_be_bytes(data.try_into().expect("float length was checked"))
        }
        _ => {
            return Err(SemanticError::Decode {
                path: path.to_owned(),
                message: format!("unsupported float width {width}"),
            });
        }
    };
    Ok(FieldValue::Float(value))
}

fn decode_string<'a>(
    string: &StringType,
    data: &'a [u8],
    localized: bool,
    path: &str,
) -> Result<FieldValue<'a>> {
    if string.encoding == "localized" && localized {
        if data.len() != 4 {
            return Err(decode_length_error(path, 4, data.len()));
        }
        let id: u32 = u32::from_le_bytes(
            data.try_into()
                .expect("localized string length was checked"),
        );
        return Ok(FieldValue::UInt(u64::from(id)));
    }
    if let Some(length) = string.fixed_length {
        if data.len() != length as usize {
            return Err(decode_length_error(path, length as usize, data.len()));
        }
    }
    let bytes: &'a [u8] = if string.zero_terminated {
        let end: usize = data
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(data.len());
        &data[..end]
    } else {
        data
    };
    if string.encoding != "utf8" && string.encoding != "localized" {
        return Err(SemanticError::Decode {
            path: path.to_owned(),
            message: format!(
                "string encoding {} requires a custom decoder",
                string.encoding
            ),
        });
    }
    let value: &'a str = std::str::from_utf8(bytes).map_err(|error| SemanticError::Decode {
        path: path.to_owned(),
        message: error.to_string(),
    })?;
    Ok(FieldValue::String(Cow::Borrowed(value)))
}

fn fixed_node_size(node: &SchemaNode) -> Option<usize> {
    match &node.kind {
        SchemaNodeKind::Primitive { primitive } => match primitive {
            PrimitiveType::Integer { integer } => Some(integer.width as usize),
            PrimitiveType::Float { width, .. } => Some(*width as usize),
            PrimitiveType::String { string } => string.fixed_length.map(|value| value as usize),
            PrimitiveType::Bytes { length } => length.map(|value| value as usize),
            PrimitiveType::FormId { .. } => Some(4),
            PrimitiveType::Enumeration { integer, .. } | PrimitiveType::Flags { integer, .. } => {
                Some(integer.width as usize)
            }
            PrimitiveType::Unused { length } => Some(*length as usize),
        },
        SchemaNodeKind::Struct { fields } => fields.iter().try_fold(0_usize, |total, field| {
            total.checked_add(fixed_node_size(field)?)
        }),
        SchemaNodeKind::Array { element, count } => {
            let size: usize = fixed_node_size(element)?;
            match count {
                ArrayCount::Fixed { count } => size.checked_mul(*count as usize),
                ArrayCount::Expression { .. } | ArrayCount::Remainder => None,
            }
        }
        _ => None,
    }
}

fn decode_length_error(path: &str, expected: usize, actual: usize) -> SemanticError {
    SemanticError::Decode {
        path: path.to_owned(),
        message: format!("expected {expected} bytes, got {actual}"),
    }
}
