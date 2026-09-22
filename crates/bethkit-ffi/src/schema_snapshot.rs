// SPDX-License-Identifier: Apache-2.0
//! Owned JSON snapshots and exact structural editing for language bindings.

use std::ffi::c_char;

use bethkit_core::{resolve_string_kind, Signature, StringFileKind};
use bethkit_schema::{PrimitiveType, SchemaNode, SchemaNodeKind, SchemaRegistry};
use bethkit_semantic::{
    schema_hash_hex, structure_hash, Field, FieldAddress, FieldOrigin, FieldValue, OwnedFieldValue,
    ValueSelection, ValueStep,
};
use serde_json::{json, Value};

use super::{owned_json, BethkitRecordEditor, BethkitSemanticContext};
use crate::error::FfiError;
use crate::record::BethkitRecord;
use crate::{cstr_to_str, ffi_try, null_check, Result};

/// Returns an owned, structurally addressed snapshot as a UTF-8 JSON document.
///
/// Borrow `context` and `record` for this call and set `localized` from the plugin.
/// Free the result with `bethkit_string_free`. The document contains version 1,
/// schema and structure hashes, record identity, and ordered fields. Every value,
/// including array items, carries its exact native address.
///
/// # Errors
///
/// Returns null and sets the last error for null handles, decoding failure, or
/// serialization failure.
///
/// # Safety
///
/// Non-null handles must remain valid for this call.
#[no_mangle]
pub extern "C" fn bethkit_semantic_snapshot_json(
    context: *const BethkitSemanticContext,
    record: *const BethkitRecord,
    localized: bool,
) -> *mut c_char {
    null_check!(
        context,
        "bethkit_semantic_snapshot_json/context",
        std::ptr::null_mut()
    );
    null_check!(
        record,
        "bethkit_semantic_snapshot_json/record",
        std::ptr::null_mut()
    );
    ffi_try!(
        (|| -> Result<*mut c_char> {
            // SAFETY: context is non-null and valid by the caller contract.
            let context = unsafe { &*context };
            // SAFETY: record is non-null and valid by the caller contract.
            let record = unsafe { &*record };
            let fields = context.0.view(&record.0, localized)?.fields()?;
            owned_json(&snapshot_document(
                context.0.registry(),
                &fields,
                record.0.header.signature,
                record.0.header.form_id.0,
                localized,
                false,
            ))
        })(),
        std::ptr::null_mut()
    )
}

/// Returns an owned JSON snapshot of an editor's current normalized record state.
///
/// Free the result with `bethkit_string_free`. Obtain fresh addresses after a
/// structural edit; addresses from before after-load normalization may be stale.
///
/// # Errors
///
/// Returns null and sets the last error for null or consumed editors, decoding
/// failure, or serialization failure.
///
/// # Safety
///
/// A non-null editor must remain valid for this call.
#[no_mangle]
pub extern "C" fn bethkit_record_editor_snapshot_json(
    editor: *const BethkitRecordEditor,
) -> *mut c_char {
    null_check!(
        editor,
        "bethkit_record_editor_snapshot_json",
        std::ptr::null_mut()
    );
    ffi_try!(
        (|| -> Result<*mut c_char> {
            // SAFETY: editor is non-null and valid by the caller contract.
            let editor = unsafe { &*editor }
                .0
                .as_ref()
                .ok_or(FfiError::WriterConsumed)?;
            let fields = editor.fields()?;
            let registry = SchemaRegistry::new(editor.schema_package().clone());
            let (signature, form_id) = editor.identity();
            owned_json(&snapshot_document(
                &registry,
                &fields,
                signature,
                form_id.0,
                editor.is_localized(),
                false,
            ))
        })(),
        std::ptr::null_mut()
    )
}

