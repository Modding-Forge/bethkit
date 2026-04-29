// SPDX-License-Identifier: Apache-2.0
//!
//! Hardcoded (implicit) FormID constants for supported games.
//!
//! Some records are referenced by other records but are never written to any
//! plugin file — they are permanently embedded in the engine. A complete set
//! of these [`GlobalFormId`]s is exposed here so that patchers and other
//! consumers can recognise and skip them when iterating load-order data.
//!
//! The design mirrors Mutagen's `Implicits.RecordFormKeys` and game-specific
//! `Constants` classes.
//!
//! # Example
//!
//! ```rust
//! use bethkit_core::ImplicitRecords;
//!
//! let implicits = ImplicitRecords::sse();
//! let player = ImplicitRecords::sse_player();
//!
//! assert!(implicits.contains(&player));
//! assert_eq!(implicits.len(), 23);
//! ```

use ahash::HashSet;

use crate::load_order::GlobalFormId;

/// The set of globally-unique FormIDs that are hardcoded by the engine and
/// never written to any plugin file.
///
/// Records in this set are engine-internal: they exist in the game runtime but
/// do not appear as actual records inside `Skyrim.esm` or any other plugin.
/// Patchers should skip them when duplicating or iterating winning-override
/// records.
///
/// Use [`ImplicitRecords::sse`] to obtain the Skyrim Special Edition set.
/// Well-known individual FormIDs (e.g. the player reference) are available
/// as associated functions such as [`ImplicitRecords::sse_player`].
pub struct ImplicitRecords {
    inner: HashSet<GlobalFormId>,
}

impl ImplicitRecords {
    /// Returns the implicit record set for Skyrim Special Edition (and by
    /// extension Skyrim LE and Skyrim VR, which share the same base masters).
    ///
    /// The 23 FormIDs included here match Mutagen's `Implicits.RecordFormKeys`
    /// list for `GameRelease.SkyrimSE`.
    pub fn sse() -> Self {
        const SKYRIM: &str = "skyrim.esm";

        // Object IDs taken from Mutagen Implicits.cs (GameRelease.SkyrimSE).
        //
        // Categories:
        //   Actor Value Information : 0x3F5, 0x5E0-0x5E1, 0x5E6, 0x5EA,
        //                             0x5EE-0x5EF, 0x5FC, 0x60B, 0x62F,
        //                             0x63C, 0x644, 0x647-0x649
        //   Body Part Data          : 0x1C
        //   Eyes                    : 0x1A
        //   Globals                 : 0x63
        //   Image Space Adapter     : 0x164, 0x166
        //   Impact Data Set         : 0x276
        //   Player Reference        : 0x14
        //   Texture Set             : 0x28
        const OBJECT_IDS: &[u32] = &[
            0x0000_03F5,
            0x0000_05E0,
            0x0000_05E1,
            0x0000_05E6,
            0x0000_05EA,
            0x0000_05EE,
            0x0000_05EF,
            0x0000_05FC,
            0x0000_060B,
            0x0000_062F,
            0x0000_063C,
            0x0000_0644,
            0x0000_0647,
            0x0000_0648,
            0x0000_0649,
            0x0000_001C,
            0x0000_001A,
            0x0000_0063,
            0x0000_0164,
            0x0000_0166,
            0x0000_0276,
            0x0000_0014,
            0x0000_0028,
        ];

        let inner: HashSet<GlobalFormId> = OBJECT_IDS
            .iter()
            .map(|&object_id| GlobalFormId {
                plugin_name: SKYRIM.to_owned(),
                object_id,
            })
            .collect();

        Self { inner }
    }

    /// Returns `true` if `gfid` is a hardcoded implicit record in this set.
    ///
    /// # Arguments
    ///
    /// * `gfid` - The globally-unique FormID to test.
    pub fn contains(&self, gfid: &GlobalFormId) -> bool {
        self.inner.contains(gfid)
    }

    /// Returns the number of implicit FormIDs in this set.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns `true` if the set contains no implicit FormIDs.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Iterates over all implicit [`GlobalFormId`]s in this set.
    ///
    /// Iteration order is unspecified.
    pub fn iter(&self) -> impl Iterator<Item = &GlobalFormId> + '_ {
        self.inner.iter()
    }

    /// Returns the [`GlobalFormId`] of the Player Reference record for
    /// Skyrim Special Edition (`skyrim.esm:0x00000014`).
    ///
    /// This record is never present in any plugin file; the engine constructs
    /// it at startup. Use this constant to refer to the player character
    /// without relying on an EditorID lookup.
    pub fn sse_player() -> GlobalFormId {
        GlobalFormId {
            plugin_name: "skyrim.esm".to_owned(),
            object_id: 0x0000_0014,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that the SSE implicit set contains exactly 23 entries.
    #[test]
    fn sse_contains_23_entries() {
        assert_eq!(ImplicitRecords::sse().len(), 23);
    }

    /// Verifies that the player reference FormID is contained in the SSE set.
    #[test]
    fn sse_contains_player_reference() -> Result<(), Box<dyn std::error::Error>> {
        let implicits = ImplicitRecords::sse();
        let player = ImplicitRecords::sse_player();
        assert!(implicits.contains(&player));
        Ok(())
    }

    /// Verifies that a random non-implicit FormID is not in the SSE set.
    #[test]
    fn sse_does_not_contain_non_implicit() -> Result<(), Box<dyn std::error::Error>> {
        let implicits = ImplicitRecords::sse();
        let not_implicit = GlobalFormId {
            plugin_name: "skyrim.esm".to_owned(),
            object_id: 0x0001_2345,
        };
        assert!(!implicits.contains(&not_implicit));
        Ok(())
    }

    /// Verifies that the player reference has the expected object ID (0x14).
    #[test]
    fn sse_player_has_correct_object_id() {
        let player = ImplicitRecords::sse_player();
        assert_eq!(player.object_id, 0x14);
        assert_eq!(player.plugin_name, "skyrim.esm");
    }
}
