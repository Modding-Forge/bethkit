// SPDX-License-Identifier: Apache-2.0
//!
//! Exact-length regression fixtures for retained Skyrim payload layouts.

use std::sync::Arc;

use bethkit_core::{GameContext, Record, Signature};
use bethkit_io::SliceCursor;
use bethkit_schema::{
    ArrayCount, ByteOrder, ConflictPriority, Expression, IntegerType, PrimitiveType, SchemaNode,
    SchemaNodeKind, SchemaPackage, SchemaRecord, SchemaSignature, StringLengthPrefix, StringType,
};

use super::tests::{payload_test_node, test_manifest, test_record_bytes};
use crate::{DecoderRegistry, FieldValue, OwnedFieldValue, SemanticContext};

/// Matches xEdit's observed one-fragment INFO representation while retaining both flag bits.
#[test]
fn info_fragment_flags_bound_complete_tail_entries() -> Result<(), Box<dyn std::error::Error>> {
    let parent = "INFO/1:Virtual Machine Adapter/payload/3:Script Fragments";
    let integer = IntegerType {
        width: 1,
        signed: false,
        byte_order: ByteOrder::LittleEndian,
    };
    let string = PrimitiveType::String {
        string: StringType {
            encoding: "utf8".to_owned(),
            localized: false,
            zero_terminated: false,
            fixed_length: None,
            length_prefix: Some(StringLengthPrefix {
                width: 2,
                offset: 2,
            }),
            trailing_terminator: None,
            allowed_values: Vec::new(),
        },
    };
    let primitive = |id, suffix: &str, primitive| {
        payload_test_node(
            id,
            &format!("{parent}/{suffix}"),
            SchemaNodeKind::Primitive { primitive },
        )
    };
    let mut payload = payload_test_node(
        2,
        parent,
        SchemaNodeKind::Struct {
            fields: vec![
                primitive(3, "0:Version", PrimitiveType::Integer { integer }),
                primitive(4, "1:Flags", PrimitiveType::Integer { integer }),
                primitive(5, "2:FileName", string.clone()),
                payload_test_node(
                    6,
                    &format!("{parent}/3:Fragments"),
                    SchemaNodeKind::Array {
                        element: Box::new(payload_test_node(
                            7,
                            &format!("{parent}/3:Fragments/element"),
                            SchemaNodeKind::Struct {
                                fields: vec![
                                    primitive(
                                        8,
                                        "3:Fragments/element/0:Unknown",
                                        PrimitiveType::Integer { integer },
                                    ),
                                    primitive(
                                        9,
                                        "3:Fragments/element/1:ScriptName",
                                        string.clone(),
                                    ),
                                    primitive(10, "3:Fragments/element/2:FragmentName", string),
                                ],
                            },
                        )),
                        count: ArrayCount::Expression {
                            expression: Expression::BitCount {
                                value: Box::new(Expression::ReadField {
                                    path: format!("{parent}/1:Flags"),
                                }),
                            },
                        },
                    },
                ),
            ],
        },
    );
    let compatible = context(*b"INFO", *b"VMAD", payload.clone())?;
    let bytes = [2, 3, 1, 0, b'F', 1, 1, 0, b'S', 1, 0, b'E'];
    let data = test_record_bytes(b"INFO", b"VMAD", &bytes);
    let record = Record::parse_header(&mut SliceCursor::new(&data), &GameContext::sse())?;
    let decoded = compatible.view(&record, false)?.fields()?;
    let FieldValue::Struct(fields) = &decoded[0].value else {
        panic!("expected fragment structure")
    };
    assert!(matches!(&fields[3].value, FieldValue::Array(values) if values.len() == 1));
    let replacement =
        crate::value::handler_to_owned_value(decoded[0].value.to_handler_value(), parent)?;
    let mut editor = compatible.edit(&record, false)?;
    editor.set("TEST/0:Data", 0, &replacement)?;
    assert_eq!(editor.into_writable_record().subrecords[0].data, bytes);
    let truncated = test_record_bytes(b"INFO", b"VMAD", &bytes[..bytes.len() - 1]);
    let record = Record::parse_header(&mut SliceCursor::new(&truncated), &GameContext::sse())?;
    assert!(compatible.view(&record, false)?.fields().is_err());
    let SchemaNodeKind::Struct { fields } = &mut payload.kind else {
        panic!("expected structure")
    };
    let SchemaNodeKind::Array { count, .. } = &mut fields[3].kind else {
        panic!("expected array")
    };
    *count = ArrayCount::Fixed { count: 2 };
    let strict = context(*b"INFO", *b"VMAD", payload)?;
    let record = Record::parse_header(&mut SliceCursor::new(&data), &GameContext::sse())?;
    assert!(strict.view(&record, false)?.fields().is_err());
    Ok(())
}

