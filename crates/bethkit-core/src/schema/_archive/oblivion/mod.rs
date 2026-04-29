// SPDX-License-Identifier: Apache-2.0
//! Oblivion (TES4) schema registry.
//!
//! Exposes a singleton [`SchemaRegistry`] pre-populated with all Oblivion
//! record schemas. Obtain it via [`crate::schema::SchemaRegistry::oblivion`].

use std::sync::OnceLock;

use crate::schema::SchemaRegistry;

mod common;
pub mod enums;
mod actors;
mod items;
mod magic;
mod quests;
mod simple;
mod world;

/// Returns the global Oblivion schema registry, building it on first call.
///
/// The returned reference is valid for the lifetime of the process.
pub(super) fn registry() -> &'static SchemaRegistry {
    static REGISTRY: OnceLock<SchemaRegistry> = OnceLock::new();
    REGISTRY.get_or_init(build)
}

fn build() -> SchemaRegistry {
    let mut reg = SchemaRegistry::new();

    // ── Simple / utility records ──────────────────────────────────────────────
    reg.register(&simple::TES4_SCHEMA);
    reg.register(&simple::GMST_SCHEMA);
    reg.register(&simple::GLOB_SCHEMA);
    reg.register(&simple::ANIO_SCHEMA);
    reg.register(&simple::GRAS_SCHEMA);
    reg.register(&simple::STAT_SCHEMA);
    reg.register(&simple::LTEX_SCHEMA);
    reg.register(&simple::WATR_SCHEMA);
    reg.register(&simple::WTHR_SCHEMA);
    reg.register(&simple::CLMT_SCHEMA);
    reg.register(&simple::LAND_SCHEMA);
    reg.register(&simple::PGRD_SCHEMA);
    reg.register(&simple::ROAD_SCHEMA);
    reg.register(&simple::SBSP_SCHEMA);
    reg.register(&simple::SCPT_SCHEMA);
    reg.register(&simple::SKIL_SCHEMA);
    reg.register(&simple::BSGN_SCHEMA);
    reg.register(&simple::SGST_SCHEMA);
    reg.register(&simple::EFSH_SCHEMA);
    reg.register(&simple::LSCR_SCHEMA);
    reg.register(&simple::SOUN_SCHEMA);

    // ── Actor records ─────────────────────────────────────────────────────────
    reg.register(&actors::NPC_SCHEMA);
    reg.register(&actors::CREA_SCHEMA);
    reg.register(&actors::RACE_SCHEMA);
    reg.register(&actors::PACK_SCHEMA);
    reg.register(&actors::FACT_SCHEMA);
    reg.register(&actors::CSTY_SCHEMA);
    reg.register(&actors::IDLE_SCHEMA);
    reg.register(&actors::LVLC_SCHEMA);
    reg.register(&actors::LVLI_SCHEMA);
    reg.register(&actors::LVSP_SCHEMA);
    reg.register(&actors::EYES_SCHEMA);
    reg.register(&actors::HAIR_SCHEMA);
    reg.register(&actors::CLAS_SCHEMA);

    // ── Item records ──────────────────────────────────────────────────────────
    reg.register(&items::WEAP_SCHEMA);
    reg.register(&items::ARMO_SCHEMA);
    reg.register(&items::CLOT_SCHEMA);
    reg.register(&items::AMMO_SCHEMA);
    reg.register(&items::BOOK_SCHEMA);
    reg.register(&items::ALCH_SCHEMA);
    reg.register(&items::INGR_SCHEMA);
    reg.register(&items::MISC_SCHEMA);
    reg.register(&items::KEYM_SCHEMA);
    reg.register(&items::SLGM_SCHEMA);
    reg.register(&items::APPA_SCHEMA);
    reg.register(&items::CONT_SCHEMA);
    reg.register(&items::DOOR_SCHEMA);
    reg.register(&items::FURN_SCHEMA);
    reg.register(&items::LIGH_SCHEMA);

    // ── Magic records ─────────────────────────────────────────────────────────
    reg.register(&magic::SPEL_SCHEMA);
    reg.register(&magic::MGEF_SCHEMA);
    reg.register(&magic::ENCH_SCHEMA);

    // ── World / environment records ───────────────────────────────────────────
    reg.register(&world::ACTI_SCHEMA);
    reg.register(&world::TREE_SCHEMA);
    reg.register(&world::FLOR_SCHEMA);
    reg.register(&world::CELL_SCHEMA);
    reg.register(&world::WRLD_SCHEMA);
    reg.register(&world::REGN_SCHEMA);

    // ── Quest / dialogue records ──────────────────────────────────────────────
    reg.register(&quests::QUST_SCHEMA);
    reg.register(&quests::DIAL_SCHEMA);
    reg.register(&quests::INFO_SCHEMA);
    reg.register(&quests::PLYR_SCHEMA);

    reg
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that the Oblivion registry contains at least 60 record types.
    #[test]
    fn oblivion_registry_has_expected_record_count(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let reg = registry();
        assert!(
            reg.len() >= 60,
            "expected at least 60 Oblivion schemas, got {}",
            reg.len()
        );
        Ok(())
    }

    /// Verifies that NPC_ is registered in the Oblivion registry.
    #[test]
    fn oblivion_npc_schema_is_registered() -> std::result::Result<(), Box<dyn std::error::Error>>
    {
        use crate::types::Signature;
        let reg = registry();
        let schema = reg.get(Signature(*b"NPC_")).ok_or("NPC_ not registered in Oblivion")?;
        assert_eq!(schema.name, "Non-Player Character");
        Ok(())
    }

    /// Verifies that CREA (Oblivion-exclusive creature record) is registered.
    #[test]
    fn oblivion_crea_schema_is_registered() -> std::result::Result<(), Box<dyn std::error::Error>>
    {
        use crate::types::Signature;
        let reg = registry();
        reg.get(Signature(*b"CREA")).ok_or("CREA not registered in Oblivion")?;
        Ok(())
    }

    /// Verifies that CLOT (Oblivion-exclusive clothing record) is registered.
    #[test]
    fn oblivion_clot_schema_is_registered() -> std::result::Result<(), Box<dyn std::error::Error>>
    {
        use crate::types::Signature;
        let reg = registry();
        reg.get(Signature(*b"CLOT")).ok_or("CLOT not registered in Oblivion")?;
        Ok(())
    }
}
