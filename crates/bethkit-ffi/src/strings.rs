// SPDX-License-Identifier: Apache-2.0
//!
//! FFI functions for string tables and localization sets.
//!
//! # Ownership
//!
//! [`BethkitStringTable`] and [`BethkitLocalizationSet`] are owned,
//! heap-allocated handles that must be freed with their matching `*_free`
//! function.
//!
//! Byte pointers returned by `*_get` functions are borrowed from the
//! table's internal storage and are valid until the table is mutated or
//! freed.

use std::ffi::c_char;
use std::path::Path;

use bethkit_core::{LocalizationSet, StringTable};

use crate::error::FfiError;
use crate::types::{string_kind_from_rust, string_kind_to_rust, BethkitStringFileKind};
use crate::{cstr_to_str, ffi_try, null_check};

/// An opaque handle to a single string table (`.strings`, `.dlstrings`, or
/// `.ilstrings`).
///
/// Created by [`bethkit_string_table_new`] or [`bethkit_string_table_open`].
/// Must be freed with [`bethkit_string_table_free`].
pub struct BethkitStringTable(StringTable);

/// Creates a new, empty string table for the given `kind`.
///
/// Returns a pointer to the handle.  Must be freed with
/// [`bethkit_string_table_free`].
///
/// # Arguments
///
/// * `kind` — The string file type to create.
#[no_mangle]
pub extern "C" fn bethkit_string_table_new(kind: BethkitStringFileKind) -> *mut BethkitStringTable {
    Box::into_raw(Box::new(BethkitStringTable(StringTable::new(
        string_kind_to_rust(kind),
    ))))
}

/// Opens and parses a string table from a file at `path`.
///
/// The file format (null-terminated vs. length-prefixed) is inferred from
/// the file extension.
///
/// Returns a pointer to the handle on success, or null on failure.  Must be
/// freed with [`bethkit_string_table_free`].
///
/// # Arguments
///
/// * `path` — NUL-terminated UTF-8 path to the string table file. Borrows.
///
/// # Errors
///
/// Returns null and sets the last error if `path` is null, the file cannot
/// be read, or the data is malformed.
#[no_mangle]
pub extern "C" fn bethkit_string_table_open(path: *const c_char) -> *mut BethkitStringTable {
    null_check!(path, "bethkit_string_table_open", std::ptr::null_mut());
    let path_str = match cstr_to_str(path, "bethkit_string_table_open") {
        Some(s) => s,
        None => return std::ptr::null_mut(),
    };
    let table = ffi_try!(
        StringTable::open(Path::new(path_str)).map_err(FfiError::Core),
        std::ptr::null_mut()
    );
    Box::into_raw(Box::new(BethkitStringTable(table)))
}

/// Frees a string table handle.  Passing a null pointer is a no-op.
#[no_mangle]
pub extern "C" fn bethkit_string_table_free(st: *mut BethkitStringTable) {
    if st.is_null() {
        return;
    }
    // SAFETY: st was produced by Box::into_raw.
    drop(unsafe { Box::from_raw(st) });
}

/// Returns the [`BethkitStringFileKind`] of `st`.
///
/// Returns [`BethkitStringFileKind::Strings`] as a sentinel and sets the last
/// error if `st` is null.
#[no_mangle]
pub extern "C" fn bethkit_string_table_kind(
    st: *const BethkitStringTable,
) -> BethkitStringFileKind {
    null_check!(
        st,
        "bethkit_string_table_kind",
        BethkitStringFileKind::Strings
    );
    // SAFETY: st is non-null.
    string_kind_from_rust(unsafe { &*st }.0.kind())
}

/// Returns the number of entries in `st`.
///
/// Returns 0 and sets the last error if `st` is null.
#[no_mangle]
pub extern "C" fn bethkit_string_table_len(st: *const BethkitStringTable) -> usize {
    null_check!(st, "bethkit_string_table_len", 0);
    // SAFETY: st is non-null.
    unsafe { &*st }.0.len()
}

