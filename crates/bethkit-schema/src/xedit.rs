// SPDX-License-Identifier: Apache-2.0
//!
//! Checked conversion from the xEdit exporter contract.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    BuiltInOperation, CallbackBinding, CallbackClass, CallbackImplementation, DecoderRequirement,
    Expression, HandlerRequirement, Result, SchemaError, SchemaGame, SchemaManifest, SchemaNode,
    SchemaNodeKind, SchemaPackage, SchemaRecord, ValidationStatus, PACKAGE_FORMAT_VERSION,
};

/// Provenance emitted by the externally built xEdit exporter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExporterProvenance {
    /// Pinned xEdit release tag.
    pub source_tag: String,
    /// Full pinned xEdit commit.
    pub source_commit: String,
    /// SHA-256 of the pinned source archive.
    pub source_archive_sha256: String,
    /// Exporter contract implementation version.
    pub exporter_version: String,
    /// SHA-256 of the exporter executable.
    pub exporter_binary_sha256: String,
    /// SHA-256 of the detailed Delphi MAP paired with the executable.
    pub exporter_map_sha256: String,
    /// SHA-256 of the applied exporter patch set.
    pub exporter_patch_sha256: String,
    /// Hash identifying the Delphi build environment.
    pub exporter_build_sha256: String,
}

/// One callback discovered in the materialized xEdit definition graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportedCallback {
    /// Stable schema path at which the callback is attached.
    pub path: String,
    /// Stable callback identifier emitted by the patched exporter.
    pub callback_id: String,
    /// Optional slot for arrays of callbacks attached to one definition.
    pub callback_slot: Option<u32>,
    /// Whether the callback can affect binary interpretation or validation.
    pub semantic: bool,
    /// Build-bound callback implementation fingerprint.
    pub implementation_fingerprint: String,
    /// Detailed Delphi MAP symbol for the callback invoke address.
    pub implementation_symbol: String,
    /// Delphi unit containing the implementation symbol.
    pub implementation_unit: String,
    /// Source line when the detailed MAP provides one.
    pub implementation_source_line: Option<u32>,
}

/// Complete normalized result produced for one xEdit game mode.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct XEditExport {
    /// Export contract version. Version 1 is currently supported.
    pub contract_version: u32,
    /// Verified source and executable provenance.
    pub provenance: ExporterProvenance,
    /// Exported xEdit game mode.
    pub game: SchemaGame,
    /// Fully materialized record definitions.
    pub records: Vec<SchemaRecord>,
    /// Every dynamic callback encountered during export.
    pub callbacks: Vec<ExportedCallback>,
}

/// Checked path-based conversion rule for one callback.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallbackRule {
    /// Exact stable schema path.
    pub path: String,
    /// Exact exporter callback identifier.
    pub callback_id: String,
    /// Optional exact callback slot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub callback_slot: Option<u32>,
    /// Audited implementation assigned to this exact path.
    #[serde(flatten)]
    pub action: CallbackRuleAction,
}

/// Audited callback behavior shared by exact-path and implementation rules.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallbackRuleAction {
    /// Audited callback classification.
    pub classification: CallbackClass,
    /// Bounded expression, required for `declarative`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expression: Option<Expression>,
    /// Stable built-in operation, required for `built_in`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub built_in_operation: Option<BuiltInOperation>,
    /// Custom semantic handler identifier, required for `custom_handler`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_handler: Option<String>,
    /// Minimum custom handler version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum_handler_version: Option<u32>,
    /// Human-readable review note.
    pub rationale: String,
}

/// Expected per-game path set for one implementation fingerprint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImplementationRuleMatch {
    /// Game for which the path-set guard applies.
    pub game: SchemaGame,
    /// Expected number of callback bindings.
    pub expected_match_count: u32,
    /// SHA-256 of sorted `slot|path` entries.
    pub expected_paths_sha256: String,
}

