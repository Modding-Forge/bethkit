// SPDX-License-Identifier: Apache-2.0
//!
//! Game catalogs and record-signature registries.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

use ahash::AHashMap;
use bethkit_core::{Game, Signature};

use crate::node_index::NodeIndex;
use crate::package::decode_bundle;
use crate::{
    CallbackBinding, Result, SchemaError, SchemaGame, SchemaLoadLimits, SchemaNode, SchemaPackage,
    SchemaRecord, SchemaSignature,
};

include!(concat!(env!("OUT_DIR"), "/embedded_catalog.rs"));

static EMBEDDED_CATALOG: OnceLock<SchemaCatalog> = OnceLock::new();
static EMBEDDED_CATALOG_INIT: Mutex<()> = Mutex::new(());

/// Record-signature index over one owned schema package.
#[derive(Clone)]
pub struct SchemaRegistry {
    package: Arc<SchemaPackage>,
    record_indices: BTreeMap<SchemaSignature, usize>,
    node_indices: Arc<Vec<OnceLock<NodeIndex>>>,
    callback_indices: Arc<OnceLock<AHashMap<String, Vec<usize>>>>,
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
        let node_indices = Arc::new(
            (0..package.records().len())
                .map(|_| OnceLock::new())
                .collect(),
        );
        Self {
            package,
            record_indices,
            node_indices,
            callback_indices: Arc::new(OnceLock::new()),
        }
    }

    /// Returns the schema for a main-record signature.
    pub fn get(&self, signature: Signature) -> Option<&SchemaRecord> {
        let index: usize = *self.record_indices.get(&SchemaSignature::from(signature))?;
        self.package.records().get(index)
    }

    /// Returns the node at an exact stable schema path for a main-record signature.
    pub fn get_node(&self, signature: Signature, path: &str) -> Option<&SchemaNode> {
        let index = *self.record_indices.get(&SchemaSignature::from(signature))?;
        let record = self.package.records().get(index)?;
        self.node_indices[index]
            .get_or_init(|| NodeIndex::new(&record.root))
            .get(&record.root, path)
    }

    /// Returns the owned package backing this registry.
    pub fn package(&self) -> &Arc<SchemaPackage> {
        &self.package
    }

    /// Returns callbacks at an exact schema path in their original declaration order.
    ///
    /// The immutable index is initialized once and shared by cloned registries. Duplicate
    /// bindings are preserved; callers retain their existing first-match or error policy.
    pub fn callbacks_at<'a>(&'a self, path: &str) -> impl Iterator<Item = &'a CallbackBinding> {
        let indices = self.callback_indices.get_or_init(|| {
            let mut indices: AHashMap<String, Vec<usize>> = AHashMap::new();
            for (index, binding) in self.package.callback_bindings().iter().enumerate() {
                indices.entry(binding.path.clone()).or_default().push(index);
            }
            indices
        });
        indices
            .get(path)
            .into_iter()
            .flatten()
            .map(|index| &self.package.callback_bindings()[*index])
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

/// Owned collection of schema packages keyed by game mode.
#[derive(Clone, Default)]
pub struct SchemaCatalog {
    packages: BTreeMap<SchemaGame, Arc<SchemaPackage>>,
}

impl SchemaCatalog {
    /// Creates an empty catalog.
    pub fn new() -> Self {
        Self::default()
    }

    /// Loads the release-time embedded catalog, sharing immutable validated packages.
    ///
    /// Successful initialization is retained for the process lifetime. Each returned
    /// catalog owns its own map; inserting packages does not change other catalogs.
    /// Failed loads are not cached and preserve their original error type.
    ///
    /// # Errors
    ///
    /// Returns [`SchemaError::EmbeddedUnavailable`] when the build has no
    /// embedded package, or another [`SchemaError`] when embedded data is
    /// invalid.
    pub fn embedded() -> Result<Self> {
        if let Some(catalog) = EMBEDDED_CATALOG.get() {
            return Ok(catalog.clone());
        }
        // The OnceLock, not the lock guard, owns initialization state. A panic can
        // therefore release a poisoned guard without exposing partial packages.
        let _initialization = EMBEDDED_CATALOG_INIT
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(catalog) = EMBEDDED_CATALOG.get() {
            return Ok(catalog.clone());
        }
        let catalog = Self::load_embedded()?;
        let _ = EMBEDDED_CATALOG.set(catalog.clone());
        Ok(catalog)
    }

