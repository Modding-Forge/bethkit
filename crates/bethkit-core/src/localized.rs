// SPDX-License-Identifier: Apache-2.0
//!
//! Localized-string extraction and replacement for Skyrim SE plugins.
//!
//! When the `LOCALIZED` flag (bit `0x80`) is set on the `TES4` record,
//! certain text-bearing subrecords no longer carry an inline ZString but a
//! `u32` lookup ID into one of three sibling files:
//!
//! * `<Plugin>_<lang>.STRINGS`   — short strings (names, prompts, …)
//! * `<Plugin>_<lang>.DLSTRINGS` — long descriptions (book text, MGEF, …)
//! * `<Plugin>_<lang>.ILSTRINGS` — dialogue / info text
//!
//! This module exposes:
//!
//! * [`localized_subrecords`] — the static, per-record-signature list of
//!   subrecord signatures that contain LString IDs in TES5/SSE.
//! * [`resolve_string_kind`] — routes a `(record_sig, subrecord_sig)` pair
//!   to the correct [`StringFileKind`].
//! * [`LocalizationSet`] — a triplet of in-memory string tables, openable
//!   from sibling files and writable back.
//! * [`LocalizedString`] / [`extract_strings`] — pull every translatable
//!   string out of a parsed plugin.
//!
//! The actual record bytes are *not* modified by this module: translating
//! a plugin only requires rewriting the three `.STRINGS` sibling files,
//! which is a constant-time operation in plugin size.

use std::path::Path;
use std::sync::OnceLock;

use ahash::HashMap;

use crate::error::{CoreError, Result};
use crate::group::{Group, GroupChild};
use crate::plugin::Plugin;
use crate::record::Record;
use crate::strings::{StringFileKind, StringTable};
use crate::types::{FormId, Signature};