/// Rule mapping one build-bound implementation to stable Bethkit behavior.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImplementationRule {
    /// Exact exporter implementation fingerprint.
    pub implementation_fingerprint: String,
    /// Exact callback role.
    pub callback_id: String,
    /// Guarded game-specific path sets.
    pub matches: Vec<ImplementationRuleMatch>,
    /// Audited behavior assigned to this implementation.
    #[serde(flatten)]
    pub action: CallbackRuleAction,
}

/// Versioned conversion-rule document tracked in the repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversionRules {
    /// Rule format version. Version 2 is currently supported.
    pub format_version: u32,
    /// Audited exact-path rules, reserved for presentation-only metadata.
    pub callbacks: Vec<CallbackRule>,
    /// Audited semantic implementation rules.
    #[serde(default)]
    pub implementations: Vec<ImplementationRule>,
}

/// Converts a normalized export into a validated candidate package.
///
/// `rules_bytes` must contain the exact tracked rule document used to create
/// `rules`; its digest is included in the package manifest.
///
/// # Errors
///
/// Returns [`SchemaError`] when a contract version is unsupported, a callback
/// is unclassified, a rule is ambiguous, custom decoder metadata is missing,
/// or the resulting package graph is invalid.
pub fn convert_xedit_export(
    export: XEditExport,
    rules: &ConversionRules,
    rules_bytes: &[u8],
) -> Result<SchemaPackage> {
    check_version(export.contract_version)?;
    check_version(rules.format_version)?;

    let mut indexed: BTreeMap<(String, String, Option<u32>), &CallbackRule> = BTreeMap::new();
    for rule in &rules.callbacks {
        if rule.action.classification != CallbackClass::UserInterfaceOnly {
            return Err(SchemaError::InvalidGraph(format!(
                "exact path rule {} at {} must be UI-only",
                rule.callback_id, rule.path
            )));
        }
        if indexed
            .insert(
                (
                    rule.path.clone(),
                    rule.callback_id.clone(),
                    rule.callback_slot,
                ),
                rule,
            )
            .is_some()
        {
            return Err(SchemaError::InvalidGraph(format!(
                "duplicate callback rule for {} at {}",
                rule.callback_id, rule.path
            )));
        }
    }
    let mut implementations: BTreeMap<(String, String), &ImplementationRule> = BTreeMap::new();
    for rule in &rules.implementations {
        let key = (
            rule.implementation_fingerprint.clone(),
            rule.callback_id.clone(),
        );
        if implementations.insert(key, rule).is_some() {
            return Err(SchemaError::InvalidGraph(format!(
                "duplicate implementation rule for {} fingerprint {}",
                rule.callback_id, rule.implementation_fingerprint
            )));
        }
    }
    validate_implementation_matches(&export, &rules.implementations)?;

    let mut missing: Vec<String> = Vec::new();
    let mut decoders: BTreeMap<String, u32> = BTreeMap::new();
    let mut handlers: BTreeMap<String, u32> = BTreeMap::new();
    let mut schema_decoders: BTreeMap<String, String> = BTreeMap::new();
    let mut callback_bindings: Vec<CallbackBinding> = Vec::with_capacity(export.callbacks.len());
    for record in &export.records {
        collect_schema_decoders(&record.root, &mut decoders, &mut schema_decoders)?;
    }
    for callback in &export.callbacks {
        validate_exported_callback(callback)?;
        let exact_key = (
            callback.path.clone(),
            callback.callback_id.clone(),
            callback.callback_slot,
        );
        let rule = indexed.get(&exact_key).or_else(|| {
            callback.callback_slot.and_then(|_| {
                indexed.get(&(callback.path.clone(), callback.callback_id.clone(), None))
            })
        });
        if let Some(rule) = rule {
            if callback.semantic && rule.action.classification == CallbackClass::UserInterfaceOnly {
                return Err(SchemaError::InvalidGraph(format!(
                    "semantic callback {} at {} cannot be classified as UI-only",
                    callback.callback_id, callback.path
                )));
            }
            callback_bindings.push(build_callback_binding(
                &rule.action,
                callback,
                &mut handlers,
            )?);
            continue;
        }
        let implementation_key = (
            callback.implementation_fingerprint.clone(),
            callback.callback_id.clone(),
        );
        if let Some(rule) = implementations.get(&implementation_key) {
            if !rule
                .matches
                .iter()
                .any(|expected| expected.game == export.game)
            {
                return Err(SchemaError::InvalidGraph(format!(
                    "implementation {} role {} is not approved for {}",
                    callback.implementation_fingerprint,
                    callback.callback_id,
                    export.game.slug()
                )));
            }
            if !callback.semantic {
                return Err(SchemaError::InvalidGraph(format!(
                    "UI-only callback {} at {} must use an exact path rule",
                    callback.callback_id, callback.path
                )));
            }
            callback_bindings.push(build_callback_binding(
                &rule.action,
                callback,
                &mut handlers,
            )?);
            continue;
        }
        if callback.semantic {
            if let Some(decoder) = schema_decoders.get(&callback.path) {
                decoders.entry(decoder.clone()).or_insert(1);
                callback_bindings.push(CallbackBinding {
                    path: callback.path.clone(),
                    callback_id: callback.callback_id.clone(),
                    callback_slot: callback.callback_slot,
                    implementation_fingerprint: callback.implementation_fingerprint.clone(),
                    implementation: CallbackImplementation::PayloadDecoder {
                        decoder: decoder.clone(),
                        minimum_decoder_version: 1,
                    },
                });
                continue;
            }
        }
        missing.push(format!(
            "{}@{}#{}",
            callback.callback_id, callback.path, callback.implementation_fingerprint
        ));
    }
    if !missing.is_empty() {
        missing.sort();
        return Err(SchemaError::UnclassifiedCallbacks(missing.join(", ")));
    }

    let required_decoders: Vec<DecoderRequirement> = decoders
        .into_iter()
        .map(|(id, minimum_version)| DecoderRequirement {
            id,
            minimum_version,
        })
        .collect();
    let required_handlers: Vec<HandlerRequirement> = handlers
        .into_iter()
        .map(|(id, minimum_version)| HandlerRequirement {
            id,
            minimum_version,
        })
        .collect();
    callback_bindings.sort_by(|left, right| {
        (&left.path, &left.callback_id, left.callback_slot).cmp(&(
            &right.path,
            &right.callback_id,
            right.callback_slot,
        ))
    });
    let callback_count = u64::try_from(export.callbacks.len())
        .map_err(|_| SchemaError::LimitExceeded("callback count exceeds u64".to_owned()))?;
    let source = export.provenance;
    SchemaPackage::new_with_callbacks(
        SchemaManifest {
            format_version: PACKAGE_FORMAT_VERSION,
            game: export.game,
            package_version: env!("CARGO_PKG_VERSION").to_owned(),
            source_repository: "https://github.com/TES5Edit/TES5Edit".to_owned(),
            source_tag: source.source_tag,
            source_commit: source.source_commit,
            source_archive_sha256: source.source_archive_sha256,
            exporter_version: source.exporter_version,
            exporter_binary_sha256: source.exporter_binary_sha256,
            exporter_map_sha256: source.exporter_map_sha256,
            exporter_patch_sha256: source.exporter_patch_sha256,
            exporter_build_sha256: source.exporter_build_sha256,
            conversion_rules_sha256: hex::encode(Sha256::digest(rules_bytes)),
            minimum_bethkit_version: "0.4.0".to_owned(),
            minimum_abi_version: 2,
            validation_status: ValidationStatus::Candidate,
            corpus_sha256: String::new(),
            validated_records: 0,
            byte_coverage: 0.0,
            callbacks_total: callback_count,
            callbacks_classified: callback_count,
            required_decoders,
            required_handlers,
        },
        export.records,
        callback_bindings,
    )
}

