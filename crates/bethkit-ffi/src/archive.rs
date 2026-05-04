// SPDX-License-Identifier: Apache-2.0
//!
//! FFI functions for reading BSA / BA2 archives and writing new ones.
//!
//! # Ownership
//!
//! [`BethkitArchive`], [`BethkitBsaWriter`], [`BethkitBa2GnrlWriter`], and
//! [`BethkitBa2Dx10Writer`] are owned, heap-allocated handles that must be
//! freed with their matching `*_free` function.
//!
//! [`BethkitArchiveEntry`] is a **borrowed** handle pointing into the owning
//! `BethkitArchive`; never free it.
//!
//! Byte buffers returned by [`bethkit_archive_extract`] are owned by the
//! caller and must be released with [`bethkit_bytes_free`].

use std::ffi::c_char;
use std::path::Path;

use bethkit_bsa::archive::{Archive, ArchiveEntry};
use bethkit_bsa::write::{Ba2Dx10Writer, Ba2GnrlWriter, BsaWriter};

use crate::error::FfiError;
use crate::types::{
    ba2_version_to_rust, bsa_version_to_rust, BethkitBa2Version, BethkitBsaVersion,
};
use crate::{cstr_to_str, ffi_try, null_check, set_last_error};

/// An opaque handle to an opened BSA or BA2 archive.
///
/// Created by [`bethkit_archive_open`].  Must be freed with
/// [`bethkit_archive_free`].
pub struct BethkitArchive(Box<dyn Archive>);

/// A borrowed, read-only handle to an archive entry.
///
/// Obtained from [`bethkit_archive_entry_get`].  Valid for the lifetime of
/// the owning [`BethkitArchive`].  Never free this handle.
#[repr(transparent)]
pub struct BethkitArchiveEntry(pub(crate) ArchiveEntry);

/// An opaque handle to a BSA archive writer.
///
/// Created by [`bethkit_bsa_writer_new`].  Must be freed with
/// [`bethkit_bsa_writer_free`].  After calling [`bethkit_bsa_writer_write_to`]
/// the writer is consumed; further calls to `write_to` return -1.
pub struct BethkitBsaWriter(Option<BsaWriter>);

/// An opaque handle to a BA2 general-content archive writer.
///
/// Created by [`bethkit_ba2_gnrl_writer_new`].  Must be freed with
/// [`bethkit_ba2_gnrl_writer_free`].
pub struct BethkitBa2GnrlWriter(Option<Ba2GnrlWriter>);

/// An opaque handle to a BA2 DX10 (texture) archive writer.
///
/// Created by [`bethkit_ba2_dx10_writer_new`].  Must be freed with
/// [`bethkit_ba2_dx10_writer_free`].
pub struct BethkitBa2Dx10Writer(Option<Ba2Dx10Writer>);

/// Opens a BSA or BA2 archive at `path`.
///
/// The archive format is detected automatically from the file header.
/// Returns a pointer to the archive handle on success, or null on failure.
///
/// # Arguments
///
/// * `path` — NUL-terminated UTF-8 path to the archive file. Borrows.
///
/// # Errors
///
/// Returns null and sets the last error when `path` is null or the archive
/// cannot be read or is not a recognized format.
#[no_mangle]
pub extern "C" fn bethkit_archive_open(path: *const c_char) -> *mut BethkitArchive {
    null_check!(path, "bethkit_archive_open", std::ptr::null_mut());
    let path_str = match cstr_to_str(path, "bethkit_archive_open") {
        Some(s) => s,
        None => return std::ptr::null_mut(),
    };
    let archive = ffi_try!(
        bethkit_bsa::open(Path::new(path_str)).map_err(FfiError::Bsa),
        std::ptr::null_mut()
    );
    Box::into_raw(Box::new(BethkitArchive(archive)))
}

/// Frees an archive handle previously returned by [`bethkit_archive_open`].
///
/// Passing a null pointer is a no-op.  After this call every borrowed handle
/// derived from the archive is invalid.
#[no_mangle]
pub extern "C" fn bethkit_archive_free(archive: *mut BethkitArchive) {
    if archive.is_null() {
        return;
    }
    // SAFETY: archive was produced by Box::into_raw.
    drop(unsafe { Box::from_raw(archive) });
}

