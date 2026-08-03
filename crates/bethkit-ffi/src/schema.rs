// SPDX-License-Identifier: Apache-2.0
//!
//! FFI functions for schema-guided record inspection.
//!
//! Because [`bethkit_semantic::RecordView`] holds a lifetime parameter tied to
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
//! Catalog, package, and semantic-context handles are owned and must be freed
//! with their matching functions.

use std::ffi::c_char;
use std::mem::ManuallyDrop;
use std::path::Path;
use std::sync::Arc;

use bethkit_schema::{SchemaCatalog, SchemaPackage};
use bethkit_semantic::{
    DecoderRegistry, FieldValue, OwnedFieldValue, RecordEditor, SemanticContext,
};

use crate::record::BethkitRecord;
use crate::types::{
    game_to_core, BethkitEnumVal, BethkitFieldValueKind, BethkitFlagsVal, BethkitGame,
    BethkitTypedFormId,
};
use crate::writer::BethkitWritableRecord;
use crate::{cstr_to_str, ffi_try, null_check, set_last_error, BethkitSlice};

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

/// Owned catalog of schema packages.
pub struct BethkitSchemaCatalog(SchemaCatalog);

/// Owned schema-package handle.
pub struct BethkitSchemaPackage(Arc<SchemaPackage>);

/// Owned semantic runtime context.
pub struct BethkitSemanticContext(SemanticContext);

/// Owned lossless semantic record editor.
pub struct BethkitRecordEditor(Option<RecordEditor>);

/// Loads the release-time embedded schema catalog.
///
/// Returns null and sets the last error when this library was built without
/// an embedded schema source or the embedded data is invalid.
#[no_mangle]
pub extern "C" fn bethkit_schema_catalog_embedded() -> *mut BethkitSchemaCatalog {
    let catalog = ffi_try!(SchemaCatalog::embedded(), std::ptr::null_mut());
    Box::into_raw(Box::new(BethkitSchemaCatalog(catalog)))
}

/// Loads a schema catalog bundle from `path`.
///
/// Returns null and sets the last error when the path or bundle is invalid.
#[no_mangle]
pub extern "C" fn bethkit_schema_catalog_open(path: *const c_char) -> *mut BethkitSchemaCatalog {
    let path = match cstr_to_str(path, "bethkit_schema_catalog_open/path") {
        Some(value) => value,
        None => return std::ptr::null_mut(),
    };
    let catalog = ffi_try!(SchemaCatalog::open(Path::new(path)), std::ptr::null_mut());
    Box::into_raw(Box::new(BethkitSchemaCatalog(catalog)))
}

/// Frees an owned schema catalog. Passing null is a no-op.
#[no_mangle]
pub extern "C" fn bethkit_schema_catalog_free(catalog: *mut BethkitSchemaCatalog) {
    if !catalog.is_null() {
        // SAFETY: catalog was produced by Box::into_raw in this module.
        drop(unsafe { Box::from_raw(catalog) });
    }
}

/// Returns an owned package handle for `game`.
///
/// Returns null and sets the last error when the catalog does not contain the
/// requested game.
#[no_mangle]
pub extern "C" fn bethkit_schema_catalog_package(
    catalog: *const BethkitSchemaCatalog,
    game: BethkitGame,
) -> *mut BethkitSchemaPackage {
    null_check!(
        catalog,
        "bethkit_schema_catalog_package",
        std::ptr::null_mut()
    );
    // SAFETY: catalog was checked for null and remains borrowed.
    let catalog = unsafe { &*catalog };
    let package = ffi_try!(catalog.0.require(game_to_core(game)), std::ptr::null_mut());
    Box::into_raw(Box::new(BethkitSchemaPackage(package)))
}

/// Opens one `.bkschema` package from `path`.
#[no_mangle]
pub extern "C" fn bethkit_schema_package_open(path: *const c_char) -> *mut BethkitSchemaPackage {
    let path = match cstr_to_str(path, "bethkit_schema_package_open/path") {
        Some(value) => value,
        None => return std::ptr::null_mut(),
    };
    let package = ffi_try!(SchemaPackage::open(Path::new(path)), std::ptr::null_mut());
    Box::into_raw(Box::new(BethkitSchemaPackage(Arc::new(package))))
}

/// Frees an owned schema-package handle. Passing null is a no-op.
#[no_mangle]
pub extern "C" fn bethkit_schema_package_free(package: *mut BethkitSchemaPackage) {
    if !package.is_null() {
        // SAFETY: package was produced by Box::into_raw in this module.
        drop(unsafe { Box::from_raw(package) });
    }
}

