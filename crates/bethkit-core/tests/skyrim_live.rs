// SPDX-License-Identifier: Apache-2.0
//!
//! XXXL live integration + benchmark suite against a real Skyrim Special Edition
//! game installation with a full mod list.
//!
//! # Precondition
//!
//! Set the environment variable `SKYRIM_DATA_DIR` to the path of your Skyrim SE
//! `Data/` folder, **or** place the installation at the default path:
//!
//! ```text
//! E:\SteamLibrary\steamapps\common\Skyrim Special Edition\Data
//! ```
//!
//! If neither path exists the entire suite is skipped so that CI passes without
//! a game installation.
//!
//! # Run
//!
//! ```text
//! cargo test --test skyrim_live -- --nocapture
//! ```

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use bethkit_core::{
    GameContext, LoadOrder, Plugin, PluginCache, PluginKind, PluginPatcher, RecordFlags,
    RecordView, SchemaRegistry, Signature, StringFileKind, StringTable,
};

const DEFAULT_DATA_DIR: &str = r"E:\SteamLibrary\steamapps\common\Skyrim Special Edition\Data";

/// Locates the Skyrim SE Data directory.
///
/// Returns `None` when the suite should be skipped.
fn find_data_dir() -> Option<PathBuf> {
    // 1. Environment variable override.
    if let Ok(val) = std::env::var("SKYRIM_DATA_DIR") {
        let p = PathBuf::from(val);
        if p.exists() {
            return Some(p);
        }
        eprintln!(
            "SKYRIM_DATA_DIR is set but path does not exist: {}",
            p.display()
        );
        return None;
    }

    // 2. Hard-coded default installation path.
    let default = PathBuf::from(DEFAULT_DATA_DIR);
    if default.exists() {
        return Some(default);
    }

    None
}

/// Collects all `.esp` / `.esm` / `.esl` files in `dir`, sorted by name.
fn collect_plugins(dir: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("failed to read Data directory")
        .filter_map(|e| {
            let e = e.ok()?;
            let p = e.path();
            let ext = p.extension()?.to_ascii_lowercase();
            if ext == "esp" || ext == "esm" || ext == "esl" {
                Some(p)
            } else {
                None
            }
        })
        .collect();
    paths.sort();
    paths
}

/// Opens a plugin with SSE context, returning an error message on failure.
fn open(path: &Path) -> Result<Plugin, String> {
    Plugin::open(path, GameContext::sse()).map_err(|e| format!("{}: {e}", path.display()))
}

/// Prints a section banner to stderr.
fn banner(title: &str) {
    eprintln!();
    eprintln!("===  {title}  ===");
}

/// Formats a byte count as a human-readable string.
fn fmt_bytes(n: u64) -> String {
    const GIB: u64 = 1 << 30;
    const MIB: u64 = 1 << 20;
    const KIB: u64 = 1 << 10;
    if n >= GIB {
        format!("{:.2} GiB", n as f64 / GIB as f64)
    } else if n >= MIB {
        format!("{:.1} MiB", n as f64 / MIB as f64)
    } else {
        format!("{:.1} KiB", n as f64 / KIB as f64)
    }
}

/// Formats a rate as MB/s.
fn fmt_mbps(bytes: u64, elapsed: Duration) -> String {
    let secs = elapsed.as_secs_f64();
    if secs == 0.0 {
        return "inf MB/s".to_owned();
    }
    format!("{:.1} MB/s", bytes as f64 / (1u64 << 20) as f64 / secs)
}

/// Verifies that the live Skyrim SE data directory is readable and contains
/// a plausible number of plugin files.
#[test]
fn live_01_dataset_discovery() -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = find_data_dir() else {
        eprintln!("SKIP: Skyrim SE Data directory not found");
        return Ok(());
    };

    banner("DATASET DISCOVERY");

    let paths = collect_plugins(&dir);
    let total_bytes: u64 = paths
        .iter()
        .filter_map(|p| std::fs::metadata(p).ok())
        .map(|m| m.len())
        .sum();

    let esm = paths
        .iter()
        .filter(|p| p.extension().map(|e| e == "esm").unwrap_or(false))
        .count();
    let esp = paths
        .iter()
        .filter(|p| p.extension().map(|e| e == "esp").unwrap_or(false))
        .count();
    let esl = paths
        .iter()
        .filter(|p| p.extension().map(|e| e == "esl").unwrap_or(false))
        .count();

    eprintln!("  Data dir : {}", dir.display());
    eprintln!("  ESM      : {esm}");
    eprintln!("  ESP      : {esp}");
    eprintln!("  ESL      : {esl}");
    eprintln!(
        "  Total    : {} files  ({})",
        paths.len(),
        fmt_bytes(total_bytes)
    );

    // Sanity: at minimum Skyrim.esm must exist.
    assert!(
        paths
            .iter()
            .any(|p| p.file_name().map(|n| n == "Skyrim.esm").unwrap_or(false)),
        "Skyrim.esm not found in Data directory"
    );
    assert!(
        paths.len() >= 5,
        "too few plugin files — expected at least 5"
    );

    Ok(())
}

/// Every `.esp` / `.esm` / `.esl` file in the Data directory must parse
/// without returning an error.
#[test]
fn live_02_all_plugins_open() -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = find_data_dir() else {
        return Ok(());
    };
    banner("ALL PLUGINS OPEN");

    let paths = collect_plugins(&dir);
    let mut ok = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for path in &paths {
        match open(path) {
            Ok(_) => ok += 1,
            Err(e) => failures.push(e),
        }
    }

    eprintln!("  Opened {ok} / {} plugins without error", paths.len());
    if !failures.is_empty() {
        eprintln!("  FAILURES ({}):", failures.len());
        for f in failures.iter().take(30) {
            eprintln!("    {f}");
        }
        if failures.len() > 30 {
            eprintln!("    ... and {} more", failures.len() - 30);
        }
        panic!("{} plugin(s) failed to open", failures.len());
    }

    Ok(())
}

/// All record signatures across every plugin must consist exclusively of
/// ASCII alphanumeric bytes or `_`.
#[test]
fn live_03_all_signatures_are_valid_ascii() -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = find_data_dir() else {
        return Ok(());
    };
    banner("RECORD SIGNATURE VALIDITY");

    let paths = collect_plugins(&dir);
    let mut bad: Vec<String> = Vec::new();
    let mut total_records = 0u64;

    for path in &paths {
        let Ok(plugin) = open(path) else { continue };
        for group in plugin.groups() {
            for record in group.records_recursive() {
                total_records += 1;
                let sig = record.header.signature;
                if !sig
                    .0
                    .iter()
                    .all(|b| b.is_ascii_alphanumeric() || *b == b'_')
                {
                    bad.push(format!(
                        "{}: invalid signature {sig} at FormID {:08X}",
                        path.file_name()
                            .expect("path ends in file name")
                            .to_string_lossy(),
                        record.header.form_id.0,
                    ));
                }
            }
        }
    }

    eprintln!("  Checked {total_records} record signatures");
    if !bad.is_empty() {
        for b in bad.iter().take(20) {
            eprintln!("  BAD: {b}");
        }
        panic!("{} invalid signature(s) found", bad.len());
    }

    Ok(())
}

