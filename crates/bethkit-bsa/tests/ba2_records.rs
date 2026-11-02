// SPDX-License-Identifier: Apache-2.0
//! BA2 file record layout against real Fallout 4 data.
//!
//! The live part reads `FO4_DATA_DIR` (default
//! `E:\SteamLibrary\steamapps\common\Fallout 4\Data`) and is skipped when the
//! folder does not exist.

use std::path::PathBuf;

use bethkit_bsa::hash::hash_fo4;
use bethkit_bsa::{Ba2GnrlWriter, Ba2Version};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const DEFAULT_FO4: &str = r"E:\SteamLibrary\steamapps\common\Fallout 4\Data";

/// Size of the BA2 header before the file records.
const HEADER_SIZE: usize = 24;
/// Size of one GNRL file record.
const RECORD_SIZE: usize = 36;

fn u32_at(bytes: &[u8], offset: usize) -> Result<u32, Box<dyn std::error::Error>> {
    Ok(u32::from_le_bytes(bytes[offset..offset + 4].try_into()?))
}

/// Splits a normalised archive path into the hashed directory and stem and the
/// raw extension field, the way BA2 records store them.
fn record_fields(path: &str) -> (u32, [u8; 4], u32) {
    let (dir, file) = path.rsplit_once('/').unwrap_or(("", path));
    let (stem, ext) = file.rsplit_once('.').unwrap_or((file, ""));
    let mut ext_field = [0u8; 4];
    for (slot, byte) in ext_field.iter_mut().zip(ext.bytes()) {
        *slot = byte;
    }
    (hash_fo4(stem), ext_field, hash_fo4(dir))
}

/// Reads an archive whose single record uses the vanilla layout byte for byte.
#[test]
fn reads_vanilla_record_layout() -> TestResult {
    // given: record 0 of vanilla `Fallout4 - Misc.ba2`, pointing at an uncompressed payload
    let payload: &[u8] = b"hello";
    let name: &[u8] = b"Misc\\Test.txt";
    let data_offset: u64 = (HEADER_SIZE + RECORD_SIZE) as u64;
    let table_offset: u64 = data_offset + payload.len() as u64;
    let mut bytes: Vec<u8> = Vec::new();
    bytes.extend_from_slice(b"BTDX");
    bytes.extend_from_slice(&8u32.to_le_bytes());
    bytes.extend_from_slice(b"GNRL");
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&table_offset.to_le_bytes());
    bytes.extend_from_slice(&0x1cc5_d776u32.to_le_bytes());
    bytes.extend_from_slice(b"txt\0");
    bytes.extend_from_slice(&0x4072_4840u32.to_le_bytes());
    bytes.extend_from_slice(&0x0010_0100u32.to_le_bytes());
    bytes.extend_from_slice(&data_offset.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&0xBAAD_F00Du32.to_le_bytes());
    bytes.extend_from_slice(payload);
    bytes.extend_from_slice(&(name.len() as u16).to_le_bytes());
    bytes.extend_from_slice(name);
    let dir = tempfile::tempdir()?;
    let path: PathBuf = dir.path().join("vanilla.ba2");
    std::fs::write(&path, &bytes)?;

    // when
    let archive = bethkit_bsa::open(&path)?;
    let extracted = archive.extract("misc/test.txt").ok_or("entry missing")??;

    // then
    assert_eq!(archive.entries()[0].path, "misc/test.txt");
    assert_eq!(&*extracted, payload);
    Ok(())
}

/// Verifies every field the writer stores in a GNRL record.
#[test]
fn writes_vanilla_record_layout() -> TestResult {
    // given
    let dir = tempfile::tempdir()?;
    let path: PathBuf = dir.path().join("written.ba2");
    let mut writer = Ba2GnrlWriter::new(Ba2Version::V8).compress(false);
    writer.add("Scripts/MyQuest.PEX", b"script".to_vec());

    // when
    writer.write_to(&path)?;
    let bytes: Vec<u8> = std::fs::read(&path)?;

    // then
    let (name_hash, ext, dir_hash) = record_fields("scripts/myquest.pex");
    let record: usize = HEADER_SIZE;
    assert_eq!(u32_at(&bytes, record)?, name_hash);
    assert_eq!(&bytes[record + 4..record + 8], &ext);
    assert_eq!(&ext, b"pex\0");
    assert_eq!(u32_at(&bytes, record + 8)?, dir_hash);
    assert_eq!(u32_at(&bytes, record + 12)?, 0x0010_0100);
    assert_eq!(u32_at(&bytes, record + 32)?, 0xBAAD_F00D);
    let archive = bethkit_bsa::open(&path)?;
    let extracted = archive
        .extract("scripts/myquest.pex")
        .ok_or("entry missing")??;
    assert_eq!(&*extracted, b"script");
    Ok(())
}

/// Reads the first `len` bytes of a file.
fn read_prefix(path: &std::path::Path, len: usize) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    use std::io::Read;

    let mut bytes: Vec<u8> = Vec::with_capacity(len);
    std::fs::File::open(path)?
        .take(len as u64)
        .read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// Entries extracted per archive; record fields are checked for all entries.
const EXTRACT_PER_ARCHIVE: usize = 500;

/// Opens every Fallout 4 archive, checks that the writer's record fields match
/// every vanilla GNRL record, and extracts the first entries of each archive.
#[test]
fn reads_and_matches_fallout4_archives() -> TestResult {
    let data: PathBuf =
        std::env::var_os("FO4_DATA_DIR").map_or_else(|| DEFAULT_FO4.into(), PathBuf::from);
    if !data.is_dir() {
        println!("SKIP: {} not found", data.display());
        return Ok(());
    }

    // given
    let mut archives: Vec<PathBuf> = std::fs::read_dir(&data)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("ba2"))
        })
        .collect();
    archives.sort();

    // when / then
    let mut records: usize = 0;
    let mut extracted_entries: usize = 0;
    for path in &archives {
        let archive =
            bethkit_bsa::open(path).map_err(|error| format!("{}: {error}", path.display()))?;
        let table_len: usize = HEADER_SIZE + archive.entries().len() * RECORD_SIZE;
        let bytes: Vec<u8> = read_prefix(path, table_len)?;
        if &bytes[8..12] != b"GNRL" {
            continue;
        }
        for (index, entry) in archive.entries().iter().enumerate() {
            let record: usize = HEADER_SIZE + index * RECORD_SIZE;
            let (name_hash, ext, dir_hash) = record_fields(&entry.path);
            assert_eq!(u32_at(&bytes, record)?, name_hash, "{}", entry.path);
            assert_eq!(&bytes[record + 4..record + 8], &ext, "{}", entry.path);
            assert_eq!(u32_at(&bytes, record + 8)?, dir_hash, "{}", entry.path);
            records += 1;
            if index >= EXTRACT_PER_ARCHIVE {
                continue;
            }
            let extracted = archive
                .extract(&entry.path)
                .ok_or_else(|| format!("{}: missing", entry.path))?
                .map_err(|error| format!("{}: {error}", entry.path))?;
            assert_eq!(
                extracted.len() as u32,
                entry.uncompressed_size,
                "{}",
                entry.path
            );
            extracted_entries += 1;
        }
    }
    println!(
        "{} archives, {records} GNRL records checked, {extracted_entries} entries extracted",
        archives.len()
    );
    assert!(records > 0);
    Ok(())
}
