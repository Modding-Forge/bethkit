// SPDX-License-Identifier: Apache-2.0
//! Differential coverage for decode-only retention, including malformed source bytes.

use std::sync::Arc;

use bethkit_core::{GameContext, Record};
use bethkit_io::SliceCursor;
use bethkit_schema::{
    ArrayCount, ByteOrder, CallbackBinding, CallbackImplementation, Expression, IntegerType,
    PrimitiveType, SchemaNode, SchemaNodeKind, SchemaPackage, SchemaRecord, SchemaRegistry,
    SchemaSignature, UnionSelector,
};

use super::tests::{payload_test_node, test_manifest, test_record_bytes};
use crate::decode_retention::DecodeRetention;
use crate::{DecoderRegistry, SemanticContext};

fn byte(id: u32) -> SchemaNode {
    payload_test_node(
        id,
        &format!("TEST/Data/payload/{id}"),
        SchemaNodeKind::Primitive {
            primitive: PrimitiveType::Integer {
                integer: IntegerType {
                    width: 1,
                    signed: false,
                    byte_order: ByteOrder::LittleEndian,
                },
            },
        },
    )
}

fn context(payload: SchemaNode) -> crate::Result<SemanticContext> {
    let package = SchemaPackage::new(
        test_manifest(),
        vec![SchemaRecord {
            signature: SchemaSignature(*b"TEST"),
            name: "Decode retention fixture".to_owned(),
            root: payload_test_node(
                0,
                "TEST",
                SchemaNodeKind::Sequence {
                    children: vec![payload_test_node(
                        1,
                        "TEST/Data",
                        SchemaNodeKind::Subrecord {
                            signature: SchemaSignature(*b"DATA"),
                            payload: Box::new(payload),
                        },
                    )],
                },
            ),
        }],
    )?;
    SemanticContext::new(Arc::new(package), DecoderRegistry::builtin())
}

fn compare(context: &SemanticContext, payload: &[u8]) -> crate::Result<bool> {
    let source = test_record_bytes(b"TEST", b"DATA", payload);
    let record = Record::parse_header(&mut SliceCursor::new(&source), &GameContext::sse())?;
    let view = context.view(&record, false)?;
    let full = view.fields().map(|_| ()).map_err(|error| error.to_string());
    let checked = view.check_decodable().map_err(|error| error.to_string());
    assert_eq!(checked, full, "source bytes: {payload:?}");
    Ok(full.is_ok())
}

fn exercise(payload: SchemaNode, valid: &[u8]) -> crate::Result<()> {
    let context = context(payload)?;
    assert!(compare(&context, valid)?);
    for length in 0..valid.len() {
        compare(&context, &valid[..length])?;
    }
    for position in 0..valid.len() {
        for replacement in [0, 1, 2, 255] {
            let mut altered = valid.to_vec();
            altered[position] = replacement;
            compare(&context, &altered)?;
        }
    }
    let mut trailing = valid.to_vec();
    trailing.push(255);
    compare(&context, &trailing)?;
    Ok(())
}

/// Preserves exact errors for nested counted arrays, truncation, and trailing bytes.
#[test]
fn nested_arrays_keep_all_decode_checks() -> Result<(), Box<dyn std::error::Error>> {
    for count in [
        ArrayCount::Fixed { count: 2 },
        ArrayCount::Remainder,
        ArrayCount::Prefixed {
            integer: IntegerType {
                width: 1,
                signed: false,
                byte_order: ByteOrder::LittleEndian,
            },
            terminator: None,
        },
        ArrayCount::PackedPrefixed {
            square: false,
            terminator: None,
        },
    ] {
        let source: &[u8] = match count {
            ArrayCount::PackedPrefixed { .. } => &[8, 1, 2, 3, 4],
            ArrayCount::Prefixed { .. } => &[2, 1, 2, 3, 4],
            _ => &[1, 2, 3, 4],
        };
        let payload = payload_test_node(
            2,
            "TEST/Data/payload",
            SchemaNodeKind::Array {
                count,
                element: Box::new(payload_test_node(
                    3,
                    "TEST/Data/payload/element",
                    SchemaNodeKind::Struct {
                        fields: vec![byte(4), byte(5)],
                    },
                )),
            },
        );
        exercise(payload, source)?;
    }
    Ok(())
}

/// Retains numeric dependencies used by counts, conditions, and union selections.
#[test]
fn expressions_keep_numeric_state() -> Result<(), Box<dyn std::error::Error>> {
    let count = byte(3);
    let expression = Expression::ReadField {
        path: count.path.clone(),
    };
    let mut conditional = byte(4);
    conditional.condition = Some(Expression::Bool { value: false });
    let payload = payload_test_node(
        2,
        "TEST/Data/payload",
        SchemaNodeKind::Struct {
            fields: vec![
                count,
                conditional,
                payload_test_node(
                    5,
                    "TEST/Data/payload/array",
                    SchemaNodeKind::Array {
                        count: ArrayCount::Expression {
                            expression: expression.clone(),
                        },
                        element: Box::new(byte(6)),
                    },
                ),
                payload_test_node(
                    7,
                    "TEST/Data/payload/union",
                    SchemaNodeKind::Union {
                        selector: UnionSelector::Expression(expression),
                        variants: vec![byte(8), byte(9), byte(10)],
                    },
                ),
            ],
        },
    );
    exercise(payload, &[2, 7, 8, 9])?;
    Ok(())
}

