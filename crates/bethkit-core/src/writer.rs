// SPDX-License-Identifier: Apache-2.0
//!
//! Plugin writer — serialises records and groups back to binary.

use std::io::Write;
use std::path::Path;

use ahash::HashMap;

use crate::error::{CoreError, Result};
use crate::types::{FormId, GameContext, RecordFlags, Signature};

/// A subrecord ready to be serialised.
pub struct WritableSubRecord {
    /// 4-byte signature.
    pub signature: Signature,
    /// Raw data payload.
    pub data: Vec<u8>,
}

impl WritableSubRecord {
    /// Serialises this subrecord into `buf`.
    ///
    /// If the data exceeds 65 535 bytes, a `XXXX` size-override subrecord is
    /// prepended automatically.
    fn write_to(&self, buf: &mut Vec<u8>) {
        let data_len: usize = self.data.len();

        if data_len > u16::MAX as usize {
            // XXXX override: 4-byte signature + 2-byte size (=4) + u32 real size
            buf.extend_from_slice(&Signature::XXXX.0);
            buf.extend_from_slice(&4u16.to_le_bytes());
            buf.extend_from_slice(&(data_len as u32).to_le_bytes());
            // Following subrecord header has data_size = 0
            buf.extend_from_slice(&self.signature.0);
            buf.extend_from_slice(&0u16.to_le_bytes());
        } else {
            buf.extend_from_slice(&self.signature.0);
            buf.extend_from_slice(&(data_len as u16).to_le_bytes());
        }

        buf.extend_from_slice(&self.data);
    }
}

/// A main record ready to be serialised.
pub struct WritableRecord {
    /// 4-byte record type signature.
    pub signature: Signature,
    /// Record-level flags (COMPRESSED, DELETED, etc.).
    pub flags: RecordFlags,
    /// Raw FormID.
    pub form_id: FormId,
    /// Form version (Creation Engine format version for this record type).
    pub form_version: u16,
    /// Subrecords in declaration order.
    pub subrecords: Vec<WritableSubRecord>,
}

impl WritableRecord {
    /// Serialises this record into `buf`.
    fn write_to(&self, buf: &mut Vec<u8>) {
        // Serialise subrecords first to know data_size.
        let mut data_buf: Vec<u8> = Vec::new();
        for sr in &self.subrecords {
            sr.write_to(&mut data_buf);
        }

        buf.extend_from_slice(&self.signature.0);
        buf.extend_from_slice(&(data_buf.len() as u32).to_le_bytes());
        buf.extend_from_slice(&self.flags.bits().to_le_bytes());
        buf.extend_from_slice(&self.form_id.0.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes()); // version_control
        buf.extend_from_slice(&self.form_version.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes()); // unknown
        buf.extend_from_slice(&data_buf);
    }
}

/// A GRUP block ready to be serialised.
pub struct WritableGroup {
    /// 4-byte group label (record signature for top-level groups).
    pub label: [u8; 4],
    /// Group type (0 = Normal for top-level record-type groups).
    pub group_type: i32,
    /// Direct children — records or nested groups.
    pub children: Vec<WritableGroupChild>,
}

/// A child of a [`WritableGroup`].
pub enum WritableGroupChild {
    /// A main record.
    Record(WritableRecord),
    /// A nested group.
    Group(WritableGroup),
}

impl WritableGroup {
    /// Serialises this group into `buf`.
    fn write_to(&self, buf: &mut Vec<u8>) {
        // Serialise children first to calculate group_size.
        let mut children_buf: Vec<u8> = Vec::new();
        for child in &self.children {
            match child {
                WritableGroupChild::Record(r) => r.write_to(&mut children_buf),
                WritableGroupChild::Group(g) => g.write_to(&mut children_buf),
            }
        }

        let group_size: u32 = 24 + children_buf.len() as u32;

        buf.extend_from_slice(b"GRUP");
        buf.extend_from_slice(&group_size.to_le_bytes());
        buf.extend_from_slice(&self.label);
        buf.extend_from_slice(&self.group_type.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes()); // version_control
        buf.extend_from_slice(&0u32.to_le_bytes()); // unknown
        buf.extend_from_slice(&children_buf);
    }
}