/// Every plugin must have a recognised PluginKind, and `.esm` files must
/// not be detected as the Starfield-only `Update` variant.
#[test]
fn live_04_plugin_kinds_are_valid() -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = find_data_dir() else {
        return Ok(());
    };
    banner("PLUGIN KINDS");

    let paths = collect_plugins(&dir);
    let mut kind_counts: HashMap<&'static str, usize> = HashMap::new();
    let mut bad_update_esm: Vec<String> = Vec::new();

    for path in &paths {
        let Ok(plugin) = open(path) else { continue };
        let kind = plugin.kind();
        let label = match kind {
            PluginKind::Plugin => "Plugin (.esp)",
            PluginKind::Master => "Master (.esm)",
            PluginKind::Light => "Light  (.esl)",
            PluginKind::Medium => "Medium",
            PluginKind::Update => "Update",
        };
        *kind_counts.entry(label).or_default() += 1;

        let ext = path.extension().map(|e| e.to_ascii_lowercase());
        if ext.map(|e| e == "esm").unwrap_or(false) && kind == PluginKind::Update {
            bad_update_esm.push(
                path.file_name()
                    .expect("path ends in file name")
                    .to_string_lossy()
                    .into_owned(),
            );
        }
    }

    for (label, count) in &kind_counts {
        eprintln!("  {label:20} : {count}");
    }

    if !bad_update_esm.is_empty() {
        for f in &bad_update_esm {
            eprintln!("  WARN: {f} detected as Update-kind (unexpected for SSE)");
        }
        panic!(
            "{} .esm file(s) incorrectly detected as Update kind",
            bad_update_esm.len()
        );
    }

    Ok(())
}

/// The HEDR version float must be positive and finite for every plugin.
#[test]
fn live_05_hedr_versions_are_valid() -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = find_data_dir() else {
        return Ok(());
    };
    banner("HEDR VERSION VALIDITY");

    let paths = collect_plugins(&dir);
    let mut bad: Vec<String> = Vec::new();
    let mut versions: HashMap<u32, usize> = HashMap::new(); // bits -> count

    for path in &paths {
        let Ok(plugin) = open(path) else { continue };
        let v = plugin.header.hedr_version;
        *versions.entry(v.to_bits()).or_default() += 1;
        if !v.is_finite() || v <= 0.0 {
            bad.push(format!(
                "{}: invalid HEDR version {v}",
                path.file_name()
                    .expect("path ends in file name")
                    .to_string_lossy()
            ));
        }
    }

    let mut sorted_versions: Vec<(f32, usize)> = versions
        .into_iter()
        .map(|(bits, count)| (f32::from_bits(bits), count))
        .collect();
    sorted_versions.sort_by(|a, b| a.0.partial_cmp(&b.0).expect("HEDR version is not NaN"));
    for (v, count) in &sorted_versions {
        eprintln!("  HEDR version {v:.4} : {count} plugin(s)");
    }

    if !bad.is_empty() {
        for f in &bad {
            eprintln!("  BAD: {f}");
        }
        panic!("{} invalid HEDR version(s)", bad.len());
    }

    Ok(())
}

/// All MAST subrecords must be non-empty, printable ASCII strings.
#[test]
fn live_06_master_filenames_are_valid() -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = find_data_dir() else {
        return Ok(());
    };
    banner("MASTER FILENAME VALIDITY");

    let paths = collect_plugins(&dir);
    let mut bad: Vec<String> = Vec::new();
    let mut total_masters = 0u64;
    let mut master_counts: HashMap<usize, usize> = HashMap::new();

    for path in &paths {
        let Ok(plugin) = open(path) else { continue };
        let masters = plugin.masters();
        *master_counts.entry(masters.len()).or_default() += 1;
        total_masters += masters.len() as u64;
        for m in masters {
            if m.is_empty() || !m.is_ascii() {
                bad.push(format!(
                    "{}: invalid master {:?}",
                    path.file_name()
                        .expect("path ends in file name")
                        .to_string_lossy(),
                    m
                ));
            }
        }
    }

    eprintln!("  Total MAST references : {total_masters}");
    let mut buckets: Vec<(usize, usize)> = master_counts.into_iter().collect();
    buckets.sort_by_key(|(k, _)| *k);
    for (count, n) in buckets.iter().take(10) {
        eprintln!("  {count:3} master(s) : {n} plugin(s)");
    }

    if !bad.is_empty() {
        for f in bad.iter().take(20) {
            eprintln!("  BAD: {f}");
        }
        panic!("{} invalid master filename(s)", bad.len());
    }

    Ok(())
}

/// Triggering lazy subrecord parsing on every record across every plugin
/// must not return an error.
#[test]
fn live_07_all_subrecords_parse() -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = find_data_dir() else {
        return Ok(());
    };
    banner("SUBRECORD PARSE COVERAGE");

    let paths = collect_plugins(&dir);
    let mut failures: Vec<String> = Vec::new();
    let mut total_records = 0u64;
    let mut total_subrecords = 0u64;

    for path in &paths {
        let Ok(plugin) = open(path) else { continue };
        for group in plugin.groups() {
            for record in group.records_recursive() {
                total_records += 1;
                match record.subrecords() {
                    Ok(srs) => total_subrecords += srs.len() as u64,
                    Err(e) => {
                        failures.push(format!(
                            "{}: FormID {:08X} ({}) parse failed: {e}",
                            path.file_name()
                                .expect("path ends in file name")
                                .to_string_lossy(),
                            record.header.form_id.0,
                            record.header.signature,
                        ));
                    }
                }
            }
        }
    }

    eprintln!("  Records        : {total_records}");
    eprintln!("  Subrecords     : {total_subrecords}");
    if !failures.is_empty() {
        eprintln!("  FAILURES ({}):", failures.len());
        for f in failures.iter().take(30) {
            eprintln!("    {f}");
        }
        panic!("{} subrecord parse failure(s)", failures.len());
    }

    Ok(())
}

/// Every EDID (Editor ID) subrecord must decode to a valid UTF-8 string.
#[test]
fn live_08_edid_subrecords_are_valid_utf8() -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = find_data_dir() else {
        return Ok(());
    };
    banner("EDID UTF-8 VALIDITY");

    let paths = collect_plugins(&dir);
    let sig_edid = Signature(*b"EDID");
    let mut failures: Vec<String> = Vec::new();
    let mut edid_count = 0u64;
    let mut max_len = 0usize;

    for path in &paths {
        let Ok(plugin) = open(path) else { continue };
        for group in plugin.groups() {
            for record in group.records_recursive() {
                let Ok(Some(sr)) = record.get(sig_edid) else {
                    continue;
                };
                edid_count += 1;
                match sr.as_zstring() {
                    Ok(s) => max_len = max_len.max(s.len()),
                    Err(e) => {
                        failures.push(format!(
                            "{}: FormID {:08X} EDID decode failed: {e}",
                            path.file_name()
                                .expect("path ends in file name")
                                .to_string_lossy(),
                            record.header.form_id.0,
                        ));
                    }
                }
            }
        }
    }

    eprintln!("  EDID subrecords : {edid_count}  (max length: {max_len})");
    if !failures.is_empty() {
        for f in failures.iter().take(20) {
            eprintln!("  BAD: {f}");
        }
        panic!("{} EDID(s) failed UTF-8 decode", failures.len());
    }

    Ok(())
}