fn context(
    signature: [u8; 4],
    subrecord: [u8; 4],
    payload: SchemaNode,
) -> crate::Result<SemanticContext> {
    let package = SchemaPackage::new(
        test_manifest(),
        vec![SchemaRecord {
            signature: SchemaSignature(signature),
            name: "Legacy fixture".to_owned(),
            root: payload_test_node(
                0,
                "TEST",
                SchemaNodeKind::Sequence {
                    children: vec![
                        payload_test_node(
                            1,
                            "TEST/0:Data",
                            SchemaNodeKind::Subrecord {
                                signature: SchemaSignature(subrecord),
                                payload: Box::new(payload),
                            },
                        ),
                        payload_test_node(
                            100,
                            "TEST/1:Name",
                            SchemaNodeKind::Subrecord {
                                signature: SchemaSignature(*b"FULL"),
                                payload: Box::new(payload_test_node(
                                    101,
                                    "TEST/1:Name/payload",
                                    SchemaNodeKind::Primitive {
                                        primitive: PrimitiveType::String {
                                            string: StringType {
                                                encoding: "windows_1252".to_owned(),
                                                localized: true,
                                                zero_terminated: true,
                                                fixed_length: None,
                                                length_prefix: None,
                                                trailing_terminator: None,
                                                allowed_values: Vec::new(),
                                            },
                                        },
                                    },
                                )),
                            },
                        ),
                    ],
                },
            ),
        }],
    )?;
    SemanticContext::new(Arc::new(package), DecoderRegistry::builtin())
}

/// Accepts exactly one optional terminator while preserving the source bytes on editing.
#[test]
fn fixed_string_optional_terminator_is_lossless() -> Result<(), Box<dyn std::error::Error>> {
    let payload = payload_test_node(
        2,
        "TEST/0:Data/payload",
        SchemaNodeKind::Primitive {
            primitive: PrimitiveType::String {
                string: StringType {
                    encoding: "windows_1252".to_owned(),
                    localized: false,
                    zero_terminated: false,
                    fixed_length: Some(4),
                    length_prefix: None,
                    trailing_terminator: None,
                    allowed_values: Vec::new(),
                },
            },
        },
    );
    let context = context(*b"RACE", *b"MTNM", payload)?;
    for bytes in [b"WALK".as_slice(), b"WALK\0".as_slice()] {
        let data = test_record_bytes(b"RACE", b"MTNM", bytes);
        let record = Record::parse_header(&mut SliceCursor::new(&data), &GameContext::sse())?;
        let fields = context.view(&record, false)?.fields()?;
        assert!(matches!(&fields[0].value, FieldValue::String(value) if value == "WALK"));
        assert_eq!(fields[0].span.end, bytes.len());
        assert_eq!(
            context
                .edit(&record, false)?
                .into_writable_record()
                .subrecords[0]
                .data,
            bytes
        );
        let mut editor = context.edit(&record, false)?;
        editor.set(
            "TEST/0:Data",
            0,
            &OwnedFieldValue::String("SWIM".to_owned()),
        )?;
        let changed = editor.into_writable_record();
        let expected = if bytes.len() == 5 {
            b"SWIM\0".as_slice()
        } else {
            b"SWIM".as_slice()
        };
        assert_eq!(changed.subrecords[0].data, expected);
    }
    for bytes in [
        b"WALKX".as_slice(),
        b"WALK\0\0".as_slice(),
        b"WAL".as_slice(),
    ] {
        let data = test_record_bytes(b"RACE", b"MTNM", bytes);
        let record = Record::parse_header(&mut SliceCursor::new(&data), &GameContext::sse())?;
        assert!(context.view(&record, false)?.fields().is_err());
    }
    Ok(())
}