/// The data written into the `TES4` header record.
pub struct WritableHeader {
    /// Ordered list of master plugin filenames.
    pub masters: Vec<String>,
    /// Optional plugin description.
    pub description: Option<String>,
    /// HEDR version float.
    pub version: f32,
}

/// Builds and serialises a plugin file from scratch.
///
/// Call [`PluginWriter::new`], populate with [`PluginWriter::add_group`], then
/// call [`PluginWriter::write_to_vec`] or [`PluginWriter::write_to_file`].
pub struct PluginWriter {
    ctx: GameContext,
    header: WritableHeader,
    groups: Vec<WritableGroup>,
    /// Whether [`PluginWriter::eslify`] was called; causes the LIGHT flag to
    /// be set in the serialised `TES4` header.
    light_flag_set: bool,
    /// Whether the `LOCALIZED` flag (bit `0x80`) is set on the `TES4` header.
    localized: bool,
}

impl PluginWriter {
    /// Creates a new writer targeting `ctx`.
    ///
    /// # Arguments
    ///
    /// * `ctx`     - Game context (determines header signature, flag bits,
    ///   etc.).
    /// * `version` - HEDR version float for this plugin.
    pub fn new(ctx: GameContext, version: f32) -> Self {
        Self {
            ctx,
            header: WritableHeader {
                masters: Vec::new(),
                description: None,
                version,
            },
            groups: Vec::new(),
            light_flag_set: false,
            localized: false,
        }
    }

    /// Appends a master plugin filename.
    ///
    /// Masters must be added in the correct load-order position.
    pub fn add_master(&mut self, name: &str) {
        self.header.masters.push(name.to_owned());
    }

    /// Sets the plugin description string.
    pub fn set_description(&mut self, desc: &str) {
        self.header.description = Some(desc.to_owned());
    }

    /// Appends a top-level GRUP.
    pub fn add_group(&mut self, group: WritableGroup) {
        self.groups.push(group);
    }

    /// Sets or clears the `LOCALIZED` flag (bit `0x80`) on the `TES4` header.
    ///
    /// When set, the engine will look up text-bearing subrecords as `u32`
    /// LString IDs in sibling `.STRINGS` / `.DLSTRINGS` / `.ILSTRINGS`
    /// files instead of treating them as inline ZStrings.
    pub fn set_localized(&mut self, localized: bool) {
        self.localized = localized;
    }

    /// Reassigns all FormIDs in all groups that belong to this plugin to the
    /// ESL range (`0xFE_000800`–`0xFE_000FFF`) and sets the LIGHT flag on the
    /// header.
    ///
    /// A FormID is considered plugin-owned when its `file_index` byte equals
    /// the master count (i.e. the file points to itself in the load order).
    ///
    /// This performs a **two-pass rewrite**:
    /// 1. All plugin-owned record header FormIDs are remapped.
    /// 2. Every 4-byte-aligned window in every subrecord's data is scanned;
    ///    any value that matches a remapped old FormID is updated to the new
    ///    ESL FormID. This covers common inline FormID references such as
    ///    `CNAM`, `NAME`, and keyword arrays.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::EslRecordLimitExceeded`] if the total count of new
    /// records exceeds 2048, or [`CoreError::UnsupportedGame`] if the target
    /// game does not support light plugins.
    pub fn eslify(&mut self) -> Result<()> {
        if !self.ctx.supports_light() {
            return Err(CoreError::UnsupportedGame(self.ctx.game));
        }

        let master_count: u8 = self.header.masters.len() as u8;

        // Count records that belong to this plugin.
        let new_count: usize = count_new_records(&self.groups, master_count);

        // ESL range: 0x800–0xFFF gives 0x800 (2048) slots.
        const ESL_MAX: usize = 0x800;
        if new_count > ESL_MAX {
            return Err(CoreError::EslRecordLimitExceeded { count: new_count });
        }

        // First pass: collect old FormIDs of all plugin-owned records in
        // iteration order so that sequential ESL IDs are assigned stably.
        let file_index_shifted: u32 = 0xFEu32 << 24;
        let old_fids: Vec<u32> = self
            .groups
            .iter()
            .flat_map(iter_writable_records)
            .filter(|r| r.form_id.file_index() == master_count)
            .map(|r| r.form_id.0)
            .collect();

        let mut remap: HashMap<u32, u32> = HashMap::default();
        for (i, old_fid) in old_fids.iter().enumerate() {
            remap.insert(*old_fid, file_index_shifted | (0x800u32 + i as u32));
        }

        // Second pass: apply the remapping to record headers and all subrecord
        // data so that internal FormID cross-references are also updated.
        apply_fid_remap(&mut self.groups, &remap);

        // Set the LIGHT flag on the header for the next serialisation.
        self.light_flag_set = true;

        Ok(())
    }

