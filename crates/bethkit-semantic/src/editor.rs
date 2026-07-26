// SPDX-License-Identifier: Apache-2.0
//!
//! Lossless schema-guided record editing.

use bethkit_core::{Record, Signature, WritableRecord, WritableSubRecord};
use bethkit_schema::{
    ArrayCount, ByteOrder, CallbackImplementation, IntegerType, PrimitiveType, SchemaNode,
    SchemaNodeKind,
};

use crate::value::float_to_raw;
use crate::{
    FieldValue, HandlerMutation, HandlerOutput, OwnedFieldValue, Result, SemanticContext,
    SemanticError, SemanticHandlerRegistry,
};

/// Lossless editor for one record.
pub struct RecordEditor {
    registry: bethkit_schema::SchemaRegistry,
    decoders: crate::DecoderRegistry,
    handlers: SemanticHandlerRegistry,
    record: WritableRecord,
    localized: bool,
}

impl RecordEditor {
    pub(crate) fn new(
        context: &SemanticContext,
        record: &Record,
        plugin_localized: bool,
    ) -> Result<Self> {
        if context.registry().get(record.header.signature).is_none() {
            return Err(SemanticError::MissingRecordSchema(
                record.header.signature.to_string(),
            ));
        }
        let subrecords: Vec<WritableSubRecord> = record
            .subrecords()?
            .iter()
            .map(|subrecord| WritableSubRecord {
                signature: subrecord.signature,
                data: subrecord.as_bytes().to_vec(),
            })
            .collect();
        Ok(Self {
            registry: context.registry().clone(),
            decoders: context.decoders().clone(),
            handlers: context.handlers().clone(),
            record: WritableRecord {
                signature: record.header.signature,
                flags: record.header.flags,
                form_id: record.header.form_id,
                form_version: record.header.form_version,
                subrecords,
            },
            localized: plugin_localized,
        })
    }

    /// Replaces an existing top-level field occurrence with a typed value.
    ///
    /// # Errors
    ///
    /// Returns [`SemanticError`] when the path, occurrence, or value is
    /// invalid for the loaded schema.
    pub fn set(&mut self, path: &str, occurrence: usize, value: &OwnedFieldValue) -> Result<()> {
        let node: &SchemaNode = self.find_node(path)?;
        let SchemaNodeKind::Subrecord { signature, payload } = &node.kind else {
            return Err(SemanticError::Encode {
                path: path.to_owned(),
                message: "only top-level subrecords can be replaced".to_owned(),
            });
        };
        let target_signature: Signature = (*signature).into();
        let index: usize = self
            .record
            .subrecords
            .iter()
            .enumerate()
            .filter(|(_, subrecord)| subrecord.signature == target_signature)
            .nth(occurrence)
            .map(|(index, _)| index)
            .ok_or_else(|| SemanticError::MissingOccurrence {
                path: path.to_owned(),
                occurrence,
            })?;
        let normalized = self.normalize_value(&payload.path, value)?;
        let encoded: Vec<u8> = self.encode_node(payload, &normalized)?;
        let mut candidate = clone_record(&self.record);
        candidate.subrecords[index].data = encoded;
        self.apply_after_set(&payload.path, &normalized, &mut candidate)?;
        self.record = candidate;
        Ok(())
    }

    /// Inserts a new top-level field after existing occurrences of its
    /// signature.
    ///
    /// # Errors
    ///
    /// Returns [`SemanticError`] when the path or value is invalid.
    pub fn insert(&mut self, path: &str, value: &OwnedFieldValue) -> Result<()> {
        let node: &SchemaNode = self.find_node(path)?;
        let SchemaNodeKind::Subrecord { signature, payload } = &node.kind else {
            return Err(SemanticError::Encode {
                path: path.to_owned(),
                message: "only top-level subrecords can be inserted".to_owned(),
            });
        };
        let target_signature: Signature = (*signature).into();
        let schema = self
            .registry
            .get(self.record.signature)
            .ok_or_else(|| SemanticError::MissingRecordSchema(self.record.signature.to_string()))?;
        let ordered = top_level_subrecords(&schema.root);
        let target_order = ordered
            .iter()
            .position(|candidate| candidate.id == node.id)
            .ok_or_else(|| SemanticError::MissingPath(path.to_owned()))?;
        let insertion_index = self
            .record
            .subrecords
            .iter()
            .position(|subrecord| {
                first_signature_order(&ordered, subrecord.signature)
                    .is_some_and(|order| order > target_order)
            })
            .unwrap_or(self.record.subrecords.len());
        let normalized = self.normalize_value(&payload.path, value)?;
        let encoded: Vec<u8> = self.encode_node(payload, &normalized)?;
        let mut candidate = clone_record(&self.record);
        candidate.subrecords.insert(
            insertion_index,
            WritableSubRecord {
                signature: target_signature,
                data: encoded,
            },
        );
        self.apply_after_set(&payload.path, &normalized, &mut candidate)?;
        self.record = candidate;
        Ok(())
    }

