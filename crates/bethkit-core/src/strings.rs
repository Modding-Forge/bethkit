// SPDX-License-Identifier: Apache-2.0
//!
//! Reader and writer for Bethesda `.STRINGS`, `.DLSTRINGS`, and `.ILSTRINGS`
//! localisation files.
//!
//! These files are referenced by lstring IDs stored in record subrecords when
//! the plugin is marked localised (see [`crate::RecordFlags::LOCALIZED`]).
//!
//! ## Binary layout
//!
//! ```text
//! u32  count               // number of entries
//! u32  data_size           // size of the data blob in bytes
//! [count × {
//!     u32 id               // lstring identifier
//!     u32 offset           // relative to start of data blob
//! }]
//! data_size bytes          // payload
//! ```
//!
//! In the data blob, `.STRINGS` files store payloads as null-terminated byte
//! sequences (`ZString`); `.DLSTRINGS` and `.ILSTRINGS` files prefix each
//! payload with a `u32` length **including** the trailing NUL.
//!
//! All string payloads are exposed and accepted as raw `Vec<u8>` because the
//! correct legacy code page depends on the language (see [`crate::encoding`]).

use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bethkit_io::{IoError, MappedFile, SliceCursor};

use crate::error::{CoreError, Result};

/// Identifies which of the three Bethesda string-table file formats applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StringFileKind {
    /// `.STRINGS` — payloads are null-terminated byte sequences.
    Strings,

    /// `.DLSTRINGS` — payloads are length-prefixed (`u32` including NUL).
    DLStrings,

    /// `.ILSTRINGS` — same on-disk layout as `.DLSTRINGS`, used for voiced
    /// dialogue lines.
    ILStrings,
}

impl StringFileKind {
    /// Returns the conventional uppercase file extension (without leading dot)
    /// for this kind.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Strings => "STRINGS",
            Self::DLStrings => "DLSTRINGS",
            Self::ILStrings => "ILSTRINGS",
        }
    }

    /// Detects the file kind from a path's extension.
    ///
    /// # Returns
    ///
    /// `None` if the path has no extension or an unrecognised one.
    pub fn from_path(path: &Path) -> Option<Self> {
        let ext: String = path.extension()?.to_str()?.to_ascii_uppercase();
        match ext.as_str() {
            "STRINGS" => Some(Self::Strings),
            "DLSTRINGS" => Some(Self::DLStrings),
            "ILSTRINGS" => Some(Self::ILStrings),
            _ => None,
        }
    }

    /// Returns `true` if this kind uses the length-prefixed payload format.
    pub fn is_length_prefixed(self) -> bool {
        matches!(self, Self::DLStrings | Self::ILStrings)
    }
}

/// Backing storage for an entry's bytes.
///
/// Borrowed entries point directly into the mapping that backs the table;
/// owned entries live on the heap and are produced by edits or by tables built
/// from scratch.
#[derive(Clone)]
enum EntryBytes {
    /// Slice into the mapped file. Stored together with the [`Arc`] so the
    /// mapping outlives the slice.
    Borrowed {
        source: Arc<MappedFile>,
        range: std::ops::Range<usize>,
    },

    /// Heap-allocated payload, e.g. from [`StringTable::insert`].
    Owned(Vec<u8>),
}

impl EntryBytes {
    fn as_slice(&self) -> &[u8] {
        match self {
            Self::Borrowed { source, range } => &source.as_bytes()[range.clone()],
            Self::Owned(v) => v.as_slice(),
        }
    }
}

/// A parsed Bethesda string-table file.
///
/// Entries are kept in a [`BTreeMap`] so iteration is deterministic by
/// identifier; this matches the order [`StringTable::write_to`] uses on
/// serialisation.
pub struct StringTable {
    kind: StringFileKind,
    entries: BTreeMap<u32, EntryBytes>,
    next_id: u32,
}

