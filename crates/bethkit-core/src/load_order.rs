// SPDX-License-Identifier: Apache-2.0
//!
//! Load-order management and global FormID resolution.
//!
//! A [`LoadOrder`] maps per-plugin raw FormIDs to game-unique
//! [`GlobalFormId`]s that are independent of the load order.

use ahash::HashMap;

use crate::types::{FormId, PluginKind};

/// A FormID that is independent of the current load order.
///
/// Uniquely identifies a record across all plugins by combining the owning
/// plugin's filename with the record's object ID.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GlobalFormId {
    /// Canonical filename of the plugin that owns this record (lowercased,
    /// includes extension).
    pub plugin_name: String,
    /// Object ID within that plugin (lower 3 bytes of the raw FormID).
    pub object_id: u32,
}

impl std::fmt::Display for GlobalFormId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{:#08X}", self.plugin_name, self.object_id)
    }
}


/// A single entry in the load order.
pub struct LoadOrderEntry {
    /// Canonical plugin filename (lowercased, includes extension).
    pub name: String,
    /// Load-order index (top byte of FormIDs in regular plugins).
    pub index: u8,
    /// The functional type of this plugin.
    pub kind: PluginKind,
    /// For light plugins: the ESL slot index (0x000–0xFFF), if known.
    pub light_slot: Option<u16>,
}

/// Associates plugin filenames with their load-order indices.
///
/// Required to resolve raw file-local FormIDs to globally unique
/// [`GlobalFormId`]s.
///
/// Regular plugins are assigned sequential file indices (0x00–0xFD). Light
/// plugins (ESL) share the sentinel byte 0xFE and are instead assigned an
/// ESL slot (bits 23:12 of the raw FormID).
pub struct LoadOrder {
    entries: Vec<LoadOrderEntry>,
    by_name: HashMap<String, usize>,
    /// Next file-index to assign to a non-ESL plugin (0x00–0xFD).
    regular_index: u8,
    /// Auto-incrementing ESL slot counter (0x000–0xFFF).
    light_slot_counter: u16,
    /// Maps ESL slot index to the owning entry's position in `entries`.
    by_light_slot: HashMap<u16, usize>,
}

