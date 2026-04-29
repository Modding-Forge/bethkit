// SPDX-License-Identifier: Apache-2.0
//!
//! Top-level plugin file: opening, header parsing, and group enumeration.

use std::{path::Path, sync::Arc};

use bethkit_io::MappedFile;

use crate::error::{CoreError, Result};
use crate::group::Group;
use crate::record::Record;
use crate::types::{FormId, GameContext, PluginKind, Signature};

/// Parsed contents of the `TES4` (or `TES3`) header record.
pub struct PluginHeader {
    /// The raw `TES4`/`TES3` record (carries flags and FormID).
    pub record: Record,
    /// HEDR version float.
    pub hedr_version: f32,
    /// Number of records declared in the HEDR subrecord.
    pub record_count: u32,
    /// Next available object ID as stored in HEDR.
    pub next_object_id: FormId,
    /// Ordered list of master plugin filenames from MAST subrecords.
    pub masters: Vec<String>,
    /// Optional plugin description from SNAM.
    pub description: Option<String>,
}

impl PluginHeader {
    /// Parses the plugin header record from `cursor`.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidSignature`] if the first record is not
    /// `TES4` (or `TES3` for Morrowind), or other [`CoreError`] variants
    /// on malformed data.
    fn parse(cursor: &mut bethkit_io::SliceCursor<'_>, ctx: &GameContext) -> Result<Self> {
        let record: Record = Record::parse_header(cursor, ctx)?;
        let expected_sig: Signature = ctx.header_signature();

        if record.header.signature != expected_sig {
            return Err(CoreError::InvalidSignature {
                expected: expected_sig.to_string(),
                got: record.header.signature.to_string(),
            });
        }

        // Parse HEDR subrecord — 12 bytes: f32 version, u32 numRecords,
        //                                   u32 nextObjectId
        let (hedr_version, record_count, next_object_id): (f32, u32, FormId) =
            if let Some(hedr) = record.get(Signature::HEDR)? {
                let data: &[u8] = hedr.as_bytes();
                if data.len() >= 12 {
                    let version: f32 = f32::from_le_bytes([data[0], data[1], data[2], data[3]]);
                    let num_records: u32 = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
                    let next_id: u32 = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);
                    (version, num_records, FormId(next_id))
                } else {
                    (ctx.hedr_version(), 0, FormId::NULL)
                }
            } else {
                (ctx.hedr_version(), 0, FormId::NULL)
            };

        // Collect all MAST subrecords (one per master plugin).
        let masters: Vec<String> = record
            .get_all(Signature::MAST)?
            .into_iter()
            .filter_map(|sr| sr.as_zstring().ok().map(str::to_owned))
            .collect();

        // Optional plugin description.
        let description: Option<String> = record
            .get(Signature::SNAM)?
            .and_then(|sr| sr.as_zstring().ok())
            .filter(|s| !s.is_empty())
            .map(str::to_owned);

        Ok(Self {
            record,
            hedr_version,
            record_count,
            next_object_id,
            masters,
            description,
        })
    }
}

/// A fully parsed Bethesda plugin file (ESP / ESL / ESM).
pub struct Plugin {
    /// Keeps the memory-mapped file alive for the lifetime of all borrowed
    /// slices, preventing the OS mapping from being released while records
    /// still reference bytes into it. `None` when the plugin was constructed
    /// from an in-memory byte slice via [`Plugin::from_bytes`].
    _source: Option<Arc<MappedFile>>,
    /// Full plugin file bytes. Used by [`crate::PluginPatcher`] to copy
    /// unmodified records and groups verbatim, and by
    /// [`Plugin::source_bytes`] for diagnostics.
    source: Arc<[u8]>,
    /// Game context used to parse this plugin.
    pub ctx: GameContext,
    /// Parsed plugin header.
    pub header: PluginHeader,
    /// Header source range (bytes spanning the TES4/TES3 record).
    pub header_range: std::ops::Range<usize>,
    /// All top-level GRUP records.
    pub groups: Vec<Group>,
}

impl Plugin {
    /// Opens and fully parses a plugin file at `path`.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the `.esp` / `.esm` / `.esl` file.
    /// * `ctx`  - Game context driving all format differences.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::Io`] if the file cannot be opened or mapped, or
    /// other [`CoreError`] variants on malformed plugin data.
    pub fn open(path: &Path, ctx: GameContext) -> Result<Self> {
        let mapped: MappedFile = MappedFile::open(path)?;
        let source: Arc<MappedFile> = Arc::new(mapped);
        Self::from_mapped(source, ctx)
    }

    /// Parses a plugin from a raw byte slice.
    ///
    /// Useful for testing or when the data is already in memory. A heap copy
    /// is made and wrapped in an `Arc<MappedFile>`-equivalent.
    ///
    /// # Arguments
    ///
    /// * `data` - Full plugin file bytes.
    /// * `ctx`  - Game context.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError`] on malformed plugin data.
    pub fn from_bytes(data: &[u8], ctx: GameContext) -> Result<Self> {
        // Write bytes to a temporary in-memory file equivalent by wrapping
        // them in a cursor directly — no MappedFile needed for in-memory use.
        // We create a dummy MappedFile wrapper by writing to a temp path.
        // For testing purposes we parse directly from a cursor here.
        let owned: Arc<[u8]> = Arc::from(data);
        Self::from_arc_bytes(owned, ctx)
    }