fn snapshot_document(
    registry: &SchemaRegistry,
    fields: &[Field<'_>],
    signature: Signature,
    form_id: u32,
    localized: bool,
    strings_only: bool,
) -> Value {
    let schema_hash = schema_hash_hex(&registry.package().payload_sha256());
    let structure_hash = structure_hash(fields);
    let fields: Vec<Value> = fields
        .iter()
        .filter_map(|field| {
            let address = FieldAddress {
                schema_payload_sha256: schema_hash.clone(),
                structure_hash: structure_hash.clone(),
                record_signature: signature.0,
                form_id,
                subrecord_index: field.subrecord_index,
                subrecord_path: field.path.clone(),
                repeat_scopes: field.repeat_scopes.clone(),
                value_steps: Vec::new(),
            };
            let origin = origin_name(field.origin);
            let node = registry.get_node(
                signature,
                field.effective_path.as_deref().unwrap_or(&field.path),
            );
            let context = WireContext {
                registry,
                selections: &field.value_selections,
                subrecord_signature: field.subrecord_signature,
                localized,
                origin,
            };
            let value = if strings_only {
                wire_strings(&field.value, &address, node, &context)?
            } else {
                wire_value(&field.value, &address, node, &context)
            };
            Some(json!({
                "name": field.name, "node_id": field.node_id.0, "path": field.path,
                "effective_path": field.effective_path,
                "span": {"start": field.span.start, "end": field.span.end},
                "origin": origin, "address": address,
                "value": value,
            }))
        })
        .collect();
    json!({"format_version":1, "schema_payload_sha256":schema_hash,
        "structure_hash":structure_hash, "record_signature":signature.to_string(),
        "form_id":form_id, "fields":fields,
        "projection":if strings_only {"strings"} else {"full"}})
}

/// Returns only schema-declared translatable values as an owned JSON snapshot.
///
/// Borrows both handles during the call. Free the result with `bethkit_string_free`.
/// The document has `projection: "strings"`; non-string branches are omitted before
/// JSON serialization. Surviving leaves retain exact full-snapshot addresses and the
/// structure hash covers the complete record, including omitted branches. This is not
/// a complete record model and must not be used to reconstruct one.
///
/// # Errors
///
/// Returns null and sets the last error for null handles, decoding/serialization
/// failures, or internal panics.
///
/// # Safety
///
/// Both handles must be live for this call. `localized` must match the source plugin.
#[no_mangle]
pub extern "C" fn bethkit_semantic_strings_snapshot_json(
    context: *const BethkitSemanticContext,
    record: *const BethkitRecord,
    localized: bool,
) -> *mut c_char {
    null_check!(
        context,
        "bethkit_semantic_strings_snapshot_json/context",
        std::ptr::null_mut()
    );
    null_check!(
        record,
        "bethkit_semantic_strings_snapshot_json/record",
        std::ptr::null_mut()
    );
    ffi_try!(
        (|| -> Result<*mut c_char> {
            // SAFETY: context is a live borrowed semantic context.
            let context = unsafe { &*context };
            // SAFETY: record is live and borrowed for this call.
            let record = unsafe { &*record };
            let fields = context.0.view(&record.0, localized)?.fields()?;
            owned_json(&snapshot_document(
                context.0.registry(),
                &fields,
                record.0.header.signature,
                record.0.header.form_id.0,
                localized,
                true,
            ))
        })(),
        std::ptr::null_mut()
    )
}

/// Returns an editor's translatable values with exact current structural addresses.
///
/// The owned JSON uses `projection: "strings"`, omits non-string branches, and must
/// be freed with `bethkit_string_free`. Hashes still cover the complete record.
/// Obtain new addresses after structural edits or after-load normalization.
///
/// # Errors
///
/// Returns null and sets the last error for null/consumed editors, decoding or
/// serialization failures, or internal panics.
///
/// # Safety
///
/// `editor` must be a live borrowed editor handle throughout the call.
#[no_mangle]
pub extern "C" fn bethkit_record_editor_strings_snapshot_json(
    editor: *const BethkitRecordEditor,
) -> *mut c_char {
    null_check!(
        editor,
        "bethkit_record_editor_strings_snapshot_json",
        std::ptr::null_mut()
    );
    ffi_try!(
        (|| -> Result<*mut c_char> {
            // SAFETY: editor is a live borrowed handle.
            let editor = unsafe { &*editor }
                .0
                .as_ref()
                .ok_or(FfiError::WriterConsumed)?;
            let fields = editor.fields()?;
            let registry = SchemaRegistry::new(editor.schema_package().clone());
            let (signature, form_id) = editor.identity();
            owned_json(&snapshot_document(
                &registry,
                &fields,
                signature,
                form_id.0,
                editor.is_localized(),
                true,
            ))
        })(),
        std::ptr::null_mut()
    )
}

fn origin_name(origin: FieldOrigin) -> &'static str {
    match origin {
        FieldOrigin::Schema => "schema",
        FieldOrigin::UnknownSubrecord => "unknown_subrecord",
        FieldOrigin::UnmatchedKnownSubrecord => "unmatched_known_subrecord",
        FieldOrigin::CustomDecoder => "custom_decoder",
    }
}

