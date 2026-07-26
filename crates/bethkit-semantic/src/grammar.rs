// SPDX-License-Identifier: Apache-2.0
//!
//! Ordered matching of record subrecords against the schema grammar.

use std::collections::BTreeSet;

use bethkit_core::{Signature, SubRecord};
use bethkit_schema::{EvalContext, EvalValue, SchemaNode, SchemaNodeKind};

use crate::{Result, SemanticError};

pub(crate) struct GrammarMatch<'schema> {
    pub(crate) assignments: Vec<Option<&'schema SchemaNode>>,
    pub(crate) declared_signatures: BTreeSet<Signature>,
    pub(crate) violations: Vec<GrammarViolation>,
}

#[derive(Clone)]
pub(crate) struct GrammarViolation {
    pub(crate) path: String,
    pub(crate) minimum: u32,
    pub(crate) actual: u32,
}

#[derive(Clone)]
struct MatchState<'schema> {
    cursor: usize,
    assignments: Vec<Option<&'schema SchemaNode>>,
    assigned: usize,
    violations: Vec<GrammarViolation>,
}

pub(crate) fn interpret<'schema>(
    root: &'schema SchemaNode,
    record_signature: Signature,
    form_version: u16,
    subrecords: &[SubRecord],
) -> Result<GrammarMatch<'schema>> {
    let mut declared_signatures = BTreeSet::new();
    collect_signatures(root, &mut declared_signatures);
    let initial = MatchState {
        cursor: 0,
        assignments: vec![None; subrecords.len()],
        assigned: 0,
        violations: Vec::new(),
    };
    let state = match_node(
        root,
        initial,
        record_signature,
        form_version,
        subrecords,
        &declared_signatures,
    )?;
    Ok(GrammarMatch {
        assignments: state.assignments,
        declared_signatures,
        violations: state.violations,
    })
}

fn match_node<'schema>(
    node: &'schema SchemaNode,
    state: MatchState<'schema>,
    record_signature: Signature,
    form_version: u16,
    subrecords: &[SubRecord],
    declared: &BTreeSet<Signature>,
) -> Result<MatchState<'schema>> {
    if !condition_applies(node, &state, record_signature, form_version, subrecords)? {
        return Ok(state);
    }
    match &node.kind {
        SchemaNodeKind::Sequence { children } => {
            let mut current = state;
            for child in children {
                current = match_node(
                    child,
                    current,
                    record_signature,
                    form_version,
                    subrecords,
                    declared,
                )?;
            }
            Ok(current)
        }
        SchemaNodeKind::Choice { alternatives } => {
            let mut best = state.clone();
            for alternative in alternatives {
                let candidate = match_node(
                    alternative,
                    state.clone(),
                    record_signature,
                    form_version,
                    subrecords,
                    declared,
                )?;
                if candidate.assigned > best.assigned
                    || (candidate.assigned == best.assigned && candidate.cursor > best.cursor)
                {
                    best = candidate;
                }
            }
            Ok(best)
        }
        SchemaNodeKind::Repeat {
            minimum,
            maximum,
            child,
        } => {
            let mut current = state;
            let mut count = 0_u32;
            while maximum.is_none_or(|limit| count < limit) {
                let candidate = match_node(
                    child,
                    current.clone(),
                    record_signature,
                    form_version,
                    subrecords,
                    declared,
                )?;
                if candidate.assigned == current.assigned {
                    break;
                }
                current = candidate;
                count += 1;
            }
            if count < *minimum {
                current.violations.push(GrammarViolation {
                    path: node.path.clone(),
                    minimum: *minimum,
                    actual: count,
                });
            }
            Ok(current)
        }
        SchemaNodeKind::Subrecord { signature, .. } => {
            let mut current = state;
            skip_unknown(&mut current, subrecords, declared);
            let Some(subrecord) = subrecords.get(current.cursor) else {
                return Ok(current);
            };
            if subrecord.signature != Signature::from(*signature) {
                return Ok(current);
            }
            current.assignments[current.cursor] = Some(node);
            current.cursor += 1;
            current.assigned += 1;
            Ok(current)
        }
        _ => Ok(state),
    }
}

fn condition_applies(
    node: &SchemaNode,
    state: &MatchState<'_>,
    record_signature: Signature,
    form_version: u16,
    subrecords: &[SubRecord],
) -> Result<bool> {
    let Some(condition) = &node.condition else {
        return Ok(true);
    };
    let payload = subrecords
        .get(state.cursor)
        .map_or(&[][..], SubRecord::as_bytes);
    let context = EvalContext {
        payload,
        form_version,
        record_signature: record_signature.into(),
    };
    match condition.evaluate(&context, 1024)? {
        EvalValue::Bool(value) => Ok(value),
        _ => Err(SemanticError::Decode {
            path: node.path.clone(),
            message: "grammar condition did not return a boolean".to_owned(),
        }),
    }
}

fn skip_unknown(
    state: &mut MatchState<'_>,
    subrecords: &[SubRecord],
    declared: &BTreeSet<Signature>,
) {
    while subrecords
        .get(state.cursor)
        .is_some_and(|subrecord| !declared.contains(&subrecord.signature))
    {
        state.cursor += 1;
    }
}