impl StringTable {
    /// Creates an empty table for the given kind.
    pub fn new(kind: StringFileKind) -> Self {
        Self {
            kind,
            entries: BTreeMap::new(),
            next_id: 1,
        }
    }

    /// Memory-maps `path` and parses it as a string table.
    ///
    /// The file kind is derived from the extension; pass an explicit kind via
    /// [`Self::open_as`] if the file extension is non-standard.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::Io`] on I/O failure or [`CoreError::InvalidStringTable`]
    /// if the file cannot be derived from the extension or if the layout is
    /// malformed.
    pub fn open(path: &Path) -> Result<Self> {
        let kind: StringFileKind = StringFileKind::from_path(path)
            .ok_or_else(|| CoreError::InvalidStringTable("unknown file extension".into()))?;
        Self::open_as(path, kind)
    }

    /// Memory-maps `path` and parses it as the given kind.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::Io`] on I/O failure or
    /// [`CoreError::InvalidStringTable`] if the layout is malformed.
    pub fn open_as(path: &Path, kind: StringFileKind) -> Result<Self> {
        let mapped: Arc<MappedFile> = Arc::new(MappedFile::open(path)?);
        Self::parse(mapped, kind)
    }

    /// Parses a string table from an in-memory byte slice.
    ///
    /// Entries returned by [`Self::get`] are heap-allocated copies because
    /// there is no backing mapping. For zero-copy access, use
    /// [`Self::open`] or [`Self::open_as`] instead.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidStringTable`] if the layout is malformed.
    pub fn from_bytes(bytes: &[u8], kind: StringFileKind) -> Result<Self> {
        let mut entries: BTreeMap<u32, EntryBytes> = BTreeMap::new();
        let mut next_id: u32 = 1;
        let directory: Vec<(u32, u32)> = parse_directory(bytes)?;
        let blob_start: usize = 8 + directory.len() * 8;
        for (id, offset) in directory {
            let entry_start: usize = blob_start
                .checked_add(offset as usize)
                .ok_or_else(|| CoreError::InvalidStringTable("offset overflow".into()))?;
            let payload: &[u8] = read_payload(bytes, entry_start, kind)?;
            entries.insert(id, EntryBytes::Owned(payload.to_vec()));
            if id.saturating_add(1) > next_id {
                next_id = id + 1;
            }
        }
        Ok(Self {
            kind,
            entries,
            next_id,
        })
    }

    /// Returns the kind of this table.
    pub fn kind(&self) -> StringFileKind {
        self.kind
    }

    /// Returns the number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` if the table is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Looks up a string by lstring identifier.
    ///
    /// The returned slice does not include the trailing NUL byte.
    pub fn get(&self, id: u32) -> Option<&[u8]> {
        self.entries.get(&id).map(EntryBytes::as_slice)
    }

    /// Returns an iterator over `(id, bytes)` pairs in ascending id order.
    pub fn iter(&self) -> impl Iterator<Item = (u32, &[u8])> {
        self.entries
            .iter()
            .map(|(id, bytes)| (*id, bytes.as_slice()))
    }

    /// Inserts or replaces an entry at `id`.
    ///
    /// `payload` must not contain the trailing NUL byte; the writer adds it
    /// automatically.
    pub fn insert(&mut self, id: u32, payload: Vec<u8>) {
        self.entries.insert(id, EntryBytes::Owned(payload));
        if id.saturating_add(1) > self.next_id {
            self.next_id = id + 1;
        }
    }

    /// Allocates a fresh identifier, inserts `payload`, and returns the
    /// identifier.
    pub fn insert_new(&mut self, payload: Vec<u8>) -> u32 {
        let id: u32 = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        self.entries.insert(id, EntryBytes::Owned(payload));
        id
    }

    /// Removes an entry, returning its payload if it existed.
    pub fn remove(&mut self, id: u32) -> Option<Vec<u8>> {
        self.entries.remove(&id).map(|e| match e {
            EntryBytes::Owned(v) => v,
            EntryBytes::Borrowed { source, range } => source.as_bytes()[range].to_vec(),
        })
    }