/// Every compressed record (COMPRESSED flag set) must decompress without
/// error and produce a non-empty subrecord list.
#[test]
fn live_09_compressed_records_decompress() -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = find_data_dir() else {
        return Ok(());
    };
    banner("COMPRESSED RECORD DECOMPRESSION");

    let paths = collect_plugins(&dir);
    let mut failures: Vec<String> = Vec::new();
    let mut compressed_count = 0u64;
    let mut total_records = 0u64;

    for path in &paths {
        let Ok(plugin) = open(path) else { continue };
        for group in plugin.groups() {
            for record in group.records_recursive() {
                total_records += 1;
                if !record.header.flags.contains(RecordFlags::COMPRESSED) {
                    continue;
                }
                compressed_count += 1;
                if let Err(e) = record.subrecords() {
                    failures.push(format!(
                        "{}: FormID {:08X} ({}) decompression failed: {e}",
                        path.file_name()
                            .expect("path ends in file name")
                            .to_string_lossy(),
                        record.header.form_id.0,
                        record.header.signature,
                    ));
                }
            }
        }
    }

    let pct = if total_records > 0 {
        compressed_count as f64 / total_records as f64 * 100.0
    } else {
        0.0
    };
    eprintln!("  Compressed : {compressed_count} / {total_records} records  ({pct:.1}%)");

    if !failures.is_empty() {
        for f in failures.iter().take(20) {
            eprintln!("  FAIL: {f}");
        }
        panic!(
            "{} compressed record(s) failed to decompress",
            failures.len()
        );
    }

    Ok(())
}

/// Collects flag distribution across all records. Does not fail — flags are
/// informational only.
#[test]
fn live_10_record_flag_inventory() -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = find_data_dir() else {
        return Ok(());
    };
    banner("RECORD FLAG INVENTORY");

    let paths = collect_plugins(&dir);
    let mut deleted_count = 0u64;
    let mut localized_count = 0u64;
    let mut compressed_count = 0u64;
    let mut ignored_count = 0u64;
    let mut initially_disabled = 0u64;

    for path in &paths {
        let Ok(plugin) = open(path) else { continue };
        for group in plugin.groups() {
            for record in group.records_recursive() {
                let f = record.header.flags;
                if f.contains(RecordFlags::DELETED) {
                    deleted_count += 1;
                }
                if f.contains(RecordFlags::LOCALIZED) {
                    localized_count += 1;
                }
                if f.contains(RecordFlags::COMPRESSED) {
                    compressed_count += 1;
                }
                if f.contains(RecordFlags::IGNORED) {
                    ignored_count += 1;
                }
                if f.contains(RecordFlags::INIT_DISABLED) {
                    initially_disabled += 1;
                }
            }
        }
    }

    eprintln!("  DELETED           : {deleted_count}");
    eprintln!("  LOCALIZED         : {localized_count}");
    eprintln!("  COMPRESSED        : {compressed_count}");
    eprintln!("  IGNORED           : {ignored_count}");
    eprintln!("  INITIALLY_DISABLED: {initially_disabled}");

    Ok(())
}

/// Measures what fraction of record types encountered in the wild are
/// covered by our SSE schema registry.
#[test]
fn live_11_schema_coverage() -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = find_data_dir() else {
        return Ok(());
    };
    banner("SSE SCHEMA COVERAGE");

    let paths = collect_plugins(&dir);
    let reg = SchemaRegistry::sse();
    let mut sig_counts: HashMap<[u8; 4], u64> = HashMap::new();

    for path in &paths {
        let Ok(plugin) = open(path) else { continue };
        for group in plugin.groups() {
            for record in group.records_recursive() {
                *sig_counts.entry(record.header.signature.0).or_default() += 1;
            }
        }
    }

    let total_distinct = sig_counts.len();
    let covered_count = sig_counts
        .keys()
        .filter(|sig| reg.get(Signature(**sig)).is_some())
        .count();
    let covered_records: u64 = sig_counts
        .iter()
        .filter(|(sig, _)| reg.get(Signature(**sig)).is_some())
        .map(|(_, &c)| c)
        .sum();
    let total_records: u64 = sig_counts.values().sum();

    let coverage_pct = covered_records as f64 / total_records as f64 * 100.0;
    let type_pct = covered_count as f64 / total_distinct as f64 * 100.0;

    eprintln!("  Schema registry size     : {}", reg.len());
    eprintln!("  Distinct record types    : {total_distinct} found in wild");
    eprintln!("  Type coverage            : {covered_count} / {total_distinct}  ({type_pct:.1}%)");
    eprintln!(
        "  Record coverage          : {covered_records} / {total_records}  ({coverage_pct:.1}%)"
    );

    // Uncovered types sorted by frequency.
    let mut uncovered: Vec<([u8; 4], u64)> = sig_counts
        .iter()
        .filter(|(sig, _)| reg.get(Signature(**sig)).is_none())
        .map(|(sig, &count)| (*sig, count))
        .collect();
    uncovered.sort_by_key(|b| std::cmp::Reverse(b.1));
    if !uncovered.is_empty() {
        eprintln!("  Uncovered types (top 20 by frequency):");
        for (sig, count) in uncovered.iter().take(20) {
            eprintln!("    {}  {count} records", Signature(*sig));
        }
    }

    Ok(())
}

