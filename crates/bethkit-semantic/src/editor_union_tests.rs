// SPDX-License-Identifier: Apache-2.0
//! Regression fixtures for MGEF's node-relative forward union selector.

use std::sync::Arc;

use bethkit_core::{FormId, Record, RecordFlags, Signature, WritableRecord, WritableSubRecord};
use bethkit_schema::{
    BuiltInOperation, ByteOrder, CallbackBinding, CallbackImplementation, ConflictPriority,
    Expression, HandlerRequirement, IntegerLookupCase, IntegerType, PrimitiveType, SchemaNode,
    SchemaNodeId, SchemaNodeKind, SchemaPackage, SchemaRecord, SchemaSignature, StringType,
    UnionSelector,
};

use crate::{DecoderRegistry, FieldValue, OwnedFieldValue, SemanticContext};

const DATA: &str = "MGEF/1:Data";
const ASSOC: &str = "MGEF/1:Data/payload/2:Assoc. Item";
const ARCHETYPE: &str = "MGEF/1:Data/payload/11:Archtype";
const COUNTER: &str = "MGEF/1:Data/payload/5:Counter Effect count";

fn node(id: u32, path: &str, kind: SchemaNodeKind) -> SchemaNode {
    SchemaNode {
        id: SchemaNodeId(id),
        path: path.to_owned(),
        name: path
            .rsplit('/')
            .next()
            .expect("fixture path is non-empty")
            .to_owned(),
        required: true,
        conflict_priority: ConflictPriority::Normal,
        condition: None,
        kind,
    }
}

fn primitive(id: u32, path: &str, primitive: PrimitiveType) -> SchemaNode {
    node(id, path, SchemaNodeKind::Primitive { primitive })
}

fn integer(width: u8) -> PrimitiveType {
    PrimitiveType::Integer {
        integer: IntegerType {
            width,
            signed: false,
            byte_order: ByteOrder::LittleEndian,
        },
    }
}

fn fixture() -> crate::Result<SemanticContext> {
    fixture_with_width(4)
}

fn fixture_with_width(archetype_width: u8) -> crate::Result<SemanticContext> {
    let fields = vec![
        primitive(3, "MGEF/1:Data/payload/0:Flags", integer(4)),
        primitive(4, "MGEF/1:Data/payload/1:Base", integer(4)),
        node(
            5,
            ASSOC,
            SchemaNodeKind::Union {
                selector: UnionSelector::Expression(Expression::IntegerLookup {
                    value: Box::new(Expression::ReadUnsigned {
                        offset: 56,
                        width: archetype_width,
                    }),
                    cases: vec![IntegerLookupCase {
                        input: 12,
                        output: 1,
                    }],
                    default: 0,
                }),
                variants: vec![
                    primitive(6, &format!("{ASSOC}/variants/0:Unused"), integer(4)),
                    primitive(7, &format!("{ASSOC}/variants/1:Light"), integer(4)),
                ],
            },
        ),
        primitive(8, "MGEF/1:Data/payload/3:Skill", integer(4)),
        primitive(9, "MGEF/1:Data/payload/4:Resist", integer(4)),
        primitive(10, COUNTER, integer(2)),
        primitive(11, "MGEF/1:Data/payload/6:Unused", integer(2)),
        primitive(
            12,
            "MGEF/1:Data/payload/7:Middle",
            PrimitiveType::Bytes { length: Some(32) },
        ),
        primitive(13, "MGEF/1:Data/payload/8:Taper", integer(4)),
        primitive(14, "MGEF/1:Data/payload/9:Second", integer(4)),
        primitive(15, ARCHETYPE, integer(archetype_width)),
        primitive(
            16,
            "MGEF/1:Data/payload/12:Tail",
            PrimitiveType::Bytes {
                length: Some(88 - u32::from(archetype_width)),
            },
        ),
    ];
    let name = primitive(
        19,
        "MGEF/0:Name/payload",
        PrimitiveType::String {
            string: StringType {
                encoding: "windows_1252".to_owned(),
                localized: false,
                zero_terminated: true,
                fixed_length: None,
                length_prefix: None,
                trailing_terminator: None,
                allowed_values: Vec::new(),
            },
        },
    );
    let root = node(
        0,
        "MGEF",
        SchemaNodeKind::Sequence {
            children: vec![
                node(
                    17,
                    "MGEF/0:Name",
                    SchemaNodeKind::Subrecord {
                        signature: SchemaSignature(*b"FULL"),
                        payload: Box::new(name),
                    },
                ),
                node(
                    1,
                    DATA,
                    SchemaNodeKind::Subrecord {
                        signature: SchemaSignature(*b"DATA"),
                        payload: Box::new(node(
                            2,
                            "MGEF/1:Data/payload",
                            SchemaNodeKind::Struct { fields },
                        )),
                    },
                ),
            ],
        },
    );
    let mut manifest = super::tests::test_manifest();
    manifest.callbacks_total = 1;
    manifest.callbacks_classified = 1;
    manifest.required_handlers = vec![HandlerRequirement {
        id: "edit.sync_record_counts".to_owned(),
        minimum_version: 1,
    }];
    let package = SchemaPackage::new_with_callbacks(
        manifest,
        vec![SchemaRecord {
            signature: SchemaSignature(*b"MGEF"),
            name: "Magic Effect".to_owned(),
            root,
        }],
        vec![CallbackBinding {
            path: "MGEF".to_owned(),
            callback_id: "def.after_set".to_owned(),
            callback_slot: None,
            implementation_fingerprint: "11".repeat(32),
            implementation: CallbackImplementation::BuiltIn {
                operation: BuiltInOperation {
                    id: "edit.sync_record_counts".to_owned(),
                    minimum_version: 1,
                    configuration: serde_json::json!({"counters":[{
                        "counter_nested":true,"counter_path":COUNTER,"counter_required":true,
                        "counter_signature":"DATA","mode":"subrecord_count",
                        "only_when_counter_exists":true,"only_when_missing":true,
                        "value_signature":"ESCE"
                    }]}),
                },
            },
        }],
    )?;
    SemanticContext::new(Arc::new(package), DecoderRegistry::builtin())
}