/// Returns the static map of `record signature -> &[localized subrecord
/// signatures]` for Skyrim SE / TES5.
///
/// The map is initialised on first call and cached for the process
/// lifetime. Keys absent from the map have no localized subrecords.
fn string_records_table() -> &'static HashMap<Signature, &'static [Signature]> {
    static TABLE: OnceLock<HashMap<Signature, &'static [Signature]>> = OnceLock::new();
    TABLE.get_or_init(|| {
        // Per `res/string_records.json` from SSE-Auto-Translator,
        // cross-checked with TES5Edit `wbDefinitionsTES5.pas` (`cpTranslate`).
        const FULL: Signature = Signature(*b"FULL");
        const DESC: Signature = Signature(*b"DESC");
        const RNAM: Signature = Signature(*b"RNAM");
        const CNAM: Signature = Signature(*b"CNAM");
        const DNAM: Signature = Signature(*b"DNAM");
        const SHRT: Signature = Signature(*b"SHRT");
        const NAM1: Signature = Signature(*b"NAM1");
        const TNAM: Signature = Signature(*b"TNAM");
        const ITXT: Signature = Signature(*b"ITXT");
        const NNAM: Signature = Signature(*b"NNAM");
        const EPF2: Signature = Signature(*b"EPF2");
        const EPFD: Signature = Signature(*b"EPFD");
        const RDMP: Signature = Signature(*b"RDMP");

        let entries: &[(Signature, &'static [Signature])] = &[
            (Signature(*b"ACTI"), &[FULL, RNAM]),
            (Signature(*b"ALCH"), &[FULL]),
            (Signature(*b"AMMO"), &[FULL, DESC]),
            (Signature(*b"APPA"), &[FULL, DESC]),
            (Signature(*b"ARMO"), &[FULL, DESC]),
            (Signature(*b"AVIF"), &[FULL, DESC]),
            (Signature(*b"BOOK"), &[FULL, DESC, CNAM]),
            (Signature(*b"CELL"), &[FULL]),
            (Signature(*b"CONT"), &[FULL]),
            (Signature(*b"DIAL"), &[FULL]),
            (Signature(*b"DOOR"), &[FULL]),
            (Signature(*b"ENCH"), &[FULL]),
            (Signature(*b"EXPL"), &[FULL]),
            (Signature(*b"FLOR"), &[FULL, RNAM]),
            (Signature(*b"FURN"), &[FULL]),
            (Signature(*b"HAZD"), &[FULL]),
            (Signature(*b"INFO"), &[NAM1, RNAM]),
            (Signature(*b"INGR"), &[FULL]),
            (Signature(*b"KEYM"), &[FULL]),
            (Signature(*b"LCTN"), &[FULL]),
            (Signature(*b"LIGH"), &[FULL]),
            (Signature(*b"LSCR"), &[DESC]),
            (Signature(*b"MESG"), &[DESC, FULL, ITXT]),
            (Signature(*b"MGEF"), &[FULL, DNAM]),
            (Signature(*b"MISC"), &[FULL]),
            (Signature(*b"NPC_"), &[FULL, SHRT]),
            (Signature(*b"NOTE"), &[FULL, TNAM]),
            (Signature(*b"PERK"), &[FULL, DESC, EPF2, EPFD]),
            (Signature(*b"PROJ"), &[FULL]),
            (Signature(*b"QUST"), &[FULL, CNAM, NNAM]),
            (Signature(*b"RACE"), &[FULL, DESC]),
            (Signature(*b"REFR"), &[FULL]),
            (Signature(*b"REGN"), &[RDMP]),
            (Signature(*b"SCRL"), &[FULL, DESC]),
            (Signature(*b"SHOU"), &[FULL, DESC]),
            (Signature(*b"SLGM"), &[FULL]),
            (Signature(*b"SPEL"), &[FULL, DESC]),
            (Signature(*b"TACT"), &[FULL]),
            (Signature(*b"TREE"), &[FULL]),
            (Signature(*b"WEAP"), &[DESC, FULL]),
            (Signature(*b"WOOP"), &[FULL, TNAM]),
            (Signature(*b"WRLD"), &[FULL]),
        ];

        let mut map: HashMap<Signature, &'static [Signature]> = HashMap::default();
        for (k, v) in entries {
            map.insert(*k, *v);
        }
        map
    })
}

/// Returns the localized subrecord signatures for a given record signature.
///
/// Returns an empty slice for record types with no translatable subrecords
/// in TES5/SSE.
pub fn localized_subrecords(record_sig: Signature) -> &'static [Signature] {
    string_records_table()
        .get(&record_sig)
        .copied()
        .unwrap_or(&[])
}

/// Routes a `(record, subrecord)` pair to the [`StringFileKind`] used for
/// its lookup.
///
/// Defaults to [`StringFileKind::Strings`]; long descriptions and dialogue
/// text live in `.DLSTRINGS` and `.ILSTRINGS` respectively.
pub fn resolve_string_kind(record_sig: Signature, subrecord_sig: Signature) -> StringFileKind {
    const DESC: Signature = Signature(*b"DESC");
    const CNAM: Signature = Signature(*b"CNAM");
    const DNAM: Signature = Signature(*b"DNAM");
    const ITXT: Signature = Signature(*b"ITXT");
    const NAM1: Signature = Signature(*b"NAM1");
    const RNAM: Signature = Signature(*b"RNAM");
    const BOOK: Signature = Signature(*b"BOOK");
    const QUST: Signature = Signature(*b"QUST");
    const MGEF: Signature = Signature(*b"MGEF");
    const MESG: Signature = Signature(*b"MESG");
    const INFO: Signature = Signature(*b"INFO");

    if subrecord_sig == DESC {
        return StringFileKind::DLStrings;
    }
    match (record_sig, subrecord_sig) {
        (BOOK, CNAM) | (QUST, CNAM) | (MGEF, DNAM) | (MESG, ITXT) => StringFileKind::DLStrings,
        (INFO, NAM1) | (INFO, RNAM) => StringFileKind::ILStrings,
        _ => StringFileKind::Strings,
    }
}