    /// Serialises the table to `writer`.
    ///
    /// # Errors
    ///
    /// Returns [`io::Error`] from the underlying writer.
    pub fn write_to(&self, writer: &mut impl Write) -> io::Result<()> {
        let count: u32 = self.entries.len() as u32;

        let mut blob: Vec<u8> = Vec::new();
        let mut directory: Vec<u8> = Vec::with_capacity(self.entries.len() * 8);
        for (id, bytes) in self.entries.iter() {
            let offset: u32 = blob.len() as u32;
            directory.extend_from_slice(&id.to_le_bytes());
            directory.extend_from_slice(&offset.to_le_bytes());
            let payload: &[u8] = bytes.as_slice();
            if self.kind.is_length_prefixed() {
                let length: u32 = payload.len() as u32 + 1;
                blob.extend_from_slice(&length.to_le_bytes());
            }
            blob.extend_from_slice(payload);
            blob.push(0);
        }

        writer.write_all(&count.to_le_bytes())?;
        writer.write_all(&(blob.len() as u32).to_le_bytes())?;
        writer.write_all(&directory)?;
        writer.write_all(&blob)?;
        Ok(())
    }

    /// Returns the three sibling paths corresponding to this plugin and
    /// language, in the order `[STRINGS, DLSTRINGS, ILSTRINGS]`.
    ///
    /// The plugin path's stem is combined with the language; the returned
    /// paths are placed in `<plugin_dir>/Strings/<stem>_<language>.<ext>`,
    /// matching the on-disk layout used by Skyrim and Fallout 4.
    pub fn sibling_paths(plugin_path: &Path, language: &str) -> [PathBuf; 3] {
        let dir: PathBuf = plugin_path
            .parent()
            .map(|p| p.join("Strings"))
            .unwrap_or_else(|| PathBuf::from("Strings"));
        let stem: String = plugin_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("plugin")
            .to_string();
        let make = |kind: StringFileKind| -> PathBuf {
            dir.join(format!("{stem}_{language}.{}", kind.extension()))
        };
        [
            make(StringFileKind::Strings),
            make(StringFileKind::DLStrings),
            make(StringFileKind::ILStrings),
        ]
    }

    fn parse(source: Arc<MappedFile>, kind: StringFileKind) -> Result<Self> {
        let bytes: &[u8] = source.as_bytes();
        let directory: Vec<(u32, u32)> = parse_directory(bytes)?;
        let blob_start: usize = 8 + directory.len() * 8;

        let mut entries: BTreeMap<u32, EntryBytes> = BTreeMap::new();
        let mut next_id: u32 = 1;
        for (id, offset) in directory {
            let entry_start: usize = blob_start
                .checked_add(offset as usize)
                .ok_or_else(|| CoreError::InvalidStringTable("offset overflow".into()))?;
            let payload: &[u8] = read_payload(bytes, entry_start, kind)?;
            // Compute the absolute byte range of `payload` inside the mapping.
            let payload_offset: usize = payload.as_ptr() as usize - bytes.as_ptr() as usize;
            let range: std::ops::Range<usize> = payload_offset..payload_offset + payload.len();
            entries.insert(
                id,
                EntryBytes::Borrowed {
                    source: source.clone(),
                    range,
                },
            );
            if id.saturating_add(1) > next_id {
                next_id = id + 1;
            }
        }
        Ok(Self {
            kind,
            entries,
            next_id,
        })
    }
}

impl std::fmt::Debug for StringTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StringTable")
            .field("kind", &self.kind)
            .field("entries", &self.entries.len())
            .field("next_id", &self.next_id)
            .finish()
    }
}

