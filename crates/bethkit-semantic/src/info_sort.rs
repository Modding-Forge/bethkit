// SPDX-License-Identifier: Apache-2.0
//! Load-order-aware ordering of Skyrim INFO topic-child groups.

use std::collections::HashMap;
use std::sync::Arc;

use bethkit_core::{
    FormId, GlobalFormId, Group, GroupLabel, GroupType, LoadOrder, PluginPatcher, RecordPatch,
    Signature, WritableRecord,
};
use bethkit_schema::CallbackImplementation;

use crate::{
    OwnedFieldValue, PluginInput, Result, SemanticContext, SemanticError, SemanticHandlerRegistry,
};

#[derive(Clone, Copy)]
struct InfoNode<'a> {
    plugin_index: usize,
    raw_form_id: FormId,
    identity: &'a GlobalFormId,
    previous: Option<&'a GlobalFormId>,
    has_previous_field: bool,
}

/// Ordered direct-record FormIDs for one topic-child group in a target plugin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InfoGroupSortPlan {
    form_ids: Vec<FormId>,
    changed: bool,
}

/// Transactional INFO PNAM record edit plus its optional containing-group sort.
pub struct InfoPreviousEdit {
    record: WritableRecord,
    group_sort: Option<InfoGroupSortPlan>,
    changed: bool,
}

impl InfoPreviousEdit {
    /// Returns whether the PNAM value differs from the source record.
    pub fn is_changed(&self) -> bool {
        self.changed
    }

    /// Returns the planned containing-group sort, when sorting is enabled.
    pub fn group_sort(&self) -> Option<&InfoGroupSortPlan> {
        self.group_sort.as_ref()
    }

    /// Registers the record replacement and group sort on `patcher`.
    ///
    /// An unchanged PNAM edit registers neither operation.
    ///
    /// # Errors
    ///
    /// Returns [`SemanticError`] when the group no longer matches the plan or
    /// the core patcher rejects either operation.
    pub fn apply(self, patcher: &mut PluginPatcher<'_>, group: &Group) -> Result<()> {
        if !self.changed {
            return Ok(());
        }
        let form_id = self.record.form_id;
        patcher.replace_record(form_id, RecordPatch::from_writable_record(self.record));
        if let Some(group_sort) = self.group_sort {
            group_sort.apply(patcher, group)?;
        }
        Ok(())
    }
}

impl InfoGroupSortPlan {
    /// Returns the complete direct-record order for the target group.
    pub fn form_ids(&self) -> &[FormId] {
        &self.form_ids
    }

    /// Returns whether applying this plan changes the target group's order.
    pub fn is_changed(&self) -> bool {
        self.changed
    }

    /// Registers this plan on a plugin patcher.
    ///
    /// A plan that already matches the group is a no-op.
    ///
    /// # Errors
    ///
    /// Returns [`SemanticError`] when the group no longer matches the planned
    /// complete record set or the core patcher rejects the ordering.
    pub fn apply(&self, patcher: &mut PluginPatcher<'_>, group: &Group) -> Result<()> {
        if self.changed {
            patcher.reorder_group_records(group, &self.form_ids)?;
        }
        Ok(())
    }
}