fn collect_schema_decoders(
    node: &SchemaNode,
    decoders: &mut BTreeMap<String, u32>,
    paths: &mut BTreeMap<String, String>,
) -> Result<()> {
    match &node.kind {
        SchemaNodeKind::Sequence { children } => {
            for child in children {
                collect_schema_decoders(child, decoders, paths)?;
            }
        }
        SchemaNodeKind::Choice { alternatives } => {
            for alternative in alternatives {
                collect_schema_decoders(alternative, decoders, paths)?;
            }
        }
        SchemaNodeKind::Repeat { child, .. }
        | SchemaNodeKind::Subrecord { payload: child, .. }
        | SchemaNodeKind::Compressed { child, .. }
        | SchemaNodeKind::Array { element: child, .. } => {
            collect_schema_decoders(child, decoders, paths)?;
        }
        SchemaNodeKind::Struct { fields } => {
            for field in fields {
                collect_schema_decoders(field, decoders, paths)?;
            }
        }
        SchemaNodeKind::Union { variants, .. } => {
            for variant in variants {
                collect_schema_decoders(variant, decoders, paths)?;
            }
        }
        SchemaNodeKind::Custom { decoder, .. } => {
            decoders.entry(decoder.clone()).or_insert(1);
            if paths.insert(node.path.clone(), decoder.clone()).is_some() {
                return Err(SchemaError::InvalidGraph(format!(
                    "duplicate custom decoder path {}",
                    node.path
                )));
            }
        }
        SchemaNodeKind::Primitive { .. } | SchemaNodeKind::Reference { .. } => {}
    }
    Ok(())
}