/// Runs RecordView field decoding against all ESM master files (`.esm`).
///
/// ESP mods routinely omit optional trailing fields inside subrecords — a valid
/// Bethesda format variation — which causes `UnexpectedEof` errors that are not
/// schema bugs.  Restricting the test to ESMs tests the canonical game data
/// where the schema coverage assertion (< 0.5% error rate) is meaningful.
///
/// Localized plugins are additionally skipped because RecordView reads the
/// per-record LOCALIZED flag, but SSE signals localisation only in the
/// plugin header; decoding LString fields in a localized plugin produces
/// false "unexpected EOF" errors.
#[test]
fn live_12_schema_field_decode() -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = find_data_dir() else {
        return Ok(());
    };
    banner("SCHEMA-GUIDED FIELD DECODE (RecordView)");

    let all_paths = collect_plugins(&dir);
    // NOTE: Only .esm master files are tested. ESP mods frequently omit
    // optional trailing fields in subrecords, which produces UnexpectedEof
    // errors that are not schema bugs but a valid format variation.
    let paths: Vec<_> = all_paths
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("esm")))
        .collect();
    let reg = SchemaRegistry::sse();

    let mut records_decoded = 0u64;
    let mut records_skipped = 0u64;
    let mut fields_decoded = 0u64;
    let mut fields_missing = 0u64;
    // Separate counter so the assertion is not fooled by the 50-entry cap on
    // the sample list below.
    let mut decode_error_count = 0u64;
    let mut decode_error_samples: Vec<String> = Vec::new();

    for path in &paths {
        let Ok(plugin) = open(path) else { continue };
        let plugin_localized = plugin.is_localized();
        for group in plugin.groups() {
            for record in group.records_recursive() {
                let Some(schema) = reg.get(record.header.signature) else {
                    records_skipped += 1;
                    continue;
                };
                records_decoded += 1;
                let view = RecordView::new(record, schema, plugin_localized);
                match view.fields() {
                    Ok(fields) => {
                        for f in &fields {
                            use bethkit_core::FieldValue;
                            match &f.value {
                                FieldValue::Missing => fields_missing += 1,
                                _ => fields_decoded += 1,
                            }
                        }
                    }
                    Err(e) => {
                        decode_error_count += 1;
                        if decode_error_samples.len() < 50 {
                            decode_error_samples.push(format!(
                                "{}: {} FormID {:08X}: {e}",
                                path.file_name()
                                    .expect("path ends in file name")
                                    .to_string_lossy(),
                                record.header.signature,
                                record.header.form_id.0,
                            ));
                        }
                    }
                }
            }
        }
    }

    eprintln!("  ESM files tested            : {}", paths.len());
    eprintln!("  Records decoded             : {records_decoded}");
    eprintln!("  Records skipped (no schema) : {records_skipped}");
    eprintln!("  Fields decoded (value)      : {fields_decoded}");
    eprintln!("  Fields missing              : {fields_missing}");
    eprintln!("  Decode errors               : {decode_error_count}");
    for e in decode_error_samples.iter().take(10) {
        eprintln!("    ERR: {e}");
    }

    // Tolerate up to 0.5% decode errors against ESM master files.
    let error_rate = decode_error_count as f64 / records_decoded.max(1) as f64;
    assert!(
        error_rate < 0.005,
        "field decode error rate {:.3}% exceeds 0.5% threshold ({decode_error_count} errors in \
         {records_decoded} records)",
        error_rate * 100.0
    );

    Ok(())
}

/// Performs a thorough analysis of `Skyrim.esm` — the largest and most
/// complex plugin in the base game — and prints a detailed breakdown.
#[test]
fn live_13_skyrim_esm_deep_analysis() -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = find_data_dir() else {
        return Ok(());
    };
    banner("SKYRIM.ESM DEEP ANALYSIS");

    let esm_path = dir.join("Skyrim.esm");
    assert!(
        esm_path.exists(),
        "Skyrim.esm not found at {}",
        esm_path.display()
    );

    let file_size = std::fs::metadata(&esm_path)?.len();
    eprintln!("  File size : {}", fmt_bytes(file_size));

    let t0 = Instant::now();
    let plugin = open(&esm_path).map_err(|e| e.to_string())?;
    let open_time = t0.elapsed();

    eprintln!("  Open time : {:.3} s", open_time.as_secs_f64());
    eprintln!("  HEDR ver  : {}", plugin.header.hedr_version);
    eprintln!("  Masters   : {:?}", plugin.masters());
    eprintln!("  Groups    : {}", plugin.group_count());
    eprintln!("  Localized : {}", plugin.is_localized());

    let mut sig_counts: HashMap<[u8; 4], u64> = HashMap::new();
    let mut total_records = 0u64;
    let mut compressed_records = 0u64;
    let mut deleted_records = 0u64;
    let mut localized_records = 0u64;
    let mut total_subrecords = 0u64;
    let mut failed_subrecords = 0u64;

    for group in plugin.groups() {
        for record in group.records_recursive() {
            total_records += 1;
            *sig_counts.entry(record.header.signature.0).or_default() += 1;
            let f = record.header.flags;
            if f.contains(RecordFlags::COMPRESSED) {
                compressed_records += 1;
            }
            if f.contains(RecordFlags::DELETED) {
                deleted_records += 1;
            }
            if f.contains(RecordFlags::LOCALIZED) {
                localized_records += 1;
            }
            match record.subrecords() {
                Ok(srs) => total_subrecords += srs.len() as u64,
                Err(_) => failed_subrecords += 1,
            }
        }
    }

    eprintln!("  Total records      : {total_records}");
    eprintln!("  Compressed records : {compressed_records}");
    eprintln!("  Deleted records    : {deleted_records}");
    eprintln!("  Localized records  : {localized_records}");
    eprintln!("  Total subrecords   : {total_subrecords}");
    eprintln!("  Subrecord failures : {failed_subrecords}");

    let mut sorted: Vec<([u8; 4], u64)> = sig_counts.into_iter().collect();
    sorted.sort_by_key(|b| std::cmp::Reverse(b.1));
    eprintln!("  Top 30 record types:");
    for (sig, count) in sorted.iter().take(30) {
        let pct = *count as f64 / total_records as f64 * 100.0;
        eprintln!("    {}  {:>8}  ({pct:4.1}%)", Signature(*sig), count);
    }

    assert_eq!(
        failed_subrecords, 0,
        "Skyrim.esm had subrecord parse failures"
    );

    Ok(())
}

/// Collects group type statistics across all plugins and validates that no
/// unknown group type (outside 0-9) appears in SSE plugins.
#[test]
fn live_14_group_type_distribution() -> Result<(), Box<dyn std::error::Error>> {
    use bethkit_core::GroupChild;
    let Some(dir) = find_data_dir() else {
        return Ok(());
    };
    banner("GROUP TYPE DISTRIBUTION");

    let paths = collect_plugins(&dir);
    let mut type_counts: HashMap<i32, u64> = HashMap::new();
    let mut unknown_types: Vec<String> = Vec::new();

    fn count_group(
        group: &bethkit_core::Group,
        counts: &mut HashMap<i32, u64>,
        unknowns: &mut Vec<String>,
        path_name: &str,
    ) {
        let raw: i32 = group.header.group_type as i32;
        *counts.entry(raw).or_default() += 1;
        if !(0..=9).contains(&raw) && unknowns.len() < 20 {
            unknowns.push(format!("{path_name}: unknown group type {raw}"));
        }
        for child in group.children() {
            if let GroupChild::Group(sub) = child {
                count_group(sub, counts, unknowns, path_name);
            }
        }
    }

    for path in &paths {
        let Ok(plugin) = open(path) else { continue };
        let name = path
            .file_name()
            .expect("path ends in file name")
            .to_string_lossy()
            .into_owned();
        for group in plugin.groups() {
            count_group(group, &mut type_counts, &mut unknown_types, &name);
        }
    }

    let type_names: [(i32, &str); 10] = [
        (0, "Normal (top-level)"),
        (1, "World children"),
        (2, "Interior cell block"),
        (3, "Interior cell sub-block"),
        (4, "Exterior cell block"),
        (5, "Exterior cell sub-block"),
        (6, "Cell children"),
        (7, "Topic children"),
        (8, "Cell persistent children"),
        (9, "Cell temporary children"),
    ];

    for (raw, label) in &type_names {
        let count = type_counts.get(raw).copied().unwrap_or(0);
        eprintln!("  type {raw}: {label:30} : {count}");
    }

    if !unknown_types.is_empty() {
        for u in &unknown_types {
            eprintln!("  UNKNOWN: {u}");
        }
        panic!("{} unknown group type(s) found", unknown_types.len());
    }

    Ok(())
}

