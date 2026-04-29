// SPDX-License-Identifier: Apache-2.0
//! Morrowind (TES3) schema registry.
//!
//! Exposes a singleton [`SchemaRegistry`] pre-populated with all Morrowind
//! record schemas. Obtain it via [`crate::schema::SchemaRegistry::morrowind`].
//!
//! TES3 records differ fundamentally from later Bethesda games: no FormIDs,
//! no GRUP system, EditorIDs in `NAME` subrecords, and flat-file layout.

use std::sync::OnceLock;

use crate::schema::SchemaRegistry;

pub(super) mod common;
pub mod enums;
pub(super) mod actors;
pub(super) mod items;
pub(super) mod magic;
pub(super) mod quests;
pub(super) mod simple;
pub(super) mod world;

/// Returns the global Morrowind schema registry, building it on first call.
///
/// The returned reference is valid for the lifetime of the process.
pub(super) fn registry() -> &'static SchemaRegistry {
    static REGISTRY: OnceLock<SchemaRegistry> = OnceLock::new();
    REGISTRY.get_or_init(build)
}

fn build() -> SchemaRegistry {
    let mut reg = SchemaRegistry::new();

    // ── Simple / utility records ──────────────────────────────────────────────
    reg.register(&simple::TES3_SCHEMA);
    reg.register(&simple::GLOB_SCHEMA);
    reg.register(&simple::GMST_SCHEMA);
    reg.register(&simple::STAT_SCHEMA);
    reg.register(&simple::SOUN_SCHEMA);
    reg.register(&simple::SSCR_SCHEMA);
    reg.register(&simple::SNDG_SCHEMA);
    reg.register(&simple::BODY_SCHEMA);
    reg.register(&simple::LTEX_SCHEMA);
    reg.register(&simple::BSGN_SCHEMA);
    reg.register(&simple::REGN_SCHEMA);
    reg.register(&simple::CELL_SCHEMA);
    reg.register(&simple::LAND_SCHEMA);
    reg.register(&simple::PGRD_SCHEMA);

    // ── Actor records ─────────────────────────────────────────────────────────
    reg.register(&actors::NPC_SCHEMA);
    reg.register(&actors::CREA_SCHEMA);
    reg.register(&actors::RACE_SCHEMA);
    reg.register(&actors::CLAS_SCHEMA);
    reg.register(&actors::FACT_SCHEMA);
    reg.register(&actors::SKIL_SCHEMA);
    reg.register(&actors::MGEF_SCHEMA);

    // ── Item records ──────────────────────────────────────────────────────────
    reg.register(&items::WEAP_SCHEMA);
    reg.register(&items::ARMO_SCHEMA);
    reg.register(&items::CLOT_SCHEMA);
    reg.register(&items::BOOK_SCHEMA);
    reg.register(&items::ALCH_SCHEMA);
    reg.register(&items::INGR_SCHEMA);
    reg.register(&items::MISC_SCHEMA);
    reg.register(&items::APPA_SCHEMA);
    reg.register(&items::LOCK_SCHEMA);
    reg.register(&items::PROB_SCHEMA);
    reg.register(&items::REPA_SCHEMA);
    reg.register(&items::LIGH_SCHEMA);
    reg.register(&items::LEVC_SCHEMA);
    reg.register(&items::LEVI_SCHEMA);

    // ── World / placement records ─────────────────────────────────────────────
    reg.register(&world::ACTI_SCHEMA);
    reg.register(&world::DOOR_SCHEMA);
    reg.register(&world::CONT_SCHEMA);
    reg.register(&world::REFR_SCHEMA);

    // ── Magic records ─────────────────────────────────────────────────────────
    reg.register(&magic::SPEL_SCHEMA);
    reg.register(&magic::ENCH_SCHEMA);

    // ── Quest / dialogue records ──────────────────────────────────────────────
    reg.register(&quests::SCPT_SCHEMA);
    reg.register(&quests::DIAL_SCHEMA);
    reg.register(&quests::INFO_SCHEMA);

    reg
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Signature;

    /// Verifies that the Morrowind registry contains at least 40 record types.
    #[test]
    fn morrowind_registry_has_expected_count(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let reg = registry();
        assert!(
            reg.len() >= 40,
            "expected at least 40 Morrowind schemas, got {}",
            reg.len()
        );
        Ok(())
    }

    /// Verifies that `NPC_` is registered with the correct name.
    #[test]
    fn morrowind_npc_is_registered() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let reg = registry();
        let schema = reg.get(Signature(*b"NPC_")).ok_or("NPC_ not registered")?;
        assert_eq!(schema.name, "Non-Player Character");
        Ok(())
    }

    /// Verifies that `CREA` (creature, Morrowind-only) is registered.
    #[test]
    fn morrowind_crea_is_registered() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let reg = registry();
        let schema = reg.get(Signature(*b"CREA")).ok_or("CREA not registered")?;
        assert_eq!(schema.name, "Creature");
        Ok(())
    }

    /// Verifies that `WEAP` is registered with subrecord members.
    #[test]
    fn morrowind_weap_has_members() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let reg = registry();
        let schema = reg.get(Signature(*b"WEAP")).ok_or("WEAP not registered")?;
        assert!(!schema.members.is_empty());
        Ok(())
    }

    /// Verifies that `SPEL` (spell) is registered.
    #[test]
    fn morrowind_spel_is_registered() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let reg = registry();
        let schema = reg.get(Signature(*b"SPEL")).ok_or("SPEL not registered")?;
        assert_eq!(schema.name, "Spell");
        Ok(())
    }
}