fn source(archetype: u32, taper: u32, counter: u16) -> Record {
    let mut data = vec![0; 152];
    data[20..22].copy_from_slice(&counter.to_le_bytes());
    data[56..60].copy_from_slice(&taper.to_le_bytes());
    data[64..68].copy_from_slice(&archetype.to_le_bytes());
    Record::from_writable(&WritableRecord {
        signature: Signature(*b"MGEF"),
        flags: RecordFlags::empty(),
        form_id: FormId(0x800),
        form_version: 44,
        subrecords: vec![
            WritableSubRecord {
                signature: Signature(*b"FULL"),
                data: b"Original\0".to_vec(),
            },
            WritableSubRecord {
                signature: Signature(*b"DATA"),
                data,
            },
        ],
    })
}

fn selected(context: &SemanticContext, source: &Record) -> crate::Result<String> {
    let fields = context.view(source, false)?.fields()?;
    let FieldValue::Struct(values) = &fields[1].value else {
        panic!("fixture DATA must decode as a structure");
    };
    Ok(values[2]
        .effective_path
        .clone()
        .expect("union has an effective path"))
}

/// Reads the archetype relative to the union, not the earlier taper field.
#[test]
fn mgef_forward_selector_uses_union_base() -> Result<(), Box<dyn std::error::Error>> {
    let context = fixture()?;
    assert!(selected(&context, &source(12, 0, 0))?.ends_with("/1:Light"));
    assert!(selected(&context, &source(0, 12, 0))?.ends_with("/0:Unused"));
    Ok(())
}

/// Edits FULL without ESCE while preserving an already-correct DATA payload.
#[test]
fn mgef_full_edit_skips_unchanged_counter() -> Result<(), Box<dyn std::error::Error>> {
    let context = fixture()?;
    for (archetype, taper) in [(12, 0), (0, 12)] {
        let original = source(archetype, taper, 0);
        let mut editor = context.edit(&original, false)?;
        editor.set(
            "MGEF/0:Name",
            0,
            &OwnedFieldValue::String("Changed".to_owned()),
        )?;
        let writable = editor.into_writable_record();
        assert_eq!(writable.subrecords[0].data, b"Changed\0");
        assert_eq!(
            writable.subrecords[1].data,
            original.subrecords()?[1].as_bytes()
        );
    }
    Ok(())
}

/// Repairs a nonzero counter through the union and preserves failed-edit rollback.
#[test]
fn mgef_full_edit_repairs_counter_and_rolls_back_invalid_text(
) -> Result<(), Box<dyn std::error::Error>> {
    let context = fixture()?;
    let original = source(12, 0, 3);
    let mut editor = context.edit(&original, false)?;
    assert!(editor
        .set("MGEF/0:Name", 0, &OwnedFieldValue::UInt(4))
        .is_err());
    assert_eq!(editor.record.subrecords[0].data, b"Original\0");
    assert_eq!(
        &editor.record.subrecords[1].data[20..22],
        &3_u16.to_le_bytes()
    );
    editor.set(
        "MGEF/0:Name",
        0,
        &OwnedFieldValue::String("Changed".to_owned()),
    )?;
    assert_eq!(
        &editor.record.subrecords[1].data[20..22],
        &0_u16.to_le_bytes()
    );
    assert!(selected(&context, &Record::from_writable(&editor.record))?.ends_with("/1:Light"));
    Ok(())
}

