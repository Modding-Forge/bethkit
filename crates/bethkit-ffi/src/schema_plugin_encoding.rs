// SPDX-License-Identifier: Apache-2.0
//! Plugin-scoped automatic encoding decisions at the owned context boundary.

use std::ffi::c_char;
use std::path::Path;

use bethkit_semantic::{DecoderRegistry, SemanticContext};
use serde_json::json;

use super::{owned_json, BethkitSchemaPackage, BethkitSemanticContext};
use crate::plugin::BethkitPlugin;
use crate::FfiError;
use crate::{cstr_to_str, ffi_try, null_check};

/// Creates an owned mixed context with conservative raw preservation for uncertain text.
///
/// # Arguments
/// `package` and `plugin` identify the schema and immutable source; `cpoverride`
/// is an optional explicit authoritative codec file and disables guessing.
///
/// # Returns
/// An owned context freed by `bethkit_semantic_context_free`, or null on failure.
///
/// # Errors
/// Returns null with the last error for invalid handles, paths or schema requirements.
///
/// # Safety
/// Both handles and an optional NUL-terminated UTF-8 path must remain live for this call.
#[no_mangle]
pub extern "C" fn bethkit_semantic_context_new_for_plugin_preserving_text(
    package: *const BethkitSchemaPackage,
    plugin: *const BethkitPlugin,
    cpoverride: *const c_char,
) -> *mut BethkitSemanticContext {
    null_check!(package, "preserving text/package", std::ptr::null_mut());
    null_check!(plugin, "preserving text/plugin", std::ptr::null_mut());
    ffi_try!(
        (|| -> crate::Result<*mut BethkitSemanticContext> {
            // SAFETY: Required handles are non-null and live for this call.
            let (package, plugin) = unsafe { (&*package, &*plugin) };
            let path = if cpoverride.is_null() {
                None
            } else {
                Some(Path::new(
                    cstr_to_str(cpoverride, "cpoverride").ok_or_else(|| {
                        FfiError::InvalidArgument {
                            context: "preserving text/cpoverride",
                            message: "override path is not valid UTF-8".to_owned(),
                        }
                    })?,
                ))
            };
            let context = SemanticContext::new_for_plugin_preserving_text(
                package.0.clone(),
                DecoderRegistry::builtin(),
                &plugin.inner,
                path,
            )?;
            Ok(Box::into_raw(Box::new(BethkitSemanticContext(context))))
        })(),
        std::ptr::null_mut()
    )
}

/// Creates an owned context using opt-in mixed UTF-8/legacy inline decoding.
///
/// The retained plugin decision is a prior. Each complete UTF-8 string is decoded
/// as UTF-8; other strings use a detected legacy fallback, or Windows-1252 if the
/// prior was UTF-8. Ambiguous valid byte sequences remain heuristic. A supplied
/// override always disables mixed decoding and wins without detector fallback.
///
/// # Errors
/// Returns null and sets the last error for invalid handles, override paths,
/// schema requirements, or internal panics.
///
/// # Safety
/// Both source handles must remain live for this call. An optional override is a
/// borrowed NUL-terminated UTF-8 path. Free the owned result with
/// `bethkit_semantic_context_free`; the result retains its schema, not its plugin.
#[no_mangle]
pub extern "C" fn bethkit_semantic_context_new_for_plugin_mixed(
    package: *const BethkitSchemaPackage,
    plugin: *const BethkitPlugin,
    cpoverride: *const c_char,
) -> *mut BethkitSemanticContext {
    null_check!(package, "mixed encoding/package", std::ptr::null_mut());
    null_check!(plugin, "mixed encoding/plugin", std::ptr::null_mut());
    ffi_try!(
        (|| -> crate::Result<*mut BethkitSemanticContext> {
            // SAFETY: Required handles are non-null and live for this call.
            let (package, plugin) = unsafe { (&*package, &*plugin) };
            let path = if cpoverride.is_null() {
                None
            } else {
                let text = cstr_to_str(cpoverride, "cpoverride").ok_or_else(|| {
                    FfiError::InvalidArgument {
                        context: "mixed encoding/cpoverride",
                        message: "override path is not valid UTF-8".to_owned(),
                    }
                })?;
                Some(Path::new(text))
            };
            let context = SemanticContext::new_for_plugin_mixed(
                package.0.clone(),
                DecoderRegistry::builtin(),
                &plugin.inner,
                path,
            )?;
            Ok(Box::into_raw(Box::new(BethkitSemanticContext(context))))
        })(),
        std::ptr::null_mut()
    )
}