impl LoadOrder {
    /// Creates an empty load order.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            by_name: HashMap::default(),
            regular_index: 0,
            light_slot_counter: 0,
            by_light_slot: HashMap::default(),
        }
    }

    /// Appends a plugin to the load order and returns a reference to its
    /// entry.
    ///
    /// The `name` is stored lowercased. Regular plugins (Master, Plugin,
    /// Medium, Update) are assigned a sequential file index (0x00–0xFD).
    /// Light plugins (ESL) receive the sentinel index 0xFE and an
    /// auto-assigned ESL slot (0x000–0xFFF) instead.
    ///
    /// # Arguments
    ///
    /// * `name` - Plugin filename including extension (e.g. `"Skyrim.esm"`).
    /// * `kind` - The functional plugin type.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::LoadOrderIndexFull`] when adding a regular plugin
    /// would consume file index `0xFE` (reserved for the ESL sentinel).
    ///
    /// Returns [`CoreError::LightSlotOverflow`] when the ESL slot counter
    /// would exceed `0xFFF`.
    pub fn push(&mut self, name: &str, kind: PluginKind) -> crate::error::Result<&LoadOrderEntry> {
        let canonical: String = name.to_lowercase();
        let entry_index: usize = self.entries.len();

        let (index, light_slot) = if kind == PluginKind::Light {
            if self.light_slot_counter > 0xFFF {
                return Err(crate::error::CoreError::LightSlotOverflow);
            }
            let slot: u16 = self.light_slot_counter;
            self.by_light_slot.insert(slot, entry_index);
            self.light_slot_counter += 1;
            (0xFE_u8, Some(slot))
        } else {
            if self.regular_index >= 0xFE {
                return Err(crate::error::CoreError::LoadOrderIndexFull);
            }
            let idx: u8 = self.regular_index;
            self.regular_index += 1;
            (idx, None)
        };

        self.entries.push(LoadOrderEntry {
            name: canonical.clone(),
            index,
            kind,
            light_slot,
        });
        self.by_name.insert(canonical, entry_index);

        Ok(&self.entries[entry_index])
    }

    /// Returns the number of entries in the load order.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` if the load order is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Resolves a raw FormID from `source_plugin` to a [`GlobalFormId`].
    ///
    /// For regular FormIDs, the top byte indexes into `source_plugin`'s master
    /// list — index 0 is the first master, the last index (equal to the master
    /// count) refers to `source_plugin` itself.
    ///
    /// For ESL-encoded FormIDs (top byte `0xFE`), the owning plugin is
    /// identified by the ESL slot stored in bits 23:12 of the raw value.
    /// The load order must contain that plugin (added via [`Self::push`])
    /// for resolution to succeed.
    ///
    /// Returns `None` if the file-index is out of range or the ESL slot is
    /// not registered.
    ///
    /// # Arguments
    ///
    /// * `form_id`       - Raw FormID as read from the plugin file.
    /// * `source_plugin` - Filename of the plugin that contains this FormID.
    /// * `masters`       - Ordered list of master filenames from the source
    ///   plugin's `MAST` subrecords.
    pub fn resolve(
        &self,
        form_id: FormId,
        source_plugin: &str,
        masters: &[String],
    ) -> Option<GlobalFormId> {
        let file_index: u8 = form_id.file_index();

        // ESL-encoded FormID: top byte 0xFE is the sentinel; the owning
        // plugin is identified by the ESL slot in bits 23:12.
        if file_index == 0xFE {
            let slot: u16 = form_id.esl_slot();
            let entry_index: usize = *self.by_light_slot.get(&slot)?;
            let owner: &str = &self.entries[entry_index].name;
            return Some(GlobalFormId {
                plugin_name: owner.to_owned(),
                object_id: u32::from(form_id.esl_object()),
            });
        }

        // Regular FormID: top byte indexes into the source plugin's master list.
        let file_index: usize = file_index as usize;
        let object_id: u32 = form_id.object_id();

        let owner: &str = if file_index < masters.len() {
            &masters[file_index]
        } else if file_index == masters.len() {
            source_plugin
        } else {
            return None;
        };

        Some(GlobalFormId {
            plugin_name: owner.to_lowercase(),
            object_id,
        })
    }
}

