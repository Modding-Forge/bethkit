// SPDX-License-Identifier: Apache-2.0
//!
//! Load-order conflict and semantic reference analysis.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use bethkit_core::{FormId, GlobalFormId, LoadOrder, Plugin, Record, Signature};

use crate::{FieldValue, Result, SemanticContext};

/// A named plugin participating in semantic load-order analysis.
#[derive(Clone, Copy)]
pub struct PluginInput<'a> {
    /// Filename used by the load order.
    pub name: &'a str,
    /// Parsed plugin.
    pub plugin: &'a Plugin,
}

/// One directed FormID reference discovered through schema decoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceEdge {
    /// Referencing record.
    pub source: GlobalFormId,
    /// Referenced record.
    pub target: GlobalFormId,
    /// Stable schema path of the FormID field.
    pub path: String,
}

/// Directed graph of semantic FormID references.
#[derive(Debug, Default, Clone)]
pub struct ReferenceGraph {
    edges: Vec<ReferenceEdge>,
}

impl ReferenceGraph {
    /// Creates an empty graph.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a directed reference edge.
    pub fn add(&mut self, edge: ReferenceEdge) {
        self.edges.push(edge);
    }

    /// Returns all edges in deterministic discovery order.
    pub fn edges(&self) -> &[ReferenceEdge] {
        &self.edges
    }
}

/// Classification of a load-order conflict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictClass {
    /// All compared records have identical semantic content.
    Identical,
    /// A later record changes one or more fields.
    Override,
    /// Three or more versions assign divergent values to the same field.
    Conflict,
}

/// One set of records sharing a global identity.
#[derive(Debug, Clone)]
pub struct Conflict {
    /// File-local object identifier.
    pub object_id: FormId,
    /// Record signature.
    pub signature: Signature,
    /// Plugins contributing records in load-order order.
    pub plugins: Vec<String>,
    /// Winning plugin.
    pub winner: String,
    /// Conflict classification.
    pub class: ConflictClass,
    /// Stable schema paths with differing values.
    pub paths: Vec<String>,
}

/// Conflict-analysis result.
#[derive(Debug, Default, Clone)]
pub struct ConflictReport {
    conflicts: Vec<Conflict>,
}

impl ConflictReport {
    /// Creates an empty report.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a conflict set.
    pub fn push(&mut self, conflict: Conflict) {
        self.conflicts.push(conflict);
    }

    /// Returns all conflict sets.
    pub fn conflicts(&self) -> &[Conflict] {
        &self.conflicts
    }
}