struct WireContext<'a> {
    registry: &'a SchemaRegistry,
    selections: &'a [ValueSelection],
    subrecord_signature: Signature,
    localized: bool,
    origin: &'a str,
}

fn selected_payload_node<'a>(
    context: &WireContext<'a>,
    address: &FieldAddress,
    mut node: &'a SchemaNode,
) -> &'a SchemaNode {
    let signature = Signature(address.record_signature);
    for _ in 0..128 {
        node = payload_node(context.registry, signature, node);
        if !matches!(node.kind, SchemaNodeKind::Union { .. }) {
            return node;
        }
        // Equal-shaped scalar variants cannot be reconstructed from their values.
        let selection = context.selections.iter().find(|selection| {
            selection.schema_path == node.path
                && selection.array_indices.iter().copied().eq(address
                    .value_steps
                    .iter()
                    .filter_map(|step| match step {
                        ValueStep::Index { index } => Some(*index),
                        ValueStep::Field { .. } => None,
                    }))
        });
        let Some(selected) = selection.and_then(|selection| {
            context
                .registry
                .get_node(signature, &selection.effective_path)
        }) else {
            return node;
        };
        node = selected;
    }
    node
}

fn payload_node<'a>(
    registry: &'a SchemaRegistry,
    signature: Signature,
    mut node: &'a SchemaNode,
) -> &'a SchemaNode {
    for _ in 0..128 {
        match &node.kind {
            SchemaNodeKind::Subrecord { payload, .. } => node = payload,
            SchemaNodeKind::Compressed { child, .. } | SchemaNodeKind::Terminated { child, .. } => {
                node = child
            }
            SchemaNodeKind::Reference { target } => {
                let Some(target) = registry.get_node(signature, target) else {
                    return node;
                };
                node = target;
            }
            _ => return node,
        }
    }
    node
}

fn is_translatable(node: &SchemaNode) -> bool {
    matches!(&node.kind,
        SchemaNodeKind::Primitive { primitive: PrimitiveType::String { string } }
        if string.localized || string.encoding == "localized")
}

fn can_contain_strings(
    registry: &SchemaRegistry,
    signature: Signature,
    node: &SchemaNode,
    depth: usize,
) -> bool {
    if depth > 128 {
        return true;
    }
    let children = match &node.kind {
        SchemaNodeKind::Primitive { .. } => return is_translatable(node),
        SchemaNodeKind::Struct { fields } | SchemaNodeKind::OptionalStruct { fields, .. } => fields,
        SchemaNodeKind::Sequence { children } | SchemaNodeKind::Unordered { children } => children,
        SchemaNodeKind::Union { variants, .. } => variants,
        SchemaNodeKind::Choice { alternatives }
        | SchemaNodeKind::SelectedChoice { alternatives, .. } => alternatives,
        SchemaNodeKind::Array { element: child, .. }
        | SchemaNodeKind::Repeat { child, .. }
        | SchemaNodeKind::Subrecord { payload: child, .. }
        | SchemaNodeKind::Compressed { child, .. }
        | SchemaNodeKind::Terminated { child, .. } => {
            return can_contain_strings(registry, signature, child, depth + 1);
        }
        SchemaNodeKind::Reference { target } => {
            return registry
                .get_node(signature, target)
                .is_none_or(|target| can_contain_strings(registry, signature, target, depth + 1));
        }
        SchemaNodeKind::Custom { .. } => return true,
    };
    children
        .iter()
        .any(|child| can_contain_strings(registry, signature, child, depth + 1))
}

