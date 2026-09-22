// SPDX-License-Identifier: Apache-2.0
//! Owned plugin snapshots for sparse, source-preserving record replacements.

use std::collections::BTreeMap;
use std::ffi::c_char;
use std::path::Path;

use bethkit_core::{FormId, Plugin, PluginPatcher, RecordFlags, RecordPatch, WritableRecord};

use crate::error::FfiError;
use crate::plugin::BethkitPlugin;
use crate::writer::BethkitWritableRecord;
use crate::{cstr_to_str, ffi_try, null_check};

/// An owned patch session whose source bytes outlive the original plugin handle.
///
/// Release with [`bethkit_plugin_patcher_free`]. Replacements are copied and do
/// not consume their writable record handles. Unchanged records remain verbatim.
pub struct BethkitPluginPatcher {
    plugin: Plugin,
    patches: BTreeMap<u32, Vec<u8>>,
}

impl BethkitPluginPatcher {
    /// Serializes the current replacements without consuming the session.
    fn bytes(&self) -> crate::Result<Vec<u8>> {
        let mut patcher = PluginPatcher::new(&self.plugin);
        for (&form_id, bytes) in &self.patches {
            patcher.replace_record(FormId(form_id), RecordPatch::RawBytes(bytes.clone()));
        }
        let mut bytes = Vec::new();
        patcher.write_to(&mut bytes)?;
        Ok(bytes)
    }
}

/// Copies a plugin into an independently owned patch session.
///
/// Borrows `plugin` only during this call. Returns an owned handle to free with
/// [`bethkit_plugin_patcher_free`], or null on failure.
///
/// # Errors
///
/// Returns null and sets the last error for a null plugin, parse errors, or panics.
///
/// # Safety
///
/// `plugin` must be a live borrowed plugin handle.
#[no_mangle]
pub extern "C" fn bethkit_plugin_patcher_new(
    plugin: *const BethkitPlugin,
) -> *mut BethkitPluginPatcher {
    null_check!(plugin, "bethkit_plugin_patcher_new", std::ptr::null_mut());
    ffi_try!(
        (|| {
            // SAFETY: plugin is a live borrowed handle for this call.
            let source = &unsafe { &*plugin }.inner;
            let plugin = Plugin::from_bytes(source.source_bytes(), source.ctx)?;
            Ok::<_, FfiError>(Box::into_raw(Box::new(BethkitPluginPatcher {
                plugin,
                patches: BTreeMap::new(),
            })))
        })(),
        std::ptr::null_mut()
    )
}

/// Frees a patch session. A null pointer is a no-op.
///
/// # Safety
///
/// A nonnull `patcher` must be an owned handle from
/// [`bethkit_plugin_patcher_new`] that has not already been freed.
#[no_mangle]
pub extern "C" fn bethkit_plugin_patcher_free(patcher: *mut BethkitPluginPatcher) {
    if patcher.is_null() {
        return;
    }
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: patcher is a unique owned allocation produced by Box::into_raw.
        drop(unsafe { Box::from_raw(patcher) });
    }))
    .is_err()
    {
        crate::set_last_error("internal panic while freeing plugin patcher");
    }
}