/// Returns a pointer to a NUL-terminated string identifying the archive
/// format (e.g. `"BSA"`, `"BA2-GNRL"`).
///
/// The returned pointer is borrowed from static memory and never needs to be
/// freed.
///
/// # Errors
///
/// Returns a pointer to an empty string and sets the last error if `archive`
/// is null.
#[no_mangle]
pub extern "C" fn bethkit_archive_format_name(archive: *const BethkitArchive) -> *const c_char {
    null_check!(archive, "bethkit_archive_format_name", c"".as_ptr());
    // SAFETY: archive is non-null.
    let name = unsafe { &*archive }.0.format_name();
    // SAFETY: format_name() returns a 'static str which is always NUL-terminated
    // SAFETY: only when stored as a C string literal. We use as_ptr on a leaked
    // SAFETY: CString backed by static data as a workaround.
    // NOTE: format_name is 'static ASCII — safe to return its ptr directly if
    // NOTE: we leak a CString once per unique value. Simpler: use a match.
    static BSA: &[u8] = b"BSA\0";
    static BA2_GNRL: &[u8] = b"BA2-GNRL\0";
    static BA2_DX10: &[u8] = b"BA2-DX10\0";
    static UNKNOWN: &[u8] = b"UNKNOWN\0";
    match name {
        "BSA" => BSA.as_ptr().cast(),
        "BA2-GNRL" => BA2_GNRL.as_ptr().cast(),
        "BA2-DX10" => BA2_DX10.as_ptr().cast(),
        _ => UNKNOWN.as_ptr().cast(),
    }
}

/// Returns the number of files contained in `archive`.
///
/// Returns 0 and sets the last error if `archive` is null.
#[no_mangle]
pub extern "C" fn bethkit_archive_file_count(archive: *const BethkitArchive) -> usize {
    null_check!(archive, "bethkit_archive_file_count", 0);
    // SAFETY: archive is non-null.
    unsafe { &*archive }.0.file_count()
}

/// Returns a borrowed pointer to the archive entry at `index`, or null if
/// `index` is out of bounds.
///
/// The returned pointer is borrowed from `archive` and is valid until the
/// archive is freed.
///
/// # Errors
///
/// Returns null and sets the last error if `archive` is null or `index` is
/// out of bounds.
#[no_mangle]
pub extern "C" fn bethkit_archive_entry_get(
    archive: *const BethkitArchive,
    index: usize,
) -> *const BethkitArchiveEntry {
    null_check!(archive, "bethkit_archive_entry_get", std::ptr::null());
    // SAFETY: archive is non-null.
    let arc = unsafe { &*archive };
    match arc.0.entries().get(index) {
        Some(e) => e as *const ArchiveEntry as *const BethkitArchiveEntry,
        None => {
            set_last_error(format!(
                "bethkit_archive_entry_get: index {index} out of bounds (len = {})",
                arc.0.file_count()
            ));
            std::ptr::null()
        }
    }
}

/// Returns a newly-allocated NUL-terminated copy of the virtual path of
/// `entry` (e.g. `"textures\\actors\\character\\male\\malehead.dds"`).
///
/// The caller takes ownership of the returned string and must free it with
/// [`bethkit_archive_entry_path_free`].
///
/// Returns null and sets the last error if `entry` is null or the path
/// contains a NUL byte (which would make it unrepresentable as a C string).
///
/// # Errors
///
/// Returns null and sets the last error if `entry` is null or path-to-CString
/// conversion fails.
#[no_mangle]
pub extern "C" fn bethkit_archive_entry_path(entry: *const BethkitArchiveEntry) -> *mut c_char {
    null_check!(entry, "bethkit_archive_entry_path", std::ptr::null_mut());
    // SAFETY: entry is non-null and points into the entries Vec of the archive.
    let path = &unsafe { &*entry }.0.path;
    match std::ffi::CString::new(path.as_bytes()) {
        Ok(cs) => cs.into_raw(),
        Err(_) => {
            set_last_error("bethkit_archive_entry_path: path contains an interior NUL byte");
            std::ptr::null_mut()
        }
    }
}

/// Frees a string previously returned by [`bethkit_archive_entry_path`].
///
/// Passing a null pointer is a no-op.
#[no_mangle]
pub extern "C" fn bethkit_archive_entry_path_free(ptr: *mut c_char) {
    if ptr.is_null() {
        return;
    }
    // SAFETY: ptr was produced by CString::into_raw inside bethkit_archive_entry_path.
    drop(unsafe { std::ffi::CString::from_raw(ptr) });
}

