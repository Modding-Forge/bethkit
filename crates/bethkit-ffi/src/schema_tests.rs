// SPDX-License-Identifier: Apache-2.0
//! Regression coverage for owned snapshots, exact addresses and semantic JSON APIs.

use std::borrow::Cow;
use std::ffi::{CStr, CString};
use std::sync::Arc;

use bethkit_core::{FormId, Record, RecordFlags, Signature, WritableRecord, WritableSubRecord};
use bethkit_schema::{
    ArrayCount, ByteOrder, ConflictPriority, Expression, IntegerType, PrimitiveType,
    SchemaManifest, SchemaNode, SchemaNodeId, SchemaNodeKind, SchemaPackage, SchemaRecord,
    SchemaSignature, UnionSelector,
};
use bethkit_semantic::{
    ByteSpan, DecoderRegistry, Field, FieldOrigin, FieldValue, SemanticContext,
};
use serde_json::{json, Value};

use super::snapshot::*;
use super::*;

type TestResult<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn string_projection_fixture(
    bulk_size: usize,
) -> TestResult<(BethkitSemanticContext, BethkitRecord)> {
    let text = |id, path: &str| {
        node(
            id,
            path,
            SchemaNodeKind::Primitive {
                primitive: PrimitiveType::String {
                    string: bethkit_schema::StringType {
                        encoding: "utf8".to_owned(),
                        localized: true,
                        zero_terminated: true,
                        fixed_length: Some(4),
                        length_prefix: None,
                        trailing_terminator: None,
                        allowed_values: Vec::new(),
                    },
                },
            },
        )
    };
    let schema = SchemaPackage::new(
        package()?.manifest().clone(),
        vec![SchemaRecord {
            signature: SchemaSignature(*b"TEST"),
            name: "Strings projection".to_owned(),
            root: node(
                1,
                "TEST",
                SchemaNodeKind::Sequence {
                    children: vec![
                        subrecord(
                            2,
                            "TEST/text",
                            b"TEXT",
                            node(
                                3,
                                "TEST/text/value",
                                SchemaNodeKind::Struct {
                                    fields: vec![
                                        integer(4, "TEST/text/value/count"),
                                        text(5, "TEST/text/value/text"),
                                    ],
                                },
                            ),
                        ),
                        subrecord(
                            6,
                            "TEST/items",
                            b"ARRY",
                            node(
                                7,
                                "TEST/items/value",
                                SchemaNodeKind::Array {
                                    count: ArrayCount::Remainder,
                                    element: Box::new(node(
                                        8,
                                        "TEST/items/value/item",
                                        SchemaNodeKind::Struct {
                                            fields: vec![
                                                integer(9, "TEST/items/value/item/count"),
                                                text(10, "TEST/items/value/item/text"),
                                            ],
                                        },
                                    )),
                                },
                            ),
                        ),
                        subrecord(
                            11,
                            "TEST/bulk",
                            b"BULK",
                            node(
                                12,
                                "TEST/bulk/value",
                                SchemaNodeKind::Primitive {
                                    primitive: PrimitiveType::Bytes { length: None },
                                },
                            ),
                        ),
                    ],
                },
            ),
        }],
    )?;
    let context = BethkitSemanticContext(SemanticContext::new(
        Arc::new(schema),
        DecoderRegistry::new(),
    )?);
    let mut first = 1u32.to_le_bytes().to_vec();
    first.extend_from_slice(b"a\0\0\0");
    let mut items = 2u32.to_le_bytes().to_vec();
    items.extend_from_slice(b"b\0\0\0");
    items.extend_from_slice(&3u32.to_le_bytes());
    items.extend_from_slice(b"c\0\0\0");
    let record = BethkitRecord(Record::from_writable(&WritableRecord {
        signature: Signature(*b"TEST"),
        flags: RecordFlags::empty(),
        form_id: FormId(0x800),
        form_version: 44,
        subrecords: vec![
            WritableSubRecord {
                signature: Signature(*b"TEXT"),
                data: first,
            },
            WritableSubRecord {
                signature: Signature(*b"ARRY"),
                data: items,
            },
            WritableSubRecord {
                signature: Signature(*b"BULK"),
                data: vec![0xAB; bulk_size],
            },
        ],
    }));
    Ok((context, record))
}

