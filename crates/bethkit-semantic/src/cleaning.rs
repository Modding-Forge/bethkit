// SPDX-License-Identifier: Apache-2.0
//!
//! Transactional cleaning and normalization plans.

use bethkit_core::{FormId, Signature};

/// Supported dry-run cleaning action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CleaningActionKind {
    /// Remove an identical-to-master override.
    RemoveIdenticalToMaster,
    /// Undelete and disable a deleted reference.
    UndeleteAndDisableReference,
    /// Repair a deleted navigation mesh through a validated game rule.
    RepairDeletedNavigationMesh,
    /// Normalize schema-defined ordering or representation.
    Normalize,
}

/// One proposed transactional cleaning action.
#[derive(Debug, Clone)]
pub struct CleaningAction {
    /// Action kind.
    pub kind: CleaningActionKind,
    /// Record signature.
    pub signature: Signature,
    /// Record FormID.
    pub form_id: FormId,
    /// Human-readable rationale.
    pub reason: String,
    /// Whether differential validation has approved this action.
    pub differential_validated: bool,
}

/// Dry-run plan produced before any cleaning changes are applied.
#[derive(Debug, Default, Clone)]
pub struct CleaningPlan {
    actions: Vec<CleaningAction>,
}

impl CleaningPlan {
    /// Creates an empty cleaning plan.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a validated cleaning action.
    pub fn push(&mut self, action: CleaningAction) {
        self.actions.push(action);
    }

    /// Returns all proposed actions.
    pub fn actions(&self) -> &[CleaningAction] {
        &self.actions
    }

    /// Returns whether every action passed differential validation.
    pub fn is_applicable(&self) -> bool {
        self.actions
            .iter()
            .all(|action| action.differential_validated)
    }
}