/// Returns the uncompressed file size in bytes for `entry`.
///
/// Returns 0 and sets the last error if `entry` is null.
#[no_mangle]
pub extern "C" fn bethkit_archive_entry_uncompressed_size(
    entry: *const BethkitArchiveEntry,
) -> u32 {
    null_check!(entry, "bethkit_archive_entry_uncompressed_size", 0);
    // SAFETY: entry is non-null.
    unsafe { &*entry }.0.uncompressed_size
}

/// Extracts the file at virtual `path` from `archive` into a heap-allocated
/// buffer.
///
/// On success, writes the number of bytes into `*out_len` and returns a
/// pointer to the buffer.  The caller takes ownership of this buffer and must
/// free it with [`bethkit_bytes_free`] passing the same `out_len` value.
///
/// When the virtual path is not found in the archive, returns null and writes
/// `0` into `*out_len` without updating the last error (not-found is not an
/// error; check the return value).
///
/// # Arguments
///
/// * `archive`  — Archive to extract from. Borrows.
/// * `path`     — NUL-terminated virtual path of the file to extract. Borrows.
/// * `out_len`  — Written with the byte count on success, or `0` on failure.
///
/// # Errors
///
/// Returns null, writes `0` into `*out_len`, and sets the last error if
/// `archive`, `path`, or `out_len` is null, `path` contains invalid UTF-8,
/// or extraction (decompression/I/O) fails.
#[no_mangle]
pub extern "C" fn bethkit_archive_extract(
    archive: *const BethkitArchive,
    path: *const c_char,
    out_len: *mut usize,
) -> *mut u8 {
    null_check!(archive, "bethkit_archive_extract", std::ptr::null_mut());
    null_check!(path, "bethkit_archive_extract/path", std::ptr::null_mut());
    null_check!(
        out_len,
        "bethkit_archive_extract/out_len",
        std::ptr::null_mut()
    );

    let path_str = match cstr_to_str(path, "bethkit_archive_extract") {
        Some(s) => s,
        None => return std::ptr::null_mut(),
    };

    // SAFETY: out_len is non-null (checked above); zero it before any early-return
    // so the caller always reads a defined value.
    unsafe { *out_len = 0 };

    // SAFETY: archive is non-null.
    let arc = unsafe { &*archive };
    let result = match arc.0.extract(path_str) {
        // Not found is not an error — return null without touching last_error.
        None => return std::ptr::null_mut(),
        Some(r) => r,
    };

    let cow = ffi_try!(result.map_err(FfiError::Bsa), std::ptr::null_mut());
    let mut vec: Vec<u8> = cow.into_owned();
    let len = vec.len();
    vec.shrink_to_fit();
    let ptr = vec.as_mut_ptr();
    // Transfer ownership to the caller via Box<[u8]>.
    std::mem::forget(vec);
    // SAFETY: out_len is non-null.
    unsafe { *out_len = len };
    ptr
}

/// Extracts the file at virtual `path` from `archive` and writes it to
/// `dest` on the file system.
///
/// Returns 0 on success or -1 on failure.
///
/// # Arguments
///
/// * `archive` — Archive to extract from. Borrows.
/// * `path`    — NUL-terminated virtual path of the file to extract. Borrows.
/// * `dest`    — NUL-terminated file system destination path. Borrows.
///
/// # Errors
///
/// Returns -1 and sets the last error if any pointer is null, the path is
/// not found, extraction fails, or the destination cannot be written.
#[no_mangle]
pub extern "C" fn bethkit_archive_extract_to_file(
    archive: *const BethkitArchive,
    path: *const c_char,
    dest: *const c_char,
) -> i32 {
    null_check!(archive, "bethkit_archive_extract_to_file", -1);
    null_check!(path, "bethkit_archive_extract_to_file/path", -1);
    null_check!(dest, "bethkit_archive_extract_to_file/dest", -1);

    let path_str = match cstr_to_str(path, "bethkit_archive_extract_to_file") {
        Some(s) => s,
        None => return -1,
    };
    let dest_str = match cstr_to_str(dest, "bethkit_archive_extract_to_file/dest") {
        Some(s) => s,
        None => return -1,
    };

    // SAFETY: archive is non-null.
    let arc = unsafe { &*archive };
    let result = match arc.0.extract(path_str) {
        None => {
            set_last_error(format!(
                "bethkit_archive_extract_to_file: path not found: {path_str}"
            ));
            return -1;
        }
        Some(r) => r,
    };

    let cow = ffi_try!(result.map_err(FfiError::Bsa), -1);
    ffi_try!(
        std::fs::write(dest_str, cow.as_ref()).map_err(|e| FfiError::Io(e.into())),
        -1
    );
    0
}

