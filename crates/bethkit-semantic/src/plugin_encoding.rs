// SPDX-License-Identifier: Apache-2.0
//! Bounded plugin-text detection and explicit caller-supplied codepage overrides.

use std::collections::BTreeSet;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::sync::Arc;

use bethkit_core::{Plugin, Signature};
use bethkit_schema::{PrimitiveType, SchemaNode, SchemaNodeKind, SchemaPackage};
use chardetng::{EncodingDetector, Iso2022JpDetection, Utf8Detection};

use crate::{
    DecoderRegistry, FieldValue, InlineStringEncoding, Result, SemanticContext, SemanticError,
};

/// Maximum number of bytes fed to the detector, including string separators.
pub const ENCODING_SAMPLE_BYTES: usize = 64 * 1024;
/// Maximum number of records inspected during one plugin encoding decision.
pub const ENCODING_SAMPLE_RECORDS: usize = 512;

/// One plugin-scoped decision, retained by its semantic context.
#[derive(Clone, Debug)]
pub struct PluginEncodingDecision {
    encoding: InlineStringEncoding,
    source: &'static str,
    sample_bytes: usize,
    sample_strings: usize,
    scanned_records: usize,
    skipped_records: usize,
    non_ascii_bytes: usize,
}

impl PluginEncodingDecision {
    /// Returns the detected or overridden codec; mixed mode uses it as a prior.
    pub fn encoding(&self) -> InlineStringEncoding {
        self.encoding
    }
    /// Returns `cpoverride`, `detector`, `ascii`, or `no_inline_text`.
    pub fn source(&self) -> &'static str {
        self.source
    }
    /// Returns the number of sample bytes, including separators.
    pub fn sample_bytes(&self) -> usize {
        self.sample_bytes
    }
    /// Returns the number of complete strings sampled.
    pub fn sample_strings(&self) -> usize {
        self.sample_strings
    }
    /// Returns the number of records inspected.
    pub fn scanned_records(&self) -> usize {
        self.scanned_records
    }
    /// Returns the number of records rejected by structural/schema decoding.
    pub fn skipped_records(&self) -> usize {
        self.skipped_records
    }
}

impl SemanticContext {
    /// Creates a mixed context retaining suspicious inline text as raw byte values.
    ///
    /// Sparse non-default legacy samples and invalid UTF-8 with substantial valid
    /// UTF-8 fragments are treated as uncertain, not repaired. Explicit overrides
    /// remain authoritative. Conservative false positives require codec selection.
    ///
    /// # Errors
    /// Returns the schema, decoder and override errors of `new_for_plugin_mixed`.
    pub fn new_for_plugin_preserving_text(
        package: Arc<SchemaPackage>,
        decoders: DecoderRegistry,
        plugin: &Plugin,
        cpoverride: Option<&Path>,
    ) -> Result<Self> {
        let mut context = Self::new_for_plugin_mixed(package, decoders, plugin, cpoverride)?;
        if cpoverride.is_some() {
            let codec = match context.inline_string_encoding {
                Some(InlineStringEncoding::Utf8) => encoding_rs::UTF_8,
                Some(InlineStringEncoding::Windows1252) => encoding_rs::WINDOWS_1252,
                Some(InlineStringEncoding::Legacy(codec)) => codec,
                _ => {
                    return Err(SemanticError::EncodingPolicy(
                        "explicit override did not select a concrete codec".to_owned(),
                    ))
                }
            };
            context.inline_string_encoding = Some(InlineStringEncoding::PreserveExplicit(codec));
        }
        if cpoverride.is_none() {
            let fallback = context
                .inline_string_encoding
                .and_then(InlineStringEncoding::fallback_encoding)
                .map_or(encoding_rs::WINDOWS_1252, |codec| match codec {
                    InlineStringEncoding::Legacy(codec) => codec,
                    _ => encoding_rs::WINDOWS_1252,
                });
            let uncertain_legacy =
                context
                    .plugin_encoding_decision
                    .as_ref()
                    .is_some_and(|decision| {
                        matches!(decision.encoding, InlineStringEncoding::Legacy(_))
                            && decision.non_ascii_bytes < 16
                    });
            context.inline_string_encoding = Some(InlineStringEncoding::PreserveUtf8WithFallback {
                encoding: fallback,
                uncertain_legacy,
            });
        }
        Ok(context)
    }

