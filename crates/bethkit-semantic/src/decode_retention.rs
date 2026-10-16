// SPDX-License-Identifier: Apache-2.0
//! Conservative observation analysis for decode-only container retention.

use bethkit_schema::{
    ArrayCount, PrimitiveType, SchemaNode, SchemaNodeKind, SchemaRegistry, UnionSelector,
};

/// Whether a decoder's container values or numeric selector state can be observed.
#[derive(Clone, Copy)]
pub(crate) enum DecodeRetention {
    /// Ordinary views and payloads with callbacks retain their complete trees.
    Full,
    /// Decode-only payloads may release unobserved values immediately.
    Discard {
        /// Expressions may still observe the latest decoded numeric values.
        track_numeric: bool,
    },
}

impl DecodeRetention {
    /// Proves eligibility conservatively for the actual payload after legacy adjustments.
    pub(crate) fn for_payload(registry: &SchemaRegistry, node: &SchemaNode) -> Self {
        match analyze(registry, node, 0) {
            Some(track_numeric) => Self::Discard { track_numeric },
            None => Self::Full,
        }
    }

    /// Reports whether container values can be observed by the caller or a callback.
    pub(crate) fn materializes(self) -> bool {
        matches!(self, Self::Full)
    }

    /// Retains expression inputs even when their enclosing containers are discarded.
    pub(crate) fn tracks_numeric(self) -> bool {
        !matches!(
            self,
            Self::Discard {
                track_numeric: false
            }
        )
    }

    /// Checks a complete array whose every byte pattern is valid and unobserved.
    pub(crate) fn array_bytes(
        self,
        element: &SchemaNode,
        count: Option<usize>,
        available: usize,
    ) -> Option<usize> {
        if self.materializes() || self.tracks_numeric() {
            return None;
        }
        let size = unconstrained_size(element, 0)?;
        let count = match count {
            Some(count) => count,
            None if size != 0 && available.is_multiple_of(size) => available / size,
            _ => return None,
        };
        let consumed = size.checked_mul(count)?;
        (consumed <= available).then_some(consumed)
    }
}

fn unconstrained_size(node: &SchemaNode, depth: usize) -> Option<usize> {
    if node.condition.is_some() || depth > 128 {
        return None;
    }
    match &node.kind {
        SchemaNodeKind::Primitive {
            primitive: PrimitiveType::Integer { integer },
        } if matches!(integer.width, 1 | 2 | 4 | 8) => Some(usize::from(integer.width)),
        SchemaNodeKind::Primitive {
            primitive: PrimitiveType::Bytes {
                length: Some(length),
            },
        } => usize::try_from(*length).ok(),
        SchemaNodeKind::Struct { fields } => fields.iter().try_fold(0_usize, |size, field| {
            size.checked_add(unconstrained_size(field, depth + 1)?)
        }),
        SchemaNodeKind::Array {
            element,
            count: ArrayCount::Fixed { count },
        } => unconstrained_size(element, depth + 1)?.checked_mul(*count as usize),
        // Strings, floats, terminators, selectors and custom payloads keep their decoder.
        _ => None,
    }
}

fn analyze(registry: &SchemaRegistry, node: &SchemaNode, depth: usize) -> Option<bool> {
    if depth > 128
        || registry.callbacks_at(&node.path).any(|binding| {
            matches!(
                binding.callback_id.as_str(),
                "float.normalizer" | "array.should_include"
            )
        })
    {
        return None;
    }
    let mut numeric = node.condition.is_some();
    let children: &[SchemaNode] = match &node.kind {
        SchemaNodeKind::Primitive { .. } => return Some(numeric),
        SchemaNodeKind::Struct { fields } | SchemaNodeKind::OptionalStruct { fields, .. } => fields,
        SchemaNodeKind::Array { element, count } => {
            if matches!(count, ArrayCount::Callback { .. }) {
                return None;
            }
            numeric |= matches!(count, ArrayCount::Expression { .. });
            std::slice::from_ref(element.as_ref())
        }
        SchemaNodeKind::Union { selector, variants } => {
            if matches!(selector, UnionSelector::Callback { .. }) {
                return None;
            }
            numeric = true;
            variants
        }
        SchemaNodeKind::Terminated { child, .. } => std::slice::from_ref(child.as_ref()),
        // Custom decoders and unresolved/unsupported nodes keep the exact ordinary
        // decoder path. Their observers, effects, or errors must not be inferred.
        _ => return None,
    };
    for child in children {
        numeric |= analyze(registry, child, depth + 1)?;
    }
    Some(numeric)
}