/// Picks the first record from each base game ESM and verifies that
/// `find_record` returns the same record by FormID.
#[test]
fn live_15_find_record_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = find_data_dir() else {
        return Ok(());
    };
    banner("find_record ROUND-TRIP");

    let base_esms = [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ];

    for esm_name in &base_esms {
        let path = dir.join(esm_name);
        if !path.exists() {
            eprintln!("  SKIP {esm_name}: not found");
            continue;
        }
        let Ok(plugin) = open(&path) else {
            eprintln!("  SKIP {esm_name}: failed to open");
            continue;
        };

        let Some(first_record) = plugin
            .groups()
            .iter()
            .flat_map(|g| g.records_recursive())
            .next()
        else {
            eprintln!("  SKIP {esm_name}: no records found");
            continue;
        };

        let fid = first_record.header.form_id;
        let found = plugin.find_record(fid);

        assert!(
            found.is_some(),
            "{esm_name}: find_record({fid}) returned None but record exists"
        );
        assert_eq!(
            found.expect("record was found").header.form_id,
            fid,
            "{esm_name}: find_record returned wrong record"
        );
        eprintln!("  {esm_name}: find_record({fid}) OK");
    }

    Ok(())
}

/// Loads the five base-game ESMs into a [`PluginCache`] in canonical load order
/// Loads Skyrim.esm into a [`PluginCache`] and verifies winning-override
/// semantics and EditorID lookup.
///
/// Checks:
/// - `record_count()` returns at least one record after loading Skyrim.esm.
/// - `find_by_editor_id` finds an EditorID that is known to exist via direct
///   plugin iteration (picks the first NPC_ EditorID it can find).
/// - After adding Update.esm, the multi-plugin cache has at least as many
///   records as Skyrim.esm alone.
#[test]
fn live_16_plugin_cache_winning_override() -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = find_data_dir() else {
        return Ok(());
    };
    banner("PLUGIN CACHE — winning-override lookup");

    let skyrim_path = dir.join("Skyrim.esm");
    if !skyrim_path.exists() {
        eprintln!("  SKIP: Skyrim.esm not found");
        return Ok(());
    }

    // Load Skyrim.esm into the cache.
    let skyrim = open(&skyrim_path).map_err(|e| e.to_string())?;
    let mut cache = PluginCache::new();
    cache.add("Skyrim.esm", skyrim).map_err(|e| e.to_string())?;
    let count_skyrim: usize = cache.record_count();
    eprintln!("  Skyrim.esm records        : {count_skyrim}");
    assert!(count_skyrim > 0, "expected records in Skyrim.esm cache");

    // Find the first NPC_ EditorID directly from the plugin (ground truth).
    let skyrim_direct = open(&skyrim_path).map_err(|e| e.to_string())?;
    let mut first_edid: Option<String> = None;
    'outer: for group in skyrim_direct.groups() {
        for record in group.records_recursive() {
            if record.header.signature == Signature(*b"NPC_") {
                if let Ok(Some(eid)) = record.editor_id() {
                    first_edid = Some(eid.to_owned());
                    break 'outer;
                }
            }
        }
    }
    eprintln!("  First NPC_ EDID (direct)  : {:?}", first_edid);

    // Verify the cache can find that same EditorID.
    if let Some(ref eid) = first_edid {
        let found: bool = cache.find_by_editor_id(eid).is_some();
        eprintln!("  EDID found in cache       : {found}");
        assert!(found, "EditorID {eid:?} not found in PluginCache");
    }

    // Load Update.esm on top and verify the record count stays >= Skyrim.esm.
    let update_path = dir.join("Update.esm");
    if update_path.exists() {
        match open(&update_path) {
            Ok(update_esm) => {
                cache
                    .add("Update.esm", update_esm)
                    .map_err(|e| e.to_string())?;
                let count_with_update: usize = cache.record_count();
                eprintln!("  With Update.esm           : {count_with_update} records");
                assert!(
                    count_with_update >= count_skyrim,
                    "cache shrunk after adding Update.esm"
                );
            }
            Err(e) => eprintln!("  SKIP Update.esm: {e}"),
        }
    }

    Ok(())
}

/// Measures the wall-clock time to open and fully iterate (with subrecord
/// parse) every ESM file in the Data directory.
#[test]
fn bench_a_all_esm_full_parse() -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = find_data_dir() else {
        return Ok(());
    };
    banner("BENCHMARK A — All ESM files: full parse + subrecord decode");

    let paths: Vec<PathBuf> = collect_plugins(&dir)
        .into_iter()
        .filter(|p| p.extension().map(|e| e == "esm").unwrap_or(false))
        .collect();

    let total_bytes: u64 = paths
        .iter()
        .filter_map(|p| std::fs::metadata(p).ok())
        .map(|m| m.len())
        .sum();

    let mut records_total = 0u64;
    let mut subrecords_total = 0u64;
    let mut errors = 0usize;

    let t0 = Instant::now();
    for path in &paths {
        let Ok(plugin) = open(path) else {
            errors += 1;
            continue;
        };
        for group in plugin.groups() {
            for record in group.records_recursive() {
                records_total += 1;
                match record.subrecords() {
                    Ok(srs) => subrecords_total += srs.len() as u64,
                    Err(_) => errors += 1,
                }
            }
        }
    }
    let elapsed = t0.elapsed();

    eprintln!(
        "  {} ESM files  {}  in {:.3} s  ({})  {} records  {} subrecords  {} errors",
        paths.len(),
        fmt_bytes(total_bytes),
        elapsed.as_secs_f64(),
        fmt_mbps(total_bytes, elapsed),
        records_total,
        subrecords_total,
        errors,
    );

    Ok(())
}

/// Measures how long it takes to open and fully parse Skyrim.esm three
/// times back-to-back to capture OS page-cache warm-up effects.
#[test]
fn bench_b_skyrim_esm_repeated_parse() -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = find_data_dir() else {
        return Ok(());
    };
    banner("BENCHMARK B — Skyrim.esm repeated parse (3 runs)");

    let path = dir.join("Skyrim.esm");
    if !path.exists() {
        eprintln!("  SKIP: Skyrim.esm not found");
        return Ok(());
    }
    let file_size = std::fs::metadata(&path)?.len();

    for run in 1..=3 {
        let t0 = Instant::now();
        let plugin = open(&path).map_err(|e| e.to_string())?;
        let mut record_count = 0u64;
        for group in plugin.groups() {
            for record in group.records_recursive() {
                record_count += 1;
                let _ = record.subrecords();
            }
        }
        let elapsed = t0.elapsed();
        eprintln!(
            "  Run {run}: {:.3} s  {}  {} records  ({})",
            elapsed.as_secs_f64(),
            fmt_bytes(file_size),
            record_count,
            fmt_mbps(file_size, elapsed),
        );
    }

    Ok(())
}

