// SPDX-License-Identifier: Apache-2.0
//! Separates native context creation and record decoding on private real corpora.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bethkit_core::{GameContext, Plugin};
use bethkit_schema::SchemaPackage;
use bethkit_semantic::{DecoderRegistry, SemanticContext};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let schema = PathBuf::from(arguments.next().ok_or("Expected schema path")?);
    let package = Arc::new(SchemaPackage::open(&schema)?);
    let limit = std::env::var("BETHKIT_PROFILE_RECORDS")
        .ok()
        .map(|value| value.parse::<usize>())
        .transpose()?
        .unwrap_or(512);
    for argument in arguments {
        let path = PathBuf::from(argument);
        let plugin = Plugin::open(&path, GameContext::sse())?;
        let started = Instant::now();
        let context = SemanticContext::new(package.clone(), DecoderRegistry::builtin())?;
        println!("context {:?}", started.elapsed());
        let mut totals: BTreeMap<String, (usize, Duration)> = BTreeMap::new();
        for record in plugin
            .groups()
            .iter()
            .flat_map(|g| g.records_recursive())
            .take(limit)
        {
            let started = Instant::now();
            let _fields = context.view(record, plugin.is_localized())?.fields()?;
            let entry = totals
                .entry(record.header.signature.to_string())
                .or_default();
            entry.0 += 1;
            entry.1 += started.elapsed();
        }
        println!("{} first {limit} records: {totals:?}", path.display());
        let started = Instant::now();
        let detected = SemanticContext::new_for_plugin(
            package.clone(),
            DecoderRegistry::builtin(),
            &plugin,
            None,
        )?;
        println!(
            "detector {:?}: {:?}",
            started.elapsed(),
            detected.plugin_encoding_decision()
        );
    }
    Ok(())
}
