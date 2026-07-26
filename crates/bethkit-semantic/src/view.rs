// SPDX-License-Identifier: Apache-2.0
//!
//! Ordered schema-guided views over parsed records.

use std::borrow::Cow;

use bethkit_core::{FormId, Record, Signature, SubRecord};
use bethkit_schema::{
    ArrayCount, ByteOrder, EvalContext, EvalValue, IntegerType, PrimitiveType, SchemaNode,
    SchemaNodeKind, SchemaRecord, StringType,
};

use crate::value::float_from_raw;
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

    /// Formats a decoded value with the xEdit callback bound to its exact path.
    ///
    /// The typed value remains unchanged. `None` means that no executable
    /// formatter is bound to the path.
    ///
    /// # Errors
    ///
    /// Returns [`SemanticError::Handler`] when the formatter rejects the value.
    pub fn format_value(&self, path: &str, value: &FieldValue<'_>) -> Result<Option<String>> {
        self.context.format_value(self.record, path, value)
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
        for violation in &grammar.violations {
            report.push(self.diagnostic(
                DiagnosticSeverity::Error,
                DiagnosticCode::MissingRequired,
                format!(
                    "repeat {} requires at least {} entries, found {}",
                    violation.path, violation.minimum, violation.actual
                ),
                None,
                None,
            ));
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
        if !self.node_applies(node, payload)? {
            return Ok(FieldValue::Absent);
        }

        let decoded = match &node.kind {
            SchemaNodeKind::Primitive { primitive } => {
                decode_primitive(primitive, current, self.localized, &node.path)
            }
            SchemaNodeKind::Struct { fields } => {
                let mut values: Vec<NamedValue<'a>> = Vec::with_capacity(fields.len());
                let mut cursor: usize = 0;
                for field in fields {
                    if !self.node_applies(field, payload)? {
                        values.push(NamedValue {
                            node_id: field.id,
                            path: field.path.clone(),
                            name: field.name.clone(),
                            span: ByteSpan {
                                start: offset + cursor,
                                end: offset + cursor,
                            },
                            value: FieldValue::Absent,
                        });
                        continue;
                    }
                    let remaining: &'a [u8] =
                        current.get(cursor..).ok_or_else(|| SemanticError::Decode {
                            path: field.path.clone(),
                            message: "struct cursor exceeded payload".to_owned(),
                        })?;
                    let consumed: usize = node_data_size(field, remaining, self.localized)?;
                    let field_data: &'a [u8] =
                        remaining
                            .get(..consumed)
                            .ok_or_else(|| SemanticError::Decode {
                                path: field.path.clone(),
                                message: format!(
                                    "field needs {consumed} bytes, only {} remain",
                                    remaining.len()
                                ),
                            })?;
                    let value: FieldValue<'a> =
                        self.decode_node(field, payload, field_data, offset + cursor)?;
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
            SchemaNodeKind::Custom { decoder, .. } => {
                let decoded = self
                    .context
                    .decoders()
                    .get(decoder)
                    .ok_or_else(|| SemanticError::MissingDecoder(decoder.clone()))?
                    .decode(current)?;
                if decoded.consumed != current.len() {
                    return Err(SemanticError::Decode {
                        path: node.path.clone(),
                        message: format!(
                            "custom decoder consumed {} of {} payload bytes",
                            decoded.consumed,
                            current.len()
                        ),
                    });
                }
                Ok(decoded.value)
            }
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
        let normalized = self
            .context
            .apply_normalizers(&node.path, self.record, decoded)?;
        apply_float_read_semantics(node, normalized)
    }

    fn node_applies(&self, node: &SchemaNode, payload: &[u8]) -> Result<bool> {
        let Some(condition) = &node.condition else {
            return Ok(true);
        };
        let context = EvalContext {
            payload,
            form_version: self.record.header.form_version,
            record_signature: self.record.header.signature.into(),
        };
        match condition.evaluate(&context, 1024)? {
            EvalValue::Bool(value) => Ok(value),
            _ => Err(SemanticError::Decode {
                path: node.path.clone(),
                message: "field condition did not return a boolean".to_owned(),
            }),
        }
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
        PrimitiveType::Float {
            width, byte_order, ..
        } => decode_float(*width, *byte_order, data, path),
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

fn apply_float_read_semantics<'a>(
    node: &SchemaNode,
    value: FieldValue<'a>,
) -> Result<FieldValue<'a>> {
    let SchemaNodeKind::Primitive {
        primitive: PrimitiveType::Float { scale, digits, .. },
    } = &node.kind
    else {
        return Ok(value);
    };
    let FieldValue::Float(value) = value else {
        return Err(SemanticError::Decode {
            path: node.path.clone(),
            message: "float schema node decoded a non-floating-point value".to_owned(),
        });
    };
    Ok(FieldValue::Float(float_from_raw(value, *scale, *digits)))
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
    if is_localized_string(string) && localized {
        if data.len() != 4 {
            return Err(decode_length_error(path, 4, data.len()));
        }
        let id: u32 = u32::from_le_bytes(
            data.try_into()
                .expect("localized string length was checked"),
        );
        return Ok(FieldValue::UInt(u64::from(id)));
    }
    let bytes: &'a [u8] = string_body(string, data, path)?;
    let bytes: &'a [u8] = if string.zero_terminated {
        let end: usize = bytes
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(bytes.len());
        &bytes[..end]
    } else {
        bytes
    };
    let value: Cow<'a, str> = match text_encoding(string) {
        "utf8" => {
            Cow::Borrowed(
                std::str::from_utf8(bytes).map_err(|error| SemanticError::Decode {
                    path: path.to_owned(),
                    message: error.to_string(),
                })?,
            )
        }
        "windows_1252" => {
            let (value, had_errors) = encoding_rs::WINDOWS_1252.decode_without_bom_handling(bytes);
            if had_errors {
                return Err(SemanticError::Decode {
                    path: path.to_owned(),
                    message: "invalid Windows-1252 byte sequence".to_owned(),
                });
            }
            value
        }
        encoding => {
            return Err(SemanticError::Decode {
                path: path.to_owned(),
                message: format!("string encoding {encoding} requires a custom decoder"),
            });
        }
    };
    Ok(FieldValue::String(value))
}

fn is_localized_string(string: &StringType) -> bool {
    string.localized || string.encoding == "localized"
}

fn text_encoding(string: &StringType) -> &str {
    if string.encoding == "localized" {
        "windows_1252"
    } else {
        &string.encoding
    }
}

fn string_body<'a>(string: &StringType, data: &'a [u8], path: &str) -> Result<&'a [u8]> {
    let representation: &'a [u8] = if let Some(terminator) = string.trailing_terminator {
        let (&actual, body) = data.split_last().ok_or_else(|| SemanticError::Decode {
            path: path.to_owned(),
            message: "string is missing its structural terminator".to_owned(),
        })?;
        if actual != terminator {
            return Err(SemanticError::Decode {
                path: path.to_owned(),
                message: format!(
                    "expected structural terminator 0x{terminator:02X}, got 0x{actual:02X}"
                ),
            });
        }
        body
    } else {
        data
    };
    if let Some(prefix) = string.length_prefix {
        if representation.len() < prefix.offset as usize {
            return Err(decode_length_error(
                path,
                prefix.offset as usize,
                representation.len(),
            ));
        }
        let length: usize = read_string_length(prefix.width, representation, path)?;
        let end: usize = (prefix.offset as usize)
            .checked_add(length)
            .ok_or_else(|| SemanticError::Decode {
                path: path.to_owned(),
                message: "string length overflowed".to_owned(),
            })?;
        if end != representation.len() {
            return Err(decode_length_error(path, end, representation.len()));
        }
        return Ok(&representation[prefix.offset as usize..end]);
    }
    if let Some(length) = string.fixed_length {
        if representation.len() != length as usize {
            return Err(decode_length_error(
                path,
                length as usize,
                representation.len(),
            ));
        }
    }
    Ok(representation)
}

