// SPDX-License-Identifier: Apache-2.0
//!
//! Record header, subrecord, and main record parsing.
//!
//! Binary layout reference (all little-endian):
//!
//! **Record header — 24 bytes**
//! ```text
//! 0   [u8;4]  signature
//! 4   u32     data_size
//! 8   u32     flags
//! 12  u32     form_id
//! 16  u32     version_control
//! 20  u16     form_version
//! 22  u16     unknown
//! ```
//!
//! **SubRecord header — 6 bytes**
//! ```text
//! 0   [u8;4]  signature
//! 4   u16     data_size  (see XXXX override)
//! ```
//!
//! **XXXX override** — when a subrecord's data would exceed 65 535 bytes, a
//! preceding `XXXX` subrecord (4-byte `u32`) carries the real size. The next
//! subrecord header's `data_size` field is then ignored.

use std::sync::{Arc, OnceLock};

use bethkit_io::SliceCursor;

use crate::error::{CoreError, Result};
use crate::types::{FormId, GameContext, PluginKind, RecordFlags, Signature};


/// The raw data payload of a subrecord.
///
/// Owned data is produced when the parent record was zlib-compressed and the
/// subrecords were parsed from the decompressed buffer.
pub enum SubRecordData {
    /// A zero-copy slice into the original memory-mapped data.
    Borrowed(Arc<[u8]>),
    /// Decompressed or otherwise reconstructed data, heap-allocated.
    Owned(Vec<u8>),
}

impl SubRecordData {
    /// Returns a byte slice over the subrecord data regardless of variant.
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            Self::Borrowed(arc) => arc.as_ref(),
            Self::Owned(vec) => vec.as_slice(),
        }
    }
}

/// A single subrecord parsed from a main record's data block.
pub struct SubRecord {
    /// The 4-byte signature identifying the subrecord type.
    pub signature: Signature,
    /// Raw payload bytes.
    pub data: SubRecordData,
}

impl SubRecord {
    /// Returns the raw bytes of this subrecord's data.
    pub fn as_bytes(&self) -> &[u8] {
        self.data.as_bytes()
    }

    /// Interprets the data as a single `u8`.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidEncoding`] if the data is not exactly 1 byte.
    pub fn as_u8(&self) -> Result<u8> {
        let bytes: &[u8] = self.as_bytes();
        if bytes.len() != 1 {
            return Err(CoreError::InvalidEncoding(format!(
                "expected 1 byte for {} subrecord, got {}",
                self.signature,
                bytes.len()
            )));
        }
        Ok(bytes[0])
    }

    /// Interprets the data as a little-endian `u16`.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidEncoding`] if the data is not exactly 2 bytes.
    pub fn as_u16(&self) -> Result<u16> {
        let bytes: &[u8] = self.as_bytes();
        let arr: [u8; 2] = bytes.try_into().map_err(|_| {
            CoreError::InvalidEncoding(format!(
                "expected 2 bytes for {} subrecord, got {}",
                self.signature,
                bytes.len()
            ))
        })?;
        Ok(u16::from_le_bytes(arr))
    }

    /// Interprets the data as a little-endian `u32`.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidEncoding`] if the data is not exactly 4 bytes.
    pub fn as_u32(&self) -> Result<u32> {
        let bytes: &[u8] = self.as_bytes();
        let arr: [u8; 4] = bytes.try_into().map_err(|_| {
            CoreError::InvalidEncoding(format!(
                "expected 4 bytes for {} subrecord, got {}",
                self.signature,
                bytes.len()
            ))
        })?;
        Ok(u32::from_le_bytes(arr))
    }

    /// Interprets the data as a little-endian `f32`.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidEncoding`] if the data is not exactly 4 bytes.
    pub fn as_f32(&self) -> Result<f32> {
        let bytes: &[u8] = self.as_bytes();
        let arr: [u8; 4] = bytes.try_into().map_err(|_| {
            CoreError::InvalidEncoding(format!(
                "expected 4 bytes for {} subrecord, got {}",
                self.signature,
                bytes.len()
            ))
        })?;
        Ok(f32::from_le_bytes(arr))
    }

