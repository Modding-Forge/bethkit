// SPDX-License-Identifier: Apache-2.0
//!
//! Ordered matching of record subrecords against the schema grammar.

use std::collections::{BTreeMap, BTreeSet};

use bethkit_core::{Signature, SubRecord, WritableSubRecord};
use bethkit_schema::{
    ByteOrder, EvalContext, EvalValue, IntegerType, PrimitiveType, SchemaNode, SchemaNodeKind,
    UnionSelector,
};

use crate::{Result, SemanticError};

pub(crate) struct GrammarMatch<'schema> {
    pub(crate) assignments: Vec<Option<&'schema SchemaNode>>,
    pub(crate) repeat_scopes: Vec<Vec<RepeatScope>>,
    pub(crate) declared_signatures: BTreeSet<Signature>,
    pub(crate) violations: Vec<GrammarViolation>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RepeatScope {
    pub(crate) path: String,
    pub(crate) occurrence: u32,
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
    repeat_scopes: Vec<Vec<RepeatScope>>,
    active_repeat_scopes: Vec<RepeatScope>,
    assigned: usize,
    violations: Vec<GrammarViolation>,
}

pub(crate) fn interpret<'schema>(
    root: &'schema SchemaNode,
    record_signature: Signature,
    form_version: u16,
    subrecords: &[SubRecord],
) -> Result<GrammarMatch<'schema>> {
    interpret_inputs(root, record_signature, form_version, subrecords)
}

pub(crate) fn interpret_writable<'schema>(
    root: &'schema SchemaNode,
    record_signature: Signature,
    form_version: u16,
    subrecords: &[WritableSubRecord],
) -> Result<GrammarMatch<'schema>> {
    interpret_inputs(root, record_signature, form_version, subrecords)
}

trait GrammarInput {
    fn signature(&self) -> Signature;
    fn payload(&self) -> &[u8];
}

impl GrammarInput for SubRecord {
    fn signature(&self) -> Signature {
        self.signature
    }

    fn payload(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl GrammarInput for WritableSubRecord {
    fn signature(&self) -> Signature {
        self.signature
    }

    fn payload(&self) -> &[u8] {
        &self.data
    }
}

fn interpret_inputs<'schema, T: GrammarInput>(
    root: &'schema SchemaNode,
    record_signature: Signature,
    form_version: u16,
    subrecords: &[T],
) -> Result<GrammarMatch<'schema>> {
    let mut declared_signatures = BTreeSet::new();
    collect_signatures(root, &mut declared_signatures);
    let initial = MatchState {
        cursor: 0,
        assignments: vec![None; subrecords.len()],
        repeat_scopes: vec![Vec::new(); subrecords.len()],
        active_repeat_scopes: Vec::new(),
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
        repeat_scopes: state.repeat_scopes,
        declared_signatures,
        violations: state.violations,
    })
}

