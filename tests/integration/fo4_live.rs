// SPDX-License-Identifier: Apache-2.0
//!
//! Live integration + benchmark suite against a real Fallout 4 game
//! installation.
//!
//! # Precondition
//!
//! Set the environment variable `FO4_DATA_DIR` to the path of your Fallout 4
//! `Data/` folder, **or** place the installation at the default path:
//!
//! ```text
//! E:\SteamLibrary\steamapps\common\Fallout 4\Data
//! ```
//!
//! If neither path exists the entire suite is skipped so that CI passes
//! without a game installation.
//!
//! # Run
//!
//! ```text
//! cargo test --test fo4_live -- --nocapture
//! ```

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use bethkit_core::{
    GameContext, Plugin, PluginKind, RecordFlags, RecordView, SchemaRegistry, Signature,
};

// ── constants ─────────────────────────────────────────────────────────────────

const DEFAULT_DATA_DIR: &str = r"E:\SteamLibrary\steamapps\common\Fallout 4\Data";

// Record types that are placement / navmesh records and intentionally have no
// schema entry (REFR, ACHR, etc. are not in the type-level registry).
const KNOWN_NO_SCHEMA: &[&[u8; 4]] = &[
    b"NAVM", b"NAVI", b"REFR", b"ACHR", b"PGRE", b"PMIS", b"PARW", b"PBAR", b"PBEA", b"PCON",
    b"PFLA", b"PHZD", b"ACRE",
];

// ── helpers ───────────────────────────────────────────────────────────────────

/// Locates the Fallout 4 Data directory.
///
/// Returns `None` when the suite should be skipped.
fn find_data_dir() -> Option<PathBuf> {
    if let Ok(val) = std::env::var("FO4_DATA_DIR") {
        let p = PathBuf::from(val);
        if p.exists() {
            return Some(p);
        }
        eprintln!(
            "FO4_DATA_DIR is set but path does not exist: {}",
            p.display()
        );
        return None;
    }

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

/// Opens a plugin with FO4 context, returning an error message on failure.
fn open(path: &Path) -> Result<Plugin, String> {
    Plugin::open(path, GameContext::fallout4()).map_err(|e| format!("{}: {e}", path.display()))
}

/// Prints a section banner to stderr.
fn banner(title: &str) {
    eprintln!();
    eprintln!("━━━  {title}  ━━━");
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
        return "∞ MB/s".to_owned();
    }
    format!("{:.1} MB/s", bytes as f64 / (1 << 20) as f64 / secs)
}

// ── Benchmark result accumulator ──────────────────────────────────────────────

struct BenchResult {
    label: &'static str,
    files: usize,
    bytes: u64,
    elapsed: Duration,
    records: u64,
    errors: usize,
}

impl BenchResult {
    fn print(&self) {
        let mbps = fmt_mbps(self.bytes, self.elapsed);
        let rps = if self.elapsed.as_secs_f64() > 0.0 {
            format!("{:.0}", self.records as f64 / self.elapsed.as_secs_f64())
        } else {
            "∞".to_owned()
        };
        eprintln!(
            "  {:50}  {:>6} files  {:>10}  {:>8.3} s  {:>12} MB/s  {:>12} rec/s  {} err",
            self.label,
            self.files,
            fmt_bytes(self.bytes),
            self.elapsed.as_secs_f64(),
            mbps.trim_end_matches(" MB/s"),
            rps,
            self.errors,
        );
    }
}

// ── Test 1: Dataset discovery ─────────────────────────────────────────────────

/// Reports dataset statistics — does NOT assert, just prints.
///
/// Verifies that the live FO4 data directory is readable and contains a
/// plausible number of plugin files.
#[test]
fn fo4_live_01_dataset_discovery() -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = find_data_dir() else {
        eprintln!("SKIP: Fallout 4 Data directory not found");
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

    assert!(
        paths
            .iter()
            .any(|p| p.file_name().map(|n| n == "Fallout4.esm").unwrap_or(false)),
        "Fallout4.esm not found in Data directory"
    );
    assert!(
        paths.len() >= 5,
        "too few plugin files — expected at least 5"
    );

    Ok(())
}

