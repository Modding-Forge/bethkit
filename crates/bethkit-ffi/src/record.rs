// SPDX-License-Identifier: Apache-2.0
//!
//! FFI functions for inspecting records and subrecords.
//!
//! # Ownership
//!
//! [`BethkitRecord`] and [`BethkitSubRecord`] are **borrowed** handles.  They
//! are `#[repr(transparent)]` newtypes that alias the corresponding Rust
//! structs in the parent `BethkitPlugin`.  Never free them directly; their
//! lifetime is bound to the owning plugin.

use std::ffi::{c_char, CString};

use bethkit_core::{Record, SubRecord};

use crate::error::FfiError;
use crate::{ffi_try, null_check, set_last_error, BethkitSlice};

/// A borrowed, read-only handle to a plugin record.
///
/// The pointer is valid for the lifetime of the `BethkitPlugin` it was
/// obtained from.  Never free this handle.
#[repr(transparent)]
pub struct BethkitRecord(pub(crate) Record);

/// A borrowed, read-only handle to a subrecord within a [`BethkitRecord`].
///
/// The pointer is valid for the lifetime of the `BethkitPlugin` it was
/// obtained from.  Never free this handle.
#[repr(transparent)]
pub struct BethkitSubRecord(pub(crate) SubRecord);

/// Writes the 4-byte record signature into `out`.
///
/// `out` must point to at least 4 writable bytes.
///
/// Returns 0 on success or -1 if `record` or `out` is null.
#[no_mangle]
pub extern "C" fn bethkit_record_signature(record: *const BethkitRecord, out: *mut u8) -> i32 {
    null_check!(record, "bethkit_record_signature", -1);
    null_check!(out, "bethkit_record_signature/out", -1);
    // SAFETY: record and out are non-null; out is guaranteed ≥4 bytes by contract.
    let sig = unsafe { &*record }.0.header.signature.0;
    unsafe { std::ptr::copy_nonoverlapping(sig.as_ptr(), out, 4) };
    0
}

/// Returns the raw FormID of `record`.
///
/// Returns 0 and sets the last error if `record` is null.
#[no_mangle]
pub extern "C" fn bethkit_record_form_id(record: *const BethkitRecord) -> u32 {
    null_check!(record, "bethkit_record_form_id", 0);
    // SAFETY: record is non-null.
    unsafe { &*record }.0.header.form_id.0
}

/// Returns the raw record flags of `record`.
///
/// Returns 0 and sets the last error if `record` is null.
#[no_mangle]
pub extern "C" fn bethkit_record_flags(record: *const BethkitRecord) -> u32 {
    null_check!(record, "bethkit_record_flags", 0);
    // SAFETY: record is non-null.
    unsafe { &*record }.0.header.flags.bits()
}

/// Returns the form version stored in the record header.
///
/// Returns 0 and sets the last error if `record` is null.
#[no_mangle]
pub extern "C" fn bethkit_record_form_version(record: *const BethkitRecord) -> u16 {
    null_check!(record, "bethkit_record_form_version", 0);
    // SAFETY: record is non-null.
    unsafe { &*record }.0.header.form_version
}

/// Returns a pointer to the NUL-terminated editor ID (EDID subrecord) of
/// `record`, or null if the record has no EDID.
///
/// The returned string is independently owned and must be freed with
/// [`bethkit_record_editor_id_free`], even after its plugin is freed.
///
/// Returns null on error (null record, I/O error, or encoding error).
///
/// # Errors
///
/// Returns null and sets the last error if `record` is null or the EDID
/// subrecord cannot be decoded.
#[no_mangle]
pub extern "C" fn bethkit_record_editor_id(record: *const BethkitRecord) -> *const c_char {
    let mut result = std::ptr::null_mut();
    bethkit_record_editor_id_status(record, &mut result);
    result
}

