// SPDX-License-Identifier: Apache-2.0
//! Exact structural editing over grammar-issued field addresses.

use std::sync::Arc;

use bethkit_core::{Record, WritableSubRecord};
use bethkit_schema::{SchemaNodeKind, SchemaPackage};

use super::{clone_record, insert_decoded_occurrence, RecordEditor};
use crate::value::handler_to_owned_value;
use crate::{
    schema_hash_hex, structure_hash, Field, FieldAddress, FieldValue, OwnedFieldValue, Result,
    SemanticContext, SemanticError, ValueStep,
};

impl RecordEditor {
    /// Returns the immutable schema package used by this editor.
    pub fn schema_package(&self) -> &Arc<SchemaPackage> {
        self.registry.package()
    }

    /// Returns the record signature and file-local FormID of the edited record.
    pub fn identity(&self) -> (bethkit_core::Signature, bethkit_core::FormId) {
        (self.record.signature, self.record.form_id)
    }

    /// Decodes an owned snapshot of the editor's current record state.
    ///
    /// # Errors
    ///
    /// Returns a semantic error if the current record cannot be decoded.
    pub fn fields(&self) -> Result<Vec<Field<'static>>> {
        let context = SemanticContext::new_with_handlers(
            self.registry.package().clone(),
            self.decoders.clone(),
            self.handlers.clone(),
        )?;
        let record = Record::from_writable(&self.record);
        context
            .view(&record, self.localized)?
            .fields()?
            .into_iter()
            .map(|field| {
                Ok(Field {
                    subrecord_index: field.subrecord_index,
                    repeat_scopes: field.repeat_scopes,
                    node_id: field.node_id,
                    path: field.path,
                    effective_path: field.effective_path,
                    name: field.name,
                    subrecord_signature: field.subrecord_signature,
                    occurrence: field.occurrence,
                    span: field.span,
                    origin: field.origin,
                    value: field.value.to_handler_value(),
                })
            })
            .collect()
    }

    /// Replaces exactly one value selected by a current structural address.
    ///
    /// Replacing a nested value re-encodes its containing subrecord through the
    /// normal semantic transaction, including xEdit after-set callbacks.
    ///
    /// # Errors
    ///
    /// Returns a semantic error for stale addresses, mismatched identity, invalid
    /// nested navigation, or a value that violates the schema.
    pub fn set_at(&mut self, address: &FieldAddress, value: &OwnedFieldValue) -> Result<()> {
        let field = self.addressed_field(address)?;
        let mut updated = handler_to_owned_value(field.value.clone(), &field.path)?;
        *owned_target(
            &mut updated,
            &field.value,
            &address.value_steps,
            &field.path,
        )? = value.clone();
        self.set(&field.path, field.occurrence, &updated)
    }

    /// Removes an addressed subrecord, array item, or optional struct value.
    ///
    /// Struct removal is encoded as an absent optional value. Required members
    /// cannot be removed. Array removal is committed through the containing
    /// subrecord's normal semantic transaction.
    ///
    /// # Errors
    ///
    /// Returns a semantic error for stale addresses or schema-invalid removal.
    pub fn remove_at(&mut self, address: &FieldAddress) -> Result<()> {
        let field = self.addressed_field(address)?;
        let Some((last, prefix)) = address.value_steps.split_last() else {
            return self.remove(&field.path, field.occurrence);
        };
        let mut updated = handler_to_owned_value(field.value.clone(), &field.path)?;
        let parent = owned_target(&mut updated, &field.value, prefix, &field.path)?;
        match (last, parent) {
            (ValueStep::Index { index }, OwnedFieldValue::Array(items)) if *index < items.len() => {
                items.remove(*index);
            }
            (ValueStep::Field { .. }, _) => {
                *owned_target(
                    &mut updated,
                    &field.value,
                    &address.value_steps,
                    &field.path,
                )? = OwnedFieldValue::Absent;
            }
            _ => {
                return Err(address_error(
                    &field.path,
                    "address does not select a removable item",
                ))
            }
        }
        self.set(&field.path, field.occurrence, &updated)
    }

    /// Inserts before an addressed array item or appends to an addressed array.
    ///
    /// A whole non-array subrecord address inserts another occurrence immediately
    /// before that subrecord, only if grammar matching preserves all existing
    /// assignments. Compound repeated groups require a complete group transaction
    /// and are not synthesized from an individual field.
    ///
    /// # Errors
    ///
    /// Returns a semantic error for stale addresses, non-insertable targets, or
    /// an insertion rejected by the schema or callbacks.
    pub fn insert_at(&mut self, address: &FieldAddress, value: &OwnedFieldValue) -> Result<()> {
        let field = self.addressed_field(address)?;
        let mut updated = handler_to_owned_value(field.value.clone(), &field.path)?;
        let target = owned_target(
            &mut updated,
            &field.value,
            &address.value_steps,
            &field.path,
        )?;
        if let OwnedFieldValue::Array(items) = target {
            items.push(value.clone());
        } else if let Some((ValueStep::Index { index }, prefix)) = address.value_steps.split_last()
        {
            let parent = owned_target(&mut updated, &field.value, prefix, &field.path)?;
            let OwnedFieldValue::Array(items) = parent else {
                return Err(address_error(&field.path, "array item has no array parent"));
            };
            if *index > items.len() {
                return Err(address_error(
                    &field.path,
                    "array insertion index is out of bounds",
                ));
            }
            items.insert(*index, value.clone());
        } else if address.value_steps.is_empty() {
            return self.insert_before_addressed_field(&field, value);
        } else {
            return Err(address_error(
                &field.path,
                "insertion requires an array or subrecord",
            ));
        }
        self.set(&field.path, field.occurrence, &updated)
    }

    fn addressed_field(&self, address: &FieldAddress) -> Result<Field<'static>> {
        if address.record_signature != self.record.signature.0
            || address.form_id != self.record.form_id.0
            || address.schema_payload_sha256
                != schema_hash_hex(&self.registry.package().payload_sha256())
        {
            return Err(address_error(
                &address.subrecord_path,
                "address belongs to another record or schema",
            ));
        }
        let fields = self.fields()?;
        if address.structure_hash != structure_hash(&fields) {
            return Err(address_error(
                &address.subrecord_path,
                "stale address; refresh the record snapshot",
            ));
        }
        fields
            .into_iter()
            .find(|field| {
                field.subrecord_index == address.subrecord_index
                    && field.path == address.subrecord_path
                    && field.repeat_scopes == address.repeat_scopes
            })
            .ok_or_else(|| {
                address_error(
                    &address.subrecord_path,
                    "address does not match a grammar assignment",
                )
            })
    }

    fn insert_before_addressed_field(
        &mut self,
        field: &Field<'_>,
        value: &OwnedFieldValue,
    ) -> Result<()> {
        let node = self.find_node(&field.path)?;
        let SchemaNodeKind::Subrecord { signature, payload } = &node.kind else {
            return Err(address_error(
                &field.path,
                "target is not a schema subrecord",
            ));
        };
        let index = field.subrecord_index;
        let normalized = self.normalize_value(&payload.path, value)?;
        let (normalized, mutations) =
            self.apply_after_set_tree(payload, &normalized, None, Some(index))?;
        let encoded = self.encode_node_at(payload, &normalized, Some(index))?;
        let decoded = self.owned_to_handler_value_at(payload, &normalized, Some(index))?;
        let mut candidate = clone_record(&self.record);
        candidate.subrecords.insert(
            index,
            WritableSubRecord {
                signature: (*signature).into(),
                data: encoded,
            },
        );
        let before = self.grammar_for(&self.record)?;
        let after = self.grammar_for(&candidate)?;
        for (original_index, assignment) in before.assignments.iter().enumerate() {
            let updated_index = original_index + usize::from(original_index >= index);
            if assignment.map(|node| node.id)
                != after.assignments[updated_index].map(|node| node.id)
            {
                return Err(address_error(
                    &field.path,
                    "insertion would change an existing grammar assignment",
                ));
            }
        }
        let changed = self.changed_field_at(&candidate, index)?;
        let mut decoded_values = self.decoded_values.clone();
        insert_decoded_occurrence(&mut decoded_values, &field.path, field.occurrence, decoded);
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
}

