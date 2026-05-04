// SPDX-License-Identifier: Apache-2.0
//!
//! FFI functions for schema-guided record inspection.
//!
//! Because [`bethkit_core::RecordView`] holds a lifetime parameter tied to
//! the record data, it cannot be stored directly behind an opaque FFI handle.
//! Instead, [`bethkit_record_view_new`] eagerly converts all decoded
//! [`FieldValue`]s into owned [`BethkitNamedField`] snapshots that are
//! independent of the record's lifetime.
//!
//! # Ownership
//!
//! [`BethkitRecordView`] is owned and must be freed with
//! [`bethkit_record_view_free`].  Freeing the view also frees all nested
//! [`BethkitFieldEntries`] and [`BethkitFieldValues`] objects reachable from
//! it.  **Do not call [`bethkit_field_entries_free`] or
//! [`bethkit_field_values_free`] on objects obtained from a view** - doing so
//! would cause a double-free.  Those free functions exist only for objects
//! that are detached from any view.
//!
//! All `name` pointers inside [`BethkitNamedField`] (field names, enum
//! variant names, flag bit names) are interned in the owning view's string
//! arena.  They are NUL-terminated and valid until the view is freed; never
//! free them individually.
//!
//! The [`BethkitSchemaRegistry`] returned by [`bethkit_schema_registry_sse`]
//! points to a `'static` value and must never be freed.

use std::ffi::c_char;
use std::mem::ManuallyDrop;

use bethkit_core::{FieldValue, RecordView, SchemaRegistry, Signature};

use crate::error::FfiError;
use crate::record::BethkitRecord;
use crate::types::{BethkitEnumVal, BethkitFieldValueKind, BethkitFlagsVal, BethkitTypedFormId};
use crate::{ffi_try, null_check, set_last_error, BethkitSlice};

/// A decoded field value stored as a `#[repr(C)]` tagged union.
///
/// Inspect `kind` to determine which arm of `payload` is active.  Arms that
/// allocate heap memory (`Struct`, `Array`, `Flags`) must be released with
/// the appropriate free functions when the containing [`BethkitRecordView`]
/// is freed (this is done automatically by [`bethkit_record_view_free`]).
/// Do not release fields borrowed from a view after the view has been freed.
#[repr(C)]
pub struct BethkitFieldValue {
    /// Identifies the active arm of `payload`.
    pub kind: BethkitFieldValueKind,
    /// The decoded value payload.
    pub payload: BethkitFieldValuePayload,
}

/// The payload union inside [`BethkitFieldValue`].
///
/// Only the arm corresponding to [`BethkitFieldValue::kind`] is valid.
#[repr(C)]
pub union BethkitFieldValuePayload {
    /// Active when `kind == Int`.
    pub int_val: i64,
    /// Active when `kind == UInt`.
    pub uint_val: u64,
    /// Active when `kind == Float`.
    pub float_val: f64,
    /// Active when `kind == Str`.  Borrowed from the owning view.
    pub str_val: *const c_char,
    /// Active when `kind == FormId`.
    pub form_id: u32,
    /// Active when `kind == FormIdTyped`.
    pub form_id_typed: BethkitTypedFormId,
    /// Active when `kind == Bytes`.  Borrowed from the owning view.
    pub bytes: std::mem::ManuallyDrop<BethkitSlice>,
    /// Active when `kind == Enum`.
    pub enum_val: BethkitEnumVal,
    /// Active when `kind == Flags`.  The flags value owns its active-names
    /// array and is dropped when the enclosing [`BethkitNamedField`] is freed.
    pub flags_val: ManuallyDrop<BethkitFlagsVal>,
    /// Active when `kind == Struct`.  Owned by the enclosing
    /// [`BethkitRecordView`]; recursively freed by [`bethkit_record_view_free`].
    /// **Do not pass to [`bethkit_field_entries_free`] if this value was
    /// obtained from a view** — that causes a double-free.
    pub struct_entries: *mut BethkitFieldEntries,
    /// Active when `kind == Array`.  Owned by the enclosing
    /// [`BethkitRecordView`]; recursively freed by [`bethkit_record_view_free`].
    /// **Do not pass to [`bethkit_field_values_free`] if this value was
    /// obtained from a view** — that causes a double-free.
    pub array_values: *mut BethkitFieldValues,
    /// Active when `kind == LocalizedId`.
    pub localized_id: u32,
    /// Active when `kind == Missing` or `kind == FormId` with zero value.
    /// No meaningful data; present so the union is never zero-sized.
    pub _pad: u64,
}