    fn load_embedded() -> Result<Self> {
        if let Some(bytes) = EMBEDDED_BUNDLE {
            return Self::from_bundle_bytes(bytes);
        }
        if EMBEDDED_PACKAGES.is_empty() {
            return Err(SchemaError::EmbeddedUnavailable);
        }
        let mut catalog = Self::new();
        for bytes in EMBEDDED_PACKAGES {
            catalog.insert(SchemaPackage::from_bytes(bytes)?)?;
        }
        Ok(catalog)
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

#[cfg(all(test, feature = "schema-skyrim-se"))]
mod tests {
    use bethkit_core::Game;

    use super::*;
    use crate::ValidationStatus;

    /// Matches the linear oracle for every embedded binding, preserving identity and order.
    #[test]
    fn callback_index_preserves_declared_bindings(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let package = SchemaCatalog::embedded()?.require(Game::SkyrimSE)?;
        let registry = SchemaRegistry::new(package);
        assert!(registry
            .callbacks_at("missing/schema/path")
            .next()
            .is_none());
        for binding in registry.package().callback_bindings() {
            let expected: Vec<_> = registry
                .package()
                .callback_bindings()
                .iter()
                .filter(|candidate| candidate.path == binding.path)
                .collect();
            let actual: Vec<_> = registry.callbacks_at(&binding.path).collect();
            assert_eq!(actual.len(), expected.len());
            for (actual, expected) in actual.into_iter().zip(expected) {
                assert!(std::ptr::eq(actual, expected));
            }
        }
        let cloned = registry.clone();
        assert!(Arc::ptr_eq(
            &registry.callback_indices,
            &cloned.callback_indices
        ));
        Ok(())
    }

    /// Shares immutable packages across threads without sharing mutable catalog maps.
    #[test]
    fn embedded_packages_are_shared() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let catalog = SchemaCatalog::embedded()?;
        let original = catalog.require(Game::SkyrimSE)?;
        let workers: Vec<_> = (0..8)
            .map(|_| {
                std::thread::spawn(|| {
                    SchemaCatalog::embedded().and_then(|catalog| catalog.require(Game::SkyrimSE))
                })
            })
            .collect();
        for worker in workers {
            let package = worker
                .join()
                .expect("embedded catalog worker must not panic")?;
            assert!(Arc::ptr_eq(&original, &package));
        }
        let mut independent = SchemaCatalog::embedded()?;
        independent.packages.clear();
        assert!(independent.is_empty());
        assert_eq!(catalog.len(), 1);
        assert_eq!(SchemaCatalog::embedded()?.len(), 1);
        Ok(())
    }

    /// Confirms that a Skyrim-only build embeds no other game package.
    #[test]
    fn embedded_catalog_contains_only_skyrim_special_edition(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // when
        let catalog = SchemaCatalog::embedded()?;

        // then
        assert_eq!(catalog.len(), 1);
        let package = catalog.require(Game::SkyrimSE)?;
        assert_eq!(package.manifest().package_version, "0.1.0");
        assert_eq!(package.manifest().source_tag, "xedit-4.1.5f");
        assert_eq!(
            package.manifest().source_commit,
            "f5c00f3fa3ee39511185515802647246c807f759"
        );
        assert_eq!(
            package.manifest().validation_status,
            ValidationStatus::Candidate
        );
        assert_eq!(package.records().len(), 134);
        assert!(catalog.get(Game::SkyrimLE).is_none());
        Ok(())
    }
}