/// Creates a semantic context for `package` and the built-in decoders.
#[no_mangle]
pub extern "C" fn bethkit_semantic_context_new(
    package: *const BethkitSchemaPackage,
) -> *mut BethkitSemanticContext {
    null_check!(
        package,
        "bethkit_semantic_context_new",
        std::ptr::null_mut()
    );
    // SAFETY: package was checked for null and remains borrowed.
    let package = unsafe { &*package };
    let context = ffi_try!(
        SemanticContext::new(package.0.clone(), DecoderRegistry::builtin()),
        std::ptr::null_mut()
    );
    Box::into_raw(Box::new(BethkitSemanticContext(context)))
}

/// Frees an owned semantic context. Passing null is a no-op.
#[no_mangle]
pub extern "C" fn bethkit_semantic_context_free(context: *mut BethkitSemanticContext) {
    if !context.is_null() {
        // SAFETY: context was produced by Box::into_raw in this module.
        drop(unsafe { Box::from_raw(context) });
    }
}

/// Creates a lossless semantic editor for `record`.
#[no_mangle]
pub extern "C" fn bethkit_record_editor_new(
    context: *const BethkitSemanticContext,
    record: *const BethkitRecord,
    localized: bool,
) -> *mut BethkitRecordEditor {
    null_check!(
        context,
        "bethkit_record_editor_new/context",
        std::ptr::null_mut()
    );
    null_check!(
        record,
        "bethkit_record_editor_new/record",
        std::ptr::null_mut()
    );
    // SAFETY: both handles were checked for null and remain borrowed.
    let context = unsafe { &*context };
    let record = unsafe { &*record };
    let editor = ffi_try!(context.0.edit(&record.0, localized), std::ptr::null_mut());
    Box::into_raw(Box::new(BethkitRecordEditor(Some(editor))))
}

/// Frees an owned record editor. Passing null is a no-op.
#[no_mangle]
pub extern "C" fn bethkit_record_editor_free(editor: *mut BethkitRecordEditor) {
    if !editor.is_null() {
        // SAFETY: editor was produced by Box::into_raw in this module.
        drop(unsafe { Box::from_raw(editor) });
    }
}

/// Sets one signed integer field occurrence.
#[no_mangle]
pub extern "C" fn bethkit_record_editor_set_i64(
    editor: *mut BethkitRecordEditor,
    path: *const c_char,
    occurrence: usize,
    value: i64,
) -> i32 {
    edit_set(editor, path, occurrence, &OwnedFieldValue::Int(value))
}

/// Sets one unsigned integer field occurrence.
#[no_mangle]
pub extern "C" fn bethkit_record_editor_set_u64(
    editor: *mut BethkitRecordEditor,
    path: *const c_char,
    occurrence: usize,
    value: u64,
) -> i32 {
    edit_set(editor, path, occurrence, &OwnedFieldValue::UInt(value))
}

/// Sets one floating-point field occurrence.
#[no_mangle]
pub extern "C" fn bethkit_record_editor_set_f64(
    editor: *mut BethkitRecordEditor,
    path: *const c_char,
    occurrence: usize,
    value: f64,
) -> i32 {
    edit_set(editor, path, occurrence, &OwnedFieldValue::Float(value))
}

/// Sets one FormID field occurrence.
#[no_mangle]
pub extern "C" fn bethkit_record_editor_set_form_id(
    editor: *mut BethkitRecordEditor,
    path: *const c_char,
    occurrence: usize,
    value: u32,
) -> i32 {
    edit_set(
        editor,
        path,
        occurrence,
        &OwnedFieldValue::FormId(bethkit_core::FormId(value)),
    )
}

/// Sets one UTF-8 string field occurrence.
#[no_mangle]
pub extern "C" fn bethkit_record_editor_set_string(
    editor: *mut BethkitRecordEditor,
    path: *const c_char,
    occurrence: usize,
    value: *const c_char,
) -> i32 {
    let value = match cstr_to_str(value, "bethkit_record_editor_set_string/value") {
        Some(value) => value,
        None => return -1,
    };
    edit_set(
        editor,
        path,
        occurrence,
        &OwnedFieldValue::String(value.to_owned()),
    )
}

/// Sets one raw-byte field occurrence.
///
/// `value` must point to `length` readable bytes.
#[no_mangle]
pub extern "C" fn bethkit_record_editor_set_bytes(
    editor: *mut BethkitRecordEditor,
    path: *const c_char,
    occurrence: usize,
    value: *const u8,
    length: usize,
) -> i32 {
    null_check!(value, "bethkit_record_editor_set_bytes/value", -1);
    // SAFETY: value is non-null and readable for length bytes by contract.
    let bytes = unsafe { std::slice::from_raw_parts(value, length) };
    edit_set(
        editor,
        path,
        occurrence,
        &OwnedFieldValue::Bytes(bytes.to_vec()),
    )
}

