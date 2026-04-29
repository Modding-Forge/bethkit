// SPDX-License-Identifier: Apache-2.0
//!
//! Schema definitions for projectile-variant SSE record types.
//!
//! These are the placed-reference equivalents of the PROJ record and share
//! identical structure: EDID + OBND + DATA (ByteArray).
//!
//! Covers: PARW, PBAR, PBEA, PCON, PFLA, PGRE, PHZD, PMIS.

use crate::schema::{FieldType, RecordSchema, SubRecordDef};
use crate::types::Signature;

use super::common::{EDID_DEF, OBND_DEF};

/// Minimal members shared by all placed projectile types.
static PLACED_PROJ_MEMBERS: [SubRecordDef; 3] = [
    EDID_DEF,
    OBND_DEF,
    SubRecordDef {
        sig: Signature(*b"DATA"),
        name: "Data",
        required: false,
        repeating: false,
        field: FieldType::ByteArray,
    },
];

/// PARW — Placed arrow projectile.
pub static PARW_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"PARW"),
    name: "Arrow Projectile",
    members: &PLACED_PROJ_MEMBERS,
};

/// PBAR — Placed barrier projectile.
pub static PBAR_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"PBAR"),
    name: "Barrier Projectile",
    members: &PLACED_PROJ_MEMBERS,
};

/// PBEA — Placed beam projectile.
pub static PBEA_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"PBEA"),
    name: "Beam Projectile",
    members: &PLACED_PROJ_MEMBERS,
};

/// PCON — Placed cone / voice projectile.
pub static PCON_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"PCON"),
    name: "Cone / Voice Projectile",
    members: &PLACED_PROJ_MEMBERS,
};

/// PFLA — Placed flame projectile.
pub static PFLA_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"PFLA"),
    name: "Flame Projectile",
    members: &PLACED_PROJ_MEMBERS,
};

/// PGRE — Placed grenade projectile.
pub static PGRE_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"PGRE"),
    name: "Grenade Projectile",
    members: &PLACED_PROJ_MEMBERS,
};

/// PHZD — Placed hazard.
pub static PHZD_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"PHZD"),
    name: "Placed Hazard",
    members: &PLACED_PROJ_MEMBERS,
};

/// PMIS — Placed missile projectile.
pub static PMIS_SCHEMA: RecordSchema = RecordSchema {
    sig: Signature(*b"PMIS"),
    name: "Missile Projectile",
    members: &PLACED_PROJ_MEMBERS,
};
