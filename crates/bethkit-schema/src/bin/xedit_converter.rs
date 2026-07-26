// SPDX-License-Identifier: Apache-2.0
//!
//! Converts one normalized xEdit export into a candidate schema package.

use std::env;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::{collections::BTreeMap, collections::BTreeSet};

use bethkit_schema::{
    convert_xedit_export, ConversionRules, ExportedCallback, SchemaGame, XEditExport,
};
use serde::Serialize;

#[derive(Serialize)]
struct CallbackInventory {
    format_version: u32,
    exports: usize,
    records: usize,
    callbacks: Vec<InventoryCallback>,
}

#[derive(Serialize)]
struct InventoryCallback {
    path: String,
    callback_id: String,
    semantic: bool,
    games: Vec<SchemaGame>,
}

fn main() -> Result<(), Box<dyn Error>> {
    let arguments: Vec<String> = env::args().skip(1).collect();
    if arguments.first().map(String::as_str) == Some("inventory") {
        return write_inventory(&arguments[1..]);
    }
    if arguments.first().map(String::as_str) == Some("convert") {
        return convert(&arguments[1..]);
    }
    convert(&arguments)
}

fn convert(arguments: &[String]) -> Result<(), Box<dyn Error>> {
    if arguments.len() != 3 {
        return Err(usage().into());
    }
    let export_path = PathBuf::from(&arguments[0]);
    let rules_path = PathBuf::from(&arguments[1]);
    let output_path = PathBuf::from(&arguments[2]);
    let export: XEditExport = read_json(&export_path)?;
    let rules_bytes = fs::read(&rules_path)?;
    let rules: ConversionRules = serde_json::from_slice(&rules_bytes)?;
    let package = convert_xedit_export(export, &rules, &rules_bytes)?;
    fs::write(output_path, package.to_bytes()?)?;
    Ok(())
}

fn write_inventory(arguments: &[String]) -> Result<(), Box<dyn Error>> {
    if arguments.len() < 2 {
        return Err(usage().into());
    }
    let output_path = PathBuf::from(&arguments[0]);
    let mut indexed: BTreeMap<(String, String), (bool, BTreeSet<SchemaGame>)> = BTreeMap::new();
    let mut record_count: usize = 0;
    for input in &arguments[1..] {
        let export: XEditExport = read_json(Path::new(input))?;
        record_count = record_count
            .checked_add(export.records.len())
            .ok_or("record count overflow")?;
        for callback in export.callbacks {
            add_callback(&mut indexed, export.game, callback)?;
        }
    }
    let callbacks = indexed
        .into_iter()
        .map(
            |((path, callback_id), (semantic, games))| InventoryCallback {
                path,
                callback_id,
                semantic,
                games: games.into_iter().collect(),
            },
        )
        .collect();
    let inventory = CallbackInventory {
        format_version: 1,
        exports: arguments.len() - 1,
        records: record_count,
        callbacks,
    };
    fs::write(output_path, serde_json::to_vec_pretty(&inventory)?)?;
    Ok(())
}

fn add_callback(
    indexed: &mut BTreeMap<(String, String), (bool, BTreeSet<SchemaGame>)>,
    game: SchemaGame,
    callback: ExportedCallback,
) -> Result<(), Box<dyn Error>> {
    let key = (callback.path, callback.callback_id);
    let entry = indexed
        .entry(key.clone())
        .or_insert_with(|| (callback.semantic, BTreeSet::new()));
    if entry.0 != callback.semantic {
        return Err(format!(
            "callback {} at {} changes semantic classification between games",
            key.1, key.0
        )
        .into());
    }
    entry.1.insert(game);
    Ok(())
}

fn usage() -> &'static str {
    "usage:\n  bethkit-xedit-converter [convert] <export.json> <rules.json> \
     <output.bkschema>\n  bethkit-xedit-converter inventory <output.json> <export.json>..."
}

fn read_json<T>(path: &Path) -> Result<T, Box<dyn Error>>
where
    T: serde::de::DeserializeOwned,
{
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies deterministic aggregation of one callback across games.
    #[test]
    fn callback_inventory_aggregates_games() {
        // given
        let mut indexed = BTreeMap::new();
        let callback = || ExportedCallback {
            path: "NPC_/0:DATA".to_owned(),
            callback_id: "def.after_load".to_owned(),
            semantic: true,
        };

        // when
        add_callback(&mut indexed, SchemaGame::SkyrimSe, callback())
            .expect("first callback should be accepted");
        add_callback(&mut indexed, SchemaGame::SkyrimLe, callback())
            .expect("matching callback should be accepted");

        // then
        let (_, games) = indexed
            .get(&("NPC_/0:DATA".to_owned(), "def.after_load".to_owned()))
            .expect("callback should be indexed");
        assert_eq!(
            games.iter().copied().collect::<Vec<_>>(),
            vec![SchemaGame::SkyrimLe, SchemaGame::SkyrimSe]
        );
    }

    /// Verifies that an unstable semantic classification stops inventory.
    #[test]
    fn callback_inventory_rejects_semantic_mismatch() {
        // given
        let mut indexed = BTreeMap::new();
        let semantic = ExportedCallback {
            path: "TEST/root".to_owned(),
            callback_id: "test.callback".to_owned(),
            semantic: true,
        };
        let ui_only = ExportedCallback {
            semantic: false,
            ..semantic.clone()
        };
        add_callback(&mut indexed, SchemaGame::SkyrimLe, semantic)
            .expect("first callback should be accepted");

        // when
        let result = add_callback(&mut indexed, SchemaGame::SkyrimSe, ui_only);

        // then
        assert!(result.is_err());
    }
}
