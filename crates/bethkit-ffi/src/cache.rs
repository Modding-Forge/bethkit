// SPDX-License-Identifier: Apache-2.0
//!
//! FFI functions for the multi-plugin record cache.
//!
//! # Ownership
//!
//! [`BethkitPluginCache`] is an owned, heap-allocated handle that must be
//! freed with [`bethkit_plugin_cache_free`].
//!
//! [`bethkit_plugin_cache_add`] **takes ownership** of the `BethkitPlugin`
//! pointer passed to it.  The caller must not use or free that pointer after
//! the call.
//!
//! [`BethkitRecord`] pointers returned by cache lookup functions are
//! borrowed from the cache's internal storage and are valid until the cache
//! is freed.

use std::ffi::c_char;

use bethkit_core::PluginCache;

use crate::load_order::BethkitGlobalFormId;
use crate::plugin::BethkitPlugin;
use crate::record::BethkitRecord;
use crate::{cstr_to_str, null_check, set_last_error};

/// An opaque handle to a multi-plugin record cache.
///
/// Created by [`bethkit_plugin_cache_new`].  Must be freed with
/// [`bethkit_plugin_cache_free`].
pub struct BethkitPluginCache(PluginCache);

/// Creates a new, empty plugin cache.
///
/// Returns a pointer to the handle.  Must be freed with
/// [`bethkit_plugin_cache_free`].
#[no_mangle]
pub extern "C" fn bethkit_plugin_cache_new() -> *mut BethkitPluginCache {
    Box::into_raw(Box::new(BethkitPluginCache(PluginCache::new())))
}

/// Frees a plugin cache handle.  Passing a null pointer is a no-op.
///
/// After this call every record pointer obtained from this cache is invalid.
#[no_mangle]
pub extern "C" fn bethkit_plugin_cache_free(cache: *mut BethkitPluginCache) {
    if cache.is_null() {
        return;
    }
    // SAFETY: cache was produced by Box::into_raw.
    drop(unsafe { Box::from_raw(cache) });
}

/// Adds `plugin` to the cache under `name`.
///
/// This function **takes ownership** of `plugin`.  The caller must not use
/// or free `plugin` after this call.
///
/// Returns 0 on success or -1 on error.
///
/// # Arguments
///
/// * `cache`  — Cache to add the plugin to. Borrows.
/// * `name`   — NUL-terminated plugin file name. Borrows.
/// * `plugin` — Plugin handle to add. Ownership transferred.
///
/// # Errors
///
/// Returns -1 and sets the last error if `cache`, `name`, or `plugin` is
/// null, or `name` contains invalid UTF-8.
#[no_mangle]
pub extern "C" fn bethkit_plugin_cache_add(
    cache: *mut BethkitPluginCache,
    name: *const c_char,
    plugin: *mut BethkitPlugin,
) -> i32 {
    null_check!(cache, "bethkit_plugin_cache_add", -1);
    null_check!(name, "bethkit_plugin_cache_add/name", -1);
    null_check!(plugin, "bethkit_plugin_cache_add/plugin", -1);

    let name_str = match cstr_to_str(name, "bethkit_plugin_cache_add") {
        Some(s) => s,
        None => return -1,
    };

    // SAFETY: plugin is non-null and was produced by bethkit_plugin_open*.
    // SAFETY: We take ownership by reconstructing the Box, then extract
    // SAFETY: the inner Plugin and move it into the cache.
    let boxed_plugin = unsafe { Box::from_raw(plugin) };
    let inner_plugin = boxed_plugin.inner;

    // SAFETY: cache is non-null.
    if let Err(e) = unsafe { &mut *cache }.0.add(name_str, inner_plugin) {
        set_last_error(format!("bethkit_plugin_cache_add: {e}"));
        return -1;
    }
    0
}

/// Returns the number of plugins in the cache.
///
/// Returns 0 and sets the last error if `cache` is null.
#[no_mangle]
pub extern "C" fn bethkit_plugin_cache_len(cache: *const BethkitPluginCache) -> usize {
    null_check!(cache, "bethkit_plugin_cache_len", 0);
    // SAFETY: cache is non-null.
    unsafe { &*cache }.0.len()
}