/// Copies a writable record as the replacement for an existing `form_id`.
///
/// Returns 0 on success. Borrows both handles without consuming them. The
/// replacement must retain the original FormID and record signature. Repeated
/// calls for the same ID replace the previous edit. Compressed input flags are
/// cleared because writable payloads are serialized uncompressed.
///
/// # Errors
///
/// Returns -1 and sets the last error for null handles, a missing source record,
/// mismatching identity/signature, unsupported source games, or panics.
///
/// # Safety
///
/// `patcher` must be live and exclusively borrowed. `record` must be a live
/// writable record and must not alias `patcher`.
#[no_mangle]
pub extern "C" fn bethkit_plugin_patcher_replace_record(
    patcher: *mut BethkitPluginPatcher,
    form_id: u32,
    record: *const BethkitWritableRecord,
) -> i32 {
    null_check!(patcher, "bethkit_plugin_patcher_replace_record", -1);
    null_check!(record, "bethkit_plugin_patcher_replace_record/record", -1);
    ffi_try!(
        (|| {
            // SAFETY: handles are valid, distinct borrows with exclusive patcher access.
            let (patcher, record) = unsafe { (&mut *patcher, &*record) };
            let record = &record.0;
            let invalid = |message: &str| FfiError::InvalidArgument {
                context: "plugin record replacement",
                message: message.to_owned(),
            };
            if patcher.plugin.ctx.game == bethkit_core::Game::Morrowind {
                return Err(invalid("TES3 record replacement is not supported"));
            }
            let source = patcher
                .plugin
                .find_record(FormId(form_id))
                .ok_or_else(|| invalid("record does not exist in the source plugin"))?;
            if record.form_id != FormId(form_id) || source.header.signature != record.signature {
                return Err(invalid(
                    "replacement must preserve source FormID and signature",
                ));
            }
            let mut flags = record.flags;
            flags.remove(RecordFlags::COMPRESSED);
            let copied = WritableRecord {
                signature: record.signature,
                flags,
                form_id: record.form_id,
                form_version: record.form_version,
                subrecords: record
                    .subrecords
                    .iter()
                    .map(|sub| bethkit_core::WritableSubRecord {
                        signature: sub.signature,
                        data: sub.data.clone(),
                    })
                    .collect(),
            };
            let RecordPatch::RawBytes(mut bytes) = RecordPatch::from_writable_record(copied);
            // Preserve source revision/unknown header metadata absent from WritableRecord.
            if let Some(original) = source.source_bytes(patcher.plugin.source_bytes()) {
                bytes[16..20].copy_from_slice(&original[16..20]);
                bytes[22..24].copy_from_slice(&original[22..24]);
            }
            patcher.patches.insert(form_id, bytes);
            Ok::<_, FfiError>(0)
        })(),
        -1
    )
}

/// Serializes a patch session into an owned byte buffer without consuming it.
///
/// Returns 0 on success or -1 on failure. Free the successful buffer using
/// [`crate::bethkit_bytes_free`] with the exact returned length. Valid outputs
/// are initialized to null/zero before validation.
///
/// # Errors
///
/// Returns -1 and sets the last error for null pointers, encoding errors, or panics.
///
/// # Safety
///
/// `patcher` must be a live borrowed handle. Outputs must point to distinct
/// writable pointer and length slots without aliasing the handle.
#[no_mangle]
pub extern "C" fn bethkit_plugin_patcher_write_to_bytes(
    patcher: *const BethkitPluginPatcher,
    out_data: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    null_check!(
        out_data,
        "bethkit_plugin_patcher_write_to_bytes/out_data",
        -1
    );
    // SAFETY: out_data is writable pointer storage.
    unsafe { *out_data = std::ptr::null_mut() };
    null_check!(out_len, "bethkit_plugin_patcher_write_to_bytes/out_len", -1);
    // SAFETY: out_len is writable length storage.
    unsafe { *out_len = 0 };
    null_check!(patcher, "bethkit_plugin_patcher_write_to_bytes", -1);
    // SAFETY: patcher is live and borrowed for serialization.
    let bytes = ffi_try!(unsafe { &*patcher }.bytes(), -1).into_boxed_slice();
    let length = bytes.len();
    let pointer = Box::into_raw(bytes).cast::<u8>();
    // SAFETY: both output slots are writable and separate.
    unsafe {
        *out_data = pointer;
        *out_len = length;
    }
    0
}

/// Serializes a patch session to a destination file without consuming it.
///
/// Returns 0 on success. The session owns its input bytes, so overwriting the
/// original path cannot invalidate it. The destination is replaced, not appended.
///
/// # Errors
///
/// Returns -1 and sets the last error for null arguments, invalid UTF-8,
/// serialization or file I/O errors, or panics.
///
/// # Safety
///
/// `patcher` must be a live borrowed handle. `path` must be NUL-terminated UTF-8.
#[no_mangle]
pub extern "C" fn bethkit_plugin_patcher_write_to_file(
    patcher: *const BethkitPluginPatcher,
    path: *const c_char,
) -> i32 {
    null_check!(patcher, "bethkit_plugin_patcher_write_to_file", -1);
    null_check!(path, "bethkit_plugin_patcher_write_to_file/path", -1);
    let Some(path) = cstr_to_str(path, "bethkit_plugin_patcher_write_to_file") else {
        return -1;
    };
    ffi_try!(
        (|| {
            // SAFETY: patcher is a live borrowed handle.
            let bytes = unsafe { &*patcher }.bytes()?;
            std::fs::write(Path::new(path), bytes).map_err(bethkit_io::IoError::from)?;
            Ok::<_, FfiError>(0)
        })(),
        -1
    )
}

