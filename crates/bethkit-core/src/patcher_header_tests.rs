// SPDX-License-Identifier: Apache-2.0
//! Regression coverage for source-preserving TES4 metadata edits.

use crate::record::Record;
use crate::test_helpers::{build_grup, build_hedr, build_record, build_subrecord};
use crate::types::{FormId, GameContext, RecordFlags, Signature};
use crate::Plugin;

use super::{PluginHeaderPatch, PluginPatcher, RecordPatch};

type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;

fn rich_header_payload(record_count: u32, next_id: u32) -> Vec<u8> {
    let mut hedr = build_hedr(1.71, record_count, next_id);
    hedr.extend_from_slice(b"future HEDR data");
    let mut data = build_subrecord(b"HEDR", &hedr);
    data.extend_from_slice(&build_subrecord(b"CNAM", b"Original author\0\0"));
    data.extend_from_slice(&build_subrecord(b"SNAM", b"Original description\0"));
    data.extend_from_slice(&build_subrecord(b"ONAM", &0x0100_1234_u32.to_le_bytes()));
    data.extend_from_slice(&build_subrecord(b"MAST", b"Skyrim.esm\0"));
    data.extend_from_slice(&build_subrecord(
        b"DATA",
        &0x1234_5678_90AB_CDEF_u64.to_le_bytes(),
    ));
    data.extend_from_slice(&build_subrecord(b"ONAM", &0x0100_5678_u32.to_le_bytes()));
    data.extend_from_slice(&build_subrecord(b"SNAM", b"Secondary description\0"));
    data.extend_from_slice(&build_subrecord(b"HEDR", b"Repeated opaque HEDR"));
    data.extend_from_slice(&build_subrecord(b"DATA", b"unrelated DATA"));
    // Legal XXXX framing need not be canonical, even for a small payload.
    data.extend_from_slice(&build_subrecord(b"XXXX", &8_u32.to_le_bytes()));
    data.extend_from_slice(b"XTRA");
    data.extend_from_slice(&321_u16.to_le_bytes());
    data.extend_from_slice(b"\0\xFFopaque");
    data
}

fn source_plugin(payload: &[u8]) -> Vec<u8> {
    let flags = RecordFlags::LOCALIZED.bits() | RecordFlags::LIGHT.bits() | 0x8000_0000;
    let mut header = build_record(b"TES4", flags, 0x1234, payload);
    header[16..20].copy_from_slice(&0x89AB_CDEF_u32.to_le_bytes());
    header[20..22].copy_from_slice(&44_u16.to_le_bytes());
    header[22..24].copy_from_slice(&0xCAFE_u16.to_le_bytes());
    let record = build_record(
        b"NPC_",
        0,
        0x0100_0900,
        &build_subrecord(b"FULL", b"Original\0"),
    );
    header.extend_from_slice(&build_grup(b"NPC_", 0, &record));
    header
}

fn replacement() -> RecordPatch {
    RecordPatch::RawBytes(build_record(
        b"NPC_",
        0,
        0x0100_0900,
        &build_subrecord(b"FULL", b"Longer replacement\0"),
    ))
}

fn subrecord_values(record: &Record) -> crate::Result<Vec<(Signature, Vec<u8>)>> {
    Ok(record
        .subrecords()?
        .iter()
        .map(|subrecord| (subrecord.signature, subrecord.as_bytes().to_vec()))
        .collect())
}

/// Preserves author, opaque/repeated frames, master DATA, and raw header metadata.
#[test]
fn replacing_record_changes_only_necessary_hedr_counters() -> TestResult {
    // given
    let source = source_plugin(&rich_header_payload(0, 0x800));
    let plugin = Plugin::from_bytes(&source, GameContext::sse())?;
    let mut expected = source[plugin.header_range.clone()].to_vec();
    expected[34..38].copy_from_slice(&2_u32.to_le_bytes());
    expected[38..42].copy_from_slice(&0x901_u32.to_le_bytes());

    // when
    let mut patcher = PluginPatcher::new(&plugin);
    patcher.replace_record(FormId(0x0100_0900), replacement());
    let mut output = Vec::new();
    patcher.write_to(&mut output)?;

    // then
    let reparsed = Plugin::from_bytes(&output, GameContext::sse())?;
    assert_eq!(&output[reparsed.header_range.clone()], expected);
    assert_eq!(reparsed.header.record_count, 2);
    assert_eq!(reparsed.header.next_object_id, FormId(0x901));
    assert_eq!(reparsed.header.masters, ["Skyrim.esm"]);
    assert_eq!(
        reparsed
            .find_record(FormId(0x0100_0900))
            .expect("replacement record")
            .get(Signature(*b"FULL"))?
            .expect("FULL subrecord")
            .as_bytes(),
        b"Longer replacement\0"
    );
    Ok(())
}