/// Keeps exact full-snapshot addresses while omitting unrelated bytes and struct members.
#[test]
fn strings_projection_preserves_addresses_and_shape_guard() -> TestResult {
    let (context, record) = string_projection_fixture(4)?;
    let complete = take_json(bethkit_semantic_snapshot_json(&context, &record, false))?;
    let strings = take_json(bethkit_semantic_strings_snapshot_json(
        &context, &record, false,
    ))?;
    assert_eq!(complete["projection"], "full");
    assert_eq!(strings["projection"], "strings");
    assert_eq!(
        complete["fields"][0]["value"]["schema_path"],
        "TEST/text/value"
    );
    assert_eq!(
        strings["fields"][1]["value"]["items"][1]["fields"][0]["value"]["schema_path"],
        "TEST/items/value/item/text"
    );
    assert_eq!(strings["structure_hash"], complete["structure_hash"]);
    assert_eq!(strings["fields"].as_array().expect("fields array").len(), 2);
    assert_eq!(
        strings["fields"][0]["value"]["fields"]
            .as_array()
            .expect("members array")
            .len(),
        1
    );
    assert_eq!(
        strings["fields"][0]["value"]["fields"][0]["value"],
        complete["fields"][0]["value"]["fields"][1]["value"]
    );
    for index in 0..2 {
        assert_eq!(
            strings["fields"][1]["value"]["items"][index]["fields"][0]["value"],
            complete["fields"][1]["value"]["items"][index]["fields"][1]["value"]
        );
    }
    let editor = bethkit_record_editor_new(&context, &record, false);
    assert!(!editor.is_null());
    let edited = take_json(bethkit_record_editor_strings_snapshot_json(editor))?;
    assert_eq!(edited, strings);
    bethkit_record_editor_free(editor);
    Ok(())
}

/// Bounds string-snapshot JSON size even when a record contains megabytes of raw payload.
#[test]
fn strings_projection_omits_large_payload_before_serialization() -> TestResult {
    let (context, record) = string_projection_fixture(16 * 1024 * 1024)?;
    let pointer = bethkit_semantic_strings_snapshot_json(&context, &record, false);
    assert!(!pointer.is_null());
    // SAFETY: the snapshot API returned an owned NUL-terminated JSON allocation.
    let length = unsafe { CStr::from_ptr(pointer) }.to_bytes().len();
    assert!(
        length < 12_000,
        "strings-only snapshot unexpectedly contains bulk data: {length}"
    );
    let strings = take_json(pointer)?;
    assert_eq!(strings["fields"].as_array().expect("fields array").len(), 2);
    let external = take_json(bethkit_semantic_strings_snapshot_json(
        &context, &record, true,
    ))?;
    let value = &external["fields"][0]["value"]["fields"][0]["value"];
    assert_eq!(value["kind"], "uint");
    assert_eq!(value["value"], 97);
    assert_eq!(value["string_table"], "strings");
    Ok(())
}

fn union_array_fixture() -> TestResult<(BethkitSemanticContext, BethkitRecord)> {
    let inner = node(
        10,
        "TEST/data/groups/group/items/item/selected",
        SchemaNodeKind::Union {
            selector: UnionSelector::Expression(Expression::ReadField {
                path: "TEST/data/groups/group/tag".to_owned(),
            }),
            variants: vec![
                integer(11, "TEST/data/groups/group/items/item/selected/number"),
                node(
                    12,
                    "TEST/data/groups/group/items/item/selected/text",
                    SchemaNodeKind::Primitive {
                        primitive: PrimitiveType::String {
                            string: bethkit_schema::StringType {
                                encoding: "utf8".to_owned(),
                                localized: true,
                                zero_terminated: true,
                                fixed_length: Some(4),
                                length_prefix: None,
                                trailing_terminator: None,
                                allowed_values: Vec::new(),
                            },
                        },
                    },
                ),
            ],
        },
    );
    let item = node(
        8,
        "TEST/data/groups/group/items/item",
        SchemaNodeKind::Union {
            selector: UnionSelector::Expression(Expression::Int { value: 1 }),
            variants: vec![
                integer(9, "TEST/data/groups/group/items/item/unused"),
                inner,
            ],
        },
    );
    let group = node(
        4,
        "TEST/data/groups/group",
        SchemaNodeKind::Struct {
            fields: vec![
                integer(5, "TEST/data/groups/group/tag"),
                node(
                    6,
                    "TEST/data/groups/group/items",
                    SchemaNodeKind::Array {
                        count: ArrayCount::Fixed { count: 2 },
                        element: Box::new(item),
                    },
                ),
            ],
        },
    );
    let schema = SchemaPackage::new(
        package()?.manifest().clone(),
        vec![SchemaRecord {
            signature: SchemaSignature(*b"TEST"),
            name: "Array unions".to_owned(),
            root: node(
                1,
                "TEST",
                SchemaNodeKind::Sequence {
                    children: vec![subrecord(
                        2,
                        "TEST/data",
                        b"DATA",
                        node(
                            3,
                            "TEST/data/groups",
                            SchemaNodeKind::Array {
                                count: ArrayCount::Remainder,
                                element: Box::new(group),
                            },
                        ),
                    )],
                },
            ),
        }],
    )?;
    let context = BethkitSemanticContext(SemanticContext::new(
        Arc::new(schema),
        DecoderRegistry::new(),
    )?);
    let record = BethkitRecord(Record::from_writable(&WritableRecord {
        signature: Signature(*b"TEST"),
        flags: RecordFlags::empty(),
        form_id: FormId(0x801),
        form_version: 44,
        subrecords: vec![WritableSubRecord {
            signature: Signature(*b"DATA"),
            data: [0u32, 97, 98, 1, 99, 100]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect(),
        }],
    }));
    Ok((context, record))
}