fn wire_strings(
    value: &FieldValue<'_>,
    address: &FieldAddress,
    node: Option<&SchemaNode>,
    context: &WireContext<'_>,
) -> Option<Value> {
    let signature = Signature(address.record_signature);
    let node = node.map(|node| selected_payload_node(context, address, node));
    if node.is_some_and(|node| !can_contain_strings(context.registry, signature, node, 0)) {
        return None;
    }
    let mut result = match value {
        FieldValue::Struct(fields) => {
            let selected: Vec<Value> = fields
                .iter()
                .enumerate()
                .filter_map(|(index, field)| {
                    let mut address = address.clone();
                    address.value_steps.push(ValueStep::Field {
                        index,
                        path: field.path.clone(),
                    });
                    let node = context.registry.get_node(
                        signature,
                        field.effective_path.as_deref().unwrap_or(&field.path),
                    );
                    let value = wire_strings(&field.value, &address, node, context)?;
                    Some(
                        json!({"name":field.name,"node_id":field.node_id.0,"path":field.path,
                    "effective_path":field.effective_path,
                    "span":{"start":field.span.start,"end":field.span.end},
                    "origin":context.origin,"address":address,"value":value}),
                    )
                })
                .collect();
            if selected.is_empty() {
                return None;
            }
            json!({"kind":"struct","fields":selected})
        }
        FieldValue::Array(values) => {
            let element = node.and_then(|node| match &node.kind {
                SchemaNodeKind::Array { element, .. } => Some(element.as_ref()),
                _ => None,
            });
            let items: Vec<Value> = values
                .iter()
                .enumerate()
                .filter_map(|(index, value)| {
                    let mut address = address.clone();
                    address.value_steps.push(ValueStep::Index { index });
                    wire_strings(value, &address, element, context)
                })
                .collect();
            if items.is_empty() {
                return None;
            }
            json!({"kind":"array","items":items})
        }
        _ if node.is_some_and(is_translatable) => {
            return Some(wire_value(value, address, node, context));
        }
        _ => return None,
    };
    result["address"] = json!(address);
    result["schema_path"] = json!(node.map(|node| node.path.as_str()));
    result["translatable"] = json!(false);
    result["string_table"] = Value::Null;
    Some(result)
}

fn wire_value(
    value: &FieldValue<'_>,
    address: &FieldAddress,
    node: Option<&SchemaNode>,
    context: &WireContext<'_>,
) -> Value {
    let signature = Signature(address.record_signature);
    let node = node.map(|node| selected_payload_node(context, address, node));
    let translatable = node.is_some_and(is_translatable);
    let string_table = (translatable && context.localized).then(|| {
        match resolve_string_kind(signature, context.subrecord_signature) {
            StringFileKind::Strings => "strings",
            StringFileKind::DLStrings => "dl_strings",
            StringFileKind::ILStrings => "il_strings",
        }
    });
    let mut output = match value {
        FieldValue::Int(value) => json!({"kind":"int", "value":value}),
        FieldValue::UInt(value) => json!({"kind":"uint", "value":value}),
        FieldValue::Float(value) => {
            let number = if value.is_finite() {
                json!(value)
            } else if value.is_nan() {
                json!("NaN")
            } else if value.is_sign_positive() {
                json!("Infinity")
            } else {
                json!("-Infinity")
            };
            json!({"kind":"float", "value":number})
        }
        FieldValue::String(value) => json!({"kind":"string", "value":value}),
        FieldValue::FormId { value, targets } => json!({"kind":"form_id", "value":value.0,
            "allowed_signatures":targets.iter().map(|sig| sig.0).collect::<Vec<_>>()}),
        FieldValue::Bytes(value) => json!({"kind":"bytes", "value":value}),
        FieldValue::Enumeration { value, name } => json!({"kind":"enum", "value":value,
            "names":name.iter().collect::<Vec<_>>()}),
        FieldValue::Flags { value, active } => {
            json!({"kind":"flags", "value":value, "names":active})
        }
        FieldValue::Absent => json!({"kind":"absent", "value":null}),
        FieldValue::Struct(fields) => {
            let fields: Vec<Value> = fields
                .iter()
                .enumerate()
                .map(|(index, field)| {
                    let mut address = address.clone();
                    address.value_steps.push(ValueStep::Field {
                        index,
                        path: field.path.clone(),
                    });
                    let node = context.registry.get_node(
                        signature,
                        field.effective_path.as_deref().unwrap_or(&field.path),
                    );
                    json!({"name":field.name, "node_id":field.node_id.0, "path":field.path,
                    "effective_path":field.effective_path,
                    "span":{"start":field.span.start,"end":field.span.end},
                    "origin":context.origin, "address":address,
                    "value":wire_value(&field.value, &address, node, context)})
                })
                .collect();
            json!({"kind":"struct", "fields":fields})
        }
        FieldValue::Array(values) => {
            let element = node.and_then(|node| match &node.kind {
                SchemaNodeKind::Array { element, .. } => Some(element.as_ref()),
                _ => None,
            });
            let items: Vec<Value> = values
                .iter()
                .enumerate()
                .map(|(index, value)| {
                    let mut address = address.clone();
                    address.value_steps.push(ValueStep::Index { index });
                    wire_value(value, &address, element, context)
                })
                .collect();
            json!({"kind":"array", "items":items})
        }
    };
    output["address"] = json!(address);
    output["schema_path"] = json!(node.map(|node| node.path.as_str()));
    output["translatable"] = json!(translatable);
    output["string_table"] = json!(string_table);
    output
}