/// Reads the directory portion of a string-table file.
fn parse_directory(bytes: &[u8]) -> Result<Vec<(u32, u32)>> {
    if bytes.len() < 8 {
        return Err(CoreError::InvalidStringTable(
            "file shorter than 8-byte header".into(),
        ));
    }
    let mut cursor: SliceCursor<'_> = SliceCursor::new(bytes);
    let count: u32 = cursor.read_u32().map_err(map_io)?;
    let _data_size: u32 = cursor.read_u32().map_err(map_io)?;
    let count_us: usize = count as usize;
    if 8 + count_us.saturating_mul(8) > bytes.len() {
        return Err(CoreError::InvalidStringTable(
            "directory exceeds file size".into(),
        ));
    }
    let mut directory: Vec<(u32, u32)> = Vec::with_capacity(count_us);
    for _ in 0..count {
        let id: u32 = cursor.read_u32().map_err(map_io)?;
        let offset: u32 = cursor.read_u32().map_err(map_io)?;
        directory.push((id, offset));
    }
    Ok(directory)
}

/// Reads a single payload from the data blob, returning the slice without
/// the trailing NUL.
fn read_payload(bytes: &[u8], start: usize, kind: StringFileKind) -> Result<&[u8]> {
    if start >= bytes.len() {
        return Err(CoreError::InvalidStringTable(
            "payload offset out of range".into(),
        ));
    }
    if kind.is_length_prefixed() {
        if start + 4 > bytes.len() {
            return Err(CoreError::InvalidStringTable(
                "truncated length prefix".into(),
            ));
        }
        let length: u32 = u32::from_le_bytes([
            bytes[start],
            bytes[start + 1],
            bytes[start + 2],
            bytes[start + 3],
        ]);
        if length == 0 {
            return Ok(&bytes[start + 4..start + 4]);
        }
        let payload_start: usize = start + 4;
        let payload_end_with_nul: usize = payload_start
            .checked_add(length as usize)
            .ok_or_else(|| CoreError::InvalidStringTable("length overflow".into()))?;
        if payload_end_with_nul > bytes.len() {
            return Err(CoreError::InvalidStringTable("payload exceeds file".into()));
        }
        // length includes the trailing NUL — exclude it from the returned slice.
        Ok(&bytes[payload_start..payload_end_with_nul - 1])
    } else {
        // Null-terminated: scan forward from `start` until NUL.
        let mut end: usize = start;
        while end < bytes.len() && bytes[end] != 0 {
            end += 1;
        }
        if end >= bytes.len() {
            return Err(CoreError::InvalidStringTable("zstring missing NUL".into()));
        }
        Ok(&bytes[start..end])
    }
}

