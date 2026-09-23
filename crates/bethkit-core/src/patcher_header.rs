// SPDX-License-Identifier: Apache-2.0
//! Targeted plugin-header updates that retain untouched source subrecord frames.

use std::io::Write;

use crate::error::{CoreError, Result};
use crate::plugin::Plugin;
use crate::record::SubRecordData;
use crate::types::{RecordFlags, Signature};
use crate::writer::WritableSubRecord;

use super::{io_err, PluginHeaderPatch};

/// Writes changed header fields without reconstructing unrelated source metadata.
///
/// The original record header is copied, retaining unknown flag bits and revision
/// metadata. Unchanged compressed payloads are retained verbatim; changed compressed
/// payloads become uncompressed, like patched records.
///
/// # Errors
///
/// Returns a source-range, encoding, or I/O error if the original header cannot be
/// updated safely, a required HEDR field is absent, or the output cannot be written.
pub(super) fn write_updated(
    plugin: &Plugin,
    patch: Option<&PluginHeaderPatch>,
    writer: &mut impl Write,
) -> Result<()> {
    let source = plugin
        .source_bytes()
        .get(plugin.header_range.clone())
        .filter(|bytes| bytes.len() >= 24)
        .ok_or(CoreError::UnexpectedEof {
            context: "plugin header source range",
        })?;
    let mut payload = updated_payload(plugin, patch)?;
    let mut header = source[..24].to_vec();
    let mut flags = u32::from_le_bytes(header[8..12].try_into().expect("24-byte header"));
    if let Some(patch) = patch {
        flags |= patch.flags_set.map_or(0, |value| value.bits());
        flags &= !patch.flags_clear.map_or(0, |value| value.bits());
    }
    if flags & RecordFlags::COMPRESSED.bits() != 0
        && plugin
            .header
            .record
            .header
            .flags
            .contains(RecordFlags::COMPRESSED)
        && payload == plugin.header.record.raw_data()?.as_ref()
    {
        payload = source[24..].to_vec();
    } else {
        flags &= !RecordFlags::COMPRESSED.bits();
    }
    let payload_size = u32::try_from(payload.len())
        .map_err(|_| CoreError::InvalidEncoding("plugin header exceeds u32 size".to_owned()))?;
    header[4..8].copy_from_slice(&payload_size.to_le_bytes());
    header[8..12].copy_from_slice(&flags.to_le_bytes());
    writer.write_all(&header).map_err(io_err)?;
    writer.write_all(&payload).map_err(io_err)?;
    Ok(())
}

fn updated_payload(plugin: &Plugin, patch: Option<&PluginHeaderPatch>) -> Result<Vec<u8>> {
    let header = &plugin.header;
    let masters = patch.and_then(|value| value.masters.as_deref());
    let description = patch.and_then(|value| value.description.as_deref());
    let own_index = masters.unwrap_or(&header.masters).len();
    let mut record_count: u32 = 0;
    let mut next_id = header.next_object_id.0.max(0x801);
    let mut groups: Vec<_> = plugin.groups().iter().collect();
    while let Some(group) = groups.pop() {
        // xEdit counts each GRUP and main record, excluding only TES4 itself.
        record_count = record_count.checked_add(1).ok_or_else(|| {
            CoreError::InvalidEncoding("plugin record count exceeds u32".to_owned())
        })?;
        for record in group.records() {
            record_count = record_count.checked_add(1).ok_or_else(|| {
                CoreError::InvalidEncoding("plugin record count exceeds u32".to_owned())
            })?;
            if usize::from(record.header.form_id.file_index()) == own_index {
                next_id = next_id.max(record.header.form_id.object_id() + 1);
            }
        }
        groups.extend(group.subgroups());
    }

    let original = header.record.raw_data()?;
    let mut output = Vec::with_capacity(original.len());
    let mut offset = 0;
    let mut seen_hedr = false;
    let mut seen_masters = false;
    let mut seen_description = false;
    let mut follows_master = false;
    for subrecord in header.record.subrecords()? {
        let SubRecordData::Borrowed { start, end, .. } = &subrecord.data else {
            return Err(CoreError::InvalidEncoding(
                "plugin header subrecord has no original byte range".to_owned(),
            ));
        };
        let frame = original.get(offset..*end).ok_or(CoreError::UnexpectedEof {
            context: "plugin header subrecord frame",
        })?;
        let payload_offset = start.checked_sub(offset).ok_or(CoreError::UnexpectedEof {
            context: "plugin header subrecord payload",
        })?;
        offset = *end;
        let master_data = follows_master && subrecord.signature == Signature::DATA;
        follows_master = subrecord.signature == Signature::MAST;
        if subrecord.signature == Signature::HEDR && !seen_hedr {
            let mut updated = frame.to_vec();
            let counters = updated
                .get_mut(payload_offset + 4..payload_offset + 12)
                .ok_or_else(|| {
                    CoreError::InvalidEncoding("HEDR is shorter than 12 bytes".to_owned())
                })?;
            counters[..4].copy_from_slice(&record_count.to_le_bytes());
            counters[4..].copy_from_slice(&next_id.to_le_bytes());
            output.extend_from_slice(&updated);
            seen_hedr = true;
        } else if follows_master && masters.is_some() {
            if !seen_masters {
                append_masters(
                    &mut output,
                    masters.expect("replacement masters are present"),
                );
                seen_masters = true;
            }
        } else if master_data && masters.is_some() {
            continue;
        } else if subrecord.signature == Signature::SNAM
            && description.is_some()
            && !seen_description
        {
            append_string(
                &mut output,
                Signature::SNAM,
                description.expect("replacement is present"),
            );
            seen_description = true;
        } else {
            output.extend_from_slice(frame);
        }
    }
    if !seen_hedr {
        return Err(CoreError::InvalidEncoding(
            "plugin header has no HEDR subrecord".to_owned(),
        ));
    }
    if let Some(description) = description.filter(|_| !seen_description) {
        append_string(&mut output, Signature::SNAM, description);
    }
    if let Some(masters) = masters.filter(|_| !seen_masters) {
        append_masters(&mut output, masters);
    }
    output.extend_from_slice(&original[offset..]);
    Ok(output)
}

fn append_masters(output: &mut Vec<u8>, masters: &[String]) {
    for master in masters {
        append_string(output, Signature::MAST, master);
        WritableSubRecord {
            signature: Signature::DATA,
            data: vec![0; 8],
        }
        .write_to(output);
    }
}

fn append_string(output: &mut Vec<u8>, signature: Signature, value: &str) {
    let mut data = value.as_bytes().to_vec();
    data.push(0);
    WritableSubRecord { signature, data }.write_to(output);
}