fn match_node<'schema, T: GrammarInput>(
    node: &'schema SchemaNode,
    state: MatchState<'schema>,
    record_signature: Signature,
    form_version: u16,
    subrecords: &[T],
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
        SchemaNodeKind::SelectedChoice {
            selector,
            alternatives,
        } => {
            let field_values = assigned_numeric_fields(&state, subrecords)?;
            let payload = subrecords
                .get(state.cursor)
                .map_or(&[][..], GrammarInput::payload);
            let index = match selector {
                UnionSelector::Expression(expression) => {
                    let context = EvalContext {
                        payload,
                        field_values: &field_values,
                        form_version,
                        record_signature: record_signature.into(),
                    };
                    match expression.evaluate(&context, 1024)? {
                        EvalValue::Int(value) => value,
                        EvalValue::Bool(_) => {
                            return Err(SemanticError::Decode {
                                path: node.path.clone(),
                                message: "ordered choice selector returned a boolean".to_owned(),
                            });
                        }
                    }
                }
                UnionSelector::Callback { callback_id } => {
                    return Err(SemanticError::Handler {
                        handler: callback_id.clone(),
                        message: format!(
                            "ordered choice {} retained a non-declarative selector",
                            node.path
                        ),
                    });
                }
            };
            let index = usize::try_from(index).map_err(|_| SemanticError::Decode {
                path: node.path.clone(),
                message: format!("ordered choice selected negative alternative {index}"),
            })?;
            let alternative = alternatives
                .get(index)
                .ok_or_else(|| SemanticError::Decode {
                    path: node.path.clone(),
                    message: format!("ordered choice alternative {index} does not exist"),
                })?;
            match_node(
                alternative,
                state,
                record_signature,
                form_version,
                subrecords,
                declared,
            )
        }
        SchemaNodeKind::Repeat {
            minimum,
            maximum,
            child,
        } => {
            let mut current = state;
            let mut count = 0_u32;
            while maximum.is_none_or(|limit| count < limit) {
                let mut scoped = current.clone();
                scoped.active_repeat_scopes.push(RepeatScope {
                    path: child.path.clone(),
                    occurrence: count,
                });
                let candidate = match_node(
                    child,
                    scoped,
                    record_signature,
                    form_version,
                    subrecords,
                    declared,
                )?;
                if candidate.assigned == current.assigned {
                    break;
                }
                current = candidate;
                current.active_repeat_scopes.pop();
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
            if subrecord.signature() != Signature::from(*signature) {
                return Ok(current);
            }
            current.assignments[current.cursor] = Some(node);
            current.repeat_scopes[current.cursor] = current.active_repeat_scopes.clone();
            current.cursor += 1;
            current.assigned += 1;
            Ok(current)
        }
        _ => Ok(state),
    }
}