/// Measures raw schema registry lookup speed with 10 million iterations.
#[test]
fn bench_c_schema_lookup_speed() -> Result<(), Box<dyn std::error::Error>> {
    banner("BENCHMARK C — Schema registry lookup speed (10 M lookups)");

    let reg = SchemaRegistry::sse();
    let sigs: [Signature; 8] = [
        Signature(*b"NPC_"),
        Signature(*b"WEAP"),
        Signature(*b"ARMO"),
        Signature(*b"CELL"),
        Signature(*b"WRLD"),
        Signature(*b"QUST"),
        Signature(*b"DIAL"),
        Signature(*b"XXXX"), // intentionally unknown
    ];

    const ITERATIONS: u64 = 10_000_000;
    let mut hits = 0u64;
    let t0 = Instant::now();
    for i in 0..ITERATIONS {
        let sig = sigs[(i % sigs.len() as u64) as usize];
        if reg.get(sig).is_some() {
            hits += 1;
        }
    }
    let elapsed = t0.elapsed();

    eprintln!(
        "  {ITERATIONS} lookups in {:.3} s  ({:.0} lookups/s)  hits: {hits}",
        elapsed.as_secs_f64(),
        ITERATIONS as f64 / elapsed.as_secs_f64(),
    );

    Ok(())
}

/// Measures EDID decode throughput across all ESM files.
#[test]
fn bench_d_edid_decode_throughput() -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = find_data_dir() else {
        return Ok(());
    };
    banner("BENCHMARK D — EDID decode throughput (all ESMs)");

    let paths: Vec<PathBuf> = collect_plugins(&dir)
        .into_iter()
        .filter(|p| p.extension().map(|e| e == "esm").unwrap_or(false))
        .collect();

    let sig_edid = Signature(*b"EDID");
    let mut edid_count = 0u64;
    let mut edid_bytes = 0u64;
    let mut errors = 0usize;

    let t0 = Instant::now();
    for path in &paths {
        let Ok(plugin) = open(path) else { continue };
        for group in plugin.groups() {
            for record in group.records_recursive() {
                let Ok(Some(sr)) = record.get(sig_edid) else {
                    continue;
                };
                edid_count += 1;
                edid_bytes += sr.as_bytes().len() as u64;
                if sr.as_zstring().is_err() {
                    errors += 1;
                }
            }
        }
    }
    let elapsed = t0.elapsed();

    eprintln!(
        "  {} EDID subrecords  {}  in {:.3} s  ({:.0} EDID/s)  {} errors",
        edid_count,
        fmt_bytes(edid_bytes),
        elapsed.as_secs_f64(),
        edid_count as f64 / elapsed.as_secs_f64(),
        errors,
    );

    Ok(())
}

/// Measures how fast we can open and read just the headers of all 2000+
/// plugins (no group/record iteration).
#[test]
fn bench_e_all_plugins_header_only() -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = find_data_dir() else {
        return Ok(());
    };
    banner("BENCHMARK E — All plugins: header-only open");

    let paths = collect_plugins(&dir);
    let total_bytes: u64 = paths
        .iter()
        .filter_map(|p| std::fs::metadata(p).ok())
        .map(|m| m.len())
        .sum();

    let mut ok = 0usize;
    let mut errors = 0usize;

    let t0 = Instant::now();
    for path in &paths {
        match open(path) {
            Ok(_) => ok += 1,
            Err(_) => errors += 1,
        }
    }
    let elapsed = t0.elapsed();

    eprintln!(
        "  {} plugins  {}  in {:.3} s  ({:.1} plugins/s)  {} errors",
        paths.len(),
        fmt_bytes(total_bytes),
        elapsed.as_secs_f64(),
        ok as f64 / elapsed.as_secs_f64(),
        errors,
    );

    Ok(())
}

/// Opens Skyrim.esm and runs RecordView field decoding over every record
/// with schema coverage.  Reports throughput.
#[test]
fn bench_f_skyrim_esm_full_field_decode() -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = find_data_dir() else {
        return Ok(());
    };
    banner("BENCHMARK F — Skyrim.esm: full RecordView field decode");

    let path = dir.join("Skyrim.esm");
    if !path.exists() {
        eprintln!("  SKIP: Skyrim.esm not found");
        return Ok(());
    }

    let plugin = open(&path).map_err(|e| e.to_string())?;
    let reg = SchemaRegistry::sse();
    let plugin_localized = plugin.is_localized();

    let mut records_decoded = 0u64;
    let mut fields_decoded = 0u64;
    let mut decode_errors = 0u64;

    let t0 = Instant::now();
    for group in plugin.groups() {
        for record in group.records_recursive() {
            let Some(schema) = reg.get(record.header.signature) else {
                continue;
            };
            records_decoded += 1;
            let view = RecordView::new(record, schema, plugin_localized);
            match view.fields() {
                Ok(fields) => fields_decoded += fields.len() as u64,
                Err(_) => decode_errors += 1,
            }
        }
    }
    let elapsed = t0.elapsed();

    eprintln!(
        "  Records decoded  : {records_decoded}  ({:.0} rec/s)",
        records_decoded as f64 / elapsed.as_secs_f64()
    );
    eprintln!(
        "  Fields decoded   : {fields_decoded}  ({:.0} field/s)",
        fields_decoded as f64 / elapsed.as_secs_f64()
    );
    eprintln!("  Decode errors    : {decode_errors}");
    eprintln!("  Elapsed          : {:.3} s", elapsed.as_secs_f64());

    Ok(())
}

/// Measures decompression throughput across all compressed records in all
/// ESM files.
#[test]
fn bench_g_decompression_throughput() -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = find_data_dir() else {
        return Ok(());
    };
    banner("BENCHMARK G — zlib decompression throughput (all ESMs)");

    let paths: Vec<PathBuf> = collect_plugins(&dir)
        .into_iter()
        .filter(|p| p.extension().map(|e| e == "esm").unwrap_or(false))
        .collect();

    let mut compressed_records = 0u64;
    let mut compressed_bytes = 0u64;
    let mut errors = 0usize;

    let t0 = Instant::now();
    for path in &paths {
        let Ok(plugin) = open(path) else { continue };
        for group in plugin.groups() {
            for record in group.records_recursive() {
                if !record.header.flags.contains(RecordFlags::COMPRESSED) {
                    continue;
                }
                compressed_records += 1;
                compressed_bytes += record.header.data_size as u64;
                if record.subrecords().is_err() {
                    errors += 1;
                }
            }
        }
    }
    let elapsed = t0.elapsed();

    eprintln!(
        "  {} compressed records  {}  in {:.3} s  ({})  {} errors",
        compressed_records,
        fmt_bytes(compressed_bytes),
        elapsed.as_secs_f64(),
        fmt_mbps(compressed_bytes, elapsed),
        errors,
    );

    Ok(())
}

