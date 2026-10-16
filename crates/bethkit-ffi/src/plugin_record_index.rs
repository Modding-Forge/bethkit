// SPDX-License-Identifier: Apache-2.0
//! Safe lazy lookup routes owned only by the immutable FFI plugin handle.
//!
//! Core plugins expose mutable group vectors, so their linear lookup cannot safely
//! cache routes. FFI plugins expose no mutation of their parsed source tree.

use ahash::AHashMap;
use bethkit_core::{FormId, GroupChild, Plugin, Record};

/// First-match group routes without self-referential pointers or borrowed lifetimes.
pub(super) struct RecordIndex {
    routes: AHashMap<FormId, Box<[usize]>>,
}

impl RecordIndex {
    /// Indexes each source record once, excluding the plugin header.
    pub(super) fn new(plugin: &Plugin) -> Self {
        let mut routes = AHashMap::new();
        for (index, group) in plugin.groups().iter().enumerate() {
            collect(group.children(), &mut vec![index], &mut routes);
        }
        Self { routes }
    }

    /// Resolves a previously indexed route against its unchanged owning plugin.
    pub(super) fn get<'a>(&self, plugin: &'a Plugin, form_id: FormId) -> Option<&'a Record> {
        let (root, route) = self.routes.get(&form_id)?.split_first()?;
        let mut group = plugin.groups().get(*root)?;
        for (position, index) in route.iter().enumerate() {
            match group.children().get(*index)? {
                GroupChild::Record(record) if position + 1 == route.len() => return Some(record),
                GroupChild::Group(child) => group = child,
                GroupChild::Record(_) => return None,
            }
        }
        None
    }
}

fn collect(
    children: &[GroupChild],
    route: &mut Vec<usize>,
    routes: &mut AHashMap<FormId, Box<[usize]>>,
) {
    for (index, child) in children.iter().enumerate() {
        route.push(index);
        match child {
            GroupChild::Record(record) => {
                routes
                    .entry(record.header.form_id)
                    .or_insert_with(|| route.clone().into_boxed_slice());
            }
            GroupChild::Group(group) => collect(group.children(), route, routes),
        }
        route.pop();
    }
}

#[cfg(test)]
mod tests {
    use bethkit_core::{
        GameContext, PluginWriter, RecordFlags, Signature, WritableGroup, WritableGroupChild,
        WritableRecord,
    };

    use super::*;

    fn record(form_id: u32) -> WritableGroupChild {
        WritableGroupChild::Record(WritableRecord {
            signature: Signature(*b"CLAS"),
            flags: RecordFlags::empty(),
            form_id: FormId(form_id),
            form_version: 44,
            subrecords: Vec::new(),
        })
    }

    fn group(children: Vec<WritableGroupChild>) -> WritableGroup {
        WritableGroup {
            label: *b"CLAS",
            group_type: 0,
            children,
        }
    }

    /// Matches linear lookup through nested groups, duplicate IDs, zero and missing IDs.
    #[test]
    fn indexed_records_keep_original_identity_and_order() -> Result<(), Box<dyn std::error::Error>>
    {
        let mut writer = PluginWriter::new(GameContext::sse(), 1.7);
        writer.add_group(group(vec![
            WritableGroupChild::Group(group(vec![record(0x800), record(0)])),
            record(0x800),
            record(0x801),
        ]));
        writer.add_group(group(vec![record(0x802)]));
        let plugin = Plugin::from_bytes(&writer.write_to_vec()?, GameContext::sse())?;
        let index = RecordIndex::new(&plugin);
        for id in [0, 0x800, 0x801, 0x802, 0x803, u32::MAX] {
            let expected = plugin.find_record(FormId(id));
            let actual = index.get(&plugin, FormId(id));
            assert_eq!(actual.is_some(), expected.is_some());
            if let (Some(actual), Some(expected)) = (actual, expected) {
                assert!(std::ptr::eq(actual, expected));
            }
        }
        assert_eq!(index.routes.len(), 4);
        Ok(())
    }

    /// Builds the FFI index lazily once and never includes the TES4 header record.
    #[test]
    fn ffi_index_is_lazy_and_thread_safe() -> Result<(), Box<dyn std::error::Error>> {
        let mut writer = PluginWriter::new(GameContext::sse(), 1.7);
        writer.add_group(group(vec![record(0x800)]));
        let plugin = Plugin::from_bytes(&writer.write_to_vec()?, GameContext::sse())?;
        let owner = std::sync::Arc::new(super::super::BethkitPlugin::new(plugin));
        assert!(owner.record_index.get().is_none());
        let workers: Vec<_> = (0..8)
            .map(|_| {
                let owner = owner.clone();
                std::thread::spawn(move || {
                    super::super::bethkit_plugin_find_record(&*owner, 0x800) as usize
                })
            })
            .collect();
        let expected = super::super::bethkit_plugin_find_record(&*owner, 0x800) as usize;
        assert_ne!(expected, 0);
        for worker in workers {
            assert_eq!(
                worker.join().expect("read-only lookup must not panic"),
                expected
            );
        }
        assert!(owner.record_index.get().is_some());
        assert!(super::super::bethkit_plugin_find_record(&*owner, 0).is_null());
        assert!(super::super::bethkit_plugin_find_record(&*owner, u32::MAX).is_null());
        assert!(super::super::bethkit_plugin_find_record(std::ptr::null(), 0).is_null());
        Ok(())
    }
}