    /// Removes a top-level field occurrence.
    ///
    /// # Errors
    ///
    /// Returns [`SemanticError`] when the path or occurrence is absent.
    pub fn remove(&mut self, path: &str, occurrence: usize) -> Result<()> {
        let node: &SchemaNode = self.find_node(path)?;
        let SchemaNodeKind::Subrecord { signature, .. } = &node.kind else {
            return Err(SemanticError::Encode {
                path: path.to_owned(),
                message: "only top-level subrecords can be removed".to_owned(),
            });
        };
        let target_signature: Signature = (*signature).into();
        let index: usize = self
            .record
            .subrecords
            .iter()
            .enumerate()
            .filter(|(_, subrecord)| subrecord.signature == target_signature)
            .nth(occurrence)
            .map(|(index, _)| index)
            .ok_or_else(|| SemanticError::MissingOccurrence {
                path: path.to_owned(),
                occurrence,
            })?;
        let mut candidate = clone_record(&self.record);
        candidate.subrecords.remove(index);
        self.record = candidate;
        Ok(())
    }

    /// Returns the edited writable record without discarding unknown
    /// subrecords.
    pub fn into_writable_record(self) -> WritableRecord {
        self.record
    }

    /// Returns whether the parent plugin uses localized string tables.
    pub fn is_localized(&self) -> bool {
        self.localized
    }

    fn find_node(&self, path: &str) -> Result<&SchemaNode> {
        let schema = self
            .registry
            .get(self.record.signature)
            .ok_or_else(|| SemanticError::MissingRecordSchema(self.record.signature.to_string()))?;
        find_node_by_path(&schema.root, path)
            .ok_or_else(|| SemanticError::MissingPath(path.to_owned()))
    }

    fn encode_node(&self, node: &SchemaNode, value: &OwnedFieldValue) -> Result<Vec<u8>> {
        match &node.kind {
            SchemaNodeKind::Primitive { primitive } => {
                encode_primitive(primitive, value, self.localized, &node.path)
            }
            SchemaNodeKind::Struct { fields } => {
                let OwnedFieldValue::Struct(values) = value else {
                    return Err(encode_error(&node.path, "expected a struct value"));
                };
                if fields.len() != values.len() {
                    return Err(encode_error(
                        &node.path,
                        format!(
                            "struct expects {} fields, got {}",
                            fields.len(),
                            values.len()
                        ),
                    ));
                }
                let mut output: Vec<u8> = Vec::new();
                for (field, value) in fields.iter().zip(values) {
                    output.extend(self.encode_node(field, value)?);
                }
                Ok(output)
            }
            SchemaNodeKind::Array { element, count } => {
                let OwnedFieldValue::Array(values) = value else {
                    return Err(encode_error(&node.path, "expected an array value"));
                };
                let mut output: Vec<u8> = Vec::new();
                match count {
                    ArrayCount::Fixed { count } if values.len() != *count as usize => {
                        return Err(encode_error(
                            &node.path,
                            format!("array expects {count} elements, got {}", values.len()),
                        ));
                    }
                    ArrayCount::Prefixed { integer } => {
                        if integer.signed {
                            return Err(encode_error(
                                &node.path,
                                "array count prefix must be unsigned",
                            ));
                        }
                        let count: u64 = u64::try_from(values.len())
                            .map_err(|_| encode_error(&node.path, "array count exceeds u64"))?;
                        output.extend(encode_integer(*integer, count, &node.path)?);
                    }
                    ArrayCount::Expression { .. } => {
                        return Err(encode_error(
                            &node.path,
                            "expression-counted array requires a specialized encoder",
                        ));
                    }
                    ArrayCount::Fixed { .. } | ArrayCount::Remainder => {}
                }
                for value in values {
                    output.extend(self.encode_node(element, value)?);
                }
                Ok(output)
            }
            SchemaNodeKind::Custom { decoder, .. } => self
                .decoders
                .get(decoder)
                .ok_or_else(|| SemanticError::MissingDecoder(decoder.clone()))?
                .encode(value),
            _ => Err(SemanticError::Encode {
                path: node.path.clone(),
                message: "this schema node requires a specialized encoder".to_owned(),
            }),
        }
    }

