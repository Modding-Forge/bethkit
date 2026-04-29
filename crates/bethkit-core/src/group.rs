// SPDX-License-Identifier: Apache-2.0
//!
//! GRUP record parsing.
//!
//! Binary layout (all little-endian):
//!
//! **GRUP header — 24 bytes**
//! ```text
//! 0   [u8;4]  signature = b"GRUP"
//! 4   u32     group_size   (includes this 24-byte header)
//! 8   [u8;4]  label        (meaning depends on group_type)
//! 12  i32     group_type
//! 16  u32     version_control
//! 20  u32     unknown
//! ```
//!
//! The children span `group_size - 24` bytes and consist of alternating GRUP
//! headers and main record headers.

use bethkit_io::SliceCursor;

use crate::error::{CoreError, Result};
use crate::record::Record;
use crate::types::{FormId, GameContext, Signature};

/// The type of a GRUP record, which determines how the label field is
/// interpreted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum GroupType {
    /// Top-level group — label is a record signature (e.g. `b"NPC_"`).
    Normal = 0,
    /// Children of a worldspace — label is a worldspace FormID.
    WorldChildren = 1,
    /// Interior cell block — label is a block number (`i32`).
    InteriorCellBlock = 2,
    /// Interior cell sub-block — label is a sub-block number (`i32`).
    InteriorCellSubBlock = 3,
    /// Exterior cell block — label encodes grid X/Y (`i16`, `i16`).
    ExteriorCellBlock = 4,
    /// Exterior cell sub-block — label encodes grid X/Y (`i16`, `i16`).
    ExteriorCellSubBlock = 5,
    /// Children of a cell — label is a cell FormID.
    CellChildren = 6,
    /// Children of a DIAL topic — label is a topic FormID.
    TopicChildren = 7,
    /// Persistent children of a cell — label is a cell FormID.
    CellPersistentChildren = 8,
    /// Temporary children of a cell — label is a cell FormID.
    CellTemporaryChildren = 9,
}

