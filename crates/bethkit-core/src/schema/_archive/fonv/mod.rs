// SPDX-License-Identifier: Apache-2.0
//! Fallout New Vegas schema registry.
//!
//! FNV is a superset of Fallout 3: it shares all FO3 records and adds ~17
//! exclusive records (challenges, reputation, casinos, crafting recipes, item
//! mods, survival stages, and media systems).
//!
//! Obtain the registry via [`crate::schema::SchemaRegistry::fonv`].

use std::sync::OnceLock;

use crate::schema::SchemaRegistry;

mod exclusive;

/// Returns the global Fallout New Vegas schema registry, building it on first
/// call.
///
/// The returned reference is valid for the lifetime of the process.
pub(super) fn registry() -> &'static SchemaRegistry {
    static REGISTRY: OnceLock<SchemaRegistry> = OnceLock::new();
    REGISTRY.get_or_init(build)
}

fn build() -> SchemaRegistry {
    use super::fo3::{actors, items, magic, quests, simple, world};

    let mut reg = SchemaRegistry::new();

    // ── FO3 simple / utility records ─────────────────────────────────────────
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

    // ── FO3 actor records ─────────────────────────────────────────────────────
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

    // ── FO3 item records ──────────────────────────────────────────────────────
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

    // ── FO3 magic / combat records ────────────────────────────────────────────
    reg.register(&magic::SPEL_SCHEMA);
    reg.register(&magic::MGEF_SCHEMA);
    reg.register(&magic::ENCH_SCHEMA);
    reg.register(&magic::PROJ_SCHEMA);
    reg.register(&magic::EXPL_SCHEMA);
    reg.register(&magic::IPCT_SCHEMA);
    reg.register(&magic::IPDS_SCHEMA);

    // ── FO3 world / environment records ───────────────────────────────────────
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

    // ── FO3 quest / dialogue / audio records ──────────────────────────────────
    reg.register(&quests::QUST_SCHEMA);
    reg.register(&quests::DIAL_SCHEMA);
    reg.register(&quests::INFO_SCHEMA);
    reg.register(&quests::SOUN_SCHEMA);

    // ── FNV-exclusive records ─────────────────────────────────────────────────
    reg.register(&exclusive::CHAL_SCHEMA);
    reg.register(&exclusive::REPU_SCHEMA);
    reg.register(&exclusive::IMOD_SCHEMA);
    reg.register(&exclusive::RCPE_SCHEMA);
    reg.register(&exclusive::RCCT_SCHEMA);
    reg.register(&exclusive::CSNO_SCHEMA);
    reg.register(&exclusive::CHIP_SCHEMA);
    reg.register(&exclusive::CCRD_SCHEMA);
    reg.register(&exclusive::CDCK_SCHEMA);
    reg.register(&exclusive::CMNY_SCHEMA);
    reg.register(&exclusive::DEHY_SCHEMA);
    reg.register(&exclusive::HUNG_SCHEMA);
    reg.register(&exclusive::SLPD_SCHEMA);
    reg.register(&exclusive::MSET_SCHEMA);
    reg.register(&exclusive::ALOC_SCHEMA);
    reg.register(&exclusive::AMEF_SCHEMA);
    reg.register(&exclusive::LSCT_SCHEMA);

    reg
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that the FNV registry contains at least 100 record types
    /// (FO3 base + FNV-exclusive).
    #[test]
    fn fonv_registry_has_expected_record_count(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let reg = registry();
        assert!(
            reg.len() >= 100,
            "expected at least 100 Fallout NV schemas, got {}",
            reg.len()
        );
        Ok(())
    }

    /// Verifies that NPC_ (shared with FO3) is registered in FNV.
    #[test]
    fn fonv_npc_schema_is_registered() -> std::result::Result<(), Box<dyn std::error::Error>> {
        use crate::types::Signature;
        let reg = registry();
        let schema =
            reg.get(Signature(*b"NPC_")).ok_or("NPC_ not registered in FNV")?;
        assert_eq!(schema.name, "Non-Player Character");
        Ok(())
    }

    /// Verifies that CHAL (FNV-exclusive challenge record) is registered.
    #[test]
    fn fonv_chal_schema_is_registered() -> std::result::Result<(), Box<dyn std::error::Error>> {
        use crate::types::Signature;
        let reg = registry();
        reg.get(Signature(*b"CHAL")).ok_or("CHAL not registered in FNV")?;
        Ok(())
    }

    /// Verifies that REPU (FNV-exclusive reputation record) is registered.
    #[test]
    fn fonv_repu_schema_is_registered() -> std::result::Result<(), Box<dyn std::error::Error>> {
        use crate::types::Signature;
        let reg = registry();
        reg.get(Signature(*b"REPU")).ok_or("REPU not registered in FNV")?;
        Ok(())
    }

    /// Verifies that IMOD (FNV-exclusive item mod record) is registered.
    #[test]
    fn fonv_imod_schema_is_registered() -> std::result::Result<(), Box<dyn std::error::Error>> {
        use crate::types::Signature;
        let reg = registry();
        reg.get(Signature(*b"IMOD")).ok_or("IMOD not registered in FNV")?;
        Ok(())
    }
}