    fn normalize_value(&self, path: &str, value: &OwnedFieldValue) -> Result<OwnedFieldValue> {
        let node = self.find_node(path)?;
        let mut normalized = match (&node.kind, value) {
            (
                SchemaNodeKind::Primitive {
                    primitive: PrimitiveType::Float { scale, digits, .. },
                },
                OwnedFieldValue::Float(value),
            ) if value.is_finite() => OwnedFieldValue::Float(float_to_raw(*value, *scale, *digits)),
            _ => value.clone(),
        };
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
            let handler_value = owned_to_handler_value(&normalized);
            if matches!(&handler_value, FieldValue::Float(value) if !value.is_finite()) {
                continue;
            }
            normalized = match self.handlers.invoke(
                binding,
                self.record.signature,
                self.record.form_id,
                self.record.form_version,
                self.registry.package().manifest().game,
                Some(&handler_value),
            )? {
                HandlerOutput::Value(value) => handler_to_owned_value(value, path)?,
                _ => {
                    return Err(SemanticError::Handler {
                        handler: binding.callback_id.clone(),
                        message: "normalizer returned a non-value result".to_owned(),
                    });
                }
            };
        }
        Ok(normalized)
    }

    fn apply_after_set(
        &self,
        path: &str,
        value: &OwnedFieldValue,
        record: &mut WritableRecord,
    ) -> Result<()> {
        for binding in self
            .registry
            .package()
            .callback_bindings()
            .iter()
            .filter(|binding| binding.path == path && binding.callback_id == "def.after_set")
        {
            if !matches!(
                binding.implementation,
                CallbackImplementation::BuiltIn { .. }
                    | CallbackImplementation::CustomHandler { .. }
            ) {
                continue;
            }
            let handler_value = owned_to_handler_value(value);
            match self.handlers.invoke(
                binding,
                record.signature,
                record.form_id,
                record.form_version,
                self.registry.package().manifest().game,
                Some(&handler_value),
            )? {
                HandlerOutput::None => {}
                HandlerOutput::Mutations(mutations) => {
                    self.apply_mutations(record, mutations)?;
                }
                _ => {
                    return Err(SemanticError::Handler {
                        handler: binding.callback_id.clone(),
                        message: "after-set handler returned an invalid result".to_owned(),
                    });
                }
            }
        }
        Ok(())
    }

    fn apply_mutations(
        &self,
        record: &mut WritableRecord,
        mutations: Vec<HandlerMutation>,
    ) -> Result<()> {
        for mutation in mutations {
            match mutation {
                HandlerMutation::Set {
                    path,
                    occurrence,
                    value,
                } => {
                    let (signature, encoded) = self.encode_path(&path, &value)?;
                    let target = record
                        .subrecords
                        .iter_mut()
                        .filter(|subrecord| subrecord.signature == signature)
                        .nth(occurrence)
                        .ok_or(SemanticError::MissingOccurrence { path, occurrence })?;
                    target.data = encoded;
                }
                HandlerMutation::Insert { path, value } => {
                    let (signature, encoded) = self.encode_path(&path, &value)?;
                    record.subrecords.push(WritableSubRecord {
                        signature,
                        data: encoded,
                    });
                }
                HandlerMutation::Remove { path, occurrence } => {
                    let node = self.find_node(&path)?;
                    let SchemaNodeKind::Subrecord { signature, .. } = &node.kind else {
                        return Err(SemanticError::Encode {
                            path,
                            message: "handler mutation path is not a subrecord".to_owned(),
                        });
                    };
                    let signature = Signature::from(*signature);
                    let index = record
                        .subrecords
                        .iter()
                        .enumerate()
                        .filter(|(_, subrecord)| subrecord.signature == signature)
                        .nth(occurrence)
                        .map(|(index, _)| index)
                        .ok_or(SemanticError::MissingOccurrence { path, occurrence })?;
                    record.subrecords.remove(index);
                }
            }
        }
        Ok(())
    }

    fn encode_path(&self, path: &str, value: &OwnedFieldValue) -> Result<(Signature, Vec<u8>)> {
        let node = self.find_node(path)?;
        let SchemaNodeKind::Subrecord { signature, payload } = &node.kind else {
            return Err(SemanticError::Encode {
                path: path.to_owned(),
                message: "handler mutation path is not a subrecord".to_owned(),
            });
        };
        Ok((
            Signature::from(*signature),
            self.encode_node(payload, value)?,
        ))
    }
}