/// Creates a new BSA archive writer for the given `version`.
///
/// Returns a pointer to the writer handle on success, or null on failure.
/// Must be freed with [`bethkit_bsa_writer_free`] after use (even if
/// [`bethkit_bsa_writer_write_to`] has been called).
///
/// # Arguments
///
/// * `version` — The BSA format version to produce.
#[no_mangle]
pub extern "C" fn bethkit_bsa_writer_new(version: BethkitBsaVersion) -> *mut BethkitBsaWriter {
    let writer = BsaWriter::new(bsa_version_to_rust(version));
    Box::into_raw(Box::new(BethkitBsaWriter(Some(writer))))
}

/// Frees a BSA writer handle.  Passing a null pointer is a no-op.
#[no_mangle]
pub extern "C" fn bethkit_bsa_writer_free(w: *mut BethkitBsaWriter) {
    if w.is_null() {
        return;
    }
    // SAFETY: w was produced by Box::into_raw.
    drop(unsafe { Box::from_raw(w) });
}

/// Enables or disables zlib compression for all files added to `w`.
///
/// Returns 0 on success or -1 if `w` is null or already consumed.
#[no_mangle]
pub extern "C" fn bethkit_bsa_writer_set_compress(w: *mut BethkitBsaWriter, compress: bool) -> i32 {
    null_check!(w, "bethkit_bsa_writer_set_compress", -1);
    // SAFETY: w is non-null.
    let bw = unsafe { &mut *w };
    match bw.0.take() {
        None => {
            set_last_error("bethkit_bsa_writer_set_compress: writer already consumed");
            -1
        }
        Some(inner) => {
            bw.0 = Some(inner.compress(compress));
            0
        }
    }
}

/// Enables or disables embedding of file names in the BSA data section.
///
/// Returns 0 on success or -1 if `w` is null or already consumed.
#[no_mangle]
pub extern "C" fn bethkit_bsa_writer_set_embed_names(w: *mut BethkitBsaWriter, embed: bool) -> i32 {
    null_check!(w, "bethkit_bsa_writer_set_embed_names", -1);
    // SAFETY: w is non-null.
    let bw = unsafe { &mut *w };
    match bw.0.take() {
        None => {
            set_last_error("bethkit_bsa_writer_set_embed_names: writer already consumed");
            -1
        }
        Some(inner) => {
            bw.0 = Some(inner.embed_names(embed));
            0
        }
    }
}

/// Adds a file to the BSA writer.
///
/// `path` is the virtual archive path (e.g. `"textures\\mymod\\foo.dds"`).
/// `data` / `len` are the file contents to pack.
///
/// Returns 0 on success or -1 on error.
///
/// # Errors
///
/// Returns -1 and sets the last error if any pointer is null, or the writer
/// has already been consumed.
#[no_mangle]
pub extern "C" fn bethkit_bsa_writer_add(
    w: *mut BethkitBsaWriter,
    path: *const c_char,
    data: *const u8,
    len: usize,
) -> i32 {
    null_check!(w, "bethkit_bsa_writer_add", -1);
    null_check!(path, "bethkit_bsa_writer_add/path", -1);
    null_check!(data, "bethkit_bsa_writer_add/data", -1);

    let path_str = match cstr_to_str(path, "bethkit_bsa_writer_add") {
        Some(s) => s,
        None => return -1,
    };

    // SAFETY: w is non-null.
    let bw = unsafe { &mut *w };
    match bw.0.as_mut() {
        None => {
            set_last_error("bethkit_bsa_writer_add: writer already consumed");
            -1
        }
        Some(inner) => {
            // SAFETY: data is non-null and valid for len bytes by caller contract.
            let bytes = unsafe { std::slice::from_raw_parts(data, len) }.to_vec();
            inner.add(path_str, bytes);
            0
        }
    }
}

