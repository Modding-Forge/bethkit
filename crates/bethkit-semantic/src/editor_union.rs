// SPDX-License-Identifier: Apache-2.0
//! Resolves forward union reads against in-flight fixed-layout sibling values.

use std::borrow::Cow;
use std::collections::BTreeMap;

use bethkit_schema::{ByteOrder, Expression, PrimitiveType, SchemaNode, SchemaNodeKind};

use super::{find_containing_subrecord, RecordEditor};

impl RecordEditor {
    /// Resolves forward integer reads using current, unambiguous sibling values.
    ///
    /// # Arguments
    ///
    /// * `node` - Union whose raw offsets are relative to its own payload start.
    /// * `expression` - Original schema selector, never modified in place.
    /// * `encoded_size` - Number of bytes supplied by the candidate variant.
    /// * `values` - Integer values including the in-flight containing payload.
    ///
    /// # Returns
    ///
    /// Returns the original expression when no forward resolution is needed,
    /// or a copy with proven fixed-layout reads replaced by their current values.
    /// Unresolved reads remain intact so normal bounds checking rejects them.
    pub(super) fn union_expression<'a>(
        &self,
        node: &SchemaNode,
        expression: &'a Expression,
        encoded_size: usize,
        values: &BTreeMap<String, i64>,
    ) -> Cow<'a, Expression> {
        if !has_forward_reads(expression, encoded_size) {
            return Cow::Borrowed(expression);
        }
        let Some(schema) = self.registry.get(self.record.signature) else {
            return Cow::Borrowed(expression);
        };
        let Some(parent) = find_containing_subrecord(&schema.root, &node.path) else {
            return Cow::Borrowed(expression);
        };
        let SchemaNodeKind::Subrecord { payload, .. } = &parent.kind else {
            return Cow::Borrowed(expression);
        };
        let Some(base) = fixed_offset(payload, &node.path) else {
            return Cow::Borrowed(expression);
        };
        let mut expression = expression.clone();
        // xEdit resolves later siblings before falling back to its union-relative raw pointer.
        // Only fixed, unambiguous integer spans are substituted; dynamic layouts fail closed.
        resolve_reads(&mut expression, &|offset, width| {
            let relative = usize::try_from(offset).ok()?;
            if relative.checked_add(usize::from(width))? <= encoded_size {
                return None;
            }
            let field = integer_at(payload, base.checked_add(relative)?, width)?;
            let value = *values.get(&field.path)?;
            let bits = u32::from(width).checked_mul(8)?;
            let mask = u64::MAX.checked_shr(64_u32.checked_sub(bits)?)?;
            i64::try_from((value as u64) & mask).ok()
        });
        Cow::Owned(expression)
    }
}

fn has_forward_reads(expression: &Expression, encoded_size: usize) -> bool {
    match expression {
        Expression::ReadUnsigned { offset, width } => usize::try_from(*offset)
            .ok()
            .and_then(|offset| offset.checked_add(usize::from(*width)))
            .is_some_and(|end| end > encoded_size),
        Expression::Equal { left, right }
        | Expression::NotEqual { left, right }
        | Expression::LessThan { left, right }
        | Expression::Add { left, right }
        | Expression::Subtract { left, right }
        | Expression::Multiply { left, right }
        | Expression::Divide { left, right } => {
            has_forward_reads(left, encoded_size) || has_forward_reads(right, encoded_size)
        }
        Expression::BitSet { value, .. }
        | Expression::Not { value }
        | Expression::BitCount { value }
        | Expression::IntegerLookup { value, .. } => has_forward_reads(value, encoded_size),
        Expression::And { values } | Expression::Or { values } => values
            .iter()
            .any(|value| has_forward_reads(value, encoded_size)),
        Expression::Select {
            condition,
            if_true,
            if_false,
        } => {
            has_forward_reads(condition, encoded_size)
                || has_forward_reads(if_true, encoded_size)
                || has_forward_reads(if_false, encoded_size)
        }
        _ => false,
    }
}

