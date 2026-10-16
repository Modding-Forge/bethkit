// SPDX-License-Identifier: Apache-2.0
//! Bounded record-batched string projections borrowing their source owners.

use std::collections::BTreeSet;
use std::ffi::c_char;

use bethkit_core::{GroupChild, Plugin, Record, Signature};
use serde_json::json;

use super::{owned_json, snapshot, BethkitSemanticContext};
use crate::plugin::BethkitPlugin;
use crate::{ffi_try, null_check, FfiError};

/// Owned cursor over a borrowed immutable plugin and semantic context.
///
/// Both source handles must outlive this cursor. No record values are retained
/// between batches. Free with `bethkit_string_cursor_free` before either owner.
pub struct BethkitStringCursor {
    plugin: *const BethkitPlugin,
    context: *const BethkitSemanticContext,
    root: usize,
    positions: Vec<usize>,
    failed: bool,
    pending_error: Option<FfiError>,
    text_free: BTreeSet<Signature>,
    validate_nontext: bool,
    preserve_opaque_nontext: bool,
}

impl BethkitStringCursor {
    fn next_record<'a>(&mut self, plugin: &'a Plugin) -> Option<&'a Record> {
        while let Some(mut group) = plugin.groups().get(self.root) {
            if self.positions.is_empty() {
                self.positions.push(0);
            }
            for position in &self.positions[..self.positions.len() - 1] {
                let GroupChild::Group(child) = &group.children()[position - 1] else {
                    unreachable!("only groups extend the cursor traversal stack");
                };
                group = child;
            }
            let position = self
                .positions
                .last_mut()
                .expect("cursor stack is initialized");
            let child = group.children().get(*position);
            *position += 1;
            match child {
                Some(GroupChild::Record(record)) => return Some(record),
                Some(GroupChild::Group(_)) => self.positions.push(0),
                None => {
                    self.positions.pop();
                    if self.positions.is_empty() {
                        self.root += 1;
                    }
                }
            }
        }
        None
    }
}

/// Creates an owned cursor, without decoding or copying the plugin.
///
/// # Errors
/// Returns null and sets the last error for null handles or an internal panic.
///
/// # Safety
/// Borrow `context` and `plugin` until the cursor is freed. They must remain live
/// and immutable, and `context` must match this plugin's game and codec policy.
/// The caller owns the result and frees it with `bethkit_string_cursor_free`.
/// `validate_nontext` retains full decoding of text-free records when true.
/// False skips only signatures proven text-free by the schema. Unknown schemas,
/// custom decoders and unresolved references are always decoded conservatively.
#[no_mangle]
pub extern "C" fn bethkit_string_cursor_new(
    context: *const BethkitSemanticContext,
    plugin: *const BethkitPlugin,
    validate_nontext: bool,
) -> *mut BethkitStringCursor {
    null_check!(context, "string cursor/context", std::ptr::null_mut());
    null_check!(plugin, "string cursor/plugin", std::ptr::null_mut());
    ffi_try!(
        {
            // SAFETY: The caller borrows a live context for cursor creation.
            let runtime = unsafe { &*context };
            let text_free = runtime
                .0
                .registry()
                .package()
                .records()
                .iter()
                .filter(|record| {
                    !snapshot::can_contain_strings(
                        runtime.0.registry(),
                        record.signature.into(),
                        &record.root,
                        0,
                    )
                })
                .map(|record| record.signature.into())
                .collect();
            Ok::<_, FfiError>(Box::into_raw(Box::new(BethkitStringCursor {
                plugin,
                context,
                root: 0,
                positions: Vec::new(),
                failed: false,
                pending_error: None,
                text_free,
                validate_nontext,
                preserve_opaque_nontext: false,
            })))
        },
        std::ptr::null_mut()
    )
}

/// Creates a full-validation cursor retaining undecodable nontext records as opaque source.
///
/// Every record is decoded first. Only schema-proven nontext failures become explicit
/// `diagnostics` entries instead of aborting extraction. Source bytes remain untouched;
/// this does not repair the record or permit semantic edits to its unknown content.
///
/// # Arguments
/// * `context` - Matching immutable semantic context borrowed until cursor release.
/// * `plugin` - Immutable source plugin borrowed until cursor release.
///
/// # Returns
/// A uniquely owned cursor, or null on failure.
///
/// # Errors
/// Returns null with the last error for invalid handles or allocation failure.
///
/// # Safety
/// Both immutable owners must outlive the cursor. Free with `bethkit_string_cursor_free`.
#[no_mangle]
pub extern "C" fn bethkit_string_cursor_new_preserving_nontext(
    context: *const BethkitSemanticContext,
    plugin: *const BethkitPlugin,
) -> *mut BethkitStringCursor {
    null_check!(context, "preserving cursor/context", std::ptr::null_mut());
    null_check!(plugin, "preserving cursor/plugin", std::ptr::null_mut());
    ffi_try!(
        {
            let cursor = bethkit_string_cursor_new(context, plugin, true);
            if !cursor.is_null() {
                // SAFETY: The constructor just returned this exclusively owned live cursor.
                unsafe { (*cursor).preserve_opaque_nontext = true };
            }
            Ok::<_, FfiError>(cursor)
        },
        std::ptr::null_mut()
    )
}