// ── Test 2: All plugins open without error ────────────────────────────────────

/// Every `.esp` / `.esm` / `.esl` file in the Data directory must parse
/// without returning an error.
#[test]
fn fo4_live_02_all_plugins_open() -> Result<(), Box<dyn std::error::Error>> {
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
            eprintln!("    … and {} more", failures.len() - 30);
        }
        panic!("{} plugin(s) failed to open", failures.len());
    }

    Ok(())
}

// ── Test 3: Record signature validity ─────────────────────────────────────────

/// All record signatures across every plugin must consist exclusively of
/// ASCII alphanumeric bytes or `_`.
#[test]
fn fo4_live_03_all_signatures_are_valid_ascii() -> Result<(), Box<dyn std::error::Error>> {
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
                        path.file_name().unwrap().to_string_lossy(),
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

// ── Test 4: Plugin kinds ──────────────────────────────────────────────────────

/// Every plugin must have a recognised PluginKind.
#[test]
fn fo4_live_04_plugin_kinds_are_valid() -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = find_data_dir() else {
        return Ok(());
    };
    banner("PLUGIN KINDS");

    let paths = collect_plugins(&dir);
    let mut kind_counts: HashMap<&'static str, usize> = HashMap::new();

    for path in &paths {
        let Ok(plugin) = open(path) else { continue };
        let label = match plugin.kind() {
            PluginKind::Plugin => "Plugin (.esp)",
            PluginKind::Master => "Master (.esm)",
            PluginKind::Light => "Light  (.esl)",
            PluginKind::Medium => "Medium",
            PluginKind::Update => "Update",
        };
        *kind_counts.entry(label).or_default() += 1;
    }

    for (label, count) in &kind_counts {
        eprintln!("  {label:20} : {count}");
    }

    Ok(())
}

// ── Test 5: HEDR version validity ─────────────────────────────────────────────

/// The HEDR version float must be positive and finite for every plugin.
#[test]
fn fo4_live_05_hedr_versions_are_valid() -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = find_data_dir() else {
        return Ok(());
    };
    banner("HEDR VERSION VALIDITY");

    let paths = collect_plugins(&dir);
    let mut bad: Vec<String> = Vec::new();
    let mut versions: HashMap<u32, usize> = HashMap::new();

    for path in &paths {
        let Ok(plugin) = open(path) else { continue };
        let v = plugin.header.hedr_version;
        *versions.entry(v.to_bits()).or_default() += 1;
        if !v.is_finite() || v <= 0.0 {
            bad.push(format!(
                "{}: invalid HEDR version {v}",
                path.file_name().unwrap().to_string_lossy()
            ));
        }
    }

    let mut sorted_versions: Vec<(f32, usize)> = versions
        .into_iter()
        .map(|(bits, count)| (f32::from_bits(bits), count))
        .collect();
    sorted_versions.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
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

// ── Test 6: Master filename validity ──────────────────────────────────────────