fn check_version(version: u32) -> Result<()> {
    if version == 2 {
        return Ok(());
    }
    Err(SchemaError::UnsupportedVersion(
        u16::try_from(version).unwrap_or(u16::MAX),
    ))
}

fn validate_exported_callback(callback: &ExportedCallback) -> Result<()> {
    let valid_fingerprint = callback.implementation_fingerprint.len() == 64
        && callback
            .implementation_fingerprint
            .bytes()
            .all(|value| value.is_ascii_digit() || (b'a'..=b'f').contains(&value));
    if !valid_fingerprint {
        return Err(SchemaError::InvalidGraph(format!(
            "callback {} at {} has an invalid implementation fingerprint",
            callback.callback_id, callback.path
        )));
    }
    if callback.semantic
        && (callback.implementation_symbol.trim().is_empty()
            || callback.implementation_unit.trim().is_empty())
    {
        return Err(SchemaError::InvalidGraph(format!(
            "semantic callback {} at {} has no resolved Delphi symbol",
            callback.callback_id, callback.path
        )));
    }
    Ok(())
}

fn validate_implementation_matches(
    export: &XEditExport,
    rules: &[ImplementationRule],
) -> Result<()> {
    for rule in rules {
        let mut games: BTreeMap<SchemaGame, &ImplementationRuleMatch> = BTreeMap::new();
        for expected in &rule.matches {
            if games.insert(expected.game, expected).is_some() {
                return Err(SchemaError::InvalidGraph(format!(
                    "implementation rule {} has duplicate match guards for {}",
                    rule.implementation_fingerprint,
                    expected.game.slug()
                )));
            }
        }
        let Some(expected) = games.get(&export.game) else {
            continue;
        };
        let mut paths: Vec<String> = export
            .callbacks
            .iter()
            .filter(|callback| {
                callback.callback_id == rule.callback_id
                    && callback.implementation_fingerprint == rule.implementation_fingerprint
            })
            .map(|callback| {
                format!(
                    "{}|{}",
                    callback
                        .callback_slot
                        .map_or_else(|| "*".to_owned(), |slot| slot.to_string()),
                    callback.path
                )
            })
            .collect();
        paths.sort();
        let count: u32 = u32::try_from(paths.len()).map_err(|_| {
            SchemaError::LimitExceeded("implementation rule path count exceeds u32".to_owned())
        })?;
        if count != expected.expected_match_count {
            return Err(SchemaError::InvalidGraph(format!(
                "implementation {} role {} matched {count} paths for {}, expected {}",
                rule.implementation_fingerprint,
                rule.callback_id,
                export.game.slug(),
                expected.expected_match_count
            )));
        }
        let mut canonical = paths.join("\n");
        if !canonical.is_empty() {
            canonical.push('\n');
        }
        let actual_digest = hex::encode(Sha256::digest(canonical.as_bytes()));
        if actual_digest != expected.expected_paths_sha256 {
            return Err(SchemaError::InvalidGraph(format!(
                "implementation {} role {} path digest changed for {}",
                rule.implementation_fingerprint,
                rule.callback_id,
                export.game.slug()
            )));
        }
    }
    Ok(())
}