fn map_io(err: IoError) -> CoreError {
    CoreError::Io(err)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_table(kind: StringFileKind, entries: &[(u32, &[u8])]) -> Vec<u8> {
        let mut table = StringTable::new(kind);
        for (id, payload) in entries {
            table.insert(*id, payload.to_vec());
        }
        let mut buf: Vec<u8> = Vec::new();
        table.write_to(&mut buf).expect("write to Vec cannot fail");
        buf
    }

    /// Verifies that a `.STRINGS` table round-trips through write+parse.
    #[test]
    fn strings_kind_roundtrips() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let entries: &[(u32, &[u8])] = &[(1, b"Hello"), (2, b"World"), (42, b"Bethesda")];

        // when
        let bytes: Vec<u8> = build_table(StringFileKind::Strings, entries);
        let parsed: StringTable = StringTable::from_bytes(&bytes, StringFileKind::Strings)?;

        // then
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed.get(1), Some(&b"Hello"[..]));
        assert_eq!(parsed.get(2), Some(&b"World"[..]));
        assert_eq!(parsed.get(42), Some(&b"Bethesda"[..]));
        assert_eq!(parsed.get(99), None);
        Ok(())
    }

    /// Verifies that a `.DLSTRINGS` table round-trips, including length prefix
    /// handling.
    #[test]
    fn dlstrings_kind_roundtrips() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let entries: &[(u32, &[u8])] = &[
            (1, b"Long description with multiple words."),
            (2, b""),
            (3, b"x"),
        ];

        // when
        let bytes: Vec<u8> = build_table(StringFileKind::DLStrings, entries);
        let parsed: StringTable = StringTable::from_bytes(&bytes, StringFileKind::DLStrings)?;

        // then
        assert_eq!(parsed.len(), 3);
        assert_eq!(
            parsed.get(1),
            Some(&b"Long description with multiple words."[..])
        );
        assert_eq!(parsed.get(2), Some(&b""[..]));
        assert_eq!(parsed.get(3), Some(&b"x"[..]));
        Ok(())
    }

    /// Verifies that the writer emits a byte-identical layout when nothing has
    /// been edited.
    #[test]
    fn write_then_parse_then_write_is_stable() -> std::result::Result<(), Box<dyn std::error::Error>>
    {
        // given
        let entries: &[(u32, &[u8])] = &[(7, b"alpha"), (3, b"beta"), (10, b"gamma")];
        let original: Vec<u8> = build_table(StringFileKind::Strings, entries);

        // when
        let parsed: StringTable = StringTable::from_bytes(&original, StringFileKind::Strings)?;
        let mut rewritten: Vec<u8> = Vec::new();
        parsed.write_to(&mut rewritten)?;

        // then
        assert_eq!(rewritten, original);
        Ok(())
    }

    /// Verifies that `insert_new` allocates monotonically increasing IDs.
    #[test]
    fn insert_new_allocates_fresh_ids() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let mut table = StringTable::new(StringFileKind::DLStrings);
        table.insert(5, b"five".to_vec());

        // when
        let a: u32 = table.insert_new(b"a".to_vec());
        let b: u32 = table.insert_new(b"b".to_vec());

        // then
        assert_eq!(a, 6);
        assert_eq!(b, 7);
        Ok(())
    }

    /// Verifies the file-extension detection helper.
    #[test]
    fn from_path_detects_kind_case_insensitively(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let cases: &[(&str, Option<StringFileKind>)] = &[
            ("MyMod_English.STRINGS", Some(StringFileKind::Strings)),
            ("MyMod_English.dlstrings", Some(StringFileKind::DLStrings)),
            ("MyMod_English.ilstrings", Some(StringFileKind::ILStrings)),
            ("MyMod_English.txt", None),
        ];

        // when / then
        for (name, expected) in cases {
            assert_eq!(
                StringFileKind::from_path(Path::new(name)),
                *expected,
                "path: {name}"
            );
        }
        Ok(())
    }

    /// Verifies the sibling-path helper builds the conventional layout.
    #[test]
    fn sibling_paths_use_strings_subdirectory(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let plugin: PathBuf = PathBuf::from("C:/games/Skyrim/Data/MyMod.esp");

        // when
        let paths: [PathBuf; 3] = StringTable::sibling_paths(&plugin, "English");

        // then
        assert!(paths[0].ends_with("Strings/MyMod_English.STRINGS"));
        assert!(paths[1].ends_with("Strings/MyMod_English.DLSTRINGS"));
        assert!(paths[2].ends_with("Strings/MyMod_English.ILSTRINGS"));
        Ok(())
    }

    /// Verifies that an empty table writes the documented 8-byte header.
    #[test]
    fn empty_table_writes_eight_byte_header() -> std::result::Result<(), Box<dyn std::error::Error>>
    {
        // given
        let table = StringTable::new(StringFileKind::Strings);

        // when
        let mut buf: Vec<u8> = Vec::new();
        table.write_to(&mut buf)?;

        // then
        assert_eq!(buf, vec![0u8; 8]);
        let parsed: StringTable = StringTable::from_bytes(&buf, StringFileKind::Strings)?;
        assert!(parsed.is_empty());
        Ok(())
    }
}
