// SPDX-License-Identifier: Apache-2.0
//!
//! Converts one normalized xEdit export into a candidate schema package.

use std::env;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::{collections::BTreeMap, collections::BTreeSet};

use bethkit_schema::{
    convert_xedit_export, CallbackClass, CallbackRule, CallbackRuleAction, ConversionRules,
    ExportedCallback, ImplementationRule, SchemaGame, SchemaNode, SchemaNodeKind, XEditExport,
};
use serde::{Deserialize, Serialize};
use sha2::Digest;

#[derive(Deserialize, Serialize)]
struct CallbackInventory {
    format_version: u32,
    exports: usize,
    records: usize,
    callbacks: Vec<InventoryCallback>,
    #[serde(default)]
    custom_decoders: Vec<InventoryDecoder>,
}

#[derive(Deserialize, Serialize)]
struct InventoryCallback {
    path: String,
    callback_id: String,
    callback_slot: Option<u32>,
    semantic: bool,
    implementation_fingerprint: String,
    implementation_symbol: String,
    implementation_unit: String,
    implementation_source_line: Option<u32>,
    games: Vec<SchemaGame>,
}

#[derive(Deserialize, Serialize)]
struct InventoryDecoder {
    path: String,
    decoder: String,
    games: Vec<SchemaGame>,
}

struct CallbackAggregate {
    semantic: bool,
    implementation_symbol: String,
    implementation_unit: String,
    implementation_source_line: Option<u32>,
    games: BTreeSet<SchemaGame>,
}

struct UnclassifiedGroup {
    implementation_symbol: String,
    implementation_unit: String,
    paths_by_game: BTreeMap<SchemaGame, Vec<String>>,
}

#[derive(Serialize)]
struct CallbackAudit {
    format_version: u32,
    callback_definitions: usize,
    callback_game_bindings: usize,
    explicit_rule_bindings: usize,
    implementation_rule_bindings: usize,
    derived_custom_bindings: usize,
    unclassified_bindings: usize,
    completely_classified_definitions: usize,
    unclassified: Vec<UnclassifiedCallback>,
    unclassified_implementations: Vec<UnclassifiedImplementation>,
    unused_rules: Vec<UnusedRule>,
    unused_implementation_rules: Vec<UnusedImplementationRule>,
}

#[derive(Serialize)]
struct UnclassifiedCallback {
    path: String,
    callback_id: String,
    callback_slot: Option<u32>,
    game: SchemaGame,
    semantic: bool,
    implementation_fingerprint: String,
    implementation_symbol: String,
}

#[derive(Serialize)]
struct UnclassifiedImplementation {
    callback_id: String,
    implementation_fingerprint: String,
    implementation_symbol: String,
    implementation_unit: String,
    bindings: usize,
    matches: Vec<UnclassifiedImplementationMatch>,
}

#[derive(Serialize)]
struct UnclassifiedImplementationMatch {
    game: SchemaGame,
    expected_match_count: usize,
    expected_paths_sha256: String,
}

#[derive(Serialize)]
struct UnusedRule {
    path: String,
    callback_id: String,
    callback_slot: Option<u32>,
}

#[derive(Serialize)]
struct UnusedImplementationRule {
    callback_id: String,
    implementation_fingerprint: String,
}

