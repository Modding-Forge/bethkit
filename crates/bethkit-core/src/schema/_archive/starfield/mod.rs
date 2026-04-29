// SPDX-License-Identifier: Apache-2.0
//! Starfield schema registry.
//!
//! Exposes a singleton [`SchemaRegistry`] pre-populated with all Starfield
//! record schemas. Obtain it via [`crate::schema::SchemaRegistry::starfield`].

use std::sync::OnceLock;

use crate::schema::SchemaRegistry;

mod common;
pub mod enums;
mod actors;
mod items;
mod magic;
mod quests;
mod simple;
mod space;
mod world;

/// Returns the global Starfield schema registry, building it on first call.
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
    reg.register(&simple::NOTE_SCHEMA);
    reg.register(&simple::PLYR_SCHEMA);
    reg.register(&simple::INNR_SCHEMA);
    reg.register(&simple::AMDL_SCHEMA);
    reg.register(&simple::ZOOM_SCHEMA);
    reg.register(&simple::AORU_SCHEMA);
    reg.register(&simple::TRNS_SCHEMA);
    reg.register(&simple::TRAV_SCHEMA);
    reg.register(&simple::CNDF_SCHEMA);
    reg.register(&simple::GCVR_SCHEMA);
    reg.register(&simple::OVIS_SCHEMA);
    reg.register(&simple::RFGP_SCHEMA);
    reg.register(&simple::STAG_SCHEMA);
    reg.register(&simple::KSSM_SCHEMA);
    reg.register(&simple::NOCM_SCHEMA);
    reg.register(&simple::DFOB_SCHEMA);
    reg.register(&simple::PKIN_SCHEMA);
    reg.register(&simple::SCOL_SCHEMA);
    reg.register(&simple::LVSC_SCHEMA);
    reg.register(&simple::GBFT_SCHEMA);
    reg.register(&simple::GBFM_SCHEMA);
    reg.register(&simple::LVLB_SCHEMA);
    reg.register(&simple::PMFT_SCHEMA);
    reg.register(&simple::AFFE_SCHEMA);
    reg.register(&simple::CURV_SCHEMA);
    reg.register(&simple::CUR3_SCHEMA);
    reg.register(&simple::SECH_SCHEMA);
    reg.register(&simple::FXPD_SCHEMA);
    reg.register(&simple::ASPC_SCHEMA);
    reg.register(&simple::REVB_SCHEMA);
    reg.register(&simple::MUSC_SCHEMA);
    reg.register(&simple::MUST_SCHEMA);
    reg.register(&simple::IMGS_SCHEMA);
    reg.register(&simple::IMAD_SCHEMA);
    reg.register(&simple::CLFM_SCHEMA);
    reg.register(&simple::DUAL_SCHEMA);
    reg.register(&simple::FSTP_SCHEMA);
    reg.register(&simple::FSTS_SCHEMA);
    reg.register(&simple::NAVI_SCHEMA);

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
    reg.register(&actors::LVLP_SCHEMA);
    reg.register(&actors::BPTD_SCHEMA);
    reg.register(&actors::OTFT_SCHEMA);
    reg.register(&actors::EYES_SCHEMA);
    reg.register(&actors::CLAS_SCHEMA);
    reg.register(&actors::PERK_SCHEMA);

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
    reg.register(&items::OMOD_SCHEMA);
    reg.register(&items::SCRL_SCHEMA);
    reg.register(&items::LGDI_SCHEMA);
    reg.register(&items::IRES_SCHEMA);
    reg.register(&items::TERM_SCHEMA);
    reg.register(&items::BNDS_SCHEMA);
    reg.register(&items::PDCL_SCHEMA);
    reg.register(&items::CMPO_SCHEMA);
    reg.register(&items::COBJ_SCHEMA);

    // ── Magic / combat records ────────────────────────────────────────────────
    reg.register(&magic::SPEL_SCHEMA);
    reg.register(&magic::MGEF_SCHEMA);
    reg.register(&magic::ENCH_SCHEMA);
    reg.register(&magic::DMGT_SCHEMA);
    reg.register(&magic::SDLT_SCHEMA);
    reg.register(&magic::PROJ_SCHEMA);
    reg.register(&magic::EXPL_SCHEMA);
    reg.register(&magic::HAZD_SCHEMA);
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
    reg.register(&world::FLOR_SCHEMA);
    reg.register(&world::CELL_SCHEMA);
    reg.register(&world::WRLD_SCHEMA);
    reg.register(&world::REGN_SCHEMA);
    reg.register(&world::NAVM_SCHEMA);
    reg.register(&world::WATR_SCHEMA);
    reg.register(&world::LSCR_SCHEMA);
    reg.register(&world::LTEX_SCHEMA);
    reg.register(&world::SCEN_SCHEMA);
    reg.register(&world::OSWP_SCHEMA);
    reg.register(&world::LMSW_SCHEMA);
    reg.register(&world::EFSH_SCHEMA);
    reg.register(&world::VOLI_SCHEMA);
    reg.register(&world::ADDN_SCHEMA);
    reg.register(&world::ARTO_SCHEMA);

    // ── Quest / dialogue records ──────────────────────────────────────────────
    reg.register(&quests::QUST_SCHEMA);
    reg.register(&quests::DIAL_SCHEMA);
    reg.register(&quests::INFO_SCHEMA);
    reg.register(&quests::DLBR_SCHEMA);
    reg.register(&quests::DLVW_SCHEMA);
    reg.register(&quests::SPCH_SCHEMA);
    reg.register(&quests::GPOF_SCHEMA);
    reg.register(&quests::GPOG_SCHEMA);
    reg.register(&quests::TMLM_SCHEMA);
    reg.register(&quests::SOUN_SCHEMA);

    // ── Starfield-exclusive space / planet records ────────────────────────────
    reg.register(&space::PNDT_SCHEMA);
    reg.register(&space::STDT_SCHEMA);
    reg.register(&space::BIOM_SCHEMA);
    reg.register(&space::SUNP_SCHEMA);
    reg.register(&space::ATMO_SCHEMA);
    reg.register(&space::SFBK_SCHEMA);
    reg.register(&space::SFPT_SCHEMA);
    reg.register(&space::SFPC_SCHEMA);
    reg.register(&space::SFTR_SCHEMA);
    reg.register(&space::PTST_SCHEMA);
    reg.register(&space::RSGD_SCHEMA);
    reg.register(&space::RSPJ_SCHEMA);
    reg.register(&space::PCMT_SCHEMA);
    reg.register(&space::PCBN_SCHEMA);
    reg.register(&space::PCCN_SCHEMA);
    reg.register(&space::AMBS_SCHEMA);
    reg.register(&space::CLDF_SCHEMA);
    reg.register(&space::TODD_SCHEMA);
    reg.register(&space::BMOD_SCHEMA);
    reg.register(&space::MRPH_SCHEMA);
    reg.register(&space::STMP_SCHEMA);
    reg.register(&space::STND_SCHEMA);
    reg.register(&space::STBH_SCHEMA);
    reg.register(&space::AOPS_SCHEMA);
    reg.register(&space::AAMD_SCHEMA);
    reg.register(&space::MAAM_SCHEMA);
    reg.register(&space::BMMO_SCHEMA);

    reg
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that the Starfield registry contains at least 100 record types.
    #[test]
    fn starfield_registry_has_expected_record_count(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let reg = registry();
        assert!(
            reg.len() >= 100,
            "expected at least 100 Starfield schemas, got {}",
            reg.len()
        );
        Ok(())
    }

    /// Verifies that NPC_ is registered in the Starfield registry.
    #[test]
    fn starfield_npc_schema_is_registered() -> std::result::Result<(), Box<dyn std::error::Error>>
    {
        use crate::types::Signature;
        let reg = registry();
        let schema =
            reg.get(Signature(*b"NPC_")).ok_or("NPC_ not registered in Starfield")?;
        assert_eq!(schema.name, "Non-Player Character");
        Ok(())
    }

    /// Verifies that PNDT (Starfield-exclusive planet record) is registered.
    #[test]
    fn starfield_pndt_schema_is_registered() -> std::result::Result<(), Box<dyn std::error::Error>>
    {
        use crate::types::Signature;
        let reg = registry();
        reg.get(Signature(*b"PNDT")).ok_or("PNDT not registered in Starfield")?;
        Ok(())
    }

    /// Verifies that SFBK (surface block, Starfield-exclusive) is registered.
    #[test]
    fn starfield_sfbk_schema_is_registered() -> std::result::Result<(), Box<dyn std::error::Error>>
    {
        use crate::types::Signature;
        let reg = registry();
        reg.get(Signature(*b"SFBK")).ok_or("SFBK not registered in Starfield")?;
        Ok(())
    }

    /// Verifies that RSPJ (research project, Starfield-exclusive) is registered.
    #[test]
    fn starfield_rspj_schema_is_registered() -> std::result::Result<(), Box<dyn std::error::Error>>
    {
        use crate::types::Signature;
        let reg = registry();
        reg.get(Signature(*b"RSPJ")).ok_or("RSPJ not registered in Starfield")?;
        Ok(())
    }
}
