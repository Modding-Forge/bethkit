// SPDX-License-Identifier: Apache-2.0
//! Fallout 3 schema registry.
//!
//! Exposes a singleton [`SchemaRegistry`] pre-populated with all Fallout 3
//! record schemas. Obtain it via [`crate::schema::SchemaRegistry::fo3`].

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

/// Returns the global Fallout 3 schema registry, building it on first call.
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
    reg.register(&simple::TXST_SCHEMA);
    reg.register(&simple::MICN_SCHEMA);
    reg.register(&simple::GLOB_SCHEMA);
    reg.register(&simple::GMST_SCHEMA);
    reg.register(&simple::AVIF_SCHEMA);
    reg.register(&simple::VTYP_SCHEMA);
    reg.register(&simple::CAMS_SCHEMA);
    reg.register(&simple::CPTH_SCHEMA);
    reg.register(&simple::ASPC_SCHEMA);
    reg.register(&simple::IMGS_SCHEMA);
    reg.register(&simple::IMAD_SCHEMA);
    reg.register(&simple::LGTM_SCHEMA);
    reg.register(&simple::MUSC_SCHEMA);
    reg.register(&simple::ANIO_SCHEMA);
    reg.register(&simple::NAVI_SCHEMA);
    reg.register(&simple::DEBR_SCHEMA);
    reg.register(&simple::IDLM_SCHEMA);
    reg.register(&simple::FLST_SCHEMA);
    reg.register(&simple::DOBJ_SCHEMA);
    reg.register(&simple::MESG_SCHEMA);
    reg.register(&simple::EFSH_SCHEMA);
    reg.register(&simple::SCOL_SCHEMA);
    reg.register(&simple::RGDL_SCHEMA);
    reg.register(&simple::RADS_SCHEMA);
    reg.register(&simple::CLMT_SCHEMA);
    reg.register(&simple::SCPT_SCHEMA);
    reg.register(&simple::ECZN_SCHEMA);
    reg.register(&simple::WTHR_SCHEMA);

    // ── Actor records ─────────────────────────────────────────────────────────
    reg.register(&actors::NPC_SCHEMA);
    reg.register(&actors::CREA_SCHEMA);
    reg.register(&actors::RACE_SCHEMA);
    reg.register(&actors::PACK_SCHEMA);
    reg.register(&actors::FACT_SCHEMA);
    reg.register(&actors::CSTY_SCHEMA);
    reg.register(&actors::IDLE_SCHEMA);
    reg.register(&actors::LVLN_SCHEMA);
    reg.register(&actors::LVLC_SCHEMA);
    reg.register(&actors::LVLI_SCHEMA);
    reg.register(&actors::HAIR_SCHEMA);
    reg.register(&actors::EYES_SCHEMA);
    reg.register(&actors::CLAS_SCHEMA);
    reg.register(&actors::PERK_SCHEMA);
    reg.register(&actors::BPTD_SCHEMA);
    reg.register(&actors::HDPT_SCHEMA);

    // ── Item records ──────────────────────────────────────────────────────────
    reg.register(&items::WEAP_SCHEMA);
    reg.register(&items::ARMO_SCHEMA);
    reg.register(&items::ARMA_SCHEMA);
    reg.register(&items::AMMO_SCHEMA);
    reg.register(&items::BOOK_SCHEMA);
    reg.register(&items::ALCH_SCHEMA);
    reg.register(&items::INGR_SCHEMA);
    reg.register(&items::MISC_SCHEMA);
    reg.register(&items::KEYM_SCHEMA);
    reg.register(&items::NOTE_SCHEMA);
    reg.register(&items::TERM_SCHEMA);
    reg.register(&items::COBJ_SCHEMA);

    // ── Magic / combat records ────────────────────────────────────────────────
    reg.register(&magic::SPEL_SCHEMA);
    reg.register(&magic::MGEF_SCHEMA);
    reg.register(&magic::ENCH_SCHEMA);
    reg.register(&magic::PROJ_SCHEMA);
    reg.register(&magic::EXPL_SCHEMA);
    reg.register(&magic::IPCT_SCHEMA);
    reg.register(&magic::IPDS_SCHEMA);

    // ── World / environment records ───────────────────────────────────────────
    reg.register(&world::ACTI_SCHEMA);
    reg.register(&world::TACT_SCHEMA);
    reg.register(&world::DOOR_SCHEMA);
    reg.register(&world::CONT_SCHEMA);
    reg.register(&world::FURN_SCHEMA);
    reg.register(&world::LIGH_SCHEMA);
    reg.register(&world::STAT_SCHEMA);
    reg.register(&world::MSTT_SCHEMA);
    reg.register(&world::GRAS_SCHEMA);
    reg.register(&world::TREE_SCHEMA);
    reg.register(&world::CELL_SCHEMA);
    reg.register(&world::WRLD_SCHEMA);
    reg.register(&world::REGN_SCHEMA);
    reg.register(&world::NAVM_SCHEMA);
    reg.register(&world::WATR_SCHEMA);
    reg.register(&world::LSCR_SCHEMA);
    reg.register(&world::LTEX_SCHEMA);
    reg.register(&world::PWAT_SCHEMA);
    reg.register(&world::LAND_SCHEMA);
    reg.register(&world::ADDN_SCHEMA);
    reg.register(&world::PLYR_SCHEMA);

    // ── Quest / dialogue / audio records ──────────────────────────────────────
    reg.register(&quests::QUST_SCHEMA);
    reg.register(&quests::DIAL_SCHEMA);
    reg.register(&quests::INFO_SCHEMA);
    reg.register(&quests::SOUN_SCHEMA);

    reg
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that the Fallout 3 registry contains at least 80 record types.
    #[test]
    fn fo3_registry_has_expected_record_count(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let reg = registry();
        assert!(
            reg.len() >= 80,
            "expected at least 80 Fallout 3 schemas, got {}",
            reg.len()
        );
        Ok(())
    }

    /// Verifies that NPC_ is registered in the Fallout 3 registry.
    #[test]
    fn fo3_npc_schema_is_registered() -> std::result::Result<(), Box<dyn std::error::Error>> {
        use crate::types::Signature;
        let reg = registry();
        let schema =
            reg.get(Signature(*b"NPC_")).ok_or("NPC_ not registered in Fallout 3")?;
        assert_eq!(schema.name, "Non-Player Character");
        Ok(())
    }

    /// Verifies that CREA (creature, FO3-exclusive) is registered.
    #[test]
    fn fo3_crea_schema_is_registered() -> std::result::Result<(), Box<dyn std::error::Error>> {
        use crate::types::Signature;
        let reg = registry();
        reg.get(Signature(*b"CREA")).ok_or("CREA not registered in Fallout 3")?;
        Ok(())
    }

    /// Verifies that WEAP is registered with the correct name.
    #[test]
    fn fo3_weap_schema_is_registered() -> std::result::Result<(), Box<dyn std::error::Error>> {
        use crate::types::Signature;
        let reg = registry();
        let schema =
            reg.get(Signature(*b"WEAP")).ok_or("WEAP not registered in Fallout 3")?;
        assert_eq!(schema.name, "Weapon");
        Ok(())
    }

    /// Verifies that SCPT (script record, FO3-era) is registered.
    #[test]
    fn fo3_scpt_schema_is_registered() -> std::result::Result<(), Box<dyn std::error::Error>> {
        use crate::types::Signature;
        let reg = registry();
        reg.get(Signature(*b"SCPT")).ok_or("SCPT not registered in Fallout 3")?;
        Ok(())
    }
}