    /// Creates a context with opt-in per-string UTF-8/legacy selection.
    ///
    /// The detector's result is a prior, not proof of an individual string's
    /// encoding. Valid UTF-8 wins per complete string; other bytes use the
    /// detected legacy codec, or Windows-1252 when detection selected UTF-8.
    /// This heuristic cannot disambiguate bytes valid under both codecs. Field
    /// codec metadata is retained for edits. An explicit `cpoverride` disables
    /// this heuristic entirely and remains authoritative.
    ///
    /// # Errors
    /// Returns the same schema, decoder and override errors as `new_for_plugin`.
    pub fn new_for_plugin_mixed(
        package: Arc<SchemaPackage>,
        decoders: DecoderRegistry,
        plugin: &Plugin,
        cpoverride: Option<&Path>,
    ) -> Result<Self> {
        let mut context = Self::new_for_plugin(package, decoders, plugin, cpoverride)?;
        if cpoverride.is_none() {
            let fallback = match context.inline_string_encoding {
                Some(InlineStringEncoding::Legacy(codec)) => codec,
                _ => encoding_rs::WINDOWS_1252,
            };
            context.inline_string_encoding =
                Some(InlineStringEncoding::PreferUtf8WithFallback(fallback));
        }
        Ok(context)
    }

    /// Creates a plugin-specific context using an override or bounded text detection.
    ///
    /// `cpoverride` is an explicitly supplied path, never inferred from a filename.
    /// Its first UTF-8 line names a codec or supported Windows codepage number.
    /// A supplied override always wins, including for an empty or localized plugin.
    /// Without it, detection samples complete schema-localizable inline strings.
    /// The returned context owns its schema and decision; it does not retain `plugin`.
    /// Reuse it only with this plugin and records derived from it. Mixed encodings
    /// still require field-specific overrides. ASCII/empty samples cannot identify
    /// an original codec and use UTF-8 as a compatible default.
    ///
    /// # Errors
    ///
    /// Returns context construction errors or [`SemanticError::EncodingPolicy`]
    /// for unreadable, oversized, invalid, or unsupported override files.
    pub fn new_for_plugin(
        package: Arc<SchemaPackage>,
        decoders: DecoderRegistry,
        plugin: &Plugin,
        cpoverride: Option<&Path>,
    ) -> Result<Self> {
        let override_codec = cpoverride.map(read_override).transpose()?;
        let mut context = Self::new(package, decoders)?;
        let decision = if let Some(encoding) = override_codec {
            PluginEncodingDecision {
                encoding,
                source: "cpoverride",
                sample_bytes: 0,
                sample_strings: 0,
                scanned_records: 0,
                skipped_records: 0,
                non_ascii_bytes: 0,
            }
        } else {
            // Windows-1252 maps all byte values reversibly during structural sampling.
            // This temporary codec never determines the final interpretation.
            context.inline_string_encoding = Some(InlineStringEncoding::Windows1252);
            detect(&context, plugin)
        };
        context.inline_string_encoding = Some(decision.encoding);
        context.plugin_encoding_decision = Some(decision);
        Ok(context)
    }
}

fn read_override(path: &Path) -> Result<InlineStringEncoding> {
    let metadata = fs::metadata(path).map_err(|error| override_error(path, &error.to_string()))?;
    if metadata.len() > 4096 {
        return Err(override_error(path, "override exceeds 4096 bytes"));
    }
    let file = fs::File::open(path).map_err(|error| override_error(path, &error.to_string()))?;
    let mut text = String::new();
    file.take(4097)
        .read_to_string(&mut text)
        .map_err(|error| override_error(path, &error.to_string()))?;
    if text.len() > 4096 {
        return Err(override_error(path, "override exceeds 4096 bytes"));
    }
    let label = text
        .trim_start_matches('\u{feff}')
        .lines()
        .next()
        .unwrap_or("")
        .trim();
    InlineStringEncoding::from_label(label)
        .ok_or_else(|| override_error(path, &format!("unsupported encoding {label:?}")))
}

fn override_error(path: &Path, message: &str) -> SemanticError {
    SemanticError::EncodingPolicy(format!("{}: {message}", path.display()))
}

fn detect(context: &SemanticContext, plugin: &Plugin) -> PluginEncodingDecision {
    let text_signatures: BTreeSet<[u8; 4]> = context
        .registry()
        .package()
        .records()
        .iter()
        .filter(|record| contains_text(&record.root))
        .map(|record| record.signature.0)
        .collect();
    let mut sample = Vec::with_capacity(ENCODING_SAMPLE_BYTES);
    let mut strings = 0;
    let mut scanned = 0;
    let mut skipped = 0;
    if !plugin.is_localized() {
        for record in plugin
            .groups()
            .iter()
            .flat_map(|group| group.records_recursive())
            .take(ENCODING_SAMPLE_RECORDS)
        {
            scanned += 1;
            if !text_signatures.contains(&record.header.signature.0) {
                continue;
            }
            // Avoid decompressing or materializing unusually large records for detection.
            if record.uncompressed_data_size() > 1024 * 1024 {
                skipped += 1;
                continue;
            }
            let fields = context.view(record, false).and_then(|view| view.fields());
            let Ok(fields) = fields else {
                skipped += 1;
                continue;
            };
            for field in &fields {
                collect(
                    context,
                    record.header.signature,
                    field.effective_path.as_deref().unwrap_or(&field.path),
                    &field.value,
                    &mut sample,
                    &mut strings,
                );
            }
            if sample.len() >= ENCODING_SAMPLE_BYTES * 3 / 4 {
                break;
            }
        }
    }
    let mut detector = EncodingDetector::new(Iso2022JpDetection::Deny);
    let non_ascii = detector.feed(&sample, true);
    let encoding = detector.guess(None, Utf8Detection::Allow);
    let selected = InlineStringEncoding::from_label(encoding.name())
        .expect("chardetng with ISO-2022-JP disabled returns a supported byte codec");
    PluginEncodingDecision {
        encoding: if non_ascii {
            selected
        } else {
            InlineStringEncoding::Utf8
        },
        source: if sample.is_empty() {
            "no_inline_text"
        } else if non_ascii {
            "detector"
        } else {
            "ascii"
        },
        sample_bytes: sample.len(),
        sample_strings: strings,
        scanned_records: scanned,
        skipped_records: skipped,
        non_ascii_bytes: sample.iter().filter(|byte| !byte.is_ascii()).count(),
    }
}

