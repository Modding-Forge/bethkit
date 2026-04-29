// SPDX-License-Identifier: Apache-2.0
//!
//! FFI functions for opening, inspecting, and freeing plugin files.
//!
//! # Ownership
//!
//! [`BethkitPlugin`] is an owned, heap-allocated handle.  The caller is
//! responsible for freeing it with [`bethkit_plugin_free`].  All borrowed
//! handles produced from a plugin (`BethkitGroup`, `BethkitRecord`, …) are
//! valid only while the parent `BethkitPlugin` is alive.

use std::ffi::{c_char, CString};
use std::path::Path;

use bethkit_core::Plugin;

use crate::group::BethkitGroup;
use crate::record::BethkitRecord;
use crate::types::{BethkitGame, BethkitPluginKind, game_to_ctx, plugin_kind_from_rust};
use crate::{cstr_to_str, ffi_try, null_check, set_last_error};

/// An opaque handle to an opened Bethesda plugin file.
///
/// Created by [`bethkit_plugin_open`] or [`bethkit_plugin_open_from_bytes`].
/// Must be freed with [`bethkit_plugin_free`].
pub struct BethkitPlugin {
    pub(crate) inner: Plugin,
    /// Interned master name strings for stable `*const c_char` return values.
    pub(crate) master_cstrings: Vec<CString>,
    /// Optional description CString.
    pub(crate) description_cstring: Option<CString>,
}

impl BethkitPlugin {
    fn new(plugin: Plugin) -> Self {
        let master_cstrings = plugin
            .masters()
            .iter()
            .map(|m| {
                let sanitized: Vec<u8> =
                    m.bytes().map(|b| if b == 0 { b'?' } else { b }).collect();
                // PANICS: sanitized has no interior NUL bytes by construction.
                CString::new(sanitized).expect("master name has no interior NULs")
            })
            .collect();

        let description_cstring = plugin.header.description.as_deref().map(|d| {
            let sanitized: Vec<u8> =
                d.bytes().map(|b| if b == 0 { b'?' } else { b }).collect();
            // PANICS: sanitized has no interior NUL bytes by construction.
            CString::new(sanitized).expect("description has no interior NULs")
        });

        Self { inner: plugin, master_cstrings, description_cstring }
    }
}

/// Opens a plugin file from `path` for the specified `game`.
///
/// Returns a pointer to the plugin handle on success, or null on failure
/// (call [`bethkit_last_error`] for details).  The caller owns the returned
/// handle and must free it with [`bethkit_plugin_free`].
///
/// # Arguments
///
/// * `path`  — NUL-terminated UTF-8 path to the plugin file. Borrows.
/// * `game`  — The game the plugin was created for.
///
/// # Errors
///
/// Returns null and sets the last error when `path` is null, the path is not
/// valid UTF-8, or the plugin file cannot be parsed.
#[no_mangle]
pub extern "C" fn bethkit_plugin_open(
    path: *const c_char,
    game: BethkitGame,
) -> *mut BethkitPlugin {
    null_check!(path, "bethkit_plugin_open", std::ptr::null_mut());
    let path_str = match cstr_to_str(path, "bethkit_plugin_open") {
        Some(s) => s,
        None => return std::ptr::null_mut(),
    };
    let plugin = ffi_try!(
        Plugin::open(Path::new(path_str), game_to_ctx(game)).map_err(crate::error::FfiError::Core),
        std::ptr::null_mut()
    );
    Box::into_raw(Box::new(BethkitPlugin::new(plugin)))
}

/// Opens a plugin from a byte slice already loaded in memory.
///
/// Returns a pointer to the plugin handle on success, or null on failure.
/// The caller owns the returned handle and must free it with
/// [`bethkit_plugin_free`].
///
/// # Arguments
///
/// * `data` — Pointer to the first byte of the plugin data. Borrows.
/// * `len`  — Number of bytes in `data`.
/// * `game` — The game the plugin was created for.
///
/// # Errors
///
/// Returns null and sets the last error when `data` is null or the bytes
/// cannot be parsed as a valid plugin.
#[no_mangle]
pub extern "C" fn bethkit_plugin_open_from_bytes(
    data: *const u8,
    len: usize,
    game: BethkitGame,
) -> *mut BethkitPlugin {
    null_check!(data, "bethkit_plugin_open_from_bytes", std::ptr::null_mut());
    // SAFETY: caller guarantees `data` is valid for `len` bytes.
    let bytes: &[u8] = unsafe { std::slice::from_raw_parts(data, len) };
    let plugin = ffi_try!(
        Plugin::from_bytes(bytes, game_to_ctx(game)).map_err(crate::error::FfiError::Core),
        std::ptr::null_mut()
    );
    Box::into_raw(Box::new(BethkitPlugin::new(plugin)))
}

/// Frees a plugin handle previously returned by [`bethkit_plugin_open`] or
/// [`bethkit_plugin_open_from_bytes`].
///
/// Passing a null pointer is a no-op.  After this call every borrowed handle
/// derived from `plugin` (records, groups, subrecords) is invalid.
#[no_mangle]
pub extern "C" fn bethkit_plugin_free(plugin: *mut BethkitPlugin) {
    if plugin.is_null() {
        return;
    }
    // SAFETY: plugin was produced by Box::into_raw and is not null.
    drop(unsafe { Box::from_raw(plugin) });
}