/// A named field snapshot inside a [`BethkitRecordView`] or
/// [`BethkitFieldEntries`].
#[repr(C)]
pub struct BethkitNamedField {
    /// Human-readable field name from the schema.  Points into the owning
    /// view's string arena; valid until the view is freed.  Never free this
    /// pointer directly.
    pub name: *const c_char,
    /// The decoded field value.
    pub value: BethkitFieldValue,
}

/// A heap-allocated list of named fields decoded from a struct field.
///
/// Ownership depends on how this was obtained:
/// - **Detached** (returned directly to the caller): free with
///   [`bethkit_field_entries_free`].
/// - **Embedded in a [`BethkitRecordView`]**: freed automatically by
///   [`bethkit_record_view_free`] — **do not** call [`bethkit_field_entries_free`]
///   on it or a double-free will occur.
pub struct BethkitFieldEntries {
    entries: Vec<BethkitNamedField>,
}

/// A heap-allocated list of field values decoded from an array field.
///
/// Ownership depends on how this was obtained:
/// - **Detached** (returned directly to the caller): free with
///   [`bethkit_field_values_free`].
/// - **Embedded in a [`BethkitRecordView`]**: freed automatically by
///   [`bethkit_record_view_free`] — **do not** call [`bethkit_field_values_free`]
///   on it or a double-free will occur.
pub struct BethkitFieldValues {
    values: Vec<BethkitFieldValue>,
}

/// An owned, schema-guided snapshot of all decoded fields from a record.
///
/// Created by [`bethkit_record_view_new`].  Must be freed with
/// [`bethkit_record_view_free`].
pub struct BethkitRecordView {
    fields: Vec<BethkitNamedField>,
    // NOTE: string_arena is never read explicitly; it exists solely to keep
    // NOTE: the CStrings alive (RAII). All `name` and `str_val` pointers in
    // NOTE: `fields` point into this arena.
    #[allow(dead_code)]
    string_arena: Vec<std::ffi::CString>,
}