/// Reads an editor ID, distinguishing a missing EDID from a decoding error.
///
/// Returns 0 and an owned string, 1 if absent, or -1 on error. Release the
/// result using [`bethkit_record_editor_id_free`]. A valid `out` is initialized
/// to null before validation. Not-found does not change the last error.
///
/// # Errors
///
/// Returns -1 for null arguments, decoding errors, interior NULs, or panics.
///
/// # Safety
///
/// `record` must be a live borrowed record. `out` must point to writable
/// pointer storage that does not alias the record.
#[no_mangle]
pub extern "C" fn bethkit_record_editor_id_status(
    record: *const BethkitRecord,
    out: *mut *mut c_char,
) -> i32 {
    null_check!(out, "bethkit_record_editor_id_status/out", -1);
    // SAFETY: out is caller-owned writable pointer storage.
    unsafe { *out = std::ptr::null_mut() };
    null_check!(record, "bethkit_record_editor_id_status/record", -1);
    let result = ffi_try!(
        (|| {
            // SAFETY: record is a live borrowed handle.
            let record = unsafe { &*record };
            record
                .0
                .editor_id()?
                .map(CString::new)
                .transpose()
                .map_err(FfiError::Nul)
        })(),
        -1
    );
    let Some(value) = result else { return 1 };
    // SAFETY: out is writable and ownership of this CString passes to the caller.
    unsafe { *out = value.into_raw() };
    0
}

/// Frees an editor ID string previously returned by [`bethkit_record_editor_id`].
///
/// Passing a null pointer is a no-op.
///
/// # Safety
///
/// `ptr` must have been returned by [`bethkit_record_editor_id`] or
/// [`bethkit_record_editor_id_status`] and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn bethkit_record_editor_id_free(ptr: *mut c_char) {
    if ptr.is_null() {
        return;
    }
    // SAFETY: ptr was produced by CString::into_raw, so it is a valid CString.
    drop(unsafe { CString::from_raw(ptr) });
}

/// Returns the number of subrecords in `record`, or -1 on error.
///
/// # Errors
///
/// Returns -1 and sets the last error if `record` is null or the subrecords
/// cannot be decoded.
#[no_mangle]
pub extern "C" fn bethkit_record_subrecord_count(record: *const BethkitRecord) -> i64 {
    null_check!(record, "bethkit_record_subrecord_count", -1);
    // SAFETY: record is non-null.
    let rec = unsafe { &*record };
    let subs = ffi_try!(rec.0.subrecords().map_err(FfiError::Core), -1);
    subs.len() as i64
}

/// Returns a borrowed pointer to the subrecord at `index`, or null if
/// `index` is out of bounds or on error.
///
/// The returned pointer is borrowed from the record's owning plugin and must
/// not be freed.
///
/// # Errors
///
/// Returns null and sets the last error if `record` is null, subrecords
/// cannot be decoded, or `index` is out of bounds.
#[no_mangle]
pub extern "C" fn bethkit_record_subrecord_get(
    record: *const BethkitRecord,
    index: usize,
) -> *const BethkitSubRecord {
    null_check!(record, "bethkit_record_subrecord_get", std::ptr::null());
    // SAFETY: record is non-null.
    let rec = unsafe { &*record };
    let subs = ffi_try!(rec.0.subrecords().map_err(FfiError::Core), std::ptr::null());
    match subs.get(index) {
        Some(s) => s as *const SubRecord as *const BethkitSubRecord,
        None => {
            set_last_error(format!(
                "bethkit_record_subrecord_get: index {index} out of bounds (len = {})",
                subs.len()
            ));
            std::ptr::null()
        }
    }
}

/// Returns a borrowed pointer to the first subrecord whose signature matches
/// the 4-byte `sig`, or null if not found.
///
/// `sig` must point to exactly 4 readable bytes.
///
/// # Errors
///
/// Returns null and sets the last error if `record` or `sig` is null, or
/// subrecords cannot be decoded.
#[no_mangle]
pub extern "C" fn bethkit_record_subrecord_find(
    record: *const BethkitRecord,
    sig: *const u8,
) -> *const BethkitSubRecord {
    null_check!(record, "bethkit_record_subrecord_find", std::ptr::null());
    null_check!(sig, "bethkit_record_subrecord_find/sig", std::ptr::null());
    // SAFETY: record and sig are non-null; sig is exactly 4 bytes by contract.
    let rec = unsafe { &*record };
    let sig_bytes: [u8; 4] = unsafe { std::ptr::read(sig as *const [u8; 4]) };
    let target = bethkit_core::Signature(sig_bytes);
    let subs = ffi_try!(rec.0.subrecords().map_err(FfiError::Core), std::ptr::null());
    match subs.iter().find(|s| s.signature == target) {
        Some(s) => s as *const SubRecord as *const BethkitSubRecord,
        None => std::ptr::null(),
    }
}

