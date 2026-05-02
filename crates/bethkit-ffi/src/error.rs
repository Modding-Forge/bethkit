// SPDX-License-Identifier: Apache-2.0
//!
//! Error types and thread-local last-error store for `bethkit-ffi`.
//!
//! Every FFI function signals failure via a return code (0 = success, -1 = error).
//! The human-readable message is stored in a thread-local buffer and retrieved by
//! the caller through [`bethkit_last_error`].

use std::cell::RefCell;
use std::ffi::{c_char, CString};

thread_local! {
    /// Stores the most recent error message for the current thread.
    ///
    /// The buffer is replaced on every call that sets an error, so the string is
    /// only valid until the next FFI call on the same thread.
    static LAST_ERROR: RefCell<Option<CString>> = const { RefCell::new(None) };
}

/// Records `msg` as the last error for the calling thread.
///
/// Called by [`crate::ffi_try`] and [`crate::null_check`] before returning an
/// error sentinel to the C caller.
pub(crate) fn set_last_error(msg: impl Into<Vec<u8>>) {
    let msg_bytes = msg.into();
    // Replace interior NUL bytes with '?' so CString::new never fails.
    let sanitized: Vec<u8> = msg_bytes
        .into_iter()
        .map(|b| if b == 0 { b'?' } else { b })
        .collect();
    // PANICS: sanitized contains no interior NUL bytes by construction above.
    let cstring = CString::new(sanitized).expect("sanitized string has no interior NULs");
    LAST_ERROR.with(|cell| {
        *cell.borrow_mut() = Some(cstring);
    });
}

/// Returns a pointer to the last error message for the calling thread, or a
/// pointer to an empty string if no error has occurred.
///
/// # Safety
///
/// The returned pointer is valid until the next `bethkit_*` FFI call on the
/// same thread. The caller must not free or write through this pointer.
#[no_mangle]
pub extern "C" fn bethkit_last_error() -> *const c_char {
    LAST_ERROR.with(|cell| match cell.borrow().as_ref() {
        Some(s) => s.as_ptr(),
        None => c"".as_ptr(),
    })
}

/// All errors that can arise in FFI function implementations.
#[derive(Debug, thiserror::Error)]
pub enum FfiError {
    /// A null pointer was passed to an FFI function that requires a valid pointer.
    #[error("null pointer passed to FFI function: {context}")]
    NullPointer { context: &'static str },

    /// An I/O error propagated from the underlying library.
    #[error("I/O error: {0}")]
    Io(#[from] bethkit_io::IoError),

    /// A core library error propagated from bethkit-core.
    #[error("core error: {0}")]
    Core(#[from] bethkit_core::CoreError),

    /// A BSA/BA2 archive error propagated from bethkit-bsa.
    #[error("archive error: {0}")]
    Bsa(#[from] bethkit_bsa::BsaError),

    /// A string argument contained invalid UTF-8 bytes.
    #[error("UTF-8 error in FFI string: {0}")]
    Utf8(#[from] std::str::Utf8Error),

    /// A string argument could not be converted to a C string (contained a NUL byte).
    #[error("interior NUL byte in string argument at position {0}")]
    Nul(#[from] std::ffi::NulError),

    /// The requested index was out of bounds.
    #[error("index {index} out of bounds (len = {len})")]
    IndexOutOfBounds { index: usize, len: usize },

    /// A writer was used after it had already been consumed.
    #[error("writer already consumed by a previous write call")]
    WriterConsumed,
}

/// Convenience alias for `Result<T, FfiError>`.
pub type Result<T> = std::result::Result<T, FfiError>;
