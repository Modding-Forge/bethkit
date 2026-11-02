// SPDX-License-Identifier: Apache-2.0
//!
//! Live parser corpus suite against real Papyrus sources.
//!
//! # Precondition
//!
//! Set one or more of these environment variables to a directory containing
//! `.psc` files (searched recursively), for example the extracted
//! `Scripts.zip` of the Creation Kit:
//!
//! | Variable                    | Dialect   |
//! |-----------------------------|-----------|
//! | `PAPYRUS_SKYRIM_SOURCES`    | Skyrim    |
//! | `PAPYRUS_FALLOUT4_SOURCES`  | Fallout 4 |
//! | `PAPYRUS_FALLOUT76_SOURCES` | Fallout 76|
//! | `PAPYRUS_STARFIELD_SOURCES` | Starfield |
//!
//! Unset variables skip their part of the suite, so CI passes without game
//! files. Every source must parse without error diagnostics.
//!
//! # Run
//!
//! ```text
//! cargo test -p bethkit-papyrus --release --test corpus_live -- --nocapture
//! ```

use std::path::{Path, PathBuf};
use std::time::Instant;

use bethkit_papyrus::{parse, Dialect, FlagFile, LineIndex, ParsedScript};
use rayon::prelude::*;

const CORPORA: [(&str, Dialect); 4] = [
    ("PAPYRUS_SKYRIM_SOURCES", Dialect::Skyrim),
    ("PAPYRUS_FALLOUT4_SOURCES", Dialect::Fallout4),
    ("PAPYRUS_FALLOUT76_SOURCES", Dialect::Fallout76),
    ("PAPYRUS_STARFIELD_SOURCES", Dialect::Starfield),
];

/// Collects files with `extension` below `dir`, case-insensitively.
fn collect(dir: &Path, extension: &str, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path: PathBuf = entry?.path();
        if path.is_dir() {
            collect(&path, extension, files)?;
        } else if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case(extension))
        {
            files.push(path);
        }
    }
    Ok(())
}

/// Renders the error diagnostics of one file with line and column numbers.
fn describe_errors(path: &Path, source: &[u8], parsed: &ParsedScript) -> String {
    let lines = LineIndex::new(source);
    parsed
        .diagnostics()
        .iter()
        .take(5)
        .map(|diagnostic| {
            let (line, column) = lines.line_col(diagnostic.span().start());
            format!("{}:{line}:{column}: {diagnostic}", path.display())
        })
        .collect::<Vec<String>>()
        .join("\n")
}

/// Parses every source of every configured corpus and requires zero errors.
#[test]
fn corpus_sources_parse_without_errors() -> Result<(), Box<dyn std::error::Error>> {
    for (variable, dialect) in CORPORA {
        let Some(dir) = std::env::var_os(variable).map(PathBuf::from) else {
            println!("SKIP: {variable} is not set");
            continue;
        };

        // given
        let mut files: Vec<PathBuf> = Vec::new();
        collect(&dir, "psc", &mut files)?;
        let sources: Vec<(PathBuf, Vec<u8>)> = files
            .into_iter()
            .map(|path| std::fs::read(&path).map(|bytes| (path, bytes)))
            .collect::<std::io::Result<_>>()?;
        let bytes: usize = sources.iter().map(|(_, source)| source.len()).sum();

        // when
        let started = Instant::now();
        let failures: Vec<String> = sources
            .par_iter()
            .filter_map(|(path, source)| match parse(source, dialect) {
                Ok(parsed) if !parsed.has_errors() => None,
                Ok(parsed) => Some(describe_errors(path, source, &parsed)),
                Err(error) => Some(format!("{}: {error}", path.display())),
            })
            .collect();
        let elapsed = started.elapsed();

        // then
        println!(
            "{dialect}: parsed {} files ({:.1} MiB) in {elapsed:?}, {} with errors",
            sources.len(),
            bytes as f64 / 1_048_576.0,
            failures.len()
        );
        for failure in failures.iter().take(20) {
            println!("{failure}");
        }
        assert!(
            failures.is_empty(),
            "{} {dialect} sources failed to parse",
            failures.len()
        );

        let mut flag_files: Vec<PathBuf> = Vec::new();
        collect(&dir, "flg", &mut flag_files)?;
        for path in flag_files {
            let flags: FlagFile = FlagFile::parse(&std::fs::read(&path)?)?;
            assert!(
                !flags.has_errors(),
                "{}: {:?}",
                path.display(),
                flags.diagnostics()
            );
            assert!(
                !flags.flags().is_empty(),
                "{} defines no flags",
                path.display()
            );
        }
    }
    Ok(())
}