/// Writes the 4-byte subrecord signature into `out`.
///
/// `out` must point to at least 4 writable bytes.
///
/// Returns 0 on success or -1 if `sr` or `out` is null.
#[no_mangle]
pub extern "C" fn bethkit_subrecord_signature(sr: *const BethkitSubRecord, out: *mut u8) -> i32 {
    null_check!(sr, "bethkit_subrecord_signature", -1);
    null_check!(out, "bethkit_subrecord_signature/out", -1);
    // SAFETY: sr and out are non-null; out is ≥4 bytes by contract.
    let sig = unsafe { &*sr }.0.signature.0;
    unsafe { std::ptr::copy_nonoverlapping(sig.as_ptr(), out, 4) };
    0
}

/// Returns a [`BethkitSlice`] pointing to the raw bytes of `sr`.
///
/// The slice is borrowed from the owning plugin and is valid until the plugin
/// is freed.  Returns a zero-length null slice on error.
///
/// # Errors
///
/// Returns a `{ ptr: null, len: 0 }` slice and sets the last error if `sr`
/// is null.
#[no_mangle]
pub extern "C" fn bethkit_subrecord_bytes(sr: *const BethkitSubRecord) -> BethkitSlice {
    null_check!(
        sr,
        "bethkit_subrecord_bytes",
        BethkitSlice {
            ptr: std::ptr::null(),
            len: 0
        }
    );
    // SAFETY: sr is non-null.
    let bytes = unsafe { &*sr }.0.as_bytes();
    BethkitSlice {
        ptr: bytes.as_ptr(),
        len: bytes.len(),
    }
}

/// Reads the subrecord payload as a single `u8`.
///
/// Writes the decoded value into `*out` and returns 0 on success, or -1 on
/// error.
///
/// # Errors
///
/// Returns -1 and sets the last error if `sr` or `out` is null, or the
/// payload length does not match.
#[no_mangle]
pub extern "C" fn bethkit_subrecord_as_u8(sr: *const BethkitSubRecord, out: *mut u8) -> i32 {
    null_check!(sr, "bethkit_subrecord_as_u8", -1);
    null_check!(out, "bethkit_subrecord_as_u8/out", -1);
    // SAFETY: sr is non-null.
    let val = ffi_try!(unsafe { &*sr }.0.as_u8().map_err(FfiError::Core), -1);
    // SAFETY: out is non-null.
    unsafe { *out = val };
    0
}

/// Reads the subrecord payload as a little-endian `u16`.
///
/// Writes the decoded value into `*out` and returns 0 on success, or -1 on
/// error.
///
/// # Errors
///
/// Returns -1 and sets the last error if `sr` or `out` is null, or the
/// payload length does not match.
#[no_mangle]
pub extern "C" fn bethkit_subrecord_as_u16(sr: *const BethkitSubRecord, out: *mut u16) -> i32 {
    null_check!(sr, "bethkit_subrecord_as_u16", -1);
    null_check!(out, "bethkit_subrecord_as_u16/out", -1);
    // SAFETY: sr is non-null.
    let val = ffi_try!(unsafe { &*sr }.0.as_u16().map_err(FfiError::Core), -1);
    // SAFETY: out is non-null.
    unsafe { *out = val };
    0
}

/// Reads the subrecord payload as a little-endian `u32`.
///
/// Writes the decoded value into `*out` and returns 0 on success, or -1 on
/// error.
///
/// # Errors
///
/// Returns -1 and sets the last error if `sr` or `out` is null, or the
/// payload length does not match.
#[no_mangle]
pub extern "C" fn bethkit_subrecord_as_u32(sr: *const BethkitSubRecord, out: *mut u32) -> i32 {
    null_check!(sr, "bethkit_subrecord_as_u32", -1);
    null_check!(out, "bethkit_subrecord_as_u32/out", -1);
    // SAFETY: sr is non-null.
    let val = ffi_try!(unsafe { &*sr }.0.as_u32().map_err(FfiError::Core), -1);
    // SAFETY: out is non-null.
    unsafe { *out = val };
    0
}

/// Reads the subrecord payload as a little-endian `f32`.
///
/// Writes the decoded value into `*out` and returns 0 on success, or -1 on
/// error.
///
/// # Errors
///
/// Returns -1 and sets the last error if `sr` or `out` is null, or the
/// payload length does not match.
#[no_mangle]
pub extern "C" fn bethkit_subrecord_as_f32(sr: *const BethkitSubRecord, out: *mut f32) -> i32 {
    null_check!(sr, "bethkit_subrecord_as_f32", -1);
    null_check!(out, "bethkit_subrecord_as_f32/out", -1);
    // SAFETY: sr is non-null.
    let val = ffi_try!(unsafe { &*sr }.0.as_f32().map_err(FfiError::Core), -1);
    // SAFETY: out is non-null.
    unsafe { *out = val };
    0
}