/// Does not lower a previously allocated next object ID during sparse replacement.
#[test]
fn valid_header_remains_byte_identical_after_record_replacement() -> TestResult {
    // given
    let source = source_plugin(&rich_header_payload(2, 0x7000));
    let plugin = Plugin::from_bytes(&source, GameContext::sse())?;

    // when
    let mut patcher = PluginPatcher::new(&plugin);
    patcher.replace_record(FormId(0x0100_0900), replacement());
    let mut output = Vec::new();
    patcher.write_to(&mut output)?;

    // then
    assert_eq!(
        &output[plugin.header_range.clone()],
        &source[plugin.header_range.clone()]
    );
    Ok(())
}

/// Applies requested flag changes without clearing unknown source flag bits.
#[test]
fn explicit_flags_patch_preserves_payload_and_other_header_fields() -> TestResult {
    // given
    let source = source_plugin(&rich_header_payload(2, 0x7000));
    let plugin = Plugin::from_bytes(&source, GameContext::sse())?;
    let mut expected = source.clone();
    let flags = RecordFlags::LOCALIZED.bits() | RecordFlags::ESM.bits() | 0x8000_0000;
    expected[8..12].copy_from_slice(&flags.to_le_bytes());

    // when
    let mut patcher = PluginPatcher::new(&plugin);
    patcher.patch_header(PluginHeaderPatch {
        masters: None,
        description: None,
        flags_set: Some(RecordFlags::ESM),
        flags_clear: Some(RecordFlags::LIGHT),
    });
    let mut output = Vec::new();
    patcher.write_to(&mut output)?;

    // then
    assert_eq!(output, expected);
    Ok(())
}

/// Replaces only the first description while preserving other metadata and order.
#[test]
fn description_patch_retains_repeated_and_unknown_subrecords() -> TestResult {
    for description in ["Updated description", ""] {
        // given
        let source = source_plugin(&rich_header_payload(2, 0x7000));
        let plugin = Plugin::from_bytes(&source, GameContext::sse())?;
        let mut expected = subrecord_values(&plugin.header.record)?;
        expected[2].1 = [description.as_bytes(), b"\0"].concat();

        // when
        let mut patcher = PluginPatcher::new(&plugin);
        patcher.patch_header(PluginHeaderPatch {
            masters: None,
            description: Some(description.to_owned()),
            flags_set: None,
            flags_clear: None,
        });
        let mut output = Vec::new();
        patcher.write_to(&mut output)?;

        // then
        let reparsed = Plugin::from_bytes(&output, GameContext::sse())?;
        assert_eq!(subrecord_values(&reparsed.header.record)?, expected);
        assert_eq!(
            reparsed.header.description.as_deref(),
            (!description.is_empty()).then_some(description)
        );
        assert_eq!(&output[8..24], &source[8..24]);
        assert!(output
            .windows(24)
            .any(|bytes| bytes == &source[plugin.header_range.end - 24..plugin.header_range.end]));
    }
    Ok(())
}

/// Replaces master pairs in place while retaining unrelated DATA and field order.
#[test]
fn master_patch_changes_only_master_pairs() -> TestResult {
    for masters in [
        vec![],
        vec!["Update.esm"],
        vec!["Update.esm", "Dawnguard.esm"],
    ] {
        // given
        let source = source_plugin(&rich_header_payload(2, 0x7000));
        let plugin = Plugin::from_bytes(&source, GameContext::sse())?;
        let mut expected = subrecord_values(&plugin.header.record)?;
        let new_pairs = masters.iter().flat_map(|master| {
            [
                (Signature::MAST, [master.as_bytes(), b"\0"].concat()),
                (Signature::DATA, vec![0; 8]),
            ]
        });
        expected.splice(4..6, new_pairs);

        // when
        let mut patcher = PluginPatcher::new(&plugin);
        patcher.patch_header(PluginHeaderPatch {
            masters: Some(masters.iter().map(|master| (*master).to_owned()).collect()),
            description: None,
            flags_set: None,
            flags_clear: None,
        });
        let mut output = Vec::new();
        patcher.write_to(&mut output)?;

        // then
        let reparsed = Plugin::from_bytes(&output, GameContext::sse())?;
        assert_eq!(subrecord_values(&reparsed.header.record)?, expected);
        assert_eq!(reparsed.header.masters, masters);
        assert_eq!(&output[8..24], &source[8..24]);
    }
    Ok(())
}

