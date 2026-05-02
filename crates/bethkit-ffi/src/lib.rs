// SPDX-License-Identifier: Apache-2.0
//!
//! `bethkit-ffi` - C ABI layer for `bethkit-core` and `bethkit-bsa`.
//!
//! # Error model
//!
//! Every FFI function returns a signed integer result code where 0 means success
//! and -1 means an error has occurred.  Functions that return a pointer signal
//! failure by returning a null pointer.  The human-readable error message is
//! stored in a thread-local buffer and can be retrieved with
//! [`bethkit_last_error`].
//!
//! # Memory ownership
//!
//! Opaque handle types (`BethkitPlugin`, `BethkitArchive`, …) are heap-allocated
//! and must be freed with their matching `*_free` function.  Borrowed handle
//! types (`BethkitRecord`, `BethkitGroup`, `BethkitSubRecord`, …) are raw
//! pointers into the memory owned by their parent handle; never free them
//! directly.  String pointers (`*const c_char`) are borrowed from the owning
//! object and are valid only until that object is freed or the next FFI call on
//! the same thread.

use std::ffi::{c_char, CStr};
mod archive;
mod cache;
mod error;
mod group;
mod load_order;
mod plugin;
mod record;
mod schema;
mod strings;
mod types;
mod writer;

pub(crate) use error::set_last_error;
pub use error::{bethkit_last_error, FfiError, Result};

/// A non-owning view of a byte slice passed across the FFI boundary.
///
/// `ptr` points into memory owned by the object that produced this slice;
/// the slice is valid as long as the owning object is alive.
#[repr(C)]
pub struct BethkitSlice {
    /// Pointer to the first byte of the slice.
    pub ptr: *const u8,
    /// Number of bytes in the slice.
    pub len: usize,
}

/// Frees a byte buffer that was returned as an owned allocation by a
/// `bethkit_*` function (e.g. `bethkit_archive_extract`,
/// `bethkit_plugin_writer_write_to_bytes`).
///
/// Passing a null pointer is a no-op.  The `len` argument must exactly match
/// the `out_len` value written by the producing function.
///
/// # Safety
///
/// `ptr` must have been produced by a `bethkit_*` function that transfers
/// ownership to the caller, and `len` must match the corresponding `out_len`.
/// After this call, `ptr` is no longer valid.
#[no_mangle]
pub unsafe extern "C" fn bethkit_bytes_free(ptr: *mut u8, len: usize) {
    if ptr.is_null() {
        return;
    }
    // SAFETY: ptr was produced by Box::into_raw(vec.into_boxed_slice()), so
    // SAFETY: it is valid, non-null, correctly aligned, and `len` bytes long.
    drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, len)) });
}

/// Checks that `$ptr` is not null; if it is, sets the last error and returns
/// `$ret` from the enclosing function.
///
/// `$ctx` must be a `&'static str` that identifies the FFI function / parameter.
macro_rules! null_check {
    ($ptr:expr, $ctx:expr, $ret:expr) => {
        if $ptr.is_null() {
            $crate::set_last_error(format!(
                "null pointer passed to {}: {}",
                $ctx,
                stringify!($ptr)
            ));
            return $ret;
        }
    };
}

/// Executes `$expr` inside `catch_unwind` and maps the result to a C return
/// value.
///
/// On `Ok(Ok(v))` the expression value `v` is returned.
/// On `Ok(Err(e))` the error is stored via [`set_last_error`] and `$err_ret`
/// is returned.
/// On `Err(_)` (panic) `"internal panic"` is stored and `$err_ret` is returned.
macro_rules! ffi_try {
    ($expr:expr, $err_ret:expr) => {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| $expr)) {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => {
                $crate::set_last_error(e.to_string());
                return $err_ret;
            }
            Err(_) => {
                $crate::set_last_error("internal panic in bethkit FFI");
                return $err_ret;
            }
        }
    };
}

pub(crate) use {ffi_try, null_check};

/// Converts a raw `*const c_char` to a `&str`, storing the last error and
/// returning `None` if the pointer is null or the bytes are not valid UTF-8.
pub(crate) fn cstr_to_str<'a>(ptr: *const c_char, ctx: &'static str) -> Option<&'a str> {
    if ptr.is_null() {
        set_last_error(format!("null string pointer in {ctx}"));
        return None;
    }
    // SAFETY: caller guarantees ptr is a valid NUL-terminated C string.
    match unsafe { CStr::from_ptr(ptr) }.to_str() {
        Ok(s) => Some(s),
        Err(e) => {
            set_last_error(format!("invalid UTF-8 in {ctx}: {e}"));
            None
        }
    }
}
