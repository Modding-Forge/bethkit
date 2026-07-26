// SPDX-License-Identifier: Apache-2.0
//!
//! Transactional cleaning and normalization plans.

use bethkit_core::{FormId, RecordFlags, Signature};

use crate::{ConflictClass, ConflictReport, PluginInput};

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
    /// Plugin containing the affected record.
    pub plugin: String,
    /// Human-readable rationale.
    pub reason: String,
    /// Whether differential validation has approved this action.
    pub differential_validated: bool,
}

/// Differential-validation gates for game-specific cleaning rules.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CleaningPolicy {
    identical_to_master: bool,
    undelete_and_disable: bool,
    deleted_navmesh: bool,
}

impl CleaningPolicy {
    /// Creates a policy from externally verified differential-test results.
    pub const fn from_validated_rules(
        identical_to_master: bool,
        undelete_and_disable: bool,
        deleted_navmesh: bool,
    ) -> Self {
        Self {
            identical_to_master,
            undelete_and_disable,
            deleted_navmesh,
        }
    }

    /// Returns a policy that permits no mutating cleaning rules.
    pub const fn disabled() -> Self {
        Self {
            identical_to_master: false,
            undelete_and_disable: false,
            deleted_navmesh: false,
        }
    }
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

/// Produces a non-mutating cleaning plan.
///
/// Rules are emitted only when the corresponding policy gate says that the
/// game-specific behavior has passed differential validation against xEdit.
pub fn plan_cleaning(
    conflicts: &ConflictReport,
    plugins: &[PluginInput<'_>],
    policy: CleaningPolicy,
) -> CleaningPlan {
    let mut plan = CleaningPlan::new();
    if policy.identical_to_master {
        for conflict in conflicts
            .conflicts()
            .iter()
            .filter(|conflict| conflict.class == ConflictClass::Identical)
        {
            plan.push(CleaningAction {
                kind: CleaningActionKind::RemoveIdenticalToMaster,
                signature: conflict.signature,
                form_id: conflict.object_id,
                plugin: conflict.winner.clone(),
                reason: "winning override is semantically identical to its master".to_owned(),
                differential_validated: true,
            });
        }
    }

    for input in plugins {
        for record in input
            .plugin
            .groups()
            .iter()
            .flat_map(|group| group.records_recursive())
            .filter(|record| record.header.flags.contains(RecordFlags::DELETED))
        {
            let is_reference = matches!(
                record.header.signature.0,
                [b'R', b'E', b'F', b'R'] | [b'A', b'C', b'H', b'R'] | [b'A', b'C', b'R', b'E']
            );
            if is_reference && policy.undelete_and_disable {
                plan.push(CleaningAction {
                    kind: CleaningActionKind::UndeleteAndDisableReference,
                    signature: record.header.signature,
                    form_id: record.header.form_id,
                    plugin: input.name.to_owned(),
                    reason: "deleted reference requires xEdit-compatible UDR repair".to_owned(),
                    differential_validated: true,
                });
            }
            if record.header.signature.0 == *b"NAVM" && policy.deleted_navmesh {
                plan.push(CleaningAction {
                    kind: CleaningActionKind::RepairDeletedNavigationMesh,
                    signature: record.header.signature,
                    form_id: record.header.form_id,
                    plugin: input.name.to_owned(),
                    reason: "deleted navigation mesh requires game-specific repair".to_owned(),
                    differential_validated: true,
                });
            }
        }
    }
    plan
}