    /// Serialises the plugin to a `Vec<u8>`.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError`] if serialisation fails.
    pub fn write_to_vec(&self) -> Result<Vec<u8>> {
        let mut buf: Vec<u8> = Vec::new();
        self.write_tes4(&mut buf)?;
        for group in &self.groups {
            group.write_to(&mut buf);
        }
        Ok(buf)
    }

    /// Serialises the plugin and writes it directly to `path` using a
    /// buffered writer.
    ///
    /// Unlike [`Self::write_to_vec`], the serialised bytes are streamed to
    /// disk without first collecting everything into a `Vec<u8>`. This avoids
    /// holding the entire plugin in memory when writing large ESMs.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::Io`] if the file cannot be created or written.
    pub fn write_to_file(&self, path: &Path) -> Result<()> {
        let file: std::fs::File = std::fs::File::create(path)
            .map_err(|e| CoreError::Io(bethkit_io::IoError::Io(e)))?;
        let mut writer: std::io::BufWriter<std::fs::File> =
            std::io::BufWriter::new(file);
        let mut buf: Vec<u8> = Vec::new();
        self.write_tes4(&mut buf)?;
        writer
            .write_all(&buf)
            .map_err(|e| CoreError::Io(bethkit_io::IoError::Io(e)))?;
        for group in &self.groups {
            buf.clear();
            group.write_to(&mut buf);
            writer
                .write_all(&buf)
                .map_err(|e| CoreError::Io(bethkit_io::IoError::Io(e)))?;
        }
        writer
            .flush()
            .map_err(|e| CoreError::Io(bethkit_io::IoError::Io(e)))
    }