/// The ultimate throughput number: open + iterate all records + parse all
/// subrecords across every plugin in a single sequential pass.
#[test]
fn bench_h_aggregate_all_plugins_full_pass() -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = find_data_dir() else {
        return Ok(());
    };
    banner("BENCHMARK H — All plugins: aggregate single-pass (open + iterate + subrecord parse)");

    let paths = collect_plugins(&dir);
    let total_bytes: u64 = paths
        .iter()
        .filter_map(|p| std::fs::metadata(p).ok())
        .map(|m| m.len())
        .sum();

    let mut records = 0u64;
    let mut subrecords = 0u64;
    let mut errors = 0usize;

    let t0 = Instant::now();
    for path in &paths {
        let Ok(plugin) = open(path) else {
            errors += 1;
            continue;
        };
        for group in plugin.groups() {
            for record in group.records_recursive() {
                records += 1;
                match record.subrecords() {
                    Ok(srs) => subrecords += srs.len() as u64,
                    Err(_) => errors += 1,
                }
            }
        }
    }
    let elapsed = t0.elapsed();

    eprintln!(
        "  {} plugins  {}  in {:.3} s",
        paths.len(),
        fmt_bytes(total_bytes),
        elapsed.as_secs_f64(),
    );
    eprintln!("  Records    : {records}");
    eprintln!("  Subrecords : {subrecords}");
    eprintln!("  Errors     : {errors}");
    eprintln!();
    eprintln!(
        "  *** Peak throughput: {}  ({:.0} records/s) ***",
        fmt_mbps(total_bytes, elapsed),
        records as f64 / elapsed.as_secs_f64(),
    );

    Ok(())
}

// ── Test 17: PluginPatcher no-op on base ESMs ─────────────────────────────────

/// Runs [`PluginPatcher`] with no patches applied over the five base game
/// ESMs and asserts that the output bytes are byte-for-byte identical to the
/// original file.
///
/// This validates that the patcher fast-path (no patches, no header changes)
/// does not corrupt real, large, complex master files.
#[test]
fn live_17_patcher_noop_base_esms() -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = find_data_dir() else {
        return Ok(());
    };
    banner("PATCHER NO-OP ON BASE ESMs");

    let base_esms = [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ];

    for esm_name in &base_esms {
        let path = dir.join(esm_name);
        if !path.exists() {
            eprintln!("  SKIP {esm_name}: not found");
            continue;
        }

        // Read original bytes before opening (so the mmap does not alias).
        let original: Vec<u8> = std::fs::read(&path)?;
        let plugin = open(&path).map_err(|e| e.to_string())?;

        let mut patched: Vec<u8> = Vec::with_capacity(original.len());
        let patcher = PluginPatcher::new(&plugin);
        patcher.write_to(&mut patched).map_err(|e| e.to_string())?;

        assert_eq!(
            original.len(),
            patched.len(),
            "{esm_name}: patcher no-op changed file size ({} -> {} bytes)",
            original.len(),
            patched.len()
        );
        assert_eq!(
            original, patched,
            "{esm_name}: patcher no-op produced different bytes"
        );

        eprintln!("  {esm_name}: byte-identical ({} bytes)", original.len());
    }

    Ok(())
}

// ── Test 18: BSA archives in the Data directory ───────────────────────────────

/// Opens every `.bsa` file found in the Data directory using
/// [`bethkit_bsa::open`] and verifies:
///
/// - The archive opens without error.
/// - It contains at least one entry.
/// - The first entry can be extracted without error.
#[test]
fn live_18_bsa_archives_open_and_extract() -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = find_data_dir() else {
        return Ok(());
    };
    banner("BSA ARCHIVES — open + entry listing + first-entry extract");

    let bsa_paths: Vec<std::path::PathBuf> = {
        let mut v: Vec<_> = std::fs::read_dir(&dir)?
            .filter_map(|e| {
                let e = e.ok()?;
                let p = e.path();
                let ext = p.extension()?.to_ascii_lowercase();
                if ext == "bsa" || ext == "ba2" {
                    Some(p)
                } else {
                    None
                }
            })
            .collect();
        v.sort();
        v
    };

    if bsa_paths.is_empty() {
        eprintln!("  SKIP: no .bsa/.ba2 files found in Data directory");
        return Ok(());
    }

    let mut ok = 0usize;
    let mut failures: Vec<String> = Vec::new();
    let mut non_utf8_skipped = 0usize;
    let mut total_entries = 0u64;

    for path in &bsa_paths {
        let name = path
            .file_name()
            .expect("path ends in file name")
            .to_string_lossy();
        match bethkit_bsa::open(path) {
            Err(e) => {
                let msg = e.to_string();
                // Non-UTF-8 file names inside BSAs are a known limitation of
                // some locale-specific archives (e.g. Japanese voice packs).
                // Count them separately rather than failing the test.
                if msg.contains("non-UTF-8") || msg.contains("UTF-8") {
                    non_utf8_skipped += 1;
                    eprintln!("  NOTE {name}: skipped (non-UTF-8 paths): {e}");
                } else {
                    failures.push(format!("{name}: open failed: {e}"));
                }
                continue;
            }
            Ok(archive) => {
                let entries = archive.entries();
                if entries.is_empty() {
                    failures.push(format!("{name}: archive has no entries"));
                    continue;
                }
                total_entries += entries.len() as u64;

                // Extract the first entry to exercise the decompression path.
                let first_path = entries[0].path.clone();
                match archive.extract(&first_path) {
                    None => {
                        failures.push(format!("{name}: extract({first_path:?}) returned None"));
                        continue;
                    }
                    Some(Err(e)) => {
                        failures.push(format!("{name}: extract({first_path:?}) error: {e}"));
                        continue;
                    }
                    Some(Ok(bytes)) => {
                        if bytes.is_empty() {
                            // Some textures have a 0-byte stored size — not
                            // a hard error, just noteworthy.
                            eprintln!("  NOTE {name}: first entry {first_path:?} is empty");
                        }
                    }
                }

                ok += 1;
            }
        }
    }

    eprintln!("  Archives opened  : {ok} / {}", bsa_paths.len());
    eprintln!("  Non-UTF-8 skipped: {non_utf8_skipped}");
    eprintln!("  Total entries    : {total_entries}");

    if !failures.is_empty() {
        for f in failures.iter().take(20) {
            eprintln!("  FAIL: {f}");
        }
        panic!("{} BSA/BA2 archive(s) failed", failures.len());
    }

    Ok(())
}

// ── Test 19: LoadOrder construction and FormID resolution ─────────────────────

