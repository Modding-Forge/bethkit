// SPDX-License-Identifier: Apache-2.0
//!
//! Checked conversion from the xEdit exporter contract.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    CallbackClass, DecoderRequirement, Result, SchemaError, SchemaGame, SchemaManifest,
    SchemaPackage, SchemaRecord, ValidationStatus, PACKAGE_FORMAT_VERSION,
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
    /// Custom decoder identifier, required for `custom`.
    pub custom_decoder: Option<String>,
    /// Minimum custom decoder version.
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
    for callback in &export.callbacks {
        let Some(rule) = indexed.get(&(callback.path.as_str(), callback.callback_id.as_str()))
        else {
            missing.push(format!("{}@{}", callback.callback_id, callback.path));
            continue;
        };
        if callback.semantic && rule.classification == CallbackClass::UserInterfaceOnly {
            return Err(SchemaError::InvalidGraph(format!(
                "semantic callback {} at {} cannot be classified as UI-only",
                callback.callback_id, callback.path
            )));
        }
        collect_decoder(rule, callback, &mut decoders)?;
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
    let callback_count = u64::try_from(export.callbacks.len())
        .map_err(|_| SchemaError::LimitExceeded("callback count exceeds u64".to_owned()))?;
    let source = export.provenance;
    SchemaPackage::new(
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
    )
}

fn check_version(version: u32) -> Result<()> {
    if version == 1 {
        return Ok(());
    }
    Err(SchemaError::UnsupportedVersion(
        u16::try_from(version).unwrap_or(u16::MAX),
    ))
}

fn collect_decoder(
    rule: &CallbackRule,
    callback: &ExportedCallback,
    decoders: &mut BTreeMap<String, u32>,
) -> Result<()> {
    if rule.classification != CallbackClass::Custom {
        return Ok(());
    }
    let decoder = rule.custom_decoder.as_ref().ok_or_else(|| {
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
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