    /// Serialises the `TES4` header record into `buf`.
    fn write_tes4(&self, buf: &mut Vec<u8>) -> Result<()> {
        // Build subrecords for TES4.
        let mut tes4_data: Vec<u8> = Vec::new();

        // HEDR — 12 bytes
        let mut hedr_data: Vec<u8> = Vec::new();
        hedr_data.extend_from_slice(&self.header.version.to_le_bytes());
        // record_count: total records across all groups (linear scan).
        let record_count: u32 = count_all_records(&self.groups) as u32;
        hedr_data.extend_from_slice(&record_count.to_le_bytes());
        // next_object_id: highest object ID in plugin-owned records, plus one.
        let next_id: u32 = next_object_id(&self.groups, self.header.masters.len() as u8);
        hedr_data.extend_from_slice(&next_id.to_le_bytes());

        let hedr_sr = WritableSubRecord {
            signature: Signature::HEDR,
            data: hedr_data,
        };
        hedr_sr.write_to(&mut tes4_data);

        // MAST subrecords (one per master).
        for master in &self.header.masters {
            let mut name: Vec<u8> = master.as_bytes().to_vec();
            name.push(0); // NUL-terminate
            let mast_sr = WritableSubRecord {
                signature: Signature::MAST,
                data: name,
            };
            mast_sr.write_to(&mut tes4_data);
            // xEdit writes a DATA subrecord (8 zero bytes) after each MAST.
            let data_sr = WritableSubRecord {
                signature: Signature::DATA,
                data: vec![0u8; 8],
            };
            data_sr.write_to(&mut tes4_data);
        }

        // Optional SNAM description.
        if let Some(ref desc) = self.header.description {
            let mut d: Vec<u8> = desc.as_bytes().to_vec();
            d.push(0);
            let snam_sr = WritableSubRecord {
                signature: Signature::SNAM,
                data: d,
            };
            snam_sr.write_to(&mut tes4_data);
        }

        // Build TES4 record header flags.
        let mut flags: RecordFlags = RecordFlags::empty();
        if self.light_flag_set {
            flags |= RecordFlags::from_bits_retain(self.ctx.light_flag());
        }
        if self.localized {
            flags |= RecordFlags::LOCALIZED;
        }

        // Write TES4 record.
        buf.extend_from_slice(&self.ctx.header_signature().0);
        buf.extend_from_slice(&(tes4_data.len() as u32).to_le_bytes());
        buf.extend_from_slice(&flags.bits().to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes()); // form_id = 0
        buf.extend_from_slice(&0u32.to_le_bytes()); // version_control
        buf.extend_from_slice(&0u16.to_le_bytes()); // form_version
        buf.extend_from_slice(&0u16.to_le_bytes()); // unknown
        buf.extend_from_slice(&tes4_data);

        Ok(())
    }
}

/// Counts records in all groups whose FormID belongs to this plugin (i.e.
/// file_index == master_count, meaning the plugin itself owns it).
fn count_new_records(groups: &[WritableGroup], master_count: u8) -> usize {
    groups
        .iter()
        .flat_map(iter_writable_records)
        .filter(|r| r.form_id.file_index() == master_count)
        .count()
}

/// Counts all records across all groups (recursive).
fn count_all_records(groups: &[WritableGroup]) -> usize {
    groups.iter().flat_map(iter_writable_records).count()
}

/// Returns the next object ID to use for new records: the highest object ID
/// among plugin-owned records plus one. Returns `0x800` when no plugin-owned
/// records exist yet (i.e. for a fresh plugin).
fn next_object_id(groups: &[WritableGroup], master_count: u8) -> u32 {
    groups
        .iter()
        .flat_map(iter_writable_records)
        .filter(|r| r.form_id.file_index() == master_count)
        .map(|r| r.form_id.object_id())
        .max()
        .map_or(0x800, |id| id + 1)
}

/// Applies a FormID remapping table to all records and subrecord data in
/// every group (recursive).
fn apply_fid_remap(groups: &mut [WritableGroup], remap: &HashMap<u32, u32>) {
    for group in groups.iter_mut() {
        apply_fid_remap_in_group(group, remap);
    }
}

fn apply_fid_remap_in_group(group: &mut WritableGroup, remap: &HashMap<u32, u32>) {
    for child in group.children.iter_mut() {
        match child {
            WritableGroupChild::Record(r) => apply_fid_remap_in_record(r, remap),
            WritableGroupChild::Group(g) => apply_fid_remap_in_group(g, remap),
        }
    }
}

fn apply_fid_remap_in_record(record: &mut WritableRecord, remap: &HashMap<u32, u32>) {
    if let Some(&new_fid) = remap.get(&record.form_id.0) {
        record.form_id = FormId(new_fid);
    }
    for sr in record.subrecords.iter_mut() {
        remap_form_ids_in_bytes(&mut sr.data, remap);
    }
}