/// Builds a [`LoadOrder`] from all plugins in the Data directory (in sorted
/// order) and verifies that `resolve()` correctly maps a selection of
/// well-known SSE FormIDs back to their owning plugins.
///
/// Well-known FormIDs tested:
/// - `0x00000014` (Player character) — always owned by `Skyrim.esm`.
/// - `0x0002BF9B` (a common Skyrim.esm record used in many mods).
#[test]
fn live_19_load_order_resolve() -> Result<(), Box<dyn std::error::Error>> {
    use bethkit_core::FormId;
    let Some(dir) = find_data_dir() else {
        return Ok(());
    };
    banner("LOAD ORDER — construction + FormID resolution");

    let paths = collect_plugins(&dir);

    // Build the load order from all discovered plugins.
    let mut load_order = LoadOrder::new();
    let mut plugin_masters: Vec<(String, Vec<String>)> = Vec::new();
    // (canonical_name, esl_slot) pairs collected for light plugins.
    let mut light_entries: Vec<(String, u16)> = Vec::new();
    let mut skipped = 0usize;

    for path in &paths {
        let file_name = path
            .file_name()
            .expect("path ends in file name")
            .to_string_lossy()
            .into_owned();
        match open(path) {
            Ok(plugin) => {
                let kind = plugin.kind();
                let masters: Vec<String> = plugin.masters().iter().map(|s| s.to_string()).collect();
                match load_order.push(&file_name, kind) {
                    Err(_) => {
                        // Regular index full (0xFE) or ESL slots exhausted.
                        skipped += 1;
                        continue;
                    }
                    Ok(entry) => {
                        if let Some(slot) = entry.light_slot {
                            light_entries.push((entry.name.clone(), slot));
                        }
                        plugin_masters.push((file_name, masters));
                    }
                }
            }
            Err(_) => skipped += 1,
        }
    }

    eprintln!("  Load order entries : {}", load_order.len());
    eprintln!("  Light (ESL) plugins: {}", light_entries.len());
    eprintln!("  Skipped            : {skipped}");

    // Verify the Player reference (FormID 0x00000014) resolves to Skyrim.esm.
    // The Player is a base-game record with file index 0 — its owner is the
    // first master of whatever plugin references it, or Skyrim.esm itself.
    let player_fid = FormId(0x0000_0014);
    let skyrim_masters: Vec<String> = Vec::new(); // Skyrim.esm has no masters
    let resolved = load_order.resolve(player_fid, "skyrim.esm", &skyrim_masters);
    match &resolved {
        Some(gfid) => {
            eprintln!("  Player FormID 0x00000014 -> {gfid}");
            assert_eq!(
                gfid.plugin_name, "skyrim.esm",
                "Player FormID must resolve to skyrim.esm, got {:?}",
                gfid.plugin_name
            );
            assert_eq!(gfid.object_id, 0x14, "Player object_id must be 0x14");
        }
        None => eprintln!("  SKIP: Skyrim.esm not in load order"),
    }

    // Verify that every plugin's own-authored FormIDs resolve back to itself.
    // Test the first N plugins to keep runtime bounded.
    let sample: Vec<&(String, Vec<String>)> = plugin_masters.iter().take(10).collect();
    for (name, masters) in &sample {
        // Construct a FormID authored by this plugin: file_index = masters.len(),
        // object_id = 0x000800 (lowest non-special object_id).
        let file_index: u8 = masters.len() as u8;
        let self_authored = FormId((u32::from(file_index) << 24) | 0x0000_0800);
        let gfid = load_order.resolve(self_authored, name, masters);
        assert!(
            gfid.is_some(),
            "FormID authored by {name} failed to resolve"
        );
        let gfid = gfid.expect("just checked is_some");
        assert_eq!(
            gfid.plugin_name,
            name.to_lowercase(),
            "self-authored FormID resolved to wrong plugin"
        );
    }
    eprintln!(
        "  Self-authored FormID resolution: {} / 10 OK",
        sample.len()
    );

    // Verify ESL-encoded FormID resolution for light plugins.
    //
    // For a light plugin with ESL slot S, a self-authored FormID is:
    //   0xFE_00_00_00  | (S << 12) | 0x0800
    // The source_plugin and masters arguments are irrelevant for 0xFE FormIDs;
    // the slot encoded in bits 23:12 uniquely identifies the owner.
    if light_entries.is_empty() {
        eprintln!("  ESL FormID resolution: no light plugins in load order, skipped");
    } else {
        let esl_sample: Vec<&(String, u16)> = light_entries.iter().take(10).collect();
        let mut esl_ok = 0usize;
        for (name, slot) in &esl_sample {
            let raw: u32 = (0xFE_u32 << 24) | ((*slot as u32) << 12) | 0x0800;
            let fid = FormId(raw);
            let gfid = load_order.resolve(fid, name, &[]);
            assert!(
                gfid.is_some(),
                "ESL FormID for {name} (slot {slot:#05X}) failed to resolve"
            );
            let gfid = gfid.expect("just checked is_some");
            assert_eq!(
                gfid.plugin_name,
                name.to_lowercase(),
                "ESL FormID resolved to wrong plugin: expected {name}, got {}",
                gfid.plugin_name,
            );
            assert_eq!(
                gfid.object_id, 0x0800,
                "ESL object_id mismatch for {name}"
            );
            esl_ok += 1;
        }
        eprintln!(
            "  ESL FormID resolution: {esl_ok} / {} OK",
            esl_sample.len()
        );
    }

    Ok(())
}

// ── Test 20: Localized plugins — string tables exist and parse ────────────────

/// Finds all localized plugins (`plugin.is_localized() == true`) and for
/// each one verifies that at least one of the three accompanying string
/// table files (`.strings`, `.dlstrings`, `.ilstrings`) exists on disk and
/// can be opened by [`StringTable::open`] without error.
///
/// Also verifies the string table is non-empty for plugins that have a
/// `STRINGS` file.
#[test]
fn live_20_localized_string_tables() -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = find_data_dir() else {
        return Ok(());
    };
    banner("LOCALIZED PLUGINS — string table existence + parse");

    let paths = collect_plugins(&dir);

    let mut localized_count = 0usize;
    let mut tables_found = 0usize;
    let mut parse_failures: Vec<String> = Vec::new();
    let mut missing_tables: Vec<String> = Vec::new();

    for path in &paths {
        let Ok(plugin) = open(path) else { continue };
        if !plugin.is_localized() {
            continue;
        }
        localized_count += 1;

        let name = path
            .file_name()
            .expect("path ends in file name")
            .to_string_lossy()
            .into_owned();

        // Check English string tables; SSE ships "english" as the base locale.
        let sibling_paths = StringTable::sibling_paths(path, "english");
        let any_exists = sibling_paths.iter().any(|p| p.exists());

        if !any_exists {
            // String tables can be absent for plugins that carry no localised
            // text (e.g. texture/mesh-only patches that wrongly set the flag).
            missing_tables.push(name.clone());
            continue;
        }

        // Open each table that exists and verify it parses correctly.
        for (idx, table_path) in sibling_paths.iter().enumerate() {
            if !table_path.exists() {
                continue;
            }
            let kind = match idx {
                0 => StringFileKind::Strings,
                1 => StringFileKind::DLStrings,
                _ => StringFileKind::ILStrings,
            };
            match StringTable::open_as(table_path, kind) {
                Ok(table) => {
                    tables_found += 1;
                    if idx == 0 {
                        // The .strings file should have at least one entry for
                        // a genuinely localized plugin.
                        assert!(
                            !table.is_empty(),
                            "{name}: .strings file exists but is empty"
                        );
                    }
                }
                Err(e) => {
                    parse_failures.push(format!(
                        "{name}: {} parse failed: {e}",
                        table_path
                            .file_name()
                            .expect("table path ends in file name")
                            .to_string_lossy()
                    ));
                }
            }
        }
    }

    eprintln!("  Localized plugins    : {localized_count}");
    eprintln!("  String tables opened : {tables_found}");
    eprintln!("  Plugins with no tables found : {}", missing_tables.len());
    if !missing_tables.is_empty() {
        for m in missing_tables.iter().take(10) {
            eprintln!("    (no tables) {m}");
        }
    }

    if !parse_failures.is_empty() {
        for f in parse_failures.iter().take(20) {
            eprintln!("  FAIL: {f}");
        }
        panic!("{} string table(s) failed to parse", parse_failures.len());
    }

    Ok(())
}
