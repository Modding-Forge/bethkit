// SPDX-License-Identifier: Apache-2.0
//!
//! Cross-plugin record cache with winning-override semantics.
//!
//! [`PluginCache`] indexes records from multiple plugins by their
//! [`GlobalFormId`]. When two plugins define the same record, the
//! later-added plugin's version "wins", matching Bethesda's load-order
//! override behaviour.

use std::sync::OnceLock;

use ahash::HashMap;

use crate::load_order::{GlobalFormId, LoadOrder};
use crate::plugin::Plugin;
use crate::record::Record;
use crate::types::{FormId, PluginKind, Signature};

/// A single plugin entry held inside a [`PluginCache`].
pub struct CacheEntry {
    /// Canonical plugin filename (lowercased, includes extension).
    pub name: String,
    /// The fully parsed plugin.
    pub plugin: Plugin,
}

/// Multi-plugin record cache with winning-override semantics.
///
/// Records are indexed by [`GlobalFormId`]. When two plugins define the same
/// global FormID, the plugin added later wins — matching how Bethesda's engine
/// resolves load-order overrides.
///
/// The EditorID index ([`PluginCache::find_by_editor_id`]) is built lazily on
/// first use and invalidated whenever a new plugin is added.
///
/// # Example
///
/// ```rust,no_run
/// use bethkit_core::{GameContext, Plugin, PluginCache};
///
/// let ctx = GameContext::sse();
/// let mut cache = PluginCache::new();
///
/// let skyrim = Plugin::open("Skyrim.esm".as_ref(), ctx)?;
/// cache.add("Skyrim.esm", skyrim)?;
///
/// let my_mod = Plugin::open("MyMod.esp".as_ref(), ctx)?;
/// cache.add("MyMod.esp", my_mod)?;
///
/// if let Some((_gfid, record)) = cache.find_by_editor_id("ManaPotion") {
///     println!("{}", record.header.form_id);
/// }
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub struct PluginCache {
    entries: Vec<CacheEntry>,
    load_order: LoadOrder,
    /// Maps each indexed [`GlobalFormId`] to `(entry_index, raw_form_id)`.
    /// Later entries overwrite earlier ones — winning-override semantics.
    by_global_id: HashMap<GlobalFormId, (usize, FormId)>,
    /// Maps each 4-byte record signature to the list of [`GlobalFormId`]s
    /// for that record type in the winning-override set. Built incrementally
    /// in [`Self::add`].
    by_signature: HashMap<Signature, Vec<GlobalFormId>>,
    /// Lazily built EditorID → [`GlobalFormId`] index.
    /// Replaced with a fresh [`OnceLock`] whenever a plugin is added.
    by_editor_id: OnceLock<HashMap<String, GlobalFormId>>,
}