fn clone_record(record: &WritableRecord) -> WritableRecord {
    WritableRecord {
        signature: record.signature,
        flags: record.flags,
        form_id: record.form_id,
        form_version: record.form_version,
        subrecords: record
            .subrecords
            .iter()
            .map(|subrecord| WritableSubRecord {
                signature: subrecord.signature,
                data: subrecord.data.clone(),
            })
            .collect(),
    }
}

fn owned_to_handler_value(value: &OwnedFieldValue) -> FieldValue<'static> {
    match value {
        OwnedFieldValue::Int(value) => FieldValue::Int(*value),
        OwnedFieldValue::UInt(value) => FieldValue::UInt(*value),
        OwnedFieldValue::Float(value) => FieldValue::Float(*value),
        OwnedFieldValue::String(value) => {
            FieldValue::String(std::borrow::Cow::Owned(value.clone()))
        }
        OwnedFieldValue::FormId(value) => FieldValue::FormId {
            value: *value,
            targets: Vec::new(),
        },
        OwnedFieldValue::Bytes(value) => FieldValue::Bytes(std::borrow::Cow::Owned(value.clone())),
        OwnedFieldValue::Struct(values) => {
            FieldValue::Array(values.iter().map(owned_to_handler_value).collect())
        }
        OwnedFieldValue::Array(values) => {
            FieldValue::Array(values.iter().map(owned_to_handler_value).collect())
        }
    }
}

fn handler_to_owned_value(value: FieldValue<'static>, path: &str) -> Result<OwnedFieldValue> {
    match value {
        FieldValue::Int(value) => Ok(OwnedFieldValue::Int(value)),
        FieldValue::UInt(value) => Ok(OwnedFieldValue::UInt(value)),
        FieldValue::Float(value) => Ok(OwnedFieldValue::Float(value)),
        FieldValue::String(value) => Ok(OwnedFieldValue::String(value.into_owned())),
        FieldValue::FormId { value, .. } => Ok(OwnedFieldValue::FormId(value)),
        FieldValue::Bytes(value) => Ok(OwnedFieldValue::Bytes(value.into_owned())),
        FieldValue::Array(values) => values
            .into_iter()
            .map(|value| handler_to_owned_value(value, path))
            .collect::<Result<Vec<_>>>()
            .map(OwnedFieldValue::Array),
        _ => Err(SemanticError::Handler {
            handler: path.to_owned(),
            message: "handler returned a value unsupported by the editor".to_owned(),
        }),
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

    let mut output = Vec::new();
    collect(root, &mut output);
    output
}

fn first_signature_order(nodes: &[&SchemaNode], signature: Signature) -> Option<usize> {
    nodes.iter().position(|node| {
        matches!(
            node.kind,
            SchemaNodeKind::Subrecord {
                signature: expected,
                ..
            } if Signature::from(expected) == signature
        )
    })
}

fn find_node_by_path<'a>(node: &'a SchemaNode, path: &str) -> Option<&'a SchemaNode> {
    if node.path == path {
        return Some(node);
    }
    let children: Vec<&SchemaNode> = match &node.kind {
        SchemaNodeKind::Sequence { children } => children.iter().collect(),
        SchemaNodeKind::Choice { alternatives } => alternatives.iter().collect(),
        SchemaNodeKind::Repeat { child, .. }
        | SchemaNodeKind::Subrecord { payload: child, .. }
        | SchemaNodeKind::Array { element: child, .. }
        | SchemaNodeKind::Compressed { child, .. } => vec![child],
        SchemaNodeKind::Struct { fields } => fields.iter().collect(),
        SchemaNodeKind::Union { variants, .. } => variants.iter().collect(),
        _ => Vec::new(),
    };
    children
        .into_iter()
        .find_map(|child| find_node_by_path(child, path))
}