impl GroupType {
    /// Converts a raw `i32` from the file into a [`GroupType`].
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidGroupType`] for unknown values.
    pub fn from_raw(value: i32) -> Result<Self> {
        Ok(match value {
            0 => Self::Normal,
            1 => Self::WorldChildren,
            2 => Self::InteriorCellBlock,
            3 => Self::InteriorCellSubBlock,
            4 => Self::ExteriorCellBlock,
            5 => Self::ExteriorCellSubBlock,
            6 => Self::CellChildren,
            7 => Self::TopicChildren,
            8 => Self::CellPersistentChildren,
            9 => Self::CellTemporaryChildren,
            v => return Err(CoreError::InvalidGroupType(v)),
        })
    }
}

/// The label field of a GRUP header, interpreted according to the group type.
#[derive(Debug, Clone, Copy)]
pub enum GroupLabel {
    /// A 4-byte record signature (used for [`GroupType::Normal`]).
    Signature(Signature),
    /// A FormID referencing the parent record (World/Topic/Cell children).
    FormId(FormId),
    /// An exterior cell grid coordinate.
    GridCell {
        /// X coordinate (signed 16-bit).
        x: i16,
        /// Y coordinate (signed 16-bit).
        y: i16,
    },
    /// An interior block number.
    BlockNumber(i32),
}

impl GroupLabel {
    /// Interprets the raw 4-byte label according to `group_type`.
    pub fn from_raw(raw: [u8; 4], group_type: GroupType) -> Self {
        match group_type {
            GroupType::Normal => Self::Signature(Signature(raw)),
            GroupType::WorldChildren
            | GroupType::CellChildren
            | GroupType::TopicChildren
            | GroupType::CellPersistentChildren
            | GroupType::CellTemporaryChildren => Self::FormId(FormId(u32::from_le_bytes(raw))),
            GroupType::ExteriorCellBlock | GroupType::ExteriorCellSubBlock => {
                let x: i16 = i16::from_le_bytes([raw[0], raw[1]]);
                let y: i16 = i16::from_le_bytes([raw[2], raw[3]]);
                Self::GridCell { x, y }
            }
            GroupType::InteriorCellBlock | GroupType::InteriorCellSubBlock => {
                Self::BlockNumber(i32::from_le_bytes(raw))
            }
        }
    }
}


/// The 24-byte header of a GRUP block.
pub struct GroupHeader {
    /// Total size of this group in bytes, including the 24-byte header.
    pub group_size: u32,
    /// Interpreted label value.
    pub label: GroupLabel,
    /// The group type.
    pub group_type: GroupType,
    /// Version control information (not interpreted by bethkit).
    pub version_control: u32,
    /// Trailing 4-byte field of unknown purpose in the GRUP header.
    /// Preserved verbatim on roundtrip.
    pub unknown: u32,
}

/// A single child inside a [`Group`] — either a main record or a nested group.
pub enum GroupChild {
    /// A main (non-GRUP) record.
    Record(Record),
    /// A nested GRUP block.
    Group(Group),
}

/// A GRUP block with its header and all parsed children.
pub struct Group {
    /// Parsed GRUP header.
    pub header: GroupHeader,
    /// Byte range of this group (header + children) within the plugin source,
    /// if known. Used by [`crate::PluginPatcher`] to copy untouched groups
    /// verbatim instead of re-serialising them.
    pub source_range: Option<std::ops::Range<usize>>,
    children: Vec<GroupChild>,
}

impl Group {
    /// Parses a GRUP from `cursor`, including all nested children.
    ///
    /// The cursor must be positioned at the `G` of `b"GRUP"`. All
    /// `cursor.pos()` values captured into [`Self::source_range`] and into
    /// child [`Record::source_range`] fields are taken directly from this
    /// cursor, so they are absolute offsets if `cursor` operates on the full
    /// plugin source.
    ///
    /// # Arguments
    ///
    /// * `cursor` - Positioned at the start of the GRUP signature.
    /// * `ctx`    - Game context forwarded to child record parsers.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidSignature`] if the first 4 bytes are not
    /// `b"GRUP"`, or other [`CoreError`] variants on malformed data.
    pub fn parse(cursor: &mut SliceCursor<'_>, ctx: &GameContext) -> Result<Self> {
        let group_start: usize = cursor.pos();

        let sig_bytes: [u8; 4] = cursor.read_array()?;
        let sig: Signature = Signature(sig_bytes);

        if sig != Signature::GRUP {
            return Err(CoreError::InvalidSignature {
                expected: Signature::GRUP.to_string(),
                got: sig.to_string(),
            });
        }

        let group_size: u32 = cursor.read_u32()?;
        let label_raw: [u8; 4] = cursor.read_array()?;
        let group_type_raw: i32 = cursor.read_i32()?;
        let version_control: u32 = cursor.read_u32()?;
        let unknown: u32 = cursor.read_u32()?;

        let group_type: GroupType = GroupType::from_raw(group_type_raw)?;
        let label: GroupLabel = GroupLabel::from_raw(label_raw, group_type);

        let header: GroupHeader = GroupHeader {
            group_size,
            label,
            group_type,
            version_control,
            unknown,
        };

        // Children span from the current position up to group_start + group_size.
        let children_end: usize =
            group_start
                .checked_add(group_size as usize)
                .ok_or(CoreError::UnexpectedEof {
                    context: "group_size overflow",
                })?;
        let children: Vec<GroupChild> = parse_children(cursor, ctx, children_end)?;

        Ok(Self {
            header,
            source_range: None,
            children,
        })
    }

    /// Iterates over all direct child records (non-recursive).
    pub fn records(&self) -> impl Iterator<Item = &Record> {
        self.children.iter().filter_map(|c| match c {
            GroupChild::Record(r) => Some(r),
            GroupChild::Group(_) => None,
        })
    }

    /// Iterates over all records in this group and all nested groups
    /// (depth-first).
    pub fn records_recursive(&self) -> impl Iterator<Item = &Record> {
        RecordsIter::new(self)
    }

    /// Iterates over all direct child subgroups.
    pub fn subgroups(&self) -> impl Iterator<Item = &Group> {
        self.children.iter().filter_map(|c| match c {
            GroupChild::Group(g) => Some(g),
            GroupChild::Record(_) => None,
        })
    }

    /// Returns the full children slice (records and groups interleaved).
    pub fn children(&self) -> &[GroupChild] {
        &self.children
    }

    /// Returns the original source bytes of this group (header + children) if
    /// the parser captured a [`Self::source_range`] for it and `source` is
    /// large enough to contain the range.
    pub fn source_bytes<'a>(&self, source: &'a [u8]) -> Option<&'a [u8]> {
        let range: std::ops::Range<usize> = self.source_range.clone()?;
        source.get(range)
    }
}

/// Size in bytes of the GRUP header, excluding any children.
pub(crate) const GROUP_HEADER_SIZE: usize = 24;