/// Removes one top-level field occurrence.
#[no_mangle]
pub extern "C" fn bethkit_record_editor_remove(
    editor: *mut BethkitRecordEditor,
    path: *const c_char,
    occurrence: usize,
) -> i32 {
    null_check!(editor, "bethkit_record_editor_remove", -1);
    let path = match cstr_to_str(path, "bethkit_record_editor_remove/path") {
        Some(path) => path,
        None => return -1,
    };
    // SAFETY: editor was checked for null and remains exclusively borrowed.
    let editor = unsafe { &mut *editor };
    let Some(inner) = editor.0.as_mut() else {
        set_last_error("record editor was already consumed");
        return -1;
    };
    ffi_try!(inner.remove(path, occurrence), -1);
    0
}

/// Consumes an editor and returns an owned writable record.
///
/// The returned record must be freed with `bethkit_writable_record_free` or
/// transferred to a writable group.
#[no_mangle]
pub extern "C" fn bethkit_record_editor_finish(
    editor: *mut BethkitRecordEditor,
) -> *mut BethkitWritableRecord {
    null_check!(editor, "bethkit_record_editor_finish", std::ptr::null_mut());
    // SAFETY: editor was checked for null and remains exclusively borrowed.
    let editor = unsafe { &mut *editor };
    let Some(inner) = editor.0.take() else {
        set_last_error("record editor was already consumed");
        return std::ptr::null_mut();
    };
    Box::into_raw(Box::new(BethkitWritableRecord(
        inner.into_writable_record(),
    )))
}

fn edit_set(
    editor: *mut BethkitRecordEditor,
    path: *const c_char,
    occurrence: usize,
    value: &OwnedFieldValue,
) -> i32 {
    null_check!(editor, "bethkit_record_editor_set", -1);
    let path = match cstr_to_str(path, "bethkit_record_editor_set/path") {
        Some(path) => path,
        None => return -1,
    };
    // SAFETY: editor was checked for null and remains exclusively borrowed.
    let editor = unsafe { &mut *editor };
    let Some(inner) = editor.0.as_mut() else {
        set_last_error("record editor was already consumed");
        return -1;
    };
    ffi_try!(inner.set(path, occurrence, value), -1);
    0
}

/// Creates a schema-guided snapshot of all decoded fields in `record`.
///
/// Uses the schema package owned by `context`. Unknown subrecords remain
/// visible as raw bytes.
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
/// Returns null and sets the last error if a handle is null or decoding fails.
#[no_mangle]
pub extern "C" fn bethkit_record_view_new(
    context: *const BethkitSemanticContext,
    record: *const BethkitRecord,
    localized: bool,
) -> *mut BethkitRecordView {
    null_check!(
        context,
        "bethkit_record_view_new/context",
        std::ptr::null_mut()
    );
    null_check!(record, "bethkit_record_view_new", std::ptr::null_mut());

    // SAFETY: context and record were checked for null and remain borrowed.
    let context = unsafe { &*context };
    let rec = unsafe { &*record };
    let view = ffi_try!(
        context
            .0
            .view(&rec.0, localized)
            .and_then(|value| value.fields()),
        std::ptr::null_mut()
    );

    let mut owned_strings: Vec<std::ffi::CString> = Vec::new();
    let fields: Vec<BethkitNamedField> = view
        .iter()
        .map(|fe| {
            let value = convert_field_value(&fe.value, &mut owned_strings);
            BethkitNamedField {
                name: intern_str(&fe.name, &mut owned_strings),
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
        FieldValue::String(s) => {
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
        FieldValue::FormId { value, targets } => {
            if targets.is_empty() {
                BethkitFieldValue {
                    kind: BethkitFieldValueKind::FormId,
                    payload: BethkitFieldValuePayload { form_id: value.0 },
                }
            } else {
                BethkitFieldValue {
                    kind: BethkitFieldValueKind::FormIdTyped,
                    payload: BethkitFieldValuePayload {
                        form_id_typed: BethkitTypedFormId {
                            raw: value.0,
                            allowed_sigs: targets.as_ptr() as *const [u8; 4],
                            allowed_count: targets.len(),
                        },
                    },
                }
            }
        }
        FieldValue::Bytes(b) => BethkitFieldValue {
            kind: BethkitFieldValueKind::Bytes,
            payload: BethkitFieldValuePayload {
                bytes: ManuallyDrop::new(BethkitSlice {
                    ptr: b.as_ptr(),
                    len: b.len(),
                }),
            },
        },
        FieldValue::Enumeration { value, name } => BethkitFieldValue {
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
                        name: intern_str(&fe.name, owned_strings),
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
        FieldValue::Absent => BethkitFieldValue {
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