/// Builds xEdit's PNAM-linked INFO order for one target plugin and DIAL topic.
///
/// `plugins` must be in load-order order and contain the target plugin. Only
/// plugins up to and including the target participate, matching xEdit's
/// master-and-self group walk.
///
/// Returns `None` when the target plugin has no matching topic-child group.
///
/// # Errors
///
/// Returns [`SemanticError`] when plugin names are duplicated, a group or INFO
/// FormID cannot be resolved, PNAM is truncated, the target group is
/// ambiguous, or the PNAM graph contains a cycle.
pub fn plan_info_group_sort(
    load_order: &LoadOrder,
    plugins: &[PluginInput<'_>],
    target_plugin: &str,
    dial: &GlobalFormId,
) -> Result<Option<InfoGroupSortPlan>> {
    plan_info_group_sort_with_override(load_order, plugins, target_plugin, dial, None)
}

/// Edits one INFO PNAM value and, when enabled, plans its containing group sort.
///
/// The callback path and operation are taken from the loaded schema package.
/// The returned edit does not mutate the source plugin until
/// [`InfoPreviousEdit::apply`] is called.
///
/// # Errors
///
/// Returns [`SemanticError`] when the target plugin, INFO record, topic group,
/// schema callback, PNAM value, or resulting linked-list order is invalid.
pub fn edit_info_previous(
    context: &SemanticContext,
    load_order: &LoadOrder,
    plugins: &[PluginInput<'_>],
    target_plugin: &str,
    info_form_id: FormId,
    new_previous: FormId,
    sort_info: bool,
) -> Result<InfoPreviousEdit> {
    let target_name = target_plugin.to_lowercase();
    let target_matches = plugins
        .iter()
        .filter(|plugin| plugin.name.eq_ignore_ascii_case(&target_name))
        .collect::<Vec<_>>();
    let [target] = target_matches.as_slice() else {
        return Err(SemanticError::Handler {
            handler: "edit.sort_info_group".to_owned(),
            message: format!(
                "target plugin {target_plugin:?} resolves {} load-order entries",
                target_matches.len()
            ),
        });
    };
    let (group, record) =
        find_topic_group_record(target, info_form_id)?.ok_or_else(|| SemanticError::Handler {
            handler: "edit.sort_info_group".to_owned(),
            message: format!("INFO {info_form_id} has no containing topic-child group"),
        })?;
    let GroupLabel::FormId(dial_raw) = group.header.label else {
        return Err(SemanticError::Handler {
            handler: "edit.sort_info_group".to_owned(),
            message: "topic-child group has a non-FormID label".to_owned(),
        });
    };
    let dial = load_order
        .resolve(dial_raw, target.name, target.plugin.masters())
        .ok_or_else(|| SemanticError::Handler {
            handler: "edit.sort_info_group".to_owned(),
            message: format!("cannot resolve containing DIAL FormID {dial_raw}"),
        })?;
    let callback_path = context
        .registry()
        .package()
        .callback_bindings()
        .iter()
        .find_map(|binding| {
            let CallbackImplementation::BuiltIn { operation } = &binding.implementation else {
                return None;
            };
            (binding.callback_id == "def.after_set"
                && operation.id == "edit.sort_info_group"
                && binding.path.starts_with("INFO/"))
            .then_some(binding.path.as_str())
        })
        .ok_or_else(|| SemanticError::MissingHandler("edit.sort_info_group".to_owned()))?;
    let old_previous = info_previous_raw(record)?;
    let changed = old_previous != Some(new_previous);
    if !changed {
        return Ok(InfoPreviousEdit {
            record: writable_record(record)?,
            group_sort: None,
            changed: false,
        });
    }

    let mut handlers: SemanticHandlerRegistry = context.handlers().clone();
    handlers.set_sort_info(false);
    let local_context = SemanticContext::new_with_handlers(
        Arc::clone(context.registry().package()),
        context.decoders().clone(),
        handlers,
    )?;
    let mut editor = local_context.edit(record, target.plugin.is_localized())?;
    if old_previous.is_some() {
        editor.set(callback_path, 0, &OwnedFieldValue::FormId(new_previous))?;
    } else {
        editor.insert(callback_path, &OwnedFieldValue::FormId(new_previous))?;
    }
    let edited = editor.into_writable_record();
    let group_sort = if sort_info {
        plan_info_group_sort_with_override(
            load_order,
            plugins,
            target_plugin,
            &dial,
            Some(&edited),
        )?
    } else {
        None
    };
    Ok(InfoPreviousEdit {
        record: edited,
        group_sort,
        changed: true,
    })
}

fn plan_info_group_sort_with_override(
    load_order: &LoadOrder,
    plugins: &[PluginInput<'_>],
    target_plugin: &str,
    dial: &GlobalFormId,
    edited: Option<&WritableRecord>,
) -> Result<Option<InfoGroupSortPlan>> {
    let target_name = target_plugin.to_lowercase();
    let target_matches = plugins
        .iter()
        .enumerate()
        .filter(|(_, plugin)| plugin.name.eq_ignore_ascii_case(&target_name))
        .collect::<Vec<_>>();
    let [(target_index, target)] = target_matches.as_slice() else {
        return Err(SemanticError::Handler {
            handler: "edit.sort_info_group".to_owned(),
            message: format!(
                "target plugin {target_plugin:?} resolves {} load-order entries",
                target_matches.len()
            ),
        });
    };
    let Some(target_group) = find_topic_group(load_order, target, dial)? else {
        return Ok(None);
    };

    let mut identities = Vec::new();
    let mut previous_identities = Vec::new();
    let mut raw_nodes = Vec::new();
    for (plugin_index, plugin) in plugins.iter().enumerate().take(*target_index + 1) {
        let Some(group) = find_topic_group(load_order, plugin, dial)? else {
            continue;
        };
        for record in group.records() {
            if record.header.signature != Signature(*b"INFO") {
                return Err(SemanticError::Handler {
                    handler: "edit.sort_info_group".to_owned(),
                    message: format!(
                        "topic-child group contains non-INFO record {}",
                        record.header.signature
                    ),
                });
            }
            let identity = load_order
                .resolve(record.header.form_id, plugin.name, plugin.plugin.masters())
                .ok_or_else(|| SemanticError::Handler {
                    handler: "edit.sort_info_group".to_owned(),
                    message: format!(
                        "cannot resolve INFO FormID {} from {}",
                        record.header.form_id, plugin.name
                    ),
                })?;
            let (has_previous_field, previous) = if edited.is_some_and(|edited| {
                plugin_index == *target_index && edited.form_id == record.header.form_id
            }) {
                info_previous_writable(
                    load_order,
                    plugin,
                    edited.expect("edited record was matched"),
                )?
            } else {
                info_previous(load_order, plugin, record)?
            };
            identities.push(identity);
            previous_identities.push(previous);
            raw_nodes.push((plugin_index, record.header.form_id, has_previous_field));
        }
    }

    let nodes = raw_nodes
        .iter()
        .enumerate()
        .map(
            |(index, (plugin_index, raw_form_id, has_previous_field))| InfoNode {
                plugin_index: *plugin_index,
                raw_form_id: *raw_form_id,
                identity: &identities[index],
                previous: previous_identities[index].as_ref(),
                has_previous_field: *has_previous_field,
            },
        )
        .collect::<Vec<_>>();
    let mut latest_by_identity = HashMap::new();
    for (index, node) in nodes.iter().enumerate() {
        latest_by_identity.insert(node.identity.clone(), index);
    }

    let mut state = vec![0_u8; nodes.len()];
    let mut ordered = Vec::with_capacity(nodes.len());
    for index in 0..nodes.len() {
        insert_info_node(index, &nodes, &latest_by_identity, &mut state, &mut ordered)?;
    }
    let form_ids = ordered
        .into_iter()
        .filter_map(|index| {
            (nodes[index].plugin_index == *target_index).then_some(nodes[index].raw_form_id)
        })
        .collect::<Vec<_>>();
    let current = target_group
        .records()
        .map(|record| record.header.form_id)
        .collect::<Vec<_>>();
    if form_ids.len() != current.len() {
        return Err(SemanticError::Handler {
            handler: "edit.sort_info_group".to_owned(),
            message: format!(
                "planned {} of {} target INFO records",
                form_ids.len(),
                current.len()
            ),
        });
    }
    Ok(Some(InfoGroupSortPlan {
        changed: form_ids != current,
        form_ids,
    }))
}

fn find_topic_group_record<'plugin>(
    plugin: &PluginInput<'plugin>,
    form_id: FormId,
) -> Result<Option<(&'plugin Group, &'plugin bethkit_core::Record)>> {
    let matches = plugin
        .plugin
        .groups()
        .iter()
        .flat_map(groups_recursive)
        .filter(|group| group.header.group_type == GroupType::TopicChildren)
        .filter_map(|group| {
            group
                .records()
                .find(|record| record.header.form_id == form_id)
                .map(|record| (group, record))
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [] => Ok(None),
        [pair] => Ok(Some(*pair)),
        _ => Err(SemanticError::Handler {
            handler: "edit.sort_info_group".to_owned(),
            message: format!(
                "INFO {form_id} resolves {} topic-child groups",
                matches.len()
            ),
        }),
    }
}

fn find_topic_group<'a>(
    load_order: &LoadOrder,
    plugin: &'a PluginInput<'_>,
    dial: &GlobalFormId,
) -> Result<Option<&'a Group>> {
    let matches = plugin
        .plugin
        .groups()
        .iter()
        .flat_map(groups_recursive)
        .filter(|group| group.header.group_type == GroupType::TopicChildren)
        .filter_map(|group| {
            let GroupLabel::FormId(label) = group.header.label else {
                return None;
            };
            load_order
                .resolve(label, plugin.name, plugin.plugin.masters())
                .filter(|identity| identity == dial)
                .map(|_| group)
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [] => Ok(None),
        [group] => Ok(Some(*group)),
        _ => Err(SemanticError::Handler {
            handler: "edit.sort_info_group".to_owned(),
            message: format!(
                "DIAL {dial} resolves {} topic-child groups in {}",
                matches.len(),
                plugin.name
            ),
        }),
    }
}

fn groups_recursive(group: &Group) -> Vec<&Group> {
    let mut groups = vec![group];
    for child in group.subgroups() {
        groups.extend(groups_recursive(child));
    }
    groups
}

fn info_previous(
    load_order: &LoadOrder,
    plugin: &PluginInput<'_>,
    record: &bethkit_core::Record,
) -> Result<(bool, Option<GlobalFormId>)> {
    let subrecords = record.subrecords()?;
    let Some(previous) = subrecords
        .iter()
        .find(|subrecord| subrecord.signature == Signature(*b"PNAM"))
    else {
        return Ok((false, None));
    };
    let bytes = previous.as_bytes();
    let raw = bytes
        .get(..4)
        .and_then(|value| <[u8; 4]>::try_from(value).ok())
        .map(u32::from_le_bytes)
        .ok_or_else(|| SemanticError::Decode {
            path: "INFO/PNAM".to_owned(),
            message: format!(
                "PNAM payload has {} bytes; expected at least four",
                bytes.len()
            ),
        })?;
    let identity = (raw != 0)
        .then(|| load_order.resolve(FormId(raw), plugin.name, plugin.plugin.masters()))
        .flatten();
    Ok((true, identity))
}

fn info_previous_writable(
    load_order: &LoadOrder,
    plugin: &PluginInput<'_>,
    record: &WritableRecord,
) -> Result<(bool, Option<GlobalFormId>)> {
    let Some(previous) = record
        .subrecords
        .iter()
        .find(|subrecord| subrecord.signature == Signature(*b"PNAM"))
    else {
        return Ok((false, None));
    };
    let raw = previous
        .data
        .get(..4)
        .and_then(|value| <[u8; 4]>::try_from(value).ok())
        .map(u32::from_le_bytes)
        .ok_or_else(|| SemanticError::Decode {
            path: "INFO/PNAM".to_owned(),
            message: format!(
                "PNAM payload has {} bytes; expected at least four",
                previous.data.len()
            ),
        })?;
    let identity = (raw != 0)
        .then(|| load_order.resolve(FormId(raw), plugin.name, plugin.plugin.masters()))
        .flatten();
    Ok((true, identity))
}

fn info_previous_raw(record: &bethkit_core::Record) -> Result<Option<FormId>> {
    let subrecords = record.subrecords()?;
    let Some(previous) = subrecords
        .iter()
        .find(|subrecord| subrecord.signature == Signature(*b"PNAM"))
    else {
        return Ok(None);
    };
    let bytes = previous.as_bytes();
    bytes
        .get(..4)
        .and_then(|value| <[u8; 4]>::try_from(value).ok())
        .map(u32::from_le_bytes)
        .map(FormId)
        .map(Some)
        .ok_or_else(|| SemanticError::Decode {
            path: "INFO/PNAM".to_owned(),
            message: format!(
                "PNAM payload has {} bytes; expected at least four",
                bytes.len()
            ),
        })
}

fn writable_record(record: &bethkit_core::Record) -> Result<WritableRecord> {
    Ok(WritableRecord {
        signature: record.header.signature,
        flags: record.header.flags,
        form_id: record.header.form_id,
        form_version: record.header.form_version,
        subrecords: record
            .subrecords()?
            .iter()
            .map(|subrecord| bethkit_core::WritableSubRecord {
                signature: subrecord.signature,
                data: subrecord.as_bytes().to_vec(),
            })
            .collect(),
    })
}

fn insert_info_node(
    index: usize,
    nodes: &[InfoNode<'_>],
    latest_by_identity: &HashMap<GlobalFormId, usize>,
    state: &mut [u8],
    ordered: &mut Vec<usize>,
) -> Result<()> {
    if state[index] == 2 {
        return Ok(());
    }
    if state[index] == 1 {
        return Err(SemanticError::Handler {
            handler: "edit.sort_info_group".to_owned(),
            message: format!("cyclic PNAM chain reaches {}", nodes[index].identity),
        });
    }
    state[index] = 1;
    let insertion = if let Some(previous) = nodes[index].previous {
        if let Some(target) = latest_by_identity.get(previous).copied() {
            insert_info_node(target, nodes, latest_by_identity, state, ordered)?;
            ordered
                .iter()
                .position(|candidate| *candidate == target)
                .map(|position| position + 1)
                .unwrap_or(ordered.len())
        } else {
            0
        }
    } else if nodes[index].has_previous_field {
        0
    } else {
        ordered.len()
    };
    if let Some(position) = ordered.iter().position(|candidate| *candidate == index) {
        ordered.remove(position);
    }
    ordered.insert(insertion.min(ordered.len()), index);
    state[index] = 2;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bethkit_core::{
        GameContext, Plugin, PluginWriter, RecordFlags, WritableGroup, WritableGroupChild,
        WritableRecord, WritableSubRecord,
    };
    use bethkit_schema::{
        BuiltInOperation, CallbackBinding, HandlerRequirement, PrimitiveType, SchemaGame,
        SchemaManifest, SchemaNode, SchemaNodeId, SchemaNodeKind, SchemaPackage, SchemaRecord,
        SchemaSignature, ValidationStatus, PACKAGE_FORMAT_VERSION,
    };

    fn info(form_id: u32, previous: Option<u32>) -> WritableGroupChild {
        WritableGroupChild::Record(WritableRecord {
            signature: Signature(*b"INFO"),
            flags: RecordFlags::empty(),
            form_id: FormId(form_id),
            form_version: 44,
            subrecords: previous
                .map(|value| WritableSubRecord {
                    signature: Signature(*b"PNAM"),
                    data: value.to_le_bytes().to_vec(),
                })
                .into_iter()
                .collect(),
        })
    }

    fn plugin_with_topic(
        masters: &[&str],
        dial_form_id: u32,
        info_children: Vec<WritableGroupChild>,
    ) -> Result<Plugin> {
        let mut writer = PluginWriter::new(GameContext::sse(), 1.7);
        for master in masters {
            writer.add_master(master);
        }
        writer.add_group(WritableGroup {
            label: *b"DIAL",
            group_type: GroupType::Normal.to_raw(),
            children: vec![
                WritableGroupChild::Record(WritableRecord {
                    signature: Signature(*b"DIAL"),
                    flags: RecordFlags::empty(),
                    form_id: FormId(dial_form_id),
                    form_version: 44,
                    subrecords: Vec::new(),
                }),
                WritableGroupChild::Group(WritableGroup {
                    label: dial_form_id.to_le_bytes(),
                    group_type: GroupType::TopicChildren.to_raw(),
                    children: info_children,
                }),
            ],
        });
        Ok(Plugin::from_bytes(
            &writer.write_to_vec()?,
            GameContext::sse(),
        )?)
    }

    fn info_context() -> Result<SemanticContext> {
        let previous_path = "INFO/0:Previous INFO";
        let mut manifest = SchemaManifest {
            format_version: PACKAGE_FORMAT_VERSION,
            game: SchemaGame::SkyrimSe,
            package_version: "test".to_owned(),
            source_repository: "TES5Edit/TES5Edit".to_owned(),
            source_tag: "test".to_owned(),
            source_commit: "00".repeat(20),
            source_archive_sha256: "00".repeat(32),
            exporter_version: "test".to_owned(),
            exporter_binary_sha256: "00".repeat(32),
            exporter_map_sha256: "00".repeat(32),
            exporter_patch_sha256: "00".repeat(32),
            exporter_build_sha256: "00".repeat(32),
            conversion_rules_sha256: "00".repeat(32),
            minimum_bethkit_version: "0.4.0".to_owned(),
            minimum_abi_version: 2,
            validation_status: ValidationStatus::Candidate,
            corpus_sha256: "00".repeat(32),
            validated_records: 0,
            byte_coverage: 0.0,
            callbacks_total: 1,
            callbacks_classified: 1,
            required_decoders: Vec::new(),
            required_handlers: Vec::new(),
        };
        manifest.required_handlers.push(HandlerRequirement {
            id: "edit.sort_info_group".to_owned(),
            minimum_version: 1,
        });
        let package = SchemaPackage::new_with_callbacks(
            manifest,
            vec![SchemaRecord {
                signature: SchemaSignature(*b"INFO"),
                name: "Dialog Response".to_owned(),
                root: SchemaNode {
                    id: SchemaNodeId(0),
                    path: "INFO".to_owned(),
                    name: "Dialog Response".to_owned(),
                    required: true,
                    conflict_priority: bethkit_schema::ConflictPriority::Normal,
                    condition: None,
                    kind: SchemaNodeKind::Sequence {
                        children: vec![SchemaNode {
                            id: SchemaNodeId(1),
                            path: previous_path.to_owned(),
                            name: "Previous INFO".to_owned(),
                            required: false,
                            conflict_priority: bethkit_schema::ConflictPriority::Normal,
                            condition: None,
                            kind: SchemaNodeKind::Subrecord {
                                signature: SchemaSignature(*b"PNAM"),
                                payload: Box::new(SchemaNode {
                                    id: SchemaNodeId(2),
                                    path: format!("{previous_path}/payload"),
                                    name: "Previous INFO".to_owned(),
                                    required: true,
                                    conflict_priority: bethkit_schema::ConflictPriority::Normal,
                                    condition: None,
                                    kind: SchemaNodeKind::Primitive {
                                        primitive: PrimitiveType::FormId {
                                            targets: vec![SchemaSignature(*b"INFO")],
                                        },
                                    },
                                }),
                            },
                        }],
                    },
                },
            }],
            vec![CallbackBinding {
                path: previous_path.to_owned(),
                callback_id: "def.after_set".to_owned(),
                callback_slot: None,
                implementation_fingerprint: "44".repeat(32),
                implementation: CallbackImplementation::BuiltIn {
                    operation: BuiltInOperation {
                        id: "edit.sort_info_group".to_owned(),
                        minimum_version: 1,
                        configuration: serde_json::json!({}),
                    },
                },
            }],
        )?;
        SemanticContext::new(Arc::new(package), crate::DecoderRegistry::builtin())
    }

    /// Rebuilds a scrambled local PNAM chain and applies it losslessly.
    #[test]
    fn local_info_group_sort_matches_pnam_chain(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let plugin = plugin_with_topic(
            &[],
            0x10,
            vec![
                info(0x13, Some(0x12)),
                info(0x11, None),
                info(0x12, Some(0x11)),
            ],
        )?;
        let input = PluginInput {
            name: "Dialog.esp",
            plugin: &plugin,
        };
        let mut load_order = LoadOrder::new();
        load_order.push(input.name, plugin.kind())?;
        let dial = GlobalFormId {
            plugin_name: "dialog.esp".to_owned(),
            object_id: 0x10,
        };

        // when
        let plan = plan_info_group_sort(&load_order, &[input], input.name, &dial)?
            .expect("target topic group exists");
        let group = plugin.groups()[0]
            .subgroups()
            .next()
            .expect("topic group exists");
        let mut patcher = PluginPatcher::new(&plugin);
        plan.apply(&mut patcher, group)?;
        let mut output = Vec::new();
        patcher.write_to(&mut output)?;

        // then
        assert_eq!(plan.form_ids(), [FormId(0x11), FormId(0x12), FormId(0x13)]);
        assert!(plan.is_changed());
        let reparsed = Plugin::from_bytes(&output, GameContext::sse())?;
        assert_eq!(
            reparsed.groups()[0]
                .subgroups()
                .next()
                .expect("reparsed topic group exists")
                .records()
                .map(|record| record.header.form_id)
                .collect::<Vec<_>>(),
            [FormId(0x11), FormId(0x12), FormId(0x13)]
        );
        Ok(())
    }

    /// Applies a changed PNAM record and containing-group order as one patch plan.
    #[test]
    fn info_previous_edit_updates_record_and_group(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let plugin = plugin_with_topic(
            &[],
            0x10,
            vec![
                info(0x13, Some(0x12)),
                info(0x11, None),
                info(0x12, Some(0x11)),
            ],
        )?;
        let input = PluginInput {
            name: "Dialog.esp",
            plugin: &plugin,
        };
        let mut load_order = LoadOrder::new();
        load_order.push(input.name, plugin.kind())?;
        let context = info_context()?;
        let group = plugin.groups()[0]
            .subgroups()
            .next()
            .expect("topic group exists");

        // when
        let edit = edit_info_previous(
            &context,
            &load_order,
            &[input],
            input.name,
            FormId(0x12),
            FormId::NULL,
            true,
        )?;
        let mut patcher = PluginPatcher::new(&plugin);
        edit.apply(&mut patcher, group)?;
        let mut output = Vec::new();
        patcher.write_to(&mut output)?;

        // then
        let reparsed = Plugin::from_bytes(&output, GameContext::sse())?;
        let records = reparsed.groups()[0]
            .subgroups()
            .next()
            .expect("reparsed topic group exists")
            .records()
            .collect::<Vec<_>>();
        assert_eq!(
            records
                .iter()
                .map(|record| record.header.form_id)
                .collect::<Vec<_>>(),
            [FormId(0x12), FormId(0x13), FormId(0x11)]
        );
        assert_eq!(info_previous_raw(records[0])?, Some(FormId::NULL));
        Ok(())
    }

    /// Leaves both record and group untouched when PNAM did not change.
    #[test]
    fn unchanged_info_previous_edit_is_noop() -> std::result::Result<(), Box<dyn std::error::Error>>
    {
        // given
        let plugin = plugin_with_topic(&[], 0x10, vec![info(0x11, None), info(0x12, Some(0x11))])?;
        let input = PluginInput {
            name: "Dialog.esp",
            plugin: &plugin,
        };
        let mut load_order = LoadOrder::new();
        load_order.push(input.name, plugin.kind())?;
        let context = info_context()?;
        let group = plugin.groups()[0]
            .subgroups()
            .next()
            .expect("topic group exists");

        // when
        let edit = edit_info_previous(
            &context,
            &load_order,
            &[input],
            input.name,
            FormId(0x12),
            FormId(0x11),
            true,
        )?;
        assert!(!edit.is_changed());
        let mut patcher = PluginPatcher::new(&plugin);
        edit.apply(&mut patcher, group)?;
        let mut output = Vec::new();
        patcher.write_to(&mut output)?;

        // then
        assert_eq!(patcher.patch_count(), 0);
        assert_eq!(patcher.group_reorder_count(), 0);
        assert_eq!(output, plugin.source_bytes());
        Ok(())
    }

    /// Uses master INFO anchors when ordering records in an override plugin.
    #[test]
    fn override_info_group_sort_uses_master_chain(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let master = plugin_with_topic(&[], 0x10, vec![info(0x11, None), info(0x12, Some(0x11))])?;
        let override_plugin = plugin_with_topic(
            &["Master.esm"],
            0x10,
            vec![
                info(0x01_000015, Some(0x01_000014)),
                info(0x01_000014, Some(0x12)),
            ],
        )?;
        let inputs = [
            PluginInput {
                name: "Master.esm",
                plugin: &master,
            },
            PluginInput {
                name: "Override.esp",
                plugin: &override_plugin,
            },
        ];
        let mut load_order = LoadOrder::new();
        for input in &inputs {
            load_order.push(input.name, input.plugin.kind())?;
        }
        let dial = GlobalFormId {
            plugin_name: "master.esm".to_owned(),
            object_id: 0x10,
        };

        // when
        let plan = plan_info_group_sort(&load_order, &inputs, "Override.esp", &dial)?
            .expect("override topic group exists");

        // then
        assert_eq!(plan.form_ids(), [FormId(0x01_000014), FormId(0x01_000015)]);
        assert!(plan.is_changed());
        Ok(())
    }

    /// Rejects cyclic PNAM chains instead of emitting an arbitrary order.
    #[test]
    fn info_group_sort_rejects_cycles() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let plugin = plugin_with_topic(
            &[],
            0x10,
            vec![info(0x11, Some(0x12)), info(0x12, Some(0x11))],
        )?;
        let input = PluginInput {
            name: "Cycle.esp",
            plugin: &plugin,
        };
        let mut load_order = LoadOrder::new();
        load_order.push(input.name, plugin.kind())?;
        let dial = GlobalFormId {
            plugin_name: "cycle.esp".to_owned(),
            object_id: 0x10,
        };

        // when
        let result = plan_info_group_sort(&load_order, &[input], input.name, &dial);

        // then
        assert!(matches!(result, Err(SemanticError::Handler { .. })));
        Ok(())
    }
}