/// Inserts a subrecord using its schema path and a tagged owned JSON value.
///
/// Use this for absent optional subrecords that do not yet have an address.
/// The native grammar determines placement and rejects ambiguous repeat scopes.
/// Complete multi-subrecord groups cannot be created through this operation.
/// All handles and UTF-8 strings are borrowed only for this call.
///
/// # Errors
///
/// Returns -1 and sets the last error for null or consumed handles, malformed JSON,
/// invalid paths or values, or a grammar-ambiguous insertion. Failed edits do not
/// change the editor. Returns zero on success.
///
/// # Safety
///
/// Non-null pointers must remain valid for this call. Strings must be valid
/// NUL-terminated UTF-8. The editor must not be concurrently accessed.
#[no_mangle]
pub extern "C" fn bethkit_record_editor_insert_json(
    editor: *mut BethkitRecordEditor,
    path: *const c_char,
    value_json: *const c_char,
) -> i32 {
    null_check!(editor, "bethkit_record_editor_insert_json/editor", -1);
    null_check!(path, "bethkit_record_editor_insert_json/path", -1);
    null_check!(value_json, "bethkit_record_editor_insert_json/value", -1);
    ffi_try!(
        (|| -> Result<()> {
            // SAFETY: editor is non-null, exclusively borrowed, and valid by contract.
            let editor = unsafe { &mut *editor };
            let editor = editor
                .0
                .as_mut()
                .ok_or_else(|| input_error("editor is consumed"))?;
            let path = cstr_to_str(path, "bethkit_record_editor_insert_json/path")
                .ok_or_else(|| input_error("path must be UTF-8"))?;
            let value_json = cstr_to_str(value_json, "bethkit_record_editor_insert_json/value")
                .ok_or_else(|| input_error("value must be UTF-8"))?;
            let json_value: Value = serde_json::from_str(value_json)?;
            let registry = SchemaRegistry::new(editor.schema_package().clone());
            let (signature, _) = editor.identity();
            let node = registry.get_node(signature, path);
            validate_struct_names(&json_value, &registry, signature, node)?;
            let value = parse_owned_value(&json_value, 0)?;
            editor.insert(path, &value)?;
            Ok(())
        })(),
        -1
    );
    0
}

/// Replaces the exact value selected by a native snapshot address.
///
/// Both JSON strings are borrowed NUL-terminated UTF-8. `value_json` uses the
/// snapshot's tagged value representation; extra snapshot metadata is accepted.
/// Returns 0 on success; the editor retains ownership of all copied values.
///
/// # Errors
///
/// Returns -1 and sets the last error for invalid JSON, a null or consumed editor,
/// a stale address, or a schema-invalid edit. Failed edits leave the editor unchanged.
///
/// # Safety
///
/// The editor must be live and both strings must be valid for this call.
#[no_mangle]
pub extern "C" fn bethkit_record_editor_set_at_json(
    editor: *mut BethkitRecordEditor,
    address_json: *const c_char,
    value_json: *const c_char,
) -> i32 {
    edit_json(editor, address_json, value_json, EditOperation::Set)
}

