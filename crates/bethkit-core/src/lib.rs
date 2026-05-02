// SPDX-License-Identifier: Apache-2.0
//!
//! `bethkit-core` — parser and writer for Bethesda plugin files.
//!
//! Supports ESP, ESL, and ESM formats for Skyrim SE (Phase 1) and is
//! extensible to other Creation Engine games via [`types::GameContext`].
//!
//! # Quick start
//!
//! ```rust,no_run
//! use bethkit_core::{GameContext, Plugin};
//!
//! let ctx = GameContext::sse();
//! let plugin = Plugin::open("MyMod.esp".as_ref(), ctx).unwrap();
//! println!("kind: {}", plugin.kind());
//! for group in plugin.groups() {
//!     for record in group.records_recursive() {
//!         println!("{}", record.header.signature);
//!     }
//! }
//! ```

mod cache;
mod encoding;
mod error;
mod group;
mod implicits;
mod load_order;
mod localized;
mod patcher;
mod plugin;
mod record;
mod schema;
mod strings;
#[cfg(test)]
mod test_helpers;
mod types;
mod writer;

pub use cache::{CacheEntry, PluginCache};
pub use encoding::{code_page_for_language, CodePage};
pub use error::{CoreError, Result};
pub use group::{Group, GroupChild, GroupHeader, GroupLabel, GroupType};
pub use implicits::ImplicitRecords;
pub use load_order::{GlobalFormId, LoadOrder, LoadOrderEntry};
pub use localized::{
    apply_edits, extract_strings, localized_subrecords, resolve_string_kind, LocalizationSet,
    LocalizedString,
};
pub use patcher::{PluginHeaderPatch, PluginPatcher, RecordPatch};
pub use plugin::{Plugin, PluginHeader};
pub use record::{Record, RecordHeader, SubRecord, SubRecordData};
pub use schema::{FieldEntry, FieldValue, RecordView, SchemaRegistry};
pub use strings::{StringFileKind, StringTable};
pub use types::{FormId, Game, GameContext, PluginKind, RecordFlags, Signature};
pub use writer::{
    PluginWriter, WritableGroup, WritableGroupChild, WritableHeader, WritableRecord,
    WritableSubRecord,
};