fn contains_text(node: &SchemaNode) -> bool {
    match &node.kind {
        SchemaNodeKind::Primitive { primitive } => matches!(primitive,
            PrimitiveType::String { string } if string.localized || string.encoding == "localized"),
        SchemaNodeKind::Sequence { children } | SchemaNodeKind::Unordered { children } => {
            children.iter().any(contains_text)
        }
        SchemaNodeKind::Choice { alternatives }
        | SchemaNodeKind::SelectedChoice { alternatives, .. } => {
            alternatives.iter().any(contains_text)
        }
        SchemaNodeKind::Struct { fields } | SchemaNodeKind::OptionalStruct { fields, .. } => {
            fields.iter().any(contains_text)
        }
        SchemaNodeKind::Union { variants, .. } => variants.iter().any(contains_text),
        SchemaNodeKind::Subrecord { payload, .. } => contains_text(payload),
        SchemaNodeKind::Repeat { child, .. }
        | SchemaNodeKind::Compressed { child, .. }
        | SchemaNodeKind::Terminated { child, .. } => contains_text(child),
        SchemaNodeKind::Array { element, .. } => contains_text(element),
        // References are conservatively included; only resolved string nodes are sampled.
        SchemaNodeKind::Reference { .. } => true,
        SchemaNodeKind::Custom { .. } => false,
    }
}

fn collect(
    context: &SemanticContext,
    signature: Signature,
    path: &str,
    value: &FieldValue<'_>,
    sample: &mut Vec<u8>,
    strings: &mut usize,
) {
    match value {
        FieldValue::Struct(fields) => {
            for field in fields {
                collect(
                    context,
                    signature,
                    field.effective_path.as_deref().unwrap_or(&field.path),
                    &field.value,
                    sample,
                    strings,
                );
            }
        }
        FieldValue::Array(values) => {
            let node = context
                .registry()
                .get_node(signature, path)
                .and_then(|node| payload_node(context, signature, node, 0));
            let element_path = node
                .and_then(|node| match &node.kind {
                    SchemaNodeKind::Array { element, .. } => Some(element.path.as_str()),
                    _ => None,
                })
                .unwrap_or(path);
            for item in values {
                collect(context, signature, element_path, item, sample, strings);
            }
        }
        FieldValue::String(text) => {
            let translatable = context
                .registry()
                .get_node(signature, path)
                .and_then(|node| payload_node(context, signature, node, 0))
                .is_some_and(|node| {
                    matches!(&node.kind, SchemaNodeKind::Primitive {
                    primitive: PrimitiveType::String { string }
                } if string.localized || string.encoding == "localized")
                });
            if translatable {
                let (bytes, _, errors) = encoding_rs::WINDOWS_1252.encode(text);
                if !errors
                    && !bytes.is_empty()
                    && sample.len() + bytes.len() < ENCODING_SAMPLE_BYTES
                {
                    sample.extend_from_slice(&bytes);
                    sample.push(b'\n');
                    *strings += 1;
                }
            }
        }
        _ => {}
    }
}

fn payload_node<'a>(
    context: &'a SemanticContext,
    signature: Signature,
    node: &'a SchemaNode,
    depth: usize,
) -> Option<&'a SchemaNode> {
    if depth > 64 {
        return None;
    }
    match &node.kind {
        SchemaNodeKind::Subrecord { payload, .. } => {
            payload_node(context, signature, payload, depth + 1)
        }
        SchemaNodeKind::Compressed { child, .. } | SchemaNodeKind::Terminated { child, .. } => {
            payload_node(context, signature, child, depth + 1)
        }
        SchemaNodeKind::Reference { target } => context
            .registry()
            .get_node(signature, target)
            .and_then(|node| payload_node(context, signature, node, depth + 1)),
        _ => Some(node),
    }
}

#[cfg(test)]
mod tests;