/// Keeps optional suffix and terminator errors identical without retaining containers.
#[test]
fn optional_and_terminated_nodes_keep_byte_coverage() -> Result<(), Box<dyn std::error::Error>> {
    let payload = payload_test_node(
        2,
        "TEST/Data/payload",
        SchemaNodeKind::Terminated {
            terminator: 255,
            child: Box::new(payload_test_node(
                3,
                "TEST/Data/payload/child",
                SchemaNodeKind::OptionalStruct {
                    optional_from: 1,
                    fields: vec![byte(4), byte(5)],
                },
            )),
        },
    );
    exercise(payload, &[1, 2, 255])?;
    Ok(())
}

/// Rejects zero-progress remainder arrays on the same node and with the same message.
#[test]
fn empty_elements_still_fail() -> Result<(), Box<dyn std::error::Error>> {
    let context = context(payload_test_node(
        2,
        "TEST/Data/payload",
        SchemaNodeKind::Array {
            count: ArrayCount::Remainder,
            element: Box::new(payload_test_node(
                3,
                "TEST/Data/payload/empty",
                SchemaNodeKind::Struct { fields: Vec::new() },
            )),
        },
    ))?;
    assert!(!compare(&context, &[1])?);
    Ok(())
}

/// Preserves conditional fields even when no array or union needs numeric state.
#[test]
fn conditions_alone_keep_prior_numeric_values() -> Result<(), Box<dyn std::error::Error>> {
    let first = byte(3);
    let mut second = byte(4);
    second.condition = Some(Expression::Equal {
        left: Box::new(Expression::ReadField {
            path: first.path.clone(),
        }),
        right: Box::new(Expression::Int { value: 1 }),
    });
    let payload = payload_test_node(
        2,
        "TEST/Data/payload",
        SchemaNodeKind::Struct {
            fields: vec![first, second],
        },
    );
    exercise(payload, &[1, 2])?;
    Ok(())
}

/// Falls back for any value observer, including inactive presentation-only bindings.
#[test]
fn callback_observers_require_complete_containers() -> Result<(), Box<dyn std::error::Error>> {
    let payload = payload_test_node(
        2,
        "TEST/Data/payload",
        SchemaNodeKind::Array {
            count: ArrayCount::Remainder,
            element: Box::new(byte(3)),
        },
    );
    let base = context(payload.clone())?;
    assert!(!DecodeRetention::for_payload(base.registry(), &payload).materializes());
    for role in ["float.normalizer", "array.should_include"] {
        let mut manifest = test_manifest();
        manifest.callbacks_total = 1;
        manifest.callbacks_classified = 1;
        let package = SchemaPackage::new_with_callbacks(
            manifest,
            base.registry().package().records().to_vec(),
            vec![CallbackBinding {
                path: payload.path.clone(),
                callback_id: role.to_owned(),
                callback_slot: None,
                implementation_fingerprint: "00".repeat(32),
                implementation: CallbackImplementation::UserInterfaceOnly,
            }],
        )?;
        let registry = SchemaRegistry::new(Arc::new(package));
        assert!(DecodeRetention::for_payload(&registry, &payload).materializes());
    }
    Ok(())
}

/// Keeps callback-selected array and union payloads on the original decoder path.
#[test]
fn callback_selectors_cannot_discard_sibling_scope() -> Result<(), Box<dyn std::error::Error>> {
    let base = context(byte(2))?;
    for kind in [
        SchemaNodeKind::Array {
            element: Box::new(byte(3)),
            count: ArrayCount::Callback {
                callback_id: "array.count".to_owned(),
            },
        },
        SchemaNodeKind::Union {
            variants: vec![byte(3)],
            selector: UnionSelector::Callback {
                callback_id: "union.select".to_owned(),
            },
        },
        SchemaNodeKind::Reference {
            target: "missing".to_owned(),
        },
    ] {
        let payload = payload_test_node(2, "TEST/Data/payload", kind);
        assert!(DecodeRetention::for_payload(base.registry(), &payload).materializes());
    }
    Ok(())
}

/// Matches ordinary decoding for every integer width, sign and endian combination.
#[test]
fn bulk_integer_checks_preserve_truncation_errors() -> Result<(), Box<dyn std::error::Error>> {
    for width in [1_u8, 2, 4, 8] {
        for signed in [false, true] {
            for byte_order in [ByteOrder::LittleEndian, ByteOrder::BigEndian] {
                let element = payload_test_node(
                    3,
                    "TEST/Data/payload/element",
                    SchemaNodeKind::Primitive {
                        primitive: PrimitiveType::Integer {
                            integer: IntegerType {
                                width,
                                signed,
                                byte_order,
                            },
                        },
                    },
                );
                let payload = payload_test_node(
                    2,
                    "TEST/Data/payload",
                    SchemaNodeKind::Array {
                        count: ArrayCount::Fixed { count: 4 },
                        element: Box::new(element),
                    },
                );
                let source: Vec<u8> = (0..width * 4).collect();
                exercise(payload, &source)?;
            }
        }
    }
    Ok(())
}
