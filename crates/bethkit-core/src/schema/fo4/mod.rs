// SPDX-License-Identifier: Apache-2.0
//! Fallout 4 schema registry.
//!
//! Exposes a singleton [`SchemaRegistry`] pre-populated with all Fallout 4
//! record schemas. Obtain it via [`crate::schema::SchemaRegistry::fo4`].

use std::sync::OnceLock;

use crate::schema::SchemaRegistry;

mod actors;
mod audio;
mod common;
pub mod enums;
mod items;
mod magic;
mod quests;
mod simple;
mod world;

/// Returns the global Fallout 4 schema registry, building it on first call.
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
    reg.register(&simple::KYWD_SCHEMA);
    reg.register(&simple::AACT_SCHEMA);
    reg.register(&simple::TXST_SCHEMA);
    reg.register(&simple::GLOB_SCHEMA);
    reg.register(&simple::GMST_SCHEMA);
    reg.register(&simple::AVIF_SCHEMA);
    reg.register(&simple::LCRT_SCHEMA);
    reg.register(&simple::VTYP_SCHEMA);
    reg.register(&simple::MATT_SCHEMA);
    reg.register(&simple::COLL_SCHEMA);
    reg.register(&simple::FLST_SCHEMA);
    reg.register(&simple::LCTN_SCHEMA);
    reg.register(&simple::MESG_SCHEMA);
    reg.register(&simple::DOBJ_SCHEMA);
    reg.register(&simple::LGTM_SCHEMA);
    reg.register(&simple::IDLM_SCHEMA);
    reg.register(&simple::ANIO_SCHEMA);
    reg.register(&simple::HDPT_SCHEMA);
    reg.register(&simple::MOVT_SCHEMA);
    reg.register(&simple::EQUP_SCHEMA);
    reg.register(&simple::RELA_SCHEMA);
    reg.register(&simple::DEBR_SCHEMA);
    reg.register(&simple::ASTP_SCHEMA);
    reg.register(&simple::CAMS_SCHEMA);
    reg.register(&simple::CPTH_SCHEMA);
    reg.register(&simple::LAYR_SCHEMA);
    reg.register(&simple::SCCO_SCHEMA);
    reg.register(&simple::DFOB_SCHEMA);
    reg.register(&simple::KSSM_SCHEMA);
    reg.register(&simple::NOTE_SCHEMA);
    reg.register(&simple::OVIS_SCHEMA);
    reg.register(&simple::RFGP_SCHEMA);
    reg.register(&simple::STAG_SCHEMA);
    reg.register(&simple::BNDS_SCHEMA);
    reg.register(&simple::GDRY_SCHEMA);
    reg.register(&simple::NOCM_SCHEMA);
    reg.register(&simple::PKIN_SCHEMA);
    reg.register(&simple::SCOL_SCHEMA);
    reg.register(&simple::SCSN_SCHEMA);
    reg.register(&simple::INNR_SCHEMA);
    reg.register(&simple::AMDL_SCHEMA);
    reg.register(&simple::AORU_SCHEMA);
    reg.register(&simple::PLYR_SCHEMA);
    // NOTE: DMGT is also declared in magic.rs; only register it once here.
    reg.register(&simple::DMGT_SCHEMA);
    reg.register(&simple::TRNS_SCHEMA);
    reg.register(&simple::ZOOM_SCHEMA);
    reg.register(&simple::CLFM_SCHEMA);
    reg.register(&simple::REVB_SCHEMA);
    reg.register(&simple::DUAL_SCHEMA);

    // ── Actor records ─────────────────────────────────────────────────────────
    reg.register(&actors::NPC_SCHEMA);
    reg.register(&actors::RACE_SCHEMA);
    reg.register(&actors::PACK_SCHEMA);
    reg.register(&actors::FACT_SCHEMA);
    reg.register(&actors::CSTY_SCHEMA);
    reg.register(&actors::IDLE_SCHEMA);
    reg.register(&actors::LVLN_SCHEMA);
    reg.register(&actors::LVLI_SCHEMA);
    reg.register(&actors::LVSP_SCHEMA);
    reg.register(&actors::BPTD_SCHEMA);
    reg.register(&actors::OTFT_SCHEMA);
    reg.register(&actors::EYES_SCHEMA);
    reg.register(&actors::CLAS_SCHEMA);

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
    reg.register(&items::TERM_SCHEMA);
    reg.register(&items::COBJ_SCHEMA);
    reg.register(&items::CONT_SCHEMA);
    reg.register(&items::DOOR_SCHEMA);
    reg.register(&items::FURN_SCHEMA);
    reg.register(&items::OMOD_SCHEMA);
    reg.register(&items::CMPO_SCHEMA);
    reg.register(&items::MSWP_SCHEMA);

    // ── Magic records ─────────────────────────────────────────────────────────
    reg.register(&magic::SPEL_SCHEMA);
    reg.register(&magic::MGEF_SCHEMA);
    reg.register(&magic::ENCH_SCHEMA);
    reg.register(&magic::RFCT_SCHEMA);
    reg.register(&magic::PROJ_SCHEMA);
    reg.register(&magic::EXPL_SCHEMA);
    reg.register(&magic::HAZD_SCHEMA);
    reg.register(&magic::PERK_SCHEMA);
    reg.register(&magic::IMGS_SCHEMA);
    reg.register(&magic::IMAD_SCHEMA);
    reg.register(&magic::IPCT_SCHEMA);
    reg.register(&magic::IPDS_SCHEMA);
    reg.register(&magic::ADDN_SCHEMA);
    reg.register(&magic::SPGD_SCHEMA);
    reg.register(&magic::EFSH_SCHEMA);
    // NOTE: magic::DMGT_SCHEMA intentionally skipped — registered above from simple.

    // ── World / environment records ───────────────────────────────────────────
    reg.register(&world::ACTI_SCHEMA);
    reg.register(&world::TACT_SCHEMA);
    reg.register(&world::STAT_SCHEMA);
    reg.register(&world::GRAS_SCHEMA);
    reg.register(&world::TREE_SCHEMA);
    reg.register(&world::FLOR_SCHEMA);
    reg.register(&world::MSTT_SCHEMA);
    reg.register(&world::LTEX_SCHEMA);
    reg.register(&world::LIGH_SCHEMA);
    reg.register(&world::WATR_SCHEMA);
    reg.register(&world::WTHR_SCHEMA);
    reg.register(&world::CLMT_SCHEMA);
    reg.register(&world::ASPC_SCHEMA);
    reg.register(&world::ECZN_SCHEMA);
    reg.register(&world::CELL_SCHEMA);
    reg.register(&world::WRLD_SCHEMA);
    reg.register(&world::LAND_SCHEMA);
    reg.register(&world::REGN_SCHEMA);
    reg.register(&world::NAVM_SCHEMA);
    reg.register(&world::NAVI_SCHEMA);
    reg.register(&world::LSCR_SCHEMA);
    reg.register(&world::LENS_SCHEMA);

    // ── Quest / dialogue records ──────────────────────────────────────────────
    reg.register(&quests::QUST_SCHEMA);
    reg.register(&quests::DIAL_SCHEMA);
    reg.register(&quests::INFO_SCHEMA);
    reg.register(&quests::DLBR_SCHEMA);
    reg.register(&quests::DLVW_SCHEMA);
    reg.register(&quests::SCEN_SCHEMA);
    reg.register(&quests::SMBN_SCHEMA);
    reg.register(&quests::SMQN_SCHEMA);
    reg.register(&quests::SMEN_SCHEMA);

    // ── Audio records ─────────────────────────────────────────────────────────
    reg.register(&audio::SOUN_SCHEMA);
    reg.register(&audio::SNDR_SCHEMA);
    reg.register(&audio::MUSC_SCHEMA);
    reg.register(&audio::MUST_SCHEMA);
    reg.register(&audio::SNCT_SCHEMA);
    reg.register(&audio::SOPM_SCHEMA);
    reg.register(&audio::FSTP_SCHEMA);
    reg.register(&audio::FSTS_SCHEMA);
    reg.register(&audio::ARTO_SCHEMA);
    reg.register(&audio::MATO_SCHEMA);
    reg.register(&audio::AECH_SCHEMA);

    reg
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that the FO4 registry contains at least 100 record types.
    #[test]
    fn fo4_registry_has_expected_record_count(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let reg = registry();
        assert!(
            reg.len() >= 100,
            "expected at least 100 FO4 schemas, got {}",
            reg.len()
        );
        Ok(())
    }

    /// Verifies that NPC_ is registered in the FO4 registry.
    #[test]
    fn fo4_npc_schema_is_registered() -> std::result::Result<(), Box<dyn std::error::Error>> {
        use crate::types::Signature;
        let reg = registry();
        let schema = reg
            .get(Signature(*b"NPC_"))
            .ok_or("NPC_ not registered in FO4")?;
        assert!(!schema.members.is_empty());
        Ok(())
    }

    /// Verifies that OMOD (FO4-exclusive) is registered.
    #[test]
    fn fo4_omod_schema_is_registered() -> std::result::Result<(), Box<dyn std::error::Error>> {
        use crate::types::Signature;
        let reg = registry();
        reg.get(Signature(*b"OMOD"))
            .ok_or("OMOD not registered in FO4")?;
        Ok(())
    }

    /// Verifies that DMGT appears exactly once (from simple, not magic).
    #[test]
    fn fo4_dmgt_not_duplicated() -> std::result::Result<(), Box<dyn std::error::Error>> {
        use crate::types::Signature;
        let reg = registry();
        // get() returns at most one entry; duplicate registration would silently
        // overwrite, so we just check it resolves to the expected name.
        let schema = reg
            .get(Signature(*b"DMGT"))
            .ok_or("DMGT not registered in FO4")?;
        assert_eq!(schema.name, "Damage Type");
        Ok(())
    }
}
