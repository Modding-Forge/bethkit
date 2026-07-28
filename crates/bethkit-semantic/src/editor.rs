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

use crate::handler::HandlerInvocationAccess;
use crate::value::{float_to_raw, handler_to_owned_value};
use crate::{
    grammar::{interpret_writable, RepeatScope},
    FieldValue, HandlerMutation, HandlerOutput, HandlerPhase, HandlerRecordContext,
    OwnedFieldValue, ParsedEditValue, Result, SemanticContext, SemanticError,
    SemanticHandlerRegistry,
};

#[derive(Clone)]
struct ChangedField {
    path: String,
    repeat_scopes: Vec<RepeatScope>,
}

#[derive(Clone, Copy)]
struct UnionSelectionContext<'a> {
    field_values: &'a BTreeMap<String, i64>,
    source_subrecord_index: Option<usize>,
    value_scope: Option<&'a FieldValue<'static>>,
}

/// Lossless editor for one record.
pub struct RecordEditor {
    registry: bethkit_schema::SchemaRegistry,
    decoders: crate::DecoderRegistry,
    handlers: SemanticHandlerRegistry,
    record: WritableRecord,
    localized: bool,
    after_load_migrations: usize,
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
        let mut editor = Self {
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
            after_load_migrations: 0,
            decoded_values: BTreeMap::new(),
        };
        editor.apply_after_load_callbacks()?;
        let snapshot = Record::from_writable(&editor.record);
        editor.decoded_values = context
            .view(&snapshot, plugin_localized)?
            .fields()?
            .into_iter()
            .map(|field| {
                (
                    (field.path, field.occurrence),
                    field.value.to_handler_value(),
                )
            })
            .collect();
        Ok(editor)
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
        let encoded: Vec<u8> = self.encode_node_at(payload, &normalized, Some(index))?;
        let decoded = self.owned_to_handler_value_at(payload, &normalized, Some(index))?;
        let mut candidate = clone_record(&self.record);
        let mut decoded_values = self.decoded_values.clone();
        candidate.subrecords[index].data = encoded;
        let changed = self.changed_field_at(&candidate, index)?;
        decoded_values.insert((path.to_owned(), occurrence), decoded);
        self.apply_local_mutations_with_scope(
            &mut candidate,
            &mut decoded_values,
            &changed,
            mutations,
        )?;
        self.apply_after_set_callbacks(&mut candidate, &mut decoded_values, &changed)?;
        self.record = candidate;
        self.decoded_values = decoded_values;
        Ok(())
    }

    /// Applies a parsed edit value to one nested schema-path occurrence.
    ///
    /// The primary value and any sibling mutations produced by the parser are
    /// committed as one transaction.
    ///
    /// # Errors
    ///
    /// Returns [`SemanticError`] when the path or occurrence is absent, the
    /// parsed value is invalid, or a sibling mutation cannot be applied.
    pub fn set_parsed_value(
        &mut self,
        path: &str,
        occurrence: usize,
        parsed: &ParsedEditValue,
    ) -> Result<()> {
        let target = self.find_node(path)?.clone();
        let schema = self
            .registry
            .get(self.record.signature)
            .ok_or_else(|| SemanticError::MissingRecordSchema(self.record.signature.to_string()))?;
        let parent = find_containing_subrecord(&schema.root, path)
            .ok_or_else(|| SemanticError::MissingPath(path.to_owned()))?
            .clone();
        let SchemaNodeKind::Subrecord { payload, .. } = &parent.kind else {
            return Err(SemanticError::Encode {
                path: path.to_owned(),
                message: "parsed edit has no containing subrecord".to_owned(),
            });
        };
        let normalized = self.normalize_value(&target.path, parsed.value())?;
        let parent_count = self.assigned_occurrence_count(&self.record, &parent.path)?;
        let mut remaining = occurrence;
        for parent_occurrence in 0..parent_count {
            let Some(current) = self
                .decoded_values
                .get(&(parent.path.clone(), parent_occurrence))
                .cloned()
            else {
                continue;
            };
            let old_owned = handler_to_owned_value(current.clone(), &payload.path)?;
            let mut updated = old_owned.clone();
            let mut replacement = Some(normalized.clone());
            if !self.set_nested_value(
                payload,
                &mut updated,
                path,
                &mut remaining,
                &mut replacement,
            )? {
                continue;
            }
            let (updated, mut mutations) =
                self.apply_after_set_tree(payload, &updated, Some(&current))?;
            mutations.extend_from_slice(parsed.mutations());
            let index =
                self.assigned_subrecord_index(&self.record, &parent.path, parent_occurrence)?;
            let encoded = self.encode_node_at(payload, &updated, Some(index))?;
            let decoded = self.owned_to_handler_value_at(payload, &updated, Some(index))?;
            let mut candidate = clone_record(&self.record);
            let mut decoded_values = self.decoded_values.clone();
            candidate.subrecords[index].data = encoded;
            let changed = self.changed_field_at(&candidate, index)?;
            decoded_values.insert((parent.path.clone(), parent_occurrence), decoded);
            self.apply_local_mutations_with_scope(
                &mut candidate,
                &mut decoded_values,
                &changed,
                mutations,
            )?;
            self.apply_after_set_callbacks(&mut candidate, &mut decoded_values, &changed)?;
            self.record = candidate;
            self.decoded_values = decoded_values;
            return Ok(());
        }
        Err(SemanticError::MissingOccurrence {
            path: path.to_owned(),
            occurrence,
        })
    }

    /// Sets a record editor ID through its classified xEdit callback.
    ///
    /// Returns `false` when the record schema has no custom editor-ID setter.
    /// The callback mutation is applied transactionally to its containing
    /// subrecord, including nested fixed-size fields.
    ///
    /// # Errors
    ///
    /// Returns [`SemanticError::Handler`] when the callback is duplicated,
    /// not executable, or returns an invalid mutation. Returns
    /// [`SemanticError::Encode`] when the replacement cannot be encoded.
    pub fn set_record_editor_id(&mut self, editor_id: &str) -> Result<bool> {
        let record_path = self.record.signature.to_string();
        let mut bindings = self
            .registry
            .package()
            .callback_bindings()
            .iter()
            .filter(|binding| {
                binding.path == record_path && binding.callback_id == "record.set_editor_id"
            });
        let Some(binding) = bindings.next() else {
            return Ok(false);
        };
        if bindings.next().is_some() {
            return Err(SemanticError::Handler {
                handler: "record.set_editor_id".to_owned(),
                message: "record editor-ID callback is bound more than once".to_owned(),
            });
        }
        if !matches!(
            binding.implementation,
            CallbackImplementation::BuiltIn { .. } | CallbackImplementation::CustomHandler { .. }
        ) {
            return Err(SemanticError::Handler {
                handler: "record.set_editor_id".to_owned(),
                message: "record editor-ID callback is not executable".to_owned(),
            });
        }
        let value = FieldValue::String(Cow::Owned(editor_id.to_owned()));
        let mutations = match self.handlers.invoke(
            binding,
            self.handler_record(),
            HandlerPhase::AfterSet,
            Some(&value),
            None,
        )? {
            HandlerOutput::Mutations(mutations) => mutations,
            _ => {
                return Err(SemanticError::Handler {
                    handler: "record.set_editor_id".to_owned(),
                    message: "record editor-ID callback returned invalid mutations".to_owned(),
                });
            }
        };
        self.apply_record_metadata_mutations(mutations)?;
        Ok(true)
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
        let encoded: Vec<u8> = self.encode_node_at(payload, &normalized, Some(insertion_index))?;
        let decoded =
            self.owned_to_handler_value_at(payload, &normalized, Some(insertion_index))?;
        let mut candidate = clone_record(&self.record);
        candidate.subrecords.insert(
            insertion_index,
            WritableSubRecord {
                signature: target_signature,
                data: encoded,
            },
        );
        let changed = self.changed_field_at(&candidate, insertion_index)?;
        let mut decoded_values = self.decoded_values.clone();
        decoded_values.insert((path.to_owned(), occurrence), decoded);
        self.apply_local_mutations_with_scope(
            &mut candidate,
            &mut decoded_values,
            &changed,
            mutations,
        )?;
        self.apply_after_set_callbacks(&mut candidate, &mut decoded_values, &changed)?;
        self.record = candidate;
        self.decoded_values = decoded_values;
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
        let changed = self.changed_field_at(&self.record, index)?;
        let mut candidate = clone_record(&self.record);
        let mut decoded_values = self.decoded_values.clone();
        candidate.subrecords.remove(index);
        remove_decoded_occurrence(&mut decoded_values, path, occurrence);
        self.apply_after_set_callbacks(&mut candidate, &mut decoded_values, &changed)?;
        self.record = candidate;
        self.decoded_values = decoded_values;
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

    /// Returns the number of normalization operations applied by `after_load` callbacks.
    pub fn after_load_migration_count(&self) -> usize {
        self.after_load_migrations
    }

    fn apply_after_load_callbacks(&mut self) -> Result<()> {
        let record_path = self.record.signature.to_string();
        let bindings = self
            .registry
            .package()
            .callback_bindings()
            .iter()
            .filter(|binding| {
                binding.callback_id == "def.after_load"
                    && (binding.path == record_path
                        || binding
                            .path
                            .strip_prefix(&record_path)
                            .is_some_and(|suffix| suffix.starts_with('/')))
            })
            .cloned()
            .collect::<Vec<_>>();
        for binding in bindings {
            if !matches!(
                binding.implementation,
                CallbackImplementation::BuiltIn { .. }
                    | CallbackImplementation::CustomHandler { .. }
            ) {
                return Err(SemanticError::Handler {
                    handler: binding.callback_id,
                    message: "after-load callback is not executable".to_owned(),
                });
            }
            let anchor_path = match &binding.implementation {
                CallbackImplementation::BuiltIn { operation } => operation
                    .configuration
                    .get("anchor_path_suffix")
                    .and_then(serde_json::Value::as_str)
                    .map_or_else(
                        || self.after_load_anchor_path(&binding.path),
                        |suffix| Ok(format!("{}{suffix}", binding.path)),
                    )?,
                _ => self.after_load_anchor_path(&binding.path)?,
            };
            let targets = if binding.path == record_path && anchor_path == binding.path {
                vec![None]
            } else {
                let grammar = self.grammar_for(&self.record)?;
                grammar
                    .assignments
                    .iter()
                    .enumerate()
                    .filter_map(|(index, assignment)| {
                        assignment
                            .is_some_and(|node| node.path == anchor_path)
                            .then_some(Some(index))
                    })
                    .collect::<Vec<_>>()
            };
            for target in targets {
                let access = target.map_or_else(
                    || HandlerInvocationAccess::writable_with_scope(&self.record, None),
                    |index| {
                        HandlerInvocationAccess::writable_subrecord_with_scope(
                            &self.record,
                            index,
                            None,
                        )
                    },
                );
                let output = self.handlers.invoke_with_records(
                    &binding,
                    self.handler_record(),
                    access,
                    HandlerPhase::AfterLoad,
                    None,
                    None,
                )?;
                match output {
                    HandlerOutput::None => {}
                    HandlerOutput::SubrecordPayload(data) => {
                        let Some(index) = target else {
                            return Err(SemanticError::Handler {
                                handler: binding.callback_id.clone(),
                                message: "record-level after-load callback returned payload bytes"
                                    .to_owned(),
                            });
                        };
                        self.record.subrecords[index].data = data;
                        self.after_load_migrations = self.after_load_migrations.saturating_add(1);
                    }
                    HandlerOutput::Mutations(mutations) => {
                        if mutations.is_empty() {
                            continue;
                        }
                        let mut updated = WritableRecord {
                            signature: self.record.signature,
                            flags: self.record.flags,
                            form_id: self.record.form_id,
                            form_version: self.record.form_version,
                            subrecords: self
                                .record
                                .subrecords
                                .iter()
                                .map(|subrecord| WritableSubRecord {
                                    signature: subrecord.signature,
                                    data: subrecord.data.clone(),
                                })
                                .collect(),
                        };
                        let mut decoded_values = BTreeMap::new();
                        self.apply_mutations_with_values(
                            &mut updated,
                            &mut decoded_values,
                            mutations,
                        )?;
                        self.record = updated;
                        self.after_load_migrations = self.after_load_migrations.saturating_add(1);
                    }
                    _ => {
                        return Err(SemanticError::Handler {
                            handler: binding.callback_id.clone(),
                            message: "after-load callback returned an invalid result".to_owned(),
                        });
                    }
                }
            }
        }
        Ok(())
    }

    fn find_node(&self, path: &str) -> Result<&SchemaNode> {
        let schema = self
            .registry
            .get(self.record.signature)
            .ok_or_else(|| SemanticError::MissingRecordSchema(self.record.signature.to_string()))?;
        find_node_by_path(&schema.root, path)
            .ok_or_else(|| SemanticError::MissingPath(path.to_owned()))
    }

    fn after_load_anchor_path(&self, path: &str) -> Result<String> {
        let mut candidate = path;
        loop {
            if self
                .find_node(candidate)
                .is_ok_and(|node| matches!(&node.kind, SchemaNodeKind::Subrecord { .. }))
            {
                return Ok(candidate.to_owned());
            }
            let Some((parent, _)) = candidate.rsplit_once('/') else {
                return Ok(path.to_owned());
            };
            candidate = parent;
        }
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
        self.encode_node_at(node, value, None)
    }

    fn encode_node_at(
        &self,
        node: &SchemaNode,
        value: &OwnedFieldValue,
        source_subrecord_index: Option<usize>,
    ) -> Result<Vec<u8>> {
        let mut field_values = self.expression_field_values();
        collect_owned_expression_field_values(node, value, &mut field_values);
        self.encode_node_with_fields(node, value, &field_values, source_subrecord_index)
    }

    fn encode_node_with_fields(
        &self,
        node: &SchemaNode,
        value: &OwnedFieldValue,
        field_values: &BTreeMap<String, i64>,
        source_subrecord_index: Option<usize>,
    ) -> Result<Vec<u8>> {
        self.encode_node_with_scope(node, value, field_values, source_subrecord_index, None)
    }

    fn encode_node_with_scope(
        &self,
        node: &SchemaNode,
        value: &OwnedFieldValue,
        field_values: &BTreeMap<String, i64>,
        source_subrecord_index: Option<usize>,
        value_scope: Option<&FieldValue<'static>>,
    ) -> Result<Vec<u8>> {
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
                let mut scope_values: Vec<crate::NamedValue<'static>> =
                    Vec::with_capacity(fields.len());
                for (field, value) in fields.iter().zip(values) {
                    let scope = FieldValue::Struct(scope_values.clone());
                    let encoded = self.encode_node_with_scope(
                        field,
                        value,
                        field_values,
                        source_subrecord_index,
                        Some(&scope),
                    )?;
                    let handler_value = if matches!(&field.kind, SchemaNodeKind::Union { .. }) {
                        FieldValue::Bytes(Cow::Owned(encoded.clone()))
                    } else {
                        self.owned_to_handler_value_with_fields(
                            field,
                            value,
                            field_values,
                            source_subrecord_index,
                        )?
                    };
                    scope_values.push(crate::NamedValue {
                        node_id: field.id,
                        path: field.path.clone(),
                        effective_path: None,
                        name: field.name.clone(),
                        span: crate::ByteSpan {
                            start: output.len(),
                            end: output.len() + encoded.len(),
                        },
                        value: handler_value,
                    });
                    output.extend(encoded);
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
                    | ArrayCount::Callback { .. }
                    | ArrayCount::Remainder => {}
                }
                for value in values {
                    output.extend(self.encode_node_with_scope(
                        element,
                        value,
                        field_values,
                        source_subrecord_index,
                        value_scope,
                    )?);
                }
                Ok(output)
            }
            SchemaNodeKind::Union { selector, variants } => {
                let variant = self.select_union_variant(
                    node,
                    selector,
                    variants,
                    value,
                    UnionSelectionContext {
                        field_values,
                        source_subrecord_index,
                        value_scope,
                    },
                )?;
                self.encode_node_with_scope(
                    variant,
                    value,
                    field_values,
                    source_subrecord_index,
                    value_scope,
                )
            }
            SchemaNodeKind::Custom { decoder, .. } => self
                .decoders
                .get(decoder)
                .ok_or_else(|| SemanticError::MissingDecoder(decoder.clone()))?
                .encode(value),
            SchemaNodeKind::Terminated { terminator, child } => {
                let mut output = self.encode_node_with_scope(
                    child,
                    value,
                    field_values,
                    source_subrecord_index,
                    value_scope,
                )?;
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
        let mut field_values = self.expression_field_values();
        collect_owned_expression_field_values(node, value, &mut field_values);
        self.apply_after_set_tree_with_fields(node, value, old_value, &field_values)
    }

    fn apply_after_set_tree_with_fields(
        &self,
        node: &SchemaNode,
        value: &OwnedFieldValue,
        old_value: Option<&FieldValue<'static>>,
        field_values: &BTreeMap<String, i64>,
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
                let mut updated = values.clone();
                let mut mutations = Vec::new();
                let old_fields = match old_value {
                    Some(FieldValue::Struct(values)) => Some(values.as_slice()),
                    _ => None,
                };
                for (index, field) in fields.iter().enumerate() {
                    let old_field = old_fields
                        .and_then(|values| values.get(index))
                        .map(|value| &value.value);
                    let (value, child_mutations) = self.apply_after_set_tree_with_fields(
                        field,
                        &updated[index],
                        old_field,
                        field_values,
                    )?;
                    updated[index] = value;
                    let mut child_mutations = child_mutations;
                    let mut container = OwnedFieldValue::Struct(updated);
                    self.apply_local_default_mutations(
                        node,
                        &mut container,
                        &mut child_mutations,
                        field_values,
                    )?;
                    let OwnedFieldValue::Struct(next) = container else {
                        return Err(encode_error(
                            &node.path,
                            "default mutation replaced a struct container",
                        ));
                    };
                    updated = next;
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
                    let (value, child_mutations) = self.apply_after_set_tree_with_fields(
                        element,
                        value,
                        old_element,
                        field_values,
                    )?;
                    updated.push(value);
                    mutations.extend(child_mutations);
                }
                (OwnedFieldValue::Array(updated), mutations)
            }
            (SchemaNodeKind::Union { selector, variants }, _) => {
                let variant = self.select_union_variant(
                    node,
                    selector,
                    variants,
                    value,
                    UnionSelectionContext {
                        field_values,
                        source_subrecord_index: None,
                        value_scope: None,
                    },
                )?;
                self.apply_after_set_tree_with_fields(variant, value, old_value, field_values)?
            }
            (
                SchemaNodeKind::Subrecord { payload, .. }
                | SchemaNodeKind::Compressed { child: payload, .. }
                | SchemaNodeKind::Terminated { child: payload, .. },
                _,
            ) => self.apply_after_set_tree_with_fields(payload, value, old_value, field_values)?,
            _ => (value.clone(), Vec::new()),
        };
        self.apply_local_default_mutations(node, &mut updated, &mut mutations, field_values)?;
        self.apply_local_set_mutations(node, &mut updated, &mut mutations)?;
        let handler_value =
            self.owned_to_handler_value_with_fields(node, &updated, field_values, None)?;
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
            let handler_value =
                self.owned_to_handler_value_with_fields(node, &updated, field_values, None)?;
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
        self.apply_local_default_mutations(node, &mut updated, &mut mutations, field_values)?;
        self.apply_local_set_mutations(node, &mut updated, &mut mutations)?;
        Ok((updated, mutations))
    }

    fn apply_local_default_mutations(
        &self,
        node: &SchemaNode,
        value: &mut OwnedFieldValue,
        mutations: &mut Vec<HandlerMutation>,
        field_values: &BTreeMap<String, i64>,
    ) -> Result<()> {
        let mut remaining = Vec::with_capacity(mutations.len());
        for mutation in mutations.drain(..) {
            let HandlerMutation::ResetToDefault { path, occurrence } = mutation else {
                remaining.push(mutation);
                continue;
            };
            if path != node.path
                && !path
                    .strip_prefix(&node.path)
                    .is_some_and(|suffix| suffix.starts_with('/'))
            {
                remaining.push(HandlerMutation::ResetToDefault { path, occurrence });
                continue;
            }
            let mut target_occurrence = occurrence;
            if !self.reset_nested_value_with_fields(
                node,
                value,
                &path,
                &mut target_occurrence,
                field_values,
                &[],
            )? {
                return Err(SemanticError::MissingOccurrence { path, occurrence });
            }
        }
        *mutations = remaining;
        Ok(())
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
                HandlerMutation::SetIfEqual {
                    path,
                    occurrence,
                    expected,
                    value: replacement,
                } => {
                    if path != node.path
                        && !path
                            .strip_prefix(&node.path)
                            .is_some_and(|suffix| suffix.starts_with('/'))
                    {
                        remaining.push(HandlerMutation::SetIfEqual {
                            path,
                            occurrence,
                            expected,
                            value: replacement,
                        });
                        continue;
                    }
                    let mut target_occurrence = occurrence;
                    let current =
                        self.nested_value_at(node, value, &path, &mut target_occurrence)?;
                    let Some(current) = current else {
                        return Err(SemanticError::MissingOccurrence { path, occurrence });
                    };
                    if !owned_values_equal(current, &expected) {
                        continue;
                    }
                    let mut target_occurrence = occurrence;
                    let mut replacement = Some(replacement);
                    if !self.set_nested_value(
                        node,
                        value,
                        &path,
                        &mut target_occurrence,
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

    fn nested_value_at<'a>(
        &self,
        node: &SchemaNode,
        value: &'a OwnedFieldValue,
        target_path: &str,
        occurrence: &mut usize,
    ) -> Result<Option<&'a OwnedFieldValue>> {
        let mut field_values = self.expression_field_values();
        collect_owned_expression_field_values(node, value, &mut field_values);
        self.nested_value_at_with_fields(node, value, target_path, occurrence, &field_values)
    }

    fn nested_value_at_with_fields<'a>(
        &self,
        node: &SchemaNode,
        value: &'a OwnedFieldValue,
        target_path: &str,
        occurrence: &mut usize,
        field_values: &BTreeMap<String, i64>,
    ) -> Result<Option<&'a OwnedFieldValue>> {
        if node.path == target_path {
            if *occurrence == 0 {
                return Ok(Some(value));
            }
            *occurrence = occurrence.saturating_sub(1);
            return Ok(None);
        }
        match (&node.kind, value) {
            (SchemaNodeKind::Struct { fields }, OwnedFieldValue::Struct(values)) => {
                for (field, value) in fields.iter().zip(values) {
                    if let Some(found) = self.nested_value_at_with_fields(
                        field,
                        value,
                        target_path,
                        occurrence,
                        field_values,
                    )? {
                        return Ok(Some(found));
                    }
                }
            }
            (SchemaNodeKind::Array { element, .. }, OwnedFieldValue::Array(values)) => {
                for value in values {
                    if let Some(found) = self.nested_value_at_with_fields(
                        element,
                        value,
                        target_path,
                        occurrence,
                        field_values,
                    )? {
                        return Ok(Some(found));
                    }
                }
            }
            (SchemaNodeKind::Union { selector, variants }, current) => {
                let variant = self.select_union_variant(
                    node,
                    selector,
                    variants,
                    current,
                    UnionSelectionContext {
                        field_values,
                        source_subrecord_index: None,
                        value_scope: None,
                    },
                )?;
                return self.nested_value_at_with_fields(
                    variant,
                    current,
                    target_path,
                    occurrence,
                    field_values,
                );
            }
            (
                SchemaNodeKind::Subrecord { payload, .. }
                | SchemaNodeKind::Compressed { child: payload, .. }
                | SchemaNodeKind::Terminated { child: payload, .. },
                current,
            ) => {
                return self.nested_value_at_with_fields(
                    payload,
                    current,
                    target_path,
                    occurrence,
                    field_values,
                );
            }
            _ => {}
        }
        Ok(None)
    }

    fn set_nested_value(
        &self,
        node: &SchemaNode,
        value: &mut OwnedFieldValue,
        target_path: &str,
        occurrence: &mut usize,
        replacement: &mut Option<OwnedFieldValue>,
    ) -> Result<bool> {
        let mut field_values = self.expression_field_values();
        collect_owned_expression_field_values(node, value, &mut field_values);
        self.set_nested_value_with_fields(
            node,
            value,
            target_path,
            occurrence,
            replacement,
            &field_values,
        )
    }

    fn set_nested_value_with_fields(
        &self,
        node: &SchemaNode,
        value: &mut OwnedFieldValue,
        target_path: &str,
        occurrence: &mut usize,
        replacement: &mut Option<OwnedFieldValue>,
        field_values: &BTreeMap<String, i64>,
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
                    if self.set_nested_value_with_fields(
                        field,
                        value,
                        target_path,
                        occurrence,
                        replacement,
                        field_values,
                    )? {
                        return Ok(true);
                    }
                }
            }
            (SchemaNodeKind::Array { element, .. }, OwnedFieldValue::Array(values)) => {
                for value in values {
                    if self.set_nested_value_with_fields(
                        element,
                        value,
                        target_path,
                        occurrence,
                        replacement,
                        field_values,
                    )? {
                        return Ok(true);
                    }
                }
            }
            (SchemaNodeKind::Union { selector, variants }, current) => {
                let variant = self.select_union_variant(
                    node,
                    selector,
                    variants,
                    current,
                    UnionSelectionContext {
                        field_values,
                        source_subrecord_index: None,
                        value_scope: None,
                    },
                )?;
                if self.set_nested_value_with_fields(
                    variant,
                    current,
                    target_path,
                    occurrence,
                    replacement,
                    field_values,
                )? {
                    return Ok(true);
                }
            }
            (
                SchemaNodeKind::Subrecord { payload, .. }
                | SchemaNodeKind::Compressed { child: payload, .. }
                | SchemaNodeKind::Terminated { child: payload, .. },
                current,
            ) => {
                if self.set_nested_value_with_fields(
                    payload,
                    current,
                    target_path,
                    occurrence,
                    replacement,
                    field_values,
                )? {
                    return Ok(true);
                }
            }
            _ => {}
        }
        Ok(false)
    }

    fn reset_nested_value_with_fields(
        &self,
        node: &SchemaNode,
        value: &mut OwnedFieldValue,
        target_path: &str,
        occurrence: &mut usize,
        field_values: &BTreeMap<String, i64>,
        array_indices: &[usize],
    ) -> Result<bool> {
        if node.path == target_path {
            if *occurrence == 0 {
                *value = self.default_value_for_node(node, field_values, 0, array_indices)?;
                return Ok(true);
            }
            *occurrence = occurrence.saturating_sub(1);
            return Ok(false);
        }
        match (&node.kind, &mut *value) {
            (SchemaNodeKind::Struct { fields }, OwnedFieldValue::Struct(values)) => {
                for (field, value) in fields.iter().zip(values) {
                    if self.reset_nested_value_with_fields(
                        field,
                        value,
                        target_path,
                        occurrence,
                        field_values,
                        array_indices,
                    )? {
                        return Ok(true);
                    }
                }
            }
            (SchemaNodeKind::Array { element, .. }, OwnedFieldValue::Array(values)) => {
                for (index, value) in values.iter_mut().enumerate() {
                    let mut child_array_indices = array_indices.to_vec();
                    child_array_indices.push(index);
                    if self.reset_nested_value_with_fields(
                        element,
                        value,
                        target_path,
                        occurrence,
                        field_values,
                        &child_array_indices,
                    )? {
                        return Ok(true);
                    }
                }
            }
            (SchemaNodeKind::Union { selector, variants }, current) => {
                let variant = self.select_union_variant(
                    node,
                    selector,
                    variants,
                    current,
                    UnionSelectionContext {
                        field_values,
                        source_subrecord_index: None,
                        value_scope: None,
                    },
                )?;
                if self.reset_nested_value_with_fields(
                    variant,
                    current,
                    target_path,
                    occurrence,
                    field_values,
                    array_indices,
                )? {
                    return Ok(true);
                }
            }
            (
                SchemaNodeKind::Subrecord { payload, .. }
                | SchemaNodeKind::Compressed { child: payload, .. }
                | SchemaNodeKind::Terminated { child: payload, .. },
                current,
            ) => {
                if self.reset_nested_value_with_fields(
                    payload,
                    current,
                    target_path,
                    occurrence,
                    field_values,
                    array_indices,
                )? {
                    return Ok(true);
                }
            }
            _ => {}
        }
        Ok(false)
    }

    fn default_value_for_node(
        &self,
        node: &SchemaNode,
        field_values: &BTreeMap<String, i64>,
        depth: usize,
        array_indices: &[usize],
    ) -> Result<OwnedFieldValue> {
        if depth >= 128 {
            return Err(encode_error(
                &node.path,
                "schema default recursion exceeds 128 nodes",
            ));
        }
        let next_depth = depth.saturating_add(1);
        let value = match &node.kind {
            SchemaNodeKind::Primitive { primitive } => self.default_primitive_value(primitive),
            SchemaNodeKind::Struct { fields } => fields
                .iter()
                .map(|field| {
                    self.default_value_for_node(field, field_values, next_depth, array_indices)
                })
                .collect::<Result<Vec<_>>>()
                .map(OwnedFieldValue::Struct)?,
            SchemaNodeKind::Array { element, count } => {
                let count = match count {
                    ArrayCount::Fixed { count } => usize::try_from(*count).map_err(|_| {
                        encode_error(&node.path, "fixed default array count exceeds usize")
                    })?,
                    _ => 0,
                };
                (0..count)
                    .map(|index| {
                        let mut child_array_indices = array_indices.to_vec();
                        child_array_indices.push(index);
                        self.default_value_for_node(
                            element,
                            field_values,
                            next_depth,
                            &child_array_indices,
                        )
                    })
                    .collect::<Result<Vec<_>>>()
                    .map(OwnedFieldValue::Array)?
            }
            SchemaNodeKind::Union { selector, variants } => {
                let variant =
                    self.select_default_union_variant(node, selector, variants, field_values)?;
                self.default_value_for_node(variant, field_values, next_depth, array_indices)?
            }
            SchemaNodeKind::Subrecord { payload, .. } => {
                self.default_value_for_node(payload, field_values, next_depth, array_indices)?
            }
            SchemaNodeKind::Compressed { child, .. } | SchemaNodeKind::Terminated { child, .. } => {
                self.default_value_for_node(child, field_values, next_depth, array_indices)?
            }
            SchemaNodeKind::Custom { decoder, .. } => {
                return Err(SemanticError::Encode {
                    path: node.path.clone(),
                    message: format!("custom decoder {decoder} has no schema-native default"),
                });
            }
            SchemaNodeKind::Sequence { .. }
            | SchemaNodeKind::Choice { .. }
            | SchemaNodeKind::SelectedChoice { .. }
            | SchemaNodeKind::Repeat { .. }
            | SchemaNodeKind::Reference { .. } => {
                return Err(SemanticError::Encode {
                    path: node.path.clone(),
                    message: "this schema node has no editable default value".to_owned(),
                });
            }
        };
        self.apply_default_value_callbacks(node, value, field_values, array_indices)
    }

    fn apply_default_value_callbacks(
        &self,
        node: &SchemaNode,
        mut value: OwnedFieldValue,
        field_values: &BTreeMap<String, i64>,
        array_indices: &[usize],
    ) -> Result<OwnedFieldValue> {
        for binding in self
            .registry
            .package()
            .callback_bindings()
            .iter()
            .filter(|binding| {
                binding.path == node.path && binding.callback_id == "value.set_default"
            })
        {
            let handler_value =
                self.owned_to_handler_value_with_fields(node, &value, field_values, None)?;
            match self.handlers.invoke_with_records(
                binding,
                self.handler_record(),
                HandlerInvocationAccess::writable_with_scope(&self.record, None)
                    .with_array_indices(array_indices),
                HandlerPhase::DefaultValue,
                Some(&handler_value),
                None,
            )? {
                HandlerOutput::Value(updated) => {
                    value = handler_to_owned_value(updated, &node.path)?;
                }
                HandlerOutput::None => {}
                _ => {
                    return Err(SemanticError::Handler {
                        handler: binding.callback_id.clone(),
                        message: "default-value handler returned an invalid result".to_owned(),
                    });
                }
            }
        }
        Ok(value)
    }

    fn default_primitive_value(&self, primitive: &PrimitiveType) -> OwnedFieldValue {
        match primitive {
            PrimitiveType::Integer { integer } if integer.signed => OwnedFieldValue::Int(0),
            PrimitiveType::Integer { .. } | PrimitiveType::PackedUnsigned => {
                OwnedFieldValue::UInt(0)
            }
            PrimitiveType::Float { .. } => OwnedFieldValue::Float(0.0),
            PrimitiveType::String { string } if self.localized && is_localized_string(string) => {
                OwnedFieldValue::UInt(0)
            }
            PrimitiveType::String { .. } => OwnedFieldValue::String(String::new()),
            PrimitiveType::Bytes { length } => {
                OwnedFieldValue::Bytes(vec![0; length.unwrap_or(0) as usize])
            }
            PrimitiveType::FormId { .. } => OwnedFieldValue::FormId(bethkit_core::FormId::NULL),
            PrimitiveType::Enumeration { .. } => OwnedFieldValue::Int(0),
            PrimitiveType::Flags { .. } => OwnedFieldValue::UInt(0),
            PrimitiveType::Unused { length } => OwnedFieldValue::Bytes(vec![0; *length as usize]),
        }
    }

    fn select_default_union_variant<'a>(
        &self,
        node: &SchemaNode,
        selector: &UnionSelector,
        variants: &'a [SchemaNode],
        field_values: &BTreeMap<String, i64>,
    ) -> Result<&'a SchemaNode> {
        let UnionSelector::Expression(expression) = selector else {
            return Err(encode_error(
                &node.path,
                "callback-selected union has no declarative default",
            ));
        };
        let context = EvalContext {
            payload: &[],
            field_values,
            form_version: self.record.form_version,
            record_signature: self.record.signature.into(),
        };
        let selected = match expression.evaluate(&context, 1024) {
            Ok(EvalValue::Int(selected)) => selected,
            Ok(_) => {
                return Err(encode_error(
                    &node.path,
                    "default union selector returned a non-integer value",
                ));
            }
            Err(error) => {
                return Err(encode_error(
                    &node.path,
                    format!("default union selector failed: {error}"),
                ));
            }
        };
        let index = usize::try_from(selected)
            .map_err(|_| encode_error(&node.path, "default union selector returned a negative"))?;
        variants
            .get(index)
            .ok_or_else(|| encode_error(&node.path, "default union selector is out of range"))
    }

    fn select_union_variant<'a>(
        &self,
        node: &SchemaNode,
        selector: &UnionSelector,
        variants: &'a [SchemaNode],
        value: &OwnedFieldValue,
        context: UnionSelectionContext<'_>,
    ) -> Result<&'a SchemaNode> {
        let UnionSelectionContext {
            field_values,
            source_subrecord_index,
            value_scope,
        } = context;
        for (index, variant) in variants.iter().enumerate() {
            let Ok(encoded) = self.encode_node_with_scope(
                variant,
                value,
                field_values,
                source_subrecord_index,
                value_scope,
            ) else {
                continue;
            };
            let selected = match selector {
                UnionSelector::Expression(expression) => {
                    let context = EvalContext {
                        payload: &encoded,
                        field_values,
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
                    let output = if let Some(source_subrecord_index) = source_subrecord_index {
                        self.handlers.invoke_with_records(
                            binding,
                            self.handler_record(),
                            HandlerInvocationAccess::writable_subrecord_with_scope(
                                &self.record,
                                source_subrecord_index,
                                value_scope,
                            ),
                            HandlerPhase::UnionSelection,
                            Some(&raw_value),
                            None,
                        )?
                    } else {
                        self.handlers.invoke_with_records(
                            binding,
                            self.handler_record(),
                            HandlerInvocationAccess::writable_with_scope(&self.record, value_scope),
                            HandlerPhase::UnionSelection,
                            Some(&raw_value),
                            None,
                        )?
                    };
                    match output {
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

    fn expression_field_values(&self) -> BTreeMap<String, i64> {
        let mut output = BTreeMap::new();
        for ((path, _), value) in &self.decoded_values {
            collect_expression_field_values(path, value, &mut output);
        }
        output
    }

    fn owned_to_handler_value(
        &self,
        node: &SchemaNode,
        value: &OwnedFieldValue,
    ) -> Result<FieldValue<'static>> {
        self.owned_to_handler_value_at(node, value, None)
    }

    fn owned_to_handler_value_at(
        &self,
        node: &SchemaNode,
        value: &OwnedFieldValue,
        source_subrecord_index: Option<usize>,
    ) -> Result<FieldValue<'static>> {
        let mut field_values = self.expression_field_values();
        collect_owned_expression_field_values(node, value, &mut field_values);
        self.owned_to_handler_value_with_fields(node, value, &field_values, source_subrecord_index)
    }

    fn owned_to_handler_value_with_fields(
        &self,
        node: &SchemaNode,
        value: &OwnedFieldValue,
        field_values: &BTreeMap<String, i64>,
        source_subrecord_index: Option<usize>,
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
                            effective_path: None,
                            name: field.name.clone(),
                            span: crate::ByteSpan { start: 0, end: 0 },
                            value: self.owned_to_handler_value_with_fields(
                                field,
                                value,
                                field_values,
                                source_subrecord_index,
                            )?,
                        })
                    })
                    .collect::<Result<Vec<_>>>()
                    .map(FieldValue::Struct)
            }
            (SchemaNodeKind::Array { element, .. }, OwnedFieldValue::Array(values)) => values
                .iter()
                .map(|value| {
                    self.owned_to_handler_value_with_fields(
                        element,
                        value,
                        field_values,
                        source_subrecord_index,
                    )
                })
                .collect::<Result<Vec<_>>>()
                .map(FieldValue::Array),
            (SchemaNodeKind::Union { selector, variants }, _) => {
                let variant = self.select_union_variant(
                    node,
                    selector,
                    variants,
                    value,
                    UnionSelectionContext {
                        field_values,
                        source_subrecord_index,
                        value_scope: None,
                    },
                )?;
                self.owned_to_handler_value_with_fields(
                    variant,
                    value,
                    field_values,
                    source_subrecord_index,
                )
            }
            (
                SchemaNodeKind::Subrecord { payload, .. }
                | SchemaNodeKind::Compressed { child: payload, .. }
                | SchemaNodeKind::Terminated { child: payload, .. },
                _,
            ) => self.owned_to_handler_value_with_fields(
                payload,
                value,
                field_values,
                source_subrecord_index,
            ),
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

    fn apply_mutations_with_values(
        &self,
        record: &mut WritableRecord,
        decoded_values: &mut BTreeMap<(String, usize), FieldValue<'static>>,
        mutations: Vec<HandlerMutation>,
    ) -> Result<()> {
        for mutation in mutations {
            match mutation {
                HandlerMutation::Set {
                    path,
                    occurrence,
                    value,
                } => {
                    let node = self.find_node(&path)?;
                    if let SchemaNodeKind::Subrecord { signature, payload } = &node.kind {
                        let signature = Signature::from(*signature);
                        let index = self.assigned_subrecord_index(record, &path, occurrence)?;
                        let encoded = self.encode_node_at(payload, &value, Some(index))?;
                        if record.subrecords[index].signature != signature {
                            return Err(SemanticError::Encode {
                                path,
                                message: "assigned subrecord signature does not match schema"
                                    .to_owned(),
                            });
                        }
                        record.subrecords[index].data = encoded;
                        let decoded =
                            self.owned_to_handler_value_at(payload, &value, Some(index))?;
                        decoded_values.insert((path, occurrence), decoded);
                    } else {
                        self.apply_nested_set(record, decoded_values, &path, occurrence, value)?;
                    }
                }
                HandlerMutation::SetIfEqual {
                    path,
                    occurrence,
                    expected,
                    value,
                } => {
                    self.apply_nested_set_if_equal(
                        record,
                        decoded_values,
                        &path,
                        occurrence,
                        &expected,
                        value,
                    )?;
                }
                HandlerMutation::ReplacePayload {
                    path,
                    occurrence,
                    data,
                } => {
                    let node = self.find_node(&path)?;
                    let SchemaNodeKind::Subrecord { signature, .. } = &node.kind else {
                        return Err(SemanticError::Encode {
                            path,
                            message: "payload replacement path is not a subrecord".to_owned(),
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
                    record.subrecords[index].data = data;
                    decoded_values.remove(&(path, occurrence));
                }
                HandlerMutation::ResetToDefault { path, .. } => {
                    return Err(SemanticError::Handler {
                        handler: "edit.reset_sibling_default".to_owned(),
                        message: format!(
                            "schema-native default mutation for {path} escaped its value container"
                        ),
                    });
                }
                HandlerMutation::Insert { path, value } => {
                    let (signature, encoded) = self.encode_path(&path, &value)?;
                    let node = self.find_node(&path)?;
                    let SchemaNodeKind::Subrecord { payload, .. } = &node.kind else {
                        return Err(SemanticError::Encode {
                            path,
                            message: "handler mutation path is not a subrecord".to_owned(),
                        });
                    };
                    let occurrence = self.assigned_occurrence_count(record, &path)?;
                    let index = self.schema_insertion_index(record, node)?;
                    record.subrecords.insert(
                        index,
                        WritableSubRecord {
                            signature,
                            data: encoded,
                        },
                    );
                    let decoded = self.owned_to_handler_value(payload, &value)?;
                    decoded_values.insert((path, occurrence), decoded);
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
                    remove_decoded_occurrence(decoded_values, &path, occurrence);
                }
                HandlerMutation::RemoveAll { path } => {
                    let node = self.find_node(&path)?;
                    let SchemaNodeKind::Subrecord { signature, .. } = &node.kind else {
                        return Err(SemanticError::Encode {
                            path,
                            message: "handler mutation path is not a subrecord".to_owned(),
                        });
                    };
                    let signature = Signature::from(*signature);
                    loop {
                        let index = match self.assigned_subrecord_index(record, &path, 0) {
                            Ok(index) => index,
                            Err(SemanticError::MissingOccurrence { .. }) => break,
                            Err(error) => return Err(error),
                        };
                        if record.subrecords[index].signature != signature {
                            return Err(SemanticError::Encode {
                                path,
                                message: "assigned subrecord signature does not match schema"
                                    .to_owned(),
                            });
                        }
                        record.subrecords.remove(index);
                        remove_decoded_occurrence(decoded_values, &path, 0);
                    }
                }
                HandlerMutation::SynchronizeCount {
                    path,
                    occurrence,
                    value,
                    remove_when_zero,
                } => {
                    let node = self.find_node(&path)?;
                    let SchemaNodeKind::Subrecord { payload, .. } = &node.kind else {
                        return Err(SemanticError::Encode {
                            path,
                            message: "counter path is not a subrecord".to_owned(),
                        });
                    };
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
                            remove_decoded_occurrence(decoded_values, &path, occurrence);
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
                        decoded_values.insert(
                            (path, occurrence),
                            self.owned_to_handler_value(payload, &OwnedFieldValue::UInt(value))?,
                        );
                    } else {
                        let index = self.schema_insertion_index(record, node)?;
                        record.subrecords.insert(
                            index,
                            WritableSubRecord {
                                signature,
                                data: encoded,
                            },
                        );
                        decoded_values.insert(
                            (path, occurrence),
                            self.owned_to_handler_value(payload, &OwnedFieldValue::UInt(value))?,
                        );
                    }
                }
                HandlerMutation::SynchronizePresence {
                    path,
                    occurrence,
                    present,
                    value,
                } => {
                    let existing = self
                        .assigned_subrecord_index(record, &path, occurrence)
                        .map(Some)
                        .or_else(|error| match error {
                            SemanticError::MissingOccurrence { .. } => Ok(None),
                            _ => Err(error),
                        })?;
                    match (present, existing) {
                        (true, None) => {
                            let (signature, encoded) = self.encode_path(&path, &value)?;
                            let node = self.find_node(&path)?;
                            let SchemaNodeKind::Subrecord { payload, .. } = &node.kind else {
                                return Err(SemanticError::Encode {
                                    path,
                                    message: "presence path is not a subrecord".to_owned(),
                                });
                            };
                            let index = self.schema_insertion_index(record, node)?;
                            record.subrecords.insert(
                                index,
                                WritableSubRecord {
                                    signature,
                                    data: encoded,
                                },
                            );
                            let decoded = self.owned_to_handler_value(payload, &value)?;
                            decoded_values.insert((path, occurrence), decoded);
                        }
                        (false, Some(index)) => {
                            record.subrecords.remove(index);
                            remove_decoded_occurrence(decoded_values, &path, occurrence);
                        }
                        (true, Some(_)) | (false, None) => {}
                    }
                }
            }
        }
        Ok(())
    }

    fn apply_local_mutations_with_scope(
        &self,
        record: &mut WritableRecord,
        decoded_values: &mut BTreeMap<(String, usize), FieldValue<'static>>,
        changed: &ChangedField,
        mutations: Vec<HandlerMutation>,
    ) -> Result<()> {
        for mutation in mutations {
            let path = mutation_path(&mutation);
            let repeat_scope = changed
                .repeat_scopes
                .iter()
                .filter(|scope| path_is_within(path, &scope.path))
                .max_by_key(|scope| scope.path.len());
            if let Some(repeat_scope) = repeat_scope {
                self.apply_mutation_in_scope(record, decoded_values, repeat_scope, mutation)?;
            } else {
                self.apply_mutations_with_values(record, decoded_values, vec![mutation])?;
            }
        }
        Ok(())
    }

    fn apply_mutation_in_scope(
        &self,
        record: &mut WritableRecord,
        decoded_values: &mut BTreeMap<(String, usize), FieldValue<'static>>,
        repeat_scope: &RepeatScope,
        mutation: HandlerMutation,
    ) -> Result<()> {
        match mutation {
            HandlerMutation::Set {
                path,
                occurrence,
                value,
            } => {
                let (_, occurrence) = self
                    .scoped_assignment(record, repeat_scope, &path, occurrence)?
                    .ok_or_else(|| SemanticError::MissingOccurrence {
                        path: path.clone(),
                        occurrence,
                    })?;
                self.apply_mutations_with_values(
                    record,
                    decoded_values,
                    vec![HandlerMutation::Set {
                        path,
                        occurrence,
                        value,
                    }],
                )
            }
            HandlerMutation::SetIfEqual {
                path,
                occurrence,
                expected,
                value,
            } => {
                let (_, occurrence) = self
                    .scoped_assignment(record, repeat_scope, &path, occurrence)?
                    .ok_or_else(|| SemanticError::MissingOccurrence {
                        path: path.clone(),
                        occurrence,
                    })?;
                self.apply_mutations_with_values(
                    record,
                    decoded_values,
                    vec![HandlerMutation::SetIfEqual {
                        path,
                        occurrence,
                        expected,
                        value,
                    }],
                )
            }
            HandlerMutation::ReplacePayload {
                path,
                occurrence,
                data,
            } => {
                let (_, occurrence) = self
                    .scoped_assignment(record, repeat_scope, &path, occurrence)?
                    .ok_or_else(|| SemanticError::MissingOccurrence {
                        path: path.clone(),
                        occurrence,
                    })?;
                self.apply_mutations_with_values(
                    record,
                    decoded_values,
                    vec![HandlerMutation::ReplacePayload {
                        path,
                        occurrence,
                        data,
                    }],
                )
            }
            HandlerMutation::ResetToDefault { path, .. } => Err(SemanticError::Handler {
                handler: "edit.reset_sibling_default".to_owned(),
                message: format!(
                    "schema-native default mutation for {path} escaped its value container"
                ),
            }),
            HandlerMutation::Insert { path, value } => {
                self.insert_in_scope(record, decoded_values, repeat_scope, &path, &value)
            }
            HandlerMutation::Remove { path, occurrence } => {
                let (_, occurrence) = self
                    .scoped_assignment(record, repeat_scope, &path, occurrence)?
                    .ok_or_else(|| SemanticError::MissingOccurrence {
                        path: path.clone(),
                        occurrence,
                    })?;
                self.apply_mutations_with_values(
                    record,
                    decoded_values,
                    vec![HandlerMutation::Remove { path, occurrence }],
                )
            }
            HandlerMutation::RemoveAll { path } => {
                self.remove_all_in_scope(record, decoded_values, repeat_scope, &path)
            }
            HandlerMutation::SynchronizeCount {
                path,
                occurrence,
                value,
                remove_when_zero,
            } => {
                let existing = self.scoped_assignment(record, repeat_scope, &path, occurrence)?;
                if let Some((_, occurrence)) = existing {
                    self.apply_mutations_with_values(
                        record,
                        decoded_values,
                        vec![HandlerMutation::SynchronizeCount {
                            path,
                            occurrence,
                            value,
                            remove_when_zero,
                        }],
                    )
                } else if value == 0 && remove_when_zero {
                    Ok(())
                } else {
                    self.insert_in_scope(
                        record,
                        decoded_values,
                        repeat_scope,
                        &path,
                        &OwnedFieldValue::UInt(value),
                    )
                }
            }
            HandlerMutation::SynchronizePresence {
                path,
                occurrence,
                present,
                value,
            } => {
                let existing = self.scoped_assignment(record, repeat_scope, &path, occurrence)?;
                if let Some((_, occurrence)) = existing {
                    self.apply_mutations_with_values(
                        record,
                        decoded_values,
                        vec![HandlerMutation::SynchronizePresence {
                            path,
                            occurrence,
                            present,
                            value,
                        }],
                    )
                } else if present {
                    self.insert_in_scope(record, decoded_values, repeat_scope, &path, &value)
                } else {
                    Ok(())
                }
            }
        }
    }

    fn scoped_assignment(
        &self,
        record: &WritableRecord,
        repeat_scope: &RepeatScope,
        path: &str,
        local_occurrence: usize,
    ) -> Result<Option<(usize, usize)>> {
        let schema = self
            .registry
            .get(record.signature)
            .ok_or_else(|| SemanticError::MissingRecordSchema(record.signature.to_string()))?;
        let parent_path = find_containing_subrecord(&schema.root, path)
            .ok_or_else(|| SemanticError::MissingPath(path.to_owned()))?
            .path
            .as_str();
        let grammar = self.grammar_for(record)?;
        let selected = grammar
            .assignments
            .iter()
            .zip(&grammar.repeat_scopes)
            .enumerate()
            .filter(|(_, (assignment, scopes))| {
                assignment.is_some_and(|node| node.path == parent_path)
                    && scopes.contains(repeat_scope)
            })
            .nth(local_occurrence);
        let Some((index, _)) = selected else {
            return Ok(None);
        };
        let occurrence = grammar.assignments[..index]
            .iter()
            .filter(|assignment| assignment.is_some_and(|node| node.path == parent_path))
            .count();
        Ok(Some((index, occurrence)))
    }

    fn insert_in_scope(
        &self,
        record: &mut WritableRecord,
        decoded_values: &mut BTreeMap<(String, usize), FieldValue<'static>>,
        repeat_scope: &RepeatScope,
        path: &str,
        value: &OwnedFieldValue,
    ) -> Result<()> {
        let node = self.find_node(path)?;
        let SchemaNodeKind::Subrecord { payload, .. } = &node.kind else {
            return Err(SemanticError::Encode {
                path: path.to_owned(),
                message: "handler mutation path is not a subrecord".to_owned(),
            });
        };
        let (signature, encoded) = self.encode_path(path, value)?;
        let index = self.scoped_schema_insertion_index(record, node, repeat_scope)?;
        let grammar = self.grammar_for(record)?;
        let occurrence = grammar.assignments[..index]
            .iter()
            .filter(|assignment| assignment.is_some_and(|assigned| assigned.path == path))
            .count();
        record.subrecords.insert(
            index,
            WritableSubRecord {
                signature,
                data: encoded,
            },
        );
        let decoded = self.owned_to_handler_value(payload, value)?;
        insert_decoded_occurrence(decoded_values, path, occurrence, decoded);
        Ok(())
    }

    fn scoped_schema_insertion_index(
        &self,
        record: &WritableRecord,
        target: &SchemaNode,
        repeat_scope: &RepeatScope,
    ) -> Result<usize> {
        let schema = self
            .registry
            .get(record.signature)
            .ok_or_else(|| SemanticError::MissingRecordSchema(record.signature.to_string()))?;
        let scope_node = find_node_by_path(&schema.root, &repeat_scope.path)
            .ok_or_else(|| SemanticError::MissingPath(repeat_scope.path.clone()))?;
        let ordered = top_level_subrecords(scope_node);
        let target_order = ordered
            .iter()
            .position(|candidate| candidate.id == target.id)
            .ok_or_else(|| SemanticError::MissingPath(target.path.clone()))?;
        let grammar = self.grammar_for(record)?;
        let scoped_indices = grammar
            .repeat_scopes
            .iter()
            .enumerate()
            .filter_map(|(index, scopes)| scopes.contains(repeat_scope).then_some(index))
            .collect::<Vec<_>>();
        let first = scoped_indices
            .first()
            .copied()
            .ok_or_else(|| SemanticError::Encode {
                path: target.path.clone(),
                message: "repeat scope has no assigned subrecords".to_owned(),
            })?;
        let mut after_existing = None;
        for index in scoped_indices {
            let Some(assigned) = grammar.assignments[index] else {
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
        Ok(after_existing.unwrap_or_else(|| {
            grammar
                .repeat_scopes
                .iter()
                .enumerate()
                .skip(first)
                .take_while(|(_, scopes)| scopes.contains(repeat_scope))
                .map(|(index, _)| index + 1)
                .last()
                .unwrap_or(first)
        }))
    }

    fn remove_all_in_scope(
        &self,
        record: &mut WritableRecord,
        decoded_values: &mut BTreeMap<(String, usize), FieldValue<'static>>,
        repeat_scope: &RepeatScope,
        path: &str,
    ) -> Result<()> {
        loop {
            let Some((index, occurrence)) =
                self.scoped_assignment(record, repeat_scope, path, 0)?
            else {
                return Ok(());
            };
            let node = self.find_node(path)?;
            let SchemaNodeKind::Subrecord { signature, .. } = &node.kind else {
                return Err(SemanticError::Encode {
                    path: path.to_owned(),
                    message: "handler mutation path is not a subrecord".to_owned(),
                });
            };
            if record.subrecords[index].signature != Signature::from(*signature) {
                return Err(SemanticError::Encode {
                    path: path.to_owned(),
                    message: "assigned subrecord signature does not match schema".to_owned(),
                });
            }
            record.subrecords.remove(index);
            remove_decoded_occurrence(decoded_values, path, occurrence);
        }
    }

    fn apply_nested_set(
        &self,
        record: &mut WritableRecord,
        decoded_values: &mut BTreeMap<(String, usize), FieldValue<'static>>,
        path: &str,
        occurrence: usize,
        value: OwnedFieldValue,
    ) -> Result<()> {
        let schema = self
            .registry
            .get(record.signature)
            .ok_or_else(|| SemanticError::MissingRecordSchema(record.signature.to_string()))?;
        let parent = find_containing_subrecord(&schema.root, path)
            .ok_or_else(|| SemanticError::MissingPath(path.to_owned()))?
            .clone();
        let parent_occurrences = decoded_values
            .keys()
            .filter_map(|(candidate, occurrence)| {
                (candidate == &parent.path).then_some(*occurrence)
            })
            .collect::<Vec<_>>();
        let mut remaining_occurrence = occurrence;
        let mut replacement = Some(value);
        for parent_occurrence in parent_occurrences {
            let key = (parent.path.clone(), parent_occurrence);
            let current =
                decoded_values
                    .get(&key)
                    .ok_or_else(|| SemanticError::MissingOccurrence {
                        path: parent.path.clone(),
                        occurrence: parent_occurrence,
                    })?;
            let mut updated = handler_to_owned_value(current.to_handler_value(), &parent.path)?;
            if !self.set_nested_value(
                &parent,
                &mut updated,
                path,
                &mut remaining_occurrence,
                &mut replacement,
            )? {
                continue;
            }
            let (signature, encoded) = self.encode_path(&parent.path, &updated)?;
            let index = self.assigned_subrecord_index(record, &parent.path, parent_occurrence)?;
            if record.subrecords[index].signature != signature {
                return Err(SemanticError::Encode {
                    path: parent.path,
                    message: "assigned subrecord signature does not match schema".to_owned(),
                });
            }
            record.subrecords[index].data = encoded;
            let decoded = self.owned_to_handler_value(&parent, &updated)?;
            decoded_values.insert(key, decoded);
            return Ok(());
        }
        Err(SemanticError::MissingOccurrence {
            path: path.to_owned(),
            occurrence,
        })
    }

    fn apply_nested_set_if_equal(
        &self,
        record: &mut WritableRecord,
        decoded_values: &mut BTreeMap<(String, usize), FieldValue<'static>>,
        path: &str,
        occurrence: usize,
        expected: &OwnedFieldValue,
        value: OwnedFieldValue,
    ) -> Result<()> {
        let schema = self
            .registry
            .get(record.signature)
            .ok_or_else(|| SemanticError::MissingRecordSchema(record.signature.to_string()))?;
        let parent = find_containing_subrecord(&schema.root, path)
            .ok_or_else(|| SemanticError::MissingPath(path.to_owned()))?
            .clone();
        let parent_occurrences = decoded_values
            .keys()
            .filter_map(|(candidate, occurrence)| {
                (candidate == &parent.path).then_some(*occurrence)
            })
            .collect::<Vec<_>>();
        let mut remaining_occurrence = occurrence;
        for parent_occurrence in parent_occurrences {
            let key = (parent.path.clone(), parent_occurrence);
            let current =
                decoded_values
                    .get(&key)
                    .ok_or_else(|| SemanticError::MissingOccurrence {
                        path: parent.path.clone(),
                        occurrence: parent_occurrence,
                    })?;
            let mut updated = handler_to_owned_value(current.to_handler_value(), &parent.path)?;
            let local_occurrence = remaining_occurrence;
            let mut probed_occurrence = remaining_occurrence;
            let Some(current) =
                self.nested_value_at(&parent, &updated, path, &mut probed_occurrence)?
            else {
                remaining_occurrence = probed_occurrence;
                continue;
            };
            if !owned_values_equal(current, expected) {
                return Ok(());
            }
            let mut target_occurrence = local_occurrence;
            let mut replacement = Some(value);
            if !self.set_nested_value(
                &parent,
                &mut updated,
                path,
                &mut target_occurrence,
                &mut replacement,
            )? {
                return Err(SemanticError::MissingOccurrence {
                    path: path.to_owned(),
                    occurrence,
                });
            }
            let (signature, encoded) = self.encode_path(&parent.path, &updated)?;
            let index = self.assigned_subrecord_index(record, &parent.path, parent_occurrence)?;
            if record.subrecords[index].signature != signature {
                return Err(SemanticError::Encode {
                    path: parent.path,
                    message: "assigned subrecord signature does not match schema".to_owned(),
                });
            }
            record.subrecords[index].data = encoded;
            let decoded = self.owned_to_handler_value(&parent, &updated)?;
            decoded_values.insert(key, decoded);
            return Ok(());
        }
        Err(SemanticError::MissingOccurrence {
            path: path.to_owned(),
            occurrence,
        })
    }

    fn changed_field_at(&self, record: &WritableRecord, index: usize) -> Result<ChangedField> {
        let grammar = self.grammar_for(record)?;
        let path = grammar
            .assignments
            .get(index)
            .and_then(|assignment| *assignment)
            .map(|node| node.path.clone())
            .ok_or_else(|| SemanticError::Encode {
                path: record.signature.to_string(),
                message: format!("edited subrecord at index {index} has no grammar assignment"),
            })?;
        let repeat_scopes =
            grammar
                .repeat_scopes
                .get(index)
                .cloned()
                .ok_or_else(|| SemanticError::Encode {
                    path: path.clone(),
                    message: format!("edited subrecord at index {index} has no repeat scope"),
                })?;
        Ok(ChangedField {
            path,
            repeat_scopes,
        })
    }

    fn apply_after_set_callbacks(
        &self,
        record: &mut WritableRecord,
        decoded_values: &mut BTreeMap<(String, usize), FieldValue<'static>>,
        changed: &ChangedField,
    ) -> Result<()> {
        let record_path = record.signature.to_string();
        for binding in self
            .registry
            .package()
            .callback_bindings()
            .iter()
            .filter(|binding| {
                binding.callback_id == "def.after_set"
                    && (binding.path == record_path
                        || changed.path == binding.path
                        || changed
                            .path
                            .strip_prefix(&binding.path)
                            .is_some_and(|suffix| suffix.starts_with('/')))
            })
        {
            if !matches!(
                binding.implementation,
                CallbackImplementation::BuiltIn { .. }
                    | CallbackImplementation::CustomHandler { .. }
            ) {
                return Err(SemanticError::Handler {
                    handler: binding.callback_id.clone(),
                    message: "record-level after-set callback is not executable".to_owned(),
                });
            }
            let repeat_scope = changed
                .repeat_scopes
                .iter()
                .filter(|scope| {
                    binding.path == scope.path
                        || binding
                            .path
                            .strip_prefix(&scope.path)
                            .is_some_and(|suffix| suffix.starts_with('/'))
                })
                .max_by_key(|scope| scope.path.len());
            let source_record = self.scoped_record(record, repeat_scope)?;
            match self.handlers.invoke_with_writable_record(
                binding,
                self.handler_record(),
                &source_record,
                HandlerPhase::AfterSet,
                None,
                None,
            )? {
                HandlerOutput::None => {}
                HandlerOutput::Mutations(handler_mutations) => {
                    let mutations =
                        self.globalize_mutations(record, repeat_scope, handler_mutations)?;
                    self.apply_mutations_with_values(record, decoded_values, mutations)?;
                }
                _ => {
                    return Err(SemanticError::Handler {
                        handler: binding.callback_id.clone(),
                        message: "record-level after-set callback returned an invalid result"
                            .to_owned(),
                    });
                }
            }
        }
        Ok(())
    }

    fn scoped_record(
        &self,
        record: &WritableRecord,
        repeat_scope: Option<&RepeatScope>,
    ) -> Result<WritableRecord> {
        let Some(repeat_scope) = repeat_scope else {
            return Ok(clone_record(record));
        };
        let grammar = self.grammar_for(record)?;
        let subrecords = record
            .subrecords
            .iter()
            .zip(&grammar.repeat_scopes)
            .filter(|(_, scopes)| scopes.contains(repeat_scope))
            .map(|(subrecord, _)| WritableSubRecord {
                signature: subrecord.signature,
                data: subrecord.data.clone(),
            })
            .collect();
        Ok(WritableRecord {
            signature: record.signature,
            flags: record.flags,
            form_id: record.form_id,
            form_version: record.form_version,
            subrecords,
        })
    }

    fn globalize_mutations(
        &self,
        record: &WritableRecord,
        repeat_scope: Option<&RepeatScope>,
        mutations: Vec<HandlerMutation>,
    ) -> Result<Vec<HandlerMutation>> {
        let Some(repeat_scope) = repeat_scope else {
            return Ok(mutations);
        };
        let grammar = self.grammar_for(record)?;
        mutations
            .into_iter()
            .map(|mutation| {
                Ok(match mutation {
                    HandlerMutation::Set {
                        path,
                        occurrence,
                        value,
                    } => HandlerMutation::Set {
                        occurrence: self.global_occurrence(
                            &grammar,
                            repeat_scope,
                            &path,
                            occurrence,
                        )?,
                        path,
                        value,
                    },
                    HandlerMutation::SetIfEqual {
                        path,
                        occurrence,
                        expected,
                        value,
                    } => HandlerMutation::SetIfEqual {
                        occurrence: self.global_occurrence(
                            &grammar,
                            repeat_scope,
                            &path,
                            occurrence,
                        )?,
                        path,
                        expected,
                        value,
                    },
                    HandlerMutation::ReplacePayload {
                        path,
                        occurrence,
                        data,
                    } => HandlerMutation::ReplacePayload {
                        occurrence: self.global_occurrence(
                            &grammar,
                            repeat_scope,
                            &path,
                            occurrence,
                        )?,
                        path,
                        data,
                    },
                    HandlerMutation::ResetToDefault { path, .. } => {
                        return Err(SemanticError::Handler {
                            handler: "edit.reset_sibling_default".to_owned(),
                            message: format!(
                                "schema-native default mutation for {path} escaped its value \
                                 container"
                            ),
                        });
                    }
                    HandlerMutation::Remove { path, occurrence } => HandlerMutation::Remove {
                        occurrence: self.global_occurrence(
                            &grammar,
                            repeat_scope,
                            &path,
                            occurrence,
                        )?,
                        path,
                    },
                    HandlerMutation::RemoveAll { .. } => {
                        return Err(SemanticError::Handler {
                            handler: "def.after_set".to_owned(),
                            message: "scoped callbacks cannot remove all subrecords".to_owned(),
                        });
                    }
                    HandlerMutation::SynchronizeCount {
                        path,
                        occurrence,
                        value,
                        remove_when_zero,
                    } => HandlerMutation::SynchronizeCount {
                        occurrence: self.global_occurrence(
                            &grammar,
                            repeat_scope,
                            &path,
                            occurrence,
                        )?,
                        path,
                        value,
                        remove_when_zero,
                    },
                    HandlerMutation::SynchronizePresence {
                        path,
                        occurrence,
                        present,
                        value,
                    } => HandlerMutation::SynchronizePresence {
                        occurrence: self.global_occurrence(
                            &grammar,
                            repeat_scope,
                            &path,
                            occurrence,
                        )?,
                        path,
                        present,
                        value,
                    },
                    HandlerMutation::Insert { .. } => {
                        return Err(SemanticError::Handler {
                            handler: "def.after_set".to_owned(),
                            message: "scoped callbacks cannot insert absent subrecords".to_owned(),
                        });
                    }
                })
            })
            .collect()
    }

    fn global_occurrence(
        &self,
        grammar: &crate::grammar::GrammarMatch<'_>,
        repeat_scope: &RepeatScope,
        path: &str,
        local_occurrence: usize,
    ) -> Result<usize> {
        let schema = self
            .registry
            .get(self.record.signature)
            .ok_or_else(|| SemanticError::MissingRecordSchema(self.record.signature.to_string()))?;
        let parent_path = find_containing_subrecord(&schema.root, path)
            .ok_or_else(|| SemanticError::MissingPath(path.to_owned()))?
            .path
            .as_str();
        let selected_index = grammar
            .assignments
            .iter()
            .zip(&grammar.repeat_scopes)
            .enumerate()
            .filter(|(_, (assignment, scopes))| {
                assignment.is_some_and(|node| node.path == parent_path)
                    && scopes.contains(repeat_scope)
            })
            .nth(local_occurrence)
            .map(|(index, _)| index)
            .ok_or_else(|| SemanticError::MissingOccurrence {
                path: path.to_owned(),
                occurrence: local_occurrence,
            })?;
        Ok(grammar.assignments[..selected_index]
            .iter()
            .filter(|assignment| assignment.is_some_and(|node| node.path == parent_path))
            .count())
    }

    fn apply_record_metadata_mutations(&mut self, mutations: Vec<HandlerMutation>) -> Result<()> {
        let [HandlerMutation::Set {
            path,
            occurrence,
            value,
        }] = mutations.as_slice()
        else {
            return Err(SemanticError::Handler {
                handler: "record.set_editor_id".to_owned(),
                message: "record editor-ID callback must return one set mutation".to_owned(),
            });
        };
        let schema = self
            .registry
            .get(self.record.signature)
            .ok_or_else(|| SemanticError::MissingRecordSchema(self.record.signature.to_string()))?;
        let parent = find_containing_subrecord(&schema.root, path)
            .ok_or_else(|| SemanticError::MissingPath(path.clone()))?
            .clone();
        let parent_occurrences: Vec<usize> = self
            .decoded_values
            .keys()
            .filter_map(|(candidate, occurrence)| {
                (candidate == &parent.path).then_some(*occurrence)
            })
            .collect();
        let [parent_occurrence] = parent_occurrences.as_slice() else {
            return Err(SemanticError::Handler {
                handler: "record.set_editor_id".to_owned(),
                message: format!(
                    "record metadata field {} must have exactly one containing subrecord",
                    path
                ),
            });
        };
        let current = self
            .decoded_values
            .get(&(parent.path.clone(), *parent_occurrence))
            .ok_or_else(|| SemanticError::MissingOccurrence {
                path: parent.path.clone(),
                occurrence: *parent_occurrence,
            })?;
        let mut updated = handler_to_owned_value(current.to_handler_value(), &parent.path)?;
        let mut target_occurrence = *occurrence;
        let mut replacement = Some(value.clone());
        if !self.set_nested_value(
            &parent,
            &mut updated,
            path,
            &mut target_occurrence,
            &mut replacement,
        )? {
            return Err(SemanticError::MissingOccurrence {
                path: path.clone(),
                occurrence: *occurrence,
            });
        }
        let (signature, encoded) = self.encode_path(&parent.path, &updated)?;
        let mut candidate = clone_record(&self.record);
        let index = self.assigned_subrecord_index(&candidate, &parent.path, *parent_occurrence)?;
        if candidate.subrecords[index].signature != signature {
            return Err(SemanticError::Encode {
                path: parent.path,
                message: "assigned subrecord signature does not match schema".to_owned(),
            });
        }
        candidate.subrecords[index].data = encoded;
        let decoded = self.owned_to_handler_value(&parent, &updated)?;
        self.record = candidate;
        self.decoded_values
            .insert((parent.path, *parent_occurrence), decoded);
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

fn remove_decoded_occurrence(
    decoded_values: &mut BTreeMap<(String, usize), FieldValue<'static>>,
    path: &str,
    occurrence: usize,
) {
    decoded_values.remove(&(path.to_owned(), occurrence));
    let shifted = decoded_values
        .keys()
        .filter(|(candidate, index)| candidate == path && *index > occurrence)
        .cloned()
        .collect::<Vec<_>>();
    for key in shifted {
        if let Some(value) = decoded_values.remove(&key) {
            decoded_values.insert((key.0, key.1.saturating_sub(1)), value);
        }
    }
}

fn insert_decoded_occurrence(
    decoded_values: &mut BTreeMap<(String, usize), FieldValue<'static>>,
    path: &str,
    occurrence: usize,
    value: FieldValue<'static>,
) {
    let mut shifted = decoded_values
        .keys()
        .filter(|(candidate, index)| candidate == path && *index >= occurrence)
        .cloned()
        .collect::<Vec<_>>();
    shifted.sort_by_key(|key| std::cmp::Reverse(key.1));
    for key in shifted {
        if let Some(value) = decoded_values.remove(&key) {
            decoded_values.insert((key.0, key.1.saturating_add(1)), value);
        }
    }
    decoded_values.insert((path.to_owned(), occurrence), value);
}

fn mutation_path(mutation: &HandlerMutation) -> &str {
    match mutation {
        HandlerMutation::Set { path, .. }
        | HandlerMutation::SetIfEqual { path, .. }
        | HandlerMutation::ReplacePayload { path, .. }
        | HandlerMutation::ResetToDefault { path, .. }
        | HandlerMutation::Insert { path, .. }
        | HandlerMutation::Remove { path, .. }
        | HandlerMutation::RemoveAll { path }
        | HandlerMutation::SynchronizeCount { path, .. }
        | HandlerMutation::SynchronizePresence { path, .. } => path,
    }
}

fn path_is_within(path: &str, parent: &str) -> bool {
    path == parent
        || path
            .strip_prefix(parent)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

fn collect_expression_field_values(
    path: &str,
    value: &FieldValue<'_>,
    output: &mut BTreeMap<String, i64>,
) {
    match value {
        FieldValue::Int(value) => {
            output.insert(path.to_owned(), *value);
        }
        FieldValue::UInt(value) | FieldValue::Flags { value, .. } => {
            if let Ok(value) = i64::try_from(*value) {
                output.insert(path.to_owned(), value);
            }
        }
        FieldValue::Enumeration { value, .. } => {
            output.insert(path.to_owned(), *value);
        }
        FieldValue::Struct(values) => {
            for value in values {
                collect_expression_field_values(&value.path, &value.value, output);
            }
        }
        FieldValue::Array(values) => {
            for value in values {
                collect_expression_field_values(path, value, output);
            }
        }
        _ => {}
    }
}

fn collect_owned_expression_field_values(
    node: &SchemaNode,
    value: &OwnedFieldValue,
    output: &mut BTreeMap<String, i64>,
) {
    match (&node.kind, value) {
        (SchemaNodeKind::Primitive { .. }, OwnedFieldValue::Int(value)) => {
            output.insert(node.path.clone(), *value);
        }
        (SchemaNodeKind::Primitive { .. }, OwnedFieldValue::UInt(value)) => {
            if let Ok(value) = i64::try_from(*value) {
                output.insert(node.path.clone(), value);
            }
        }
        (SchemaNodeKind::Primitive { .. }, OwnedFieldValue::FormId(value)) => {
            output.insert(node.path.clone(), i64::from(value.0));
        }
        (SchemaNodeKind::Struct { fields }, OwnedFieldValue::Struct(values))
            if fields.len() == values.len() =>
        {
            for (field, value) in fields.iter().zip(values) {
                collect_owned_expression_field_values(field, value, output);
            }
        }
        (SchemaNodeKind::Array { element, .. }, OwnedFieldValue::Array(values)) => {
            for value in values {
                collect_owned_expression_field_values(element, value, output);
            }
        }
        (SchemaNodeKind::Union { variants, .. }, _) => {
            for variant in variants {
                collect_owned_expression_field_values(variant, value, output);
            }
        }
        (
            SchemaNodeKind::Subrecord { payload, .. }
            | SchemaNodeKind::Compressed { child: payload, .. }
            | SchemaNodeKind::Terminated { child: payload, .. },
            _,
        ) => collect_owned_expression_field_values(payload, value, output),
        _ => {}
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

fn owned_values_equal(left: &OwnedFieldValue, right: &OwnedFieldValue) -> bool {
    match (left, right) {
        (OwnedFieldValue::Int(left), OwnedFieldValue::UInt(right)) => {
            u64::try_from(*left).is_ok_and(|left| left == *right)
        }
        (OwnedFieldValue::UInt(left), OwnedFieldValue::Int(right)) => {
            u64::try_from(*right).is_ok_and(|right| *left == right)
        }
        _ => left == right,
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
            SchemaNodeKind::Choice { alternatives }
            | SchemaNodeKind::SelectedChoice { alternatives, .. } => {
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
        SchemaNodeKind::Choice { alternatives }
        | SchemaNodeKind::SelectedChoice { alternatives, .. } => alternatives.iter().collect(),
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

fn find_containing_subrecord<'a>(node: &'a SchemaNode, path: &str) -> Option<&'a SchemaNode> {
    fn find<'a>(
        node: &'a SchemaNode,
        path: &str,
        parent: Option<&'a SchemaNode>,
    ) -> Option<&'a SchemaNode> {
        let parent = if matches!(node.kind, SchemaNodeKind::Subrecord { .. }) {
            Some(node)
        } else {
            parent
        };
        if node.path == path {
            return parent;
        }
        let children: Vec<&SchemaNode> = match &node.kind {
            SchemaNodeKind::Sequence { children } => children.iter().collect(),
            SchemaNodeKind::Choice { alternatives }
            | SchemaNodeKind::SelectedChoice { alternatives, .. } => alternatives.iter().collect(),
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
            .find_map(|child| find(child, path, parent))
    }

    find(node, path, None)
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
        BuiltInOperation, CallbackBinding, CallbackImplementation, ConflictPriority, Expression,
        HandlerRequirement, SchemaGame, SchemaManifest, SchemaNodeId, SchemaPackage, SchemaRecord,
        SchemaSignature, StringLengthPrefix, StringType, ValidationStatus, PACKAGE_FORMAT_VERSION,
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
                assert!(invocation.source_writable_record.is_some());
                assert_eq!(invocation.source_subrecord_index, Some(0));
                Ok(HandlerOutput::Integer(1))
            } else {
                Ok(HandlerOutput::None)
            }
        }
    }

    struct TestMagicEffectResolver;

    impl crate::FormLinkResolver for TestMagicEffectResolver {
        fn resolve_form_id(
            &self,
            _source: HandlerRecordContext,
            form_id: bethkit_core::FormId,
            targets: &[Signature],
        ) -> Option<crate::FormLinkInfo> {
            (form_id == bethkit_core::FormId(0x6789) && targets == [Signature(*b"MGEF")]).then(
                || {
                    crate::FormLinkInfo::new(
                        "Example Effect [MGEF:00006789]",
                        "Example Effect [MGEF:00006789]",
                    )
                    .with_signature(Signature(*b"MGEF"))
                    .with_magic_effect_actor_value(48)
                },
            )
        }

        fn resolve_magic_effect_code(
            &self,
            _source: HandlerRecordContext,
            code: u32,
        ) -> Option<crate::FormLinkInfo> {
            (code == u32::from_le_bytes(*b"ABCD")).then(|| {
                crate::FormLinkInfo::new(
                    "Example Effect [MGEF:00006789]",
                    "Example Effect [MGEF:00006789]",
                )
                .with_signature(Signature(*b"MGEF"))
                .with_magic_effect_metadata(0x0100_0000, 42)
            })
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

    /// Applies parsed CTDA values and sibling string mutations atomically.
    #[test]
    fn parsed_nested_edit_updates_ctda_string_subrecord(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let ctda_path = "TEST/0:CTDA";
        let parameter_path = "TEST/0:CTDA/payload/5:Parameter #1/variants/2:String";
        let string_path = "TEST/1:CIS1";
        let parameter = SchemaNode {
            id: SchemaNodeId(2),
            path: parameter_path.to_owned(),
            name: "String".to_owned(),
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
        };
        let package = SchemaPackage::new_with_callbacks(
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
                            SchemaNode {
                                id: SchemaNodeId(1),
                                path: ctda_path.to_owned(),
                                name: "CTDA".to_owned(),
                                required: true,
                                conflict_priority: ConflictPriority::Normal,
                                condition: None,
                                kind: SchemaNodeKind::Subrecord {
                                    signature: SchemaSignature(*b"CTDA"),
                                    payload: Box::new(parameter.clone()),
                                },
                            },
                            SchemaNode {
                                id: SchemaNodeId(3),
                                path: string_path.to_owned(),
                                name: "Parameter #1".to_owned(),
                                required: false,
                                conflict_priority: ConflictPriority::Normal,
                                condition: None,
                                kind: SchemaNodeKind::Subrecord {
                                    signature: SchemaSignature(*b"CIS1"),
                                    payload: Box::new(SchemaNode {
                                        id: SchemaNodeId(4),
                                        path: format!("{string_path}/payload"),
                                        name: "Parameter #1".to_owned(),
                                        required: true,
                                        conflict_priority: ConflictPriority::Normal,
                                        condition: None,
                                        kind: SchemaNodeKind::Primitive {
                                            primitive: PrimitiveType::String {
                                                string: StringType {
                                                    encoding: "utf8".to_owned(),
                                                    localized: false,
                                                    zero_terminated: true,
                                                    fixed_length: None,
                                                    length_prefix: None,
                                                    trailing_terminator: None,
                                                    allowed_values: Vec::new(),
                                                },
                                            },
                                        },
                                    }),
                                },
                            },
                        ],
                    },
                },
            }],
            Vec::new(),
        )?;
        let mut editor = RecordEditor {
            registry: bethkit_schema::SchemaRegistry::new(Arc::new(package)),
            decoders: crate::DecoderRegistry::builtin(),
            handlers: SemanticHandlerRegistry::builtin(),
            record: WritableRecord {
                signature: Signature(*b"TEST"),
                flags: bethkit_core::RecordFlags::empty(),
                form_id: bethkit_core::FormId::NULL,
                form_version: 44,
                subrecords: vec![WritableSubRecord {
                    signature: Signature(*b"CTDA"),
                    data: 5_u32.to_le_bytes().to_vec(),
                }],
            },
            localized: false,
            after_load_migrations: 0,
            decoded_values: BTreeMap::from([((ctda_path.to_owned(), 0), FieldValue::UInt(5))]),
        };
        let parsed = ParsedEditValue::new(
            OwnedFieldValue::UInt(0),
            vec![HandlerMutation::SynchronizePresence {
                path: string_path.to_owned(),
                occurrence: 0,
                present: true,
                value: OwnedFieldValue::String("Updated".to_owned()),
            }],
        );

        // when
        editor.set_parsed_value(parameter_path, 0, &parsed)?;

        // then
        assert_eq!(editor.record.subrecords.len(), 2);
        assert_eq!(editor.record.subrecords[0].data, 0_u32.to_le_bytes());
        assert_eq!(editor.record.subrecords[1].signature, Signature(*b"CIS1"));
        assert_eq!(editor.record.subrecords[1].data, b"Updated\0");
        Ok(())
    }

    /// Resets a VMAD value to the newly selected property's schema-native default.
    #[test]
    fn property_type_change_resets_sibling_union_to_default(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let subrecord_path = "TEST/0:VMAD";
        let payload_path = "TEST/0:VMAD/payload";
        let type_path = "TEST/0:VMAD/payload/0:Type";
        let value_path = "TEST/0:VMAD/payload/1:Value";
        let string_type = PrimitiveType::String {
            string: StringType {
                encoding: "utf8".to_owned(),
                localized: false,
                zero_terminated: false,
                fixed_length: None,
                length_prefix: Some(StringLengthPrefix {
                    width: 2,
                    offset: 2,
                }),
                trailing_terminator: None,
                allowed_values: Vec::new(),
            },
        };
        let payload = SchemaNode {
            id: SchemaNodeId(2),
            path: payload_path.to_owned(),
            name: "Property".to_owned(),
            required: true,
            conflict_priority: ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Struct {
                fields: vec![
                    SchemaNode {
                        id: SchemaNodeId(3),
                        path: type_path.to_owned(),
                        name: "Type".to_owned(),
                        required: true,
                        conflict_priority: ConflictPriority::Normal,
                        condition: None,
                        kind: SchemaNodeKind::Primitive {
                            primitive: PrimitiveType::Enumeration {
                                integer: IntegerType {
                                    width: 1,
                                    signed: false,
                                    byte_order: ByteOrder::LittleEndian,
                                },
                                values: vec![
                                    (0, "None".to_owned()),
                                    (1, "Int32".to_owned()),
                                    (2, "String".to_owned()),
                                ],
                            },
                        },
                    },
                    SchemaNode {
                        id: SchemaNodeId(4),
                        path: value_path.to_owned(),
                        name: "Value".to_owned(),
                        required: true,
                        conflict_priority: ConflictPriority::Normal,
                        condition: None,
                        kind: SchemaNodeKind::Union {
                            selector: UnionSelector::Expression(Expression::ReadField {
                                path: type_path.to_owned(),
                            }),
                            variants: vec![
                                SchemaNode {
                                    id: SchemaNodeId(5),
                                    path: format!("{value_path}/variants/0:None"),
                                    name: "None".to_owned(),
                                    required: true,
                                    conflict_priority: ConflictPriority::Normal,
                                    condition: None,
                                    kind: SchemaNodeKind::Primitive {
                                        primitive: PrimitiveType::Bytes { length: Some(0) },
                                    },
                                },
                                SchemaNode {
                                    id: SchemaNodeId(6),
                                    path: format!("{value_path}/variants/1:Int32"),
                                    name: "Int32".to_owned(),
                                    required: true,
                                    conflict_priority: ConflictPriority::Normal,
                                    condition: None,
                                    kind: SchemaNodeKind::Primitive {
                                        primitive: PrimitiveType::Integer {
                                            integer: IntegerType {
                                                width: 4,
                                                signed: true,
                                                byte_order: ByteOrder::LittleEndian,
                                            },
                                        },
                                    },
                                },
                                SchemaNode {
                                    id: SchemaNodeId(7),
                                    path: format!("{value_path}/variants/2:String"),
                                    name: "String".to_owned(),
                                    required: true,
                                    conflict_priority: ConflictPriority::Normal,
                                    condition: None,
                                    kind: SchemaNodeKind::Primitive {
                                        primitive: string_type,
                                    },
                                },
                            ],
                        },
                    },
                ],
            },
        };
        let mut manifest = test_manifest();
        manifest.callbacks_total = 1;
        manifest.callbacks_classified = 1;
        manifest.required_handlers = vec![HandlerRequirement {
            id: "edit.reset_sibling_default".to_owned(),
            minimum_version: 1,
        }];
        let package = SchemaPackage::new_with_callbacks(
            manifest,
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
                        children: vec![SchemaNode {
                            id: SchemaNodeId(1),
                            path: subrecord_path.to_owned(),
                            name: "VMAD".to_owned(),
                            required: true,
                            conflict_priority: ConflictPriority::Normal,
                            condition: None,
                            kind: SchemaNodeKind::Subrecord {
                                signature: SchemaSignature(*b"VMAD"),
                                payload: Box::new(payload.clone()),
                            },
                        }],
                    },
                },
            }],
            vec![CallbackBinding {
                path: type_path.to_owned(),
                callback_id: "def.after_set".to_owned(),
                callback_slot: None,
                implementation_fingerprint: "44".repeat(32),
                implementation: CallbackImplementation::BuiltIn {
                    operation: BuiltInOperation {
                        id: "edit.reset_sibling_default".to_owned(),
                        minimum_version: 1,
                        configuration: serde_json::json!({
                            "target_path": value_path,
                        }),
                    },
                },
            }],
        )?;
        let mut editor = RecordEditor {
            registry: bethkit_schema::SchemaRegistry::new(Arc::new(package)),
            decoders: crate::DecoderRegistry::builtin(),
            handlers: SemanticHandlerRegistry::builtin(),
            record: WritableRecord {
                signature: Signature(*b"TEST"),
                flags: bethkit_core::RecordFlags::empty(),
                form_id: bethkit_core::FormId::NULL,
                form_version: 44,
                subrecords: vec![WritableSubRecord {
                    signature: Signature(*b"VMAD"),
                    data: vec![1, 42, 0, 0, 0],
                }],
            },
            localized: false,
            after_load_migrations: 0,
            decoded_values: BTreeMap::new(),
        };
        let decoded = editor.owned_to_handler_value(
            &payload,
            &OwnedFieldValue::Struct(vec![OwnedFieldValue::Int(1), OwnedFieldValue::Int(42)]),
        )?;
        editor
            .decoded_values
            .insert((subrecord_path.to_owned(), 0), decoded);

        // when
        editor.set(
            subrecord_path,
            0,
            &OwnedFieldValue::Struct(vec![OwnedFieldValue::Int(2), OwnedFieldValue::Int(42)]),
        )?;

        // then
        assert_eq!(editor.record.subrecords[0].data, vec![2, 0, 0]);

        // when
        editor.set(
            subrecord_path,
            0,
            &OwnedFieldValue::Struct(vec![
                OwnedFieldValue::Int(2),
                OwnedFieldValue::String("kept".to_owned()),
            ]),
        )?;

        // then
        assert_eq!(editor.record.subrecords[0].data, b"\x02\x04\x00kept");
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

    /// Resets a nested LGDI star slot using its outer array position.
    #[test]
    fn default_callback_receives_nested_array_indices() -> Result<()> {
        let star_path = "LGDI/0:Data/payload/element/element/0:Star Slot";
        let star_slot = SchemaNode {
            id: SchemaNodeId(5),
            path: star_path.to_owned(),
            name: "Star Slot".to_owned(),
            required: true,
            conflict_priority: ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Primitive {
                primitive: PrimitiveType::Enumeration {
                    integer: IntegerType {
                        width: 4,
                        signed: false,
                        byte_order: ByteOrder::LittleEndian,
                    },
                    values: (0_i64..5)
                        .map(|value| (value, format!("Slot {value}")))
                        .collect(),
                },
            },
        };
        let entry = SchemaNode {
            id: SchemaNodeId(4),
            path: "LGDI/0:Data/payload/element/element".to_owned(),
            name: "Entry".to_owned(),
            required: true,
            conflict_priority: ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Struct {
                fields: vec![
                    star_slot,
                    SchemaNode {
                        id: SchemaNodeId(6),
                        path: "LGDI/0:Data/payload/element/element/1:Value".to_owned(),
                        name: "Value".to_owned(),
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
                    },
                ],
            },
        };
        let inner = SchemaNode {
            id: SchemaNodeId(3),
            path: "LGDI/0:Data/payload/element".to_owned(),
            name: "Slot".to_owned(),
            required: true,
            conflict_priority: ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Array {
                element: Box::new(entry),
                count: ArrayCount::Remainder,
            },
        };
        let outer = SchemaNode {
            id: SchemaNodeId(2),
            path: "LGDI/0:Data/payload".to_owned(),
            name: "Slots".to_owned(),
            required: true,
            conflict_priority: ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Array {
                element: Box::new(inner),
                count: ArrayCount::Fixed { count: 5 },
            },
        };
        let root = SchemaNode {
            id: SchemaNodeId(0),
            path: "LGDI".to_owned(),
            name: "Leveled Item".to_owned(),
            required: true,
            conflict_priority: ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Sequence {
                children: vec![SchemaNode {
                    id: SchemaNodeId(1),
                    path: "LGDI/0:Data".to_owned(),
                    name: "Data".to_owned(),
                    required: true,
                    conflict_priority: ConflictPriority::Normal,
                    condition: None,
                    kind: SchemaNodeKind::Subrecord {
                        signature: SchemaSignature(*b"DATA"),
                        payload: Box::new(outer.clone()),
                    },
                }],
            },
        };
        let mut manifest = test_manifest();
        manifest.game = SchemaGame::Starfield;
        manifest.callbacks_total = 1;
        manifest.callbacks_classified = 1;
        manifest.required_handlers = vec![HandlerRequirement {
            id: "default.star_slot_outer_index".to_owned(),
            minimum_version: 1,
        }];
        let package = SchemaPackage::new_with_callbacks(
            manifest,
            vec![SchemaRecord {
                signature: SchemaSignature(*b"LGDI"),
                name: "Leveled Item".to_owned(),
                root,
            }],
            vec![CallbackBinding {
                path: star_path.to_owned(),
                callback_id: "value.set_default".to_owned(),
                callback_slot: None,
                implementation_fingerprint: "00".repeat(32),
                implementation: CallbackImplementation::BuiltIn {
                    operation: BuiltInOperation {
                        id: "default.star_slot_outer_index".to_owned(),
                        minimum_version: 1,
                        configuration: serde_json::Value::Null,
                    },
                },
            }],
        )?;
        let editor = RecordEditor {
            registry: bethkit_schema::SchemaRegistry::new(Arc::new(package)),
            decoders: crate::DecoderRegistry::builtin(),
            handlers: SemanticHandlerRegistry::builtin(),
            record: WritableRecord {
                signature: Signature(*b"LGDI"),
                flags: bethkit_core::RecordFlags::empty(),
                form_id: bethkit_core::FormId::NULL,
                form_version: 0,
                subrecords: Vec::new(),
            },
            localized: false,
            after_load_migrations: 0,
            decoded_values: BTreeMap::new(),
        };
        let mut groups = vec![OwnedFieldValue::Array(Vec::new()); 5];
        groups[3] = OwnedFieldValue::Array(vec![OwnedFieldValue::Struct(vec![
            OwnedFieldValue::Int(0),
            OwnedFieldValue::UInt(99),
        ])]);
        let mut value = OwnedFieldValue::Array(groups);
        let mut occurrence = 0;

        let reset = editor.reset_nested_value_with_fields(
            &outer,
            &mut value,
            star_path,
            &mut occurrence,
            &BTreeMap::new(),
            &[],
        )?;

        assert!(reset);
        let OwnedFieldValue::Array(groups) = value else {
            return Err(encode_error(star_path, "expected outer array"));
        };
        let OwnedFieldValue::Array(entries) = &groups[3] else {
            return Err(encode_error(star_path, "expected slot group"));
        };
        let OwnedFieldValue::Struct(fields) = &entries[0] else {
            return Err(encode_error(star_path, "expected slot entry"));
        };
        assert_eq!(fields[0], OwnedFieldValue::Int(3));
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
            after_load_migrations: 0,
            decoded_values: BTreeMap::new(),
        };

        assert_eq!(
            editor.encode_node_at(&union, &OwnedFieldValue::UInt(7), Some(0))?,
            vec![7, 0]
        );
        Ok(())
    }

    /// Supplies updated sibling strings while encoding callback-selected unions.
    #[test]
    fn callback_union_encoding_receives_sibling_scope() -> Result<()> {
        let script_name_path = "TEST/data/0:ScriptName";
        let union_path = "TEST/data/1:Script";
        let bytes = |id, length| SchemaNode {
            id: SchemaNodeId(id),
            path: format!("{union_path}/variants/{id}"),
            name: format!("Variant {id}"),
            required: true,
            conflict_priority: ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Primitive {
                primitive: PrimitiveType::Bytes {
                    length: Some(length),
                },
            },
        };
        let node = SchemaNode {
            id: SchemaNodeId(30),
            path: "TEST/data".to_owned(),
            name: "Data".to_owned(),
            required: true,
            conflict_priority: ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Struct {
                fields: vec![
                    SchemaNode {
                        id: SchemaNodeId(31),
                        path: script_name_path.to_owned(),
                        name: "ScriptName".to_owned(),
                        required: true,
                        conflict_priority: ConflictPriority::Normal,
                        condition: None,
                        kind: SchemaNodeKind::Primitive {
                            primitive: PrimitiveType::String {
                                string: StringType {
                                    encoding: "utf8".to_owned(),
                                    localized: false,
                                    zero_terminated: false,
                                    fixed_length: None,
                                    length_prefix: Some(StringLengthPrefix {
                                        width: 1,
                                        offset: 1,
                                    }),
                                    trailing_terminator: None,
                                    allowed_values: Vec::new(),
                                },
                            },
                        },
                    },
                    SchemaNode {
                        id: SchemaNodeId(32),
                        path: union_path.to_owned(),
                        name: "Script".to_owned(),
                        required: true,
                        conflict_priority: ConflictPriority::Normal,
                        condition: None,
                        kind: SchemaNodeKind::Union {
                            selector: UnionSelector::Callback {
                                callback_id: "union.select".to_owned(),
                            },
                            variants: vec![bytes(33, 1), bytes(34, 0)],
                        },
                    },
                ],
            },
        };
        let mut manifest = test_manifest();
        manifest.callbacks_total = 1;
        manifest.callbacks_classified = 1;
        manifest.required_handlers = vec![HandlerRequirement {
            id: "select.empty_string".to_owned(),
            minimum_version: 1,
        }];
        let package = SchemaPackage::new_with_callbacks(
            manifest,
            vec![SchemaRecord {
                signature: SchemaSignature(*b"TEST"),
                name: "Test".to_owned(),
                root: node.clone(),
            }],
            vec![CallbackBinding {
                path: union_path.to_owned(),
                callback_id: "union.select".to_owned(),
                callback_slot: None,
                implementation_fingerprint: "00".repeat(32),
                implementation: CallbackImplementation::BuiltIn {
                    operation: BuiltInOperation {
                        id: "select.empty_string".to_owned(),
                        minimum_version: 1,
                        configuration: serde_json::json!({ "path": script_name_path }),
                    },
                },
            }],
        )?;
        let editor = RecordEditor {
            registry: bethkit_schema::SchemaRegistry::new(Arc::new(package)),
            decoders: crate::DecoderRegistry::builtin(),
            handlers: crate::SemanticHandlerRegistry::builtin(),
            record: WritableRecord {
                signature: Signature(*b"TEST"),
                flags: bethkit_core::RecordFlags::empty(),
                form_id: bethkit_core::FormId::NULL,
                form_version: 44,
                subrecords: Vec::new(),
            },
            localized: false,
            after_load_migrations: 0,
            decoded_values: BTreeMap::new(),
        };

        assert_eq!(
            editor.encode_node_at(
                &node,
                &OwnedFieldValue::Struct(vec![
                    OwnedFieldValue::String("Q".to_owned()),
                    OwnedFieldValue::Bytes(vec![7]),
                ]),
                Some(0),
            )?,
            vec![1, b'Q', 7]
        );
        assert_eq!(
            editor.encode_node_at(
                &node,
                &OwnedFieldValue::Struct(vec![
                    OwnedFieldValue::String(String::new()),
                    OwnedFieldValue::Bytes(Vec::new()),
                ]),
                Some(0),
            )?,
            vec![0]
        );
        Ok(())
    }

    /// Selects a sibling-dependent union from the in-flight edited fields.
    #[test]
    fn union_encoding_uses_updated_sibling_fields() -> Result<()> {
        let editor = editor_with_reused_signature()?;
        let integer = |width| PrimitiveType::Integer {
            integer: IntegerType {
                width,
                signed: false,
                byte_order: ByteOrder::LittleEndian,
            },
        };
        let flags_path = "TEST/data/0:Flags".to_owned();
        let node = SchemaNode {
            id: SchemaNodeId(200),
            path: "TEST/data".to_owned(),
            name: "Data".to_owned(),
            required: true,
            conflict_priority: ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Struct {
                fields: vec![
                    SchemaNode {
                        id: SchemaNodeId(201),
                        path: flags_path.clone(),
                        name: "Flags".to_owned(),
                        required: true,
                        conflict_priority: ConflictPriority::Normal,
                        condition: None,
                        kind: SchemaNodeKind::Primitive {
                            primitive: integer(1),
                        },
                    },
                    SchemaNode {
                        id: SchemaNodeId(202),
                        path: "TEST/data/1:Value".to_owned(),
                        name: "Value".to_owned(),
                        required: true,
                        conflict_priority: ConflictPriority::Normal,
                        condition: None,
                        kind: SchemaNodeKind::Union {
                            selector: UnionSelector::Expression(
                                bethkit_schema::Expression::Select {
                                    condition: Box::new(bethkit_schema::Expression::BitSet {
                                        value: Box::new(bethkit_schema::Expression::ReadField {
                                            path: flags_path,
                                        }),
                                        bit: 0,
                                    }),
                                    if_true: Box::new(bethkit_schema::Expression::Int { value: 1 }),
                                    if_false: Box::new(bethkit_schema::Expression::Int {
                                        value: 0,
                                    }),
                                },
                            ),
                            variants: vec![
                                SchemaNode {
                                    id: SchemaNodeId(203),
                                    path: "TEST/data/1:Value/variants/0".to_owned(),
                                    name: "Byte".to_owned(),
                                    required: true,
                                    conflict_priority: ConflictPriority::Normal,
                                    condition: None,
                                    kind: SchemaNodeKind::Primitive {
                                        primitive: integer(1),
                                    },
                                },
                                SchemaNode {
                                    id: SchemaNodeId(204),
                                    path: "TEST/data/1:Value/variants/1".to_owned(),
                                    name: "Word".to_owned(),
                                    required: true,
                                    conflict_priority: ConflictPriority::Normal,
                                    condition: None,
                                    kind: SchemaNodeKind::Primitive {
                                        primitive: integer(2),
                                    },
                                },
                            ],
                        },
                    },
                ],
            },
        };
        let value = OwnedFieldValue::Struct(vec![
            OwnedFieldValue::UInt(1),
            OwnedFieldValue::UInt(0x0203),
        ]);

        let encoded = editor.encode_node(&node, &value)?;

        assert_eq!(encoded, vec![1, 3, 2]);
        Ok(())
    }

    /// Applies a record-level editor-ID callback to a nested fixed string atomically.
    #[test]
    fn record_editor_id_updates_nested_morrowind_script_name() -> Result<()> {
        let name_path = "SCPT/0:Script Header/payload/0:Name";
        let string_field = SchemaNode {
            id: SchemaNodeId(3),
            path: name_path.to_owned(),
            name: "Name".to_owned(),
            required: true,
            conflict_priority: ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Primitive {
                primitive: PrimitiveType::String {
                    string: StringType {
                        encoding: "windows_1252".to_owned(),
                        localized: false,
                        zero_terminated: false,
                        fixed_length: Some(32),
                        length_prefix: None,
                        trailing_terminator: None,
                        allowed_values: Vec::new(),
                    },
                },
            },
        };
        let mut fields = vec![string_field];
        for (index, name) in [
            "NumShorts",
            "NumLongs",
            "NumFloats",
            "ScriptDataSize",
            "LocalVarSize",
        ]
        .into_iter()
        .enumerate()
        {
            fields.push(SchemaNode {
                id: SchemaNodeId(4 + index as u32),
                path: format!("SCPT/0:Script Header/payload/{}:{name}", index + 1),
                name: name.to_owned(),
                required: true,
                conflict_priority: ConflictPriority::Normal,
                condition: None,
                kind: SchemaNodeKind::Primitive {
                    primitive: PrimitiveType::Integer {
                        integer: IntegerType {
                            width: 4,
                            signed: true,
                            byte_order: ByteOrder::LittleEndian,
                        },
                    },
                },
            });
        }
        let root = SchemaNode {
            id: SchemaNodeId(0),
            path: "SCPT".to_owned(),
            name: "Script".to_owned(),
            required: true,
            conflict_priority: ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Sequence {
                children: vec![SchemaNode {
                    id: SchemaNodeId(1),
                    path: "SCPT/0:Script Header".to_owned(),
                    name: "Script Header".to_owned(),
                    required: true,
                    conflict_priority: ConflictPriority::Normal,
                    condition: None,
                    kind: SchemaNodeKind::Subrecord {
                        signature: SchemaSignature(*b"SCHD"),
                        payload: Box::new(SchemaNode {
                            id: SchemaNodeId(2),
                            path: "SCPT/0:Script Header/payload".to_owned(),
                            name: "Structure".to_owned(),
                            required: true,
                            conflict_priority: ConflictPriority::Normal,
                            condition: None,
                            kind: SchemaNodeKind::Struct { fields },
                        }),
                    },
                }],
            },
        };
        let mut manifest = test_manifest();
        manifest.game = SchemaGame::Morrowind;
        manifest.callbacks_total = 1;
        manifest.callbacks_classified = 1;
        manifest.required_handlers = vec![HandlerRequirement {
            id: "metadata.morrowind.script_editor_id".to_owned(),
            minimum_version: 1,
        }];
        let package = SchemaPackage::new_with_callbacks(
            manifest,
            vec![SchemaRecord {
                signature: SchemaSignature(*b"SCPT"),
                name: "Script".to_owned(),
                root,
            }],
            vec![CallbackBinding {
                path: "SCPT".to_owned(),
                callback_id: "record.set_editor_id".to_owned(),
                callback_slot: None,
                implementation_fingerprint: "11".repeat(32),
                implementation: CallbackImplementation::BuiltIn {
                    operation: BuiltInOperation {
                        id: "metadata.morrowind.script_editor_id".to_owned(),
                        minimum_version: 1,
                        configuration: serde_json::json!({ "field_path": name_path }),
                    },
                },
            }],
        )?;
        let context = SemanticContext::new(Arc::new(package), crate::DecoderRegistry::builtin())?;
        let mut header = vec![0_u8; 52];
        header[..10].copy_from_slice(b"OldScript\0");
        for (index, value) in [1_i32, 2, 3, 4, 5].into_iter().enumerate() {
            let start = 32 + index * 4;
            header[start..start + 4].copy_from_slice(&value.to_le_bytes());
        }
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"SCPT");
        bytes.extend_from_slice(&58_u32.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        bytes.extend_from_slice(b"SCHD");
        bytes.extend_from_slice(&52_u16.to_le_bytes());
        bytes.extend_from_slice(&header);
        let mut cursor = bethkit_io::SliceCursor::new(&bytes);
        let record = Record::parse_header(&mut cursor, &bethkit_core::GameContext::sse())?;
        let mut editor = context.edit(&record, false)?;

        assert!(editor.set_record_editor_id("NewScript")?);
        let writable = editor.into_writable_record();
        assert_eq!(&writable.subrecords[0].data[..9], b"NewScript");
        assert!(writable.subrecords[0].data[9..32]
            .iter()
            .all(|byte| *byte == 0));
        assert_eq!(&writable.subrecords[0].data[32..], &header[32..]);
        Ok(())
    }

    /// Normalizes legacy Fallout CTDA bytes only in the writable editor snapshot.
    #[test]
    fn editor_applies_legacy_ctda_after_load_migration() -> Result<()> {
        let condition_path = "TEST/0:Condition";
        let payload_path = "TEST/0:Condition/payload";
        let fields = vec![
            SchemaNode {
                id: SchemaNodeId(3),
                path: format!("{payload_path}/0:Type"),
                name: "Type".to_owned(),
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
            },
            SchemaNode {
                id: SchemaNodeId(4),
                path: format!("{payload_path}/1:Legacy Data"),
                name: "Legacy Data".to_owned(),
                required: true,
                conflict_priority: ConflictPriority::Normal,
                condition: None,
                kind: SchemaNodeKind::Primitive {
                    primitive: PrimitiveType::Bytes { length: Some(19) },
                },
            },
            SchemaNode {
                id: SchemaNodeId(5),
                path: format!("{payload_path}/2:Run On"),
                name: "Run On".to_owned(),
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
            },
            SchemaNode {
                id: SchemaNodeId(6),
                path: format!("{payload_path}/3:Reference"),
                name: "Reference".to_owned(),
                required: true,
                conflict_priority: ConflictPriority::Normal,
                condition: None,
                kind: SchemaNodeKind::Primitive {
                    primitive: PrimitiveType::Bytes { length: Some(4) },
                },
            },
        ];
        let mut manifest = test_manifest();
        manifest.game = SchemaGame::FalloutNv;
        manifest.callbacks_total = 1;
        manifest.callbacks_classified = 1;
        manifest.required_handlers = vec![HandlerRequirement {
            id: "migrate.legacy_ctda_run_on".to_owned(),
            minimum_version: 1,
        }];
        let package = SchemaPackage::new_with_callbacks(
            manifest,
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
                        children: vec![SchemaNode {
                            id: SchemaNodeId(1),
                            path: condition_path.to_owned(),
                            name: "Condition".to_owned(),
                            required: true,
                            conflict_priority: ConflictPriority::Normal,
                            condition: None,
                            kind: SchemaNodeKind::Subrecord {
                                signature: SchemaSignature(*b"CTDA"),
                                payload: Box::new(SchemaNode {
                                    id: SchemaNodeId(2),
                                    path: payload_path.to_owned(),
                                    name: "Condition".to_owned(),
                                    required: true,
                                    conflict_priority: ConflictPriority::Normal,
                                    condition: None,
                                    kind: SchemaNodeKind::Struct { fields },
                                }),
                            },
                        }],
                    },
                },
            }],
            vec![CallbackBinding {
                path: condition_path.to_owned(),
                callback_id: "def.after_load".to_owned(),
                callback_slot: None,
                implementation_fingerprint: "55".repeat(32),
                implementation: CallbackImplementation::BuiltIn {
                    operation: BuiltInOperation {
                        id: "migrate.legacy_ctda_run_on".to_owned(),
                        minimum_version: 1,
                        configuration: serde_json::json!({}),
                    },
                },
            }],
        )?;
        let context = SemanticContext::new(Arc::new(package), crate::DecoderRegistry::builtin())?;
        let mut legacy_payload = (0_u8..20).collect::<Vec<_>>();
        legacy_payload[0] = 0x27;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"TEST");
        bytes.extend_from_slice(&26_u32.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        bytes.extend_from_slice(b"CTDA");
        bytes.extend_from_slice(&20_u16.to_le_bytes());
        bytes.extend_from_slice(&legacy_payload);
        let mut cursor = bethkit_io::SliceCursor::new(&bytes);
        let record = Record::parse_header(&mut cursor, &bethkit_core::GameContext::sse())?;

        let editor = context.edit(&record, false)?;

        assert_eq!(record.subrecords()?[0].as_bytes(), legacy_payload);
        assert_eq!(editor.after_load_migration_count(), 1);
        let writable = editor.into_writable_record();
        assert_eq!(writable.subrecords[0].data.len(), 28);
        assert_eq!(writable.subrecords[0].data[0], 0x27 & !0x02);
        assert_eq!(&writable.subrecords[0].data[1..20], &legacy_payload[1..]);
        assert_eq!(&writable.subrecords[0].data[20..24], &1_u32.to_le_bytes());
        assert_eq!(&writable.subrecords[0].data[24..28], &[0; 4]);
        Ok(())
    }

    /// Supplies repeat-local EFID and resolver context to an EFIT load migration.
    #[test]
    fn editor_applies_legacy_efit_after_load_migration() -> Result<()> {
        let efid_path = "TEST/0:Effect/0:EFID";
        let efit_path = "TEST/0:Effect/1:EFIT";
        let root = SchemaNode {
            id: SchemaNodeId(0),
            path: "TEST".to_owned(),
            name: "Test".to_owned(),
            required: true,
            conflict_priority: ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Sequence {
                children: vec![SchemaNode {
                    id: SchemaNodeId(1),
                    path: "TEST/0:Effect".to_owned(),
                    name: "Effect".to_owned(),
                    required: true,
                    conflict_priority: ConflictPriority::Normal,
                    condition: None,
                    kind: SchemaNodeKind::Sequence {
                        children: vec![
                            SchemaNode {
                                id: SchemaNodeId(2),
                                path: efid_path.to_owned(),
                                name: "Base Effect".to_owned(),
                                required: true,
                                conflict_priority: ConflictPriority::Normal,
                                condition: None,
                                kind: SchemaNodeKind::Subrecord {
                                    signature: SchemaSignature(*b"EFID"),
                                    payload: Box::new(SchemaNode {
                                        id: SchemaNodeId(3),
                                        path: format!("{efid_path}/payload"),
                                        name: "Base Effect".to_owned(),
                                        required: true,
                                        conflict_priority: ConflictPriority::Normal,
                                        condition: None,
                                        kind: SchemaNodeKind::Primitive {
                                            primitive: PrimitiveType::FormId {
                                                targets: vec![SchemaSignature(*b"MGEF")],
                                            },
                                        },
                                    }),
                                },
                            },
                            SchemaNode {
                                id: SchemaNodeId(4),
                                path: efit_path.to_owned(),
                                name: "Effect Data".to_owned(),
                                required: true,
                                conflict_priority: ConflictPriority::Normal,
                                condition: None,
                                kind: SchemaNodeKind::Subrecord {
                                    signature: SchemaSignature(*b"EFIT"),
                                    payload: Box::new(SchemaNode {
                                        id: SchemaNodeId(5),
                                        path: format!("{efit_path}/payload"),
                                        name: "Effect Data".to_owned(),
                                        required: true,
                                        conflict_priority: ConflictPriority::Normal,
                                        condition: None,
                                        kind: SchemaNodeKind::Primitive {
                                            primitive: PrimitiveType::Bytes { length: Some(20) },
                                        },
                                    }),
                                },
                            },
                        ],
                    },
                }],
            },
        };
        let mut manifest = test_manifest();
        manifest.game = SchemaGame::FalloutNv;
        manifest.callbacks_total = 1;
        manifest.callbacks_classified = 1;
        manifest.required_handlers = vec![HandlerRequirement {
            id: "migrate.legacy_efit_actor_value".to_owned(),
            minimum_version: 1,
        }];
        let package = SchemaPackage::new_with_callbacks(
            manifest,
            vec![SchemaRecord {
                signature: SchemaSignature(*b"TEST"),
                name: "Test".to_owned(),
                root,
            }],
            vec![CallbackBinding {
                path: efit_path.to_owned(),
                callback_id: "def.after_load".to_owned(),
                callback_slot: None,
                implementation_fingerprint: "66".repeat(32),
                implementation: CallbackImplementation::BuiltIn {
                    operation: BuiltInOperation {
                        id: "migrate.legacy_efit_actor_value".to_owned(),
                        minimum_version: 1,
                        configuration: serde_json::json!({}),
                    },
                },
            }],
        )?;
        let mut handlers = SemanticHandlerRegistry::builtin();
        handlers.set_form_link_resolver(Arc::new(TestMagicEffectResolver));
        let context = SemanticContext::new_with_handlers(
            Arc::new(package),
            crate::DecoderRegistry::builtin(),
            handlers,
        )?;
        let mut efit = (0_u8..20).collect::<Vec<_>>();
        efit[16..20].copy_from_slice(&(-1_i32).to_le_bytes());
        let source = Record::from_writable(&WritableRecord {
            signature: Signature(*b"TEST"),
            flags: bethkit_core::RecordFlags::empty(),
            form_id: bethkit_core::FormId(0x1111),
            form_version: 0,
            subrecords: vec![
                WritableSubRecord {
                    signature: Signature(*b"EFID"),
                    data: 0x6789_u32.to_le_bytes().to_vec(),
                },
                WritableSubRecord {
                    signature: Signature(*b"EFIT"),
                    data: efit.clone(),
                },
            ],
        });

        let editor = context.edit(&source, false)?;

        assert_eq!(source.subrecords()?[1].as_bytes(), efit);
        assert_eq!(editor.after_load_migration_count(), 1);
        let writable = editor.into_writable_record();
        assert_eq!(&writable.subrecords[1].data[..16], &efit[..16]);
        assert_eq!(&writable.subrecords[1].data[16..20], &48_i32.to_le_bytes());
        Ok(())
    }

    /// Applies Oblivion's MGEF-code EFIT migration only to the writable snapshot.
    #[test]
    fn editor_applies_oblivion_efit_after_load_migration() -> Result<()> {
        let efit_path = "TEST/0:EFIT";
        let mut manifest = test_manifest();
        manifest.game = SchemaGame::Oblivion;
        manifest.callbacks_total = 1;
        manifest.callbacks_classified = 1;
        manifest.required_handlers = vec![HandlerRequirement {
            id: "migrate.oblivion_efit_actor_value".to_owned(),
            minimum_version: 1,
        }];
        let package = SchemaPackage::new_with_callbacks(
            manifest,
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
                        children: vec![SchemaNode {
                            id: SchemaNodeId(1),
                            path: efit_path.to_owned(),
                            name: "Effect Data".to_owned(),
                            required: true,
                            conflict_priority: ConflictPriority::Normal,
                            condition: None,
                            kind: SchemaNodeKind::Subrecord {
                                signature: SchemaSignature(*b"EFIT"),
                                payload: Box::new(SchemaNode {
                                    id: SchemaNodeId(2),
                                    path: format!("{efit_path}/payload"),
                                    name: "Effect Data".to_owned(),
                                    required: true,
                                    conflict_priority: ConflictPriority::Normal,
                                    condition: None,
                                    kind: SchemaNodeKind::Primitive {
                                        primitive: PrimitiveType::Bytes { length: Some(24) },
                                    },
                                }),
                            },
                        }],
                    },
                },
            }],
            vec![CallbackBinding {
                path: efit_path.to_owned(),
                callback_id: "def.after_load".to_owned(),
                callback_slot: None,
                implementation_fingerprint: "77".repeat(32),
                implementation: CallbackImplementation::BuiltIn {
                    operation: BuiltInOperation {
                        id: "migrate.oblivion_efit_actor_value".to_owned(),
                        minimum_version: 1,
                        configuration: serde_json::json!({}),
                    },
                },
            }],
        )?;
        let mut handlers = SemanticHandlerRegistry::builtin();
        handlers.set_form_link_resolver(Arc::new(TestMagicEffectResolver));
        let context = SemanticContext::new_with_handlers(
            Arc::new(package),
            crate::DecoderRegistry::builtin(),
            handlers,
        )?;
        let mut efit = (0_u8..24).collect::<Vec<_>>();
        efit[..4].copy_from_slice(b"ABCD");
        efit[20..24].copy_from_slice(&(-1_i32).to_le_bytes());
        let source = Record::from_writable(&WritableRecord {
            signature: Signature(*b"TEST"),
            flags: bethkit_core::RecordFlags::empty(),
            form_id: bethkit_core::FormId(0x1111),
            form_version: 0,
            subrecords: vec![WritableSubRecord {
                signature: Signature(*b"EFIT"),
                data: efit.clone(),
            }],
        });

        let editor = context.edit(&source, false)?;

        assert_eq!(source.subrecords()?[0].as_bytes(), efit);
        assert_eq!(editor.after_load_migration_count(), 1);
        let writable = editor.into_writable_record();
        assert_eq!(&writable.subrecords[0].data[..20], &efit[..20]);
        assert_eq!(&writable.subrecords[0].data[20..24], &42_i32.to_le_bytes());
        Ok(())
    }

    /// Applies a record-level keyword cleanup before the initial decoded snapshot.
    #[test]
    fn editor_applies_record_after_load_mutations() -> Result<()> {
        let keyword_path = "MISC/1:Keywords";
        let mut manifest = test_manifest();
        manifest.game = SchemaGame::SkyrimSe;
        manifest.callbacks_total = 1;
        manifest.callbacks_classified = 1;
        manifest.required_handlers = vec![HandlerRequirement {
            id: "migrate.remove_orphaned_keyword_array".to_owned(),
            minimum_version: 1,
        }];
        let subrecord = |id, path: &str, name: &str, signature| SchemaNode {
            id: SchemaNodeId(id),
            path: path.to_owned(),
            name: name.to_owned(),
            required: false,
            conflict_priority: ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Subrecord {
                signature: SchemaSignature(signature),
                payload: Box::new(SchemaNode {
                    id: SchemaNodeId(id + 1),
                    path: format!("{path}/payload"),
                    name: name.to_owned(),
                    required: false,
                    conflict_priority: ConflictPriority::Normal,
                    condition: None,
                    kind: SchemaNodeKind::Primitive {
                        primitive: PrimitiveType::Bytes { length: Some(4) },
                    },
                }),
            },
        };
        let package = SchemaPackage::new_with_callbacks(
            manifest,
            vec![SchemaRecord {
                signature: SchemaSignature(*b"MISC"),
                name: "Misc. Item".to_owned(),
                root: SchemaNode {
                    id: SchemaNodeId(0),
                    path: "MISC".to_owned(),
                    name: "Misc. Item".to_owned(),
                    required: true,
                    conflict_priority: ConflictPriority::Normal,
                    condition: None,
                    kind: SchemaNodeKind::Sequence {
                        children: vec![
                            subrecord(1, "MISC/0:Keyword Count", "Keyword Count", *b"KSIZ"),
                            subrecord(3, keyword_path, "Keywords", *b"KWDA"),
                        ],
                    },
                },
            }],
            vec![CallbackBinding {
                path: "MISC".to_owned(),
                callback_id: "def.after_load".to_owned(),
                callback_slot: None,
                implementation_fingerprint: "88".repeat(32),
                implementation: CallbackImplementation::BuiltIn {
                    operation: BuiltInOperation {
                        id: "migrate.remove_orphaned_keyword_array".to_owned(),
                        minimum_version: 1,
                        configuration: serde_json::json!({
                            "keyword_path": keyword_path,
                        }),
                    },
                },
            }],
        )?;
        let context = SemanticContext::new(Arc::new(package), crate::DecoderRegistry::builtin())?;
        let source = Record::from_writable(&WritableRecord {
            signature: Signature(*b"MISC"),
            flags: bethkit_core::RecordFlags::empty(),
            form_id: bethkit_core::FormId(0x1111),
            form_version: 0,
            subrecords: vec![WritableSubRecord {
                signature: Signature(*b"KWDA"),
                data: 0x1234_u32.to_le_bytes().to_vec(),
            }],
        });

        let editor = context.edit(&source, false)?;

        assert_eq!(source.subrecords()?.len(), 1);
        assert_eq!(source.subrecords()?[0].signature, Signature(*b"KWDA"));
        assert_eq!(editor.after_load_migration_count(), 1);
        assert!(editor.into_writable_record().subrecords.is_empty());
        Ok(())
    }

    /// Applies all mutating branches of xEdit's MESG after-load reconciliation.
    #[test]
    fn editor_applies_message_after_load_mutations() -> Result<()> {
        let flags_path = "MESG/5:Flags";
        let display_time_path = "MESG/6:Display Time";
        let subrecord = |id, path: &str, name: &str, signature| SchemaNode {
            id: SchemaNodeId(id),
            path: path.to_owned(),
            name: name.to_owned(),
            required: false,
            conflict_priority: ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Subrecord {
                signature: SchemaSignature(signature),
                payload: Box::new(SchemaNode {
                    id: SchemaNodeId(id + 1),
                    path: format!("{path}/payload"),
                    name: name.to_owned(),
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
        };
        let mut manifest = test_manifest();
        manifest.game = SchemaGame::SkyrimSe;
        manifest.callbacks_total = 1;
        manifest.callbacks_classified = 1;
        manifest.required_handlers = vec![HandlerRequirement {
            id: "migrate.message_display_time".to_owned(),
            minimum_version: 1,
        }];
        let package = SchemaPackage::new_with_callbacks(
            manifest,
            vec![SchemaRecord {
                signature: SchemaSignature(*b"MESG"),
                name: "Message".to_owned(),
                root: SchemaNode {
                    id: SchemaNodeId(0),
                    path: "MESG".to_owned(),
                    name: "Message".to_owned(),
                    required: true,
                    conflict_priority: ConflictPriority::Normal,
                    condition: None,
                    kind: SchemaNodeKind::Sequence {
                        children: vec![
                            subrecord(1, flags_path, "Flags", *b"DNAM"),
                            subrecord(3, display_time_path, "Display Time", *b"TNAM"),
                        ],
                    },
                },
            }],
            vec![CallbackBinding {
                path: "MESG".to_owned(),
                callback_id: "def.after_load".to_owned(),
                callback_slot: None,
                implementation_fingerprint: "99".repeat(32),
                implementation: CallbackImplementation::BuiltIn {
                    operation: BuiltInOperation {
                        id: "migrate.message_display_time".to_owned(),
                        minimum_version: 1,
                        configuration: serde_json::json!({
                            "flags_path": flags_path,
                            "display_time_path": display_time_path,
                        }),
                    },
                },
            }],
        )?;
        let context = SemanticContext::new(Arc::new(package), crate::DecoderRegistry::builtin())?;
        let record = |flags: Option<u32>, display_time: bool| {
            let mut subrecords = Vec::new();
            if let Some(flags) = flags {
                subrecords.push(WritableSubRecord {
                    signature: Signature(*b"DNAM"),
                    data: flags.to_le_bytes().to_vec(),
                });
            }
            if display_time {
                subrecords.push(WritableSubRecord {
                    signature: Signature(*b"TNAM"),
                    data: 10_u32.to_le_bytes().to_vec(),
                });
            }
            Record::from_writable(&WritableRecord {
                signature: Signature(*b"MESG"),
                flags: bethkit_core::RecordFlags::empty(),
                form_id: bethkit_core::FormId(0x1111),
                form_version: 0,
                subrecords,
            })
        };

        let message_box = record(Some(1), true);
        let migrated = context.edit(&message_box, false)?.into_writable_record();
        assert_eq!(message_box.subrecords()?.len(), 2);
        assert_eq!(migrated.subrecords.len(), 1);
        assert_eq!(migrated.subrecords[0].data, 1_u32.to_le_bytes());

        let ordinary = context
            .edit(&record(Some(2), false), false)?
            .into_writable_record();
        assert_eq!(ordinary.subrecords[0].data, 3_u32.to_le_bytes());

        let missing = context
            .edit(&record(None, false), false)?
            .into_writable_record();
        assert_eq!(missing.subrecords.len(), 1);
        assert_eq!(missing.subrecords[0].signature, Signature(*b"DNAM"));
        assert_eq!(missing.subrecords[0].data, 1_u32.to_le_bytes());
        Ok(())
    }

    /// Anchors a nested payload callback to its containing source subrecord.
    #[test]
    fn editor_applies_nested_payload_after_load_callback() -> Result<()> {
        let objects_path = "DOBJ/1:Objects";
        let payload_path = "DOBJ/1:Objects/payload";
        let mut manifest = test_manifest();
        manifest.game = SchemaGame::SkyrimSe;
        manifest.callbacks_total = 1;
        manifest.callbacks_classified = 1;
        manifest.required_handlers = vec![HandlerRequirement {
            id: "migrate.remove_empty_default_objects".to_owned(),
            minimum_version: 1,
        }];
        let package = SchemaPackage::new_with_callbacks(
            manifest,
            vec![SchemaRecord {
                signature: SchemaSignature(*b"DOBJ"),
                name: "Default Object Manager".to_owned(),
                root: SchemaNode {
                    id: SchemaNodeId(0),
                    path: "DOBJ".to_owned(),
                    name: "Default Object Manager".to_owned(),
                    required: true,
                    conflict_priority: ConflictPriority::Normal,
                    condition: None,
                    kind: SchemaNodeKind::Sequence {
                        children: vec![SchemaNode {
                            id: SchemaNodeId(1),
                            path: objects_path.to_owned(),
                            name: "Objects".to_owned(),
                            required: true,
                            conflict_priority: ConflictPriority::Normal,
                            condition: None,
                            kind: SchemaNodeKind::Subrecord {
                                signature: SchemaSignature(*b"DNAM"),
                                payload: Box::new(SchemaNode {
                                    id: SchemaNodeId(2),
                                    path: payload_path.to_owned(),
                                    name: "Objects".to_owned(),
                                    required: true,
                                    conflict_priority: ConflictPriority::Normal,
                                    condition: None,
                                    kind: SchemaNodeKind::Primitive {
                                        primitive: PrimitiveType::Bytes { length: None },
                                    },
                                }),
                            },
                        }],
                    },
                },
            }],
            vec![CallbackBinding {
                path: payload_path.to_owned(),
                callback_id: "def.after_load".to_owned(),
                callback_slot: None,
                implementation_fingerprint: "aa".repeat(32),
                implementation: CallbackImplementation::BuiltIn {
                    operation: BuiltInOperation {
                        id: "migrate.remove_empty_default_objects".to_owned(),
                        minimum_version: 1,
                        configuration: serde_json::json!({}),
                    },
                },
            }],
        )?;
        let context = SemanticContext::new(Arc::new(package), crate::DecoderRegistry::builtin())?;
        let retained = [1_u32.to_le_bytes(), 0x1234_u32.to_le_bytes()].concat();
        let removed = [0_u32.to_le_bytes(), 0x5678_u32.to_le_bytes()].concat();
        let source = Record::from_writable(&WritableRecord {
            signature: Signature(*b"DOBJ"),
            flags: bethkit_core::RecordFlags::empty(),
            form_id: bethkit_core::FormId(0x1111),
            form_version: 0,
            subrecords: vec![WritableSubRecord {
                signature: Signature(*b"DNAM"),
                data: [retained.as_slice(), removed.as_slice()].concat(),
            }],
        });

        let editor = context.edit(&source, false)?;

        assert_eq!(source.subrecords()?[0].as_bytes().len(), 16);
        assert_eq!(editor.after_load_migration_count(), 1);
        assert_eq!(editor.into_writable_record().subrecords[0].data, retained);
        Ok(())
    }

    /// Applies Skyrim's WEAP cleanup while retaining every unrelated DNAM byte.
    #[test]
    fn editor_applies_skyrim_weapon_after_load_cleanup() -> Result<()> {
        let data_path = "WEAP/29:Data";
        let mut manifest = test_manifest();
        manifest.game = SchemaGame::SkyrimSe;
        manifest.callbacks_total = 1;
        manifest.callbacks_classified = 1;
        manifest.required_handlers = vec![HandlerRequirement {
            id: "migrate.skyrim_weapon_flags".to_owned(),
            minimum_version: 1,
        }];
        let package = SchemaPackage::new_with_callbacks(
            manifest,
            vec![SchemaRecord {
                signature: SchemaSignature(*b"WEAP"),
                name: "Weapon".to_owned(),
                root: SchemaNode {
                    id: SchemaNodeId(0),
                    path: "WEAP".to_owned(),
                    name: "Weapon".to_owned(),
                    required: true,
                    conflict_priority: ConflictPriority::Normal,
                    condition: None,
                    kind: SchemaNodeKind::Sequence {
                        children: vec![SchemaNode {
                            id: SchemaNodeId(1),
                            path: data_path.to_owned(),
                            name: "Data".to_owned(),
                            required: false,
                            conflict_priority: ConflictPriority::Normal,
                            condition: None,
                            kind: SchemaNodeKind::Subrecord {
                                signature: SchemaSignature(*b"DNAM"),
                                payload: Box::new(SchemaNode {
                                    id: SchemaNodeId(2),
                                    path: format!("{data_path}/payload"),
                                    name: "Data".to_owned(),
                                    required: true,
                                    conflict_priority: ConflictPriority::Normal,
                                    condition: None,
                                    kind: SchemaNodeKind::Primitive {
                                        primitive: PrimitiveType::Bytes { length: Some(100) },
                                    },
                                }),
                            },
                        }],
                    },
                },
            }],
            vec![CallbackBinding {
                path: "WEAP".to_owned(),
                callback_id: "def.after_load".to_owned(),
                callback_slot: None,
                implementation_fingerprint: "bb".repeat(32),
                implementation: CallbackImplementation::BuiltIn {
                    operation: BuiltInOperation {
                        id: "migrate.skyrim_weapon_flags".to_owned(),
                        minimum_version: 1,
                        configuration: serde_json::json!({
                            "anchor_path_suffix": "/29:Data",
                        }),
                    },
                },
            }],
        )?;
        let context = SemanticContext::new(Arc::new(package), crate::DecoderRegistry::builtin())?;
        let mut original = (0_u8..100).collect::<Vec<_>>();
        original[12..14].copy_from_slice(&0x00c1_u16.to_le_bytes());
        original[40..44].copy_from_slice(&0x1234_0181_u32.to_le_bytes());
        let source = Record::from_writable(&WritableRecord {
            signature: Signature(*b"WEAP"),
            flags: bethkit_core::RecordFlags::empty(),
            form_id: bethkit_core::FormId(0x1111),
            form_version: 0,
            subrecords: vec![WritableSubRecord {
                signature: Signature(*b"DNAM"),
                data: original.clone(),
            }],
        });

        let editor = context.edit(&source, false)?;

        let mut expected = original.clone();
        expected[12..14].copy_from_slice(&0x0081_u16.to_le_bytes());
        expected[40..44].copy_from_slice(&0x1234_0081_u32.to_le_bytes());
        assert_eq!(source.subrecords()?[0].as_bytes(), original);
        assert_eq!(editor.after_load_migration_count(), 1);
        assert_eq!(editor.into_writable_record().subrecords[0].data, expected);
        Ok(())
    }

    /// Applies LIGH payload replacement and FNAM insertion in one transaction.
    #[test]
    fn editor_applies_light_after_load_defaults() -> Result<()> {
        let data_path = "LIGH/7:DATA";
        let fade_path = "LIGH/8:Fade value";
        let mut manifest = test_manifest();
        manifest.game = SchemaGame::SkyrimSe;
        manifest.callbacks_total = 1;
        manifest.callbacks_classified = 1;
        manifest.required_handlers = vec![HandlerRequirement {
            id: "migrate.light_defaults".to_owned(),
            minimum_version: 1,
        }];
        let subrecord = |id, path: &str, name: &str, signature, payload| SchemaNode {
            id: SchemaNodeId(id),
            path: path.to_owned(),
            name: name.to_owned(),
            required: false,
            conflict_priority: ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Subrecord {
                signature: SchemaSignature(signature),
                payload: Box::new(payload),
            },
        };
        let package = SchemaPackage::new_with_callbacks(
            manifest,
            vec![SchemaRecord {
                signature: SchemaSignature(*b"LIGH"),
                name: "Light".to_owned(),
                root: SchemaNode {
                    id: SchemaNodeId(0),
                    path: "LIGH".to_owned(),
                    name: "Light".to_owned(),
                    required: true,
                    conflict_priority: ConflictPriority::Normal,
                    condition: None,
                    kind: SchemaNodeKind::Sequence {
                        children: vec![
                            subrecord(
                                1,
                                data_path,
                                "DATA",
                                *b"DATA",
                                SchemaNode {
                                    id: SchemaNodeId(2),
                                    path: format!("{data_path}/payload"),
                                    name: "DATA".to_owned(),
                                    required: true,
                                    conflict_priority: ConflictPriority::Normal,
                                    condition: None,
                                    kind: SchemaNodeKind::Primitive {
                                        primitive: PrimitiveType::Bytes { length: Some(48) },
                                    },
                                },
                            ),
                            subrecord(
                                3,
                                fade_path,
                                "Fade value",
                                *b"FNAM",
                                SchemaNode {
                                    id: SchemaNodeId(4),
                                    path: format!("{fade_path}/payload"),
                                    name: "Fade value".to_owned(),
                                    required: true,
                                    conflict_priority: ConflictPriority::Normal,
                                    condition: None,
                                    kind: SchemaNodeKind::Primitive {
                                        primitive: PrimitiveType::Float {
                                            width: 4,
                                            byte_order: ByteOrder::LittleEndian,
                                            scale: 1.0,
                                            digits: 6,
                                        },
                                    },
                                },
                            ),
                        ],
                    },
                },
            }],
            vec![CallbackBinding {
                path: "LIGH".to_owned(),
                callback_id: "def.after_load".to_owned(),
                callback_slot: None,
                implementation_fingerprint: "cc".repeat(32),
                implementation: CallbackImplementation::BuiltIn {
                    operation: BuiltInOperation {
                        id: "migrate.light_defaults".to_owned(),
                        minimum_version: 1,
                        configuration: serde_json::json!({
                            "data_path": data_path,
                            "fade_path": fade_path,
                        }),
                    },
                },
            }],
        )?;
        let context = SemanticContext::new(Arc::new(package), crate::DecoderRegistry::builtin())?;
        let mut original = (0_u8..48).collect::<Vec<_>>();
        original[16..20].copy_from_slice(&0.0_f32.to_le_bytes());
        original[20..24].copy_from_slice(&0.0_f32.to_le_bytes());
        let source = Record::from_writable(&WritableRecord {
            signature: Signature(*b"LIGH"),
            flags: bethkit_core::RecordFlags::empty(),
            form_id: bethkit_core::FormId(0x1111),
            form_version: 0,
            subrecords: vec![WritableSubRecord {
                signature: Signature(*b"DATA"),
                data: original.clone(),
            }],
        });

        let editor = context.edit(&source, false)?;

        let mut expected = original.clone();
        expected[16..20].copy_from_slice(&1.0_f32.to_le_bytes());
        expected[20..24].copy_from_slice(&90.0_f32.to_le_bytes());
        assert_eq!(source.subrecords()?[0].as_bytes(), original);
        assert_eq!(editor.after_load_migration_count(), 1);
        let writable = editor.into_writable_record();
        assert_eq!(writable.subrecords.len(), 2);
        assert_eq!(writable.subrecords[0].data, expected);
        assert_eq!(writable.subrecords[1].signature, Signature(*b"FNAM"));
        assert_eq!(writable.subrecords[1].data, 1.0_f32.to_le_bytes());
        Ok(())
    }

    /// Dispatches a container-level load callback through its explicit SCHR anchor.
    #[test]
    fn editor_applies_embedded_script_after_load_migration() -> Result<()> {
        let script_path = "TEST/0:Embedded Script";
        let header_path = "TEST/0:Embedded Script/0:Basic Script Data";
        let mut manifest = test_manifest();
        manifest.game = SchemaGame::Fallout3;
        manifest.callbacks_total = 1;
        manifest.callbacks_classified = 1;
        manifest.required_handlers = vec![HandlerRequirement {
            id: "migrate.embedded_script_type".to_owned(),
            minimum_version: 1,
        }];
        let package = SchemaPackage::new_with_callbacks(
            manifest,
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
                        children: vec![SchemaNode {
                            id: SchemaNodeId(1),
                            path: script_path.to_owned(),
                            name: "Embedded Script".to_owned(),
                            required: true,
                            conflict_priority: ConflictPriority::Normal,
                            condition: None,
                            kind: SchemaNodeKind::Sequence {
                                children: vec![SchemaNode {
                                    id: SchemaNodeId(2),
                                    path: header_path.to_owned(),
                                    name: "Basic Script Data".to_owned(),
                                    required: true,
                                    conflict_priority: ConflictPriority::Normal,
                                    condition: None,
                                    kind: SchemaNodeKind::Subrecord {
                                        signature: SchemaSignature(*b"SCHR"),
                                        payload: Box::new(SchemaNode {
                                            id: SchemaNodeId(3),
                                            path: format!("{header_path}/payload"),
                                            name: "Basic Script Data".to_owned(),
                                            required: true,
                                            conflict_priority: ConflictPriority::Normal,
                                            condition: None,
                                            kind: SchemaNodeKind::Primitive {
                                                primitive: PrimitiveType::Bytes {
                                                    length: Some(20),
                                                },
                                            },
                                        }),
                                    },
                                }],
                            },
                        }],
                    },
                },
            }],
            vec![CallbackBinding {
                path: script_path.to_owned(),
                callback_id: "def.after_load".to_owned(),
                callback_slot: None,
                implementation_fingerprint: "77".repeat(32),
                implementation: CallbackImplementation::BuiltIn {
                    operation: BuiltInOperation {
                        id: "migrate.embedded_script_type".to_owned(),
                        minimum_version: 1,
                        configuration: serde_json::json!({
                            "anchor_path_suffix": "/0:Basic Script Data",
                        }),
                    },
                },
            }],
        )?;
        let context = SemanticContext::new(Arc::new(package), crate::DecoderRegistry::builtin())?;
        let mut script_header = (0_u8..20).collect::<Vec<_>>();
        script_header[16..18].copy_from_slice(&1_u16.to_le_bytes());
        let source = Record::from_writable(&WritableRecord {
            signature: Signature(*b"TEST"),
            flags: bethkit_core::RecordFlags::empty(),
            form_id: bethkit_core::FormId::NULL,
            form_version: 0,
            subrecords: vec![WritableSubRecord {
                signature: Signature(*b"SCHR"),
                data: script_header.clone(),
            }],
        });

        let editor = context.edit(&source, false)?;

        assert_eq!(source.subrecords()?[0].as_bytes(), script_header);
        assert_eq!(editor.after_load_migration_count(), 1);
        let writable = editor.into_writable_record();
        assert_eq!(&writable.subrecords[0].data[..16], &script_header[..16]);
        assert_eq!(&writable.subrecords[0].data[16..18], &0_u16.to_le_bytes());
        assert_eq!(&writable.subrecords[0].data[18..], &script_header[18..]);
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

        let mut manifest = test_manifest();
        manifest.callbacks_total = 1;
        manifest.callbacks_classified = 1;
        manifest.required_handlers = vec![HandlerRequirement {
            id: "edit.sync_record_counts".to_owned(),
            minimum_version: 1,
        }];
        let package = SchemaPackage::new_with_callbacks(
            manifest,
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
            vec![CallbackBinding {
                path: "TEST".to_owned(),
                callback_id: "def.after_set".to_owned(),
                callback_slot: None,
                implementation_fingerprint: "22".repeat(32),
                implementation: CallbackImplementation::BuiltIn {
                    operation: BuiltInOperation {
                        id: "edit.sync_record_counts".to_owned(),
                        minimum_version: 1,
                        configuration: serde_json::json!({
                            "counters": [{
                                "counter_path": "TEST/2:Value Count",
                                "counter_signature": "VCNT",
                                "counter_required": false,
                                "value_signature": "VALU",
                                "mode": "u32_payload_count",
                                "only_when_missing": true
                            }]
                        }),
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
            after_load_migrations: 0,
            decoded_values: BTreeMap::new(),
        })
    }

    fn editor_with_nested_counter() -> Result<RecordEditor> {
        let count_path = "TEST/0:IDLC/payload/0:Animation Count";
        let unused_path = "TEST/0:IDLC/payload/1:Unused";
        let payload = SchemaNode {
            id: SchemaNodeId(2),
            path: "TEST/0:IDLC/payload".to_owned(),
            name: "Animation Control".to_owned(),
            required: true,
            conflict_priority: ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Struct {
                fields: vec![
                    SchemaNode {
                        id: SchemaNodeId(3),
                        path: count_path.to_owned(),
                        name: "Animation Count".to_owned(),
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
                    },
                    SchemaNode {
                        id: SchemaNodeId(4),
                        path: unused_path.to_owned(),
                        name: "Unused".to_owned(),
                        required: true,
                        conflict_priority: ConflictPriority::Ignore,
                        condition: None,
                        kind: SchemaNodeKind::Primitive {
                            primitive: PrimitiveType::Unused { length: 3 },
                        },
                    },
                ],
            },
        };
        let parent = SchemaNode {
            id: SchemaNodeId(1),
            path: "TEST/0:IDLC".to_owned(),
            name: "Animation Count".to_owned(),
            required: true,
            conflict_priority: ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Subrecord {
                signature: SchemaSignature(*b"IDLC"),
                payload: Box::new(payload.clone()),
            },
        };
        let package = SchemaPackage::new_with_callbacks(
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
                        children: vec![parent],
                    },
                },
            }],
            Vec::new(),
        )?;
        let mut editor = RecordEditor {
            registry: bethkit_schema::SchemaRegistry::new(Arc::new(package)),
            decoders: crate::DecoderRegistry::builtin(),
            handlers: SemanticHandlerRegistry::builtin(),
            record: WritableRecord {
                signature: Signature(*b"TEST"),
                flags: bethkit_core::RecordFlags::empty(),
                form_id: bethkit_core::FormId::NULL,
                form_version: 44,
                subrecords: vec![WritableSubRecord {
                    signature: Signature(*b"IDLC"),
                    data: vec![1, 0xaa, 0xbb, 0xcc],
                }],
            },
            localized: false,
            after_load_migrations: 0,
            decoded_values: BTreeMap::new(),
        };
        let decoded = editor.owned_to_handler_value(
            &payload,
            &OwnedFieldValue::Struct(vec![
                OwnedFieldValue::UInt(1),
                OwnedFieldValue::Bytes(vec![0xaa, 0xbb, 0xcc]),
            ]),
        )?;
        editor
            .decoded_values
            .insert(("TEST/0:IDLC".to_owned(), 0), decoded);
        Ok(editor)
    }

    fn editor_with_repeated_counter_groups() -> Result<RecordEditor> {
        fn integer_subrecord(id: u32, path: &str, signature: [u8; 4]) -> SchemaNode {
            SchemaNode {
                id: SchemaNodeId(id),
                path: path.to_owned(),
                name: path.to_owned(),
                required: true,
                conflict_priority: ConflictPriority::Normal,
                condition: None,
                kind: SchemaNodeKind::Subrecord {
                    signature: SchemaSignature(signature),
                    payload: Box::new(SchemaNode {
                        id: SchemaNodeId(id + 100),
                        path: format!("{path}/payload"),
                        name: "Value".to_owned(),
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
            }
        }

        fn bytes_subrecord(id: u32, path: &str, signature: [u8; 4]) -> SchemaNode {
            SchemaNode {
                id: SchemaNodeId(id),
                path: path.to_owned(),
                name: path.to_owned(),
                required: false,
                conflict_priority: ConflictPriority::Normal,
                condition: None,
                kind: SchemaNodeKind::Subrecord {
                    signature: SchemaSignature(signature),
                    payload: Box::new(SchemaNode {
                        id: SchemaNodeId(id + 100),
                        path: format!("{path}/payload"),
                        name: "Value".to_owned(),
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

        let group_path = "TEST/0:Groups/repeat/0:Group";
        let counter_path = format!("{group_path}/0:Count");
        let value_path = format!("{group_path}/1:Values");
        let mut manifest = test_manifest();
        manifest.callbacks_total = 1;
        manifest.callbacks_classified = 1;
        manifest.required_handlers = vec![HandlerRequirement {
            id: "edit.sync_record_counts".to_owned(),
            minimum_version: 1,
        }];
        let package = SchemaPackage::new_with_callbacks(
            manifest,
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
                    kind: SchemaNodeKind::Repeat {
                        minimum: 0,
                        maximum: None,
                        child: Box::new(SchemaNode {
                            id: SchemaNodeId(1),
                            path: group_path.to_owned(),
                            name: "Group".to_owned(),
                            required: false,
                            conflict_priority: ConflictPriority::Normal,
                            condition: None,
                            kind: SchemaNodeKind::Sequence {
                                children: vec![
                                    integer_subrecord(2, &counter_path, *b"VCNT"),
                                    bytes_subrecord(3, &value_path, *b"VALU"),
                                ],
                            },
                        }),
                    },
                },
            }],
            vec![CallbackBinding {
                path: group_path.to_owned(),
                callback_id: "def.after_set".to_owned(),
                callback_slot: None,
                implementation_fingerprint: "33".repeat(32),
                implementation: CallbackImplementation::BuiltIn {
                    operation: BuiltInOperation {
                        id: "edit.sync_record_counts".to_owned(),
                        minimum_version: 1,
                        configuration: serde_json::json!({
                            "counters": [{
                                "counter_path": counter_path,
                                "counter_signature": "VCNT",
                                "counter_required": true,
                                "value_signature": "VALU",
                                "mode": "subrecord_count",
                                "only_when_missing": true,
                                "only_when_counter_exists": true
                            }]
                        }),
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
                        signature: Signature(*b"VCNT"),
                        data: 1_u32.to_le_bytes().to_vec(),
                    },
                    WritableSubRecord {
                        signature: Signature(*b"VALU"),
                        data: vec![1],
                    },
                    WritableSubRecord {
                        signature: Signature(*b"VCNT"),
                        data: 1_u32.to_le_bytes().to_vec(),
                    },
                    WritableSubRecord {
                        signature: Signature(*b"VALU"),
                        data: vec![2],
                    },
                ],
            },
            localized: false,
            after_load_migrations: 0,
            decoded_values: BTreeMap::from([
                ((counter_path.clone(), 0), FieldValue::UInt(1)),
                ((counter_path, 1), FieldValue::UInt(1)),
                (
                    (value_path.clone(), 0),
                    FieldValue::Bytes(Cow::Owned(vec![1])),
                ),
                ((value_path, 1), FieldValue::Bytes(Cow::Owned(vec![2]))),
            ]),
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

    /// Applies multiple nested callback changes without losing sibling bytes.
    #[test]
    fn callback_mutations_update_nested_fields_transactionally() -> Result<()> {
        // given
        let editor = editor_with_nested_counter()?;
        let mut record = clone_record(&editor.record);
        let mut decoded_values = editor.decoded_values.clone();

        // when
        editor.apply_mutations_with_values(
            &mut record,
            &mut decoded_values,
            vec![
                HandlerMutation::Set {
                    path: "TEST/0:IDLC/payload/0:Animation Count".to_owned(),
                    occurrence: 0,
                    value: OwnedFieldValue::UInt(7),
                },
                HandlerMutation::Set {
                    path: "TEST/0:IDLC/payload/1:Unused".to_owned(),
                    occurrence: 0,
                    value: OwnedFieldValue::Bytes(vec![9, 8, 7]),
                },
            ],
        )?;

        // then
        assert_eq!(record.subrecords[0].data, vec![7, 9, 8, 7]);
        let cached = decoded_values
            .get(&("TEST/0:IDLC".to_owned(), 0))
            .ok_or_else(|| SemanticError::MissingPath("TEST/0:IDLC".to_owned()))?;
        assert_eq!(
            handler_to_owned_value(cached.to_handler_value(), "TEST/0:IDLC")?,
            OwnedFieldValue::Struct(vec![
                OwnedFieldValue::UInt(7),
                OwnedFieldValue::Bytes(vec![9, 8, 7]),
            ])
        );
        Ok(())
    }

    /// Applies a nested conditional set only while the expected value still matches.
    #[test]
    fn conditional_mutation_preserves_changed_nested_fields() -> Result<()> {
        let editor = editor_with_nested_counter()?;
        let mut record = clone_record(&editor.record);
        let mut decoded_values = editor.decoded_values.clone();
        let path = "TEST/0:IDLC/payload/0:Animation Count";

        editor.apply_mutations_with_values(
            &mut record,
            &mut decoded_values,
            vec![HandlerMutation::SetIfEqual {
                path: path.to_owned(),
                occurrence: 0,
                expected: OwnedFieldValue::UInt(1),
                value: OwnedFieldValue::UInt(7),
            }],
        )?;
        assert_eq!(record.subrecords[0].data, vec![7, 0xaa, 0xbb, 0xcc]);

        editor.apply_mutations_with_values(
            &mut record,
            &mut decoded_values,
            vec![HandlerMutation::SetIfEqual {
                path: path.to_owned(),
                occurrence: 0,
                expected: OwnedFieldValue::UInt(1),
                value: OwnedFieldValue::UInt(9),
            }],
        )?;
        assert_eq!(record.subrecords[0].data, vec![7, 0xaa, 0xbb, 0xcc]);
        Ok(())
    }

    /// Limits container callbacks to the edited repeated grammar occurrence.
    #[test]
    fn repeated_container_callbacks_preserve_other_occurrences() -> Result<()> {
        // given
        let mut editor = editor_with_repeated_counter_groups()?;
        let value_path = "TEST/0:Groups/repeat/0:Group/1:Values";

        // when
        editor.remove(value_path, 0)?;
        let record = editor.into_writable_record();

        // then
        assert_eq!(
            record
                .subrecords
                .iter()
                .map(|subrecord| (subrecord.signature, subrecord.data.clone()))
                .collect::<Vec<_>>(),
            vec![
                (Signature(*b"VCNT"), 0_u32.to_le_bytes().to_vec()),
                (Signature(*b"VCNT"), 1_u32.to_le_bytes().to_vec()),
                (Signature(*b"VALU"), vec![2]),
            ]
        );
        Ok(())
    }

    /// Maps local callback sets to the edited repeated grammar occurrence.
    #[test]
    fn local_callback_set_targets_changed_repeat_scope() -> Result<()> {
        // given
        let editor = editor_with_repeated_counter_groups()?;
        let counter_path = "TEST/0:Groups/repeat/0:Group/0:Count";
        let mut record = clone_record(&editor.record);
        let mut decoded_values = editor.decoded_values.clone();
        let changed = editor.changed_field_at(&record, 2)?;

        // when
        editor.apply_local_mutations_with_scope(
            &mut record,
            &mut decoded_values,
            &changed,
            vec![HandlerMutation::Set {
                path: counter_path.to_owned(),
                occurrence: 0,
                value: OwnedFieldValue::UInt(7),
            }],
        )?;

        // then
        assert_eq!(record.subrecords[0].data, 1_u32.to_le_bytes());
        assert_eq!(record.subrecords[2].data, 7_u32.to_le_bytes());
        assert!(matches!(
            decoded_values.get(&(counter_path.to_owned(), 0)),
            Some(FieldValue::UInt(1))
        ));
        assert!(matches!(
            decoded_values.get(&(counter_path.to_owned(), 1)),
            Some(FieldValue::UInt(7))
        ));
        Ok(())
    }

    /// Inserts an absent callback field inside its repeated grammar occurrence.
    #[test]
    fn local_callback_presence_inserts_inside_changed_repeat_scope() -> Result<()> {
        // given
        let editor = editor_with_repeated_counter_groups()?;
        let value_path = "TEST/0:Groups/repeat/0:Group/1:Values";
        let mut record = clone_record(&editor.record);
        let mut decoded_values = editor.decoded_values.clone();
        record.subrecords.remove(1);
        remove_decoded_occurrence(&mut decoded_values, value_path, 0);
        let changed = editor.changed_field_at(&record, 0)?;

        // when
        editor.apply_local_mutations_with_scope(
            &mut record,
            &mut decoded_values,
            &changed,
            vec![HandlerMutation::SynchronizePresence {
                path: value_path.to_owned(),
                occurrence: 0,
                present: true,
                value: OwnedFieldValue::Bytes(vec![9]),
            }],
        )?;

        // then
        assert_eq!(
            record
                .subrecords
                .iter()
                .map(|subrecord| (subrecord.signature, subrecord.data.clone()))
                .collect::<Vec<_>>(),
            vec![
                (Signature(*b"VCNT"), 1_u32.to_le_bytes().to_vec()),
                (Signature(*b"VALU"), vec![9]),
                (Signature(*b"VCNT"), 1_u32.to_le_bytes().to_vec()),
                (Signature(*b"VALU"), vec![2]),
            ]
        );
        assert!(matches!(
            decoded_values.get(&(value_path.to_owned(), 0)),
            Some(FieldValue::Bytes(bytes)) if bytes.as_ref() == [9]
        ));
        assert!(matches!(
            decoded_values.get(&(value_path.to_owned(), 1)),
            Some(FieldValue::Bytes(bytes)) if bytes.as_ref() == [2]
        ));
        Ok(())
    }

    /// Removes callback fields only from the changed repeated grammar occurrence.
    #[test]
    fn local_callback_remove_all_preserves_other_repeat_scopes() -> Result<()> {
        // given
        let editor = editor_with_repeated_counter_groups()?;
        let value_path = "TEST/0:Groups/repeat/0:Group/1:Values";
        let mut record = clone_record(&editor.record);
        let mut decoded_values = editor.decoded_values.clone();
        let changed = editor.changed_field_at(&record, 0)?;

        // when
        editor.apply_local_mutations_with_scope(
            &mut record,
            &mut decoded_values,
            &changed,
            vec![HandlerMutation::RemoveAll {
                path: value_path.to_owned(),
            }],
        )?;

        // then
        assert_eq!(
            record
                .subrecords
                .iter()
                .map(|subrecord| (subrecord.signature, subrecord.data.clone()))
                .collect::<Vec<_>>(),
            vec![
                (Signature(*b"VCNT"), 1_u32.to_le_bytes().to_vec()),
                (Signature(*b"VCNT"), 1_u32.to_le_bytes().to_vec()),
                (Signature(*b"VALU"), vec![2]),
            ]
        );
        assert!(matches!(
            decoded_values.get(&(value_path.to_owned(), 0)),
            Some(FieldValue::Bytes(bytes)) if bytes.as_ref() == [2]
        ));
        assert!(!decoded_values.contains_key(&(value_path.to_owned(), 1)));
        Ok(())
    }

    /// Inserts, updates, and removes optional counters in schema order.
    #[test]
    fn editor_synchronizes_optional_counter_transactionally() -> Result<()> {
        let editor = editor_with_reused_signature()?;
        let mut record = clone_record(&editor.record);
        let mut decoded_values = editor.decoded_values.clone();

        editor.apply_mutations_with_values(
            &mut record,
            &mut decoded_values,
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

        editor.apply_mutations_with_values(
            &mut record,
            &mut decoded_values,
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

    /// Synchronizes optional field presence without duplicates or missing-field errors.
    #[test]
    fn editor_synchronizes_optional_presence_idempotently() -> Result<()> {
        let editor = editor_with_reused_signature()?;
        let mut record = clone_record(&editor.record);
        let mut decoded_values = editor.decoded_values.clone();
        let present = HandlerMutation::SynchronizePresence {
            path: "TEST/2:Value Count".to_owned(),
            occurrence: 0,
            present: true,
            value: OwnedFieldValue::UInt(0),
        };

        editor.apply_mutations_with_values(
            &mut record,
            &mut decoded_values,
            vec![present.clone(), present],
        )?;
        assert_eq!(
            record
                .subrecords
                .iter()
                .filter(|subrecord| subrecord.signature == Signature(*b"VCNT"))
                .count(),
            1
        );

        let absent = HandlerMutation::SynchronizePresence {
            path: "TEST/2:Value Count".to_owned(),
            occurrence: 0,
            present: false,
            value: OwnedFieldValue::UInt(0),
        };
        editor.apply_mutations_with_values(
            &mut record,
            &mut decoded_values,
            vec![absent.clone(), absent],
        )?;
        assert!(record
            .subrecords
            .iter()
            .all(|subrecord| subrecord.signature != Signature(*b"VCNT")));
        Ok(())
    }

    /// Removes every grammar-assigned occurrence of a repeated subrecord.
    #[test]
    fn editor_removes_all_repeated_subrecords_transactionally() -> Result<()> {
        let editor = editor_with_repeated_counter_groups()?;
        let mut record = clone_record(&editor.record);
        let mut decoded_values = editor.decoded_values.clone();

        editor.apply_mutations_with_values(
            &mut record,
            &mut decoded_values,
            vec![HandlerMutation::RemoveAll {
                path: "TEST/0:Groups/repeat/0:Group/1:Values".to_owned(),
            }],
        )?;

        assert!(record
            .subrecords
            .iter()
            .all(|subrecord| subrecord.signature != Signature(*b"VALU")));
        assert_eq!(
            record
                .subrecords
                .iter()
                .filter(|subrecord| subrecord.signature == Signature(*b"VCNT"))
                .count(),
            2
        );
        Ok(())
    }

    /// Dispatches record callbacks after removal and clears a populated optional counter.
    #[test]
    fn editor_dispatches_record_after_set_callbacks() -> Result<()> {
        let mut editor = editor_with_reused_signature()?;

        editor.record.subrecords.insert(
            2,
            WritableSubRecord {
                signature: Signature(*b"VCNT"),
                data: 2_u32.to_le_bytes().to_vec(),
            },
        );

        editor.remove("TEST/3:Values", 0)?;
        assert_eq!(
            editor
                .record
                .subrecords
                .iter()
                .map(|subrecord| subrecord.signature)
                .collect::<Vec<_>>(),
            vec![
                Signature(*b"AAAA"),
                Signature(*b"AAAA"),
                Signature(*b"VCNT")
            ]
        );
        assert_eq!(editor.record.subrecords[2].data, 0_u32.to_le_bytes());
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
            effective_path: None,
            name: "Mode".to_owned(),
            span: crate::ByteSpan { start: 0, end: 4 },
            value: FieldValue::Int(2),
        }]);
        let decoded_shape = FieldValue::Struct(vec![crate::NamedValue {
            node_id: bethkit_schema::SchemaNodeId(1),
            path: "TEST/0:Mode".to_owned(),
            effective_path: None,
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