    /// Interprets the data as a NUL-terminated UTF-8 string.
    ///
    /// All trailing NUL bytes are stripped. The returned slice borrows from
    /// `self`.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidEncoding`] if the bytes are not valid UTF-8.
    pub fn as_zstring(&self) -> Result<&str> {
        let bytes: &[u8] = self.as_bytes();
        let trimmed: &[u8] = {
            let mut end = bytes.len();
            while end > 0 && bytes[end - 1] == 0 {
                end -= 1;
            }
            &bytes[..end]
        };
        std::str::from_utf8(trimmed).map_err(|e| {
            CoreError::InvalidEncoding(format!(
                "invalid UTF-8 in {} subrecord: {e}",
                self.signature
            ))
        })
    }

    /// Interprets the data as a signed `i8`.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::Io`] if the data is not exactly 1 byte.
    pub fn as_i8(&self) -> Result<i8> {
        Ok(self.as_u8()? as i8)
    }

    /// Interprets the data as a little-endian `i16`.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidEncoding`] if the data is not exactly 2 bytes.
    pub fn as_i16(&self) -> Result<i16> {
        let bytes: &[u8] = self.as_bytes();
        let arr: [u8; 2] = bytes.try_into().map_err(|_| {
            CoreError::InvalidEncoding(format!(
                "expected 2 bytes for {} subrecord, got {}",
                self.signature,
                bytes.len()
            ))
        })?;
        Ok(i16::from_le_bytes(arr))
    }

    /// Interprets the data as a little-endian `i32`.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidEncoding`] if the data is not exactly 4 bytes.
    pub fn as_i32(&self) -> Result<i32> {
        let bytes: &[u8] = self.as_bytes();
        let arr: [u8; 4] = bytes.try_into().map_err(|_| {
            CoreError::InvalidEncoding(format!(
                "expected 4 bytes for {} subrecord, got {}",
                self.signature,
                bytes.len()
            ))
        })?;
        Ok(i32::from_le_bytes(arr))
    }

