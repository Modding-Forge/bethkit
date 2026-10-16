// SPDX-License-Identifier: Apache-2.0
//!
//! Narrow, lossless readers for Skyrim payload representations retained by existing mods.

use bethkit_core::Signature;
use bethkit_schema::{
    ArrayCount, ConflictPriority, Expression, PrimitiveType, SchemaNode, SchemaNodeKind,
};

/// Recognizes xEdit's flag-derived upper bound for an INFO fragment tail.
///
/// # Arguments
///
/// * `node` - Array definition being decoded.
///
/// # Returns
///
/// Whether complete fragments may end at the payload boundary before the flag limit.
pub(crate) fn info_fragment_limit(node: &SchemaNode) -> bool {
    if !node.path.starts_with("INFO/") || !node.path.ends_with("/3:Script Fragments/3:Fragments") {
        return false;
    }
    let SchemaNodeKind::Array {
        count: ArrayCount::Expression { expression },
        element,
    } = &node.kind
    else {
        return false;
    };
    let Expression::BitCount { value } = expression else {
        return false;
    };
    let Expression::ReadField { path } = value.as_ref() else {
        return false;
    };
    let Some(parent) = node.path.strip_suffix("/3:Fragments") else {
        return false;
    };
    let SchemaNodeKind::Struct { fields } = &element.kind else {
        return false;
    };
    path.strip_suffix("/1:Flags") == Some(parent)
        && fields.len() == 3
        && matches!(&fields[0].kind, SchemaNodeKind::Primitive {
            primitive: PrimitiveType::Integer { integer },
        } if integer.width == 1)
        && fields[1..].iter().all(|field| {
            matches!(&field.kind, SchemaNodeKind::Primitive {
            primitive: PrimitiveType::String { string },
        } if string.length_prefix.is_some_and(|prefix| prefix.width == 2 && prefix.offset == 2))
        })
}

/// Recognizes an explicitly ignored legacy icon slot when its payload is empty.
///
/// # Arguments
///
/// * `record` - Containing record signature.
/// * `subrecord` - Source subrecord signature.
/// * `node` - Declared payload definition.
///
/// # Returns
///
/// Whether an empty payload represents absence, not a fabricated zero FormID.
pub(crate) fn empty_unused_form_id(
    record: Signature,
    subrecord: Signature,
    node: &SchemaNode,
) -> bool {
    record == Signature(*b"MESG")
        && subrecord == Signature(*b"INAM")
        && node.conflict_priority == ConflictPriority::Ignore
        && matches!(&node.kind, SchemaNodeKind::Primitive {
            primitive: PrimitiveType::FormId { targets },
        } if targets.iter().all(|signature| signature.0 == *b"NULL"))
}

/// Recognizes one optional zero byte after a complete fixed-width string.
///
/// # Arguments
///
/// * `node` - Declared payload definition.
/// * `data` - Entire source subrecord payload.
///
/// # Returns
///
/// Whether the final byte is an additional terminator rather than string content.
pub(crate) fn fixed_string_terminator(node: &SchemaNode, data: &[u8]) -> bool {
    matches!(&node.kind, SchemaNodeKind::Primitive {
        primitive: PrimitiveType::String { string },
    } if !string.localized && !string.zero_terminated && string.length_prefix.is_none()
        && string.trailing_terminator.is_none()
        && string.fixed_length.is_some_and(|length| usize::try_from(length).ok()
            .and_then(|length| length.checked_add(1)) == Some(data.len()))
        && data.last() == Some(&0))
}

/// Resolves the known legacy critical-data layout without changing source bytes.
///
/// # Arguments
///
/// * `record` - Containing record signature.
/// * `subrecord` - Source subrecord signature.
/// * `node` - Declared modern payload definition.
/// * `length` - Actual source payload byte length.
///
/// # Returns
///
/// A compatible payload definition only for the exact recognized legacy layout.
pub(crate) fn weapon_critical_data(
    record: Signature,
    subrecord: Signature,
    node: &SchemaNode,
    length: usize,
) -> Option<SchemaNode> {
    if record != Signature(*b"WEAP") || subrecord != Signature(*b"CRDT") || length != 16 {
        return None;
    }
    let SchemaNodeKind::Struct { fields } = &node.kind else {
        return None;
    };
    // The LE representation has three padding bytes before Effect and no final padding.
    // Decode original offsets; the editor retains untouched subrecords verbatim.
    if fields.len() != 7
        || !matches!(&fields[0].kind, SchemaNodeKind::Primitive {
            primitive: PrimitiveType::Integer { integer },
        } if integer.width == 2)
        || !matches!(
            &fields[1].kind,
            SchemaNodeKind::Primitive {
                primitive: PrimitiveType::Bytes { length: Some(2) },
            }
        )
        || !matches!(
            &fields[2].kind,
            SchemaNodeKind::Primitive {
                primitive: PrimitiveType::Float { width: 4, .. },
            }
        )
        || !matches!(&fields[3].kind, SchemaNodeKind::Primitive {
            primitive: PrimitiveType::Integer { integer } | PrimitiveType::Flags { integer, .. },
        } if integer.width == 1)
        || !matches!(
            &fields[4].kind,
            SchemaNodeKind::Primitive {
                primitive: PrimitiveType::Bytes { length: Some(7) },
            }
        )
        || !matches!(
            &fields[5].kind,
            SchemaNodeKind::Primitive {
                primitive: PrimitiveType::FormId { .. },
            }
        )
        || !matches!(
            &fields[6].kind,
            SchemaNodeKind::Primitive {
                primitive: PrimitiveType::Bytes { length: Some(4) },
            }
        )
    {
        return None;
    }
    let mut legacy = node.clone();
    let mut legacy_fields = fields[..6].to_vec();
    legacy_fields[4].kind = SchemaNodeKind::Primitive {
        primitive: PrimitiveType::Bytes { length: Some(3) },
    };
    legacy.kind = SchemaNodeKind::Struct {
        fields: legacy_fields,
    };
    Some(legacy)
}
