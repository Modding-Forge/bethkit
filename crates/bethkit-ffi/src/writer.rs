// SPDX-License-Identifier: Apache-2.0
//!
//! FFI functions for constructing and serializing plugin files.
//!
//! # Ownership
//!
//! [`BethkitPluginWriter`], [`BethkitWritableGroup`], and
//! [`BethkitWritableRecord`] are all owned, heap-allocated handles.
//!
//! - [`BethkitWritableGroup`] is consumed (ownership transferred) when passed
//!   to [`bethkit_plugin_writer_add_group`] or
//!   [`bethkit_writable_group_add_group`].  The caller must not use or free
//!   the pointer after that call.
//! - [`BethkitWritableRecord`] is consumed when passed to
//!   [`bethkit_writable_group_add_record`].
//! - [`BethkitPluginWriter`] must be freed with [`bethkit_plugin_writer_free`]
//!   (it is never consumed).
//!
//! # Buffer ownership
//!
//! Bytes returned by [`bethkit_plugin_writer_write_to_bytes`] are heap-
//! allocated and must be freed with [`bethkit_bytes_free`].

use std::ffi::c_char;
use std::path::Path;

use bethkit_core::{GameContext, PluginWriter, WritableGroup, WritableGroupChild, WritableRecord, WritableSubRecord};

use crate::error::FfiError;
use crate::types::BethkitGame;
use crate::{cstr_to_str, ffi_try, null_check};


/// An opaque handle to a plugin writer.
///
/// Created by [`bethkit_plugin_writer_new`].  Must be freed with
/// [`bethkit_plugin_writer_free`].
pub struct BethkitPluginWriter(PluginWriter);

/// An opaque handle to a writable top-level group.
///
/// Created by [`bethkit_writable_group_new`].  Ownership is transferred to
/// the plugin writer or a parent group when passed to the respective
/// `add_group` function.
pub struct BethkitWritableGroup(WritableGroup);

/// An opaque handle to a writable record.
///
/// Created by [`bethkit_writable_record_new`].  Ownership is transferred to
/// the parent group when passed to [`bethkit_writable_group_add_record`].
pub struct BethkitWritableRecord(WritableRecord);


/// Creates a new plugin writer for `game` at the given form version.
///
/// Returns a pointer to the handle.  Must be freed with
/// [`bethkit_plugin_writer_free`].
///
/// # Arguments
///
/// * `game`         — Target game.
/// * `form_version` — Plugin form version (e.g. `44.0` for Skyrim SE).
#[no_mangle]
pub extern "C" fn bethkit_plugin_writer_new(
    game: BethkitGame,
    form_version: f32,
) -> *mut BethkitPluginWriter {
    // SAFETY: game discriminant is valid; crate::types guarantees repr.
    let ctx = game_to_context(game);
    Box::into_raw(Box::new(BethkitPluginWriter(PluginWriter::new(ctx, form_version))))
}

/// Frees a plugin writer handle.  Passing a null pointer is a no-op.
#[no_mangle]
pub extern "C" fn bethkit_plugin_writer_free(pw: *mut BethkitPluginWriter) {
    if pw.is_null() {
        return;
    }
    // SAFETY: pw was produced by Box::into_raw.
    drop(unsafe { Box::from_raw(pw) });
}


/// Adds a top-level group to the plugin writer.
///
/// This function **takes ownership** of `group`.  The caller must not use
/// or free `group` after this call.
///
/// Returns 0 on success or -1 on error.
///
/// # Arguments
///
/// * `pw`    — Plugin writer. Borrows.
/// * `group` — Group to add. Ownership transferred.
///
/// # Errors
///
/// Returns -1 and sets the last error if `pw` or `group` is null.
#[no_mangle]
pub extern "C" fn bethkit_plugin_writer_add_group(
    pw: *mut BethkitPluginWriter,
    group: *mut BethkitWritableGroup,
) -> i32 {
    null_check!(pw, "bethkit_plugin_writer_add_group", -1);
    null_check!(group, "bethkit_plugin_writer_add_group/group", -1);
    // SAFETY: group is non-null and was produced by Box::into_raw.
    let g = unsafe { Box::from_raw(group) };
    // SAFETY: pw is non-null.
    unsafe { &mut *pw }.0.add_group(g.0);
    0
}


