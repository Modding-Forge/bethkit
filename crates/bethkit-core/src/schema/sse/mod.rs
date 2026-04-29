// SPDX-License-Identifier: Apache-2.0
//!
//! SSE schema sub-modules and registry singleton.
//!
//! All record schemas defined across the sub-modules are collected here and
//! registered into a [`SchemaRegistry`] on first access via [`registry()`].

mod actors;
mod audio;
mod items;
mod magic;
mod projectiles;
mod quests;
mod simple;
mod world;

pub mod common;

use std::sync::OnceLock;

use crate::schema::SchemaRegistry;

/// Returns the global SSE [`SchemaRegistry`], building it on first call.
///
/// Uses [`OnceLock`] so construction happens at most once and is safe for
/// concurrent callers.
pub(super) fn registry() -> &'static SchemaRegistry {
    static INSTANCE: OnceLock<SchemaRegistry> = OnceLock::new();
    INSTANCE.get_or_init(build)
}

/// Constructs the SSE [`SchemaRegistry`] by registering all known record
/// schemas.
fn build() -> SchemaRegistry {
    let mut reg = SchemaRegistry::new();

    // ── simple records ────────────────────────────────────────────────────────
    reg.register(&simple::TES4_SCHEMA);
    reg.register(&simple::KYWD_SCHEMA);
    reg.register(&simple::AACT_SCHEMA);
    reg.register(&simple::TXST_SCHEMA);
    reg.register(&simple::GLOB_SCHEMA);
    reg.register(&simple::GMST_SCHEMA);
    reg.register(&simple::VTYP_SCHEMA);
    reg.register(&simple::LCRT_SCHEMA);
    reg.register(&simple::MATT_SCHEMA);
    reg.register(&simple::COLL_SCHEMA);
    reg.register(&simple::CLFM_SCHEMA);
    reg.register(&simple::REVB_SCHEMA);
    reg.register(&simple::SHOU_SCHEMA);
    reg.register(&simple::WOOP_SCHEMA);
    reg.register(&simple::ASTP_SCHEMA);
    reg.register(&simple::EQUP_SCHEMA);
    reg.register(&simple::RELA_SCHEMA);
    reg.register(&simple::DEBR_SCHEMA);
    reg.register(&simple::LGTM_SCHEMA);
    reg.register(&simple::DOBJ_SCHEMA);
    reg.register(&simple::FLST_SCHEMA);
    reg.register(&simple::IDLM_SCHEMA);
    reg.register(&simple::ANIO_SCHEMA);
    reg.register(&simple::HDPT_SCHEMA);
    reg.register(&simple::LCTN_SCHEMA);
    reg.register(&simple::MESG_SCHEMA);
    reg.register(&simple::AVIF_SCHEMA);
    reg.register(&simple::CAMS_SCHEMA);
    reg.register(&simple::CPTH_SCHEMA);
    reg.register(&simple::MOVT_SCHEMA);
    reg.register(&simple::DUAL_SCHEMA);
    reg.register(&simple::PLYR_SCHEMA);

    // ── items ─────────────────────────────────────────────────────────────────
    reg.register(&items::WEAP_SCHEMA);
    reg.register(&items::ARMO_SCHEMA);
    reg.register(&items::ARMA_SCHEMA);
    reg.register(&items::AMMO_SCHEMA);
    reg.register(&items::BOOK_SCHEMA);
    reg.register(&items::ALCH_SCHEMA);
    reg.register(&items::INGR_SCHEMA);
    reg.register(&items::MISC_SCHEMA);
    reg.register(&items::KEYM_SCHEMA);
    reg.register(&items::SLGM_SCHEMA);
    reg.register(&items::APPA_SCHEMA);
    reg.register(&items::COBJ_SCHEMA);
    reg.register(&items::CONT_SCHEMA);
    reg.register(&items::DOOR_SCHEMA);
    reg.register(&items::FURN_SCHEMA);

    // ── actors ────────────────────────────────────────────────────────────────
    reg.register(&actors::NPC_SCHEMA);
    reg.register(&actors::RACE_SCHEMA);
    reg.register(&actors::PACK_SCHEMA);
    reg.register(&actors::FACT_SCHEMA);
    reg.register(&actors::CSTY_SCHEMA);
    reg.register(&actors::IDLE_SCHEMA);
    reg.register(&actors::CLAS_SCHEMA);
    reg.register(&actors::EYES_SCHEMA);
    reg.register(&actors::OTFT_SCHEMA);
    reg.register(&actors::BPTD_SCHEMA);
    reg.register(&actors::LVLN_SCHEMA);
    reg.register(&actors::LVLI_SCHEMA);
    reg.register(&actors::LVSP_SCHEMA);

    // ── magic & effects ───────────────────────────────────────────────────────
    reg.register(&magic::SPEL_SCHEMA);
    reg.register(&magic::SCRL_SCHEMA);
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

    // ── world & environment ───────────────────────────────────────────────────
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
    reg.register(&world::VOLI_SCHEMA);

    // ── quests & dialogue ─────────────────────────────────────────────────────
    reg.register(&quests::QUST_SCHEMA);
    reg.register(&quests::DIAL_SCHEMA);
    reg.register(&quests::INFO_SCHEMA);
    reg.register(&quests::DLBR_SCHEMA);
    reg.register(&quests::DLVW_SCHEMA);
    reg.register(&quests::SCEN_SCHEMA);
    reg.register(&quests::SMBN_SCHEMA);
    reg.register(&quests::SMQN_SCHEMA);
    reg.register(&quests::SMEN_SCHEMA);

    // ── audio & sound ─────────────────────────────────────────────────────────
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

    // ── placed projectiles ────────────────────────────────────────────────────
    reg.register(&projectiles::PARW_SCHEMA);
    reg.register(&projectiles::PBAR_SCHEMA);
    reg.register(&projectiles::PBEA_SCHEMA);
    reg.register(&projectiles::PCON_SCHEMA);
    reg.register(&projectiles::PFLA_SCHEMA);
    reg.register(&projectiles::PGRE_SCHEMA);
    reg.register(&projectiles::PHZD_SCHEMA);
    reg.register(&projectiles::PMIS_SCHEMA);

    reg
}
