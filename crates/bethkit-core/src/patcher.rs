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
use crate::types::{FormId, RecordFlags};

/// Modifications to apply to the TES4/TES3 plugin header during
/// [`PluginPatcher::write_to`].
///
/// All fields are optional. Fields left as `None` inherit the value from
/// the original header record.
pub struct PluginHeaderPatch {
    /// Replacement master plugin list.  When set, replaces the original MAST
    /// subrecords in their entirety.
    pub masters: Option<Vec<String>>,
    /// Replacement plugin description (written to SNAM).
    pub description: Option<String>,
    /// Flag bits to set on the TES4 record header.
    pub flags_set: Option<RecordFlags>,
    /// Flag bits to clear on the TES4 record header.
    pub flags_clear: Option<RecordFlags>,
}

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
    header_patch: Option<PluginHeaderPatch>,
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
            header_patch: None,
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

    /// Registers a header patch.
    ///
    /// Replaces any previously registered [`PluginHeaderPatch`]. At write
    /// time the TES4/TES3 header is re-serialised with the given overrides;
    /// HEDR `record_count` and `next_object_id` are always recomputed from the
    /// actual group content rather than being caller-settable.
    pub fn patch_header(&mut self, patch: PluginHeaderPatch) -> &mut Self {
        self.header_patch = Some(patch);
        self
    }

    /// Returns the number of registered record patches.
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
        // Fast path: no patches and the whole source slice is available —
        // copy verbatim.
        if self.patches.is_empty() && self.header_patch.is_none() {
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

        let source: &[u8] = self.plugin.source_bytes();

        // Write the TES4/TES3 header. Recompute when records were actually
        // replaced or when an explicit header patch was registered.
        if self.header_patch.is_some() || !touched_groups.is_empty() {
            self.write_header_updated(writer)?;
        } else {
            let header_bytes: &[u8] =
                source
                    .get(self.plugin.header_range.clone())
                    .ok_or(CoreError::UnexpectedEof {
                        context: "plugin header source range",
                    })?;
            writer.write_all(header_bytes).map_err(io_err)?;
        }

        for group in self.plugin.groups() {
            self.write_group(group, source, &touched_groups, writer)?;
        }

        Ok(())
    }

    /// Serialises an updated TES4/TES3 record with recomputed HEDR fields.
    ///
    /// `record_count` is counted from the groups currently in the plugin.
    /// `next_object_id` is the maximum object ID of plugin-owned FormIDs,
    /// clamped to a minimum of `0x800`.
    fn write_header_updated(&self, writer: &mut impl Write) -> Result<()> {
        let header = &self.plugin.header;
        let hp = self.header_patch.as_ref();

        let effective_masters: &[String] = hp
            .and_then(|p| p.masters.as_deref())
            .unwrap_or(&header.masters);

        let effective_description: Option<&str> = hp
            .and_then(|p| p.description.as_deref())
            .or(header.description.as_deref());

        // The file-index for plugin-owned records: one past the last master.
        let own_file_index: u8 = effective_masters.len() as u8;

        // Count all records that live under GRUPs.
        let record_count: u32 = self
            .plugin
            .groups()
            .iter()
            .flat_map(|g| g.records_recursive())
            .count() as u32;

        // Largest object_id among self-owned records, minimum 0x800.
        let next_id_raw: u32 = self
            .plugin
            .groups()
            .iter()
            .flat_map(|g| g.records_recursive())
            .filter(|r| r.header.form_id.file_index() == own_file_index)
            .map(|r| r.header.form_id.object_id())
            .max()
            .unwrap_or(0x800)
            .max(0x800);

        // Build subrecord payload.
        let mut payload: Vec<u8> = Vec::new();

        // HEDR subrecord: 4-sig + 2-size + 12-data
        payload.extend_from_slice(b"HEDR");
        payload.extend_from_slice(&12u16.to_le_bytes());
        payload.extend_from_slice(&header.hedr_version.to_le_bytes());
        payload.extend_from_slice(&record_count.to_le_bytes());
        payload.extend_from_slice(&next_id_raw.to_le_bytes());

        // MAST/DATA pairs (one per effective master).
        for master in effective_masters {
            let mut name_bytes: Vec<u8> = master.as_bytes().to_vec();
            name_bytes.push(0); // NUL terminator
            payload.extend_from_slice(b"MAST");
            payload.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
            payload.extend_from_slice(&name_bytes);
            // DATA is always 8 zero bytes following a MAST.
            payload.extend_from_slice(b"DATA");
            payload.extend_from_slice(&8u16.to_le_bytes());
            payload.extend_from_slice(&0u64.to_le_bytes());
        }

        // Optional SNAM description.
        if let Some(desc) = effective_description {
            let mut desc_bytes: Vec<u8> = desc.as_bytes().to_vec();
            desc_bytes.push(0); // NUL terminator
            payload.extend_from_slice(b"SNAM");
            payload.extend_from_slice(&(desc_bytes.len() as u16).to_le_bytes());
            payload.extend_from_slice(&desc_bytes);
        }

        // Compute effective record flags.
        let mut flags: RecordFlags = header.record.header.flags;
        if let Some(p) = hp {
            if let Some(set) = p.flags_set {
                flags |= set;
            }
            if let Some(clear) = p.flags_clear {
                flags &= !clear;
            }
        }

        // Write the 24-byte record header, then the subrecord payload.
        let sig_bytes: [u8; 4] = header.record.header.signature.0;
        writer.write_all(&sig_bytes).map_err(io_err)?;
        writer
            .write_all(&(payload.len() as u32).to_le_bytes())
            .map_err(io_err)?;
        writer
            .write_all(&flags.bits().to_le_bytes())
            .map_err(io_err)?;
        writer.write_all(&0u32.to_le_bytes()).map_err(io_err)?; // form_id = 0
        writer
            .write_all(&header.record.header.version_control.to_le_bytes())
            .map_err(io_err)?;
        writer
            .write_all(&header.record.header.form_version.to_le_bytes())
            .map_err(io_err)?;
        writer
            .write_all(&header.record.header.unknown.to_le_bytes())
            .map_err(io_err)?;
        writer.write_all(&payload).map_err(io_err)?;

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
    use crate::test_helpers::{build_grup, build_hedr, build_record, build_subrecord};
    use crate::types::GameContext;

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

        // then — unrelated patches do not trigger a rewalk: output is
        // identical to the original.
        assert_eq!(out, original);
        Ok(())
    }

    /// Verifies that replacing a record causes HEDR record_count to be
    /// recomputed and remain accurate.
    #[test]
    fn replacing_record_updates_hedr_record_count(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given — plugin with 2 records under one group
        let original: Vec<u8> = sample_plugin_bytes();
        let plugin = Plugin::from_bytes(&original, GameContext::sse())?;
        let replacement: Vec<u8> = build_record(b"NPC_", 0, 0x01, b"\xFF");

        // when
        let mut patcher = PluginPatcher::new(&plugin);
        patcher.replace_record(FormId(0x01), RecordPatch::RawBytes(replacement));
        let mut out: Vec<u8> = Vec::new();
        patcher.write_to(&mut out)?;

        // then — reparsed header still reports 2 records
        let reparsed = Plugin::from_bytes(&out, GameContext::sse())?;
        assert_eq!(reparsed.header.record_count, 2);
        Ok(())
    }

    /// Verifies that replacing a record causes HEDR next_object_id to reflect
    /// the highest self-owned FormID, minimum 0x800.
    #[test]
    fn replacing_record_updates_next_object_id(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given — records 0x000001 and 0x000002 (file_index 0, own)
        let original: Vec<u8> = sample_plugin_bytes();
        let plugin = Plugin::from_bytes(&original, GameContext::sse())?;
        let replacement: Vec<u8> = build_record(b"NPC_", 0, 0x02, b"\xBB");

        // when
        let mut patcher = PluginPatcher::new(&plugin);
        patcher.replace_record(FormId(0x02), RecordPatch::RawBytes(replacement));
        let mut out: Vec<u8> = Vec::new();
        patcher.write_to(&mut out)?;

        // then — max object_id is 0x02, but minimum is 0x800
        let reparsed = Plugin::from_bytes(&out, GameContext::sse())?;
        assert_eq!(reparsed.header.next_object_id, FormId(0x800));
        Ok(())
    }

    /// Verifies that PluginHeaderPatch replaces the master list.
    #[test]
    fn header_patch_replaces_masters() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given — plugin with no masters
        let hedr_data: Vec<u8> = build_hedr(1.7, 1, 0x800);
        let tes4_data: Vec<u8> = build_subrecord(b"HEDR", &hedr_data);
        let tes4: Vec<u8> = build_record(b"TES4", 0, 0, &tes4_data);
        // Record with file_index 0 = self-owned (no masters)
        let child: Vec<u8> = build_record(b"NPC_", 0, 0x00_0001, &[]);
        let grup: Vec<u8> = build_grup(b"NPC_", 0, &child);
        let mut original: Vec<u8> = Vec::new();
        original.extend_from_slice(&tes4);
        original.extend_from_slice(&grup);
        let plugin = Plugin::from_bytes(&original, GameContext::sse())?;

        // when — inject Skyrim.esm as a master
        let mut patcher = PluginPatcher::new(&plugin);
        patcher.patch_header(PluginHeaderPatch {
            masters: Some(vec!["Skyrim.esm".to_owned()]),
            description: None,
            flags_set: None,
            flags_clear: None,
        });
        let mut out: Vec<u8> = Vec::new();
        patcher.write_to(&mut out)?;

        // then — reparsed plugin has exactly one master
        let reparsed = Plugin::from_bytes(&out, GameContext::sse())?;
        assert_eq!(reparsed.header.masters, vec!["Skyrim.esm".to_owned()]);
        Ok(())
    }

    /// Verifies that PluginHeaderPatch sets the plugin description.
    #[test]
    fn header_patch_sets_description() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given — plugin with no description
        let original: Vec<u8> = sample_plugin_bytes();
        let plugin = Plugin::from_bytes(&original, GameContext::sse())?;

        // when
        let mut patcher = PluginPatcher::new(&plugin);
        patcher.patch_header(PluginHeaderPatch {
            masters: None,
            description: Some("Test plugin".to_owned()),
            flags_set: None,
            flags_clear: None,
        });
        let mut out: Vec<u8> = Vec::new();
        patcher.write_to(&mut out)?;

        // then
        let reparsed = Plugin::from_bytes(&out, GameContext::sse())?;
        assert_eq!(reparsed.header.description.as_deref(), Some("Test plugin"));
        Ok(())
    }
}