/// Inserts a value before an array item or appends to an addressed array.
///
/// A whole non-array subrecord address inserts a same-path occurrence before it
/// when grammar permits. Both JSON strings are borrowed. Returns 0 on success.
///
/// # Errors
///
/// Returns -1 and sets the last error for malformed JSON, null or consumed handles,
/// stale addresses, or invalid insertion. The editor remains unchanged on failure.
///
/// # Safety
///
/// The editor and NUL-terminated UTF-8 strings must remain valid for this call.
#[no_mangle]
pub extern "C" fn bethkit_record_editor_insert_at_json(
    editor: *mut BethkitRecordEditor,
    address_json: *const c_char,
    value_json: *const c_char,
) -> i32 {
    edit_json(editor, address_json, value_json, EditOperation::Insert)
}

/// Removes an addressed subrecord, array item, or optional packed member.
///
/// `address_json` is borrowed NUL-terminated UTF-8. Returns 0 on success.
///
/// # Errors
///
/// Returns -1 and sets the last error for invalid JSON, stale addresses, a null or
/// consumed editor, or schema-invalid removal. Failure leaves the editor unchanged.
///
/// # Safety
///
/// The editor and address string must remain valid for this call.
#[no_mangle]
pub extern "C" fn bethkit_record_editor_remove_at_json(
    editor: *mut BethkitRecordEditor,
    address_json: *const c_char,
) -> i32 {
    edit_json(
        editor,
        address_json,
        std::ptr::null(),
        EditOperation::Remove,
    )
}

#[derive(Clone, Copy)]
enum EditOperation {
    Set,
    Insert,
    Remove,
}

fn edit_json(
    editor: *mut BethkitRecordEditor,
    address: *const c_char,
    value: *const c_char,
    operation: EditOperation,
) -> i32 {
    null_check!(editor, "bethkit_record_editor_edit_json/editor", -1);
    null_check!(address, "bethkit_record_editor_edit_json/address", -1);
    if !matches!(operation, EditOperation::Remove) {
        null_check!(value, "bethkit_record_editor_edit_json/value", -1);
    }
    ffi_try!(
        (|| -> Result<()> {
            let address_text = cstr_to_str(address, "record editor address")
                .ok_or_else(|| input_error("address is not valid UTF-8"))?;
            let address: FieldAddress = serde_json::from_str(address_text)?;
            // SAFETY: editor is non-null and exclusively borrowed by caller contract.
            let editor = unsafe { &mut *editor }
                .0
                .as_mut()
                .ok_or(FfiError::WriterConsumed)?;
            if matches!(operation, EditOperation::Remove) {
                return Ok(editor.remove_at(&address)?);
            }
            let value_text = cstr_to_str(value, "record editor value")
                .ok_or_else(|| input_error("value is not valid UTF-8"))?;
            let json_value: Value = serde_json::from_str(value_text)?;
            let registry = SchemaRegistry::new(editor.schema_package().clone());
            let signature = Signature(address.record_signature);
            let mut node = registry.get_node(signature, &address.subrecord_path);
            for step in &address.value_steps {
                node =
                    match step {
                        ValueStep::Field { path, .. } => registry.get_node(signature, path),
                        ValueStep::Index { .. } => node.and_then(|node| {
                            match &payload_node(&registry, signature, node).kind {
                                SchemaNodeKind::Array { element, .. } => Some(element.as_ref()),
                                _ => None,
                            }
                        }),
                    };
            }
            if matches!(operation, EditOperation::Insert) {
                if let Some(parent) = node {
                    if let SchemaNodeKind::Array { element, .. } =
                        &payload_node(&registry, signature, parent).kind
                    {
                        node = Some(element);
                    }
                }
            }
            validate_struct_names(&json_value, &registry, signature, node)?;
            let value = parse_owned_value(&json_value, 0)?;
            match operation {
                EditOperation::Set => editor.set_at(&address, &value)?,
                EditOperation::Insert => editor.insert_at(&address, &value)?,
                EditOperation::Remove => unreachable!("remove returned before value parsing"),
            }
            Ok(())
        })(),
        -1
    );
    0
}

fn input_error(message: &str) -> FfiError {
    FfiError::InvalidArgument {
        context: "semantic JSON value",
        message: message.to_owned(),
    }
}

