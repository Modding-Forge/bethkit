// SPDX-License-Identifier: Apache-2.0
//!
//! Streaming rewrite of plugins with sparse edits.
//!
//! [`PluginPatcher`] walks an already-parsed [`Plugin`] and serialises it
//! back to a writer. Records and groups whose [`Record::source_range`] /
//! [`Group::source_range`] is set are written verbatim from the original
//! source bytes; only records present in [`PluginPatcher::patches`] are
//! re-serialised.
//!
//! Group sizes are recomputed from the actual emitted children so that
//! patches changing record sizes still produce a valid plugin. Groups that
//! contain no patched descendant are written verbatim with no per-child
//! traversal, keeping the cost linear in the number of edits rather than the
//! plugin size.

use std::io::{self, Write};
use std::ops::Range;

use ahash::{HashMap, HashMapExt, HashSet, HashSetExt};

use crate::error::{CoreError, Result};
use crate::group::{Group, GroupChild, GROUP_HEADER_SIZE};
use crate::plugin::Plugin;
use crate::record::Record;
use crate::types::FormId;

/// How a single record should be rewritten by [`PluginPatcher`].
pub enum RecordPatch {
    /// Provide the complete new record bytes (24-byte header + data block).
    ///
    /// The caller is responsible for ensuring `data_size` in the header is
    /// consistent with the trailing data length and for setting / clearing
    /// the [`crate::RecordFlags::COMPRESSED`] flag as appropriate.
    RawBytes(Vec<u8>),
}

impl RecordPatch {
    /// Returns the byte length of this patch when serialised.
    pub fn byte_len(&self) -> usize {
        match self {
            Self::RawBytes(b) => b.len(),
        }
    }

    /// Writes this patch to `writer`.
    fn write_to(&self, writer: &mut impl Write) -> io::Result<()> {
        match self {
            Self::RawBytes(b) => writer.write_all(b),
        }
    }
}

/// Applies sparse edits to a parsed plugin and writes the result to a stream.
///
/// Construction is cheap; [`Self::write_to`] does the actual work.
pub struct PluginPatcher<'p> {
    plugin: &'p Plugin,
    patches: HashMap<FormId, RecordPatch>,
}

impl<'p> PluginPatcher<'p> {
    /// Creates a new patcher with no edits attached. Calling
    /// [`Self::write_to`] in this state produces a byte-identical copy of
    /// the original plugin (provided every record carries a
    /// [`Record::source_range`]).
    pub fn new(plugin: &'p Plugin) -> Self {
        Self {
            plugin,
            patches: HashMap::new(),
        }
    }

    /// Registers `patch` to replace the record with `form_id`.
    ///
    /// Replaces any previous patch for the same FormID. If no record with
    /// `form_id` exists in the plugin, the patch is silently ignored at
    /// write time.
    pub fn replace_record(&mut self, form_id: FormId, patch: RecordPatch) -> &mut Self {
        self.patches.insert(form_id, patch);
        self
    }

    /// Returns the number of registered patches.
    pub fn patch_count(&self) -> usize {
        self.patches.len()
    }

    /// Writes the patched plugin to `writer`.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::UnexpectedEof`] (with a context naming the
    /// element) when a record or group lacks both a usable
    /// [`Record::source_range`] / [`Group::source_range`] and a patch — this
    /// can only happen when callers manually construct records without
    /// running them through the parser. Bubbles up [`CoreError::Io`] from
    /// the underlying writer via [`std::io::Error`].
    pub fn write_to(&self, writer: &mut impl Write) -> Result<()> {
        // Fast path: no patches and the whole source slice is available \u2014
        // copy verbatim.
        if self.patches.is_empty() {
            writer
                .write_all(self.plugin.source_bytes())
                .map_err(io_err)?;
            return Ok(());
        }

        // Pre-compute which groups touch a patched record. Groups not in this
        // set are emitted via their `source_range` without recursive walk.
        let mut touched_groups: HashSet<Range<usize>> = HashSet::new();
        for group in self.plugin.groups() {
            mark_touched(group, &self.patches, &mut touched_groups);
        }

        // Write the plugin header (TES4) verbatim. Header records cannot be
        // patched in this version.
        let source: &[u8] = self.plugin.source_bytes();
        let header_bytes: &[u8] =
            source
                .get(self.plugin.header_range.clone())
                .ok_or(CoreError::UnexpectedEof {
                    context: "plugin header source range",
                })?;
        writer.write_all(header_bytes).map_err(io_err)?;

        for group in self.plugin.groups() {
            self.write_group(group, source, &touched_groups, writer)?;
        }

        Ok(())
    }