/// Selects the union using an edited later sibling rather than stale source bytes.
#[test]
fn mgef_forward_selector_uses_in_flight_archetype() -> Result<(), Box<dyn std::error::Error>> {
    let context = fixture()?;
    let original = source(0, 12, 0);
    let mut editor = context.edit(&original, false)?;
    let fields = editor.fields()?;
    let mut updated = crate::value::handler_to_owned_value(fields[1].value.clone(), DATA)?;
    let OwnedFieldValue::Struct(values) = &mut updated else {
        panic!("fixture DATA must decode as a structure");
    };
    values[10] = OwnedFieldValue::UInt(12);
    editor.set(DATA, 0, &updated)?;
    assert!(selected(&context, &Record::from_writable(&editor.record))?.ends_with("/1:Light"));
    Ok(())
}

/// Selects defaults from the updated archetype without reading stale source bytes.
#[test]
fn mgef_forward_selector_defaults_use_updated_sibling() -> Result<(), Box<dyn std::error::Error>> {
    let context = fixture()?;
    let original = source(0, 12, 0);
    let editor = context.edit(&original, false)?;
    let node = editor.find_node(ASSOC)?;
    let SchemaNodeKind::Union { selector, variants } = &node.kind else {
        panic!("fixture associated item must be a union");
    };
    let mut fields = editor.expression_field_values();
    fields.insert(ARCHETYPE.to_owned(), 12);
    let selected = editor.select_default_union_variant(
        node,
        selector,
        variants,
        &fields,
        &editor.record,
        Some(1),
    )?;
    assert!(selected.path.ends_with("/1:Light"));
    Ok(())
}

/// Rejects unresolved forward reads with their original reason and no data changes.
#[test]
fn mgef_forward_selector_fails_closed_without_matching_field(
) -> Result<(), Box<dyn std::error::Error>> {
    let context = fixture()?;
    let original = source(12, 0, 0);
    let editor = context.edit(&original, false)?;
    let node = editor.find_node(ASSOC)?;
    let SchemaNodeKind::Union { variants, .. } = &node.kind else {
        panic!("fixture associated item must be a union");
    };
    let fields = editor.expression_field_values();
    let selector = UnionSelector::Expression(Expression::ReadUnsigned {
        offset: 999,
        width: 4,
    });
    let error = editor
        .select_union_variant(
            node,
            &selector,
            variants,
            &OwnedFieldValue::UInt(0),
            super::UnionSelectionContext {
                field_values: &fields,
                source_record: &editor.record,
                source_subrecord_index: Some(1),
                value_scope: None,
            },
        )
        .expect_err("unmapped forward reads must fail");
    assert!(error
        .to_string()
        .contains("read range 999..1003 exceeds payload length 4"));
    assert_eq!(
        editor.record.subrecords[1].data,
        original.subrecords()?[1].as_bytes()
    );
    Ok(())
}

/// Rejects an oversized in-flight u64 selector instead of reusing its previous value.
#[test]
fn forward_selector_rejects_u64_overflow_transactionally() -> Result<(), Box<dyn std::error::Error>>
{
    let context = fixture_with_width(8)?;
    let original = source(0, 12, 0);
    let mut editor = context.edit(&original, false)?;
    let fields = editor.fields()?;
    let mut updated = crate::value::handler_to_owned_value(fields[1].value.clone(), DATA)?;
    let OwnedFieldValue::Struct(values) = &mut updated else {
        panic!("fixture DATA must decode as a structure");
    };
    values[10] = OwnedFieldValue::UInt(i64::MAX as u64);
    editor.set(DATA, 0, &updated)?;
    let valid_data = editor.record.subrecords[1].data.clone();
    assert_eq!(&valid_data[64..72], &(i64::MAX as u64).to_le_bytes());
    let OwnedFieldValue::Struct(values) = &mut updated else {
        panic!("fixture DATA must remain a structure");
    };
    values[10] = OwnedFieldValue::UInt(u64::MAX);
    let error = editor
        .set(DATA, 0, &updated)
        .expect_err("unrepresentable selector must fail");
    assert!(error
        .to_string()
        .contains("read range 56..64 exceeds payload length 4"));
    assert_eq!(editor.record.subrecords[1].data, valid_data);
    assert!(context
        .view(&Record::from_writable(&editor.record), false)?
        .fields()
        .is_ok());
    Ok(())
}