/// Resolves nested array union arms exactly even when all localized values are integers.
#[test]
fn json_snapshot_retains_each_nested_array_union_selection() -> TestResult {
    // given
    let (context, record) = union_array_fixture()?;
    for localized in [false, true] {
        // when
        let full = take_json(bethkit_semantic_snapshot_json(&context, &record, localized))?;
        let strings = take_json(bethkit_semantic_strings_snapshot_json(
            &context, &record, localized,
        ))?;

        // then
        let groups = full["fields"][0]["value"]["items"]
            .as_array()
            .expect("two decoded groups");
        assert_eq!(groups.len(), 2);
        for (group_index, group) in groups.iter().enumerate() {
            assert_eq!(group["schema_path"], "TEST/data/groups/group");
            assert_eq!(
                group["fields"][0]["value"]["schema_path"],
                "TEST/data/groups/group/tag"
            );
            let items = group["fields"][1]["value"]["items"]
                .as_array()
                .expect("two inner elements");
            for (item_index, item) in items.iter().enumerate() {
                let suffix = if group_index == 0 { "number" } else { "text" };
                assert_eq!(
                    item["schema_path"],
                    format!("TEST/data/groups/group/items/item/selected/{suffix}")
                );
                assert_eq!(item["translatable"], group_index == 1);
                assert_eq!(
                    item["address"]["value_steps"],
                    json!([
                        {"kind":"index","index":group_index},
                        {"kind":"field","index":1,"path":"TEST/data/groups/group/items"},
                        {"kind":"index","index":item_index}
                    ])
                );
                if localized {
                    assert_eq!(item["kind"], "uint");
                }
            }
        }
        let filtered = &strings["fields"][0]["value"]["items"];
        assert_eq!(filtered.as_array().expect("string-bearing group").len(), 1);
        assert_eq!(
            filtered[0]["fields"][0]["value"],
            groups[1]["fields"][1]["value"]
        );
        assert_eq!(strings["structure_hash"], full["structure_hash"]);
        let editor = bethkit_record_editor_new(&context, &record, localized);
        assert!(!editor.is_null());
        let edited = take_json(bethkit_record_editor_snapshot_json(editor))?;
        assert_eq!(edited, full);
        bethkit_record_editor_free(editor);
    }
    Ok(())
}