#[cfg(test)]
mod tests {
    use bethkit_core::{GameContext, PluginWriter, Signature, WritableGroup, WritableGroupChild};

    use super::*;
    use crate::plugin::{bethkit_plugin_free, bethkit_plugin_open_from_bytes};
    use crate::types::BethkitGame;

    /// Builds two source records so unchanged bytes can be checked independently.
    fn fixture() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let mut writer = PluginWriter::new(GameContext::sse(), 1.7);
        let children = [0x800, 0x801]
            .into_iter()
            .map(|form_id| {
                WritableGroupChild::Record(WritableRecord {
                    signature: Signature(*b"STAT"),
                    flags: RecordFlags::empty(),
                    form_id: FormId(form_id),
                    form_version: 44,
                    subrecords: Vec::new(),
                })
            })
            .collect();
        writer.add_group(WritableGroup {
            label: *b"STAT",
            group_type: 0,
            children,
        });
        Ok(writer.write_to_vec()?)
    }

    /// Confirms snapshots survive source closure and sparse replacements preserve other records.
    #[test]
    fn snapshots_preserve_unmodified_records() -> Result<(), Box<dyn std::error::Error>> {
        // given
        let source = fixture()?;
        let plugin =
            bethkit_plugin_open_from_bytes(source.as_ptr(), source.len(), BethkitGame::SkyrimSe);
        assert!(!plugin.is_null());
        let patcher = bethkit_plugin_patcher_new(plugin);
        assert!(!patcher.is_null());
        bethkit_plugin_free(plugin);
        let mut output = std::ptr::null_mut();
        let mut length = 0;
        assert_eq!(
            bethkit_plugin_patcher_write_to_bytes(patcher, &mut output, &mut length),
            0
        );
        // SAFETY: a successful export owns length readable bytes until freed below.
        assert_eq!(
            unsafe { std::slice::from_raw_parts(output, length) },
            source
        );
        // SAFETY: output is the successful exact-length owned allocation.
        unsafe { crate::bethkit_bytes_free(output, length) };
        let record = BethkitWritableRecord(WritableRecord {
            signature: Signature(*b"STAT"),
            flags: RecordFlags::COMPRESSED,
            form_id: FormId(0x800),
            form_version: 44,
            subrecords: vec![bethkit_core::WritableSubRecord {
                signature: Signature(*b"EDID"),
                data: b"changed\0".to_vec(),
            }],
        });
        // when
        assert_eq!(
            bethkit_plugin_patcher_replace_record(patcher, 0x800, &record),
            0
        );
        assert_eq!(
            bethkit_plugin_patcher_replace_record(patcher, 0x999, &record),
            -1
        );
        assert_eq!(
            bethkit_plugin_patcher_write_to_bytes(patcher, &mut output, &mut length),
            0
        );
        // SAFETY: output holds length bytes until freed below.
        let bytes = unsafe { std::slice::from_raw_parts(output, length) }.to_vec();
        // SAFETY: output is the successful exact-length owned allocation.
        unsafe { crate::bethkit_bytes_free(output, length) };
        bethkit_plugin_patcher_free(patcher);
        // then
        let original = Plugin::from_bytes(&source, GameContext::sse())?;
        let changed = Plugin::from_bytes(&bytes, GameContext::sse())?;
        let changed_record = changed
            .find_record(FormId(0x800))
            .expect("changed record exists");
        assert_eq!(changed_record.editor_id()?, Some("changed"));
        assert!(!changed_record
            .header
            .flags
            .contains(RecordFlags::COMPRESSED));
        assert_eq!(
            original
                .find_record(FormId(0x801))
                .expect("source record exists")
                .source_bytes(&source),
            changed
                .find_record(FormId(0x801))
                .expect("unchanged record exists")
                .source_bytes(&bytes)
        );
        Ok(())
    }

    /// Ensures failed export clears caller outputs without producing an allocation.
    #[test]
    fn null_export_clears_outputs() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = std::ptr::dangling_mut();
        let mut length = usize::MAX;
        assert_eq!(
            bethkit_plugin_patcher_write_to_bytes(std::ptr::null(), &mut data, &mut length),
            -1
        );
        assert!(data.is_null());
        assert_eq!(length, 0);
        bethkit_plugin_patcher_free(std::ptr::null_mut());
        Ok(())
    }
}