impl Default for LoadOrder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that push assigns sequential indices.
    #[test]
    fn push_assigns_sequential_indices() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let mut lo = LoadOrder::new();

        // when
        lo.push("Skyrim.esm", PluginKind::Master)?;
        lo.push("Update.esm", PluginKind::Master)?;
        lo.push("Dawnguard.esm", PluginKind::Master)?;

        // then
        assert_eq!(lo.entries[0].index, 0);
        assert_eq!(lo.entries[1].index, 1);
        assert_eq!(lo.entries[2].index, 2);
        Ok(())
    }

    /// Verifies that resolve maps a file-index-0 FormID to the first master.
    #[test]
    fn resolve_points_to_master() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let lo = LoadOrder::new();
        let masters: Vec<String> = vec!["Skyrim.esm".to_string()];
        let form_id = FormId(0x00_001234);

        // when
        let global = lo.resolve(form_id, "MyMod.esp", &masters);

        // then
        let g = global.ok_or("should resolve")?;
        assert_eq!(g.plugin_name, "skyrim.esm");
        assert_eq!(g.object_id, 0x001234);
        Ok(())
    }

    /// Verifies that resolve maps a FormID pointing past all masters to the
    /// source plugin.
    #[test]
    fn resolve_points_to_source_plugin() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let lo = LoadOrder::new();
        let masters: Vec<String> = vec!["Skyrim.esm".to_string()];
        let form_id = FormId(0x01_AABBCC);

        // when
        let global = lo.resolve(form_id, "MyMod.esp", &masters);

        // then
        let g = global.ok_or("should resolve")?;
        assert_eq!(g.plugin_name, "mymod.esp");
        assert_eq!(g.object_id, 0xAABBCC);
        Ok(())
    }

    /// Verifies that an out-of-range file index returns None.
    #[test]
    fn resolve_out_of_range_returns_none() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let lo = LoadOrder::new();
        let masters: Vec<String> = vec!["Skyrim.esm".to_string()];
        let form_id = FormId(0x02_000001);

        // when
        let global = lo.resolve(form_id, "MyMod.esp", &masters);

        // then
        assert!(global.is_none());
        Ok(())
    }

    /// Verifies that a light plugin receives the ESL sentinel index and slot 0.
    #[test]
    fn light_plugin_gets_esl_slot() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let mut lo = LoadOrder::new();

        // when
        lo.push("Skyrim.esm", PluginKind::Master)?;
        let entry = lo.push("LightMod.esp", PluginKind::Light)?;

        // then
        assert_eq!(entry.index, 0xFE);
        assert_eq!(entry.light_slot, Some(0));
        Ok(())
    }

    /// Verifies that multiple light plugins receive sequential ESL slots.
    #[test]
    fn multiple_light_plugins_get_sequential_slots(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let mut lo = LoadOrder::new();

        // when
        lo.push("Light0.esp", PluginKind::Light)?;
        lo.push("Light1.esp", PluginKind::Light)?;
        lo.push("Light2.esp", PluginKind::Light)?;

        // then
        assert_eq!(lo.entries[0].light_slot, Some(0));
        assert_eq!(lo.entries[1].light_slot, Some(1));
        assert_eq!(lo.entries[2].light_slot, Some(2));
        Ok(())
    }

    /// Verifies that regular plugins do not consume ESL slots.
    #[test]
    fn regular_plugins_do_not_consume_esl_slots(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let mut lo = LoadOrder::new();

        // when
        lo.push("Skyrim.esm", PluginKind::Master)?;
        // Copy the values out immediately so the returned reference does not
        // outlive the next mutable borrow of `lo`.
        let (light_index, light_slot) = {
            let e = lo.push("LightMod.esp", PluginKind::Light)?;
            (e.index, e.light_slot)
        };
        lo.push("AnotherMod.esp", PluginKind::Plugin)?;

        // then — regular plugins each occupy a sequential file-index slot.
        assert_eq!(lo.entries[0].index, 0x00);
        assert_eq!(lo.entries[2].index, 0x01);
        // The light plugin has the ESL sentinel and slot 0.
        assert_eq!(light_index, 0xFE);
        assert_eq!(light_slot, Some(0));
        Ok(())
    }

    /// Verifies that an ESL-encoded FormID (0xFE sentinel) resolves to the
    /// owning light plugin.
    #[test]
    fn resolve_esl_form_id_to_light_plugin(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given — ESL FormID: slot 0, object 0xABC → 0xFE_000_ABC
        let mut lo = LoadOrder::new();
        lo.push("LightMod.esp", PluginKind::Light)?;
        let form_id = FormId(0xFE00_0ABC);

        // when
        let global = lo.resolve(form_id, "LightMod.esp", &[]);

        // then
        let g = global.ok_or("should resolve ESL FormID")?;
        assert_eq!(g.plugin_name, "lightmod.esp");
        assert_eq!(g.object_id, 0xABC);
        Ok(())
    }

    /// Verifies that an ESL FormID with an unregistered slot returns None.
    #[test]
    fn resolve_esl_unregistered_slot_returns_none(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given — no light plugins registered
        let lo = LoadOrder::new();
        let form_id = FormId(0xFE00_0001);

        // when
        let global = lo.resolve(form_id, "SomeMod.esp", &[]);

        // then
        assert!(global.is_none());
        Ok(())
    }
}