fn address_error(path: &str, message: &str) -> SemanticError {
    SemanticError::Encode {
        path: path.to_owned(),
        message: message.to_owned(),
    }
}

fn owned_target<'a>(
    owned: &'a mut OwnedFieldValue,
    decoded: &FieldValue<'_>,
    steps: &[ValueStep],
    context: &str,
) -> Result<&'a mut OwnedFieldValue> {
    let Some((step, remaining)) = steps.split_first() else {
        return Ok(owned);
    };
    match (step, owned, decoded) {
        (
            ValueStep::Field { index, path },
            OwnedFieldValue::Struct(values),
            FieldValue::Struct(fields),
        ) => {
            let field = fields
                .get(*index)
                .filter(|field| field.path == *path)
                .ok_or_else(|| {
                    address_error(context, "struct step does not match decoded schema path")
                })?;
            let value = values
                .get_mut(*index)
                .ok_or_else(|| address_error(context, "struct index is out of bounds"))?;
            owned_target(value, &field.value, remaining, context)
        }
        (ValueStep::Index { index }, OwnedFieldValue::Array(values), FieldValue::Array(items)) => {
            let item = items
                .get(*index)
                .ok_or_else(|| address_error(context, "array index is out of bounds"))?;
            let value = values
                .get_mut(*index)
                .ok_or_else(|| address_error(context, "array index is out of bounds"))?;
            owned_target(value, item, remaining, context)
        }
        _ => Err(address_error(
            context,
            "address step does not match decoded value kind",
        )),
    }
}