/// Returns the [`BethkitPluginKind`] (Full, Light, or Overlay) of `plugin`.
///
/// # Errors
///
/// Returns [`BethkitPluginKind::Full`] as a sentinel and sets the last error
/// if `plugin` is null.
#[no_mangle]
pub extern "C" fn bethkit_plugin_kind(plugin: *const BethkitPlugin) -> BethkitPluginKind {
    null_check!(plugin, "bethkit_plugin_kind", BethkitPluginKind::Full);
    // SAFETY: plugin is non-null and was produced by bethkit_plugin_open*.
    let p = unsafe { &*plugin };
    plugin_kind_from_rust(p.inner.kind())
}

/// Returns `true` if the plugin has the LOCALIZED flag set, `false` otherwise.
///
/// # Errors
///
/// Returns `false` and sets the last error if `plugin` is null.
#[no_mangle]
pub extern "C" fn bethkit_plugin_is_localized(plugin: *const BethkitPlugin) -> bool {
    null_check!(plugin, "bethkit_plugin_is_localized", false);
    // SAFETY: plugin is non-null and was produced by bethkit_plugin_open*.
    unsafe { &*plugin }.inner.is_localized()
}

/// Returns the number of master files listed in the plugin header.
///
/// # Errors
///
/// Returns 0 and sets the last error if `plugin` is null.
#[no_mangle]
pub extern "C" fn bethkit_plugin_master_count(plugin: *const BethkitPlugin) -> usize {
    null_check!(plugin, "bethkit_plugin_master_count", 0);
    // SAFETY: plugin is non-null.
    unsafe { &*plugin }.master_cstrings.len()
}

/// Returns a pointer to the NUL-terminated master file name at `index`, or
/// null if `index` is out of bounds.
///
/// The returned pointer is borrowed from `plugin` and is valid until
/// [`bethkit_plugin_free`] is called.
///
/// # Errors
///
/// Returns null and sets the last error if `plugin` is null or `index` is
/// out of bounds.
#[no_mangle]
pub extern "C" fn bethkit_plugin_master_get(
    plugin: *const BethkitPlugin,
    index: usize,
) -> *const c_char {
    null_check!(plugin, "bethkit_plugin_master_get", std::ptr::null());
    // SAFETY: plugin is non-null.
    let p = unsafe { &*plugin };
    match p.master_cstrings.get(index) {
        Some(s) => s.as_ptr(),
        None => {
            set_last_error(format!(
                "bethkit_plugin_master_get: index {index} out of bounds (len = {})",
                p.master_cstrings.len()
            ));
            std::ptr::null()
        }
    }
}

/// Returns a pointer to the NUL-terminated plugin description, or null if the
/// plugin has no description.
///
/// The returned pointer is borrowed from `plugin` and is valid until
/// [`bethkit_plugin_free`] is called.
///
/// # Errors
///
/// Returns null and sets the last error if `plugin` is null.
#[no_mangle]
pub extern "C" fn bethkit_plugin_description(plugin: *const BethkitPlugin) -> *const c_char {
    null_check!(plugin, "bethkit_plugin_description", std::ptr::null());
    // SAFETY: plugin is non-null.
    match &unsafe { &*plugin }.description_cstring {
        Some(s) => s.as_ptr(),
        None => std::ptr::null(),
    }
}

/// Returns the number of top-level record groups in `plugin`.
///
/// # Errors
///
/// Returns 0 and sets the last error if `plugin` is null.
#[no_mangle]
pub extern "C" fn bethkit_plugin_group_count(plugin: *const BethkitPlugin) -> usize {
    null_check!(plugin, "bethkit_plugin_group_count", 0);
    // SAFETY: plugin is non-null.
    unsafe { &*plugin }.inner.group_count()
}

/// Returns a borrowed pointer to the group at `index`, or null if `index` is
/// out of bounds.
///
/// The returned pointer is borrowed from `plugin` and must not be freed.
///
/// # Errors
///
/// Returns null and sets the last error if `plugin` is null or `index` is
/// out of bounds.
#[no_mangle]
pub extern "C" fn bethkit_plugin_group_get(
    plugin: *const BethkitPlugin,
    index: usize,
) -> *const BethkitGroup {
    null_check!(plugin, "bethkit_plugin_group_get", std::ptr::null());
    // SAFETY: plugin is non-null.
    let p = unsafe { &*plugin };
    match p.inner.groups().get(index) {
        Some(g) => g as *const _ as *const BethkitGroup,
        None => {
            set_last_error(format!(
                "bethkit_plugin_group_get: index {index} out of bounds (len = {})",
                p.inner.group_count()
            ));
            std::ptr::null()
        }
    }
}

/// Searches for a record with the given `form_id` inside `plugin`.
///
/// Returns a borrowed pointer to the first matching record, or null if not
/// found.  The returned pointer is borrowed from `plugin` and must not be
/// freed.
///
/// # Errors
///
/// Returns null and sets the last error if `plugin` is null.
#[no_mangle]
pub extern "C" fn bethkit_plugin_find_record(
    plugin: *const BethkitPlugin,
    form_id: u32,
) -> *const BethkitRecord {
    null_check!(plugin, "bethkit_plugin_find_record", std::ptr::null());
    // SAFETY: plugin is non-null.
    let p = unsafe { &*plugin };
    match p.inner.find_record(bethkit_core::FormId(form_id)) {
        Some(r) => r as *const _ as *const BethkitRecord,
        None => std::ptr::null(),
    }
}