    fn write_group(
        &self,
        group: &Group,
        source: &[u8],
        touched: &HashSet<Range<usize>>,
        writer: &mut impl Write,
    ) -> Result<()> {
        let needs_walk: bool = group
            .source_range
            .as_ref()
            .map(|r| touched.contains(r))
            .unwrap_or(true);

        if !needs_walk {
            // Verbatim copy of the entire group.
            let bytes: &[u8] = group.source_bytes(source).ok_or(CoreError::UnexpectedEof {
                context: "group source range",
            })?;
            writer.write_all(bytes).map_err(io_err)?;
            return Ok(());
        }

        // Serialise children into a temporary buffer so we can compute the
        // new group_size before writing the GRUP header.
        let mut children_buf: Vec<u8> = Vec::new();
        for child in group.children() {
            match child {
                GroupChild::Record(r) => self.write_record(r, source, &mut children_buf)?,
                GroupChild::Group(g) => {
                    self.write_group(g, source, touched, &mut children_buf)?;
                }
            }
        }

        let new_group_size: u32 = (GROUP_HEADER_SIZE + children_buf.len()) as u32;
        write_group_header(group, new_group_size, writer)?;
        writer.write_all(&children_buf).map_err(io_err)?;
        Ok(())
    }

    fn write_record(&self, record: &Record, source: &[u8], writer: &mut impl Write) -> Result<()> {
        if let Some(patch) = self.patches.get(&record.header.form_id) {
            patch.write_to(writer).map_err(io_err)?;
            return Ok(());
        }
        let bytes: &[u8] = record
            .source_bytes(source)
            .ok_or(CoreError::UnexpectedEof {
                context: "record source range",
            })?;
        writer.write_all(bytes).map_err(io_err)?;
        Ok(())
    }
}

/// Recursively flags every group that contains at least one patched record.
fn mark_touched(
    group: &Group,
    patches: &HashMap<FormId, RecordPatch>,
    touched: &mut HashSet<Range<usize>>,
) -> bool {
    let mut any: bool = false;
    for child in group.children() {
        match child {
            GroupChild::Record(r) => {
                if patches.contains_key(&r.header.form_id) {
                    any = true;
                }
            }
            GroupChild::Group(g) => {
                if mark_touched(g, patches, touched) {
                    any = true;
                }
            }
        }
    }
    if any {
        if let Some(range) = &group.source_range {
            touched.insert(range.clone());
        }
    }
    any
}

/// Re-emits a group header with a recalculated `group_size`.
///
/// All other fields (label, group_type, version_control, unknown) are taken
/// from the parsed [`Group`].
fn write_group_header(group: &Group, new_group_size: u32, writer: &mut impl Write) -> Result<()> {
    use crate::group::{GroupLabel, GroupType};

    let label_bytes: [u8; 4] = match group.header.label {
        GroupLabel::Signature(sig) => sig.0,
        GroupLabel::FormId(id) => id.0.to_le_bytes(),
        GroupLabel::GridCell { x, y } => {
            let xb: [u8; 2] = x.to_le_bytes();
            let yb: [u8; 2] = y.to_le_bytes();
            [xb[0], xb[1], yb[0], yb[1]]
        }
        GroupLabel::BlockNumber(n) => n.to_le_bytes(),
    };
    let group_type_raw: i32 = match group.header.group_type {
        GroupType::Normal => 0,
        GroupType::WorldChildren => 1,
        GroupType::InteriorCellBlock => 2,
        GroupType::InteriorCellSubBlock => 3,
        GroupType::ExteriorCellBlock => 4,
        GroupType::ExteriorCellSubBlock => 5,
        GroupType::CellChildren => 6,
        GroupType::TopicChildren => 7,
        GroupType::CellPersistentChildren => 8,
        GroupType::CellTemporaryChildren => 9,
    };

    writer.write_all(b"GRUP").map_err(io_err)?;
    writer
        .write_all(&new_group_size.to_le_bytes())
        .map_err(io_err)?;
    writer.write_all(&label_bytes).map_err(io_err)?;
    writer
        .write_all(&group_type_raw.to_le_bytes())
        .map_err(io_err)?;
    writer
        .write_all(&group.header.version_control.to_le_bytes())
        .map_err(io_err)?;
    writer
        .write_all(&group.header.unknown.to_le_bytes())
        .map_err(io_err)?;
    Ok(())
}

