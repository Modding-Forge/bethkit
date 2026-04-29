// SPDX-License-Identifier: Apache-2.0
//!
//! End-to-end integration test for the localized-string workflow:
//! build a synthetic localized plugin in memory together with matching
//! `.STRINGS` / `.DLSTRINGS` / `.ILSTRINGS` tables, extract every
//! translatable string, edit a few entries, write the tables to a temp
//! directory, re-read everything and verify the round-trip.

use std::path::PathBuf;

use bethkit_core::{
    apply_edits, extract_strings, GameContext, LocalizationSet, Plugin, PluginWriter, RecordFlags,
    Signature, StringFileKind, StringTable, WritableGroup, WritableGroupChild, WritableRecord,
    WritableSubRecord,
};

/// Builds a minimal, valid synthetic plugin containing one BOOK record
/// (FULL + DESC + CNAM, all u32 LString IDs) and one NPC_ record
/// (FULL + SHRT, all u32 LString IDs).
///
/// Returns the plugin bytes and a populated [`LocalizationSet`].
fn build_synthetic_localized_plugin() -> (Vec<u8>, LocalizationSet) {
    let mut writer = PluginWriter::new(GameContext::sse(), 1.71);
    writer.set_localized(true);

    let book = WritableRecord {
        signature: Signature(*b"BOOK"),
        flags: RecordFlags::empty(),
        form_id: bethkit_core::FormId(0x01),
        form_version: 44,
        subrecords: vec![
            WritableSubRecord {
                signature: Signature(*b"EDID"),
                data: b"TestBook\0".to_vec(),
            },
            WritableSubRecord {
                signature: Signature(*b"FULL"),
                data: 1u32.to_le_bytes().to_vec(),
            },
            WritableSubRecord {
                signature: Signature(*b"DESC"),
                data: 2u32.to_le_bytes().to_vec(),
            },
            WritableSubRecord {
                signature: Signature(*b"CNAM"),
                data: 3u32.to_le_bytes().to_vec(),
            },
        ],
    };
    let book_grup = WritableGroup {
        label: *b"BOOK",
        group_type: 0,
        children: vec![WritableGroupChild::Record(book)],
    };
    writer.add_group(book_grup);

    let npc = WritableRecord {
        signature: Signature(*b"NPC_"),
        flags: RecordFlags::empty(),
        form_id: bethkit_core::FormId(0x02),
        form_version: 44,
        subrecords: vec![
            WritableSubRecord {
                signature: Signature(*b"EDID"),
                data: b"TestNpc\0".to_vec(),
            },
            WritableSubRecord {
                signature: Signature(*b"FULL"),
                data: 10u32.to_le_bytes().to_vec(),
            },
            WritableSubRecord {
                signature: Signature(*b"SHRT"),
                data: 11u32.to_le_bytes().to_vec(),
            },
        ],
    };
    let npc_grup = WritableGroup {
        label: *b"NPC_",
        group_type: 0,
        children: vec![WritableGroupChild::Record(npc)],
    };
    writer.add_group(npc_grup);

    let plugin_bytes: Vec<u8> = writer
        .write_to_vec()
        .expect("build_synthetic_localized_plugin: plugin write must succeed");

    let mut set = LocalizationSet::new();
    set.strings.insert(1, b"Book Title".to_vec()); // BOOK FULL
    set.strings.insert(10, b"Generic NPC".to_vec()); // NPC_ FULL
    set.strings.insert(11, b"Bandit".to_vec()); // NPC_ SHRT
    set.dlstrings
        .insert(2, b"A long book description.".to_vec()); // BOOK DESC
    set.dlstrings
        .insert(3, b"Author note: testing 1-2-3.".to_vec()); // BOOK CNAM

    (plugin_bytes, set)
}

/// Verifies that every translatable string in a localized synthetic plugin
#[test]
/// is extracted and resolved against the matching string tables.
fn extract_strings_resolves_all_subrecords() -> Result<(), Box<dyn std::error::Error>> {
    // given
    let (plugin_bytes, set) = build_synthetic_localized_plugin();
    let plugin = Plugin::from_bytes(&plugin_bytes, GameContext::sse())?;
    assert!(plugin.is_localized());

    // when
    let mut found = extract_strings(&plugin, &set, true)?;
    found.sort_by_key(|ls| (ls.form_id.0, ls.subrecord_type.0));

    // then
    assert_eq!(found.len(), 5);

    // BOOK 0x01
    let book_full = found
        .iter()
        .find(|s| s.form_id.0 == 1 && s.subrecord_type.0 == *b"FULL")
        .ok_or("book FULL not found")?;
    assert_eq!(book_full.kind, StringFileKind::Strings);
    assert_eq!(book_full.string_id, 1);
    assert_eq!(book_full.bytes, b"Book Title");

    let book_desc = found
        .iter()
        .find(|s| s.form_id.0 == 1 && s.subrecord_type.0 == *b"DESC")
        .ok_or("book DESC not found")?;
    assert_eq!(book_desc.kind, StringFileKind::DLStrings);
    assert_eq!(book_desc.bytes, b"A long book description.");

    let book_cnam = found
        .iter()
        .find(|s| s.form_id.0 == 1 && s.subrecord_type.0 == *b"CNAM")
        .ok_or("book CNAM not found")?;
    assert_eq!(book_cnam.kind, StringFileKind::DLStrings);
    assert_eq!(book_cnam.bytes, b"Author note: testing 1-2-3.");

    // NPC_ 0x02
    let npc_full = found
        .iter()
        .find(|s| s.form_id.0 == 2 && s.subrecord_type.0 == *b"FULL")
        .ok_or("NPC_ FULL not found")?;
    assert_eq!(npc_full.kind, StringFileKind::Strings);
    assert_eq!(npc_full.bytes, b"Generic NPC");

    let npc_shrt = found
        .iter()
        .find(|s| s.form_id.0 == 2 && s.subrecord_type.0 == *b"SHRT")
        .ok_or("NPC_ SHRT not found")?;
    assert_eq!(npc_shrt.kind, StringFileKind::Strings);
    assert_eq!(npc_shrt.bytes, b"Bandit");

    Ok(())
}