fn condition_applies<T: GrammarInput>(
    node: &SchemaNode,
    state: &MatchState<'_>,
    record_signature: Signature,
    form_version: u16,
    subrecords: &[T],
) -> Result<bool> {
    let Some(condition) = &node.condition else {
        return Ok(true);
    };
    let payload = subrecords
        .get(state.cursor)
        .map_or(&[][..], GrammarInput::payload);
    let field_values = BTreeMap::new();
    let context = EvalContext {
        payload,
        field_values: &field_values,
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

fn assigned_numeric_fields<T: GrammarInput>(
    state: &MatchState<'_>,
    subrecords: &[T],
) -> Result<BTreeMap<String, i64>> {
    let mut values = BTreeMap::new();
    for (index, assignment) in state.assignments.iter().enumerate() {
        let (Some(node), Some(subrecord)) = (assignment, subrecords.get(index)) else {
            continue;
        };
        if let SchemaNodeKind::Subrecord { payload, .. } = &node.kind {
            collect_numeric_payload_fields(payload, subrecord.payload(), 0, &mut values)?;
            if let Some(value) = numeric_payload_value(payload, subrecord.payload(), 0)? {
                values.insert(node.path.clone(), value);
            }
        }
    }
    Ok(values)
}

fn collect_numeric_payload_fields(
    node: &SchemaNode,
    payload: &[u8],
    offset: usize,
    values: &mut BTreeMap<String, i64>,
) -> Result<Option<usize>> {
    if let Some(value) = numeric_payload_value(node, payload, offset)? {
        values.insert(node.path.clone(), value);
    }
    match &node.kind {
        SchemaNodeKind::Struct { fields } => {
            let mut cursor = offset;
            for field in fields {
                let Some(consumed) =
                    collect_numeric_payload_fields(field, payload, cursor, values)?
                else {
                    return Ok(None);
                };
                cursor = cursor
                    .checked_add(consumed)
                    .ok_or_else(|| SemanticError::Decode {
                        path: field.path.clone(),
                        message: "numeric field cursor overflowed".to_owned(),
                    })?;
            }
            Ok(Some(cursor - offset))
        }
        SchemaNodeKind::Primitive { primitive } => Ok(primitive_fixed_size(primitive)),
        _ => Ok(None),
    }
}

fn numeric_payload_value(node: &SchemaNode, payload: &[u8], offset: usize) -> Result<Option<i64>> {
    let integer = match &node.kind {
        SchemaNodeKind::Primitive {
            primitive:
                PrimitiveType::Integer { integer }
                | PrimitiveType::Enumeration { integer, .. }
                | PrimitiveType::Flags { integer, .. },
        } => *integer,
        _ => return Ok(None),
    };
    let width = integer.width as usize;
    let bytes = payload
        .get(offset..offset.saturating_add(width))
        .ok_or_else(|| SemanticError::Decode {
            path: node.path.clone(),
            message: format!(
                "selector field needs {width} bytes at offset {offset}, payload has {}",
                payload.len()
            ),
        })?;
    Ok(Some(decode_selector_integer(bytes, integer)))
}

fn decode_selector_integer(bytes: &[u8], integer: IntegerType) -> i64 {
    let unsigned = match integer.byte_order {
        ByteOrder::LittleEndian => bytes
            .iter()
            .enumerate()
            .fold(0_u64, |value, (index, byte)| {
                value | (u64::from(*byte) << (index * 8))
            }),
        ByteOrder::BigEndian => bytes
            .iter()
            .fold(0_u64, |value, byte| (value << 8) | u64::from(*byte)),
    };
    if !integer.signed {
        return unsigned as i64;
    }
    let shift = 64_u32.saturating_sub(u32::from(integer.width) * 8);
    ((unsigned << shift) as i64) >> shift
}

fn primitive_fixed_size(primitive: &PrimitiveType) -> Option<usize> {
    match primitive {
        PrimitiveType::Integer { integer }
        | PrimitiveType::Enumeration { integer, .. }
        | PrimitiveType::Flags { integer, .. } => Some(integer.width as usize),
        PrimitiveType::Float { width, .. } => Some(*width as usize),
        PrimitiveType::FormId { .. } => Some(4),
        PrimitiveType::String { string } => string.fixed_length.map(|length| length as usize),
        PrimitiveType::Bytes { length } => length.map(|length| length as usize),
        PrimitiveType::Unused { length } => Some(*length as usize),
        PrimitiveType::PackedUnsigned => None,
    }
}

fn skip_unknown<T: GrammarInput>(
    state: &mut MatchState<'_>,
    subrecords: &[T],
    declared: &BTreeSet<Signature>,
) {
    while subrecords
        .get(state.cursor)
        .is_some_and(|subrecord| !declared.contains(&subrecord.signature()))
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
        SchemaNodeKind::Choice { alternatives }
        | SchemaNodeKind::SelectedChoice { alternatives, .. } => {
            for alternative in alternatives {
                collect_signatures(alternative, output);
            }
        }
        SchemaNodeKind::Repeat { child, .. }
        | SchemaNodeKind::Compressed { child, .. }
        | SchemaNodeKind::Terminated { child, .. }
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
        assert_eq!(
            matched.repeat_scopes[0],
            vec![RepeatScope {
                path: "AAAA".to_owned(),
                occurrence: 0,
            }]
        );
        assert_eq!(
            matched.repeat_scopes[1],
            vec![RepeatScope {
                path: "AAAA".to_owned(),
                occurrence: 1,
            }]
        );
        assert!(matched.repeat_scopes[2].is_empty());
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

    /// Keeps identical signatures attached to their ordered schema paths.
    #[test]
    fn ordered_match_distinguishes_paths_with_the_same_signature(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let mut first = subrecord_node(1, *b"AAAA", true);
        first.path = "TEST/first".to_owned();
        let mut second = subrecord_node(2, *b"AAAA", true);
        second.path = "TEST/second".to_owned();
        let root = SchemaNode {
            id: SchemaNodeId(0),
            path: "TEST".to_owned(),
            name: "Test".to_owned(),
            required: true,
            conflict_priority: ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Sequence {
                children: vec![first, second],
            },
        };
        let source = vec![subrecord(*b"AAAA"), subrecord(*b"AAAA")];
        let writable = vec![
            WritableSubRecord {
                signature: Signature(*b"AAAA"),
                data: Vec::new(),
            },
            WritableSubRecord {
                signature: Signature(*b"AAAA"),
                data: Vec::new(),
            },
        ];

        let matched = interpret(&root, Signature(*b"TEST"), 44, &source)?;
        let editable = interpret_writable(&root, Signature(*b"TEST"), 44, &writable)?;

        for assignments in [&matched.assignments, &editable.assignments] {
            assert_eq!(
                assignments[0].map(|node| node.path.as_str()),
                Some("TEST/first")
            );
            assert_eq!(
                assignments[1].map(|node| node.path.as_str()),
                Some("TEST/second")
            );
        }
        Ok(())
    }

    /// Re-evaluates a declarative choice selector for every repeated group.
    #[test]
    fn ordered_match_selects_choice_from_latest_integer_field(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let type_path = "TEST/items/repeat/type/payload";
        let type_node = SchemaNode {
            id: SchemaNodeId(10),
            path: "TEST/items/repeat/type".to_owned(),
            name: "Type".to_owned(),
            required: true,
            conflict_priority: ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Subrecord {
                signature: SchemaSignature(*b"TYPE"),
                payload: Box::new(SchemaNode {
                    id: SchemaNodeId(11),
                    path: type_path.to_owned(),
                    name: "Type".to_owned(),
                    required: true,
                    conflict_priority: ConflictPriority::Normal,
                    condition: None,
                    kind: SchemaNodeKind::Primitive {
                        primitive: PrimitiveType::Enumeration {
                            integer: IntegerType {
                                width: 1,
                                signed: false,
                                byte_order: ByteOrder::LittleEndian,
                            },
                            values: vec![(0, "A".to_owned()), (1, "B".to_owned())],
                        },
                    },
                }),
            },
        };
        let mut alternative_a = subrecord_node(20, *b"AAAA", true);
        alternative_a.path = "TEST/items/repeat/choice/0:A".to_owned();
        let mut alternative_b = subrecord_node(30, *b"BBBB", true);
        alternative_b.path = "TEST/items/repeat/choice/1:B".to_owned();
        let root = SchemaNode {
            id: SchemaNodeId(0),
            path: "TEST".to_owned(),
            name: "Test".to_owned(),
            required: true,
            conflict_priority: ConflictPriority::Normal,
            condition: None,
            kind: SchemaNodeKind::Repeat {
                minimum: 0,
                maximum: None,
                child: Box::new(SchemaNode {
                    id: SchemaNodeId(1),
                    path: "TEST/items/repeat".to_owned(),
                    name: "Item".to_owned(),
                    required: true,
                    conflict_priority: ConflictPriority::Normal,
                    condition: None,
                    kind: SchemaNodeKind::Sequence {
                        children: vec![
                            type_node,
                            SchemaNode {
                                id: SchemaNodeId(2),
                                path: "TEST/items/repeat/choice".to_owned(),
                                name: "Choice".to_owned(),
                                required: true,
                                conflict_priority: ConflictPriority::Normal,
                                condition: None,
                                kind: SchemaNodeKind::SelectedChoice {
                                    selector: UnionSelector::Expression(
                                        bethkit_schema::Expression::ReadField {
                                            path: type_path.to_owned(),
                                        },
                                    ),
                                    alternatives: vec![alternative_a, alternative_b],
                                },
                            },
                        ],
                    },
                }),
            },
        };
        let subrecords = vec![
            SubRecord {
                signature: Signature(*b"TYPE"),
                data: SubRecordData::Owned(vec![1]),
            },
            subrecord(*b"BBBB"),
            SubRecord {
                signature: Signature(*b"TYPE"),
                data: SubRecordData::Owned(vec![0]),
            },
            subrecord(*b"AAAA"),
        ];

        let matched = interpret(&root, Signature(*b"TEST"), 44, &subrecords)?;

        assert_eq!(
            matched.assignments[1].map(|node| node.path.as_str()),
            Some("TEST/items/repeat/choice/1:B")
        );
        assert_eq!(
            matched.assignments[3].map(|node| node.path.as_str()),
            Some("TEST/items/repeat/choice/0:A")
        );
        assert!(matched.violations.is_empty());
        Ok(())
    }
}