fn build_callback_binding(
    action: &CallbackRuleAction,
    callback: &ExportedCallback,
    handlers: &mut BTreeMap<String, u32>,
) -> Result<CallbackBinding> {
    let unexpected = |field: &str| {
        SchemaError::InvalidGraph(format!(
            "{} callback {} at {} has unexpected {field}",
            callback_class_name(action.classification),
            callback.callback_id,
            callback.path
        ))
    };
    let implementation = match action.classification {
        CallbackClass::Declarative => {
            if action.built_in_operation.is_some()
                || action.custom_handler.is_some()
                || action.minimum_handler_version.is_some()
            {
                return Err(unexpected("implementation metadata"));
            }
            CallbackImplementation::Declarative {
                expression: action.expression.clone().ok_or_else(|| {
                    SchemaError::InvalidGraph(format!(
                        "declarative callback {} at {} has no expression",
                        callback.callback_id, callback.path
                    ))
                })?,
            }
        }
        CallbackClass::BuiltIn => {
            if action.expression.is_some()
                || action.custom_handler.is_some()
                || action.minimum_handler_version.is_some()
            {
                return Err(unexpected("implementation metadata"));
            }
            let operation = action.built_in_operation.clone().ok_or_else(|| {
                SchemaError::InvalidGraph(format!(
                    "built-in callback {} at {} has no operation",
                    callback.callback_id, callback.path
                ))
            })?;
            if operation.id.trim().is_empty() || operation.minimum_version == 0 {
                return Err(SchemaError::InvalidGraph(format!(
                    "built-in callback {} at {} has an invalid operation",
                    callback.callback_id, callback.path
                )));
            }
            handlers
                .entry(operation.id.clone())
                .and_modify(|current| *current = (*current).max(operation.minimum_version))
                .or_insert(operation.minimum_version);
            CallbackImplementation::BuiltIn { operation }
        }
        CallbackClass::CustomHandler => {
            if action.expression.is_some() || action.built_in_operation.is_some() {
                return Err(unexpected("implementation metadata"));
            }
            let handler = action.custom_handler.clone().ok_or_else(|| {
                SchemaError::InvalidGraph(format!(
                    "custom callback {} at {} has no handler",
                    callback.callback_id, callback.path
                ))
            })?;
            let version = action.minimum_handler_version.ok_or_else(|| {
                SchemaError::InvalidGraph(format!(
                    "custom callback {} at {} has no handler version",
                    callback.callback_id, callback.path
                ))
            })?;
            if handler.trim().is_empty() || version == 0 {
                return Err(SchemaError::InvalidGraph(format!(
                    "custom callback {} at {} has invalid handler metadata",
                    callback.callback_id, callback.path
                )));
            }
            handlers
                .entry(handler.clone())
                .and_modify(|current| *current = (*current).max(version))
                .or_insert(version);
            CallbackImplementation::CustomHandler {
                handler,
                minimum_handler_version: version,
            }
        }
        CallbackClass::UserInterfaceOnly => {
            if action.expression.is_some()
                || action.built_in_operation.is_some()
                || action.custom_handler.is_some()
                || action.minimum_handler_version.is_some()
            {
                return Err(unexpected("implementation metadata"));
            }
            CallbackImplementation::UserInterfaceOnly
        }
    };
    Ok(CallbackBinding {
        path: callback.path.clone(),
        callback_id: callback.callback_id.clone(),
        callback_slot: callback.callback_slot,
        implementation_fingerprint: callback.implementation_fingerprint.clone(),
        implementation,
    })
}