fn read_string_length(width: u8, data: &[u8], path: &str) -> Result<usize> {
    let length: u32 = match width {
        1 if !data.is_empty() => u32::from(data[0]),
        2 if data.len() >= 2 => u32::from(u16::from_le_bytes([data[0], data[1]])),
        4 if data.len() >= 4 => {
            u32::from_le_bytes(data[..4].try_into().expect("prefix length was checked"))
        }
        1 | 2 | 4 => return Err(decode_length_error(path, width as usize, data.len())),
        _ => {
            return Err(SemanticError::Decode {
                path: path.to_owned(),
                message: format!("unsupported string length prefix width {width}"),
            });
        }
    };
    usize::try_from(length).map_err(|_| SemanticError::Decode {
        path: path.to_owned(),
        message: "string length exceeds platform size".to_owned(),
    })
}

fn node_data_size(node: &SchemaNode, data: &[u8], localized: bool) -> Result<usize> {
    if let SchemaNodeKind::Primitive {
        primitive: PrimitiveType::String { string },
    } = &node.kind
    {
        if is_localized_string(string) && localized {
            return Ok(4);
        }
        let trailing: usize = usize::from(string.trailing_terminator.is_some());
        if let Some(prefix) = string.length_prefix {
            let length: usize = read_string_length(prefix.width, data, &node.path)?;
            return (prefix.offset as usize)
                .checked_add(length)
                .and_then(|value| value.checked_add(trailing))
                .ok_or_else(|| SemanticError::Decode {
                    path: node.path.clone(),
                    message: "string size overflowed".to_owned(),
                });
        }
        if let Some(length) = string.fixed_length {
            return (length as usize)
                .checked_add(trailing)
                .ok_or_else(|| SemanticError::Decode {
                    path: node.path.clone(),
                    message: "string size overflowed".to_owned(),
                });
        }
        if string.zero_terminated {
            let body_length: usize = data
                .iter()
                .position(|byte| *byte == 0)
                .map_or(data.len(), |index| index + 1);
            return body_length
                .checked_add(trailing)
                .ok_or_else(|| SemanticError::Decode {
                    path: node.path.clone(),
                    message: "string size overflowed".to_owned(),
                });
        }
    }
    Ok(fixed_node_size(node).unwrap_or(data.len()))
}

