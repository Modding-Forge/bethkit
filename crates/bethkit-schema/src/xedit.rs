// SPDX-License-Identifier: Apache-2.0
//!
//! Checked conversion from the xEdit exporter contract.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    CallbackBinding, CallbackClass, CallbackImplementation, DecoderRequirement, Expression, Result,
    SchemaError, SchemaGame, SchemaManifest, SchemaNode, SchemaNodeKind, SchemaPackage,
    SchemaRecord, ValidationStatus, PACKAGE_FORMAT_VERSION,
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
    /// Whether the callback can affect binary interpretation or validation.
    pub semantic: bool,
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
    /// Audited callback classification.
    pub classification: CallbackClass,
    /// Bounded expression, required for `declarative`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expression: Option<Expression>,
    /// Stable built-in operation, required for `built_in`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub built_in_operation: Option<String>,
    /// Custom decoder identifier, required for `custom`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_decoder: Option<String>,
    /// Minimum custom decoder version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum_decoder_version: Option<u32>,
    /// Human-readable review note.
    pub rationale: String,
}

/// Versioned conversion-rule document tracked in the repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversionRules {
    /// Rule format version. Version 1 is currently supported.
    pub format_version: u32,
    /// Audited callback rules.
    pub callbacks: Vec<CallbackRule>,
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

    let mut indexed: BTreeMap<(&str, &str), &CallbackRule> = BTreeMap::new();
    for rule in &rules.callbacks {
        if indexed
            .insert((&rule.path, &rule.callback_id), rule)
            .is_some()
        {
            return Err(SchemaError::InvalidGraph(format!(
                "duplicate callback rule for {} at {}",
                rule.callback_id, rule.path
            )));
        }
    }

    let mut missing: Vec<String> = Vec::new();
    let mut decoders: BTreeMap<String, u32> = BTreeMap::new();
    let mut schema_decoders: BTreeMap<String, String> = BTreeMap::new();
    let mut callback_bindings: Vec<CallbackBinding> = Vec::with_capacity(export.callbacks.len());
    for record in &export.records {
        collect_schema_decoders(&record.root, &mut decoders, &mut schema_decoders)?;
    }
    for callback in &export.callbacks {
        let rule = indexed.get(&(callback.path.as_str(), callback.callback_id.as_str()));
        if let Some(rule) = rule {
            if callback.semantic && rule.classification == CallbackClass::UserInterfaceOnly {
                return Err(SchemaError::InvalidGraph(format!(
                    "semantic callback {} at {} cannot be classified as UI-only",
                    callback.callback_id, callback.path
                )));
            }
            callback_bindings.push(build_callback_binding(rule, callback, &mut decoders)?);
            continue;
        }
        if callback.semantic {
            if let Some(decoder) = schema_decoders.get(&callback.path) {
                decoders.entry(decoder.clone()).or_insert(1);
                callback_bindings.push(CallbackBinding {
                    path: callback.path.clone(),
                    callback_id: callback.callback_id.clone(),
                    implementation: CallbackImplementation::Custom {
                        decoder: decoder.clone(),
                        minimum_decoder_version: 1,
                    },
                });
                continue;
            }
        }
        missing.push(format!("{}@{}", callback.callback_id, callback.path));
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
    callback_bindings.sort_by(|left, right| {
        (&left.path, &left.callback_id).cmp(&(&right.path, &right.callback_id))
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
    if version == 1 {
        return Ok(());
    }
    Err(SchemaError::UnsupportedVersion(
        u16::try_from(version).unwrap_or(u16::MAX),
    ))
}

fn build_callback_binding(
    rule: &CallbackRule,
    callback: &ExportedCallback,
    decoders: &mut BTreeMap<String, u32>,
) -> Result<CallbackBinding> {
    let unexpected = |field: &str| {
        SchemaError::InvalidGraph(format!(
            "{} callback {} at {} has unexpected {field}",
            callback_class_name(rule.classification),
            callback.callback_id,
            callback.path
        ))
    };
    let implementation = match rule.classification {
        CallbackClass::Declarative => {
            if rule.built_in_operation.is_some()
                || rule.custom_decoder.is_some()
                || rule.minimum_decoder_version.is_some()
            {
                return Err(unexpected("implementation metadata"));
            }
            CallbackImplementation::Declarative {
                expression: rule.expression.clone().ok_or_else(|| {
                    SchemaError::InvalidGraph(format!(
                        "declarative callback {} at {} has no expression",
                        callback.callback_id, callback.path
                    ))
                })?,
            }
        }
        CallbackClass::BuiltIn => {
            if rule.expression.is_some()
                || rule.custom_decoder.is_some()
                || rule.minimum_decoder_version.is_some()
            {
                return Err(unexpected("implementation metadata"));
            }
            let operation = rule.built_in_operation.clone().ok_or_else(|| {
                SchemaError::InvalidGraph(format!(
                    "built-in callback {} at {} has no operation",
                    callback.callback_id, callback.path
                ))
            })?;
            CallbackImplementation::BuiltIn { operation }
        }
        CallbackClass::Custom => {
            if rule.expression.is_some() || rule.built_in_operation.is_some() {
                return Err(unexpected("implementation metadata"));
            }
            let decoder = rule.custom_decoder.clone().ok_or_else(|| {
                SchemaError::InvalidGraph(format!(
                    "custom callback {} at {} has no decoder",
                    callback.callback_id, callback.path
                ))
            })?;
            let version = rule.minimum_decoder_version.ok_or_else(|| {
                SchemaError::InvalidGraph(format!(
                    "custom callback {} at {} has no decoder version",
                    callback.callback_id, callback.path
                ))
            })?;
            decoders
                .entry(decoder.clone())
                .and_modify(|current| *current = (*current).max(version))
                .or_insert(version);
            CallbackImplementation::Custom {
                decoder,
                minimum_decoder_version: version,
            }
        }
        CallbackClass::UserInterfaceOnly => {
            if rule.expression.is_some()
                || rule.built_in_operation.is_some()
                || rule.custom_decoder.is_some()
                || rule.minimum_decoder_version.is_some()
            {
                return Err(unexpected("implementation metadata"));
            }
            CallbackImplementation::UserInterfaceOnly
        }
    };
    Ok(CallbackBinding {
        path: callback.path.clone(),
        callback_id: callback.callback_id.clone(),
        implementation,
    })
}

const fn callback_class_name(classification: CallbackClass) -> &'static str {
    match classification {
        CallbackClass::Declarative => "declarative",
        CallbackClass::BuiltIn => "built-in",
        CallbackClass::Custom => "custom",
        CallbackClass::UserInterfaceOnly => "UI-only",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            contract_version: 1,
            provenance: ExporterProvenance {
                source_tag: "xedit-4.1.5f".to_owned(),
                source_commit: "f5c00f3fa3ee39511185515802647246c807f759".to_owned(),
                source_archive_sha256: "00".repeat(32),
                exporter_version: "1".to_owned(),
                exporter_binary_sha256: "11".repeat(32),
                exporter_patch_sha256: "22".repeat(32),
                exporter_build_sha256: "33".repeat(32),
            },
            game: SchemaGame::SkyrimSe,
            records: Vec::new(),
            callbacks: vec![ExportedCallback {
                path: "TEST/ui".to_owned(),
                callback_id: "def.dont_show".to_owned(),
                semantic: false,
            }],
        };
        let rules = ConversionRules {
            format_version: 1,
            callbacks: vec![CallbackRule {
                path: "TEST/ui".to_owned(),
                callback_id: "def.dont_show".to_owned(),
                classification: CallbackClass::UserInterfaceOnly,
                expression: None,
                built_in_operation: None,
                custom_decoder: None,
                minimum_decoder_version: None,
                rationale: "Presentation only.".to_owned(),
            }],
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
            contract_version: 1,
            provenance: ExporterProvenance {
                source_tag: "xedit-4.1.5f".to_owned(),
                source_commit: "f5c00f3fa3ee39511185515802647246c807f759".to_owned(),
                source_archive_sha256: "00".repeat(32),
                exporter_version: "1".to_owned(),
                exporter_binary_sha256: "11".repeat(32),
                exporter_patch_sha256: "22".repeat(32),
                exporter_build_sha256: "33".repeat(32),
            },
            game: SchemaGame::SkyrimSe,
            records: Vec::new(),
            callbacks: vec![ExportedCallback {
                path: "NPC_.DATA".to_owned(),
                callback_id: "size_decider".to_owned(),
                semantic: true,
            }],
        };
        let rules = ConversionRules {
            format_version: 1,
            callbacks: Vec::new(),
        };

        // when
        let result = convert_xedit_export(export, &rules, b"{}");

        // then
        assert!(matches!(result, Err(SchemaError::UnclassifiedCallbacks(_))));
    }

    /// Verifies that custom schema nodes always become decoder requirements.
    #[test]
    fn conversion_collects_custom_node_decoders(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let export = XEditExport {
            contract_version: 1,
            provenance: ExporterProvenance {
                source_tag: "xedit-4.1.5f".to_owned(),
                source_commit: "f5c00f3fa3ee39511185515802647246c807f759".to_owned(),
                source_archive_sha256: "00".repeat(32),
                exporter_version: "1".to_owned(),
                exporter_binary_sha256: "11".repeat(32),
                exporter_patch_sha256: "22".repeat(32),
                exporter_build_sha256: "33".repeat(32),
            },
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
            callbacks: vec![ExportedCallback {
                path: "TEST/root".to_owned(),
                callback_id: "decoder.required".to_owned(),
                semantic: true,
            }],
        };
        let rules = ConversionRules {
            format_version: 1,
            callbacks: Vec::new(),
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
            CallbackImplementation::Custom {
                ref decoder,
                minimum_decoder_version: 1,
            } if decoder == "xedit.dtunion"
        ));
        Ok(())
    }
}
