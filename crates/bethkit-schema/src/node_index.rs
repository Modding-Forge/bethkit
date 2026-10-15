// SPDX-License-Identifier: Apache-2.0
//! Safe immutable node routes, avoiding repeated full-schema path searches.

use ahash::AHashMap;

use crate::{SchemaNode, SchemaNodeKind};

pub(crate) struct NodeIndex {
    routes: AHashMap<String, Vec<usize>>,
}

impl NodeIndex {
    pub(crate) fn new(root: &SchemaNode) -> Self {
        let mut routes = AHashMap::new();
        collect(root, &mut Vec::new(), &mut routes);
        Self { routes }
    }

    pub(crate) fn get<'a>(&self, mut root: &'a SchemaNode, path: &str) -> Option<&'a SchemaNode> {
        for index in self.routes.get(path)? {
            root = children(root).get(*index)?;
        }
        Some(root)
    }
}

fn children(node: &SchemaNode) -> &[SchemaNode] {
    match &node.kind {
        SchemaNodeKind::Sequence { children } | SchemaNodeKind::Unordered { children } => children,
        SchemaNodeKind::Choice { alternatives }
        | SchemaNodeKind::SelectedChoice { alternatives, .. } => alternatives,
        SchemaNodeKind::Struct { fields } | SchemaNodeKind::OptionalStruct { fields, .. } => fields,
        SchemaNodeKind::Union { variants, .. } => variants,
        SchemaNodeKind::Repeat { child, .. }
        | SchemaNodeKind::Subrecord { payload: child, .. }
        | SchemaNodeKind::Array { element: child, .. }
        | SchemaNodeKind::Compressed { child, .. }
        | SchemaNodeKind::Terminated { child, .. } => std::slice::from_ref(child.as_ref()),
        _ => &[],
    }
}

fn collect(node: &SchemaNode, route: &mut Vec<usize>, routes: &mut AHashMap<String, Vec<usize>>) {
    // Keep first-match traversal semantics even for an externally supplied duplicate path.
    routes
        .entry(node.path.clone())
        .or_insert_with(|| route.clone());
    for (index, child) in children(node).iter().enumerate() {
        route.push(index);
        collect(child, route, routes);
        route.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ConflictPriority, PrimitiveType, SchemaNodeId};

    fn leaf(id: u32, path: &str) -> SchemaNode {
        SchemaNode {
            id: SchemaNodeId(id),
            path: path.to_owned(),
            name: path.to_owned(),
            required: false,
            condition: None,
            conflict_priority: ConflictPriority::Normal,
            kind: SchemaNodeKind::Primitive {
                primitive: PrimitiveType::Bytes { length: None },
            },
        }
    }

    /// Resolves every node in the actual embedded game graph to its original allocation.
    #[cfg(feature = "schema-skyrim-se")]
    #[test]
    fn embedded_graph_routes_are_exact() -> std::result::Result<(), Box<dyn std::error::Error>> {
        fn verify(node: &SchemaNode, root: &SchemaNode, index: &NodeIndex) {
            assert!(std::ptr::eq(
                index.get(root, &node.path).expect("node indexed"),
                node
            ));
            for child in children(node) {
                verify(child, root, index);
            }
        }
        let catalog = crate::SchemaCatalog::embedded()?;
        let package = catalog.require(bethkit_core::Game::SkyrimSE)?;
        for record in package.records() {
            let index = NodeIndex::new(&record.root);
            verify(&record.root, &record.root, &index);
        }
        Ok(())
    }

    /// Preserves original-node identity, missing paths, wrapper routes and first matches.
    #[test]
    fn routes_preserve_tree_identity() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let mut nested = leaf(2, "TEST/repeat");
        nested.kind = SchemaNodeKind::Repeat {
            minimum: 0,
            maximum: None,
            child: Box::new(leaf(3, "TEST/value")),
        };
        let mut root = leaf(1, "TEST");
        root.kind = SchemaNodeKind::Sequence {
            children: vec![nested, leaf(4, "TEST/value")],
        };
        let index = NodeIndex::new(&root);
        assert!(std::ptr::eq(
            index.get(&root, "TEST").expect("root indexed"),
            &root
        ));
        assert_eq!(
            index.get(&root, "TEST/value").expect("child indexed").id.0,
            3
        );
        assert!(index.get(&root, "missing").is_none());
        Ok(())
    }
}
