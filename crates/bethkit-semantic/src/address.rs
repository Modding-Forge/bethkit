// SPDX-License-Identifier: Apache-2.0
//! Structural addresses derived from actual grammar matches and decoded value trees.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{Field, FieldValue};

/// One locally numbered repetition of a grammar child.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepeatScope {
    /// Stable path of the repeated child, not its enclosing repeat node.
    pub path: String,
    /// Zero-based occurrence within its complete parent repeat stack.
    pub occurrence: u32,
}

/// One navigation step inside a decoded subrecord payload.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ValueStep {
    /// Select a named packed-struct member with verified schema identity.
    Field {
        /// Zero-based member index in decoded declaration order.
        index: usize,
        /// Stable path expected at this index.
        path: String,
    },
    /// Select one item of an array.
    Index {
        /// Zero-based index in the containing array.
        index: usize,
    },
}

/// An exact address issued by a decoded snapshot.
///
/// Addresses contain no editable values. Structural edits invalidate addresses
/// through `structure_hash`; callers must obtain a new snapshot before editing
/// again. Scalar edits with unchanged layout keep their addresses valid.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldAddress {
    /// SHA-256 of the schema package payload used to interpret this address.
    pub schema_payload_sha256: String,
    /// SHA-256 of the decoded record structure used to issue this address.
    pub structure_hash: String,
    /// Main-record signature as four literal bytes.
    pub record_signature: [u8; 4],
    /// File-local main-record FormID.
    pub form_id: u32,
    /// Exact subrecord index in the record payload.
    pub subrecord_index: usize,
    /// Stable schema path of the containing subrecord.
    pub subrecord_path: String,
    /// Complete outermost-to-innermost grammar repeat stack.
    pub repeat_scopes: Vec<RepeatScope>,
    /// Navigation inside the containing subrecord; empty selects its whole value.
    pub value_steps: Vec<ValueStep>,
}

/// Encodes a SHA-256 digest as 64 lowercase hexadecimal characters.
pub fn schema_hash_hex(digest: &[u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    digest
        .iter()
        .flat_map(|byte| {
            [
                char::from(HEX[usize::from(byte >> 4)]),
                char::from(HEX[usize::from(byte & 15)]),
            ]
        })
        .collect()
}

/// Hashes exact grammar assignments, repeat scopes and value-tree shape.
///
/// Scalar values are deliberately excluded so independent value edits do not
/// invalidate each other's addresses. Byte offsets and scalar lengths are excluded
/// because navigation uses schema members, not byte positions. Selected union paths
/// remain part of the structural identity, including each nested array occurrence.
pub fn structure_hash(fields: &[Field<'_>]) -> String {
    let mut digest = Sha256::new();
    digest.update(b"bethkit-field-address-v1");
    hash_usize(&mut digest, fields.len());
    for field in fields {
        hash_usize(&mut digest, field.subrecord_index);
        hash_text(&mut digest, &field.path);
        hash_text(&mut digest, field.effective_path.as_deref().unwrap_or(""));
        digest.update(field.node_id.0.to_le_bytes());
        digest.update(field.subrecord_signature.0);
        hash_usize(&mut digest, field.repeat_scopes.len());
        for scope in &field.repeat_scopes {
            hash_text(&mut digest, &scope.path);
            digest.update(scope.occurrence.to_le_bytes());
        }
        hash_usize(&mut digest, field.value_selections.len());
        for selection in &field.value_selections {
            hash_text(&mut digest, &selection.schema_path);
            hash_usize(&mut digest, selection.array_indices.len());
            for index in &selection.array_indices {
                hash_usize(&mut digest, *index);
            }
            hash_text(&mut digest, &selection.effective_path);
        }
        hash_value(&mut digest, &field.value);
    }
    schema_hash_hex(&digest.finalize().into())
}

fn hash_usize(digest: &mut Sha256, value: usize) {
    digest.update((value as u64).to_le_bytes());
}

fn hash_text(digest: &mut Sha256, value: &str) {
    hash_usize(digest, value.len());
    digest.update(value.as_bytes());
}

fn hash_value(digest: &mut Sha256, value: &FieldValue<'_>) {
    let kind = match value {
        FieldValue::Int(_) => 0,
        FieldValue::UInt(_) => 1,
        FieldValue::Float(_) => 2,
        FieldValue::String(_) => 3,
        FieldValue::FormId { .. } => 4,
        FieldValue::Enumeration { .. } => 5,
        FieldValue::Flags { .. } => 6,
        FieldValue::Bytes(_) => 7,
        FieldValue::Struct(_) => 8,
        FieldValue::Array(_) => 9,
        FieldValue::Absent => 10,
    };
    digest.update([kind]);
    match value {
        FieldValue::Struct(fields) => {
            hash_usize(digest, fields.len());
            for field in fields {
                hash_text(digest, &field.path);
                digest.update(field.node_id.0.to_le_bytes());
                hash_text(digest, field.effective_path.as_deref().unwrap_or(""));
                hash_value(digest, &field.value);
            }
        }
        FieldValue::Array(values) => {
            hash_usize(digest, values.len());
            for item in values {
                hash_value(digest, item);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use bethkit_core::Signature;
    use bethkit_schema::SchemaNodeId;

    use super::structure_hash;
    use crate::{ByteSpan, Field, FieldOrigin, FieldValue, ValueSelection};

    fn selected_field() -> Field<'static> {
        Field {
            subrecord_index: 0,
            repeat_scopes: Vec::new(),
            node_id: SchemaNodeId(1),
            path: "TEST/data".to_owned(),
            effective_path: None,
            value_selections: vec![ValueSelection {
                schema_path: "TEST/data/items/item".to_owned(),
                array_indices: vec![0, 1],
                effective_path: "TEST/data/items/item/first".to_owned(),
            }],
            name: "Data".to_owned(),
            subrecord_signature: Signature(*b"DATA"),
            occurrence: 0,
            span: ByteSpan { start: 0, end: 4 },
            origin: FieldOrigin::Schema,
            value: FieldValue::Array(vec![FieldValue::String(Cow::Borrowed("old"))]),
        }
    }

    /// Distinguishes every selected union occurrence without hashing editable scalar text.
    #[test]
    fn union_selection_identity_is_structural() -> Result<(), Box<dyn std::error::Error>> {
        // given
        let mut fields = [selected_field()];
        let original = structure_hash(&fields);

        // when
        fields[0].value = FieldValue::Array(vec![FieldValue::String(Cow::Borrowed(
            "a substantially longer replacement",
        ))]);
        fields[0].span.end = 36;

        // then
        assert_eq!(structure_hash(&fields), original);
        fields[0].value_selections[0].effective_path = "TEST/data/items/item/second".to_owned();
        assert_ne!(structure_hash(&fields), original);
        fields[0].value_selections[0].effective_path = "TEST/data/items/item/first".to_owned();
        fields[0].value_selections[0].array_indices = vec![1, 0];
        assert_ne!(structure_hash(&fields), original);
        fields[0].value_selections[0].array_indices = vec![0, 1];
        fields[0].value_selections[0].schema_path = "TEST/data/other/item".to_owned();
        assert_ne!(structure_hash(&fields), original);
        fields[0].value_selections.clear();
        assert_ne!(structure_hash(&fields), original);
        Ok(())
    }
}