/// Writes the BSA archive to `dest` on the file system.
///
/// This call **consumes** the writer; subsequent calls return -1.
///
/// Returns 0 on success or -1 on failure.
///
/// # Arguments
///
/// * `w`    — Writer handle. Takes ownership of the inner writer state.
/// * `dest` — NUL-terminated destination path. Borrows.
///
/// # Errors
///
/// Returns -1 and sets the last error if `w` or `dest` is null, the writer
/// has already been consumed, or writing fails.
#[no_mangle]
pub extern "C" fn bethkit_bsa_writer_write_to(
    w: *mut BethkitBsaWriter,
    dest: *const c_char,
) -> i32 {
    null_check!(w, "bethkit_bsa_writer_write_to", -1);
    null_check!(dest, "bethkit_bsa_writer_write_to/dest", -1);

    let dest_str = match cstr_to_str(dest, "bethkit_bsa_writer_write_to") {
        Some(s) => s,
        None => return -1,
    };

    // SAFETY: w is non-null.
    let bw = unsafe { &mut *w };
    match bw.0.take() {
        None => {
            set_last_error("bethkit_bsa_writer_write_to: writer already consumed");
            -1
        }
        Some(inner) => {
            ffi_try!(
                inner.write_to(Path::new(dest_str)).map_err(FfiError::Bsa),
                -1
            );
            0
        }
    }
}

/// Creates a new BA2 general-content archive writer for the given `version`.
///
/// Returns a pointer to the writer handle on success.
/// Must be freed with [`bethkit_ba2_gnrl_writer_free`].
///
/// # Arguments
///
/// * `version` — The BA2 format version to produce.
#[no_mangle]
pub extern "C" fn bethkit_ba2_gnrl_writer_new(
    version: BethkitBa2Version,
) -> *mut BethkitBa2GnrlWriter {
    let writer = Ba2GnrlWriter::new(ba2_version_to_rust(version));
    Box::into_raw(Box::new(BethkitBa2GnrlWriter(Some(writer))))
}

/// Frees a BA2 general-content writer handle.  Passing a null pointer is a
/// no-op.
#[no_mangle]
pub extern "C" fn bethkit_ba2_gnrl_writer_free(w: *mut BethkitBa2GnrlWriter) {
    if w.is_null() {
        return;
    }
    // SAFETY: w was produced by Box::into_raw.
    drop(unsafe { Box::from_raw(w) });
}

/// Adds a file to a BA2 general-content writer.
///
/// Returns 0 on success or -1 on error.
///
/// # Errors
///
/// Returns -1 and sets the last error if any pointer is null or the writer
/// has already been consumed.
#[no_mangle]
pub extern "C" fn bethkit_ba2_gnrl_writer_add(
    w: *mut BethkitBa2GnrlWriter,
    path: *const c_char,
    data: *const u8,
    len: usize,
) -> i32 {
    null_check!(w, "bethkit_ba2_gnrl_writer_add", -1);
    null_check!(path, "bethkit_ba2_gnrl_writer_add/path", -1);
    null_check!(data, "bethkit_ba2_gnrl_writer_add/data", -1);

    let path_str = match cstr_to_str(path, "bethkit_ba2_gnrl_writer_add") {
        Some(s) => s,
        None => return -1,
    };

    // SAFETY: w is non-null.
    let bw = unsafe { &mut *w };
    match bw.0.as_mut() {
        None => {
            set_last_error("bethkit_ba2_gnrl_writer_add: writer already consumed");
            -1
        }
        Some(inner) => {
            // SAFETY: data is non-null and valid for len bytes by caller contract.
            let bytes = unsafe { std::slice::from_raw_parts(data, len) }.to_vec();
            inner.add(path_str, bytes);
            0
        }
    }
}

/// Writes the BA2 general-content archive to `dest`.  Consumes the writer.
///
/// Returns 0 on success or -1 on failure.
///
/// # Errors
///
/// Returns -1 and sets the last error if `w` or `dest` is null, the writer
/// has already been consumed, or writing fails.
#[no_mangle]
pub extern "C" fn bethkit_ba2_gnrl_writer_write_to(
    w: *mut BethkitBa2GnrlWriter,
    dest: *const c_char,
) -> i32 {
    null_check!(w, "bethkit_ba2_gnrl_writer_write_to", -1);
    null_check!(dest, "bethkit_ba2_gnrl_writer_write_to/dest", -1);

    let dest_str = match cstr_to_str(dest, "bethkit_ba2_gnrl_writer_write_to") {
        Some(s) => s,
        None => return -1,
    };

    // SAFETY: w is non-null.
    let bw = unsafe { &mut *w };
    match bw.0.take() {
        None => {
            set_last_error("bethkit_ba2_gnrl_writer_write_to: writer already consumed");
            -1
        }
        Some(inner) => {
            ffi_try!(
                inner.write_to(Path::new(dest_str)).map_err(FfiError::Bsa),
                -1
            );
            0
        }
    }
}

