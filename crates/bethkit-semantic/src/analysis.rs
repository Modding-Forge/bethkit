// SPDX-License-Identifier: Apache-2.0
//!
//! Conflict and reference analysis result models.

use bethkit_core::{FormId, GlobalFormId, Signature};

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
    /// All compared records have identical bytes.
    Identical,
    /// Later override changes data without conflicting sibling changes.
    Override,
    /// Multiple plugins change the same semantic field differently.
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
    /// Stable schema paths with conflicting values.
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