fn resolve_reads(expression: &mut Expression, resolve: &impl Fn(u32, u8) -> Option<i64>) {
    match expression {
        Expression::ReadUnsigned { offset, width } => {
            if let Some(value) = resolve(*offset, *width) {
                *expression = Expression::Int { value };
            }
        }
        Expression::Equal { left, right }
        | Expression::NotEqual { left, right }
        | Expression::LessThan { left, right }
        | Expression::Add { left, right }
        | Expression::Subtract { left, right }
        | Expression::Multiply { left, right }
        | Expression::Divide { left, right } => {
            resolve_reads(left, resolve);
            resolve_reads(right, resolve);
        }
        Expression::BitSet { value, .. }
        | Expression::Not { value }
        | Expression::BitCount { value }
        | Expression::IntegerLookup { value, .. } => resolve_reads(value, resolve),
        Expression::And { values } | Expression::Or { values } => {
            for value in values {
                resolve_reads(value, resolve);
            }
        }
        Expression::Select {
            condition,
            if_true,
            if_false,
        } => {
            resolve_reads(condition, resolve);
            resolve_reads(if_true, resolve);
            resolve_reads(if_false, resolve);
        }
        _ => {}
    }
}

fn fixed_offset(node: &SchemaNode, path: &str) -> Option<usize> {
    if node.condition.is_some() {
        return None;
    }
    if node.path == path {
        return Some(0);
    }
    let SchemaNodeKind::Struct { fields } = &node.kind else {
        return None;
    };
    let mut offset = 0_usize;
    for field in fields {
        if let Some(nested) = fixed_offset(field, path) {
            return offset.checked_add(nested);
        }
        offset = offset.checked_add(fixed_size(field)?)?;
    }
    None
}

fn integer_at(node: &SchemaNode, offset: usize, width: u8) -> Option<&SchemaNode> {
    if node.condition.is_some() {
        return None;
    }
    match &node.kind {
        SchemaNodeKind::Struct { fields } => {
            let mut remaining = offset;
            for field in fields {
                let size = fixed_size(field)?;
                if remaining < size {
                    return integer_at(field, remaining, width);
                }
                remaining = remaining.checked_sub(size)?;
            }
            None
        }
        SchemaNodeKind::Primitive { primitive } if offset == 0 => match primitive {
            PrimitiveType::Integer { integer }
            | PrimitiveType::Enumeration { integer, .. }
            | PrimitiveType::Flags { integer, .. }
                if integer.width == width && integer.byte_order == ByteOrder::LittleEndian =>
            {
                Some(node)
            }
            PrimitiveType::FormId { .. } if width == 4 => Some(node),
            _ => None,
        },
        _ => None,
    }
}

fn fixed_size(node: &SchemaNode) -> Option<usize> {
    if node.condition.is_some() {
        return None;
    }
    match &node.kind {
        SchemaNodeKind::Primitive { primitive } => match primitive {
            PrimitiveType::Integer { integer }
            | PrimitiveType::Enumeration { integer, .. }
            | PrimitiveType::Flags { integer, .. } => Some(usize::from(integer.width)),
            PrimitiveType::Float { width, .. } => Some(usize::from(*width)),
            PrimitiveType::FormId { .. } => Some(4),
            PrimitiveType::Bytes {
                length: Some(length),
            }
            | PrimitiveType::Unused { length } => usize::try_from(*length).ok(),
            _ => None,
        },
        SchemaNodeKind::Struct { fields } => fields
            .iter()
            .try_fold(0_usize, |size, field| size.checked_add(fixed_size(field)?)),
        SchemaNodeKind::Union { variants, .. } => {
            let size = fixed_size(variants.first()?)?;
            variants
                .iter()
                .all(|variant| fixed_size(variant) == Some(size))
                .then_some(size)
        }
        _ => None,
    }
}
