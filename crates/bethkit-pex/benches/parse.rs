// SPDX-License-Identifier: Apache-2.0
//! Parse throughput over a real script corpus.
//!
//! # Precondition
//!
//! Set `BETHKIT_PEX_BENCH` to one or more `;`-separated folders. They are
//! searched recursively for loose `.pex` files and for `.bsa`/`.ba2` archives
//! containing scripts. All scripts are loaded into memory first, so only
//! parsing is measured. Without the variable the benchmark is skipped.
//!
//! # Run
//!
//! ```text
//! BETHKIT_PEX_BENCH="<mods folder>" cargo bench -p bethkit-pex --bench parse
//! ```

use std::path::{Path, PathBuf};
use std::time::Instant;

use bethkit_pex::{PexFile, PexView};
use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use rayon::prelude::*;

fn has_extension(path: &Path, extensions: &[&str]) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| extensions.iter().any(|e| ext.eq_ignore_ascii_case(e)))
}

/// Loads all scripts below `dir`; unreadable files and archives are skipped.
fn collect(dir: &Path, scripts: &mut Vec<Vec<u8>>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path: PathBuf = entry.path();
        if path.is_dir() {
            collect(&path, scripts);
        } else if has_extension(&path, &["pex"]) {
            if let Ok(bytes) = std::fs::read(&path) {
                scripts.push(bytes);
            }
        } else if has_extension(&path, &["bsa", "ba2"]) {
            let Ok(archive) = bethkit_bsa::open(&path) else {
                continue;
            };
            for entry in archive.entries() {
                if entry.path.ends_with(".pex") {
                    if let Some(Ok(bytes)) = archive.extract(&entry.path) {
                        scripts.push(bytes.into_owned());
                    }
                }
            }
        }
    }
}

/// Parses every script and returns how many succeeded.
fn parse_all(scripts: &[Vec<u8>]) -> usize {
    scripts
        .iter()
        .filter(|bytes| PexFile::parse(bytes).is_ok())
        .count()
}

fn parse(c: &mut Criterion) {
    let Some(dirs) = std::env::var_os("BETHKIT_PEX_BENCH") else {
        println!("SKIP: BETHKIT_PEX_BENCH is not set");
        return;
    };
    let mut scripts: Vec<Vec<u8>> = Vec::new();
    for dir in std::env::split_paths(&dirs) {
        collect(&dir, &mut scripts);
    }
    let bytes: u64 = scripts.iter().map(|s| s.len() as u64).sum();

    let started = Instant::now();
    let parallel: usize = scripts
        .par_iter()
        .filter(|bytes| PexFile::parse(bytes).is_ok())
        .count();
    println!(
        "{} scripts ({:.1} MB), {parallel} parsed in parallel in {:?}",
        scripts.len(),
        bytes as f64 / 1e6,
        started.elapsed()
    );

    let mut group = c.benchmark_group("pex");
    group.sample_size(10);
    group.throughput(Throughput::Bytes(bytes));
    group.bench_function(format!("parse_{}_scripts", scripts.len()), |bencher| {
        bencher.iter(|| parse_all(&scripts));
    });
    group.bench_function(format!("view_{}_scripts", scripts.len()), |bencher| {
        bencher.iter(|| {
            scripts
                .iter()
                .filter(|bytes| PexView::parse(bytes).is_ok())
                .count()
        });
    });
    group.finish();
}

criterion_group!(benches, parse);
criterion_main!(benches);