fn collect_signatures(node: &SchemaNode, output: &mut BTreeSet<Signature>) {
    match &node.kind {
        SchemaNodeKind::Sequence { children } => {
            for child in children {
                collect_signatures(child, output);
            }
        }
        SchemaNodeKind::Choice { alternatives } => {
            for alternative in alternatives {
                collect_signatures(alternative, output);
            }
        }
        SchemaNodeKind::Repeat { child, .. }
        | SchemaNodeKind::Compressed { child, .. }
        | SchemaNodeKind::Array { element: child, .. } => collect_signatures(child, output),
        SchemaNodeKind::Subrecord { signature, .. } => {
            output.insert(Signature::from(*signature));
        }
        SchemaNodeKind::Struct { fields } => {
            for field in fields {
                collect_signatures(field, output);
            }
        }
        SchemaNodeKind::Union { variants, .. } => {
            for variant in variants {
                collect_signatures(variant, output);
            }
        }
        SchemaNodeKind::Primitive { .. }
        | SchemaNodeKind::Custom { .. }
        | SchemaNodeKind::Reference { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    use bethkit_core::SubRecordData;
    use bethkit_schema::{ConflictPriority, SchemaNodeId, SchemaSignature};

    use super::*;

    fn subrecord_node(id: u32, signature: [u8; 4], required: bool) -> SchemaNode {
        SchemaNode {
            id: SchemaNodeId(id),
            path: String::from_utf8_lossy(&signature).into_owned(),
            name: "Test".to_owned(),
            required,
            conflict_priority: ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Subrecord {
                signature: SchemaSignature(signature),
                payload: Box::new(SchemaNode {
                    id: SchemaNodeId(id + 100),
                    path: format!("{id}/payload"),
                    name: "Payload".to_owned(),
                    required: true,
                    conflict_priority: ConflictPriority::Normal,
                    condition: None,
                    kind: SchemaNodeKind::Primitive {
                        primitive: bethkit_schema::PrimitiveType::Bytes { length: None },
                    },
                }),
            },
        }
    }

    fn subrecord(signature: [u8; 4]) -> SubRecord {
        SubRecord {
            signature: Signature(signature),
            data: SubRecordData::Owned(Vec::new()),
        }
    }

    /// Preserves unknown fields while refusing to reinterpret out-of-order known fields.
    #[test]
    fn ordered_match_skips_only_unknown_signatures(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let root = SchemaNode {
            id: SchemaNodeId(0),
            path: "TEST".to_owned(),
            name: "Test".to_owned(),
            required: true,
            conflict_priority: ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Sequence {
                children: vec![
                    subrecord_node(1, *b"AAAA", false),
                    subrecord_node(2, *b"BBBB", true),
                ],
            },
        };
        let subrecords = vec![
            subrecord(*b"XXXX"),
            subrecord(*b"BBBB"),
            subrecord(*b"AAAA"),
        ];

        // when
        let matched = interpret(&root, Signature(*b"TEST"), 44, &subrecords)?;

        // then
        assert!(matched.assignments[0].is_none());
        assert_eq!(
            matched.assignments[1].map(|node| node.id),
            Some(SchemaNodeId(2))
        );
        assert!(matched.assignments[2].is_none());
        assert!(!matched.declared_signatures.contains(&Signature(*b"XXXX")));
        assert!(matched.violations.is_empty());
        Ok(())
    }

    /// Consumes repeated grammar groups before advancing to the next declaration.
    #[test]
    fn ordered_match_consumes_repetitions() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let root = SchemaNode {
            id: SchemaNodeId(0),
            path: "TEST".to_owned(),
            name: "Test".to_owned(),
            required: true,
            conflict_priority: ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Sequence {
                children: vec![
                    SchemaNode {
                        id: SchemaNodeId(10),
                        path: "repeat".to_owned(),
                        name: "Repeat".to_owned(),
                        required: false,
                        conflict_priority: ConflictPriority::Normal,
                        condition: None,
                        kind: SchemaNodeKind::Repeat {
                            minimum: 0,
                            maximum: None,
                            child: Box::new(subrecord_node(1, *b"AAAA", false)),
                        },
                    },
                    subrecord_node(2, *b"BBBB", true),
                ],
            },
        };
        let subrecords = vec![
            subrecord(*b"AAAA"),
            subrecord(*b"AAAA"),
            subrecord(*b"BBBB"),
        ];

        // when
        let matched = interpret(&root, Signature(*b"TEST"), 44, &subrecords)?;

        // then
        assert!(matched.assignments.iter().all(Option::is_some));
        assert_eq!(
            matched.assignments[2].map(|node| node.id),
            Some(SchemaNodeId(2))
        );
        assert!(matched.violations.is_empty());
        Ok(())
    }

    /// Reports an unmet repetition minimum without inventing a match.
    #[test]
    fn ordered_match_reports_repetition_minimum(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let root = SchemaNode {
            id: SchemaNodeId(0),
            path: "TEST".to_owned(),
            name: "Test".to_owned(),
            required: true,
            conflict_priority: ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Repeat {
                minimum: 2,
                maximum: Some(3),
                child: Box::new(subrecord_node(1, *b"AAAA", false)),
            },
        };
        let subrecords = vec![subrecord(*b"AAAA")];

        // when
        let matched = interpret(&root, Signature(*b"TEST"), 44, &subrecords)?;

        // then
        assert_eq!(matched.violations.len(), 1);
        assert_eq!(matched.violations[0].path, "TEST");
        assert_eq!(matched.violations[0].minimum, 2);
        assert_eq!(matched.violations[0].actual, 1);
        Ok(())
    }
}