/// Scans `data` at every 4-byte-aligned offset and replaces any little-endian
/// `u32` that matches a key in `remap` with the corresponding value.
///
/// Alignment-based scanning is used because FormID fields in Bethesda records
/// are always 4-byte-aligned within their subrecord payload.
fn remap_form_ids_in_bytes(data: &mut [u8], remap: &HashMap<u32, u32>) {
    let len = data.len();
    let mut i = 0usize;
    while i + 4 <= len {
        let raw = u32::from_le_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]);
        if let Some(&new_fid) = remap.get(&raw) {
            data[i..i + 4].copy_from_slice(&new_fid.to_le_bytes());
        }
        i += 4;
    }
}

/// Iterates over all [`WritableRecord`]s in a group tree.
fn iter_writable_records(group: &WritableGroup) -> impl Iterator<Item = &WritableRecord> {
    WritableRecordIter::new(group)
}

struct WritableRecordIter<'a> {
    stack: Vec<&'a [WritableGroupChild]>,
    pos: Vec<usize>,
}

impl<'a> WritableRecordIter<'a> {
    fn new(group: &'a WritableGroup) -> Self {
        Self {
            stack: vec![&group.children],
            pos: vec![0],
        }
    }
}

impl<'a> Iterator for WritableRecordIter<'a> {
    type Item = &'a WritableRecord;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let depth: usize = self.stack.len().checked_sub(1)?;
            let idx: usize = self.pos[depth];
            let children: &[WritableGroupChild] = self.stack[depth];

            if idx >= children.len() {
                self.stack.pop();
                self.pos.pop();
                continue;
            }

            self.pos[depth] += 1;

            match &children[idx] {
                WritableGroupChild::Record(r) => return Some(r),
                WritableGroupChild::Group(g) => {
                    self.stack.push(&g.children);
                    self.pos.push(0);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::Plugin;
    use crate::types::{GameContext, PluginKind};

    fn make_group_with_record(form_id: u32) -> WritableGroup {
        WritableGroup {
            label: *b"NPC_",
            group_type: 0,
            children: vec![WritableGroupChild::Record(WritableRecord {
                signature: Signature(*b"NPC_"),
                flags: RecordFlags::empty(),
                form_id: FormId(form_id),
                form_version: 44,
                subrecords: vec![],
            })],
        }
    }

    /// Verifies that eslify() reassigns owned FormIDs to the ESL range (0x800+)
    /// and sets the LIGHT flag on the serialised plugin header.
    #[test]
    fn eslify_compacts_form_ids_to_esl_range(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given — plugin with no masters and three records at standard offsets
        let ctx = GameContext::sse();
        let mut writer = PluginWriter::new(ctx, 1.7);
        writer.add_group(make_group_with_record(0x00_000001));
        writer.add_group(make_group_with_record(0x00_000002));
        writer.add_group(make_group_with_record(0x00_000003));

        // when
        writer.eslify()?;

        // then — serialise and re-parse
        let bytes: Vec<u8> = writer.write_to_vec()?;
        let plugin = Plugin::from_bytes(&bytes, ctx)?;

        assert_eq!(plugin.kind(), PluginKind::Light, "LIGHT flag must be set");

        for group in plugin.groups() {
            for record in group.records_recursive() {
                let obj_id: u32 = record.header.form_id.0 & 0x00_000FFF;
                assert!(
                    obj_id >= 0x800,
                    "FormID {:#010x} not in ESL range",
                    record.header.form_id.0,
                );
            }
        }
        Ok(())
    }

    /// Verifies that eslify() returns an error when the record count exceeds
    /// the ESL limit (2048 records).
    #[test]
    fn eslify_rejects_oversized_plugin() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given — 2049 records, one over the ESL limit
        let ctx = GameContext::sse();
        let mut writer = PluginWriter::new(ctx, 1.7);
        for i in 1u32..=2049 {
            writer.add_group(make_group_with_record(i));
        }

        // when / then
        let result = writer.eslify();
        assert!(result.is_err(), "expected EslRecordLimitExceeded error");
        Ok(())
    }
}