/// Serializes the plugin to a file at `path`.
///
/// Returns 0 on success or -1 on failure.
///
/// # Arguments
///
/// * `pw`   — Plugin writer. Borrows.
/// * `path` — NUL-terminated UTF-8 destination path. Borrows.
///
/// # Errors
///
/// Returns -1 and sets the last error if `pw` or `path` is null, the path
/// contains invalid UTF-8, or writing fails.
#[no_mangle]
pub extern "C" fn bethkit_plugin_writer_write_to_file(
    pw: *const BethkitPluginWriter,
    path: *const c_char,
) -> i32 {
    null_check!(pw, "bethkit_plugin_writer_write_to_file", -1);
    null_check!(path, "bethkit_plugin_writer_write_to_file/path", -1);

    let path_str = match cstr_to_str(path, "bethkit_plugin_writer_write_to_file") {
        Some(s) => s,
        None => return -1,
    };
    // SAFETY: pw is non-null.
    ffi_try!(
        unsafe { &*pw }.0.write_to_file(Path::new(path_str)).map_err(FfiError::Core),
        -1
    );
    0
}

/// Serializes the plugin to a heap-allocated byte buffer.
///
/// On success, writes the buffer length into `*out_len` and returns a pointer
/// to the buffer.  The buffer must be freed with [`bethkit_bytes_free`].
///
/// Returns null on failure.
///
/// # Arguments
///
/// * `pw`      — Plugin writer. Borrows.
/// * `out_len` — Written with the buffer size on success.
///
/// # Errors
///
/// Returns null and sets the last error if `pw` or `out_len` is null, or
/// serialization fails.
#[no_mangle]
pub extern "C" fn bethkit_plugin_writer_write_to_bytes(
    pw: *const BethkitPluginWriter,
    out_len: *mut usize,
) -> *mut u8 {
    null_check!(pw, "bethkit_plugin_writer_write_to_bytes", std::ptr::null_mut());
    null_check!(out_len, "bethkit_plugin_writer_write_to_bytes/out_len", std::ptr::null_mut());

    // SAFETY: pw is non-null.
    let bytes = ffi_try!(
        unsafe { &*pw }.0.write_to_vec().map_err(FfiError::Core),
        std::ptr::null_mut()
    );
    let len = bytes.len();
    let boxed: Box<[u8]> = bytes.into_boxed_slice();
    let ptr = boxed.as_ptr() as *mut u8;
    std::mem::forget(boxed);
    // SAFETY: out_len is non-null.
    unsafe { *out_len = len };
    ptr
}


/// Creates a new writable group with the given 4-byte `label` and
/// `group_type`.
///
/// Returns a pointer to the handle.  Ownership is transferred to the plugin
/// writer or parent group when this handle is added to one.
///
/// # Arguments
///
/// * `label`      — Pointer to 4 bytes used as the group label. Borrows.
/// * `group_type` — Bethesda group type integer.
///
/// # Errors
///
/// Returns null and sets the last error if `label` is null.
#[no_mangle]
pub extern "C" fn bethkit_writable_group_new(
    label: *const u8,
    group_type: i32,
) -> *mut BethkitWritableGroup {
    null_check!(label, "bethkit_writable_group_new", std::ptr::null_mut());
    // SAFETY: label is non-null and at least 4 bytes by contract.
    let label_bytes: [u8; 4] = unsafe { std::ptr::read(label as *const [u8; 4]) };
    Box::into_raw(Box::new(BethkitWritableGroup(WritableGroup {
        label: label_bytes,
        group_type,
        children: Vec::new(),
    })))
}

/// Frees a writable group that was **not** added to a writer or parent group.
///
/// Do not call this function after ownership has been transferred.  Passing a
/// null pointer is a no-op.
#[no_mangle]
pub extern "C" fn bethkit_writable_group_free(group: *mut BethkitWritableGroup) {
    if group.is_null() {
        return;
    }
    // SAFETY: group was produced by Box::into_raw.
    drop(unsafe { Box::from_raw(group) });
}


/// Adds a record as a child of `group`.
///
/// This function **takes ownership** of `record`.  The caller must not use
/// or free `record` after this call.
///
/// Returns 0 on success or -1 on error.
///
/// # Arguments
///
/// * `group`  — Parent group. Borrows.
/// * `record` — Record to add. Ownership transferred.
///
/// # Errors
///
/// Returns -1 and sets the last error if `group` or `record` is null.
#[no_mangle]
pub extern "C" fn bethkit_writable_group_add_record(
    group: *mut BethkitWritableGroup,
    record: *mut BethkitWritableRecord,
) -> i32 {
    null_check!(group, "bethkit_writable_group_add_record", -1);
    null_check!(record, "bethkit_writable_group_add_record/record", -1);
    // SAFETY: record is non-null and was produced by Box::into_raw.
    let r = unsafe { Box::from_raw(record) };
    // SAFETY: group is non-null.
    unsafe { &mut *group }.0.children.push(WritableGroupChild::Record(r.0));
    0
}