const fn callback_class_name(classification: CallbackClass) -> &'static str {
    match classification {
        CallbackClass::Declarative => "declarative",
        CallbackClass::BuiltIn => "built-in",
        CallbackClass::CustomHandler => "custom handler",
        CallbackClass::UserInterfaceOnly => "UI-only",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provenance() -> ExporterProvenance {
        ExporterProvenance {
            source_tag: "xedit-4.1.5f".to_owned(),
            source_commit: "f5c00f3fa3ee39511185515802647246c807f759".to_owned(),
            source_archive_sha256: "00".repeat(32),
            exporter_version: "2".to_owned(),
            exporter_binary_sha256: "11".repeat(32),
            exporter_map_sha256: "12".repeat(32),
            exporter_patch_sha256: "22".repeat(32),
            exporter_build_sha256: "33".repeat(32),
        }
    }

    fn callback(path: &str, callback_id: &str, semantic: bool) -> ExportedCallback {
        ExportedCallback {
            path: path.to_owned(),
            callback_id: callback_id.to_owned(),
            callback_slot: None,
            semantic,
            implementation_fingerprint: "aa".repeat(32),
            implementation_symbol: if semantic {
                "Test.UnitCallback".to_owned()
            } else {
                String::new()
            },
            implementation_unit: if semantic {
                "Test".to_owned()
            } else {
                String::new()
            },
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

    /// Verifies the flat node representation emitted by the Delphi exporter.
    #[test]
    fn exported_nodes_deserialize_from_flat_json(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let json = r#"{
            "id": 0,
            "path": "TEST",
            "name": "Test",
            "required": true,
            "condition": null,
            "kind": "sequence",
            "children": []
        }"#;

        // when
        let node: SchemaNode = serde_json::from_str(json)?;

        // then
        assert!(matches!(
            node.kind,
            SchemaNodeKind::Sequence { ref children } if children.is_empty()
        ));
        assert_eq!(serde_json::to_value(&node)?["kind"], "sequence");
        Ok(())
    }

    /// Verifies that serialized game names match the exporter/package slugs.
    #[test]
    fn schema_games_use_stable_slugs() -> std::result::Result<(), Box<dyn std::error::Error>> {
        for game in SchemaGame::all() {
            let json = serde_json::to_string(&game)?;
            assert_eq!(json, format!("\"{}\"", game.slug()));
        }
        Ok(())
    }

    /// Verifies that classifications become executable package bindings.
    #[test]
    fn conversion_embeds_callback_bindings() -> std::result::Result<(), Box<dyn std::error::Error>>
    {
        // given
        let export = XEditExport {
            contract_version: 2,
            provenance: provenance(),
            game: SchemaGame::SkyrimSe,
            records: Vec::new(),
            callbacks: vec![callback("TEST/ui", "def.dont_show", false)],
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
        let package = convert_xedit_export(export, &rules, b"{}")?;

        // then
        assert_eq!(package.callback_bindings().len(), 1);
        assert!(matches!(
            package.callback_bindings()[0].implementation,
            CallbackImplementation::UserInterfaceOnly
        ));
        Ok(())
    }

    /// Verifies that unknown callbacks block conversion.
    #[test]
    fn conversion_rejects_unknown_callbacks() {
        // given
        let export = XEditExport {
            contract_version: 2,
            provenance: provenance(),
            game: SchemaGame::SkyrimSe,
            records: Vec::new(),
            callbacks: vec![callback("NPC_.DATA", "size_decider", true)],
        };
        let rules = ConversionRules {
            format_version: 2,
            callbacks: Vec::new(),
            implementations: Vec::new(),
        };

        // when
        let result = convert_xedit_export(export, &rules, b"{}");

        // then
        assert!(matches!(result, Err(SchemaError::UnclassifiedCallbacks(_))));
    }

    /// Verifies implementation rules are path-guarded and require runtime handlers.
    #[test]
    fn conversion_expands_guarded_implementation_rules(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let exported = callback("TEST/value", "float.normalizer", true);
        let digest = hex::encode(Sha256::digest(b"*|TEST/value\n"));
        let export = XEditExport {
            contract_version: 2,
            provenance: provenance(),
            game: SchemaGame::SkyrimSe,
            records: Vec::new(),
            callbacks: vec![exported.clone()],
        };
        let operation = BuiltInOperation {
            id: "normalize.radians".to_owned(),
            minimum_version: 1,
            configuration: serde_json::json!({"period": std::f64::consts::TAU}),
        };
        let rules = ConversionRules {
            format_version: 2,
            callbacks: Vec::new(),
            implementations: vec![ImplementationRule {
                implementation_fingerprint: exported.implementation_fingerprint,
                callback_id: exported.callback_id,
                matches: vec![ImplementationRuleMatch {
                    game: SchemaGame::SkyrimSe,
                    expected_match_count: 1,
                    expected_paths_sha256: digest,
                }],
                action: CallbackRuleAction {
                    classification: CallbackClass::BuiltIn,
                    expression: None,
                    built_in_operation: Some(operation.clone()),
                    custom_handler: None,
                    minimum_handler_version: None,
                    rationale: "Matches xEdit's radians normalizer.".to_owned(),
                },
            }],
        };

        // when
        let package = convert_xedit_export(export, &rules, b"{}")?;

        // then
        assert_eq!(
            package.manifest().required_handlers,
            vec![HandlerRequirement {
                id: operation.id.clone(),
                minimum_version: operation.minimum_version,
            }]
        );
        assert!(matches!(
            &package.callback_bindings()[0].implementation,
            CallbackImplementation::BuiltIn { operation: actual } if actual == &operation
        ));
        Ok(())
    }

    /// Verifies that custom schema nodes always become decoder requirements.
    #[test]
    fn conversion_collects_custom_node_decoders(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let export = XEditExport {
            contract_version: 2,
            provenance: provenance(),
            game: SchemaGame::SkyrimSe,
            records: vec![SchemaRecord {
                signature: crate::SchemaSignature(*b"TEST"),
                name: "Test".to_owned(),
                root: SchemaNode {
                    id: crate::SchemaNodeId(0),
                    path: "TEST/root".to_owned(),
                    name: "Root".to_owned(),
                    required: true,
                    condition: None,
                    kind: SchemaNodeKind::Custom {
                        decoder: "xedit.dtunion".to_owned(),
                        configuration: serde_json::json!({}),
                    },
                },
            }],
            callbacks: vec![callback("TEST/root", "decoder.required", true)],
        };
        let rules = ConversionRules {
            format_version: 2,
            callbacks: Vec::new(),
            implementations: Vec::new(),
        };

        // when
        let package = convert_xedit_export(export, &rules, b"{}")?;

        // then
        assert_eq!(
            package.manifest().required_decoders,
            vec![DecoderRequirement {
                id: "xedit.dtunion".to_owned(),
                minimum_version: 1,
            }]
        );
        assert!(matches!(
            package.callback_bindings()[0].implementation,
            CallbackImplementation::PayloadDecoder {
                ref decoder,
                minimum_decoder_version: 1,
            } if decoder == "xedit.dtunion"
        ));
        Ok(())
    }
}
