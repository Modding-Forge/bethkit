// SPDX-License-Identifier: Apache-2.0
//!
//! Game catalogs and record-signature registries.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use bethkit_core::{Game, Signature};

use crate::package::decode_bundle;
use crate::{
    Result, SchemaError, SchemaGame, SchemaLoadLimits, SchemaNode, SchemaNodeKind, SchemaPackage,
    SchemaRecord, SchemaSignature,
};

include!(concat!(env!("OUT_DIR"), "/embedded_catalog.rs"));

/// Record-signature index over one owned schema package.
#[derive(Clone)]
pub struct SchemaRegistry {
    package: Arc<SchemaPackage>,
    record_indices: BTreeMap<SchemaSignature, usize>,
}

impl SchemaRegistry {
    /// Builds a registry over an owned package.
    pub fn new(package: Arc<SchemaPackage>) -> Self {
        let record_indices: BTreeMap<SchemaSignature, usize> = package
            .records()
            .iter()
            .enumerate()
            .map(|(index, record)| (record.signature, index))
            .collect();
        Self {
            package,
            record_indices,
        }
    }

    /// Returns the schema for a main-record signature.
    pub fn get(&self, signature: Signature) -> Option<&SchemaRecord> {
        let index: usize = *self.record_indices.get(&SchemaSignature::from(signature))?;
        self.package.records().get(index)
    }

    /// Returns the node at an exact stable schema path for a main-record signature.
    pub fn get_node(&self, signature: Signature, path: &str) -> Option<&SchemaNode> {
        let record = self.get(signature)?;
        find_node(&record.root, path)
    }

    /// Returns the owned package backing this registry.
    pub fn package(&self) -> &Arc<SchemaPackage> {
        &self.package
    }

    /// Returns the number of indexed record signatures.
    pub fn len(&self) -> usize {
        self.record_indices.len()
    }

    /// Returns whether no record schemas are indexed.
    pub fn is_empty(&self) -> bool {
        self.record_indices.is_empty()
    }
}

fn find_node<'a>(node: &'a SchemaNode, path: &str) -> Option<&'a SchemaNode> {
    if node.path == path {
        return Some(node);
    }
    match &node.kind {
        SchemaNodeKind::Sequence { children } => find_in_nodes(children, path),
        SchemaNodeKind::Choice { alternatives }
        | SchemaNodeKind::SelectedChoice { alternatives, .. } => find_in_nodes(alternatives, path),
        SchemaNodeKind::Repeat { child, .. }
        | SchemaNodeKind::Subrecord { payload: child, .. }
        | SchemaNodeKind::Array { element: child, .. }
        | SchemaNodeKind::Compressed { child, .. }
        | SchemaNodeKind::Terminated { child, .. } => find_node(child, path),
        SchemaNodeKind::Struct { fields } => find_in_nodes(fields, path),
        SchemaNodeKind::Union { variants, .. } => find_in_nodes(variants, path),
        SchemaNodeKind::Primitive { .. }
        | SchemaNodeKind::Custom { .. }
        | SchemaNodeKind::Reference { .. } => None,
    }
}

fn find_in_nodes<'a>(nodes: &'a [SchemaNode], path: &str) -> Option<&'a SchemaNode> {
    nodes.iter().find_map(|node| find_node(node, path))
}

/// Owned collection of schema packages keyed by game mode.
#[derive(Default)]
pub struct SchemaCatalog {
    packages: BTreeMap<SchemaGame, Arc<SchemaPackage>>,
}

impl SchemaCatalog {
    /// Creates an empty catalog.
    pub fn new() -> Self {
        Self::default()
    }

    /// Loads the release-time embedded catalog.
    ///
    /// # Errors
    ///
    /// Returns [`SchemaError::EmbeddedUnavailable`] when this source build
    /// was not compiled with `BETHKIT_SCHEMA_BUNDLE`, or another
    /// [`SchemaError`] when the embedded bundle is invalid.
    pub fn embedded() -> Result<Self> {
        let bytes: &[u8] = EMBEDDED_BUNDLE.ok_or(SchemaError::EmbeddedUnavailable)?;
        Self::from_bundle_bytes(bytes)
    }

    /// Loads a catalog bundle from disk.
    ///
    /// # Errors
    ///
    /// Returns [`SchemaError`] when the file cannot be read or the catalog
    /// contains an invalid package.
    pub fn open(path: &Path) -> Result<Self> {
        let bytes: Vec<u8> = fs::read(path)?;
        Self::from_bundle_bytes(&bytes)
    }

    /// Loads a catalog bundle with default safety limits.
    ///
    /// # Errors
    ///
    /// Returns [`SchemaError`] when the bundle or one of its packages is
    /// invalid.
    pub fn from_bundle_bytes(bytes: &[u8]) -> Result<Self> {
        let packages: Vec<SchemaPackage> = decode_bundle(bytes, &SchemaLoadLimits::default())?;
        let mut catalog = Self::new();
        for package in packages {
            catalog.insert(package)?;
        }
        Ok(catalog)
    }

    /// Inserts a package.
    ///
    /// # Errors
    ///
    /// Returns [`SchemaError::InvalidGraph`] when a package for the same game
    /// already exists.
    pub fn insert(&mut self, package: SchemaPackage) -> Result<()> {
        let game: SchemaGame = package.manifest().game;
        if self.packages.contains_key(&game) {
            return Err(SchemaError::InvalidGraph(format!(
                "catalog already contains {}",
                game.slug()
            )));
        }
        self.packages.insert(game, Arc::new(package));
        Ok(())
    }

    /// Returns the package for a game when present.
    pub fn get(&self, game: Game) -> Option<Arc<SchemaPackage>> {
        self.packages.get(&SchemaGame::from(game)).cloned()
    }

    /// Returns the package for a game.
    ///
    /// # Errors
    ///
    /// Returns [`SchemaError::MissingGame`] when the catalog does not contain
    /// the requested game.
    pub fn require(&self, game: Game) -> Result<Arc<SchemaPackage>> {
        self.get(game)
            .ok_or_else(|| SchemaError::MissingGame(SchemaGame::from(game).slug().to_owned()))
    }

    /// Returns the number of game packages.
    pub fn len(&self) -> usize {
        self.packages.len()
    }

    /// Returns whether the catalog contains no packages.
    pub fn is_empty(&self) -> bool {
        self.packages.is_empty()
    }
}