/// An opaque handle to a schema registry (a map from record signature to
/// schema definition).
///
/// The registry returned by [`bethkit_schema_registry_sse`] is `'static`
/// and must never be freed.
pub struct BethkitSchemaRegistry(&'static SchemaRegistry);

/// Returns a pointer to the Skyrim SE schema registry.
///
/// The registry is a static singleton; do not free the returned pointer.
#[no_mangle]
pub extern "C" fn bethkit_schema_registry_sse() -> *const BethkitSchemaRegistry {
    static HANDLE: std::sync::OnceLock<BethkitSchemaRegistry> = std::sync::OnceLock::new();
    HANDLE.get_or_init(|| BethkitSchemaRegistry(SchemaRegistry::sse()))
}

/// Returns `true` if the registry contains a schema for the 4-byte record
/// signature pointed to by `sig`.
///
/// `sig` must point to exactly 4 readable bytes.
///
/// Returns `false` and sets the last error if `reg` or `sig` is null.
#[no_mangle]
pub extern "C" fn bethkit_schema_registry_has(
    reg: *const BethkitSchemaRegistry,
    sig: *const u8,
) -> bool {
    null_check!(reg, "bethkit_schema_registry_has", false);
    null_check!(sig, "bethkit_schema_registry_has/sig", false);
    // SAFETY: reg and sig are non-null; sig is 4 bytes by contract.
    let reg = unsafe { &*reg };
    let sig_bytes: [u8; 4] = unsafe { std::ptr::read(sig as *const [u8; 4]) };
    reg.0.get(Signature(sig_bytes)).is_some()
}

/// Creates a schema-guided snapshot of all decoded fields in `record`.
///
/// Looks up the schema for the 4-byte `sig` in the SSE registry.  If no
/// schema is found for `sig`, or decoding a field fails, the affected field
/// is stored as [`BethkitFieldValueKind::Missing`].
///
/// `localized` should be `true` when the plugin that contains `record` has
/// its LOCALIZED flag set; see [`bethkit_plugin_is_localized`].
///
/// Returns a pointer to the view on success, or null on error.  Must be
/// freed with [`bethkit_record_view_free`].
///
/// # Arguments
///
/// * `record`    — Record to inspect. Borrows.
/// * `sig`       — 4-byte record signature used for schema lookup. Borrows.
/// * `localized` — Whether the parent plugin is localized.
///
/// # Errors
///
/// Returns null and sets the last error if `record` or `sig` is null, or
/// schema decoding fails entirely.
#[no_mangle]
pub extern "C" fn bethkit_record_view_new(
    record: *const BethkitRecord,
    sig: *const u8,
    localized: bool,
) -> *mut BethkitRecordView {
    null_check!(record, "bethkit_record_view_new", std::ptr::null_mut());
    null_check!(sig, "bethkit_record_view_new/sig", std::ptr::null_mut());

    // SAFETY: record and sig are non-null; sig is 4 bytes by contract.
    let rec = unsafe { &*record };
    let sig_bytes: [u8; 4] = unsafe { std::ptr::read(sig as *const [u8; 4]) };

    let registry = SchemaRegistry::sse();
    let schema = match registry.get(Signature(sig_bytes)) {
        Some(s) => s,
        None => {
            set_last_error(format!(
                "bethkit_record_view_new: no schema for signature {:?}",
                sig_bytes
            ));
            return std::ptr::null_mut();
        }
    };

    let view = ffi_try!(
        RecordView::new(&rec.0, schema, localized)
            .fields()
            .map_err(FfiError::Core),
        std::ptr::null_mut()
    );

    let mut owned_strings: Vec<std::ffi::CString> = Vec::new();
    let fields: Vec<BethkitNamedField> = view
        .iter()
        .map(|fe| {
            let value = convert_field_value(&fe.value, &mut owned_strings);
            BethkitNamedField {
                name: intern_str(fe.name, &mut owned_strings),
                value,
            }
        })
        .collect();

    Box::into_raw(Box::new(BethkitRecordView {
        fields,
        string_arena: owned_strings,
    }))
}

/// Frees a record view and recursively all owned sub-objects — nested
/// [`BethkitFieldEntries`] (struct fields), [`BethkitFieldValues`] (array
/// fields), and flags-name arrays.  All `name` and `str_val` pointers
/// borrowed from the view become invalid after this call.
///
/// Passing a null pointer is a no-op.
#[no_mangle]
pub extern "C" fn bethkit_record_view_free(view: *mut BethkitRecordView) {
    if view.is_null() {
        return;
    }
    // SAFETY: view was produced by Box::into_raw.
    let v = unsafe { Box::from_raw(view) };
    // Drop each owned field value before dropping the Vec.
    for field in v.fields {
        drop_field_value(field.value);
    }
}

/// Returns the number of fields in the view.
///
/// Returns 0 and sets the last error if `view` is null.
#[no_mangle]
pub extern "C" fn bethkit_record_view_field_count(view: *const BethkitRecordView) -> usize {
    null_check!(view, "bethkit_record_view_field_count", 0);
    // SAFETY: view is non-null.
    unsafe { &*view }.fields.len()
}

/// Returns a borrowed pointer to the field at `index`, or null if out of
/// bounds.
///
/// The returned pointer is borrowed from `view` and is valid until
/// [`bethkit_record_view_free`] is called.
///
/// # Errors
///
/// Returns null and sets the last error if `view` is null or `index` is out
/// of bounds.
#[no_mangle]
pub extern "C" fn bethkit_record_view_field_get(
    view: *const BethkitRecordView,
    index: usize,
) -> *const BethkitNamedField {
    null_check!(view, "bethkit_record_view_field_get", std::ptr::null());
    // SAFETY: view is non-null.
    let v = unsafe { &*view };
    match v.fields.get(index) {
        Some(f) => f as *const BethkitNamedField,
        None => {
            set_last_error(format!(
                "bethkit_record_view_field_get: index {index} out of bounds (len = {})",
                v.fields.len()
            ));
            std::ptr::null()
        }
    }
}

/// Returns the number of entries in a struct field list.
///
/// Returns 0 and sets the last error if `entries` is null.
#[no_mangle]
pub extern "C" fn bethkit_field_entries_len(entries: *const BethkitFieldEntries) -> usize {
    null_check!(entries, "bethkit_field_entries_len", 0);
    // SAFETY: entries is non-null.
    unsafe { &*entries }.entries.len()
}

/// Returns a borrowed pointer to the named field at `index` in `entries`, or
/// null if `index` is out of bounds.
///
/// # Errors
///
/// Returns null and sets the last error if `entries` is null or `index` is
/// out of bounds.
#[no_mangle]
pub extern "C" fn bethkit_field_entries_get(
    entries: *const BethkitFieldEntries,
    index: usize,
) -> *const BethkitNamedField {
    null_check!(entries, "bethkit_field_entries_get", std::ptr::null());
    // SAFETY: entries is non-null.
    let e = unsafe { &*entries };
    match e.entries.get(index) {
        Some(f) => f as *const BethkitNamedField,
        None => {
            set_last_error(format!(
                "bethkit_field_entries_get: index {index} out of bounds (len = {})",
                e.entries.len()
            ));
            std::ptr::null()
        }
    }
}

/// Frees a **detached** field entries list — one explicitly owned by the
/// caller and not embedded in a [`BethkitRecordView`].
///
/// **Do not call this on values obtained from a [`BethkitRecordView`].**
/// [`bethkit_record_view_free`] handles recursive cleanup automatically;
/// calling this on view-owned entries causes a double-free.
///
/// Passing a null pointer is a no-op.
#[no_mangle]
pub extern "C" fn bethkit_field_entries_free(entries: *mut BethkitFieldEntries) {
    if entries.is_null() {
        return;
    }
    // SAFETY: entries was produced by Box::into_raw.
    let e = unsafe { Box::from_raw(entries) };
    for field in e.entries {
        drop_field_value(field.value);
    }
}

/// Returns the number of values in an array field list.
///
/// Returns 0 and sets the last error if `values` is null.
#[no_mangle]
pub extern "C" fn bethkit_field_values_len(values: *const BethkitFieldValues) -> usize {
    null_check!(values, "bethkit_field_values_len", 0);
    // SAFETY: values is non-null.
    unsafe { &*values }.values.len()
}

/// Returns a borrowed pointer to the value at `index` in `values`, or null
/// if `index` is out of bounds.
///
/// # Errors
///
/// Returns null and sets the last error if `values` is null or `index` is
/// out of bounds.
#[no_mangle]
pub extern "C" fn bethkit_field_values_get(
    values: *const BethkitFieldValues,
    index: usize,
) -> *const BethkitFieldValue {
    null_check!(values, "bethkit_field_values_get", std::ptr::null());
    // SAFETY: values is non-null.
    let v = unsafe { &*values };
    match v.values.get(index) {
        Some(val) => val as *const BethkitFieldValue,
        None => {
            set_last_error(format!(
                "bethkit_field_values_get: index {index} out of bounds (len = {})",
                v.values.len()
            ));
            std::ptr::null()
        }
    }
}

/// Frees a **detached** field values list — one explicitly owned by the
/// caller and not embedded in a [`BethkitRecordView`].
///
/// **Do not call this on values obtained from a [`BethkitRecordView`].**
/// [`bethkit_record_view_free`] handles recursive cleanup automatically;
/// calling this on view-owned values causes a double-free.
///
/// Passing a null pointer is a no-op.
#[no_mangle]
pub extern "C" fn bethkit_field_values_free(values: *mut BethkitFieldValues) {
    if values.is_null() {
        return;
    }
    // SAFETY: values was produced by Box::into_raw.
    let v = unsafe { Box::from_raw(values) };
    for val in v.values {
        drop_field_value(val);
    }
}

/// Interns `s` as a NUL-terminated [`std::ffi::CString`] into `arena` and
/// returns a stable pointer to its data.
///
/// The pointer is valid for as long as `arena` is alive.  Any embedded NUL
/// bytes in `s` are replaced with `?` to guarantee a valid C string.
fn intern_str(s: &str, arena: &mut Vec<std::ffi::CString>) -> *const c_char {
    let sanitized: Vec<u8> = s.bytes().map(|b| if b == 0 { b'?' } else { b }).collect();
    let cs = std::ffi::CString::new(sanitized)
        .unwrap_or_else(|_| std::ffi::CString::new("?").expect("single char is always valid"));
    let ptr = cs.as_ptr();
    arena.push(cs);
    ptr
}

/// Recursively converts a [`FieldValue`] into a [`BethkitFieldValue`].
///
/// String values and schema label strings (field names, enum variant names,
/// flag bit names) are interned into `owned_strings` so their pointers are
/// NUL-terminated and stable for the lifetime of the view.
fn convert_field_value<'a>(
    fv: &FieldValue<'a>,
    owned_strings: &mut Vec<std::ffi::CString>,
) -> BethkitFieldValue {
    match fv {
        FieldValue::Int(v) => BethkitFieldValue {
            kind: BethkitFieldValueKind::Int,
            payload: BethkitFieldValuePayload { int_val: *v },
        },
        FieldValue::UInt(v) => BethkitFieldValue {
            kind: BethkitFieldValueKind::UInt,
            payload: BethkitFieldValuePayload { uint_val: *v },
        },
        FieldValue::Float(v) => BethkitFieldValue {
            kind: BethkitFieldValueKind::Float,
            payload: BethkitFieldValuePayload { float_val: *v },
        },
        FieldValue::Str(s) => {
            let sanitized: Vec<u8> = s.bytes().map(|b| if b == 0 { b'?' } else { b }).collect();
            let cs = std::ffi::CString::new(sanitized)
                .unwrap_or_else(|_| std::ffi::CString::new("?").expect("single char is valid"));
            let ptr = cs.as_ptr();
            owned_strings.push(cs);
            BethkitFieldValue {
                kind: BethkitFieldValueKind::Str,
                payload: BethkitFieldValuePayload { str_val: ptr },
            }
        }
        FieldValue::FormId(id) => BethkitFieldValue {
            kind: BethkitFieldValueKind::FormId,
            payload: BethkitFieldValuePayload { form_id: id.0 },
        },
        FieldValue::FormIdTyped { raw, allowed } => BethkitFieldValue {
            kind: BethkitFieldValueKind::FormIdTyped,
            payload: BethkitFieldValuePayload {
                form_id_typed: BethkitTypedFormId {
                    raw: raw.0,
                    allowed_sigs: allowed.as_ptr() as *const [u8; 4],
                    allowed_count: allowed.len(),
                },
            },
        },
        FieldValue::Bytes(b) => BethkitFieldValue {
            kind: BethkitFieldValueKind::Bytes,
            payload: BethkitFieldValuePayload {
                bytes: ManuallyDrop::new(BethkitSlice {
                    ptr: b.as_ptr(),
                    len: b.len(),
                }),
            },
        },
        FieldValue::Enum { value, name } => BethkitFieldValue {
            kind: BethkitFieldValueKind::Enum,
            payload: BethkitFieldValuePayload {
                enum_val: BethkitEnumVal {
                    value: *value,
                    name: match name {
                        Some(n) => intern_str(n, owned_strings),
                        None => std::ptr::null(),
                    },
                },
            },
        },
        FieldValue::Flags { value, active } => {
            // Build a heap-allocated array of *const c_char. Each name pointer
            // is interned into owned_strings, so it is NUL-terminated and
            // stable for the lifetime of the enclosing view.
            let name_ptrs: Vec<*const c_char> = active
                .iter()
                .map(|s| intern_str(s, owned_strings))
                .collect();
            let count = name_ptrs.len();
            let boxed = name_ptrs.into_boxed_slice();
            let ptr = boxed.as_ptr();
            // Transfer ownership; drop happens in drop_field_value.
            std::mem::forget(boxed);
            BethkitFieldValue {
                kind: BethkitFieldValueKind::Flags,
                payload: BethkitFieldValuePayload {
                    flags_val: ManuallyDrop::new(BethkitFlagsVal {
                        raw_value: *value,
                        active_names: ptr,
                        active_count: count,
                    }),
                },
            }
        }
        FieldValue::Struct(sub_fields) => {
            let entries: Vec<BethkitNamedField> = sub_fields
                .iter()
                .map(|fe| {
                    let value = convert_field_value(&fe.value, owned_strings);
                    BethkitNamedField {
                        name: intern_str(fe.name, owned_strings),
                        value,
                    }
                })
                .collect();
            let boxed = Box::new(BethkitFieldEntries { entries });
            BethkitFieldValue {
                kind: BethkitFieldValueKind::Struct,
                payload: BethkitFieldValuePayload {
                    struct_entries: Box::into_raw(boxed),
                },
            }
        }
        FieldValue::Array(items) => {
            let values: Vec<BethkitFieldValue> = items
                .iter()
                .map(|v| convert_field_value(v, owned_strings))
                .collect();
            let boxed = Box::new(BethkitFieldValues { values });
            BethkitFieldValue {
                kind: BethkitFieldValueKind::Array,
                payload: BethkitFieldValuePayload {
                    array_values: Box::into_raw(boxed),
                },
            }
        }
        FieldValue::LocalizedId(id) => BethkitFieldValue {
            kind: BethkitFieldValueKind::LocalizedId,
            payload: BethkitFieldValuePayload { localized_id: *id },
        },
        FieldValue::Missing => BethkitFieldValue {
            kind: BethkitFieldValueKind::Missing,
            payload: BethkitFieldValuePayload { _pad: 0 },
        },
    }
}