    /// Internal: parse from a memory-mapped file.
    fn from_mapped(source: Arc<MappedFile>, ctx: GameContext) -> Result<Self> {
        let bytes: Arc<[u8]> = Arc::from(source.as_bytes());
        let (header, header_range, groups) = Self::parse_bytes(&bytes, &ctx)?;
        Ok(Self {
            _source: Some(source),
            source: bytes,
            ctx,
            header,
            header_range,
            groups,
        })
    }

    /// Internal: parse from an `Arc<[u8]>` (used by `from_bytes`).
    ///
    /// No `MappedFile` is needed because the data is already owned — records
    /// hold `Arc<[u8]>` slices into `data`, keeping it alive independently.
    fn from_arc_bytes(data: Arc<[u8]>, ctx: GameContext) -> Result<Self> {
        let (header, header_range, groups) = Self::parse_bytes(&data, &ctx)?;
        Ok(Self {
            _source: None,
            source: data,
            ctx,
            header,
            header_range,
            groups,
        })
    }

    /// Core parsing logic shared by `from_mapped` and `from_arc_bytes`.
    fn parse_bytes(
        data: &[u8],
        ctx: &GameContext,
    ) -> Result<(PluginHeader, std::ops::Range<usize>, Vec<Group>)> {
        let mut cursor: bethkit_io::SliceCursor<'_> = bethkit_io::SliceCursor::new(data);
        let header_start: usize = cursor.pos();
        let header: PluginHeader = PluginHeader::parse(&mut cursor, ctx)?;
        let header_range: std::ops::Range<usize> = header_start..cursor.pos();

        let mut groups: Vec<Group> = Vec::new();
        while !cursor.is_empty() {
            let start: usize = cursor.pos();
            let mut group: Group = Group::parse(&mut cursor, ctx)?;
            group.source_range = Some(start..cursor.pos());
            groups.push(group);
        }

        Ok((header, header_range, groups))
    }

    /// Returns the functional type of this plugin.
    pub fn kind(&self) -> PluginKind {
        self.header.record.plugin_kind(&self.ctx)
    }

    /// Returns the full plugin file bytes.
    ///
    /// Backed by the original memory mapping when the plugin was opened from
    /// disk via [`Self::open`]; backed by an `Arc<[u8]>` copy when it was
    /// constructed via [`Self::from_bytes`]. Either way, all
    /// [`crate::Record::source_range`] / [`crate::Group::source_range`]
    /// values are valid indices into this slice.
    pub fn source_bytes(&self) -> &[u8] {
        &self.source
    }

    /// Returns `true` if the plugin has externalized strings (LOCALIZED flag).
    pub fn is_localized(&self) -> bool {
        use crate::types::RecordFlags;
        self.header
            .record
            .header
            .flags
            .contains(RecordFlags::LOCALIZED)
    }

    /// Returns the number of top-level GRUP records.
    pub fn group_count(&self) -> usize {
        self.groups.len()
    }

    /// Returns a slice of all top-level groups.
    pub fn groups(&self) -> &[Group] {
        &self.groups
    }

    /// Returns the ordered list of master plugin filenames.
    pub fn masters(&self) -> &[String] {
        &self.header.masters
    }

    /// Iterates over all records with `sig` across all groups (recursive).
    pub fn records_of(&self, sig: Signature) -> impl Iterator<Item = &Record> {
        self.groups
            .iter()
            .flat_map(|g| g.records_recursive())
            .filter(move |r| r.header.signature == sig)
    }

    /// Looks up the first record with the given raw `form_id` across all
    /// groups.
    ///
    /// This is a linear scan — it is intended for diagnostics and tests, not
    /// hot-path use.
    pub fn find_record(&self, form_id: FormId) -> Option<&Record> {
        self.groups
            .iter()
            .flat_map(|g| g.records_recursive())
            .find(|r| r.header.form_id == form_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        buf.extend_from_slice(&0u32.to_le_bytes()); // version_control
        buf.extend_from_slice(&0u16.to_le_bytes()); // form_version
        buf.extend_from_slice(&0u16.to_le_bytes()); // unknown
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
        buf.extend_from_slice(&0u32.to_le_bytes()); // version_control
        buf.extend_from_slice(&0u32.to_le_bytes()); // unknown
        buf.extend_from_slice(children);
        buf
    }

    fn minimal_plugin_bytes() -> Vec<u8> {
        let hedr = build_hedr(1.7, 0, 0x800);
        let tes4_data = build_subrecord(b"HEDR", &hedr);
        let tes4 = build_record(b"TES4", 0, 0, &tes4_data);

        let child_record = build_record(b"NPC_", 0, 0x01, &[]);
        let grup = build_grup(b"NPC_", 0, &child_record);

        let mut plugin: Vec<u8> = Vec::new();
        plugin.extend_from_slice(&tes4);
        plugin.extend_from_slice(&grup);
        plugin
    }

    /// Verifies that a minimal valid plugin parses without error.
    #[test]
    fn minimal_plugin_parses() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let bytes = minimal_plugin_bytes();
        let ctx = GameContext::sse();

        // when
        let plugin = Plugin::from_bytes(&bytes, ctx)?;

        // then
        assert_eq!(plugin.kind(), PluginKind::Plugin);
        assert_eq!(plugin.group_count(), 1);
        assert_eq!(plugin.groups()[0].records().count(), 1);
        Ok(())
    }

    /// Verifies that the HEDR version is read correctly.
    #[test]
    fn hedr_version_is_1_7() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let bytes = minimal_plugin_bytes();
        let ctx = GameContext::sse();

        // when
        let plugin = Plugin::from_bytes(&bytes, ctx)?;

        // then
        assert!((plugin.header.hedr_version - 1.7_f32).abs() < f32::EPSILON);
        Ok(())
    }
}