/// Fails before producing output rather than synthesizing missing header fields.
#[test]
fn malformed_hedr_is_not_silently_rebuilt() -> TestResult {
    for payload in [
        build_subrecord(b"CNAM", b"Author\0"),
        build_subrecord(b"HEDR", b"short"),
    ] {
        // given
        let source = source_plugin(&payload);
        let plugin = Plugin::from_bytes(&source, GameContext::sse())?;
        let mut patcher = PluginPatcher::new(&plugin);
        patcher.replace_record(FormId(0x0100_0900), replacement());
        let mut output = Vec::new();

        // when
        let result = patcher.write_to(&mut output);

        // then
        assert!(matches!(result, Err(crate::CoreError::InvalidEncoding(_))));
        assert!(output.is_empty());
    }
    Ok(())
}

fn stored_zlib(payload: &[u8]) -> Vec<u8> {
    let length = u16::try_from(payload.len()).expect("small test fixture");
    let mut result = vec![0x78, 0x01, 0x01];
    result.extend_from_slice(&length.to_le_bytes());
    result.extend_from_slice(&(!length).to_le_bytes());
    result.extend_from_slice(payload);
    let (mut a, mut b) = (1_u32, 0_u32);
    for byte in payload {
        a = (a + u32::from(*byte)) % 65521;
        b = (b + a) % 65521;
    }
    result.extend_from_slice(&((b << 16) | a).to_be_bytes());
    result
}

/// Preserves valid compressed headers and safely normalizes changed header data.
#[test]
fn compressed_header_preserves_metadata_without_invalid_flags() -> TestResult {
    for count in [0_u32, 2] {
        // given
        let payload = rich_header_payload(count, 0x7000);
        let mut compressed = (payload.len() as u32).to_le_bytes().to_vec();
        compressed.extend_from_slice(&stored_zlib(&payload));
        let mut source = source_plugin(&compressed);
        let original_flags = RecordFlags::LOCALIZED.bits()
            | RecordFlags::LIGHT.bits()
            | RecordFlags::COMPRESSED.bits()
            | 0x8000_0000;
        source[8..12].copy_from_slice(&original_flags.to_le_bytes());
        let plugin = Plugin::from_bytes(&source, GameContext::sse())?;
        let mut expected_payload = payload;
        expected_payload[10..14].copy_from_slice(&2_u32.to_le_bytes());

        // when
        let mut patcher = PluginPatcher::new(&plugin);
        patcher.replace_record(FormId(0x0100_0900), replacement());
        let mut output = Vec::new();
        patcher.write_to(&mut output)?;

        // then
        let reparsed = Plugin::from_bytes(&output, GameContext::sse())?;
        assert_eq!(
            reparsed.header.record.raw_data()?.as_ref(),
            expected_payload
        );
        assert_eq!(&output[12..24], &source[12..24]);
        if count == 2 {
            assert_eq!(
                &output[reparsed.header_range.clone()],
                &source[plugin.header_range.clone()]
            );
        } else {
            let flags = original_flags & !RecordFlags::COMPRESSED.bits();
            assert_eq!(&output[8..12], &flags.to_le_bytes());
        }
    }
    Ok(())
}

/// Counts all nested groups and their records in the same way as xEdit.
#[test]
fn hedr_counter_includes_nested_groups() -> TestResult {
    // given
    let header_payload = build_subrecord(b"HEDR", &build_hedr(1.71, 0, 0x800));
    let mut source = build_record(b"TES4", 0, 0, &header_payload);
    let first = build_record(b"NPC_", 0, 1, &[]);
    let second = build_record(b"NPC_", 0, 2, &[]);
    let inner = build_grup(&1_u32.to_le_bytes(), 6, &second);
    let children = [first, inner].concat();
    let outer = build_grup(b"NPC_", 0, &children);
    source.extend_from_slice(&outer);
    let plugin = Plugin::from_bytes(&source, GameContext::sse())?;

    // when
    let mut patcher = PluginPatcher::new(&plugin);
    patcher.replace_record(
        FormId(2),
        RecordPatch::RawBytes(build_record(b"NPC_", 0, 2, &[])),
    );
    let mut output = Vec::new();
    patcher.write_to(&mut output)?;

    // then
    let reparsed = Plugin::from_bytes(&output, GameContext::sse())?;
    assert_eq!(reparsed.header.record_count, 4);
    assert_eq!(
        &output[reparsed.header_range.end..],
        &source[plugin.header_range.end..]
    );
    Ok(())
}