/// A triplet of [`StringTable`]s sharing one language and one plugin stem.
pub struct LocalizationSet {
    /// `.STRINGS` table.
    pub strings: StringTable,

    /// `.DLSTRINGS` table.
    pub dlstrings: StringTable,

    /// `.ILSTRINGS` table.
    pub ilstrings: StringTable,
}

impl LocalizationSet {
    /// Creates an empty set with all three tables initialised.
    pub fn new() -> Self {
        Self {
            strings: StringTable::new(StringFileKind::Strings),
            dlstrings: StringTable::new(StringFileKind::DLStrings),
            ilstrings: StringTable::new(StringFileKind::ILStrings),
        }
    }

    /// Opens the three sibling files next to `plugin_path` for `language`.
    ///
    /// The expected layout is:
    /// `<dir>/Strings/<plugin_stem>_<language>.{STRINGS,DLSTRINGS,ILSTRINGS}`.
    ///
    /// Each file is opened independently; missing files yield an empty
    /// in-memory table for that kind so that partial language packs still
    /// work.
    ///
    /// # Arguments
    ///
    /// * `plugin_path` - Path to the `.esp` / `.esm` / `.esl` file.
    /// * `language`    - Bethesda language tag (e.g. `"english"`).
    ///
    /// # Errors
    ///
    /// Returns [`CoreError`] if any of the existing sibling files cannot be
    /// parsed.
    pub fn open(plugin_path: &Path, language: &str) -> Result<Self> {
        let [s, dl, il] = StringTable::sibling_paths(plugin_path, language);
        let strings = if s.exists() {
            StringTable::open_as(&s, StringFileKind::Strings)?
        } else {
            StringTable::new(StringFileKind::Strings)
        };
        let dlstrings = if dl.exists() {
            StringTable::open_as(&dl, StringFileKind::DLStrings)?
        } else {
            StringTable::new(StringFileKind::DLStrings)
        };
        let ilstrings = if il.exists() {
            StringTable::open_as(&il, StringFileKind::ILStrings)?
        } else {
            StringTable::new(StringFileKind::ILStrings)
        };
        Ok(Self {
            strings,
            dlstrings,
            ilstrings,
        })
    }

    /// Returns a reference to the table for `kind`.
    pub fn table(&self, kind: StringFileKind) -> &StringTable {
        match kind {
            StringFileKind::Strings => &self.strings,
            StringFileKind::DLStrings => &self.dlstrings,
            StringFileKind::ILStrings => &self.ilstrings,
        }
    }

    /// Returns a mutable reference to the table for `kind`.
    pub fn table_mut(&mut self, kind: StringFileKind) -> &mut StringTable {
        match kind {
            StringFileKind::Strings => &mut self.strings,
            StringFileKind::DLStrings => &mut self.dlstrings,
            StringFileKind::ILStrings => &mut self.ilstrings,
        }
    }

    /// Looks up a string ID in the table for `kind`.
    pub fn get(&self, kind: StringFileKind, id: u32) -> Option<&[u8]> {
        self.table(kind).get(id)
    }

    /// Replaces (or inserts) the payload for `id` in the table for `kind`.
    pub fn set(&mut self, kind: StringFileKind, id: u32, payload: Vec<u8>) {
        self.table_mut(kind).insert(id, payload);
    }

    /// Writes the three tables back to the standard sibling paths next to
    /// `plugin_path` for `language`.
    ///
    /// # Errors
    ///
    /// Returns [`std::io::Error`] on I/O failure.
    pub fn write(&self, plugin_path: &Path, language: &str) -> std::io::Result<()> {
        let [s, dl, il] = StringTable::sibling_paths(plugin_path, language);
        if let Some(parent) = s.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut fs = std::fs::File::create(&s)?;
        self.strings.write_to(&mut fs)?;
        let mut fdl = std::fs::File::create(&dl)?;
        self.dlstrings.write_to(&mut fdl)?;
        let mut fil = std::fs::File::create(&il)?;
        self.ilstrings.write_to(&mut fil)?;
        Ok(())
    }
}