/// Adds a child group inside `group`.
///
/// This function **takes ownership** of `child`.  The caller must not use
/// or free `child` after this call.
///
/// Returns 0 on success or -1 on error.
///
/// # Arguments
///
/// * `group` — Parent group. Borrows.
/// * `child` — Child group to add. Ownership transferred.
///
/// # Errors
///
/// Returns -1 and sets the last error if `group` or `child` is null.
#[no_mangle]
pub extern "C" fn bethkit_writable_group_add_group(
    group: *mut BethkitWritableGroup,
    child: *mut BethkitWritableGroup,
) -> i32 {
    null_check!(group, "bethkit_writable_group_add_group", -1);
    null_check!(child, "bethkit_writable_group_add_group/child", -1);
    // SAFETY: child is non-null and was produced by Box::into_raw.
    let c = unsafe { Box::from_raw(child) };
    // SAFETY: group is non-null.
    unsafe { &mut *group }.0.children.push(WritableGroupChild::Group(c.0));
    0
}


/// Creates a new writable record.
///
/// Returns a pointer to the handle.  Ownership is transferred when the record
/// is added to a group.
///
/// # Arguments
///
/// * `signature`    — Pointer to 4 bytes for the record type signature. Borrows.
/// * `flags`        — Record flags word.
/// * `form_id`      — Raw FormID.
/// * `form_version` — Record form version.
///
/// # Errors
///
/// Returns null and sets the last error if `signature` is null.
#[no_mangle]
pub extern "C" fn bethkit_writable_record_new(
    signature: *const u8,
    flags: u32,
    form_id: u32,
    form_version: u16,
) -> *mut BethkitWritableRecord {
    null_check!(signature, "bethkit_writable_record_new", std::ptr::null_mut());
    // SAFETY: signature is non-null and at least 4 bytes by contract.
    let sig_bytes: [u8; 4] = unsafe { std::ptr::read(signature as *const [u8; 4]) };
    Box::into_raw(Box::new(BethkitWritableRecord(WritableRecord {
        signature: bethkit_core::Signature(sig_bytes),
        flags: bethkit_core::RecordFlags::from_bits_truncate(flags),
        form_id: bethkit_core::FormId(form_id),
        form_version,
        subrecords: Vec::new(),
    })))
}

/// Frees a writable record that was **not** added to a group.
///
/// Do not call this function after ownership has been transferred.  Passing a
/// null pointer is a no-op.
#[no_mangle]
pub extern "C" fn bethkit_writable_record_free(record: *mut BethkitWritableRecord) {
    if record.is_null() {
        return;
    }
    // SAFETY: record was produced by Box::into_raw.
    drop(unsafe { Box::from_raw(record) });
}

/// Appends a sub-record to `record`.
///
/// Returns 0 on success or -1 on error.
///
/// # Arguments
///
/// * `record`    — Record to append to. Borrows.
/// * `signature` — Pointer to 4 bytes for the sub-record type signature. Borrows.
/// * `data`      — Pointer to the raw sub-record payload bytes. Borrows.
/// * `data_len`  — Number of bytes in `data`.
///
/// # Errors
///
/// Returns -1 and sets the last error if `record`, `signature`, or `data` is
/// null.
#[no_mangle]
pub extern "C" fn bethkit_writable_record_add_subrecord(
    record: *mut BethkitWritableRecord,
    signature: *const u8,
    data: *const u8,
    data_len: usize,
) -> i32 {
    null_check!(record, "bethkit_writable_record_add_subrecord", -1);
    null_check!(signature, "bethkit_writable_record_add_subrecord/signature", -1);
    null_check!(data, "bethkit_writable_record_add_subrecord/data", -1);
    // SAFETY: all three pointers are non-null; data is valid for data_len bytes.
    let sig_bytes: [u8; 4] = unsafe { std::ptr::read(signature as *const [u8; 4]) };
    let payload: Vec<u8> = unsafe { std::slice::from_raw_parts(data, data_len) }.to_vec();
    unsafe { &mut *record }.0.subrecords.push(WritableSubRecord {
        signature: bethkit_core::Signature(sig_bytes),
        data: payload,
    });
    0
}


/// Maps a [`BethkitGame`] discriminant to a [`GameContext`].
fn game_to_context(game: BethkitGame) -> GameContext {
    use bethkit_core::Game;
    match game {
        BethkitGame::SkyrimSe => GameContext { game: Game::SkyrimSE },
        BethkitGame::Fallout4 => GameContext { game: Game::Fallout4 },
        BethkitGame::Skyrim => GameContext { game: Game::SkyrimLE },
        BethkitGame::Fallout3 => GameContext { game: Game::Fallout3 },
        BethkitGame::FalloutNv => GameContext { game: Game::FalloutNV },
    }
}
