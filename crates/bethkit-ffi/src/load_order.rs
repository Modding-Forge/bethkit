// SPDX-License-Identifier: Apache-2.0
//!
//! FFI functions for building a load order and resolving FormIDs.
//!
//! # Ownership
//!
//! [`BethkitLoadOrder`] is an owned, heap-allocated handle that must be freed
//! with [`bethkit_load_order_free`].
//!
//! [`BethkitGlobalFormId`] is a value type (returned by value into a
//! caller-supplied out-parameter).  Its `plugin_name` pointer is borrowed
//! from the load order's internal storage and is valid until the load order
//! is freed.

use std::ffi::c_char;

use bethkit_core::LoadOrder;

use crate::types::BethkitPluginKind;
use crate::{cstr_to_str, ffi_try, null_check, set_last_error};
use crate::error::FfiError;


/// A globally unique FormID, combining the source plugin name and a
/// 24-bit object ID.
#[repr(C)]
pub struct BethkitGlobalFormId {
    /// NUL-terminated plugin file name.  Borrowed from the owning
    /// [`BethkitLoadOrder`] or [`BethkitPluginCache`]; valid until that
    /// object is freed.
    pub plugin_name: *const c_char,
    /// The 24-bit object ID component of the global FormID.
    pub object_id: u32,
}

/// An opaque handle to an ordered list of plugin files.
///
/// Created by [`bethkit_load_order_new`].  Must be freed with
/// [`bethkit_load_order_free`].
pub struct BethkitLoadOrder {
    inner: LoadOrder,
    /// Interned name strings for stable `plugin_name` pointers.
    name_cstrings: Vec<std::ffi::CString>,
}


/// Creates a new, empty load order.
///
/// Returns a pointer to the handle.  Must be freed with
/// [`bethkit_load_order_free`].
#[no_mangle]
pub extern "C" fn bethkit_load_order_new() -> *mut BethkitLoadOrder {
    Box::into_raw(Box::new(BethkitLoadOrder {
        inner: LoadOrder::new(),
        name_cstrings: Vec::new(),
    }))
}

/// Frees a load order handle.  Passing a null pointer is a no-op.
#[no_mangle]
pub extern "C" fn bethkit_load_order_free(lo: *mut BethkitLoadOrder) {
    if lo.is_null() {
        return;
    }
    // SAFETY: lo was produced by Box::into_raw.
    drop(unsafe { Box::from_raw(lo) });
}


/// Appends `name` to the load order with the given plugin `kind`.
///
/// Returns 0 on success or -1 on error.
///
/// # Errors
///
/// Returns -1 and sets the last error if `lo` or `name` is null, or `name`
/// contains invalid UTF-8.
#[no_mangle]
pub extern "C" fn bethkit_load_order_push(
    lo: *mut BethkitLoadOrder,
    name: *const c_char,
    kind: BethkitPluginKind,
) -> i32 {
    null_check!(lo, "bethkit_load_order_push", -1);
    null_check!(name, "bethkit_load_order_push/name", -1);

    let name_str = match cstr_to_str(name, "bethkit_load_order_push") {
        Some(s) => s,
        None => return -1,
    };

    let rust_kind = match kind {
        BethkitPluginKind::Full | BethkitPluginKind::Overlay => bethkit_core::PluginKind::Plugin,
        BethkitPluginKind::Light => bethkit_core::PluginKind::Light,
    };

    // SAFETY: lo is non-null.
    let handle = unsafe { &mut *lo };
    if let Err(e) = handle.inner.push(name_str, rust_kind) {
        set_last_error(format!("bethkit_load_order_push: {e}"));
        return -1;
    }

    // Intern a stable CString so resolve can return borrowed plugin_name ptrs.
    let sanitized: Vec<u8> =
        name_str.bytes().map(|b| if b == 0 { b'?' } else { b }).collect();
    let cs = ffi_try!(
        std::ffi::CString::new(sanitized).map_err(FfiError::Nul),
        -1
    );
    handle.name_cstrings.push(cs);
    0
}

/// Returns the number of plugins in the load order.
///
/// Returns 0 and sets the last error if `lo` is null.
#[no_mangle]
pub extern "C" fn bethkit_load_order_len(lo: *const BethkitLoadOrder) -> usize {
    null_check!(lo, "bethkit_load_order_len", 0);
    // SAFETY: lo is non-null.
    unsafe { &*lo }.inner.len()
}

/// Resolves `form_id` (as seen in `source_plugin`) to a
/// [`BethkitGlobalFormId`] and writes it into `*out`.
///
/// Returns 0 on success, or -1 if the FormID cannot be resolved (e.g.
/// master index out of range).
///
/// # Arguments
///
/// * `lo`            — Load order. Borrows.
/// * `form_id`       — The file-local FormID to resolve.
/// * `source_plugin` — NUL-terminated name of the plugin that contains
///   `form_id`. Borrows.
/// * `out`           — Written with the resolved global FormID on success.
///
/// # Errors
///
/// Returns -1 and sets the last error if any pointer is null, `source_plugin`
/// has invalid UTF-8, `source_plugin` is not in the load order, or the master
/// index is out of range.
#[no_mangle]
pub extern "C" fn bethkit_load_order_resolve(
    lo: *const BethkitLoadOrder,
    form_id: u32,
    source_plugin: *const c_char,
    out: *mut BethkitGlobalFormId,
) -> i32 {
    null_check!(lo, "bethkit_load_order_resolve", -1);
    null_check!(source_plugin, "bethkit_load_order_resolve/source_plugin", -1);
    null_check!(out, "bethkit_load_order_resolve/out", -1);

    let src = match cstr_to_str(source_plugin, "bethkit_load_order_resolve") {
        Some(s) => s,
        None => return -1,
    };

    // SAFETY: lo is non-null.
    let handle = unsafe { &*lo };

    // The LoadOrder::resolve method requires the list of masters from the
    // source plugin, which we do not have here. Resolve with empty masters
    // for now — the caller is expected to pass a top-level plugin name.
    let gfid = match handle.inner.resolve(
        bethkit_core::FormId(form_id),
        src,
        &[],
    ) {
        Some(g) => g,
        None => {
            set_last_error(format!(
                "bethkit_load_order_resolve: could not resolve FormID {form_id:#010x} \
                 from plugin '{src}'"
            ));
            return -1;
        }
    };

    // Find the interned CString for the resolved plugin name.
    let name_ptr = handle
        .name_cstrings
        .iter()
        .find(|cs| cs.to_str().ok() == Some(gfid.plugin_name.as_str()))
        .map(|cs| cs.as_ptr())
        .unwrap_or(std::ptr::null());

    // SAFETY: out is non-null.
    unsafe {
        *out = BethkitGlobalFormId { plugin_name: name_ptr, object_id: gfid.object_id };
    }
    0
}