impl Default for LocalizationSet {
    fn default() -> Self {
        Self::new()
    }
}

/// One translatable string extracted from a plugin.
#[derive(Clone, Debug)]
pub struct LocalizedString {
    /// FormID of the owning record.
    pub form_id: FormId,

    /// Editor ID of the owning record, if present.
    pub editor_id: Option<String>,

    /// 4-byte signature of the owning record (e.g. `b"NPC_"`).
    pub record_type: Signature,

    /// 4-byte signature of the localized subrecord (e.g. `b"FULL"`).
    pub subrecord_type: Signature,

    /// Zero-based index when the same subrecord type appears multiple times
    /// in one record (most cases: `0`).
    pub index: usize,

    /// String-table file in which the payload is stored.
    pub kind: StringFileKind,

    /// LString lookup ID found in the record.
    pub string_id: u32,

    /// The decoded raw bytes from the string table (without the trailing
    /// `NUL` for `.STRINGS`). May be empty if the ID is not present in the
    /// table.
    pub bytes: Vec<u8>,
}

/// Walks every record (recursing into sub-groups) and invokes `f`.
fn for_each_record(plugin: &Plugin, mut f: impl FnMut(&Record)) {
    fn walk(group: &Group, f: &mut dyn FnMut(&Record)) {
        for child in group.children() {
            match child {
                GroupChild::Record(r) => f(r),
                GroupChild::Group(g) => walk(g, f),
            }
        }
    }
    for g in plugin.groups() {
        walk(g, &mut f);
    }
}

/// Extracts every localized string referenced by `plugin`, resolving each
/// reference against `set`.
///
/// The plugin must have the `LOCALIZED` flag set on its `TES4` header;
/// otherwise this returns [`CoreError::LocalizedFlagWithoutTables`] only if
/// `require_localized` is `true`. If `false`, returns an empty vector.
///
/// Missing string IDs (referenced by the plugin but absent from the
/// matching table) are still emitted as [`LocalizedString`] entries with
/// an empty `bytes` field, so the caller can decide whether to treat them
/// as errors.
///
/// # Arguments
///
/// * `plugin`             - Parsed plugin.
/// * `set`                - Loaded localization tables.
/// * `require_localized`  - When `true`, errors out on plugins missing the
///   `LOCALIZED` flag.
///
/// # Errors
///
/// Returns [`CoreError::UnsupportedGame`] if the game in `plugin.ctx` does
/// not support string-table localisation. Returns [`CoreError`] on subrecord
/// parse failure, or if the plugin is not localised and `require_localized`
/// is `true`.
pub fn extract_strings(
    plugin: &Plugin,
    set: &LocalizationSet,
    require_localized: bool,
) -> Result<Vec<LocalizedString>> {
    if !plugin.ctx.supports_localization() {
        return Err(CoreError::UnsupportedGame(plugin.ctx.game));
    }
    if !plugin.is_localized() {
        if require_localized {
            return Err(CoreError::LocalizedFlagWithoutTables);
        }
        return Ok(Vec::new());
    }

    let mut out: Vec<LocalizedString> = Vec::new();
    let mut error: Option<CoreError> = None;

    for_each_record(plugin, |record| {
        if error.is_some() {
            return;
        }
        let record_sig: Signature = record.header.signature;
        let wanted: &'static [Signature] = localized_subrecords(record_sig);
        if wanted.is_empty() {
            return;
        }

        let editor_id: Option<String> = match record.editor_id() {
            Ok(opt) => opt.map(|s| s.to_owned()),
            Err(_) => None,
        };

        let subrecords: &[crate::record::SubRecord] = match record.subrecords() {
            Ok(srs) => srs,
            Err(e) => {
                error = Some(e);
                return;
            }
        };

        let mut counter: HashMap<Signature, usize> = HashMap::default();
        for sr in subrecords {
            if !wanted.contains(&sr.signature) {
                continue;
            }
            let payload: &[u8] = sr.data.as_bytes();
            if payload.len() < 4 {
                continue;
            }
            let id_bytes: [u8; 4] = match payload[..4].try_into() {
                Ok(b) => b,
                Err(_) => continue,
            };
            let string_id: u32 = u32::from_le_bytes(id_bytes);
            let kind: StringFileKind = resolve_string_kind(record_sig, sr.signature);
            let bytes: Vec<u8> = set
                .get(kind, string_id)
                .map(<[u8]>::to_vec)
                .unwrap_or_default();
            let index: usize = {
                let entry = counter.entry(sr.signature).or_insert(0);
                let i = *entry;
                *entry += 1;
                i
            };
            out.push(LocalizedString {
                form_id: record.header.form_id,
                editor_id: editor_id.clone(),
                record_type: record_sig,
                subrecord_type: sr.signature,
                index,
                kind,
                string_id,
                bytes,
            });
        }
    });

    if let Some(e) = error {
        return Err(e);
    }
    Ok(out)
}