/// Returns the total number of records across all plugins in the cache.
///
/// Returns 0 and sets the last error if `cache` is null.
#[no_mangle]
pub extern "C" fn bethkit_plugin_cache_record_count(cache: *const BethkitPluginCache) -> usize {
    null_check!(cache, "bethkit_plugin_cache_record_count", 0);
    // SAFETY: cache is non-null.
    unsafe { &*cache }.0.record_count()
}

/// Resolves a global FormID (plugin name + object ID) to the winning record.
///
/// Returns a borrowed pointer to the record on success, or null if not found.
/// The returned pointer is valid until the cache is freed.
///
/// # Arguments
///
/// * `cache`       — Cache to search. Borrows.
/// * `plugin_name` — NUL-terminated plugin file name. Borrows.
/// * `object_id`   — The 24-bit object ID component.
///
/// # Errors
///
/// Returns null and sets the last error if `cache` or `plugin_name` is null,
/// or `plugin_name` contains invalid UTF-8.
#[no_mangle]
pub extern "C" fn bethkit_plugin_cache_resolve(
    cache: *const BethkitPluginCache,
    plugin_name: *const c_char,
    object_id: u32,
) -> *const BethkitRecord {
    null_check!(cache, "bethkit_plugin_cache_resolve", std::ptr::null());
    null_check!(
        plugin_name,
        "bethkit_plugin_cache_resolve/plugin_name",
        std::ptr::null()
    );

    let name_str = match cstr_to_str(plugin_name, "bethkit_plugin_cache_resolve") {
        Some(s) => s,
        None => return std::ptr::null(),
    };

    let gfid = bethkit_core::GlobalFormId {
        plugin_name: name_str.to_owned(),
        object_id,
    };

    // SAFETY: cache is non-null.
    match unsafe { &*cache }.0.resolve_record(&gfid) {
        Some(r) => r as *const _ as *const BethkitRecord,
        None => std::ptr::null(),
    }
}

/// Searches all plugins in the cache for a record with the given editor ID.
///
/// On success, writes the global FormID into `*out_gfid` (the `plugin_name`
/// pointer inside is borrowed from the cache and valid until the cache is
/// freed) and returns a borrowed pointer to the record.
///
/// Returns null if no matching record is found.
///
/// # Arguments
///
/// * `cache`     — Cache to search. Borrows.
/// * `edid`      — NUL-terminated editor ID string. Borrows.
/// * `out_gfid`  — Written with the global FormID on success. May be null
///   (in which case the FormID is not written).
///
/// # Errors
///
/// Returns null and sets the last error if `cache` or `edid` is null.
#[no_mangle]
pub extern "C" fn bethkit_plugin_cache_find_by_editor_id(
    cache: *const BethkitPluginCache,
    edid: *const c_char,
    out_gfid: *mut BethkitGlobalFormId,
) -> *const BethkitRecord {
    null_check!(
        cache,
        "bethkit_plugin_cache_find_by_editor_id",
        std::ptr::null()
    );
    null_check!(
        edid,
        "bethkit_plugin_cache_find_by_editor_id/edid",
        std::ptr::null()
    );

    let edid_str = match cstr_to_str(edid, "bethkit_plugin_cache_find_by_editor_id") {
        Some(s) => s,
        None => return std::ptr::null(),
    };

    // SAFETY: cache is non-null.
    match unsafe { &*cache }.0.find_by_editor_id(edid_str) {
        None => {
            set_last_error(format!(
                "bethkit_plugin_cache_find_by_editor_id: editor ID '{edid_str}' not found"
            ));
            std::ptr::null()
        }
        Some((gfid, record)) => {
            if !out_gfid.is_null() {
                // SAFETY: out_gfid is non-null.
                unsafe {
                    *out_gfid = BethkitGlobalFormId {
                        plugin_name: gfid.plugin_name.as_ptr().cast::<c_char>(),
                        object_id: gfid.object_id,
                    };
                }
            }
            record as *const _ as *const BethkitRecord
        }
    }
}