/// Looks up the string with `id` in `st`.
///
/// On success, writes the byte count into `*out_len` and returns a pointer
/// to the raw string bytes.  The bytes are **borrowed** from the table and
/// are valid until the table is mutated or freed.
///
/// When `id` is not present in the table, returns null and writes `0` into
/// `*out_len` without setting the last error.
///
/// # Arguments
///
/// * `st`      — String table. Borrows.
/// * `id`      — String table entry ID.
/// * `out_len` — Written with the byte count on success, or `0` if not found.
///
/// # Errors
///
/// Returns null, writes `0` into `*out_len`, and sets the last error if
/// `st` or `out_len` is null.
#[no_mangle]
pub extern "C" fn bethkit_string_table_get(
    st: *const BethkitStringTable,
    id: u32,
    out_len: *mut usize,
) -> *const u8 {
    null_check!(st, "bethkit_string_table_get", std::ptr::null());
    null_check!(
        out_len,
        "bethkit_string_table_get/out_len",
        std::ptr::null()
    );
    // SAFETY: st and out_len are non-null.
    // Zero out_len first so every return path leaves a defined value.
    unsafe { *out_len = 0 };
    match unsafe { &*st }.0.get(id) {
        None => std::ptr::null(),
        Some(bytes) => {
            unsafe { *out_len = bytes.len() };
            bytes.as_ptr()
        }
    }
}

/// Inserts or replaces the entry with `id` in `st`.
///
/// Returns 0 on success or -1 if `st` or `data` is null.
///
/// # Arguments
///
/// * `st`   — String table. Borrows.
/// * `id`   — Entry ID to insert.
/// * `data` — Pointer to the byte payload. Borrows.
/// * `len`  — Number of bytes in `data`.
///
/// # Errors
///
/// Returns -1 and sets the last error if `st` or `data` is null.
#[no_mangle]
pub extern "C" fn bethkit_string_table_insert(
    st: *mut BethkitStringTable,
    id: u32,
    data: *const u8,
    len: usize,
) -> i32 {
    null_check!(st, "bethkit_string_table_insert", -1);
    null_check!(data, "bethkit_string_table_insert/data", -1);
    // SAFETY: st and data are non-null; data is valid for len bytes by contract.
    let bytes = unsafe { std::slice::from_raw_parts(data, len) }.to_vec();
    unsafe { &mut *st }.0.insert(id, bytes);
    0
}

/// Inserts a new entry with an auto-assigned ID and writes that ID into
/// `*out_id`.
///
/// Returns 0 on success or -1 on error.
///
/// # Arguments
///
/// * `st`     — String table. Borrows.
/// * `data`   — Pointer to the byte payload. Borrows.
/// * `len`    — Number of bytes in `data`.
/// * `out_id` — Written with the assigned ID on success.
///
/// # Errors
///
/// Returns -1 and sets the last error if `st`, `data`, or `out_id` is null.
#[no_mangle]
pub extern "C" fn bethkit_string_table_insert_new(
    st: *mut BethkitStringTable,
    data: *const u8,
    len: usize,
    out_id: *mut u32,
) -> i32 {
    null_check!(st, "bethkit_string_table_insert_new", -1);
    null_check!(data, "bethkit_string_table_insert_new/data", -1);
    null_check!(out_id, "bethkit_string_table_insert_new/out_id", -1);
    // SAFETY: st, data, and out_id are non-null; data is valid for len bytes.
    let bytes = unsafe { std::slice::from_raw_parts(data, len) }.to_vec();
    let id = unsafe { &mut *st }.0.insert_new(bytes);
    unsafe { *out_id = id };
    0
}

/// Removes the entry with `id` from `st`.
///
/// Returns `true` if the entry was present, `false` if it was absent.
///
/// # Errors
///
/// Returns `false` and sets the last error if `st` is null.
#[no_mangle]
pub extern "C" fn bethkit_string_table_remove(st: *mut BethkitStringTable, id: u32) -> bool {
    null_check!(st, "bethkit_string_table_remove", false);
    // SAFETY: st is non-null.
    unsafe { &mut *st }.0.remove(id).is_some()
}