/// Creates an owned semantic context with a retained plugin encoding decision.
///
/// An optional UTF-8 `cpoverride` path always wins over detection. Invalid overrides
/// fail without falling back. Without an override, a bounded sample of translatable
/// inline text is detected once. Reuse the context for this plugin's reads and edits.
///
/// # Errors
///
/// Returns null and sets the thread-local error for invalid pointers, paths,
/// override files, or schema requirements.
///
/// # Safety
///
/// `package` and `plugin` must be live borrowed handles for this call. `cpoverride`
/// may be null; otherwise it must point to a readable NUL-terminated UTF-8 path.
/// The returned context retains its schema but not the plugin. The caller owns it
/// and must call `bethkit_semantic_context_free` exactly once.
#[no_mangle]
pub extern "C" fn bethkit_semantic_context_new_for_plugin(
    package: *const BethkitSchemaPackage,
    plugin: *const BethkitPlugin,
    cpoverride: *const c_char,
) -> *mut BethkitSemanticContext {
    null_check!(package, "plugin encoding/package", std::ptr::null_mut());
    null_check!(plugin, "plugin encoding/plugin", std::ptr::null_mut());
    ffi_try!(
        (|| -> crate::Result<*mut BethkitSemanticContext> {
            // SAFETY: Required handles were checked for null and remain borrowed.
            let (package, plugin) = unsafe { (&*package, &*plugin) };
            let path = if cpoverride.is_null() {
                None
            } else {
                let text = cstr_to_str(cpoverride, "cpoverride").ok_or_else(|| {
                    FfiError::InvalidArgument {
                        context: "plugin encoding/cpoverride",
                        message: "override path is not valid UTF-8".to_owned(),
                    }
                })?;
                Some(Path::new(text))
            };
            let context = SemanticContext::new_for_plugin(
                package.0.clone(),
                DecoderRegistry::builtin(),
                &plugin.inner,
                path,
            )?;
            Ok(Box::into_raw(Box::new(BethkitSemanticContext(context))))
        })(),
        std::ptr::null_mut()
    )
}

/// Returns an owned JSON encoding decision, or JSON null for a manual context.
///
/// # Errors
///
/// Returns null and sets the thread-local error for a null context or serialization failure.
///
/// # Safety
///
/// `context` must be a live borrowed context. Free the returned UTF-8 allocation
/// with `bethkit_string_free`; it remains valid independently of the context.
#[no_mangle]
pub extern "C" fn bethkit_semantic_context_encoding_json(
    context: *const BethkitSemanticContext,
) -> *mut c_char {
    null_check!(context, "plugin encoding/context", std::ptr::null_mut());
    ffi_try!(
        {
            // SAFETY: context was checked for null and remains borrowed.
            let decision = unsafe { &*context }.0.plugin_encoding_decision();
            let value = decision.map(|decision| {
                json!({
                    "encoding": decision.encoding().schema_name(),
                    "source": decision.source(),
                    "sample_bytes": decision.sample_bytes(),
                    "sample_strings": decision.sample_strings(),
                    "scanned_records": decision.scanned_records(),
                    "skipped_records": decision.skipped_records(),
                })
            });
            owned_json(&json!(value))
        },
        std::ptr::null_mut()
    )
}