/// Parses children from `cursor` until `cursor.pos() >= end`.
///
/// Each child starts with a 4-byte signature: `b"GRUP"` → nested group,
/// anything else → main record.
///
/// Source-byte ranges are captured for both records and groups using
/// `cursor.pos()` before and after each child. As long as the caller passes
/// a top-level cursor over the entire plugin source, these ranges are
/// absolute file offsets.
///
/// # Errors
///
/// Returns [`CoreError`] on malformed data.
fn parse_children(
    cursor: &mut SliceCursor<'_>,
    ctx: &GameContext,
    end: usize,
) -> Result<Vec<GroupChild>> {
    let mut children: Vec<GroupChild> = Vec::new();

    while cursor.pos() < end {
        let sig_bytes: Option<&[u8]> = cursor.peek_bytes(4);
        let start: usize = cursor.pos();
        match sig_bytes {
            Some(b) if b == b"GRUP" => {
                let mut group: Group = Group::parse(cursor, ctx)?;
                group.source_range = Some(start..cursor.pos());
                children.push(GroupChild::Group(group));
            }
            Some(_) => {
                let mut record: Record = Record::parse_header(cursor, ctx)?;
                record.source_range = Some(start..cursor.pos());
                children.push(GroupChild::Record(record));
            }
            None => break,
        }
    }

    if cursor.pos() != end {
        return Err(CoreError::UnexpectedEof {
            context: "group children misaligned with group_size",
        });
    }

    Ok(children)
}

/// Depth-first iterator over all records in a group tree.
struct RecordsIter<'a> {
    stack: Vec<&'a [GroupChild]>,
    pos: Vec<usize>,
}

impl<'a> RecordsIter<'a> {
    fn new(group: &'a Group) -> Self {
        Self {
            stack: vec![group.children()],
            pos: vec![0],
        }
    }
}

impl<'a> Iterator for RecordsIter<'a> {
    type Item = &'a Record;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let depth: usize = self.stack.len().checked_sub(1)?;
            let idx: usize = self.pos[depth];
            let children: &[GroupChild] = self.stack[depth];

            if idx >= children.len() {
                self.stack.pop();
                self.pos.pop();
                continue;
            }

            self.pos[depth] += 1;

            match &children[idx] {
                GroupChild::Record(r) => return Some(r),
                GroupChild::Group(g) => {
                    self.stack.push(g.children());
                    self.pos.push(0);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_record_bytes(sig: &[u8; 4], form_id: u32) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(sig);
        buf.extend_from_slice(&0u32.to_le_bytes()); // data_size = 0
        buf.extend_from_slice(&0u32.to_le_bytes()); // flags
        buf.extend_from_slice(&form_id.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes()); // version_control
        buf.extend_from_slice(&0u16.to_le_bytes()); // form_version
        buf.extend_from_slice(&0u16.to_le_bytes()); // unknown
        buf
    }

    fn build_grup_bytes(group_type: i32, children: &[u8]) -> Vec<u8> {
        let group_size: u32 = 24 + children.len() as u32;
        let mut buf = Vec::new();
        buf.extend_from_slice(b"GRUP");
        buf.extend_from_slice(&group_size.to_le_bytes());
        buf.extend_from_slice(b"NPC_"); // label
        buf.extend_from_slice(&group_type.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes()); // version_control
        buf.extend_from_slice(&0u32.to_le_bytes()); // unknown
        buf.extend_from_slice(children);
        buf
    }

    /// Verifies that a GRUP with two child records is parsed correctly.
    #[test]
    fn group_with_two_records() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let mut children: Vec<u8> = Vec::new();
        children.extend_from_slice(&build_record_bytes(b"NPC_", 0x01));
        children.extend_from_slice(&build_record_bytes(b"NPC_", 0x02));
        let bytes = build_grup_bytes(0, &children);
        let ctx = GameContext::sse();
        let mut cursor = SliceCursor::new(&bytes);

        // when
        let group = Group::parse(&mut cursor, &ctx)?;

        // then
        assert_eq!(group.records().count(), 2);
        Ok(())
    }

    /// Verifies that records_recursive descends into nested groups.
    #[test]
    fn records_recursive_descends() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given — outer GRUP contains an inner GRUP with one record
        let inner_record = build_record_bytes(b"CELL", 0x42);
        let inner_grup = build_grup_bytes(1, &inner_record);
        let outer_bytes = build_grup_bytes(0, &inner_grup);
        let ctx = GameContext::sse();
        let mut cursor = SliceCursor::new(&outer_bytes);

        // when
        let group = Group::parse(&mut cursor, &ctx)?;
        let count: usize = group.records_recursive().count();

        // then
        assert_eq!(count, 1);
        Ok(())
    }

    /// Verifies that GroupType::from_raw rejects unknown values.
    #[test]
    fn invalid_group_type_returns_error() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let raw_value: i32 = 99;

        // when
        let result = GroupType::from_raw(raw_value);

        // then
        assert!(matches!(result, Err(CoreError::InvalidGroupType(99))));
        Ok(())
    }
}