/// Creates a new BA2 DX10 (texture) archive writer for the given `version`.
///
/// Returns a pointer to the writer handle on success.
/// Must be freed with [`bethkit_ba2_dx10_writer_free`].
///
/// # Arguments
///
/// * `version` — The BA2 format version to produce.
#[no_mangle]
pub extern "C" fn bethkit_ba2_dx10_writer_new(
    version: BethkitBa2Version,
) -> *mut BethkitBa2Dx10Writer {
    let writer = Ba2Dx10Writer::new(ba2_version_to_rust(version));
    Box::into_raw(Box::new(BethkitBa2Dx10Writer(Some(writer))))
}

/// Frees a BA2 DX10 writer handle.  Passing a null pointer is a no-op.
#[no_mangle]
pub extern "C" fn bethkit_ba2_dx10_writer_free(w: *mut BethkitBa2Dx10Writer) {
    if w.is_null() {
        return;
    }
    // SAFETY: w was produced by Box::into_raw.
    drop(unsafe { Box::from_raw(w) });
}

/// Adds a file to a BA2 DX10 writer.
///
/// `path` is the virtual archive path (e.g. `"textures\\mymod\\foo.dds"`).
/// `data` / `len` are the raw DDS file bytes to pack.
///
/// Returns 0 on success or -1 on error.
///
/// # Errors
///
/// Returns -1 and sets the last error if any pointer is null, the writer has
/// already been consumed, or `data` does not contain a valid DX10/DDS image.
#[no_mangle]
pub extern "C" fn bethkit_ba2_dx10_writer_add(
    w: *mut BethkitBa2Dx10Writer,
    path: *const c_char,
    data: *const u8,
    len: usize,
) -> i32 {
    null_check!(w, "bethkit_ba2_dx10_writer_add", -1);
    null_check!(path, "bethkit_ba2_dx10_writer_add/path", -1);
    null_check!(data, "bethkit_ba2_dx10_writer_add/data", -1);

    let path_str = match cstr_to_str(path, "bethkit_ba2_dx10_writer_add") {
        Some(s) => s,
        None => return -1,
    };

    // SAFETY: w is non-null.
    let bw = unsafe { &mut *w };
    match bw.0.as_mut() {
        None => {
            set_last_error("bethkit_ba2_dx10_writer_add: writer already consumed");
            -1
        }
        Some(inner) => {
            // SAFETY: data is non-null and valid for len bytes by caller contract.
            let bytes = unsafe { std::slice::from_raw_parts(data, len) }.to_vec();
            ffi_try!(inner.add(path_str, bytes).map_err(FfiError::Bsa), -1);
            0
        }
    }
}

/// Writes the BA2 DX10 archive to `dest`.  Consumes the writer.
///
/// Returns 0 on success or -1 on failure.
///
/// # Errors
///
/// Returns -1 and sets the last error if `w` or `dest` is null, the writer
/// has already been consumed, or writing fails.
#[no_mangle]
pub extern "C" fn bethkit_ba2_dx10_writer_write_to(
    w: *mut BethkitBa2Dx10Writer,
    dest: *const c_char,
) -> i32 {
    null_check!(w, "bethkit_ba2_dx10_writer_write_to", -1);
    null_check!(dest, "bethkit_ba2_dx10_writer_write_to/dest", -1);

    let dest_str = match cstr_to_str(dest, "bethkit_ba2_dx10_writer_write_to") {
        Some(s) => s,
        None => return -1,
    };

    // SAFETY: w is non-null.
    let bw = unsafe { &mut *w };
    match bw.0.take() {
        None => {
            set_last_error("bethkit_ba2_dx10_writer_write_to: writer already consumed");
            -1
        }
        Some(inner) => {
            ffi_try!(
                inner.write_to(Path::new(dest_str)).map_err(FfiError::Bsa),
                -1
            );
            0
        }
    }
}