impl PluginCache {
    /// Creates an empty cache.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            load_order: LoadOrder::new(),
            by_global_id: HashMap::default(),
            by_signature: HashMap::default(),
            by_editor_id: OnceLock::new(),
        }
    }

    /// Returns a reference to the underlying [`LoadOrder`].
    pub fn load_order(&self) -> &LoadOrder {
        &self.load_order
    }

    /// Returns all plugin entries in load-order (earliest first).
    pub fn entries(&self) -> &[CacheEntry] {
        &self.entries
    }

    /// Returns the number of plugins in the cache.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` if no plugins have been added.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns the number of winning-override records currently indexed.
    pub fn record_count(&self) -> usize {
        self.by_global_id.len()
    }

    /// Adds a plugin to the cache.
    ///
    /// The plugin is appended to the load order. All of its records are
    /// indexed; if a record shares a [`GlobalFormId`] with a previously added
    /// plugin, the new plugin's version becomes the winning override.
    ///
    /// Any previously built EditorID index is invalidated and will be rebuilt
    /// on the next call to [`Self::find_by_editor_id`].
    ///
    /// # Arguments
    ///
    /// * `name`   - Plugin filename including extension (e.g. `"MyMod.esp"`).
    /// * `plugin` - The fully parsed plugin to add.
    /// # Errors
    ///
    /// Returns [`CoreError::LoadOrderIndexFull`] or
    /// [`CoreError::LightSlotOverflow`] if the load-order index is exhausted.
    pub fn add(&mut self, name: &str, plugin: Plugin) -> crate::error::Result<()> {
        let kind: PluginKind = plugin.kind();
        let masters: Vec<String> = plugin.masters().to_vec();
        let canonical: String = name.to_lowercase();
        let entry_index: usize = self.entries.len();

        // Register the plugin in the load order FIRST so that the ESL slot is
        // available when resolving this plugin's own ESL-encoded FormIDs.
        self.load_order.push(name, kind)?;

        // Collect (GlobalFormId, raw FormId, Signature) triples for all
        // non-null records before moving the plugin into entries.
        let triples: Vec<(GlobalFormId, FormId, Signature)> = plugin
            .groups()
            .iter()
            .flat_map(|g| g.records_recursive())
            .filter(|r| !r.header.form_id.is_null())
            .filter_map(|r| {
                self.load_order
                    .resolve(r.header.form_id, &canonical, &masters)
                    .map(|gfid| (gfid, r.header.form_id, r.header.signature))
            })
            .collect();

        // Move the plugin into the entries vec.
        self.entries.push(CacheEntry { name: canonical, plugin });

        // Update the winning-override index. Later entries overwrite earlier.
        for (gfid, raw_fid, sig) in triples {
            // Remove stale signature index entry for the previous winner.
            if let Some(&(_, prev_fid)) = self.by_global_id.get(&gfid) {
                if let Some(sig_list) = self.by_signature.get_mut(&sig) {
                    // NOTE: This is O(k) where k = records of that type.
                    // For large plugins this may be slow, but it only fires
                    // when the same GlobalFormId is overridden by a later
                    // plugin, which is the minority case.
                    let _ = prev_fid; // suppress unused warning
                    sig_list.retain(|g| g != &gfid);
                }
            }

            self.by_signature.entry(sig).or_default().push(gfid.clone());
            self.by_global_id.insert(gfid, (entry_index, raw_fid));
        }

        // Invalidate the EditorID cache so it is rebuilt on next access.
        self.by_editor_id = OnceLock::new();
        Ok(())
    }

    /// Returns the winning-override [`Record`] for `gfid`, or `None` if the
    /// FormID is not present in the cache.
    ///
    /// # Arguments
    ///
    /// * `gfid` - The game-unique FormID to look up.
    pub fn resolve_record(&self, gfid: &GlobalFormId) -> Option<&Record> {
        let &(entry_idx, raw_fid) = self.by_global_id.get(gfid)?;
        self.entries[entry_idx].plugin.find_record(raw_fid)
    }

    /// Looks up the winning-override record with the given EditorID.
    ///
    /// The EditorID index is built lazily on first call and cached until
    /// a new plugin is added via [`Self::add`].
    ///
    /// Returns `None` if no indexed record has that EditorID.
    ///
    /// # Arguments
    ///
    /// * `edid` - The EditorID to find (case-sensitive).
    pub fn find_by_editor_id(&self, edid: &str) -> Option<(&GlobalFormId, &Record)> {
        let by_global_id: &HashMap<GlobalFormId, (usize, FormId)> = &self.by_global_id;
        let entries: &[CacheEntry] = &self.entries;

        let index: &HashMap<String, GlobalFormId> = self.by_editor_id.get_or_init(|| {
            // Group winning-override entries by entry index so we can do a single
            // O(n) pass per plugin: for each plugin we build a set of its winning
            // raw FormIDs, then iterate its records once, looking up EDID only for
            // records that are in the winning set.
            let mut winning_by_entry: HashMap<usize, HashMap<FormId, GlobalFormId>> =
                HashMap::default();
            for (gfid, &(entry_idx, raw_fid)) in by_global_id {
                winning_by_entry
                    .entry(entry_idx)
                    .or_default()
                    .insert(raw_fid, gfid.clone());
            }

            let mut map: HashMap<String, GlobalFormId> = HashMap::default();
            for (entry_idx, entry) in entries.iter().enumerate() {
                let Some(winning) = winning_by_entry.get(&entry_idx) else {
                    continue;
                };
                for group in entry.plugin.groups() {
                    for record in group.records_recursive() {
                        if let Some(gfid) = winning.get(&record.header.form_id) {
                            if let Ok(Some(eid)) = record.editor_id() {
                                map.insert(eid.to_owned(), gfid.clone());
                            }
                        }
                    }
                }
            }
            map
        });

        let gfid: &GlobalFormId = index.get(edid)?;
        let record: &Record = self.resolve_record(gfid)?;
        Some((gfid, record))
    }

    /// Iterates all winning-override records with the given 4-byte signature.
    ///
    /// Each item is a `(&GlobalFormId, &Record)` pair. Iteration order is
    /// unspecified.
    ///
    /// Uses the internal signature index for O(1) lookup of matching
    /// [`GlobalFormId`]s, then retrieves each record via
    /// [`Self::resolve_record`].
    ///
    /// # Arguments
    ///
    /// * `sig` - 4-byte record type signature to filter by (e.g. `b"NPC_"`).
    pub fn records_of_type(
        &self,
        sig: Signature,
    ) -> impl Iterator<Item = (&GlobalFormId, &Record)> + '_ {
        self.by_signature
            .get(&sig)
            .into_iter()
            .flat_map(|gfids| gfids.iter())
            .filter_map(move |gfid| {
                let record: &Record = self.resolve_record(gfid)?;
                Some((gfid, record))
            })
    }
}