/// Serializes `st` to a file at `path`.
///
/// Returns 0 on success or -1 on failure.
///
/// # Arguments
///
/// * `st`   — String table. Borrows.
/// * `path` — NUL-terminated UTF-8 destination path. Borrows.
///
/// # Errors
///
/// Returns -1 and sets the last error if `st` or `path` is null, or writing
/// fails.
#[no_mangle]
pub extern "C" fn bethkit_string_table_write_to_file(
    st: *const BethkitStringTable,
    path: *const c_char,
) -> i32 {
    null_check!(st, "bethkit_string_table_write_to_file", -1);
    null_check!(path, "bethkit_string_table_write_to_file/path", -1);

    let path_str = match cstr_to_str(path, "bethkit_string_table_write_to_file") {
        Some(s) => s,
        None => return -1,
    };

    let mut file = ffi_try!(
        std::fs::File::create(path_str).map_err(|e| FfiError::Io(e.into())),
        -1
    );
    // SAFETY: st is non-null.
    ffi_try!(
        unsafe { &*st }
            .0
            .write_to(&mut file)
            .map_err(|e| FfiError::Io(e.into())),
        -1
    );
    0
}

/// An opaque handle to a localization set (the three sibling string tables
/// `.strings`, `.dlstrings`, and `.ilstrings` for one plugin + language).
///
/// Created by [`bethkit_localization_set_new`] or
/// [`bethkit_localization_set_open`].  Must be freed with
/// [`bethkit_localization_set_free`].
pub struct BethkitLocalizationSet(LocalizationSet);

/// Creates a new, empty localization set.
///
/// Returns a pointer to the handle.  Must be freed with
/// [`bethkit_localization_set_free`].
#[no_mangle]
pub extern "C" fn bethkit_localization_set_new() -> *mut BethkitLocalizationSet {
    Box::into_raw(Box::new(BethkitLocalizationSet(LocalizationSet::new())))
}

/// Opens and parses the three sibling string tables for `plugin_path` and
/// `language`.
///
/// The three files are expected to follow the Skyrim naming convention:
/// `<stem>_<language>.strings`, `…dlstrings`, and `…ilstrings`.
///
/// Returns a pointer to the handle on success, or null on failure.  Must be
/// freed with [`bethkit_localization_set_free`].
///
/// # Arguments
///
/// * `plugin_path` — NUL-terminated path to the `.esp`/`.esm` file. Borrows.
/// * `language`    — NUL-terminated language code (e.g. `"english"`). Borrows.
///
/// # Errors
///
/// Returns null and sets the last error if any pointer is null, the paths
/// contain invalid UTF-8, or any string table file cannot be read.
#[no_mangle]
pub extern "C" fn bethkit_localization_set_open(
    plugin_path: *const c_char,
    language: *const c_char,
) -> *mut BethkitLocalizationSet {
    null_check!(
        plugin_path,
        "bethkit_localization_set_open",
        std::ptr::null_mut()
    );
    null_check!(
        language,
        "bethkit_localization_set_open",
        std::ptr::null_mut()
    );

    let path_str = match cstr_to_str(plugin_path, "bethkit_localization_set_open") {
        Some(s) => s,
        None => return std::ptr::null_mut(),
    };
    let lang_str = match cstr_to_str(language, "bethkit_localization_set_open/language") {
        Some(s) => s,
        None => return std::ptr::null_mut(),
    };

    let set = ffi_try!(
        LocalizationSet::open(Path::new(path_str), lang_str).map_err(FfiError::Core),
        std::ptr::null_mut()
    );
    Box::into_raw(Box::new(BethkitLocalizationSet(set)))
}

/// Frees a localization set handle.  Passing a null pointer is a no-op.
#[no_mangle]
pub extern "C" fn bethkit_localization_set_free(ls: *mut BethkitLocalizationSet) {
    if ls.is_null() {
        return;
    }
    // SAFETY: ls was produced by Box::into_raw.
    drop(unsafe { Box::from_raw(ls) });
}