/// Copies the next bounded batch of translatable scalars into owned UTF-8 JSON.
///
/// `record_limit` must be 1..=256. Batches stop after 4096 strings or 2 MiB
/// of uncompressed source payload, but always finish their current record. One
/// unusually large record can exceed these soft byte/string limits. The JSON
/// contains `values` and `done`. Empty nonterminal batches are valid. Every value
/// has the same address, codec metadata and structure hash as a record snapshot.
/// Free each result with `bethkit_string_free`. After an error, recreate the
/// cursor: it cannot resume past a malformed record and silently omit its strings.
///
/// # Errors
/// Returns null and sets the last error for invalid limits, a failed cursor,
/// record decoding or serialization failure, or an internal panic.
///
/// # Safety
/// The cursor and both borrowed owners must remain live for the call. Do not
/// advance this cursor concurrently or mutate its plugin/context. The returned
/// JSON owns its contents independently of all native handles.
#[no_mangle]
pub extern "C" fn bethkit_string_cursor_next_json(
    cursor: *mut BethkitStringCursor,
    record_limit: usize,
) -> *mut c_char {
    null_check!(cursor, "string cursor/next", std::ptr::null_mut());
    ffi_try!(
        (|| -> crate::Result<*mut c_char> {
            // SAFETY: The caller exclusively borrows a live cursor for this call.
            let cursor = unsafe { &mut *cursor };
            if let Some(error) = cursor.pending_error.take() {
                return Err(error);
            }
            if !(1..=256).contains(&record_limit) || cursor.failed {
                return Err(FfiError::InvalidArgument {
                    context: "string cursor/next",
                    message: "invalid record limit or cursor previously failed".to_owned(),
                });
            }
            // Keep failure sticky even if decoding or allocation unwinds.
            cursor.failed = true;
            // SAFETY: Creation requires both source handles to outlive the cursor.
            let (plugin, context) = unsafe { (&*cursor.plugin, &*cursor.context) };
            let mut values = Vec::new();
            let mut diagnostics = Vec::new();
            let mut bytes = 0;
            let mut done = false;
            #[cfg(feature = "profile-string-stages")]
            let mut profile = super::string_profile::StageProfile::default();
            for _ in 0..record_limit {
                let Some(record) = cursor.next_record(&plugin.inner) else {
                    done = true;
                    break;
                };
                let text_free = cursor.text_free.contains(&record.header.signature);
                if text_free && !cursor.validate_nontext {
                    continue;
                }
                let rows = match snapshot::string_rows(
                    &context.0,
                    record,
                    plugin.inner.is_localized(),
                    text_free,
                    #[cfg(feature = "profile-string-stages")]
                    &mut profile,
                ) {
                    Ok(rows) => rows,
                    Err(error) if text_free && cursor.preserve_opaque_nontext => {
                        diagnostics.push(json!({
                            "record_signature": record.header.signature.to_string(),
                            "form_id": record.header.form_id.0,
                            "source_size": record.header.data_size,
                            "message": error.to_string(),
                        }));
                        continue;
                    }
                    Err(error) if !values.is_empty() => {
                        // Yield the same valid prefix as the record-at-a-time iterator,
                        // then report the original error on the very next call.
                        cursor.pending_error = Some(error);
                        break;
                    }
                    Err(error) => return Err(error),
                };
                bytes += record.uncompressed_data_size();
                values.extend(rows);
                if bytes >= 2 * 1024 * 1024 || values.len() >= 4096 {
                    break;
                }
            }
            let document = json!({"values":values,"done":done,"diagnostics":diagnostics});
            #[cfg(feature = "profile-string-stages")]
            let document = {
                let mut document = document;
                document["profile"] = profile.document();
                document
            };
            let result = owned_json(&document)?;
            cursor.failed = cursor.pending_error.is_some();
            Ok(result)
        })(),
        std::ptr::null_mut()
    )
}

/// Frees an owned cursor without touching its borrowed source handles.
///
/// # Safety
/// Null is a no-op. Otherwise the cursor must be live, exclusively owned, and
/// not previously freed. No concurrent advance may be running.
#[no_mangle]
pub extern "C" fn bethkit_string_cursor_free(cursor: *mut BethkitStringCursor) {
    if cursor.is_null() {
        return;
    }
    ffi_try!({
        // SAFETY: The caller transfers exclusive ownership of a live allocation.
        drop(unsafe { Box::from_raw(cursor) });
        Ok::<(), FfiError>(())
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rejects null and invalid-limit calls before dereferencing borrowed owners.
    #[test]
    fn invalid_inputs_fail_without_accessing_owners(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        assert!(bethkit_string_cursor_new(std::ptr::null(), std::ptr::null(), true).is_null());
        assert!(
            bethkit_string_cursor_new_preserving_nontext(std::ptr::null(), std::ptr::null())
                .is_null()
        );
        assert!(bethkit_string_cursor_next_json(std::ptr::null_mut(), 1).is_null());
        bethkit_string_cursor_free(std::ptr::null_mut());
        let cursor = Box::into_raw(Box::new(BethkitStringCursor {
            plugin: std::ptr::null(),
            context: std::ptr::null(),
            root: 0,
            positions: Vec::new(),
            failed: false,
            pending_error: None,
            text_free: BTreeSet::new(),
            validate_nontext: true,
            preserve_opaque_nontext: false,
        }));
        for limit in [0, 257, usize::MAX] {
            assert!(bethkit_string_cursor_next_json(cursor, limit).is_null());
        }
        bethkit_string_cursor_free(cursor);
        Ok(())
    }
}