fn main() -> Result<(), Box<dyn Error>> {
    let arguments: Vec<String> = env::args().skip(1).collect();
    if arguments.first().map(String::as_str) == Some("inventory") {
        return write_inventory(&arguments[1..]);
    }
    if arguments.first().map(String::as_str) == Some("convert") {
        return convert(&arguments[1..]);
    }
    if arguments.first().map(String::as_str) == Some("classify-ui") {
        return classify_ui(&arguments[1..]);
    }
    if arguments.first().map(String::as_str) == Some("audit") {
        return audit(&arguments[1..]);
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
    let mut indexed: BTreeMap<(String, String, Option<u32>, String), CallbackAggregate> =
        BTreeMap::new();
    let mut decoders: BTreeMap<(String, String), BTreeSet<SchemaGame>> = BTreeMap::new();
    let mut record_count: usize = 0;
    for input in &arguments[1..] {
        let export: XEditExport = read_json(Path::new(input))?;
        record_count = record_count
            .checked_add(export.records.len())
            .ok_or("record count overflow")?;
        for record in &export.records {
            collect_decoders(&record.root, export.game, &mut decoders);
        }
        for callback in export.callbacks {
            add_callback(&mut indexed, export.game, callback)?;
        }
    }
    let callbacks = indexed
        .into_iter()
        .map(
            |((path, callback_id, callback_slot, implementation_fingerprint), aggregate)| {
                InventoryCallback {
                    path,
                    callback_id,
                    callback_slot,
                    semantic: aggregate.semantic,
                    implementation_fingerprint,
                    implementation_symbol: aggregate.implementation_symbol,
                    implementation_unit: aggregate.implementation_unit,
                    implementation_source_line: aggregate.implementation_source_line,
                    games: aggregate.games.into_iter().collect(),
                }
            },
        )
        .collect();
    let custom_decoders = decoders
        .into_iter()
        .map(|((path, decoder), games)| InventoryDecoder {
            path,
            decoder,
            games: games.into_iter().collect(),
        })
        .collect();
    let inventory = CallbackInventory {
        format_version: 2,
        exports: arguments.len() - 1,
        records: record_count,
        callbacks,
        custom_decoders,
    };
    fs::write(output_path, serde_json::to_vec_pretty(&inventory)?)?;
    Ok(())
}

fn classify_ui(arguments: &[String]) -> Result<(), Box<dyn Error>> {
    if arguments.len() != 3 {
        return Err(usage().into());
    }
    let inventory: CallbackInventory = read_json(Path::new(&arguments[0]))?;
    let rules: ConversionRules = read_json(Path::new(&arguments[1]))?;
    let merged = merge_ui_rules(&inventory, rules)?;
    let mut bytes = serde_json::to_vec_pretty(&merged)?;
    bytes.push(b'\n');
    fs::write(&arguments[2], bytes)?;
    Ok(())
}

fn audit(arguments: &[String]) -> Result<(), Box<dyn Error>> {
    if arguments.len() != 3 {
        return Err(usage().into());
    }
    let inventory: CallbackInventory = read_json(Path::new(&arguments[0]))?;
    let rules: ConversionRules = read_json(Path::new(&arguments[1]))?;
    let report = audit_rules(&inventory, &rules)?;
    let mut bytes = serde_json::to_vec_pretty(&report)?;
    bytes.push(b'\n');
    fs::write(&arguments[2], bytes)?;
    Ok(())
}

fn audit_rules(
    inventory: &CallbackInventory,
    rules: &ConversionRules,
) -> Result<CallbackAudit, Box<dyn Error>> {
    if inventory.format_version != 2 || rules.format_version != 2 {
        return Err("unsupported inventory or rule format version".into());
    }
    let mut rule_keys: BTreeSet<(String, String, Option<u32>)> = BTreeSet::new();
    for rule in &rules.callbacks {
        let key = (
            rule.path.clone(),
            rule.callback_id.clone(),
            rule.callback_slot,
        );
        if !rule_keys.insert(key.clone()) {
            return Err(format!("duplicate rule {} at {}", key.1, key.0).into());
        }
    }
    let mut implementation_rules: BTreeMap<(String, String), &ImplementationRule> = BTreeMap::new();
    for rule in &rules.implementations {
        let key = (
            rule.implementation_fingerprint.clone(),
            rule.callback_id.clone(),
        );
        if implementation_rules.insert(key.clone(), rule).is_some() {
            return Err(format!("duplicate implementation rule {} for {}", key.0, key.1).into());
        }
    }
    let mut custom_games: BTreeMap<String, BTreeSet<SchemaGame>> = BTreeMap::new();
    for decoder in &inventory.custom_decoders {
        custom_games
            .entry(decoder.path.clone())
            .or_default()
            .extend(decoder.games.iter().copied());
    }

    let mut callback_game_bindings: usize = 0;
    let mut explicit_rule_bindings: usize = 0;
    let mut implementation_rule_bindings: usize = 0;
    let mut derived_custom_bindings: usize = 0;
    let mut completely_classified_definitions: usize = 0;
    let mut unclassified: Vec<UnclassifiedCallback> = Vec::new();
    let mut unclassified_groups: BTreeMap<(String, String), UnclassifiedGroup> = BTreeMap::new();
    for callback in &inventory.callbacks {
        let exact_key = (
            callback.path.clone(),
            callback.callback_id.clone(),
            callback.callback_slot,
        );
        let wildcard_key = (callback.path.clone(), callback.callback_id.clone(), None);
        let explicit = rule_keys.contains(&exact_key) || rule_keys.contains(&wildcard_key);
        let implementation_key = (
            callback.implementation_fingerprint.clone(),
            callback.callback_id.clone(),
        );
        let implementation_rule = implementation_rules.get(&implementation_key);
        let mut definition_complete = true;
        for game in &callback.games {
            callback_game_bindings += 1;
            if explicit {
                explicit_rule_bindings += 1;
                continue;
            }
            if implementation_rule
                .is_some_and(|rule| rule.matches.iter().any(|expected| expected.game == *game))
            {
                implementation_rule_bindings += 1;
                continue;
            }
            if callback.semantic
                && custom_games
                    .get(&callback.path)
                    .is_some_and(|games| games.contains(game))
            {
                derived_custom_bindings += 1;
                continue;
            }
            definition_complete = false;
            unclassified.push(UnclassifiedCallback {
                path: callback.path.clone(),
                callback_id: callback.callback_id.clone(),
                callback_slot: callback.callback_slot,
                game: *game,
                semantic: callback.semantic,
                implementation_fingerprint: callback.implementation_fingerprint.clone(),
                implementation_symbol: callback.implementation_symbol.clone(),
            });
            let group = unclassified_groups
                .entry(implementation_key.clone())
                .or_insert_with(|| UnclassifiedGroup {
                    implementation_symbol: callback.implementation_symbol.clone(),
                    implementation_unit: callback.implementation_unit.clone(),
                    paths_by_game: BTreeMap::new(),
                });
            group.paths_by_game.entry(*game).or_default().push(format!(
                "{}|{}",
                callback
                    .callback_slot
                    .map_or_else(|| "*".to_owned(), |slot| slot.to_string()),
                callback.path
            ));
        }
        if definition_complete {
            completely_classified_definitions += 1;
        }
    }
    let unused_rules = rules
        .callbacks
        .iter()
        .filter(|rule| {
            !inventory.callbacks.iter().any(|callback| {
                callback.path == rule.path
                    && callback.callback_id == rule.callback_id
                    && (rule.callback_slot.is_none()
                        || callback.callback_slot == rule.callback_slot)
            })
        })
        .map(|rule| UnusedRule {
            path: rule.path.clone(),
            callback_id: rule.callback_id.clone(),
            callback_slot: rule.callback_slot,
        })
        .collect();
    let inventory_implementation_keys: BTreeSet<(String, String)> = inventory
        .callbacks
        .iter()
        .map(|callback| {
            (
                callback.implementation_fingerprint.clone(),
                callback.callback_id.clone(),
            )
        })
        .collect();
    let unused_implementation_rules = implementation_rules
        .keys()
        .filter(|key| !inventory_implementation_keys.contains(*key))
        .map(
            |(implementation_fingerprint, callback_id)| UnusedImplementationRule {
                callback_id: callback_id.clone(),
                implementation_fingerprint: implementation_fingerprint.clone(),
            },
        )
        .collect();
    let unclassified_implementations = unclassified_groups
        .into_iter()
        .map(|((implementation_fingerprint, callback_id), group)| {
            let bindings = group.paths_by_game.values().map(Vec::len).sum();
            let matches = group
                .paths_by_game
                .into_iter()
                .map(|(game, mut paths)| {
                    paths.sort();
                    let mut canonical = paths.join("\n");
                    canonical.push('\n');
                    UnclassifiedImplementationMatch {
                        game,
                        expected_match_count: paths.len(),
                        expected_paths_sha256: hex::encode(sha2::Sha256::digest(
                            canonical.as_bytes(),
                        )),
                    }
                })
                .collect();
            UnclassifiedImplementation {
                callback_id,
                implementation_fingerprint,
                implementation_symbol: group.implementation_symbol,
                implementation_unit: group.implementation_unit,
                bindings,
                matches,
            }
        })
        .collect();
    Ok(CallbackAudit {
        format_version: 2,
        callback_definitions: inventory.callbacks.len(),
        callback_game_bindings,
        explicit_rule_bindings,
        implementation_rule_bindings,
        derived_custom_bindings,
        unclassified_bindings: unclassified.len(),
        completely_classified_definitions,
        unclassified,
        unclassified_implementations,
        unused_rules,
        unused_implementation_rules,
    })
}

fn merge_ui_rules(
    inventory: &CallbackInventory,
    rules: ConversionRules,
) -> Result<ConversionRules, Box<dyn Error>> {
    if inventory.format_version != 2 || rules.format_version != 2 {
        return Err("unsupported inventory or rule format version".into());
    }
    let mut indexed: BTreeMap<(String, String, Option<u32>), CallbackRule> = BTreeMap::new();
    for rule in rules.callbacks {
        let key = (
            rule.path.clone(),
            rule.callback_id.clone(),
            rule.callback_slot,
        );
        if indexed.insert(key.clone(), rule).is_some() {
            return Err(format!("duplicate rule {} at {}", key.1, key.0).into());
        }
    }
    for callback in inventory
        .callbacks
        .iter()
        .filter(|callback| !callback.semantic)
    {
        let key = (callback.path.clone(), callback.callback_id.clone(), None);
        let generated = CallbackRule {
            path: callback.path.clone(),
            callback_id: callback.callback_id.clone(),
            callback_slot: None,
            action: CallbackRuleAction {
                classification: CallbackClass::UserInterfaceOnly,
                expression: None,
                built_in_operation: None,
                custom_handler: None,
                minimum_handler_version: None,
                rationale: "xEdit exposes this callback as presentation-only metadata.".to_owned(),
            },
        };
        if let Some(existing) = indexed.get(&key) {
            if existing.action.classification != CallbackClass::UserInterfaceOnly {
                return Err(format!(
                    "existing rule {} at {} conflicts with UI-only metadata",
                    callback.callback_id, callback.path
                )
                .into());
            }
            continue;
        }
        indexed.insert(key, generated);
    }
    Ok(ConversionRules {
        format_version: rules.format_version,
        callbacks: indexed.into_values().collect(),
        implementations: rules.implementations,
    })
}

fn collect_decoders(
    node: &SchemaNode,
    game: SchemaGame,
    indexed: &mut BTreeMap<(String, String), BTreeSet<SchemaGame>>,
) {
    match &node.kind {
        SchemaNodeKind::Sequence { children } => {
            for child in children {
                collect_decoders(child, game, indexed);
            }
        }
        SchemaNodeKind::Choice { alternatives } => {
            for alternative in alternatives {
                collect_decoders(alternative, game, indexed);
            }
        }
        SchemaNodeKind::Repeat { child, .. }
        | SchemaNodeKind::Subrecord { payload: child, .. }
        | SchemaNodeKind::Array { element: child, .. }
        | SchemaNodeKind::Compressed { child, .. } => collect_decoders(child, game, indexed),
        SchemaNodeKind::Struct { fields } => {
            for field in fields {
                collect_decoders(field, game, indexed);
            }
        }
        SchemaNodeKind::Union { variants, .. } => {
            for variant in variants {
                collect_decoders(variant, game, indexed);
            }
        }
        SchemaNodeKind::Custom { decoder, .. } => {
            indexed
                .entry((node.path.clone(), decoder.clone()))
                .or_default()
                .insert(game);
        }
        SchemaNodeKind::Primitive { .. } | SchemaNodeKind::Reference { .. } => {}
    }
}

fn add_callback(
    indexed: &mut BTreeMap<(String, String, Option<u32>, String), CallbackAggregate>,
    game: SchemaGame,
    callback: ExportedCallback,
) -> Result<(), Box<dyn Error>> {
    let key = (
        callback.path,
        callback.callback_id,
        callback.callback_slot,
        callback.implementation_fingerprint,
    );
    let entry = indexed
        .entry(key.clone())
        .or_insert_with(|| CallbackAggregate {
            semantic: callback.semantic,
            implementation_symbol: callback.implementation_symbol.clone(),
            implementation_unit: callback.implementation_unit.clone(),
            implementation_source_line: callback.implementation_source_line,
            games: BTreeSet::new(),
        });
    if entry.semantic != callback.semantic {
        return Err(format!(
            "callback {} at {} changes semantic classification between games",
            key.1, key.0
        )
        .into());
    }
    if entry.implementation_symbol != callback.implementation_symbol
        || entry.implementation_unit != callback.implementation_unit
        || entry.implementation_source_line != callback.implementation_source_line
    {
        return Err(format!(
            "callback implementation metadata changed for {} at {}",
            key.1, key.0
        )
        .into());
    }
    entry.games.insert(game);
    Ok(())
}

fn usage() -> &'static str {
    "usage:\n  bethkit-xedit-converter [convert] <export.json> <rules.json> \
     <output.bkschema>\n  bethkit-xedit-converter inventory <output.json> <export.json>...\n  \
     bethkit-xedit-converter classify-ui <inventory.json> <input-rules.json> \
     <output-rules.json>\n  bethkit-xedit-converter audit <inventory.json> <rules.json> \
     <output-report.json>"
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

    fn inventory_callback(
        path: &str,
        callback_id: &str,
        semantic: bool,
        games: Vec<SchemaGame>,
    ) -> InventoryCallback {
        InventoryCallback {
            path: path.to_owned(),
            callback_id: callback_id.to_owned(),
            callback_slot: None,
            semantic,
            implementation_fingerprint: "aa".repeat(32),
            implementation_symbol: "Test.Callback".to_owned(),
            implementation_unit: "Test".to_owned(),
            implementation_source_line: None,
            games,
        }
    }

    fn exported_callback() -> ExportedCallback {
        ExportedCallback {
            path: "NPC_/0:DATA".to_owned(),
            callback_id: "def.after_load".to_owned(),
            callback_slot: None,
            semantic: true,
            implementation_fingerprint: "aa".repeat(32),
            implementation_symbol: "Test.Callback".to_owned(),
            implementation_unit: "Test".to_owned(),
            implementation_source_line: None,
        }
    }

    fn ui_action() -> CallbackRuleAction {
        CallbackRuleAction {
            classification: CallbackClass::UserInterfaceOnly,
            expression: None,
            built_in_operation: None,
            custom_handler: None,
            minimum_handler_version: None,
            rationale: "Presentation only.".to_owned(),
        }
    }

    /// Verifies per-game audit accounting for rules, custom nodes, and gaps.
    #[test]
    fn callback_audit_reports_each_game_binding(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let inventory = CallbackInventory {
            format_version: 2,
            exports: 2,
            records: 1,
            callbacks: vec![
                inventory_callback(
                    "TEST/ui",
                    "def.dont_show",
                    false,
                    vec![SchemaGame::SkyrimLe, SchemaGame::SkyrimSe],
                ),
                inventory_callback(
                    "TEST/custom",
                    "decoder.required",
                    true,
                    vec![SchemaGame::SkyrimLe, SchemaGame::SkyrimSe],
                ),
            ],
            custom_decoders: vec![InventoryDecoder {
                path: "TEST/custom".to_owned(),
                decoder: "xedit.test".to_owned(),
                games: vec![SchemaGame::SkyrimSe],
            }],
        };
        let rules = ConversionRules {
            format_version: 2,
            callbacks: vec![CallbackRule {
                path: "TEST/ui".to_owned(),
                callback_id: "def.dont_show".to_owned(),
                callback_slot: None,
                action: ui_action(),
            }],
            implementations: Vec::new(),
        };

        // when
        let audit = audit_rules(&inventory, &rules)?;

        // then
        assert_eq!(audit.callback_game_bindings, 4);
        assert_eq!(audit.explicit_rule_bindings, 2);
        assert_eq!(audit.derived_custom_bindings, 1);
        assert_eq!(audit.unclassified_bindings, 1);
        assert_eq!(audit.completely_classified_definitions, 1);
        assert_eq!(audit.unclassified[0].game, SchemaGame::SkyrimLe);
        Ok(())
    }

    /// Verifies that UI metadata creates exact rules without semantic callbacks.
    #[test]
    fn ui_rule_generation_is_strict() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let inventory = CallbackInventory {
            format_version: 2,
            exports: 1,
            records: 1,
            callbacks: vec![
                inventory_callback(
                    "TEST/ui",
                    "def.dont_show",
                    false,
                    vec![SchemaGame::SkyrimSe],
                ),
                inventory_callback(
                    "TEST/value",
                    "def.after_set",
                    true,
                    vec![SchemaGame::SkyrimSe],
                ),
            ],
            custom_decoders: Vec::new(),
        };
        let rules = ConversionRules {
            format_version: 2,
            callbacks: Vec::new(),
            implementations: Vec::new(),
        };

        // when
        let merged = merge_ui_rules(&inventory, rules)?;

        // then
        assert_eq!(merged.callbacks.len(), 1);
        assert_eq!(merged.callbacks[0].path, "TEST/ui");
        assert_eq!(
            merged.callbacks[0].action.classification,
            CallbackClass::UserInterfaceOnly
        );
        Ok(())
    }

    /// Verifies that custom decoder requirements are collected recursively.
    #[test]
    fn decoder_inventory_collects_nested_nodes(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let custom = SchemaNode {
            id: bethkit_schema::SchemaNodeId(1),
            path: "TEST/value".to_owned(),
            name: "Value".to_owned(),
            required: true,
            condition: None,
            kind: SchemaNodeKind::Custom {
                decoder: "xedit.test".to_owned(),
                configuration: serde_json::json!({}),
            },
        };
        let root = SchemaNode {
            id: bethkit_schema::SchemaNodeId(0),
            path: "TEST".to_owned(),
            name: "Test".to_owned(),
            required: true,
            condition: None,
            kind: SchemaNodeKind::Sequence {
                children: vec![custom],
            },
        };
        let mut indexed = BTreeMap::new();

        // when
        collect_decoders(&root, SchemaGame::SkyrimSe, &mut indexed);

        // then
        let games = indexed
            .get(&("TEST/value".to_owned(), "xedit.test".to_owned()))
            .ok_or("custom decoder was not indexed")?;
        assert_eq!(
            games.iter().copied().collect::<Vec<_>>(),
            vec![SchemaGame::SkyrimSe]
        );
        Ok(())
    }

    /// Verifies deterministic aggregation of one callback across games.
    #[test]
    fn callback_inventory_aggregates_games() {
        // given
        let mut indexed = BTreeMap::new();
        let callback = exported_callback;

        // when
        add_callback(&mut indexed, SchemaGame::SkyrimSe, callback())
            .expect("first callback should be accepted");
        add_callback(&mut indexed, SchemaGame::SkyrimLe, callback())
            .expect("matching callback should be accepted");

        // then
        let aggregate = indexed
            .get(&(
                "NPC_/0:DATA".to_owned(),
                "def.after_load".to_owned(),
                None,
                "aa".repeat(32),
            ))
            .expect("callback should be indexed");
        assert_eq!(
            aggregate.games.iter().copied().collect::<Vec<_>>(),
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
            ..exported_callback()
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