    /// Interprets the data as a raw [`FormId`].
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::Io`] if the data is not exactly 4 bytes.
    pub fn as_form_id(&self) -> Result<FormId> {
        Ok(FormId(self.as_u32()?))
    }
}

/// Parses all subrecords from a byte slice that represents a record's data
/// block.
///
/// Handles the XXXX large-field override: when a `XXXX` subrecord is
/// encountered, its 4-byte payload is used as the data size for the
/// immediately following subrecord (overriding the 16-bit size field in that
/// subrecord's header, which may be 0 or garbage).
///
/// # Arguments
///
/// * `data`  - Raw record data block (already decompressed if necessary).
///
/// # Errors
///
/// Returns [`CoreError`] on malformed data.
fn parse_subrecords(data: Arc<[u8]>) -> Result<Vec<SubRecord>> {
    let mut cursor: SliceCursor<'_> = SliceCursor::new(&data);
    let mut subrecords: Vec<SubRecord> = Vec::new();
    // Carries the real size for the next subrecord when a XXXX was seen.
    let mut pending_xxxx_size: Option<u32> = None;

    while !cursor.is_empty() {
        let sig_bytes: [u8; 4] = cursor.read_array()?;
        let sig: Signature = Signature(sig_bytes);
        let raw_size: u16 = cursor.read_u16()?;

        if sig == Signature::XXXX {
            // XXXX always carries exactly 4 bytes containing a u32 real size.
            if raw_size != 4 {
                return Err(CoreError::InvalidEncoding(format!(
                    "XXXX subrecord has unexpected size {raw_size}, expected 4"
                )));
            }
            let mut xxxx_cursor: SliceCursor<'_> = cursor.sub_cursor(4)?;
            let real_size: u32 = xxxx_cursor.read_u32()?;
            pending_xxxx_size = Some(real_size);
            continue;
        }

        let data_size: usize = match pending_xxxx_size.take() {
            Some(real) => real as usize,
            None => raw_size as usize,
        };

        let slice: &[u8] = cursor.read_slice(data_size)?;
        // Build a sub-Arc that shares ownership with the record's data Arc.
        // We copy only the required bytes to avoid lifetime entanglement with
        // the cursor borrow.
        // NOTE: A zero-copy approach would require unsafe aliasing; copying
        //       keeps the API safe and the cost is only paid on first parse.
        let owned: Vec<u8> = slice.to_vec();

        subrecords.push(SubRecord {
            signature: sig,
            data: SubRecordData::Owned(owned),
        });
    }

    Ok(subrecords)
}

/// The 24-byte header that precedes every main record.
pub struct RecordHeader {
    /// 4-byte signature identifying the record type.
    pub signature: Signature,
    /// Size in bytes of the data block that immediately follows this header.
    pub data_size: u32,
    /// Record-level flags.
    pub flags: RecordFlags,
    /// Raw FormID as stored in the file.
    pub form_id: FormId,
    /// Version control info (not interpreted by bethkit).
    pub version_control: u32,
    /// Format version used by the Creation Engine for this record.
    pub form_version: u16,
    /// Reserved / unknown field.
    pub unknown: u16,
}

impl RecordHeader {
    /// Parses a 24-byte record header from `cursor`.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::Io`] if fewer than 24 bytes remain.
    pub fn parse(cursor: &mut SliceCursor<'_>) -> Result<Self> {
        let sig_bytes: [u8; 4] = cursor.read_array()?;
        let data_size: u32 = cursor.read_u32()?;
        let flags_raw: u32 = cursor.read_u32()?;
        let form_id_raw: u32 = cursor.read_u32()?;
        let version_control: u32 = cursor.read_u32()?;
        let form_version: u16 = cursor.read_u16()?;
        let unknown: u16 = cursor.read_u16()?;

        Ok(Self {
            signature: Signature(sig_bytes),
            data_size,
            flags: RecordFlags::from_bits_retain(flags_raw),
            form_id: FormId(form_id_raw),
            version_control,
            form_version,
            unknown,
        })
    }
}

/// How the raw data block is stored inside a [`Record`].
enum RecordData {
    /// Uncompressed data (either from the file directly, or already
    /// decompressed).
    Raw(Arc<[u8]>),
    /// Compressed data, with the expected decompressed size stored alongside.
    Compressed {
        data: Arc<[u8]>,
        decompressed_size: u32,
    },
}

/// A main record (any non-GRUP block) with lazily parsed subrecords.
///
/// Subrecords are not parsed until [`Record::subrecords`] is first called.
/// If the record was compressed, decompression also happens at that point.
/// Subsequent calls return the cached result immediately.
pub struct Record {
    /// Parsed record header.
    pub header: RecordHeader,
    /// Byte range of this record (header + data) within the plugin source,
    /// if the parser was given access to the source. Used by
    /// [`crate::PluginPatcher`] to copy unmodified records as raw bytes
    /// instead of re-serialising them.
    pub source_range: Option<std::ops::Range<usize>>,
    /// Raw or compressed data block.
    data: RecordData,
    /// Lazily populated subrecord list.
    parsed: OnceLock<Vec<SubRecord>>,
}