/// Recursively drops owned resources inside a [`BethkitFieldValue`].
///
/// Does not free the [`BethkitFieldValue`] itself (it is stack or
/// slice-allocated).
fn drop_field_value(v: BethkitFieldValue) {
    match v.kind {
        BethkitFieldValueKind::Flags => {
            // SAFETY: Flags arm was set by convert_field_value; flags_val is
            // SAFETY: valid and the active_names array was Box::into_raw'd.
            let flags = unsafe { ManuallyDrop::into_inner(v.payload.flags_val) };
            if !flags.active_names.is_null() && flags.active_count > 0 {
                // SAFETY: active_names was produced by Box<[*const c_char]>::into_raw.
                drop(unsafe {
                    Box::from_raw(std::ptr::slice_from_raw_parts_mut(
                        flags.active_names as *mut *const c_char,
                        flags.active_count,
                    ))
                });
            }
        }
        BethkitFieldValueKind::Struct => {
            // SAFETY: struct_entries was set in convert_field_value via Box::into_raw.
            let ptr = unsafe { v.payload.struct_entries };
            if !ptr.is_null() {
                let entries = unsafe { Box::from_raw(ptr) };
                for fe in entries.entries {
                    drop_field_value(fe.value);
                }
            }
        }
        BethkitFieldValueKind::Array => {
            // SAFETY: array_values was set in convert_field_value via Box::into_raw.
            let ptr = unsafe { v.payload.array_values };
            if !ptr.is_null() {
                let values = unsafe { Box::from_raw(ptr) };
                for item in values.values {
                    drop_field_value(item);
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::{CStr, CString};

    use super::*;

    /// Verifies that `intern_str` produces a NUL-terminated pointer into the arena.
    #[test]
    fn intern_str_is_nul_terminated() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let mut arena: Vec<CString> = Vec::new();

        // when
        let ptr = intern_str("TestField", &mut arena);

        // then
        // SAFETY: ptr points into arena, which is alive for the rest of this function.
        let cstr = unsafe { CStr::from_ptr(ptr) };
        assert_eq!(cstr.to_str()?, "TestField");
        assert_eq!(arena.len(), 1);
        Ok(())
    }

    /// Verifies that `drop_field_value` correctly frees the flags active-names
    /// array without panicking or leaking.
    #[test]
    fn drop_field_value_flags_cleans_up() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given: build a Flags field value exactly as convert_field_value does.
        let mut arena: Vec<CString> = Vec::new();
        let name_ptrs: Vec<*const c_char> = vec![
            intern_str("BitA", &mut arena),
            intern_str("BitB", &mut arena),
        ];
        let count = name_ptrs.len();
        let boxed = name_ptrs.into_boxed_slice();
        let ptr = boxed.as_ptr();
        std::mem::forget(boxed);

        let fv = BethkitFieldValue {
            kind: BethkitFieldValueKind::Flags,
            payload: BethkitFieldValuePayload {
                flags_val: ManuallyDrop::new(BethkitFlagsVal {
                    raw_value: 0b11,
                    active_names: ptr,
                    active_count: count,
                }),
            },
        };

        // when / then: must not panic or leak
        drop_field_value(fv);
        Ok(())
    }

    /// Verifies that `drop_field_value` recursively cleans up nested struct
    /// entries without panicking or leaking.
    #[test]
    fn drop_field_value_struct_recursively_drops(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given: a Struct containing one Int entry.
        let inner = BethkitNamedField {
            name: std::ptr::null(),
            value: BethkitFieldValue {
                kind: BethkitFieldValueKind::Int,
                payload: BethkitFieldValuePayload { int_val: 99 },
            },
        };
        let entries = Box::new(BethkitFieldEntries {
            entries: vec![inner],
        });
        let fv = BethkitFieldValue {
            kind: BethkitFieldValueKind::Struct,
            payload: BethkitFieldValuePayload {
                struct_entries: Box::into_raw(entries),
            },
        };

        // when / then: recursive drop must not panic or leak
        drop_field_value(fv);
        Ok(())
    }
}