/// Verifies the full translation workflow: build → write tables to disk →
#[test]
/// re-open via `LocalizationSet::open` → extract again → bytes match the
/// edits.
fn full_localized_roundtrip_via_disk() -> Result<(), Box<dyn std::error::Error>> {
    // given
    let (plugin_bytes, mut set) = build_synthetic_localized_plugin();

    // Apply translation edits via `apply_edits`.
    apply_edits(
        &mut set,
        [
            (StringFileKind::Strings, 1u32, b"Buchtitel".to_vec()),
            (
                StringFileKind::DLStrings,
                2u32,
                "Eine lange Buchbeschreibung.".as_bytes().to_vec(),
            ),
            (StringFileKind::Strings, 11u32, b"Bandit (DE)".to_vec()),
        ],
    );

    // Set up a temp plugin path.
    let tmp_dir: PathBuf = std::env::temp_dir().join("bethkit-localized-roundtrip");
    let _ = std::fs::remove_dir_all(&tmp_dir);
    std::fs::create_dir_all(&tmp_dir)?;
    let plugin_path: PathBuf = tmp_dir.join("Synthetic.esp");
    std::fs::write(&plugin_path, &plugin_bytes)?;

    // when — write tables next to the plugin and reopen.
    set.write(&plugin_path, "english")?;

    let reopened_set = LocalizationSet::open(&plugin_path, "english")?;
    let plugin = Plugin::open(&plugin_path, GameContext::sse())?;
    let mut extracted = extract_strings(&plugin, &reopened_set, true)?;
    extracted.sort_by_key(|ls| (ls.form_id.0, ls.subrecord_type.0));

    // then — edited strings have new bytes, untouched strings unchanged.
    let book_full = extracted.iter().find(|s| s.string_id == 1).ok_or("book_full not found")?;
    assert_eq!(book_full.bytes, b"Buchtitel");

    let book_desc = extracted.iter().find(|s| s.string_id == 2).ok_or("book_desc not found")?;
    assert_eq!(book_desc.bytes, "Eine lange Buchbeschreibung.".as_bytes());

    let npc_shrt = extracted.iter().find(|s| s.string_id == 11).ok_or("npc_shrt not found")?;
    assert_eq!(npc_shrt.bytes, b"Bandit (DE)");

    let npc_full = extracted.iter().find(|s| s.string_id == 10).ok_or("npc_full not found")?;
    assert_eq!(npc_full.bytes, b"Generic NPC"); // not edited

    // Verify file kinds round-trip too.
    assert_eq!(reopened_set.strings.kind(), StringFileKind::Strings);
    assert_eq!(reopened_set.dlstrings.kind(), StringFileKind::DLStrings);
    assert_eq!(reopened_set.ilstrings.kind(), StringFileKind::ILStrings);

    // Verify per-table bytes survive a write-then-read cycle independently.
    let mut dl_buf: Vec<u8> = Vec::new();
    set.dlstrings.write_to(&mut dl_buf)?;
    let parsed_dl = StringTable::from_bytes(&dl_buf, StringFileKind::DLStrings)?;
    assert_eq!(
        parsed_dl.get(2),
        Some("Eine lange Buchbeschreibung.".as_bytes())
    );

    // Cleanup.
    let _ = std::fs::remove_dir_all(&tmp_dir);
    Ok(())
}

/// Verifies that a plugin without the `LOCALIZED` flag returns no strings
#[test]
/// when `require_localized` is false, and errors when true.
fn extract_strings_respects_require_localized_flag() -> Result<(), Box<dyn std::error::Error>> {
    // given
    let mut writer = PluginWriter::new(GameContext::sse(), 1.71);
    // Note: no set_localized(true)
    let book = WritableRecord {
        signature: Signature(*b"BOOK"),
        flags: RecordFlags::empty(),
        form_id: bethkit_core::FormId(0x01),
        form_version: 44,
        subrecords: vec![WritableSubRecord {
            signature: Signature(*b"FULL"),
            data: 1u32.to_le_bytes().to_vec(),
        }],
    };
    writer.add_group(WritableGroup {
        label: *b"BOOK",
        group_type: 0,
        children: vec![WritableGroupChild::Record(book)],
    });
    let plugin_bytes = writer.write_to_vec()?;
    let plugin = Plugin::from_bytes(&plugin_bytes, GameContext::sse())?;
    assert!(!plugin.is_localized());

    let set = LocalizationSet::new();

    // when / then
    let lenient = extract_strings(&plugin, &set, false)?;
    assert_eq!(lenient.len(), 0);

    let strict = extract_strings(&plugin, &set, true);
    assert!(strict.is_err());

    Ok(())
}