fn encode_primitive(
    primitive: &PrimitiveType,
    value: &OwnedFieldValue,
    localized: bool,
    path: &str,
) -> Result<Vec<u8>> {
    match (primitive, value) {
        (PrimitiveType::Integer { integer }, OwnedFieldValue::Int(value)) => {
            encode_integer(*integer, *value as u64, path)
        }
        (PrimitiveType::Integer { integer }, OwnedFieldValue::UInt(value)) => {
            encode_integer(*integer, *value, path)
        }
        (
            PrimitiveType::Float {
                width: 4,
                byte_order,
                ..
            },
            OwnedFieldValue::Float(value),
        ) => {
            let bytes: [u8; 4] = match byte_order {
                ByteOrder::LittleEndian => (*value as f32).to_le_bytes(),
                ByteOrder::BigEndian => (*value as f32).to_be_bytes(),
            };
            Ok(bytes.to_vec())
        }
        (
            PrimitiveType::Float {
                width: 8,
                byte_order,
                ..
            },
            OwnedFieldValue::Float(value),
        ) => {
            let bytes: [u8; 8] = match byte_order {
                ByteOrder::LittleEndian => value.to_le_bytes(),
                ByteOrder::BigEndian => value.to_be_bytes(),
            };
            Ok(bytes.to_vec())
        }
        (PrimitiveType::String { string }, OwnedFieldValue::UInt(value))
            if is_localized_string(string) && localized =>
        {
            let id: u32 = u32::try_from(*value)
                .map_err(|_| encode_error(path, "localized string ID exceeds u32"))?;
            Ok(id.to_le_bytes().to_vec())
        }
        (PrimitiveType::String { string }, OwnedFieldValue::String(value)) => {
            if is_localized_string(string) && localized {
                return Err(encode_error(
                    path,
                    "localized plugin string requires a string-table ID",
                ));
            }
            let bytes: Vec<u8> = match text_encoding(string) {
                "utf8" => value.as_bytes().to_vec(),
                "windows_1252" => {
                    let (bytes, _, had_errors) = encoding_rs::WINDOWS_1252.encode(value);
                    if had_errors {
                        return Err(encode_error(
                            path,
                            "string contains characters that Windows-1252 cannot represent",
                        ));
                    }
                    bytes.into_owned()
                }
                encoding => {
                    return Err(encode_error(
                        path,
                        format!("unsupported string encoding {encoding}"),
                    ));
                }
            };
            finish_string_encoding(string, bytes, path)
        }
        (PrimitiveType::Bytes { length }, OwnedFieldValue::Bytes(value)) => {
            if length.is_some_and(|length| value.len() != length as usize) {
                return Err(encode_error(path, "byte array has incorrect length"));
            }
            Ok(value.clone())
        }
        (PrimitiveType::Unused { length }, OwnedFieldValue::Bytes(value)) => {
            if value.len() != *length as usize {
                return Err(encode_error(path, "unused bytes have incorrect length"));
            }
            Ok(value.clone())
        }
        (PrimitiveType::FormId { .. }, OwnedFieldValue::FormId(value)) => {
            Ok(value.0.to_le_bytes().to_vec())
        }
        (PrimitiveType::Enumeration { integer, .. }, OwnedFieldValue::Int(value)) => {
            encode_integer(*integer, *value as u64, path)
        }
        (PrimitiveType::Flags { integer, .. }, OwnedFieldValue::UInt(value)) => {
            encode_integer(*integer, *value, path)
        }
        _ => Err(encode_error(path, "value type does not match schema type")),
    }
}

fn is_localized_string(string: &bethkit_schema::StringType) -> bool {
    string.localized || string.encoding == "localized"
}

fn text_encoding(string: &bethkit_schema::StringType) -> &str {
    if string.encoding == "localized" {
        "windows_1252"
    } else {
        &string.encoding
    }
}