fn fixed_node_size(node: &SchemaNode) -> Option<usize> {
    match &node.kind {
        SchemaNodeKind::Primitive { primitive } => match primitive {
            PrimitiveType::Integer { integer } => Some(integer.width as usize),
            PrimitiveType::Float { width, .. } => Some(*width as usize),
            PrimitiveType::String { string } if !is_localized_string(string) => {
                string.fixed_length.and_then(|value| {
                    (value as usize).checked_add(usize::from(string.trailing_terminator.is_some()))
                })
            }
            PrimitiveType::String { .. } => None,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_1252_strings_decode_without_losing_non_ascii_bytes() {
        let string = StringType {
            encoding: "windows_1252".to_owned(),
            localized: false,
            zero_terminated: true,
            fixed_length: None,
            length_prefix: None,
            trailing_terminator: None,
        };

        let value = decode_string(&string, b"Gr\xfc\xdfe\0ignored", false, "TEST")
            .expect("Windows-1252 string should decode");

        match value {
            FieldValue::String(value) => assert_eq!(value, "Grüße"),
            other => panic!("expected string, got {other:?}"),
        }
    }

    #[test]
    fn windows_1252_strings_preserve_control_bytes() {
        let string = StringType {
            encoding: "windows_1252".to_owned(),
            localized: false,
            zero_terminated: false,
            fixed_length: None,
            length_prefix: None,
            trailing_terminator: None,
        };

        let value =
            decode_string(&string, b"\x81", false, "TEST").expect("control byte should decode");

        match value {
            FieldValue::String(value) => assert_eq!(value, "\u{81}"),
            other => panic!("expected string, got {other:?}"),
        }
    }

    #[test]
    fn length_prefixed_strings_decode_padding_and_structural_terminator() {
        let string = StringType {
            encoding: "utf8".to_owned(),
            localized: false,
            zero_terminated: false,
            fixed_length: None,
            length_prefix: Some(bethkit_schema::StringLengthPrefix {
                width: 1,
                offset: 2,
            }),
            trailing_terminator: Some(b'|'),
        };

        let value = decode_string(&string, b"\x03\0abc|", false, "TEST")
            .expect("length-prefixed string should decode");

        match value {
            FieldValue::String(value) => assert_eq!(value, "abc"),
            other => panic!("expected string, got {other:?}"),
        }
    }

    #[test]
    fn localized_strings_decode_exact_table_ids() {
        let string = StringType {
            encoding: "windows_1252".to_owned(),
            localized: true,
            zero_terminated: true,
            fixed_length: None,
            length_prefix: None,
            trailing_terminator: Some(b'|'),
        };

        let bytes = 0x1234_5678_u32.to_le_bytes();
        let value = decode_string(&string, &bytes, true, "TEST")
            .expect("localized string ID should decode");

        match value {
            FieldValue::UInt(value) => assert_eq!(value, 0x1234_5678),
            other => panic!("expected string-table ID, got {other:?}"),
        }
    }
}