/// Builds a semantic reference graph for plugins in load-order order.
///
/// Records without a schema are skipped. Invalid schema-covered records abort
/// analysis so known structures never silently fall back to raw bytes.
///
/// # Errors
///
/// Returns [`crate::SemanticError`] when a covered record cannot be decoded.
pub fn build_reference_graph(
    context: &SemanticContext,
    load_order: &LoadOrder,
    plugins: &[PluginInput<'_>],
) -> Result<ReferenceGraph> {
    let mut graph = ReferenceGraph::new();
    for input in plugins {
        for record in records(input.plugin) {
            if context.registry().get(record.header.signature).is_none() {
                continue;
            }
            let Some(source) =
                load_order.resolve(record.header.form_id, input.name, input.plugin.masters())
            else {
                continue;
            };
            let view = context.view(record, input.plugin.is_localized())?;
            for field in view.fields()? {
                collect_references(
                    &field.value,
                    &field.path,
                    &source,
                    *input,
                    load_order,
                    &mut graph,
                );
            }
        }
    }
    Ok(graph)
}

/// Compares all override chains using stable schema paths.
///
/// # Errors
///
/// Returns [`crate::SemanticError`] when a schema-covered record cannot be
/// decoded.
pub fn analyze_conflicts(
    context: &SemanticContext,
    load_order: &LoadOrder,
    plugins: &[PluginInput<'_>],
) -> Result<ConflictReport> {
    let mut chains: HashMap<GlobalFormId, Vec<Version>> = HashMap::new();
    for input in plugins {
        for record in records(input.plugin) {
            if context.registry().get(record.header.signature).is_none() {
                continue;
            }
            let Some(identity) =
                load_order.resolve(record.header.form_id, input.name, input.plugin.masters())
            else {
                continue;
            };
            let view = context.view(record, input.plugin.is_localized())?;
            let fields: BTreeMap<String, String> = view
                .fields()?
                .into_iter()
                .map(|field| (field.path, fingerprint(&field.value)))
                .collect();
            chains.entry(identity).or_default().push(Version {
                plugin: input.name.to_owned(),
                form_id: record.header.form_id,
                signature: record.header.signature,
                fields,
            });
        }
    }

    let mut ordered: Vec<(GlobalFormId, Vec<Version>)> = chains
        .into_iter()
        .filter(|(_, versions)| versions.len() > 1)
        .collect();
    ordered.sort_by(|left, right| {
        left.0
            .plugin_name
            .cmp(&right.0.plugin_name)
            .then(left.0.object_id.cmp(&right.0.object_id))
    });

    let mut report = ConflictReport::new();
    for (_, versions) in ordered {
        let paths = differing_paths(&versions);
        let class = if paths.is_empty() {
            ConflictClass::Identical
        } else if versions.len() == 2 {
            ConflictClass::Override
        } else {
            ConflictClass::Conflict
        };
        let last = versions
            .last()
            .expect("override chain has at least two entries");
        report.push(Conflict {
            object_id: last.form_id,
            signature: last.signature,
            plugins: versions
                .iter()
                .map(|version| version.plugin.clone())
                .collect(),
            winner: last.plugin.clone(),
            class,
            paths,
        });
    }
    Ok(report)
}

struct Version {
    plugin: String,
    form_id: FormId,
    signature: Signature,
    fields: BTreeMap<String, String>,
}

fn records(plugin: &Plugin) -> impl Iterator<Item = &Record> {
    plugin
        .groups()
        .iter()
        .flat_map(|group| group.records_recursive())
}

fn collect_references(
    value: &FieldValue<'_>,
    path: &str,
    source: &GlobalFormId,
    input: PluginInput<'_>,
    load_order: &LoadOrder,
    graph: &mut ReferenceGraph,
) {
    match value {
        FieldValue::FormId { value, .. } => {
            if let Some(target) = load_order.resolve(*value, input.name, input.plugin.masters()) {
                graph.add(ReferenceEdge {
                    source: source.clone(),
                    target,
                    path: path.to_owned(),
                });
            }
        }
        FieldValue::Struct(fields) => {
            for field in fields {
                collect_references(&field.value, &field.path, source, input, load_order, graph);
            }
        }
        FieldValue::Array(values) => {
            for (index, item) in values.iter().enumerate() {
                collect_references(
                    item,
                    &format!("{path}[{index}]"),
                    source,
                    input,
                    load_order,
                    graph,
                );
            }
        }
        _ => {}
    }
}

fn differing_paths(versions: &[Version]) -> Vec<String> {
    let all_paths: BTreeSet<&str> = versions
        .iter()
        .flat_map(|version| version.fields.keys().map(String::as_str))
        .collect();
    all_paths
        .into_iter()
        .filter(|path| {
            let values: BTreeSet<Option<&str>> = versions
                .iter()
                .map(|version| version.fields.get(*path).map(String::as_str))
                .collect();
            values.len() > 1
        })
        .map(str::to_owned)
        .collect()
}

fn fingerprint(value: &FieldValue<'_>) -> String {
    match value {
        FieldValue::Int(value) => format!("i:{value}"),
        FieldValue::UInt(value) => format!("u:{value}"),
        FieldValue::Float(value) => format!("f:{:016x}", value.to_bits()),
        FieldValue::String(value) => format!("s:{value:?}"),
        FieldValue::FormId { value, targets } => {
            format!("r:{}:{:?}", value.0, targets)
        }
        FieldValue::Enumeration { value, name } => format!("e:{value}:{name:?}"),
        FieldValue::Flags { value, active } => format!("g:{value}:{active:?}"),
        FieldValue::Bytes(value) => format!("b:{}", hex_bytes(value)),
        FieldValue::Struct(fields) => fields
            .iter()
            .map(|field| format!("{}={}", field.path, fingerprint(&field.value)))
            .collect::<Vec<_>>()
            .join(";"),
        FieldValue::Array(values) => values.iter().map(fingerprint).collect::<Vec<_>>().join(","),
        FieldValue::Absent => "absent".to_owned(),
    }
}

fn hex_bytes(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(DIGITS[(byte >> 4) as usize]));
        output.push(char::from(DIGITS[(byte & 0x0f) as usize]));
    }
    output
}