fn io_err(err: io::Error) -> CoreError {
    CoreError::Io(bethkit_io::IoError::Io(err))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::GameContext;

    fn build_hedr(version: f32, num_records: u32, next_id: u32) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&version.to_le_bytes());
        data.extend_from_slice(&num_records.to_le_bytes());
        data.extend_from_slice(&next_id.to_le_bytes());
        data
    }

    fn build_subrecord(sig: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(sig);
        buf.extend_from_slice(&(data.len() as u16).to_le_bytes());
        buf.extend_from_slice(data);
        buf
    }

    fn build_record(sig: &[u8; 4], flags: u32, form_id: u32, data: &[u8]) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(sig);
        buf.extend_from_slice(&(data.len() as u32).to_le_bytes());
        buf.extend_from_slice(&flags.to_le_bytes());
        buf.extend_from_slice(&form_id.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(data);
        buf
    }

    fn build_grup(label: &[u8; 4], group_type: i32, children: &[u8]) -> Vec<u8> {
        let size: u32 = 24 + children.len() as u32;
        let mut buf = Vec::new();
        buf.extend_from_slice(b"GRUP");
        buf.extend_from_slice(&size.to_le_bytes());
        buf.extend_from_slice(label);
        buf.extend_from_slice(&group_type.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(children);
        buf
    }

    fn sample_plugin_bytes() -> Vec<u8> {
        let hedr = build_hedr(1.7, 0, 0x800);
        let tes4_data = build_subrecord(b"HEDR", &hedr);
        let tes4 = build_record(b"TES4", 0, 0, &tes4_data);

        let r1 = build_record(b"NPC_", 0, 0x01, b"\x00\x00\x00\x00");
        let r2 = build_record(b"NPC_", 0, 0x02, b"\xAA\xBB\xCC\xDD");
        let mut children: Vec<u8> = Vec::new();
        children.extend_from_slice(&r1);
        children.extend_from_slice(&r2);
        let grup = build_grup(b"NPC_", 0, &children);

        let mut plugin: Vec<u8> = Vec::new();
        plugin.extend_from_slice(&tes4);
        plugin.extend_from_slice(&grup);
        plugin
    }

    /// Verifies that a patcher with no edits produces byte-identical output.
    #[test]
    fn no_op_patcher_is_byte_identical() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let original: Vec<u8> = sample_plugin_bytes();
        let plugin = Plugin::from_bytes(&original, GameContext::sse())?;

        // when
        let patcher = PluginPatcher::new(&plugin);
        let mut out: Vec<u8> = Vec::new();
        patcher.write_to(&mut out)?;

        // then
        assert_eq!(out, original);
        Ok(())
    }

    /// Verifies that replacing a record's bytes updates the parent group_size.
    #[test]
    fn replacing_record_updates_group_size() -> std::result::Result<(), Box<dyn std::error::Error>>
    {
        // given
        let original: Vec<u8> = sample_plugin_bytes();
        let plugin = Plugin::from_bytes(&original, GameContext::sse())?;
        // Build a replacement record that is 8 bytes larger than the original
        // (data block grows from 4 to 12 bytes).
        let replacement: Vec<u8> = build_record(
            b"NPC_",
            0,
            0x02,
            b"\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0A\x0B\x0C",
        );

        // when
        let mut patcher = PluginPatcher::new(&plugin);
        patcher.replace_record(FormId(0x02), RecordPatch::RawBytes(replacement.clone()));
        let mut out: Vec<u8> = Vec::new();
        patcher.write_to(&mut out)?;

        // then \u2014 re-parse and confirm new size is consistent
        let reparsed = Plugin::from_bytes(&out, GameContext::sse())?;
        assert_eq!(reparsed.groups().len(), 1);
        let records: Vec<&Record> = reparsed.groups()[0].records().collect();
        assert_eq!(records.len(), 2);
        assert_eq!(records[1].header.form_id, FormId(0x02));
        assert_eq!(records[1].header.data_size, 12);
        Ok(())
    }

    /// Verifies that an unrelated FormID patch is silently ignored.
    #[test]
    fn unknown_form_id_patch_is_noop() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let original: Vec<u8> = sample_plugin_bytes();
        let plugin = Plugin::from_bytes(&original, GameContext::sse())?;

        // when
        let mut patcher = PluginPatcher::new(&plugin);
        patcher.replace_record(FormId(0xDEAD_BEEF), RecordPatch::RawBytes(vec![]));
        let mut out: Vec<u8> = Vec::new();
        patcher.write_to(&mut out)?;

        // then \u2014 unrelated patches do not trigger a rewalk: output is
        // identical to the original.
        assert_eq!(out, original);
        Ok(())
    }
}