fn validate_struct_names(
    input: &Value,
    registry: &SchemaRegistry,
    signature: Signature,
    node: Option<&SchemaNode>,
) -> Result<()> {
    let node = node.map(|node| payload_node(registry, signature, node));
    match input["kind"].as_str() {
        Some("struct") => {
            let fields = input["fields"]
                .as_array()
                .ok_or_else(|| input_error("expected struct fields"))?;
            let definitions = node.and_then(|node| match &node.kind {
                SchemaNodeKind::Struct { fields }
                | SchemaNodeKind::OptionalStruct { fields, .. } => Some(fields.as_slice()),
                _ => None,
            });
            if let Some(definitions) = definitions {
                if fields.len() != definitions.len() {
                    return Err(input_error(
                        "struct field count does not match schema; use absent for optional members",
                    ));
                }
            }
            for (index, field) in fields.iter().enumerate() {
                let definition = definitions.and_then(|definitions| definitions.get(index));
                if let Some(definition) = definition {
                    if field["path"]
                        .as_str()
                        .is_some_and(|path| path != definition.path)
                        || field["name"]
                            .as_str()
                            .is_some_and(|name| name != definition.name)
                        || (field["path"].as_str().is_none() && field["name"].as_str().is_none())
                    {
                        return Err(input_error(
                            "struct field identity/order does not match schema",
                        ));
                    }
                }
                let effective = field["effective_path"]
                    .as_str()
                    .and_then(|path| registry.get_node(signature, path))
                    .or(definition);
                validate_struct_names(&field["value"], registry, signature, effective)?;
            }
        }
        Some("array") => {
            let element = node.and_then(|node| match &node.kind {
                SchemaNodeKind::Array { element, .. } => Some(element.as_ref()),
                _ => None,
            });
            for item in input["items"]
                .as_array()
                .ok_or_else(|| input_error("expected array items"))?
            {
                validate_struct_names(item, registry, signature, element)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn parse_owned_value(input: &Value, depth: usize) -> Result<OwnedFieldValue> {
    if depth > 128 {
        return Err(input_error("value nesting exceeds 128 levels"));
    }
    let scalar = &input["value"];
    Ok(
        match input["kind"]
            .as_str()
            .ok_or_else(|| input_error("value requires kind"))?
        {
            "int" | "enum" => {
                OwnedFieldValue::Int(scalar.as_i64().ok_or_else(|| input_error("expected i64"))?)
            }
            "uint" | "flags" => {
                OwnedFieldValue::UInt(scalar.as_u64().ok_or_else(|| input_error("expected u64"))?)
            }
            "float" => OwnedFieldValue::Float(match scalar.as_str() {
                Some("NaN") => f64::NAN,
                Some("Infinity") => f64::INFINITY,
                Some("-Infinity") => f64::NEG_INFINITY,
                _ => scalar
                    .as_f64()
                    .ok_or_else(|| input_error("expected float"))?,
            }),
            "string" => OwnedFieldValue::String(
                scalar
                    .as_str()
                    .ok_or_else(|| input_error("expected string"))?
                    .to_owned(),
            ),
            "form_id" => OwnedFieldValue::FormId(bethkit_core::FormId(
                u32::try_from(
                    scalar
                        .as_u64()
                        .ok_or_else(|| input_error("expected u32 FormID"))?,
                )
                .map_err(|_| input_error("FormID exceeds u32"))?,
            )),
            "bytes" => OwnedFieldValue::Bytes(
                scalar
                    .as_array()
                    .ok_or_else(|| input_error("expected byte array"))?
                    .iter()
                    .map(|value| {
                        value
                            .as_u64()
                            .and_then(|value| u8::try_from(value).ok())
                            .ok_or_else(|| input_error("byte outside 0..255"))
                    })
                    .collect::<Result<Vec<_>>>()?,
            ),
            "struct" => OwnedFieldValue::Struct(
                input["fields"]
                    .as_array()
                    .ok_or_else(|| input_error("expected struct fields"))?
                    .iter()
                    .map(|field| parse_owned_value(&field["value"], depth + 1))
                    .collect::<Result<Vec<_>>>()?,
            ),
            "array" => OwnedFieldValue::Array(
                input["items"]
                    .as_array()
                    .ok_or_else(|| input_error("expected array items"))?
                    .iter()
                    .map(|item| parse_owned_value(item, depth + 1))
                    .collect::<Result<Vec<_>>>()?,
            ),
            "absent" => OwnedFieldValue::Absent,
            _ => return Err(input_error("unknown value kind")),
        },
    )
}
