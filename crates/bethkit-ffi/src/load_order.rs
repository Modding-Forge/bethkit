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

use crate::error::FfiError;
use crate::plugin::BethkitPlugin;
use crate::types::BethkitPluginKind;
use crate::{cstr_to_str, ffi_try, null_check, set_last_error};

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
    let sanitized: Vec<u8> = name_str
        .to_lowercase()
        .bytes()
        .map(|b| if b == 0 { b'?' } else { b })
        .collect();
    let cs = ffi_try!(std::ffi::CString::new(sanitized).map_err(FfiError::Nul), -1);
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
/// This compatibility endpoint assumes no masters. For plugins with masters,
/// use [`bethkit_load_order_resolve_with_plugin`]. Names are returned lowercase.
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
    null_check!(
        source_plugin,
        "bethkit_load_order_resolve/source_plugin",
        -1
    );
    null_check!(out, "bethkit_load_order_resolve/out", -1);

    let src = match cstr_to_str(source_plugin, "bethkit_load_order_resolve") {
        Some(s) => s,
        None => return -1,
    };

    // SAFETY: lo is non-null.
    let handle = unsafe { &*lo };

    // This compatibility endpoint assumes a source plugin with no masters.
    let gfid = match handle
        .inner
        .resolve(bethkit_core::FormId(form_id), src, &[])
    {
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
        .map(|cs| cs.as_ptr());
    let Some(name_ptr) = name_ptr else {
        set_last_error("bethkit_load_order_resolve: owner is not registered in the load order");
        return -1;
    };

    // SAFETY: out is non-null.
    unsafe {
        *out = BethkitGlobalFormId {
            plugin_name: name_ptr,
            object_id: gfid.object_id,
        };
    }
    0
}

/// Resolves a file-local FormID using the source plugin's ordered masters.
///
/// Borrows `lo` and `plugin`; `source_plugin` is the source filename. Writes
/// `out` and returns 0 on success. Its name is borrowed until `lo` is freed.
///
/// # Errors
///
/// Returns -1 for null pointers, invalid UTF-8, unregistered source or owner,
/// invalid master indexes, or an internal panic. The output remains unchanged.
///
/// # Safety
///
/// Handles must be live, `source_plugin` must be a readable NUL-terminated
/// string, and `out` must point to writable storage for one global ID.
#[no_mangle]
pub extern "C" fn bethkit_load_order_resolve_with_plugin(
    lo: *const BethkitLoadOrder,
    form_id: u32,
    source_plugin: *const c_char,
    plugin: *const BethkitPlugin,
    out: *mut BethkitGlobalFormId,
) -> i32 {
    null_check!(lo, "bethkit_load_order_resolve_with_plugin", -1);
    null_check!(plugin, "bethkit_load_order_resolve_with_plugin/plugin", -1);
    null_check!(
        source_plugin,
        "bethkit_load_order_resolve_with_plugin/source",
        -1
    );
    null_check!(out, "bethkit_load_order_resolve_with_plugin/out", -1);
    let Some(source) = cstr_to_str(source_plugin, "bethkit_load_order_resolve_with_plugin") else {
        return -1;
    };
    let resolved = ffi_try!(
        (|| {
            // SAFETY: handles are live and borrowed for the duration of this call.
            let (handle, source_handle) = unsafe { (&*lo, &*plugin) };
            let invalid = || FfiError::InvalidArgument {
                context: "load-order FormID resolution",
                message: format!("cannot resolve {form_id:#010x} from '{source}'"),
            };
            let canonical = source.to_lowercase();
            if !handle
                .inner
                .entries()
                .iter()
                .any(|entry| entry.name == canonical)
            {
                return Err(invalid());
            }
            let global = handle
                .inner
                .resolve(
                    bethkit_core::FormId(form_id),
                    source,
                    source_handle.inner.masters(),
                )
                .ok_or_else(invalid)?;
            let name = handle
                .name_cstrings
                .iter()
                .find(|name| name.as_bytes() == global.plugin_name.as_bytes())
                .ok_or_else(invalid)?;
            Ok::<_, FfiError>(BethkitGlobalFormId {
                plugin_name: name.as_ptr(),
                object_id: global.object_id,
            })
        })(),
        -1
    );
    // SAFETY: out is valid writable storage and does not alias either handle.
    unsafe { *out = resolved };
    0
}

#[cfg(test)]
mod tests {
    use std::ffi::CStr;

    use bethkit_core::{GameContext, PluginWriter};

    use super::*;
    use crate::plugin::{bethkit_plugin_free, bethkit_plugin_open_from_bytes};
    use crate::types::BethkitGame;

    /// Checks canonical names and file-local master and source indexes.
    #[test]
    fn resolves_master_and_source_names() -> Result<(), Box<dyn std::error::Error>> {
        let mut writer = PluginWriter::new(GameContext::sse(), 1.7);
        writer.add_master("Skyrim.esm");
        let bytes = writer.write_to_vec()?;
        let plugin =
            bethkit_plugin_open_from_bytes(bytes.as_ptr(), bytes.len(), BethkitGame::SkyrimSe);
        assert!(!plugin.is_null());
        let lo = bethkit_load_order_new();
        assert_eq!(
            bethkit_load_order_push(lo, c"Skyrim.esm".as_ptr(), BethkitPluginKind::Full),
            0
        );
        assert_eq!(
            bethkit_load_order_push(lo, c"MyMod.esp".as_ptr(), BethkitPluginKind::Full),
            0
        );
        let mut result = BethkitGlobalFormId {
            plugin_name: std::ptr::null(),
            object_id: 0,
        };
        for (form_id, expected) in [(0x1234, "skyrim.esm"), (0x0100_1234, "mymod.esp")] {
            assert_eq!(
                bethkit_load_order_resolve_with_plugin(
                    lo,
                    form_id,
                    c"MYMOD.ESP".as_ptr(),
                    plugin,
                    &mut result
                ),
                0
            );
            // SAFETY: result refers to the still-live load order's interned C string.
            assert_eq!(
                unsafe { CStr::from_ptr(result.plugin_name) }.to_str()?,
                expected
            );
            assert_eq!(result.object_id, 0x1234);
        }
        assert_eq!(
            bethkit_load_order_resolve(lo, 0x1234, c"Skyrim.esm".as_ptr(), &mut result),
            0
        );
        assert!(!result.plugin_name.is_null());
        assert_eq!(
            bethkit_load_order_resolve(lo, 0x1234, c"Missing.esm".as_ptr(), &mut result),
            -1
        );
        bethkit_plugin_free(plugin);
        bethkit_load_order_free(lo);
        Ok(())
    }
}
