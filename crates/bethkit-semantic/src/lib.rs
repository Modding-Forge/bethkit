// SPDX-License-Identifier: Apache-2.0
//!
//! Schema-guided semantic operations over losslessly parsed Bethesda plugins.

mod analysis;
mod cleaning;
mod context;
mod decoder;
mod editor;
mod error;
mod grammar;
mod handler;
mod validation;
mod value;
mod view;

pub use analysis::{
    analyze_conflicts, build_reference_graph, Conflict, ConflictClass, ConflictReport, PluginInput,
    ReferenceEdge, ReferenceGraph,
};
pub use cleaning::{
    plan_cleaning, CleaningAction, CleaningActionKind, CleaningPlan, CleaningPolicy,
};
pub use context::SemanticContext;
pub use decoder::{CustomDecoder, DecodedPayload, DecoderRegistry};
pub use editor::RecordEditor;
pub use error::{Result, SemanticError};
pub use handler::{
    FormLinkInfo, FormLinkResolver, HandlerContext, HandlerInvocation, HandlerMutation,
    HandlerOutput, HandlerPhase, HandlerRecordContext, QuestAliasInfo, QuestStageInfo,
    RecordGridCell, RecordIndexKey, ResourceHashResolver, SemanticHandler, SemanticHandlerRegistry,
    SemanticLink, ValueFormat, WwiseGuidInfo, WwiseGuidResolver,
};
pub use validation::{Diagnostic, DiagnosticCode, DiagnosticSeverity, ValidationReport};
pub use value::{ByteSpan, FieldOrigin, FieldValue, NamedValue, OwnedFieldValue};
pub use view::{Field, RecordView};