/// Applies a batch of `(string_id, kind, new_bytes)` edits to `set`.
///
/// The plugin records themselves are not modified — the caller can re-write
/// the three string-table files via [`LocalizationSet::write`] and the
/// translation is complete.
///
/// IDs that exist in the table are overwritten; IDs that do not exist are
/// inserted.
pub fn apply_edits(
    set: &mut LocalizationSet,
    edits: impl IntoIterator<Item = (StringFileKind, u32, Vec<u8>)>,
) {
    for (kind, id, bytes) in edits {
        set.set(kind, id, bytes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that the static subrecord table covers the canonical
    /// TES5/SSE record types and routes them correctly.
    #[test]
    fn localized_subrecords_lookup_basic() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let npc: Signature = Signature(*b"NPC_");
        let book: Signature = Signature(*b"BOOK");
        let unknown: Signature = Signature(*b"ZZZZ");

        // when
        let npc_subs = localized_subrecords(npc);
        let book_subs = localized_subrecords(book);
        let unknown_subs = localized_subrecords(unknown);

        // then
        assert!(npc_subs.contains(&Signature(*b"FULL")));
        assert!(npc_subs.contains(&Signature(*b"SHRT")));
        assert!(book_subs.contains(&Signature(*b"DESC")));
        assert!(book_subs.contains(&Signature(*b"CNAM")));
        assert!(unknown_subs.is_empty());
        Ok(())
    }

    /// Verifies file-kind routing for the most common cases.
    #[test]
    fn resolve_string_kind_routes_correctly() -> std::result::Result<(), Box<dyn std::error::Error>>
    {
        // given
        let npc: Signature = Signature(*b"NPC_");
        let book: Signature = Signature(*b"BOOK");
        let info: Signature = Signature(*b"INFO");
        let mgef: Signature = Signature(*b"MGEF");
        let full: Signature = Signature(*b"FULL");
        let desc: Signature = Signature(*b"DESC");
        let cnam: Signature = Signature(*b"CNAM");
        let dnam: Signature = Signature(*b"DNAM");
        let nam1: Signature = Signature(*b"NAM1");

        // when / then
        assert_eq!(resolve_string_kind(npc, full), StringFileKind::Strings);
        assert_eq!(resolve_string_kind(npc, desc), StringFileKind::DLStrings);
        assert_eq!(resolve_string_kind(book, cnam), StringFileKind::DLStrings);
        assert_eq!(resolve_string_kind(mgef, dnam), StringFileKind::DLStrings);
        assert_eq!(resolve_string_kind(info, nam1), StringFileKind::ILStrings);
        Ok(())
    }

    /// Verifies that an empty `LocalizationSet` round-trips through
    /// `write_to` for all three kinds.
    #[test]
    fn empty_set_roundtrips() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let set = LocalizationSet::new();

        // when
        let mut buf: Vec<u8> = Vec::new();
        set.strings.write_to(&mut buf)?;
        let parsed = StringTable::from_bytes(&buf, StringFileKind::Strings)?;

        // then
        assert_eq!(parsed.len(), 0);
        Ok(())
    }
}