/// Returns a pointer to the NUL-terminated string content of `sr`.
///
/// The returned pointer is heap-allocated for this call. Use
/// [`bethkit_record_editor_id_free`] (which accepts any CString produced this
/// way) — or more precisely, call [`bethkit_zstring_free`] — to release it.
///
/// Returns null on error.
///
/// # Errors
///
/// Returns null and sets the last error if `sr` is null or the payload is not
/// valid UTF-8.
#[no_mangle]
pub extern "C" fn bethkit_subrecord_as_zstring(sr: *const BethkitSubRecord) -> *mut c_char {
    null_check!(sr, "bethkit_subrecord_as_zstring", std::ptr::null_mut());
    // SAFETY: sr is non-null.
    let s = ffi_try!(
        unsafe { &*sr }.0.as_zstring().map_err(FfiError::Core),
        std::ptr::null_mut()
    );
    let sanitized: Vec<u8> = s.bytes().map(|b| if b == 0 { b'?' } else { b }).collect();
    match CString::new(sanitized) {
        Ok(cs) => cs.into_raw(),
        Err(e) => {
            set_last_error(e.to_string());
            std::ptr::null_mut()
        }
    }
}

/// Frees a NUL-terminated string produced by [`bethkit_subrecord_as_zstring`]
/// or [`bethkit_record_editor_id`].
///
/// Passing a null pointer is a no-op.
///
/// # Safety
///
/// `ptr` must have been produced by one of the above functions and not yet
/// freed.
#[no_mangle]
pub unsafe extern "C" fn bethkit_zstring_free(ptr: *mut c_char) {
    if ptr.is_null() {
        return;
    }
    // SAFETY: ptr was produced by CString::into_raw.
    drop(unsafe { CString::from_raw(ptr) });
}

#[cfg(test)]
mod tests {
    use std::ffi::CStr;

    use bethkit_core::{FormId, GameContext, Plugin, PluginWriter, RecordFlags, Signature};
    use bethkit_core::{WritableGroup, WritableGroupChild, WritableRecord, WritableSubRecord};

    use super::*;

    /// Creates a parsed record with an optional EDID payload.
    fn fixture(data: Option<&[u8]>) -> Result<Plugin, Box<dyn std::error::Error>> {
        let mut writer = PluginWriter::new(GameContext::sse(), 1.7);
        let subrecords = data
            .into_iter()
            .map(|bytes| WritableSubRecord {
                signature: Signature::EDID,
                data: bytes.to_vec(),
            })
            .collect();
        writer.add_group(WritableGroup {
            label: *b"STAT",
            group_type: 0,
            children: vec![WritableGroupChild::Record(WritableRecord {
                signature: Signature(*b"STAT"),
                flags: RecordFlags::empty(),
                form_id: FormId(0x800),
                form_version: 44,
                subrecords,
            })],
        });
        Ok(Plugin::from_bytes(
            &writer.write_to_vec()?,
            GameContext::sse(),
        )?)
    }

    /// Distinguishes missing and malformed EDIDs without depending on stale errors.
    #[test]
    fn editor_id_status_distinguishes_absent_and_invalid() -> Result<(), Box<dyn std::error::Error>>
    {
        for (data, status) in [
            (None, 1),
            (Some(&[0xff, 0][..]), -1),
            (Some(&b"valid\0"[..]), 0),
        ] {
            let plugin = fixture(data)?;
            let record = plugin
                .find_record(FormId(0x800))
                .expect("fixture record exists");
            let mut result = std::ptr::dangling_mut();
            set_last_error("previous unrelated failure");
            assert_eq!(
                bethkit_record_editor_id_status(
                    record as *const _ as *const BethkitRecord,
                    &mut result
                ),
                status
            );
            drop(plugin);
            if status == 0 {
                // SAFETY: the successful result is owned independently of the dropped plugin.
                assert_eq!(unsafe { CStr::from_ptr(result) }.to_str()?, "valid");
                // SAFETY: result is a live CString allocation returned by the status API.
                unsafe { bethkit_record_editor_id_free(result) };
            } else {
                assert!(result.is_null());
            }
        }
        Ok(())
    }
}