/// Keeps an empty ignored MESG icon absent rather than inventing a zero FormID.
#[test]
fn unused_icon_is_absent_but_required_form_ids_stay_strict(
) -> Result<(), Box<dyn std::error::Error>> {
    let mut payload = payload_test_node(
        2,
        "TEST/0:Data/payload",
        SchemaNodeKind::Primitive {
            primitive: PrimitiveType::FormId {
                targets: vec![SchemaSignature(*b"NULL")],
            },
        },
    );
    payload.conflict_priority = ConflictPriority::Ignore;
    let context = context(*b"MESG", *b"INAM", payload.clone())?;
    let data = test_record_bytes(b"MESG", b"INAM", &[]);
    let record = Record::parse_header(&mut SliceCursor::new(&data), &GameContext::sse())?;
    assert!(matches!(
        context.view(&record, false)?.fields()?[0].value,
        FieldValue::Absent
    ));
    assert!(context
        .edit(&record, false)?
        .into_writable_record()
        .subrecords[0]
        .data
        .is_empty());
    let other = test_record_bytes(b"MESG", b"INAM", &[0, 0]);
    let record = Record::parse_header(&mut SliceCursor::new(&other), &GameContext::sse())?;
    assert!(context.view(&record, false)?.fields().is_err());
    payload.conflict_priority = ConflictPriority::Normal;
    let strict = self::context(*b"MESG", *b"INAM", payload)?;
    let record = Record::parse_header(&mut SliceCursor::new(&data), &GameContext::sse())?;
    assert!(strict.view(&record, false)?.fields().is_err());
    Ok(())
}

/// Reads the legacy CRDT Effect at byte twelve without padding or reserializing the source.
#[test]
fn legacy_critical_data_keeps_effect_offset_and_original_bytes(
) -> Result<(), Box<dyn std::error::Error>> {
    let primitives = vec![
        PrimitiveType::Integer {
            integer: IntegerType {
                width: 2,
                signed: false,
                byte_order: ByteOrder::LittleEndian,
            },
        },
        PrimitiveType::Bytes { length: Some(2) },
        PrimitiveType::Float {
            width: 4,
            byte_order: ByteOrder::LittleEndian,
            scale: 1.0,
            digits: 6,
        },
        PrimitiveType::Integer {
            integer: IntegerType {
                width: 1,
                signed: false,
                byte_order: ByteOrder::LittleEndian,
            },
        },
        PrimitiveType::Bytes { length: Some(7) },
        PrimitiveType::FormId {
            targets: Vec::new(),
        },
        PrimitiveType::Bytes { length: Some(4) },
    ];
    let fields = primitives
        .into_iter()
        .enumerate()
        .map(|(index, primitive)| {
            payload_test_node(
                index as u32 + 3,
                &format!("TEST/0:Data/payload/{index}"),
                SchemaNodeKind::Primitive { primitive },
            )
        })
        .collect();
    let context = context(
        *b"WEAP",
        *b"CRDT",
        payload_test_node(2, "TEST/0:Data/payload", SchemaNodeKind::Struct { fields }),
    )?;
    let mut bytes = vec![0; 16];
    bytes[12..16].copy_from_slice(&0x12345678_u32.to_le_bytes());
    let mut data = test_record_bytes(b"WEAP", b"CRDT", &bytes);
    data.extend_from_slice(b"FULL\x07\0Before\0");
    let size = (data.len() - 24) as u32;
    data[4..8].copy_from_slice(&size.to_le_bytes());
    let record = Record::parse_header(&mut SliceCursor::new(&data), &GameContext::sse())?;
    let decoded = context.view(&record, false)?.fields()?;
    let FieldValue::Struct(fields) = &decoded[0].value else {
        panic!("expected critical structure")
    };
    assert!(matches!(fields[5].value, FieldValue::FormId { value, .. } if value.0 == 0x12345678));
    assert_eq!(fields[5].span.start, 12);
    assert_eq!(
        context
            .edit(&record, false)?
            .into_writable_record()
            .subrecords[0]
            .data,
        bytes
    );
    assert_eq!(record.header.signature, Signature(*b"WEAP"));
    let mut editor = context.edit(&record, false)?;
    editor.set(
        "TEST/1:Name",
        0,
        &OwnedFieldValue::String("After".to_owned()),
    )?;
    let changed = editor.into_writable_record();
    assert_eq!(changed.subrecords[0].data, bytes);
    assert_eq!(changed.subrecords[1].data, b"After\0");
    let mut replacement = crate::value::handler_to_owned_value(
        decoded[0].value.to_handler_value(),
        "TEST/0:Data/payload",
    )?;
    let OwnedFieldValue::Struct(values) = &mut replacement else {
        panic!("expected editable critical structure")
    };
    values[0] = OwnedFieldValue::UInt(7);
    let mut editor = context.edit(&record, false)?;
    editor.set("TEST/0:Data", 0, &replacement)?;
    let changed = editor.into_writable_record();
    bytes[..2].copy_from_slice(&7_u16.to_le_bytes());
    assert_eq!(changed.subrecords[0].data, bytes);
    assert_eq!(changed.subrecords[1].data, b"Before\0");
    Ok(())
}