impl Default for PluginCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::GameContext;

    // Minimal plugin byte-builder helpers (mirrors the pattern from plugin::tests).

    fn build_hedr(version: f32, num_records: u32, next_id: u32) -> Vec<u8> {
        let mut v: Vec<u8> = Vec::new();
        v.extend_from_slice(&version.to_le_bytes());
        v.extend_from_slice(&num_records.to_le_bytes());
        v.extend_from_slice(&next_id.to_le_bytes());
        v
    }

    fn build_subrecord(sig: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut v: Vec<u8> = Vec::new();
        v.extend_from_slice(sig);
        v.extend_from_slice(&(data.len() as u16).to_le_bytes());
        v.extend_from_slice(data);
        v
    }

    fn build_record(sig: &[u8; 4], flags: u32, form_id: u32, data: &[u8]) -> Vec<u8> {
        let mut v: Vec<u8> = Vec::new();
        v.extend_from_slice(sig);
        v.extend_from_slice(&(data.len() as u32).to_le_bytes());
        v.extend_from_slice(&flags.to_le_bytes());
        v.extend_from_slice(&form_id.to_le_bytes());
        v.extend_from_slice(&0u32.to_le_bytes()); // version_control
        v.extend_from_slice(&0u16.to_le_bytes()); // form_version
        v.extend_from_slice(&0u16.to_le_bytes()); // unknown
        v.extend_from_slice(data);
        v
    }

    fn build_grup(label: &[u8; 4], group_type: i32, children: &[u8]) -> Vec<u8> {
        let size: u32 = 24 + children.len() as u32;
        let mut v: Vec<u8> = Vec::new();
        v.extend_from_slice(b"GRUP");
        v.extend_from_slice(&size.to_le_bytes());
        v.extend_from_slice(label);
        v.extend_from_slice(&group_type.to_le_bytes());
        v.extend_from_slice(&0u32.to_le_bytes()); // version_control
        v.extend_from_slice(&0u32.to_le_bytes()); // unknown
        v.extend_from_slice(children);
        v
    }

    /// Builds a minimal plugin (no masters) containing a single record.
    ///
    /// `form_id` should use file_index 0 (= `masters.len()` = 0 = self) for
    /// a self-owned record, e.g. `0x00_000001`.
    fn plugin_with_one_record(
        _plugin_name: &str,
        form_id: u32,
        sig: &[u8; 4],
        edid: Option<&str>,
    ) -> Plugin {
        let hedr: Vec<u8> = build_hedr(1.7, 1, 0x800);
        let tes4_data: Vec<u8> = build_subrecord(b"HEDR", &hedr);
        let tes4: Vec<u8> = build_record(b"TES4", 0, 0, &tes4_data);

        let mut record_data: Vec<u8> = Vec::new();
        if let Some(id) = edid {
            let mut edid_bytes: Vec<u8> = id.as_bytes().to_vec();
            edid_bytes.push(0); // NUL terminator
            record_data.extend_from_slice(&build_subrecord(b"EDID", &edid_bytes));
        }
        let child: Vec<u8> = build_record(sig, 0, form_id, &record_data);
        let grup: Vec<u8> = build_grup(sig, 0, &child);

        let mut bytes: Vec<u8> = Vec::new();
        bytes.extend_from_slice(&tes4);
        bytes.extend_from_slice(&grup);

        Plugin::from_bytes(&bytes, GameContext::sse())
            .expect("plugin_with_one_record: parse failed")
    }

    /// Builds a minimal plugin with one master and a single override record.
    ///
    /// `form_id` with file_index 0 references `master` (masters[0]).
    fn plugin_with_master_and_record(
        master: &str,
        form_id: u32,
        sig: &[u8; 4],
    ) -> Plugin {
        let hedr: Vec<u8> = build_hedr(1.7, 1, 0x800);
        let mut tes4_data: Vec<u8> = build_subrecord(b"HEDR", &hedr);
        let mut mast_bytes: Vec<u8> = master.as_bytes().to_vec();
        mast_bytes.push(0);
        tes4_data.extend_from_slice(&build_subrecord(b"MAST", &mast_bytes));
        let tes4: Vec<u8> = build_record(b"TES4", 0, 0, &tes4_data);

        let child: Vec<u8> = build_record(sig, 0, form_id, &[]);
        let grup: Vec<u8> = build_grup(sig, 0, &child);

        let mut bytes: Vec<u8> = Vec::new();
        bytes.extend_from_slice(&tes4);
        bytes.extend_from_slice(&grup);

        Plugin::from_bytes(&bytes, GameContext::sse())
            .expect("plugin_with_master_and_record: parse failed")
    }

    /// Verifies that adding a plugin indexes its records.
    #[test]
    fn add_indexes_records() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let plugin = plugin_with_one_record("mymod.esp", 0x00_000001, b"NPC_", None);

        // when
        let mut cache = PluginCache::new();
        cache.add("mymod.esp", plugin)?;

        // then
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.record_count(), 1);
        Ok(())
    }

    /// Verifies that the later plugin wins when two plugins define the same
    /// GlobalFormId (winning-override semantics).
    #[test]
    fn winning_override_replaces_earlier_entry(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        // mod_a.esp owns record 0x00_000001 and gives it an EDID.
        let plugin_a = plugin_with_one_record("mod_a.esp", 0x00_000001, b"NPC_", Some("OrigNpc"));
        // mod_b.esp lists mod_a.esp as master and overrides the same record
        // (form_id file_index = 0 → masters[0] = "mod_a.esp").
        let plugin_b = plugin_with_master_and_record("mod_a.esp", 0x00_000001, b"NPC_");

        // when
        let mut cache = PluginCache::new();
        cache.add("mod_a.esp", plugin_a)?;
        cache.add("mod_b.esp", plugin_b)?;

        // then — only one record per GlobalFormId; the winner comes from mod_b.
        let gfid = GlobalFormId { plugin_name: "mod_a.esp".to_owned(), object_id: 0x000001 };
        let record = cache.resolve_record(&gfid).ok_or("record not found")?;
        assert!(record.editor_id()?.is_none(), "mod_b override has no EDID");
        assert_eq!(cache.record_count(), 1);
        Ok(())
    }

    /// Verifies that resolve_record returns None for an unknown GlobalFormId.
    #[test]
    fn resolve_record_unknown_returns_none(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let cache = PluginCache::new();
        let gfid = GlobalFormId { plugin_name: "nobody.esp".to_owned(), object_id: 0x001 };

        // when / then
        assert!(cache.resolve_record(&gfid).is_none());
        Ok(())
    }

    /// Verifies that find_by_editor_id locates a record by its EDID subrecord.
    #[test]
    fn find_by_editor_id_returns_record(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let plugin = plugin_with_one_record("mymod.esp", 0x00_000001, b"NPC_", Some("ManaPotion"));

        // when
        let mut cache = PluginCache::new();
        cache.add("mymod.esp", plugin)?;
        let result = cache.find_by_editor_id("ManaPotion");

        // then
        let (gfid, record) = result.ok_or("EditorID not found")?;
        assert_eq!(gfid.plugin_name, "mymod.esp");
        assert_eq!(gfid.object_id, 0x000001);
        assert_eq!(record.header.signature, Signature(*b"NPC_"));
        Ok(())
    }

    /// Verifies that find_by_editor_id returns None for an unknown EditorID.
    #[test]
    fn find_by_editor_id_unknown_returns_none(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let cache = PluginCache::new();

        // when / then
        assert!(cache.find_by_editor_id("DoesNotExist").is_none());
        Ok(())
    }

    /// Verifies that records_of_type returns only records with the given
    /// signature, across multiple plugins.
    #[test]
    fn records_of_type_filters_by_signature(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given — each plugin defines one record with a distinct type
        let plugin_npc = plugin_with_one_record("mod_npc.esp", 0x00_000001, b"NPC_", None);
        let plugin_weap = plugin_with_one_record("mod_weap.esp", 0x00_000001, b"WEAP", None);

        // when
        let mut cache = PluginCache::new();
        cache.add("mod_npc.esp", plugin_npc)?;
        cache.add("mod_weap.esp", plugin_weap)?;
        let npcs: Vec<_> = cache.records_of_type(Signature(*b"NPC_")).collect();
        let weapons: Vec<_> = cache.records_of_type(Signature(*b"WEAP")).collect();

        // then
        assert_eq!(npcs.len(), 1);
        assert_eq!(weapons.len(), 1);
        assert_eq!(npcs[0].1.header.signature, Signature(*b"NPC_"));
        assert_eq!(weapons[0].1.header.signature, Signature(*b"WEAP"));
        Ok(())
    }

    /// Verifies that the EditorID index is rebuilt after a new plugin is added.
    #[test]
    fn editor_id_index_rebuilt_after_add(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let plugin_a = plugin_with_one_record("mod_a.esp", 0x00_000001, b"NPC_", Some("FirstNpc"));
        let plugin_b =
            plugin_with_one_record("mod_b.esp", 0x00_000002, b"NPC_", Some("SecondNpc"));

        // when — trigger index build, then invalidate it by adding a second plugin
        let mut cache = PluginCache::new();
        cache.add("mod_a.esp", plugin_a)?;
        assert!(cache.find_by_editor_id("FirstNpc").is_some(), "first lookup");
        cache.add("mod_b.esp", plugin_b)?;

        // then — both EditorIDs are available after index rebuild
        assert!(cache.find_by_editor_id("FirstNpc").is_some());
        assert!(cache.find_by_editor_id("SecondNpc").is_some());
        Ok(())
    }
}