impl Record {
    /// Parses the record header from `cursor` and stores the raw data block.
    ///
    /// Subrecords are **not** parsed yet.
    ///
    /// # Arguments
    ///
    /// * `cursor` - Positioned immediately before the record signature.
    /// * `_ctx`   - Game context (reserved for future flag checks during
    ///   header parsing).
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::Io`] if the data is truncated, or
    /// [`CoreError::InvalidSignature`] if the header is a GRUP.
    pub fn parse_header(cursor: &mut SliceCursor<'_>, _ctx: &GameContext) -> Result<Self> {
        let header: RecordHeader = RecordHeader::parse(cursor)?;

        // Reject GRUP headers — those are handled by group.rs.
        if header.signature == Signature::GRUP {
            return Err(CoreError::InvalidSignature {
                expected: "non-GRUP".to_string(),
                got: header.signature.to_string(),
            });
        }

        let raw_data: &[u8] = cursor.read_slice(header.data_size as usize)?;

        let record_data: RecordData = if header.flags.contains(RecordFlags::COMPRESSED) {
            // First 4 bytes of the compressed block are the decompressed size.
            if raw_data.len() < 4 {
                return Err(CoreError::UnexpectedEof {
                    context: "compressed record decompressed_size field",
                });
            }
            let decompressed_size: u32 =
                u32::from_le_bytes([raw_data[0], raw_data[1], raw_data[2], raw_data[3]]);
            let compressed_payload: Arc<[u8]> = raw_data[4..].into();
            RecordData::Compressed {
                data: compressed_payload,
                decompressed_size,
            }
        } else {
            RecordData::Raw(raw_data.into())
        };

        Ok(Self {
            header,
            source_range: None,
            data: record_data,
            parsed: OnceLock::new(),
        })
    }

    /// Returns the parsed subrecord list, decompressing and parsing on first
    /// call.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError`] if decompression or subrecord parsing fails.
    pub fn subrecords(&self) -> Result<&[SubRecord]> {
        if let Some(cached) = self.parsed.get() {
            return Ok(cached.as_slice());
        }

        let data: Arc<[u8]> = match &self.data {
            RecordData::Raw(arc) => arc.clone(),
            RecordData::Compressed {
                data,
                decompressed_size,
            } => {
                let decompressed: Vec<u8> =
                    bethkit_io::decompress_zlib(data, *decompressed_size as usize)?;
                Arc::from(decompressed)
            }
        };

        let subrecords: Vec<SubRecord> = parse_subrecords(data)?;

        // If another thread raced us here, discard our result and use theirs.
        Ok(self.parsed.get_or_init(|| subrecords).as_slice())
    }

    /// Finds the first subrecord with `sig`, parsing subrecords if necessary.
    ///
    /// # Arguments
    ///
    /// * `sig` - The 4-byte signature to search for.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError`] if subrecord parsing fails.
    pub fn get(&self, sig: Signature) -> Result<Option<&SubRecord>> {
        Ok(self.subrecords()?.iter().find(|sr| sr.signature == sig))
    }

    /// Collects all subrecords with `sig`, parsing subrecords if necessary.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError`] if subrecord parsing fails.
    pub fn get_all(&self, sig: Signature) -> Result<Vec<&SubRecord>> {
        Ok(self
            .subrecords()?
            .iter()
            .filter(|sr| sr.signature == sig)
            .collect())
    }

    /// Returns the editor ID (EDID subrecord) if present.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError`] if subrecord parsing or string decoding fails.
    pub fn editor_id(&self) -> Result<Option<&str>> {
        match self.get(Signature::EDID)? {
            Some(sr) => Ok(Some(sr.as_zstring()?)),
            None => Ok(None),
        }
    }

    /// Derives the plugin kind from the record flags using `ctx`.
    ///
    /// Only meaningful when called on the `TES4` header record.
    ///
    /// # Arguments
    ///
    /// * `ctx` - Game context used to interpret flag bit positions.
    pub fn plugin_kind(&self, ctx: &GameContext) -> PluginKind {
        ctx.plugin_kind_from_flags(self.header.flags)
    }

    /// Returns the original source bytes of this record (header + data) if
    /// the parser captured a [`Self::source_range`] for it and `source` is
    /// large enough to contain the range.
    pub fn source_bytes<'a>(&self, source: &'a [u8]) -> Option<&'a [u8]> {
        let range: std::ops::Range<usize> = self.source_range.clone()?;
        source.get(range)
    }

    /// Returns the raw (uncompressed) record data block, **excluding** the
    /// 24-byte header.
    ///
    /// For compressed records this triggers a decompression on first call;
    /// the result is *not* cached. For uncompressed records this is a
    /// zero-copy slice into the parent storage.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError`] if zlib decompression fails.
    pub fn raw_data(&self) -> Result<std::borrow::Cow<'_, [u8]>> {
        match &self.data {
            RecordData::Raw(arc) => Ok(std::borrow::Cow::Borrowed(arc.as_ref())),
            RecordData::Compressed {
                data,
                decompressed_size,
            } => {
                let decompressed: Vec<u8> =
                    bethkit_io::decompress_zlib(data, *decompressed_size as usize)?;
                Ok(std::borrow::Cow::Owned(decompressed))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a minimal 24-byte record header + data block in memory.
    fn build_record_bytes(sig: &[u8; 4], flags: u32, form_id: u32, data: &[u8]) -> Vec<u8> {
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

    /// Builds a subrecord as raw bytes.
    fn build_subrecord(sig: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(sig);
        buf.extend_from_slice(&(data.len() as u16).to_le_bytes());
        buf.extend_from_slice(data);
        buf
    }

    /// Verifies that a simple uncompressed record parses its header correctly.
    #[test]
    fn parse_uncompressed_record_header() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let edid_sr = build_subrecord(b"EDID", b"TestRecord\0");
        let bytes = build_record_bytes(b"NPC_", 0, 0x0000_0001, &edid_sr);
        let ctx = GameContext::sse();
        let mut cursor = SliceCursor::new(&bytes);

        // when
        let record = Record::parse_header(&mut cursor, &ctx)?;

        // then
        assert_eq!(record.header.signature, Signature(*b"NPC_"));
        assert_eq!(record.header.form_id, FormId(0x0000_0001));
        Ok(())
    }

    /// Verifies that subrecords are parsed lazily and EDID is found.
    #[test]
    fn subrecords_parsed_lazily() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let edid_sr = build_subrecord(b"EDID", b"TestRecord\0");
        let bytes = build_record_bytes(b"NPC_", 0, 0x0000_0001, &edid_sr);
        let ctx = GameContext::sse();
        let mut cursor = SliceCursor::new(&bytes);
        let record = Record::parse_header(&mut cursor, &ctx)?;

        // when
        let editor_id = record.editor_id()?;

        // then
        assert_eq!(editor_id, Some("TestRecord"));
        Ok(())
    }

    /// Verifies that the XXXX large-field override is handled correctly.
    #[test]
    fn xxxx_override_uses_correct_size() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given — build a record whose data block contains a XXXX + a large SR
        let payload: Vec<u8> = vec![0xABu8; 70_000]; // larger than u16::MAX
        let real_size: u32 = payload.len() as u32;

        let mut record_data: Vec<u8> = Vec::new();
        // XXXX subrecord
        record_data.extend_from_slice(b"XXXX");
        record_data.extend_from_slice(&4u16.to_le_bytes());
        record_data.extend_from_slice(&real_size.to_le_bytes());
        // Large subrecord — size field is 0 (ignored due to XXXX)
        record_data.extend_from_slice(b"DATA");
        record_data.extend_from_slice(&0u16.to_le_bytes());
        record_data.extend_from_slice(&payload);

        let bytes = build_record_bytes(b"NPC_", 0, 1, &record_data);
        let ctx = GameContext::sse();
        let mut cursor = SliceCursor::new(&bytes);
        let record = Record::parse_header(&mut cursor, &ctx)?;

        // when
        let subrecords = record.subrecords()?;

        // then
        assert_eq!(subrecords.len(), 1);
        assert_eq!(subrecords[0].signature, Signature(*b"DATA"));
        assert_eq!(subrecords[0].as_bytes().len(), 70_000);
        Ok(())
    }
}
