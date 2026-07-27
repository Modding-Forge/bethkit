// SPDX-License-Identifier: Apache-2.0
//!
//! Lossless schema-guided record editing.

use std::borrow::Cow;
use std::collections::BTreeMap;

use bethkit_core::{Record, Signature, WritableRecord, WritableSubRecord};
use bethkit_schema::{
    ArrayCount, ByteOrder, CallbackImplementation, EvalContext, EvalValue, IntegerType,
    PrimitiveType, SchemaNode, SchemaNodeKind, UnionSelector,
};

use crate::value::{float_to_raw, handler_to_owned_value};
use crate::{
    grammar::interpret_writable, FieldValue, HandlerMutation, HandlerOutput, HandlerPhase,
    HandlerRecordContext, OwnedFieldValue, Result, SemanticContext, SemanticError,
    SemanticHandlerRegistry,
};

/// Lossless editor for one record.
pub struct RecordEditor {
    registry: bethkit_schema::SchemaRegistry,
    decoders: crate::DecoderRegistry,
    handlers: SemanticHandlerRegistry,
    record: WritableRecord,
    localized: bool,
    decoded_values: BTreeMap<(String, usize), FieldValue<'static>>,
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
        let decoded_values = context
            .view(record, plugin_localized)?
            .fields()?
            .into_iter()
            .map(|field| {
                (
                    (field.path, field.occurrence),
                    field.value.to_handler_value(),
                )
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
            decoded_values,
        })
    }

    /// Replaces an existing top-level schema-path occurrence with a typed value.
    ///
    /// # Errors
    ///
    /// Returns [`SemanticError`] when the path, occurrence, or value is
    /// invalid for the loaded schema.
    pub fn set(&mut self, path: &str, occurrence: usize, value: &OwnedFieldValue) -> Result<()> {
        let node: &SchemaNode = self.find_node(path)?;
        let SchemaNodeKind::Subrecord { payload, .. } = &node.kind else {
            return Err(SemanticError::Encode {
                path: path.to_owned(),
                message: "only top-level subrecords can be replaced".to_owned(),
            });
        };
        let index = self.assigned_subrecord_index(&self.record, path, occurrence)?;
        let normalized = self.normalize_value(&payload.path, value)?;
        let old_value = self.decoded_values.get(&(path.to_owned(), occurrence));
        let (normalized, mutations) = self.apply_after_set_tree(payload, &normalized, old_value)?;
        let encoded: Vec<u8> = self.encode_node(payload, &normalized)?;
        let decoded = self.owned_to_handler_value(payload, &normalized)?;
        let mut candidate = clone_record(&self.record);
        candidate.subrecords[index].data = encoded;
        self.apply_mutations(&mut candidate, mutations)?;
        self.record = candidate;
        self.decoded_values
            .insert((path.to_owned(), occurrence), decoded);
        Ok(())
    }

    /// Inserts a new top-level field after existing occurrences of its path.
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
        let insertion_index = self.schema_insertion_index(&self.record, node)?;
        let occurrence = self.assigned_occurrence_count(&self.record, path)?;
        let normalized = self.normalize_value(&payload.path, value)?;
        let (normalized, mutations) = self.apply_after_set_tree(payload, &normalized, None)?;
        let encoded: Vec<u8> = self.encode_node(payload, &normalized)?;
        let decoded = self.owned_to_handler_value(payload, &normalized)?;
        let mut candidate = clone_record(&self.record);
        candidate.subrecords.insert(
            insertion_index,
            WritableSubRecord {
                signature: target_signature,
                data: encoded,
            },
        );
        self.apply_mutations(&mut candidate, mutations)?;
        self.record = candidate;
        self.decoded_values
            .insert((path.to_owned(), occurrence), decoded);
        Ok(())
    }

    /// Removes a top-level schema-path occurrence.
    ///
    /// # Errors
    ///
    /// Returns [`SemanticError`] when the path or occurrence is absent.
    pub fn remove(&mut self, path: &str, occurrence: usize) -> Result<()> {
        let node: &SchemaNode = self.find_node(path)?;
        let SchemaNodeKind::Subrecord { .. } = &node.kind else {
            return Err(SemanticError::Encode {
                path: path.to_owned(),
                message: "only top-level subrecords can be removed".to_owned(),
            });
        };
        let index = self.assigned_subrecord_index(&self.record, path, occurrence)?;
        let mut candidate = clone_record(&self.record);
        candidate.subrecords.remove(index);
        self.record = candidate;
        self.remove_decoded_occurrence(path, occurrence);
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

    fn grammar_for(&self, record: &WritableRecord) -> Result<crate::grammar::GrammarMatch<'_>> {
        let schema = self
            .registry
            .get(record.signature)
            .ok_or_else(|| SemanticError::MissingRecordSchema(record.signature.to_string()))?;
        interpret_writable(
            &schema.root,
            record.signature,
            record.form_version,
            &record.subrecords,
        )
    }

    fn assigned_subrecord_index(
        &self,
        record: &WritableRecord,
        path: &str,
        occurrence: usize,
    ) -> Result<usize> {
        self.grammar_for(record)?
            .assignments
            .iter()
            .enumerate()
            .filter(|(_, assignment)| assignment.is_some_and(|candidate| candidate.path == path))
            .nth(occurrence)
            .map(|(index, _)| index)
            .ok_or_else(|| SemanticError::MissingOccurrence {
                path: path.to_owned(),
                occurrence,
            })
    }

    fn assigned_occurrence_count(&self, record: &WritableRecord, path: &str) -> Result<usize> {
        Ok(self
            .grammar_for(record)?
            .assignments
            .iter()
            .filter(|assignment| assignment.is_some_and(|candidate| candidate.path == path))
            .count())
    }

    fn schema_insertion_index(
        &self,
        record: &WritableRecord,
        target: &SchemaNode,
    ) -> Result<usize> {
        let schema = self
            .registry
            .get(record.signature)
            .ok_or_else(|| SemanticError::MissingRecordSchema(record.signature.to_string()))?;
        let ordered = top_level_subrecords(&schema.root);
        let target_order = ordered
            .iter()
            .position(|candidate| candidate.id == target.id)
            .ok_or_else(|| SemanticError::MissingPath(target.path.clone()))?;
        let grammar = self.grammar_for(record)?;
        let mut after_existing = None;
        for (index, assignment) in grammar.assignments.iter().enumerate() {
            let Some(assigned) = assignment else {
                continue;
            };
            if assigned.path == target.path {
                after_existing = Some(index + 1);
                continue;
            }
            let Some(order) = ordered
                .iter()
                .position(|candidate| candidate.id == assigned.id)
            else {
                continue;
            };
            if order > target_order {
                return Ok(after_existing.unwrap_or(index));
            }
        }
        Ok(after_existing.unwrap_or(record.subrecords.len()))
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
                    ArrayCount::Prefixed {
                        integer,
                        terminator,
                    } => {
                        if integer.signed {
                            return Err(encode_error(
                                &node.path,
                                "array count prefix must be unsigned",
                            ));
                        }
                        let count: u64 = u64::try_from(values.len())
                            .map_err(|_| encode_error(&node.path, "array count exceeds u64"))?;
                        output.extend(encode_integer(*integer, count, &node.path)?);
                        output.extend(terminator);
                    }
                    ArrayCount::PackedPrefixed { square, terminator } => {
                        let count = array_prefix_count(values.len(), *square, &node.path)?;
                        output.extend(encode_packed_unsigned(count, &node.path)?);
                        output.extend(terminator);
                    }
                    ArrayCount::SquaredPrefixed {
                        integer,
                        terminator,
                    } => {
                        if integer.signed {
                            return Err(encode_error(
                                &node.path,
                                "array count prefix must be unsigned",
                            ));
                        }
                        let count = array_prefix_count(values.len(), true, &node.path)?;
                        output.extend(encode_integer(*integer, count, &node.path)?);
                        output.extend(terminator);
                    }
                    ArrayCount::Fixed { .. }
                    | ArrayCount::Expression { .. }
                    | ArrayCount::Remainder => {}
                }
                for value in values {
                    output.extend(self.encode_node(element, value)?);
                }
                Ok(output)
            }
            SchemaNodeKind::Union { selector, variants } => {
                let variant = self.select_union_variant(node, selector, variants, value)?;
                self.encode_node(variant, value)
            }
            SchemaNodeKind::Custom { decoder, .. } => self
                .decoders
                .get(decoder)
                .ok_or_else(|| SemanticError::MissingDecoder(decoder.clone()))?
                .encode(value),
            SchemaNodeKind::Terminated { terminator, child } => {
                let mut output = self.encode_node(child, value)?;
                output.push(*terminator);
                Ok(output)
            }
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
            let handler_value = self.owned_to_handler_value(node, &normalized)?;
            if matches!(&handler_value, FieldValue::Float(value) if !value.is_finite()) {
                continue;
            }
            normalized = match self.handlers.invoke(
                binding,
                self.handler_record(),
                HandlerPhase::DecodeNormalize,
                Some(&handler_value),
                None,
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

    fn apply_after_set_tree(
        &self,
        node: &SchemaNode,
        value: &OwnedFieldValue,
        old_value: Option<&FieldValue<'static>>,
    ) -> Result<(OwnedFieldValue, Vec<HandlerMutation>)> {
        let (mut updated, mut mutations) = match (&node.kind, value) {
            (SchemaNodeKind::Struct { fields }, OwnedFieldValue::Struct(values)) => {
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
                let mut updated = Vec::with_capacity(values.len());
                let mut mutations = Vec::new();
                let old_fields = match old_value {
                    Some(FieldValue::Struct(values)) => Some(values.as_slice()),
                    _ => None,
                };
                for (index, (field, value)) in fields.iter().zip(values).enumerate() {
                    let old_field = old_fields
                        .and_then(|values| values.get(index))
                        .map(|value| &value.value);
                    let (value, child_mutations) =
                        self.apply_after_set_tree(field, value, old_field)?;
                    updated.push(value);
                    mutations.extend(child_mutations);
                }
                (OwnedFieldValue::Struct(updated), mutations)
            }
            (SchemaNodeKind::Array { element, .. }, OwnedFieldValue::Array(values)) => {
                let mut updated = Vec::with_capacity(values.len());
                let mut mutations = Vec::new();
                let old_values = match old_value {
                    Some(FieldValue::Array(values)) => Some(values.as_slice()),
                    _ => None,
                };
                for (index, value) in values.iter().enumerate() {
                    let old_element = old_values.and_then(|values| values.get(index));
                    let (value, child_mutations) =
                        self.apply_after_set_tree(element, value, old_element)?;
                    updated.push(value);
                    mutations.extend(child_mutations);
                }
                (OwnedFieldValue::Array(updated), mutations)
            }
            (SchemaNodeKind::Union { selector, variants }, _) => {
                let variant = self.select_union_variant(node, selector, variants, value)?;
                self.apply_after_set_tree(variant, value, old_value)?
            }
            (
                SchemaNodeKind::Subrecord { payload, .. }
                | SchemaNodeKind::Compressed { child: payload, .. }
                | SchemaNodeKind::Terminated { child: payload, .. },
                _,
            ) => self.apply_after_set_tree(payload, value, old_value)?,
            _ => (value.clone(), Vec::new()),
        };
        self.apply_local_set_mutations(node, &mut updated, &mut mutations)?;
        let handler_value = self.owned_to_handler_value(node, &updated)?;
        if old_value.is_some_and(|old| handler_values_equal(&handler_value, old)) {
            return Ok((updated, mutations));
        }
        for binding in self
            .registry
            .package()
            .callback_bindings()
            .iter()
            .filter(|binding| binding.path == node.path && binding.callback_id == "def.after_set")
        {
            if !matches!(
                binding.implementation,
                CallbackImplementation::BuiltIn { .. }
                    | CallbackImplementation::CustomHandler { .. }
            ) {
                continue;
            }
            let handler_value = self.owned_to_handler_value(node, &updated)?;
            match self.handlers.invoke(
                binding,
                self.handler_record(),
                HandlerPhase::AfterSet,
                Some(&handler_value),
                old_value,
            )? {
                HandlerOutput::None => {}
                HandlerOutput::Value(value) => {
                    updated = handler_to_owned_value(value, &node.path)?;
                }
                HandlerOutput::Mutations(handler_mutations) => mutations.extend(handler_mutations),
                _ => {
                    return Err(SemanticError::Handler {
                        handler: binding.callback_id.clone(),
                        message: "after-set handler returned an invalid result".to_owned(),
                    });
                }
            }
        }
        self.apply_local_set_mutations(node, &mut updated, &mut mutations)?;
        Ok((updated, mutations))
    }

    fn apply_local_set_mutations(
        &self,
        node: &SchemaNode,
        value: &mut OwnedFieldValue,
        mutations: &mut Vec<HandlerMutation>,
    ) -> Result<()> {
        let mut remaining = Vec::with_capacity(mutations.len());
        for mutation in mutations.drain(..) {
            match mutation {
                HandlerMutation::Set {
                    path,
                    occurrence,
                    value: replacement,
                } => {
                    if path != node.path
                        && !path
                            .strip_prefix(&node.path)
                            .is_some_and(|suffix| suffix.starts_with('/'))
                    {
                        remaining.push(HandlerMutation::Set {
                            path,
                            occurrence,
                            value: replacement,
                        });
                        continue;
                    }
                    let mut occurrence = occurrence;
                    let mut replacement = Some(replacement);
                    if !self.set_nested_value(
                        node,
                        value,
                        &path,
                        &mut occurrence,
                        &mut replacement,
                    )? {
                        return Err(SemanticError::MissingOccurrence { path, occurrence });
                    }
                }
                mutation => remaining.push(mutation),
            }
        }
        *mutations = remaining;
        Ok(())
    }

    fn set_nested_value(
        &self,
        node: &SchemaNode,
        value: &mut OwnedFieldValue,
        target_path: &str,
        occurrence: &mut usize,
        replacement: &mut Option<OwnedFieldValue>,
    ) -> Result<bool> {
        if node.path == target_path {
            if *occurrence == 0 {
                *value = replacement.take().ok_or_else(|| SemanticError::Handler {
                    handler: "def.after_set".to_owned(),
                    message: "local mutation has no replacement value".to_owned(),
                })?;
                return Ok(true);
            }
            *occurrence = occurrence.saturating_sub(1);
            return Ok(false);
        }
        match (&node.kind, &mut *value) {
            (SchemaNodeKind::Struct { fields }, OwnedFieldValue::Struct(values)) => {
                for (field, value) in fields.iter().zip(values) {
                    if self.set_nested_value(field, value, target_path, occurrence, replacement)? {
                        return Ok(true);
                    }
                }
            }
            (SchemaNodeKind::Array { element, .. }, OwnedFieldValue::Array(values)) => {
                for value in values {
                    if self.set_nested_value(
                        element,
                        value,
                        target_path,
                        occurrence,
                        replacement,
                    )? {
                        return Ok(true);
                    }
                }
            }
            (SchemaNodeKind::Union { selector, variants }, current) => {
                let variant = self.select_union_variant(node, selector, variants, current)?;
                if self.set_nested_value(variant, current, target_path, occurrence, replacement)? {
                    return Ok(true);
                }
            }
            (
                SchemaNodeKind::Subrecord { payload, .. }
                | SchemaNodeKind::Compressed { child: payload, .. }
                | SchemaNodeKind::Terminated { child: payload, .. },
                current,
            ) => {
                if self.set_nested_value(payload, current, target_path, occurrence, replacement)? {
                    return Ok(true);
                }
            }
            _ => {}
        }
        Ok(false)
    }

    fn select_union_variant<'a>(
        &self,
        node: &SchemaNode,
        selector: &UnionSelector,
        variants: &'a [SchemaNode],
        value: &OwnedFieldValue,
    ) -> Result<&'a SchemaNode> {
        for (index, variant) in variants.iter().enumerate() {
            let Ok(encoded) = self.encode_node(variant, value) else {
                continue;
            };
            let selected = match selector {
                UnionSelector::Expression(expression) => {
                    let context = EvalContext {
                        payload: &encoded,
                        form_version: self.record.form_version,
                        record_signature: self.record.signature.into(),
                    };
                    match expression.evaluate(&context, 1024) {
                        Ok(EvalValue::Int(selected)) => selected,
                        _ => continue,
                    }
                }
                UnionSelector::Callback { callback_id } => {
                    let Some(binding) =
                        self.registry
                            .package()
                            .callback_bindings()
                            .iter()
                            .find(|binding| {
                                binding.path == node.path
                                    && binding.callback_id.as_str() == callback_id
                            })
                    else {
                        return Err(SemanticError::Handler {
                            handler: callback_id.clone(),
                            message: format!("union node {} has no callback binding", node.path),
                        });
                    };
                    let raw_value = FieldValue::Bytes(Cow::Owned(encoded));
                    match self.handlers.invoke(
                        binding,
                        self.handler_record(),
                        HandlerPhase::UnionSelection,
                        Some(&raw_value),
                        None,
                    )? {
                        HandlerOutput::Integer(selected) => selected,
                        _ => {
                            return Err(SemanticError::Handler {
                                handler: callback_id.clone(),
                                message: "union selector returned a non-integer result".to_owned(),
                            });
                        }
                    }
                }
            };
            if selected == index as i64 {
                return Ok(variant);
            }
        }
        Err(encode_error(
            &node.path,
            "value does not match the selected union variant",
        ))
    }

    fn owned_to_handler_value(
        &self,
        node: &SchemaNode,
        value: &OwnedFieldValue,
    ) -> Result<FieldValue<'static>> {
        match (&node.kind, value) {
            (SchemaNodeKind::Struct { fields }, OwnedFieldValue::Struct(values)) => {
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
                fields
                    .iter()
                    .zip(values)
                    .map(|(field, value)| {
                        Ok(crate::NamedValue {
                            node_id: field.id,
                            path: field.path.clone(),
                            name: field.name.clone(),
                            span: crate::ByteSpan { start: 0, end: 0 },
                            value: self.owned_to_handler_value(field, value)?,
                        })
                    })
                    .collect::<Result<Vec<_>>>()
                    .map(FieldValue::Struct)
            }
            (SchemaNodeKind::Array { element, .. }, OwnedFieldValue::Array(values)) => values
                .iter()
                .map(|value| self.owned_to_handler_value(element, value))
                .collect::<Result<Vec<_>>>()
                .map(FieldValue::Array),
            (SchemaNodeKind::Union { selector, variants }, _) => {
                let variant = self.select_union_variant(node, selector, variants, value)?;
                self.owned_to_handler_value(variant, value)
            }
            (
                SchemaNodeKind::Subrecord { payload, .. }
                | SchemaNodeKind::Compressed { child: payload, .. }
                | SchemaNodeKind::Terminated { child: payload, .. },
                _,
            ) => self.owned_to_handler_value(payload, value),
            _ => Ok(owned_leaf_to_handler_value(value)),
        }
    }

    fn handler_record(&self) -> HandlerRecordContext {
        HandlerRecordContext::new(
            self.record.signature,
            self.record.form_id,
            self.record.form_version,
            self.registry.package().manifest().game,
        )
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
                    let index = self.assigned_subrecord_index(record, &path, occurrence)?;
                    if record.subrecords[index].signature != signature {
                        return Err(SemanticError::Encode {
                            path,
                            message: "assigned subrecord signature does not match schema"
                                .to_owned(),
                        });
                    }
                    record.subrecords[index].data = encoded;
                }
                HandlerMutation::Insert { path, value } => {
                    let (signature, encoded) = self.encode_path(&path, &value)?;
                    let node = self.find_node(&path)?;
                    let index = self.schema_insertion_index(record, node)?;
                    record.subrecords.insert(
                        index,
                        WritableSubRecord {
                            signature,
                            data: encoded,
                        },
                    );
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
                    let index = self.assigned_subrecord_index(record, &path, occurrence)?;
                    if record.subrecords[index].signature != signature {
                        return Err(SemanticError::Encode {
                            path,
                            message: "assigned subrecord signature does not match schema"
                                .to_owned(),
                        });
                    }
                    record.subrecords.remove(index);
                }
                HandlerMutation::SynchronizeCount {
                    path,
                    occurrence,
                    value,
                    remove_when_zero,
                } => {
                    let existing = self
                        .assigned_subrecord_index(record, &path, occurrence)
                        .map(Some)
                        .or_else(|error| match error {
                            SemanticError::MissingOccurrence { .. } => Ok(None),
                            _ => Err(error),
                        })?;
                    if value == 0 && remove_when_zero {
                        if let Some(index) = existing {
                            record.subrecords.remove(index);
                        }
                        continue;
                    }
                    let (signature, encoded) =
                        self.encode_path(&path, &OwnedFieldValue::UInt(value))?;
                    if let Some(index) = existing {
                        if record.subrecords[index].signature != signature {
                            return Err(SemanticError::Encode {
                                path,
                                message: "assigned counter signature does not match schema"
                                    .to_owned(),
                            });
                        }
                        record.subrecords[index].data = encoded;
                    } else {
                        let node = self.find_node(&path)?;
                        let index = self.schema_insertion_index(record, node)?;
                        record.subrecords.insert(
                            index,
                            WritableSubRecord {
                                signature,
                                data: encoded,
                            },
                        );
                    }
                }
            }
        }
        Ok(())
    }

    fn remove_decoded_occurrence(&mut self, path: &str, occurrence: usize) {
        self.decoded_values.remove(&(path.to_owned(), occurrence));
        let shifted = self
            .decoded_values
            .keys()
            .filter(|(candidate, index)| candidate == path && *index > occurrence)
            .cloned()
            .collect::<Vec<_>>();
        for key in shifted {
            if let Some(value) = self.decoded_values.remove(&key) {
                self.decoded_values
                    .insert((key.0, key.1.saturating_sub(1)), value);
            }
        }
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

fn handler_values_equal(left: &FieldValue<'_>, right: &FieldValue<'_>) -> bool {
    if let (Some(left), Some(right)) = (integer_handler_value(left), integer_handler_value(right)) {
        return left == right;
    }
    match (left, right) {
        (FieldValue::Float(left), FieldValue::Float(right)) => left.to_bits() == right.to_bits(),
        (FieldValue::String(left), FieldValue::String(right)) => left == right,
        (FieldValue::FormId { value: left, .. }, FieldValue::FormId { value: right, .. }) => {
            left == right
        }
        (FieldValue::Bytes(left), FieldValue::Bytes(right)) => left == right,
        (FieldValue::Struct(left), FieldValue::Struct(right)) => {
            left.len() == right.len()
                && left.iter().zip(right).all(|(left, right)| {
                    left.node_id == right.node_id && handler_values_equal(&left.value, &right.value)
                })
        }
        (FieldValue::Array(left), FieldValue::Array(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right)
                    .all(|(left, right)| handler_values_equal(left, right))
        }
        (FieldValue::Absent, FieldValue::Absent) => true,
        _ => false,
    }
}

fn integer_handler_value(value: &FieldValue<'_>) -> Option<i128> {
    match value {
        FieldValue::Int(value) => Some(i128::from(*value)),
        FieldValue::UInt(value) | FieldValue::Flags { value, .. } => Some(i128::from(*value)),
        FieldValue::Enumeration { value, .. } => Some(i128::from(*value)),
        _ => None,
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

fn owned_leaf_to_handler_value(value: &OwnedFieldValue) -> FieldValue<'static> {
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
            FieldValue::Array(values.iter().map(owned_leaf_to_handler_value).collect())
        }
        OwnedFieldValue::Array(values) => {
            FieldValue::Array(values.iter().map(owned_leaf_to_handler_value).collect())
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

    let mut output = Vec::new();
    collect(root, &mut output);
    output
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
        | SchemaNodeKind::Compressed { child, .. }
        | SchemaNodeKind::Terminated { child, .. } => vec![child],
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
        (PrimitiveType::PackedUnsigned, OwnedFieldValue::UInt(value)) => {
            encode_packed_unsigned(*value, path)
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

fn encode_packed_unsigned(value: u64, path: &str) -> Result<Vec<u8>> {
    let (width, selector) = if value <= 0x3f {
        (1, 0_u64)
    } else if value <= 0x3fff {
        (2, 1_u64)
    } else if value <= 0x3fff_ffff {
        (4, 2_u64)
    } else {
        return Err(encode_error(path, "packed integer exceeds 30 bits"));
    };
    let raw = value
        .checked_shl(2)
        .and_then(|value| value.checked_add(selector))
        .ok_or_else(|| encode_error(path, "packed integer overflowed"))?;
    Ok(raw.to_le_bytes()[..width].to_vec())
}

fn array_prefix_count(length: usize, square: bool, path: &str) -> Result<u64> {
    let length =
        u64::try_from(length).map_err(|_| encode_error(path, "array count exceeds u64"))?;
    if !square {
        return Ok(length);
    }
    let mut low = 0_u64;
    let mut high = length.min(u64::from(u32::MAX)).saturating_add(1);
    while low + 1 < high {
        let middle = low + (high - low) / 2;
        if middle <= length / middle.max(1) {
            low = middle;
        } else {
            high = middle;
        }
    }
    if low.checked_mul(low) != Some(length) {
        return Err(encode_error(
            path,
            "matrix array length is not a perfect square",
        ));
    }
    Ok(low)
}

fn encode_error(path: &str, message: impl Into<String>) -> SemanticError {
    SemanticError::Encode {
        path: path.to_owned(),
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bethkit_schema::{
        BuiltInOperation, CallbackBinding, CallbackImplementation, ConflictPriority,
        HandlerRequirement, SchemaGame, SchemaManifest, SchemaNodeId, SchemaPackage, SchemaRecord,
        SchemaSignature, StringType, ValidationStatus, PACKAGE_FORMAT_VERSION,
    };

    use super::*;

    struct SelectSecondUnionVariant;

    impl crate::SemanticHandler for SelectSecondUnionVariant {
        fn id(&self) -> &'static str {
            "test.union_selector"
        }

        fn version(&self) -> u32 {
            1
        }

        fn invoke(&self, invocation: crate::HandlerInvocation<'_>) -> Result<HandlerOutput> {
            if invocation.phase == HandlerPhase::UnionSelection {
                Ok(HandlerOutput::Integer(1))
            } else {
                Ok(HandlerOutput::None)
            }
        }
    }

    fn windows_1252_string(zero_terminated: bool) -> PrimitiveType {
        PrimitiveType::String {
            string: StringType {
                encoding: "windows_1252".to_owned(),
                localized: false,
                zero_terminated,
                fixed_length: None,
                length_prefix: None,
                trailing_terminator: None,
                allowed_values: Vec::new(),
            },
        }
    }

    /// Emits canonical xEdit packed integer widths and rejects overflow.
    #[test]
    fn packed_unsigned_encoding_uses_minimum_width() -> Result<()> {
        assert_eq!(encode_packed_unsigned(63, "TEST")?, vec![0xfc]);
        assert_eq!(encode_packed_unsigned(64, "TEST")?, vec![0x01, 0x01]);
        assert_eq!(
            encode_packed_unsigned(16_384, "TEST")?,
            vec![0x02, 0x00, 0x01, 0x00]
        );
        assert!(encode_packed_unsigned(0x4000_0000, "TEST").is_err());
        assert_eq!(array_prefix_count(16, true, "TEST")?, 4);
        assert!(array_prefix_count(15, true, "TEST").is_err());
        Ok(())
    }

    /// Appends the structural terminator after encoding the wrapped value.
    #[test]
    fn terminated_node_encoding_appends_terminator() -> Result<()> {
        let editor = editor_with_reused_signature()?;
        let node = SchemaNode {
            id: SchemaNodeId(10),
            path: "TEST/value".to_owned(),
            name: "Value".to_owned(),
            required: true,
            conflict_priority: ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Terminated {
                terminator: 0xff,
                child: Box::new(SchemaNode {
                    id: SchemaNodeId(11),
                    path: "TEST/value/body".to_owned(),
                    name: "Body".to_owned(),
                    required: true,
                    conflict_priority: ConflictPriority::Normal,
                    condition: None,
                    kind: SchemaNodeKind::Primitive {
                        primitive: PrimitiveType::Integer {
                            integer: IntegerType {
                                width: 1,
                                signed: false,
                                byte_order: ByteOrder::LittleEndian,
                            },
                        },
                    },
                }),
            },
        };

        assert_eq!(
            editor.encode_node(&node, &OwnedFieldValue::UInt(7))?,
            vec![7, 0xff]
        );
        Ok(())
    }

    /// Writes a separator after an xEdit array count prefix.
    #[test]
    fn array_count_encoding_appends_prefix_terminator() -> Result<()> {
        let editor = editor_with_reused_signature()?;
        let node = SchemaNode {
            id: SchemaNodeId(12),
            path: "TEST/items".to_owned(),
            name: "Items".to_owned(),
            required: true,
            conflict_priority: ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Array {
                element: Box::new(SchemaNode {
                    id: SchemaNodeId(13),
                    path: "TEST/items/element".to_owned(),
                    name: "Element".to_owned(),
                    required: true,
                    conflict_priority: ConflictPriority::Normal,
                    condition: None,
                    kind: SchemaNodeKind::Primitive {
                        primitive: PrimitiveType::Integer {
                            integer: IntegerType {
                                width: 1,
                                signed: false,
                                byte_order: ByteOrder::LittleEndian,
                            },
                        },
                    },
                }),
                count: ArrayCount::Prefixed {
                    integer: IntegerType {
                        width: 1,
                        signed: false,
                        byte_order: ByteOrder::LittleEndian,
                    },
                    terminator: Some(0x7c),
                },
            },
        };

        assert_eq!(
            editor.encode_node(
                &node,
                &OwnedFieldValue::Array(vec![OwnedFieldValue::UInt(1), OwnedFieldValue::UInt(2),]),
            )?,
            vec![2, 0x7c, 1, 2]
        );
        Ok(())
    }

    /// Selects an encoded union variant through its classified semantic callback.
    #[test]
    fn callback_union_encoding_dispatches_handler() -> Result<()> {
        let integer = |id, width| SchemaNode {
            id: SchemaNodeId(id),
            path: format!("TEST/value/variants/{id}"),
            name: format!("Variant {id}"),
            required: true,
            conflict_priority: ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Primitive {
                primitive: PrimitiveType::Integer {
                    integer: IntegerType {
                        width,
                        signed: false,
                        byte_order: ByteOrder::LittleEndian,
                    },
                },
            },
        };
        let union = SchemaNode {
            id: SchemaNodeId(20),
            path: "TEST/value".to_owned(),
            name: "Value".to_owned(),
            required: true,
            conflict_priority: ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Union {
                selector: UnionSelector::Callback {
                    callback_id: "union.select".to_owned(),
                },
                variants: vec![integer(21, 1), integer(22, 2)],
            },
        };
        let mut manifest = test_manifest();
        manifest.callbacks_total = 1;
        manifest.callbacks_classified = 1;
        manifest.required_handlers = vec![HandlerRequirement {
            id: "test.union_selector".to_owned(),
            minimum_version: 1,
        }];
        let package = SchemaPackage::new_with_callbacks(
            manifest,
            vec![SchemaRecord {
                signature: SchemaSignature(*b"TEST"),
                name: "Test".to_owned(),
                root: union.clone(),
            }],
            vec![CallbackBinding {
                path: union.path.clone(),
                callback_id: "union.select".to_owned(),
                callback_slot: None,
                implementation_fingerprint: "00".repeat(32),
                implementation: CallbackImplementation::BuiltIn {
                    operation: BuiltInOperation {
                        id: "test.union_selector".to_owned(),
                        minimum_version: 1,
                        configuration: serde_json::Value::Null,
                    },
                },
            }],
        )?;
        let mut handlers = SemanticHandlerRegistry::new();
        handlers.register(Arc::new(SelectSecondUnionVariant));
        let editor = RecordEditor {
            registry: bethkit_schema::SchemaRegistry::new(Arc::new(package)),
            decoders: crate::DecoderRegistry::builtin(),
            handlers,
            record: WritableRecord {
                signature: Signature(*b"TEST"),
                flags: bethkit_core::RecordFlags::empty(),
                form_id: bethkit_core::FormId::NULL,
                form_version: 44,
                subrecords: Vec::new(),
            },
            localized: false,
            decoded_values: BTreeMap::new(),
        };

        assert_eq!(
            editor.encode_node(&union, &OwnedFieldValue::UInt(7))?,
            vec![7, 0]
        );
        Ok(())
    }

    fn test_manifest() -> SchemaManifest {
        SchemaManifest {
            format_version: PACKAGE_FORMAT_VERSION,
            game: SchemaGame::SkyrimSe,
            package_version: "test".to_owned(),
            source_repository: "TES5Edit/TES5Edit".to_owned(),
            source_tag: "test".to_owned(),
            source_commit: "00".repeat(20),
            source_archive_sha256: "00".repeat(32),
            exporter_version: "test".to_owned(),
            exporter_binary_sha256: "00".repeat(32),
            exporter_map_sha256: "00".repeat(32),
            exporter_patch_sha256: "00".repeat(32),
            exporter_build_sha256: "00".repeat(32),
            conversion_rules_sha256: "00".repeat(32),
            minimum_bethkit_version: "0.4.0".to_owned(),
            minimum_abi_version: 2,
            validation_status: ValidationStatus::Candidate,
            corpus_sha256: "00".repeat(32),
            validated_records: 0,
            byte_coverage: 0.0,
            callbacks_total: 0,
            callbacks_classified: 0,
            required_decoders: Vec::new(),
            required_handlers: Vec::new(),
        }
    }

    fn editor_with_reused_signature() -> Result<RecordEditor> {
        fn subrecord(id: u32, path: &str) -> SchemaNode {
            SchemaNode {
                id: SchemaNodeId(id),
                path: path.to_owned(),
                name: path.to_owned(),
                required: true,
                conflict_priority: ConflictPriority::Normal,
                condition: None,
                kind: SchemaNodeKind::Subrecord {
                    signature: SchemaSignature(*b"AAAA"),
                    payload: Box::new(SchemaNode {
                        id: SchemaNodeId(id + 100),
                        path: format!("{path}/payload"),
                        name: "Payload".to_owned(),
                        required: true,
                        conflict_priority: ConflictPriority::Normal,
                        condition: None,
                        kind: SchemaNodeKind::Primitive {
                            primitive: PrimitiveType::Bytes { length: None },
                        },
                    }),
                },
            }
        }

        let package = SchemaPackage::new(
            test_manifest(),
            vec![SchemaRecord {
                signature: SchemaSignature(*b"TEST"),
                name: "Test".to_owned(),
                root: SchemaNode {
                    id: SchemaNodeId(0),
                    path: "TEST".to_owned(),
                    name: "Test".to_owned(),
                    required: true,
                    conflict_priority: ConflictPriority::Normal,
                    condition: None,
                    kind: SchemaNodeKind::Sequence {
                        children: vec![
                            subrecord(1, "TEST/first"),
                            subrecord(2, "TEST/second"),
                            SchemaNode {
                                id: SchemaNodeId(3),
                                path: "TEST/2:Value Count".to_owned(),
                                name: "Value Count".to_owned(),
                                required: false,
                                conflict_priority: ConflictPriority::Normal,
                                condition: None,
                                kind: SchemaNodeKind::Subrecord {
                                    signature: SchemaSignature(*b"VCNT"),
                                    payload: Box::new(SchemaNode {
                                        id: SchemaNodeId(103),
                                        path: "TEST/2:Value Count/payload".to_owned(),
                                        name: "Count".to_owned(),
                                        required: true,
                                        conflict_priority: ConflictPriority::Normal,
                                        condition: None,
                                        kind: SchemaNodeKind::Primitive {
                                            primitive: PrimitiveType::Integer {
                                                integer: IntegerType {
                                                    width: 4,
                                                    signed: false,
                                                    byte_order: ByteOrder::LittleEndian,
                                                },
                                            },
                                        },
                                    }),
                                },
                            },
                            SchemaNode {
                                id: SchemaNodeId(4),
                                path: "TEST/3:Values".to_owned(),
                                name: "Values".to_owned(),
                                required: true,
                                conflict_priority: ConflictPriority::Normal,
                                condition: None,
                                kind: SchemaNodeKind::Subrecord {
                                    signature: SchemaSignature(*b"VALU"),
                                    payload: Box::new(SchemaNode {
                                        id: SchemaNodeId(104),
                                        path: "TEST/3:Values/payload".to_owned(),
                                        name: "Values".to_owned(),
                                        required: true,
                                        conflict_priority: ConflictPriority::Normal,
                                        condition: None,
                                        kind: SchemaNodeKind::Primitive {
                                            primitive: PrimitiveType::Bytes { length: None },
                                        },
                                    }),
                                },
                            },
                        ],
                    },
                },
            }],
        )?;
        Ok(RecordEditor {
            registry: bethkit_schema::SchemaRegistry::new(Arc::new(package)),
            decoders: crate::DecoderRegistry::builtin(),
            handlers: SemanticHandlerRegistry::builtin(),
            record: WritableRecord {
                signature: Signature(*b"TEST"),
                flags: bethkit_core::RecordFlags::empty(),
                form_id: bethkit_core::FormId::NULL,
                form_version: 44,
                subrecords: vec![
                    WritableSubRecord {
                        signature: Signature(*b"AAAA"),
                        data: vec![1],
                    },
                    WritableSubRecord {
                        signature: Signature(*b"AAAA"),
                        data: vec![2],
                    },
                    WritableSubRecord {
                        signature: Signature(*b"VALU"),
                        data: Vec::new(),
                    },
                ],
            },
            localized: false,
            decoded_values: BTreeMap::new(),
        })
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

    /// Addresses repeated signatures by their ordered schema path.
    #[test]
    fn editor_distinguishes_reused_signatures_by_path() -> Result<()> {
        let editor = editor_with_reused_signature()?;

        assert_eq!(
            editor.assigned_subrecord_index(&editor.record, "TEST/first", 0)?,
            0
        );
        assert_eq!(
            editor.assigned_subrecord_index(&editor.record, "TEST/second", 0)?,
            1
        );
        assert!(matches!(
            editor.assigned_subrecord_index(&editor.record, "TEST/second", 1),
            Err(SemanticError::MissingOccurrence { occurrence: 1, .. })
        ));
        Ok(())
    }

    /// Inserts, updates, and removes optional counters in schema order.
    #[test]
    fn editor_synchronizes_optional_counter_transactionally() -> Result<()> {
        let editor = editor_with_reused_signature()?;
        let mut record = clone_record(&editor.record);

        editor.apply_mutations(
            &mut record,
            vec![HandlerMutation::SynchronizeCount {
                path: "TEST/2:Value Count".to_owned(),
                occurrence: 0,
                value: 3,
                remove_when_zero: true,
            }],
        )?;

        assert_eq!(
            record
                .subrecords
                .iter()
                .map(|subrecord| subrecord.signature)
                .collect::<Vec<_>>(),
            vec![
                Signature(*b"AAAA"),
                Signature(*b"AAAA"),
                Signature(*b"VCNT"),
                Signature(*b"VALU")
            ]
        );
        assert_eq!(record.subrecords[2].data, 3_u32.to_le_bytes());

        editor.apply_mutations(
            &mut record,
            vec![HandlerMutation::SynchronizeCount {
                path: "TEST/2:Value Count".to_owned(),
                occurrence: 0,
                value: 0,
                remove_when_zero: true,
            }],
        )?;
        assert_eq!(
            record
                .subrecords
                .iter()
                .map(|subrecord| subrecord.signature)
                .collect::<Vec<_>>(),
            vec![
                Signature(*b"AAAA"),
                Signature(*b"AAAA"),
                Signature(*b"VALU")
            ]
        );
        Ok(())
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
    fn after_set_equality_matches_owned_and_decoded_integer_shapes() {
        let owned_shape = FieldValue::Struct(vec![crate::NamedValue {
            node_id: bethkit_schema::SchemaNodeId(1),
            path: "TEST/0:Mode".to_owned(),
            name: "Mode".to_owned(),
            span: crate::ByteSpan { start: 0, end: 4 },
            value: FieldValue::Int(2),
        }]);
        let decoded_shape = FieldValue::Struct(vec![crate::NamedValue {
            node_id: bethkit_schema::SchemaNodeId(1),
            path: "TEST/0:Mode".to_owned(),
            name: "Mode".to_owned(),
            span: crate::ByteSpan { start: 0, end: 4 },
            value: FieldValue::Enumeration {
                value: 2,
                name: Some("Reference".to_owned()),
            },
        }]);

        assert!(handler_values_equal(&owned_shape, &decoded_shape));
    }

    #[test]
    fn after_set_equality_detects_nested_array_changes() {
        let old = FieldValue::Array(vec![FieldValue::UInt(1), FieldValue::UInt(2)]);
        let new = FieldValue::Array(vec![FieldValue::UInt(1), FieldValue::UInt(3)]);

        assert!(!handler_values_equal(&new, &old));
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
                allowed_values: Vec::new(),
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
                allowed_values: Vec::new(),
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