fn finish_string_encoding(
    string: &bethkit_schema::StringType,
    mut body: Vec<u8>,
    path: &str,
) -> Result<Vec<u8>> {
    if string.zero_terminated {
        body.push(0);
    }
    if let Some(length) = string.fixed_length {
        if body.len() > length as usize {
            return Err(encode_error(path, "string exceeds fixed length"));
        }
        body.resize(length as usize, 0);
    }
    let mut output: Vec<u8> = if let Some(prefix) = string.length_prefix {
        let length: u64 = body.len() as u64;
        let mut output: Vec<u8> = vec![0; prefix.offset as usize];
        match prefix.width {
            1 if length <= u8::MAX as u64 => output[0] = length as u8,
            2 if length <= u16::MAX as u64 => {
                output[..2].copy_from_slice(&(length as u16).to_le_bytes());
            }
            4 if length <= u32::MAX as u64 => {
                output[..4].copy_from_slice(&(length as u32).to_le_bytes());
            }
            1 | 2 | 4 => {
                return Err(encode_error(path, "string exceeds length prefix width"));
            }
            width => {
                return Err(encode_error(
                    path,
                    format!("unsupported string length prefix width {width}"),
                ));
            }
        }
        output.extend_from_slice(&body);
        output
    } else {
        body
    };
    if let Some(terminator) = string.trailing_terminator {
        output.push(terminator);
    }
    Ok(output)
}

fn encode_integer(integer: IntegerType, value: u64, path: &str) -> Result<Vec<u8>> {
    let full: [u8; 8] = match integer.byte_order {
        ByteOrder::LittleEndian => value.to_le_bytes(),
        ByteOrder::BigEndian => value.to_be_bytes(),
    };
    match (integer.width, integer.byte_order) {
        (1, _) if value <= u8::MAX as u64 => Ok(vec![value as u8]),
        (2, ByteOrder::LittleEndian) if value <= u16::MAX as u64 => Ok(full[..2].to_vec()),
        (2, ByteOrder::BigEndian) if value <= u16::MAX as u64 => Ok(full[6..].to_vec()),
        (4, ByteOrder::LittleEndian) if value <= u32::MAX as u64 => Ok(full[..4].to_vec()),
        (4, ByteOrder::BigEndian) if value <= u32::MAX as u64 => Ok(full[4..].to_vec()),
        (8, _) => Ok(full.to_vec()),
        _ => Err(encode_error(path, "integer does not fit schema width")),
    }
}

fn encode_error(path: &str, message: impl Into<String>) -> SemanticError {
    SemanticError::Encode {
        path: path.to_owned(),
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use bethkit_schema::StringType;

    use super::*;

    fn windows_1252_string(zero_terminated: bool) -> PrimitiveType {
        PrimitiveType::String {
            string: StringType {
                encoding: "windows_1252".to_owned(),
                localized: false,
                zero_terminated,
                fixed_length: None,
                length_prefix: None,
                trailing_terminator: None,
            },
        }
    }

    #[test]
    fn windows_1252_strings_encode_exact_bytes() {
        let bytes = encode_primitive(
            &windows_1252_string(true),
            &OwnedFieldValue::String("Grüße".to_owned()),
            false,
            "TEST",
        )
        .expect("Windows-1252 string should encode");

        assert_eq!(bytes, b"Gr\xfc\xdfe\0");
    }

    #[test]
    fn windows_1252_strings_reject_unrepresentable_characters() {
        let error = encode_primitive(
            &windows_1252_string(false),
            &OwnedFieldValue::String("Dragon 🐉".to_owned()),
            false,
            "TEST",
        )
        .expect_err("unrepresentable character should fail");

        assert!(error.to_string().contains("cannot represent"));
    }

    #[test]
    fn length_prefixed_strings_include_padding_and_structural_terminator() {
        let primitive = PrimitiveType::String {
            string: StringType {
                encoding: "utf8".to_owned(),
                localized: false,
                zero_terminated: false,
                fixed_length: None,
                length_prefix: Some(bethkit_schema::StringLengthPrefix {
                    width: 1,
                    offset: 2,
                }),
                trailing_terminator: Some(b'|'),
            },
        };

        let bytes = encode_primitive(
            &primitive,
            &OwnedFieldValue::String("abc".to_owned()),
            false,
            "TEST",
        )
        .expect("length-prefixed string should encode");

        assert_eq!(bytes, b"\x03\0abc|");
    }

    #[test]
    fn localized_strings_encode_as_table_ids() {
        let primitive = PrimitiveType::String {
            string: StringType {
                encoding: "windows_1252".to_owned(),
                localized: true,
                zero_terminated: true,
                fixed_length: None,
                length_prefix: None,
                trailing_terminator: Some(b'|'),
            },
        };

        let bytes = encode_primitive(
            &primitive,
            &OwnedFieldValue::UInt(0x1234_5678),
            true,
            "TEST",
        )
        .expect("localized string ID should encode");

        assert_eq!(bytes, 0x1234_5678_u32.to_le_bytes());
    }
}