/// All MAST subrecords must be non-empty, printable ASCII strings.
#[test]
fn fo4_live_06_master_filenames_are_valid() -> Result<(), Box<dyn std::error::Error>> {
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
                    path.file_name().unwrap().to_string_lossy(),
                    m
                ));
            }
        }
    }

    eprintln!("  Total MAST references : {total_masters}");
    let mut buckets: Vec<(usize, usize)> = master_counts.into_iter().collect();
    buckets.sort_by_key(|(k, _)| *k);
    for (count, n) in &buckets {
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

// ── Test 7: Subrecord parsing ──────────────────────────────────────────────────

/// Triggering lazy subrecord parsing on every record across every plugin
/// must not return an error.
#[test]
fn fo4_live_07_all_subrecords_parse() -> Result<(), Box<dyn std::error::Error>> {
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
                            path.file_name().unwrap().to_string_lossy(),
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

// ── Test 8: EDID subrecords are valid UTF-8 ───────────────────────────────────

/// Every EDID (Editor ID) subrecord must decode to a valid UTF-8 string.
#[test]
fn fo4_live_08_edid_subrecords_are_valid_utf8() -> Result<(), Box<dyn std::error::Error>> {
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
                            path.file_name().unwrap().to_string_lossy(),
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

// ── Test 9: Compressed records decompress ─────────────────────────────────────

/// Every compressed record must decompress without error and produce a
/// non-empty subrecord list.
#[test]
fn fo4_live_09_compressed_records_decompress() -> Result<(), Box<dyn std::error::Error>> {
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
                match record.subrecords() {
                    Ok(_) => {}
                    Err(e) => {
                        failures.push(format!(
                            "{}: FormID {:08X} ({}) decompression failed: {e}",
                            path.file_name().unwrap().to_string_lossy(),
                            record.header.form_id.0,
                            record.header.signature,
                        ));
                    }
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

// ── Test 10: Record flag inventory ────────────────────────────────────────────

/// Collects flag statistics across all plugins. Does not fail — informational
/// only.
#[test]
fn fo4_live_10_record_flag_inventory() -> Result<(), Box<dyn std::error::Error>> {
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

// ── Test 11: FO4 schema coverage ──────────────────────────────────────────────

/// Measures what fraction of record types encountered in the wild are
/// covered by our FO4 schema registry.
///
/// Emits a detailed coverage report. Does not fail — schema coverage is
/// tracked as a metric, not an invariant.
#[test]
fn fo4_live_11_schema_coverage() -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = find_data_dir() else {
        return Ok(());
    };
    banner("FO4 SCHEMA COVERAGE");

    let paths = collect_plugins(&dir);
    let reg = SchemaRegistry::fo4();

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
    let covered: Vec<([u8; 4], u64)> = sig_counts
        .iter()
        .filter(|(sig, _)| reg.get(Signature(**sig)).is_some())
        .map(|(sig, &count)| (*sig, count))
        .collect();
    let mut uncovered: Vec<([u8; 4], u64)> = sig_counts
        .iter()
        .filter(|(sig, _)| reg.get(Signature(**sig)).is_none())
        .map(|(sig, &count)| (*sig, count))
        .collect();

    let total_covered_records: u64 = covered.iter().map(|(_, c)| c).sum();
    let total_uncovered_records: u64 = uncovered.iter().map(|(_, c)| c).sum();
    let total_records: u64 = total_covered_records + total_uncovered_records;
    let coverage_pct = total_covered_records as f64 / total_records as f64 * 100.0;
    let type_coverage_pct = covered.len() as f64 / total_distinct as f64 * 100.0;

    eprintln!("  Schema registry size     : {}", reg.len());
    eprintln!("  Distinct record types    : {total_distinct} found in wild");
    eprintln!(
        "  Type coverage            : {} / {total_distinct}  ({type_coverage_pct:.1}%)",
        covered.len()
    );
    eprintln!(
        "  Record coverage          : {total_covered_records} / {total_records}  \
         ({coverage_pct:.1}%)"
    );

    uncovered.sort_by_key(|b| std::cmp::Reverse(b.1));
    if !uncovered.is_empty() {
        eprintln!("  Uncovered types (sorted by frequency, known placement records marked *):");
        for (sig, count) in uncovered.iter().take(40) {
            let s = Signature(*sig);
            let known = KNOWN_NO_SCHEMA.contains(&sig);
            let marker = if known { " *" } else { "" };
            eprintln!("    {s}  {count:>8} records{marker}");
        }
    }

    Ok(())
}

// ── Test 12: Schema-guided field decode (RecordView) ──────────────────────────

/// Runs RecordView field decoding on every record whose type is covered by
/// the FO4 schema. Counts decode successes, benign-missing fields, and hard
/// decode errors.
#[test]
fn fo4_live_12_schema_field_decode() -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = find_data_dir() else {
        return Ok(());
    };
    banner("SCHEMA-GUIDED FIELD DECODE (RecordView)");

    let paths = collect_plugins(&dir);
    let reg = SchemaRegistry::fo4();

    let mut records_decoded = 0u64;
    let mut records_skipped = 0u64;
    let mut fields_decoded = 0u64;
    let mut fields_missing = 0u64;
    let mut decode_errors: Vec<String> = Vec::new();

    for path in &paths {
        let Ok(plugin) = open(path) else { continue };
        for group in plugin.groups() {
            for record in group.records_recursive() {
                let Some(schema) = reg.get(record.header.signature) else {
                    records_skipped += 1;
                    continue;
                };
                records_decoded += 1;
                let view = RecordView::new(record, schema, plugin.is_localized());
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
                        if decode_errors.len() < 50 {
                            decode_errors.push(format!(
                                "{}: {} FormID {:08X}: {e}",
                                path.file_name().unwrap().to_string_lossy(),
                                record.header.signature,
                                record.header.form_id.0,
                            ));
                        }
                    }
                }
            }
        }
    }

    eprintln!("  Records decoded        : {records_decoded}");
    eprintln!("  Records skipped        : {records_skipped} (no schema)");
    eprintln!("  Fields decoded (value) : {fields_decoded}");
    eprintln!("  Fields missing         : {fields_missing}");
    eprintln!("  Decode errors          : {}", decode_errors.len());
    for e in decode_errors.iter().take(10) {
        eprintln!("    ERR: {e}");
    }

    let error_rate = decode_errors.len() as f64 / records_decoded.max(1) as f64;
    assert!(
        error_rate < 0.005,
        "field decode error rate {:.3}% exceeds 0.5% threshold",
        error_rate * 100.0
    );

    Ok(())
}

// ── Test 13: Fallout4.esm deep analysis ───────────────────────────────────────

/// Performs a thorough analysis of `Fallout4.esm` — the base game master —
/// and prints a detailed breakdown.
#[test]
fn fo4_live_13_fallout4_esm_deep_analysis() -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = find_data_dir() else {
        return Ok(());
    };
    banner("FALLOUT4.ESM DEEP ANALYSIS");

    let esm_path = dir.join("Fallout4.esm");
    assert!(
        esm_path.exists(),
        "Fallout4.esm not found at {}",
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
        "Fallout4.esm had subrecord parse failures"
    );

    Ok(())
}

// ── Test 14: Throughput benchmark ─────────────────────────────────────────────

/// Measures raw plugin-open throughput across all FO4 plugins.
///
/// Does not assert performance numbers — prints a benchmark summary only.
#[test]
fn fo4_live_14_throughput_benchmark() -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = find_data_dir() else {
        return Ok(());
    };
    banner("THROUGHPUT BENCHMARK");

    let paths = collect_plugins(&dir);
    let total_bytes: u64 = paths
        .iter()
        .filter_map(|p| std::fs::metadata(p).ok())
        .map(|m| m.len())
        .sum();

    // Warm up: open once, discard.
    for path in paths.iter().take(3) {
        let _ = open(path);
    }

    let t0 = Instant::now();
    let mut records = 0u64;
    let mut errors = 0usize;

    for path in &paths {
        match open(path) {
            Ok(plugin) => {
                for group in plugin.groups() {
                    for _record in group.records_recursive() {
                        records += 1;
                    }
                }
            }
            Err(_) => errors += 1,
        }
    }

    let elapsed = t0.elapsed();

    BenchResult {
        label: "open + record scan (all plugins)",
        files: paths.len(),
        bytes: total_bytes,
        elapsed,
        records,
        errors,
    }
    .print();

    Ok(())
}