/// Looks up string `id` of type `kind` in `ls`.
///
/// On success, writes the byte count into `*out_len` and returns a pointer
/// to the raw bytes.  The bytes are borrowed from the set and are valid until
/// the set is mutated or freed.
///
/// When `id` is not present, returns null and writes `0` into `*out_len`
/// without setting the last error.
///
/// # Arguments
///
/// * `ls`      — Localization set. Borrows.
/// * `kind`    — Which table to look in.
/// * `id`      — String entry ID.
/// * `out_len` — Written with the byte count on success, or `0` if not found.
///
/// # Errors
///
/// Returns null, writes `0` into `*out_len`, and sets the last error if
/// `ls` or `out_len` is null.
#[no_mangle]
pub extern "C" fn bethkit_localization_set_get(
    ls: *const BethkitLocalizationSet,
    kind: BethkitStringFileKind,
    id: u32,
    out_len: *mut usize,
) -> *const u8 {
    null_check!(ls, "bethkit_localization_set_get", std::ptr::null());
    null_check!(
        out_len,
        "bethkit_localization_set_get/out_len",
        std::ptr::null()
    );
    // SAFETY: ls and out_len are non-null.
    // Zero out_len first so every return path leaves a defined value.
    unsafe { *out_len = 0 };
    match unsafe { &*ls }.0.get(string_kind_to_rust(kind), id) {
        None => std::ptr::null(),
        Some(bytes) => {
            unsafe { *out_len = bytes.len() };
            bytes.as_ptr()
        }
    }
}

/// Inserts or replaces string `id` of type `kind` in `ls`.
///
/// Returns 0 on success or -1 on error.
///
/// # Arguments
///
/// * `ls`   — Localization set. Borrows.
/// * `kind` — Which table to write to.
/// * `id`   — Entry ID.
/// * `data` — Pointer to the byte payload. Borrows.
/// * `len`  — Number of bytes in `data`.
///
/// # Errors
///
/// Returns -1 and sets the last error if `ls` or `data` is null.
#[no_mangle]
pub extern "C" fn bethkit_localization_set_set(
    ls: *mut BethkitLocalizationSet,
    kind: BethkitStringFileKind,
    id: u32,
    data: *const u8,
    len: usize,
) -> i32 {
    null_check!(ls, "bethkit_localization_set_set", -1);
    null_check!(data, "bethkit_localization_set_set/data", -1);
    // SAFETY: ls and data are non-null; data is valid for len bytes.
    let bytes = unsafe { std::slice::from_raw_parts(data, len) }.to_vec();
    unsafe { &mut *ls }
        .0
        .set(string_kind_to_rust(kind), id, bytes);
    0
}

/// Serializes all three string tables in `ls` to the file system using
/// Skyrim's sibling-file naming convention.
///
/// Returns 0 on success or -1 on failure.
///
/// # Arguments
///
/// * `ls`          — Localization set. Borrows.
/// * `plugin_path` — NUL-terminated path to the `.esp`/`.esm` file. Borrows.
/// * `language`    — NUL-terminated language code. Borrows.
///
/// # Errors
///
/// Returns -1 and sets the last error if any pointer is null, the paths
/// contain invalid UTF-8, or writing any file fails.
#[no_mangle]
pub extern "C" fn bethkit_localization_set_write(
    ls: *const BethkitLocalizationSet,
    plugin_path: *const c_char,
    language: *const c_char,
) -> i32 {
    null_check!(ls, "bethkit_localization_set_write", -1);
    null_check!(
        plugin_path,
        "bethkit_localization_set_write/plugin_path",
        -1
    );
    null_check!(language, "bethkit_localization_set_write/language", -1);

    let path_str = match cstr_to_str(plugin_path, "bethkit_localization_set_write") {
        Some(s) => s,
        None => return -1,
    };
    let lang_str = match cstr_to_str(language, "bethkit_localization_set_write/language") {
        Some(s) => s,
        None => return -1,
    };

    // SAFETY: ls is non-null.
    ffi_try!(
        unsafe { &*ls }
            .0
            .write(Path::new(path_str), lang_str)
            .map_err(|e| FfiError::Io(e.into())),
        -1
    );
    0
}