/// Rejects old addresses when a selector switches equally shaped scalar array variants.
#[test]
fn array_union_variant_changes_invalidate_addresses() -> TestResult {
    // given
    let (context, record) = union_array_fixture()?;
    let editor = bethkit_record_editor_new(&context, &record, true);
    assert!(!editor.is_null());
    let before = take_json(bethkit_record_editor_snapshot_json(editor))?;
    let original_group = &before["fields"][0]["value"]["items"][0];
    let tag_address = CString::new(serde_json::to_string(
        &original_group["fields"][0]["address"],
    )?)?;
    let old_item = CString::new(serde_json::to_string(
        &original_group["fields"][1]["value"]["items"][0]["address"],
    )?)?;
    let selected = CString::new(r#"{"kind":"uint","value":1}"#)?;

    // when
    assert_eq!(
        bethkit_record_editor_set_at_json(editor, tag_address.as_ptr(), selected.as_ptr()),
        0
    );
    let after = take_json(bethkit_record_editor_snapshot_json(editor))?;

    // then
    let new_item = &after["fields"][0]["value"]["items"][0]["fields"][1]["value"]["items"][0];
    assert_eq!(new_item["kind"], "uint");
    assert_eq!(
        new_item["schema_path"],
        "TEST/data/groups/group/items/item/selected/text"
    );
    assert_ne!(before["structure_hash"], after["structure_hash"]);
    assert_eq!(
        bethkit_record_editor_set_at_json(editor, old_item.as_ptr(), selected.as_ptr()),
        -1
    );
    assert_eq!(
        take_json(bethkit_record_editor_snapshot_json(editor))?,
        after
    );
    let new_address = CString::new(serde_json::to_string(&new_item["address"])?)?;
    assert_eq!(
        bethkit_record_editor_set_at_json(editor, new_address.as_ptr(), selected.as_ptr()),
        0
    );
    let edited = take_json(bethkit_record_editor_snapshot_json(editor))?;
    assert_eq!(edited["structure_hash"], after["structure_hash"]);
    bethkit_record_editor_free(editor);
    Ok(())
}

fn node(id: u32, path: &str, kind: SchemaNodeKind) -> SchemaNode {
    SchemaNode {
        id: SchemaNodeId(id),
        path: path.to_owned(),
        name: path.to_owned(),
        required: true,
        conflict_priority: ConflictPriority::Normal,
        condition: None,
        kind,
    }
}

fn integer(id: u32, path: &str) -> SchemaNode {
    node(
        id,
        path,
        SchemaNodeKind::Primitive {
            primitive: PrimitiveType::Integer {
                integer: IntegerType {
                    width: 4,
                    signed: false,
                    byte_order: ByteOrder::LittleEndian,
                },
            },
        },
    )
}

fn subrecord(id: u32, path: &str, signature: &[u8; 4], payload: SchemaNode) -> SchemaNode {
    node(
        id,
        path,
        SchemaNodeKind::Subrecord {
            signature: SchemaSignature(*signature),
            payload: Box::new(payload),
        },
    )
}

fn package() -> TestResult<SchemaPackage> {
    let manifest: SchemaManifest = serde_json::from_value(json!({
        "format_version":bethkit_schema::PACKAGE_FORMAT_VERSION,"game":"skyrim_se",
        "package_version":"1.0.0","source_repository":"TES5Edit/TES5Edit",
        "source_tag":"test","source_commit":"11".repeat(20),
        "source_archive_sha256":"11".repeat(32),"exporter_version":"test",
        "exporter_binary_sha256":"11".repeat(32),"exporter_map_sha256":"11".repeat(32),
        "exporter_patch_sha256":"11".repeat(32),"exporter_build_sha256":"11".repeat(32),
        "conversion_rules_sha256":"11".repeat(32),"minimum_bethkit_version":"0.4.0",
        "minimum_abi_version":2,"validation_status":"candidate","corpus_sha256":"11".repeat(32),
        "validated_records":0,"byte_coverage":0.0,"callbacks_total":0,"callbacks_classified":0,
        "required_decoders":[],"required_handlers":[]
    }))?;
    let stages = node(
        2,
        "TEST/stages",
        SchemaNodeKind::Repeat {
            minimum: 0,
            maximum: None,
            child: Box::new(node(
                3,
                "TEST/stages/stage",
                SchemaNodeKind::Sequence {
                    children: vec![
                        subrecord(
                            4,
                            "TEST/stages/stage/index",
                            b"STAG",
                            integer(5, "TEST/stages/stage/index/value"),
                        ),
                        node(
                            6,
                            "TEST/stages/stage/entries",
                            SchemaNodeKind::Repeat {
                                minimum: 0,
                                maximum: None,
                                child: Box::new(subrecord(
                                    7,
                                    "TEST/stages/stage/entries/entry",
                                    b"ENTR",
                                    integer(8, "TEST/stages/stage/entries/entry/value"),
                                )),
                            },
                        ),
                    ],
                },
            )),
        },
    );
    let array = subrecord(
        9,
        "TEST/items",
        b"ARRY",
        node(
            10,
            "TEST/items/array",
            SchemaNodeKind::Array {
                count: ArrayCount::Remainder,
                element: Box::new(node(
                    11,
                    "TEST/items/array/item",
                    SchemaNodeKind::Struct {
                        fields: vec![
                            integer(12, "TEST/items/array/item/left"),
                            integer(13, "TEST/items/array/item/right"),
                        ],
                    },
                )),
            },
        ),
    );
    let link = subrecord(
        14,
        "TEST/link",
        b"LINK",
        node(
            15,
            "TEST/link/value",
            SchemaNodeKind::Primitive {
                primitive: PrimitiveType::FormId {
                    targets: vec![SchemaSignature(*b"WEAP"), SchemaSignature(*b"ARMO")],
                },
            },
        ),
    );
    Ok(SchemaPackage::new(
        manifest,
        vec![SchemaRecord {
            signature: SchemaSignature(*b"TEST"),
            name: "Test".to_owned(),
            root: node(
                1,
                "TEST",
                SchemaNodeKind::Sequence {
                    children: vec![stages, array, link],
                },
            ),
        }],
    )?)
}

fn record() -> BethkitRecord {
    let entries = [
        (b"STAG", 10u32),
        (b"ENTR", 101),
        (b"ENTR", 102),
        (b"STAG", 20),
        (b"ENTR", 201),
    ];
    let mut subrecords: Vec<WritableSubRecord> = entries
        .into_iter()
        .map(|(signature, value)| WritableSubRecord {
            signature: Signature(*signature),
            data: value.to_le_bytes().to_vec(),
        })
        .collect();
    subrecords.push(WritableSubRecord {
        signature: Signature(*b"ARRY"),
        data: [1u32.to_le_bytes(), 2u32.to_le_bytes()].concat(),
    });
    subrecords.push(WritableSubRecord {
        signature: Signature(*b"LINK"),
        data: 0x1234u32.to_le_bytes().to_vec(),
    });
    subrecords.push(WritableSubRecord {
        signature: Signature(*b"ZZZZ"),
        data: vec![4, 5, 6],
    });
    BethkitRecord(Record::from_writable(&WritableRecord {
        signature: Signature(*b"TEST"),
        flags: RecordFlags::empty(),
        form_id: FormId(0x800),
        form_version: 44,
        subrecords,
    }))
}

fn fixture() -> TestResult<(BethkitSemanticContext, BethkitRecord)> {
    Ok((
        BethkitSemanticContext(SemanticContext::new(
            Arc::new(package()?),
            DecoderRegistry::builtin(),
        )?),
        record(),
    ))
}

fn take_json(ptr: *mut std::ffi::c_char) -> TestResult<Value> {
    assert!(!ptr.is_null(), "JSON API must succeed");
    // SAFETY: a non-null owned JSON result is a NUL-terminated CString allocation.
    let bytes = unsafe { CStr::from_ptr(ptr) }.to_bytes().to_vec();
    // SAFETY: ptr is the owned JSON allocation, released exactly once after copying.
    unsafe { bethkit_string_free(ptr) };
    Ok(serde_json::from_slice(&bytes)?)
}

/// Owns typed signatures and raw bytes independently of both source handles.
#[test]
fn snapshot_survives_source_release() -> TestResult {
    let (context, record) = fixture()?;
    let view = bethkit_record_view_new(&context, &record, false);
    assert!(!view.is_null());
    drop(record);
    drop(context);
    // SAFETY: the snapshot remains owned and alive until its matching free below.
    let view_ref = unsafe { &*view };
    // SAFETY: fixture field 6 is a typed FormID and field 7 is raw bytes.
    let link = unsafe { view_ref.fields[6].value.payload.form_id_typed };
    assert_eq!(link.allowed_count, 2);
    // SAFETY: signatures are copied into the live snapshot's arena.
    assert_eq!(
        unsafe { std::slice::from_raw_parts(link.allowed_sigs, link.allowed_count) },
        &[*b"WEAP", *b"ARMO"]
    );
    // SAFETY: fixture field 7 has the Bytes discriminant and its arena is alive.
    let bytes = unsafe { &view_ref.fields[7].value.payload.bytes };
    // SAFETY: this slice is owned by the live snapshot.
    assert_eq!(
        unsafe { std::slice::from_raw_parts(bytes.ptr, bytes.len) },
        &[4, 5, 6]
    );
    bethkit_record_view_free(view);
    Ok(())
}

/// Copies handler-owned bytes recursively before temporary decoded values disappear.
#[test]
fn snapshot_copies_owned_bytes_in_nested_arrays() -> TestResult {
    let fields = vec![Field {
        subrecord_index: 0,
        repeat_scopes: vec![],
        node_id: SchemaNodeId(1),
        path: "TEST/data".to_owned(),
        effective_path: None,
        name: "Data".to_owned(),
        subrecord_signature: Signature(*b"DATA"),
        occurrence: 0,
        span: ByteSpan { start: 0, end: 3 },
        origin: FieldOrigin::CustomDecoder,
        value: FieldValue::Array(vec![FieldValue::Bytes(Cow::Owned(vec![8, 9, 10]))]),
        value_selections: Vec::new(),
    }];
    let snapshot = snapshot_fields(&fields);
    assert_eq!(snapshot._storage.bytes.len(), 1);
    drop(fields);
    // SAFETY: the root value was constructed as Array and owns this list.
    let array = unsafe { &*snapshot.fields[0].value.payload.array_values };
    // SAFETY: the sole array item was constructed as Bytes.
    let bytes = unsafe { &array.values[0].payload.bytes };
    // SAFETY: the snapshot retains the independent byte allocation.
    assert_eq!(
        unsafe { std::slice::from_raw_parts(bytes.ptr, bytes.len) },
        &[8, 9, 10]
    );
    bethkit_record_view_free(Box::into_raw(Box::new(snapshot)));
    Ok(())
}

/// Preserves actual nested repeat scopes, including entry indices restarting per stage.
#[test]
fn json_snapshot_exposes_local_repeat_scopes() -> TestResult {
    let (context, record) = fixture()?;
    let snapshot = take_json(bethkit_semantic_snapshot_json(&context, &record, false))?;
    assert_eq!(
        snapshot["fields"][1]["address"]["repeat_scopes"],
        json!([
            {"path":"TEST/stages/stage","occurrence":0},
            {"path":"TEST/stages/stage/entries/entry","occurrence":0}
        ])
    );
    assert_eq!(
        snapshot["fields"][4]["address"]["repeat_scopes"],
        json!([
            {"path":"TEST/stages/stage","occurrence":1},
            {"path":"TEST/stages/stage/entries/entry","occurrence":0}
        ])
    );
    assert_eq!(
        snapshot["fields"][5]["value"]["items"][0]["fields"][1]["address"]["value_steps"],
        json!([
            {"kind":"index","index":0},
            {"kind":"field","index":1,"path":"TEST/items/array/item/right"}
        ])
    );
    Ok(())
}

/// Edits a nested packed value, rejects forged scopes, and detects stale array addresses.
#[test]
fn addressed_editor_edits_exactly_and_rejects_stale_addresses() -> TestResult {
    let (context, record) = fixture()?;
    let editor = bethkit_record_editor_new(&context, &record, false);
    assert!(!editor.is_null());
    let before = take_json(bethkit_record_editor_snapshot_json(editor))?;
    let selected = &before["fields"][5]["value"]["items"][0]["fields"][1]["address"];
    let address = CString::new(serde_json::to_string(selected)?)?;
    let value = CString::new(r#"{"kind":"uint","value":99}"#)?;
    assert_eq!(
        bethkit_record_editor_set_at_json(editor, address.as_ptr(), value.as_ptr()),
        0
    );
    let after = take_json(bethkit_record_editor_snapshot_json(editor))?;
    assert_eq!(
        after["fields"][5]["value"]["items"][0]["fields"][0]["value"]["value"],
        1
    );
    assert_eq!(
        after["fields"][5]["value"]["items"][0]["fields"][1]["value"]["value"],
        99
    );
    assert_eq!(before["structure_hash"], after["structure_hash"]);
    let array_address = CString::new(serde_json::to_string(&after["fields"][5]["address"])?)?;
    let item = CString::new(serde_json::to_string(
        &after["fields"][5]["value"]["items"][0],
    )?)?;
    assert_eq!(
        bethkit_record_editor_insert_at_json(editor, array_address.as_ptr(), item.as_ptr()),
        0
    );
    assert_eq!(
        bethkit_record_editor_set_at_json(editor, address.as_ptr(), value.as_ptr()),
        -1
    );
    let mut forged = after["fields"][4]["address"].clone();
    forged["repeat_scopes"][0]["occurrence"] = json!(0);
    let forged = CString::new(serde_json::to_string(&forged)?)?;
    assert_eq!(
        bethkit_record_editor_remove_at_json(editor, forged.as_ptr()),
        -1
    );
    let final_state = take_json(bethkit_record_editor_snapshot_json(editor))?;
    assert_eq!(
        final_state["fields"][5]["value"]["items"]
            .as_array()
            .expect("items array")
            .len(),
        2
    );
    bethkit_record_editor_free(editor);
    Ok(())
}

/// Keeps editor allocation independently freeable after transferring its finished contents.
#[test]
fn editor_finish_leaves_outer_handle_for_explicit_free() -> TestResult {
    let (context, record) = fixture()?;
    let editor = bethkit_record_editor_new(&context, &record, false);
    let writable = bethkit_record_editor_finish(editor);
    assert!(!writable.is_null());
    assert!(bethkit_record_editor_finish(editor).is_null());
    bethkit_record_editor_free(editor);
    crate::writer::bethkit_writable_record_free(writable);
    Ok(())
}

/// Keeps original field addresses valid across independent string-length changes.
#[test]
fn scalar_text_length_changes_preserve_structural_addresses() -> TestResult {
    // given
    let original = package()?;
    let text = |id, path: &str, signature| -> TestResult<SchemaNode> {
        Ok(subrecord(
            id,
            path,
            signature,
            node(
                id + 1,
                &format!("{path}/value"),
                SchemaNodeKind::Primitive {
                    primitive: PrimitiveType::String {
                        string: serde_json::from_value(json!({
                            "encoding":"utf8", "zero_terminated":true,
                            "fixed_length":null
                        }))?,
                    },
                },
            ),
        ))
    };
    let package = SchemaPackage::new(
        original.manifest().clone(),
        vec![SchemaRecord {
            signature: SchemaSignature(*b"TEST"),
            name: "Text".to_owned(),
            root: node(
                1,
                "TEST",
                SchemaNodeKind::Sequence {
                    children: vec![
                        text(2, "TEST/first", b"ONE_")?,
                        text(4, "TEST/second", b"TWO_")?,
                    ],
                },
            ),
        }],
    )?;
    let context = BethkitSemanticContext(SemanticContext::new(
        Arc::new(package),
        DecoderRegistry::builtin(),
    )?);
    let record = BethkitRecord(Record::from_writable(&WritableRecord {
        signature: Signature(*b"TEST"),
        flags: RecordFlags::empty(),
        form_id: FormId(0x800),
        form_version: 44,
        subrecords: vec![
            WritableSubRecord {
                signature: Signature(*b"ONE_"),
                data: b"old\0".to_vec(),
            },
            WritableSubRecord {
                signature: Signature(*b"TWO_"),
                data: b"also old\0".to_vec(),
            },
        ],
    }));
    let editor = bethkit_record_editor_new(&context, &record, false);
    let before = take_json(bethkit_record_editor_snapshot_json(editor))?;
    // when
    for (index, replacement) in ["a substantially longer first text", "new second text"]
        .into_iter()
        .enumerate()
    {
        let address = CString::new(serde_json::to_string(&before["fields"][index]["address"])?)?;
        let value = CString::new(serde_json::to_string(
            &json!({"kind":"string", "value":replacement}),
        )?)?;
        assert_eq!(
            bethkit_record_editor_set_at_json(editor, address.as_ptr(), value.as_ptr(),),
            0
        );
    }
    // then
    let after = take_json(bethkit_record_editor_snapshot_json(editor))?;
    assert_eq!(before["structure_hash"], after["structure_hash"]);
    assert_ne!(before["fields"][0]["span"], after["fields"][0]["span"]);
    assert_eq!(
        after["fields"][0]["value"]["value"],
        "a substantially longer first text"
    );
    assert_eq!(after["fields"][1]["value"]["value"], "new second text");
    bethkit_record_editor_free(editor);
    Ok(())
}

/// Rejects swapped same-type packed fields before an edit can change record bytes.
#[test]
fn structured_edits_validate_field_identity_and_removal() -> TestResult {
    let (context, record) = fixture()?;
    let editor = bethkit_record_editor_new(&context, &record, false);
    let before = take_json(bethkit_record_editor_snapshot_json(editor))?;
    let selected = &before["fields"][5]["value"]["items"][0];
    let address = CString::new(serde_json::to_string(&selected["address"])?)?;
    let mut swapped = selected.clone();
    swapped["fields"]
        .as_array_mut()
        .expect("struct fields")
        .swap(0, 1);
    let replacement = CString::new(serde_json::to_string(&swapped)?)?;
    assert_eq!(
        bethkit_record_editor_set_at_json(editor, address.as_ptr(), replacement.as_ptr()),
        -1
    );
    assert_eq!(
        take_json(bethkit_record_editor_snapshot_json(editor))?,
        before
    );
    assert_eq!(
        bethkit_record_editor_remove_at_json(editor, address.as_ptr()),
        0
    );
    let after = take_json(bethkit_record_editor_snapshot_json(editor))?;
    assert_eq!(after["fields"][5]["value"]["items"], json!([]));
    assert_eq!(after["fields"][7]["value"]["value"], json!([4, 5, 6]));
    bethkit_record_editor_free(editor);
    Ok(())
}

/// Uses the complete repeat stack to reject a different stage with the same local entry index.
#[test]
fn repeat_addresses_edit_only_the_selected_stage() -> TestResult {
    let (context, record) = fixture()?;
    let editor = bethkit_record_editor_new(&context, &record, false);
    let before = take_json(bethkit_record_editor_snapshot_json(editor))?;
    let mut forged = before["fields"][4]["address"].clone();
    forged["repeat_scopes"][0]["occurrence"] = json!(0);
    let forged = CString::new(serde_json::to_string(&forged)?)?;
    let replacement = CString::new(r#"{"kind":"uint","value":202}"#)?;
    assert_eq!(
        bethkit_record_editor_set_at_json(editor, forged.as_ptr(), replacement.as_ptr()),
        -1
    );
    let address = CString::new(serde_json::to_string(&before["fields"][4]["address"])?)?;
    assert_eq!(
        bethkit_record_editor_set_at_json(editor, address.as_ptr(), replacement.as_ptr()),
        0
    );
    let after = take_json(bethkit_record_editor_snapshot_json(editor))?;
    assert_eq!(after["fields"][1]["value"]["value"], 101);
    assert_eq!(after["fields"][2]["value"]["value"], 102);
    assert_eq!(after["fields"][4]["value"]["value"], 202);
    bethkit_record_editor_free(editor);
    Ok(())
}

/// Preserves existing grammar assignments when inserting a repeated subrecord in one stage.
#[test]
fn scoped_subrecord_insertion_preserves_other_scopes() -> TestResult {
    let (context, record) = fixture()?;
    let editor = bethkit_record_editor_new(&context, &record, false);
    let before = take_json(bethkit_record_editor_snapshot_json(editor))?;
    let address = CString::new(serde_json::to_string(&before["fields"][4]["address"])?)?;
    let replacement = CString::new(r#"{"kind":"uint","value":200}"#)?;
    assert_eq!(
        bethkit_record_editor_insert_at_json(editor, address.as_ptr(), replacement.as_ptr()),
        0
    );
    let after = take_json(bethkit_record_editor_snapshot_json(editor))?;
    assert_eq!(after["fields"][4]["value"]["value"], 200);
    assert_eq!(after["fields"][5]["value"]["value"], 201);
    assert_eq!(
        after["fields"][5]["address"]["repeat_scopes"][0]["occurrence"],
        1
    );
    assert_eq!(
        after["fields"][5]["address"]["repeat_scopes"][1]["occurrence"],
        1
    );
    bethkit_record_editor_free(editor);
    Ok(())
}

/// Supplies field metadata without changing the pre-existing named-field layout.
#[test]
fn metadata_getters_keep_nested_identity_and_bounds() -> TestResult {
    let (context, record) = fixture()?;
    let view = bethkit_record_view_new(&context, &record, false);
    let mut metadata = std::mem::MaybeUninit::<BethkitFieldMetadata>::uninit();
    assert_eq!(
        bethkit_record_view_field_metadata(view, 5, metadata.as_mut_ptr()),
        0
    );
    // SAFETY: a successful metadata call initialized the entire out value.
    let metadata = unsafe { metadata.assume_init() };
    // SAFETY: metadata strings are borrowed from the live view.
    assert_eq!(
        unsafe { CStr::from_ptr(metadata.path) }.to_str()?,
        "TEST/items"
    );
    assert_eq!(metadata.subrecord_signature, *b"ARRY");
    // SAFETY: the fixture's field 5 is a live array containing one struct.
    let array = unsafe {
        &*(*bethkit_record_view_field_get(view, 5))
            .value
            .payload
            .array_values
    };
    // SAFETY: the sole array value is a struct owned by view.
    let entries = unsafe { array.values[0].payload.struct_entries };
    let mut nested = std::mem::MaybeUninit::<BethkitFieldMetadata>::uninit();
    assert_eq!(
        bethkit_field_entries_metadata(entries, 1, nested.as_mut_ptr()),
        0
    );
    // SAFETY: the successful nested metadata call initialized the out value.
    let nested = unsafe { nested.assume_init() };
    // SAFETY: nested.path is borrowed from the live view.
    assert_eq!(
        unsafe { CStr::from_ptr(nested.path) }.to_str()?,
        "TEST/items/array/item/right"
    );
    assert_eq!((nested.span_start, nested.span_end), (4, 8));
    assert_eq!(
        bethkit_record_view_field_metadata(view, usize::MAX, std::ptr::null_mut()),
        -1
    );
    bethkit_record_view_free(view);
    Ok(())
}

/// Serializes deterministic schema introspection and structured validation modes.
#[test]
fn schema_json_and_validation_modes_are_structured() -> TestResult {
    let package = BethkitSchemaPackage(Arc::new(package()?));
    let first = take_json(bethkit_schema_package_graph_json(&package))?;
    let second = take_json(bethkit_schema_package_graph_json(&package))?;
    assert_eq!(first, second);
    assert_eq!(first["records"][0]["signature"], json!([84, 69, 83, 84]));
    assert_eq!(
        take_json(bethkit_schema_package_manifest_json(&package))?["package_version"],
        "1.0.0"
    );
    let context = BethkitSemanticContext(SemanticContext::new(
        package.0.clone(),
        DecoderRegistry::builtin(),
    )?);
    let empty = BethkitRecord(Record::from_writable(&WritableRecord {
        signature: Signature(*b"TEST"),
        flags: RecordFlags::empty(),
        form_id: FormId(0x800),
        form_version: 44,
        subrecords: vec![],
    }));
    let strict = take_json(bethkit_semantic_validate_json(&context, &empty, false, 0))?;
    let compatible = take_json(bethkit_semantic_validate_json(&context, &empty, false, 1))?;
    assert_eq!(strict["has_errors"], true);
    assert_eq!(compatible["has_errors"], false);
    assert!(compatible["diagnostics"]
        .as_array()
        .expect("diagnostics array")
        .iter()
        .any(|item| item["code"] == "missing_required" && item["severity"] == "warning"));
    assert!(bethkit_semantic_validate_json(&context, &empty, false, 42).is_null());
    Ok(())
}

/// Inserts an absent optional field in grammar order and rolls back invalid requests.
#[test]
fn json_insert_adds_optional_full_and_preserves_failed_edits() -> TestResult {
    // given
    let original = package()?;
    let mut records = original.records().to_vec();
    let SchemaNodeKind::Sequence { children } = &mut records[0].root.kind else {
        panic!("fixture root must be a sequence");
    };
    let mut name = subrecord(
        16,
        "TEST/name",
        b"FULL",
        node(
            17,
            "TEST/name/value",
            SchemaNodeKind::Primitive {
                primitive: PrimitiveType::String {
                    string: serde_json::from_value(json!({
                        "encoding":"utf8", "zero_terminated":true, "fixed_length":null
                    }))?,
                },
            },
        ),
    );
    name.required = false;
    children.insert(1, name);
    let package = SchemaPackage::new(original.manifest().clone(), records)?;
    let context = BethkitSemanticContext(SemanticContext::new(
        Arc::new(package),
        DecoderRegistry::builtin(),
    )?);
    let source = record();
    let editor = bethkit_record_editor_new(&context, &source, false);
    let path = CString::new("TEST/name")?;
    let value = CString::new(r#"{"kind":"string","value":"Inserted name"}"#)?;
    // when
    assert_eq!(
        bethkit_record_editor_insert_json(editor, path.as_ptr(), value.as_ptr()),
        0
    );
    let inserted = take_json(bethkit_record_editor_snapshot_json(editor))?;
    // then
    assert_eq!(inserted["fields"][5]["path"], "TEST/name");
    assert_eq!(inserted["fields"][5]["value"]["value"], "Inserted name");
    assert_eq!(inserted["fields"][6]["path"], "TEST/items");
    assert_eq!(inserted["fields"][8]["value"]["value"], json!([4, 5, 6]));
    for (invalid_path, invalid_value) in [
        ("TEST/missing", r#"{"kind":"string","value":"Ignored"}"#),
        ("TEST/name", "malformed JSON"),
        ("TEST/name", r#"{"kind":"uint","value":42}"#),
    ] {
        let invalid_path = CString::new(invalid_path)?;
        let invalid_value = CString::new(invalid_value)?;
        assert_eq!(
            bethkit_record_editor_insert_json(
                editor,
                invalid_path.as_ptr(),
                invalid_value.as_ptr(),
            ),
            -1
        );
        assert_eq!(
            take_json(bethkit_record_editor_snapshot_json(editor))?,
            inserted
        );
    }
    assert!(source.0.get(Signature(*b"FULL"))?.is_none());
    assert_eq!(
        bethkit_record_editor_insert_json(editor, std::ptr::null(), value.as_ptr()),
        -1
    );
    bethkit_record_editor_free(editor);
    Ok(())
}
