// SPDX-License-Identifier: Apache-2.0
//!
//! FFI functions for iterating record groups and their children.
//!
//! # Ownership
//!
//! [`BethkitGroup`] is a **borrowed** handle.  It aliases a [`Group`] inside
//! the parent `BethkitPlugin` and must never be freed directly.

use bethkit_core::{Group, GroupChild};

use crate::record::BethkitRecord;
use crate::{null_check, set_last_error};

/// A borrowed, read-only handle to a record group.
///
/// Obtained from [`bethkit_plugin_group_get`] or
/// [`bethkit_group_child_as_group`].  Valid for the lifetime of the owning
/// `BethkitPlugin`.  Never free this handle.
#[repr(transparent)]
pub struct BethkitGroup(pub(crate) Group);

/// Returns the raw integer group type of `group`.
///
/// The value maps to [`bethkit_core::GroupType`]:
/// 0 = Normal, 1 = WorldChildren, 2 = InteriorCellBlock,
/// 3 = InteriorCellSubBlock, 4 = ExteriorCellBlock,
/// 5 = ExteriorCellSubBlock, 6 = CellChildren, 7 = TopicChildren,
/// 8 = CellPersistentChildren, 9 = CellTemporaryChildren.
///
/// Returns -1 and sets the last error if `group` is null.
#[no_mangle]
pub extern "C" fn bethkit_group_type(group: *const BethkitGroup) -> i32 {
    null_check!(group, "bethkit_group_type", -1);
    // SAFETY: group is non-null and borrowed from a live BethkitPlugin.
    unsafe { &*group }.0.header.group_type as i32
}

/// Returns the number of direct children (records or nested groups) in `group`.
///
/// Returns 0 and sets the last error if `group` is null.
#[no_mangle]
pub extern "C" fn bethkit_group_child_count(group: *const BethkitGroup) -> usize {
    null_check!(group, "bethkit_group_child_count", 0);
    // SAFETY: group is non-null.
    unsafe { &*group }.0.children().len()
}

/// Returns `true` if the child at `index` is a record, `false` if it is a
/// nested group.
///
/// Returns `false` and sets the last error if `group` is null or `index` is
/// out of bounds.
#[no_mangle]
pub extern "C" fn bethkit_group_child_is_record(group: *const BethkitGroup, index: usize) -> bool {
    null_check!(group, "bethkit_group_child_is_record", false);
    // SAFETY: group is non-null.
    let g = unsafe { &*group };
    match g.0.children().get(index) {
        Some(GroupChild::Record(_)) => true,
        Some(GroupChild::Group(_)) => false,
        None => {
            set_last_error(format!(
                "bethkit_group_child_is_record: index {index} out of bounds (len = {})",
                g.0.children().len()
            ));
            false
        }
    }
}

/// Returns a borrowed pointer to the record at `index`, or null if the child
/// is a nested group or the index is out of bounds.
///
/// The returned pointer is borrowed from the owning plugin and must not be
/// freed.
///
/// # Errors
///
/// Returns null and sets the last error if `group` is null, `index` is out
/// of bounds, or the child is not a record.
#[no_mangle]
pub extern "C" fn bethkit_group_child_as_record(
    group: *const BethkitGroup,
    index: usize,
) -> *const BethkitRecord {
    null_check!(group, "bethkit_group_child_as_record", std::ptr::null());
    // SAFETY: group is non-null.
    let g = unsafe { &*group };
    match g.0.children().get(index) {
        Some(GroupChild::Record(r)) => r as *const _ as *const BethkitRecord,
        Some(GroupChild::Group(_)) => {
            set_last_error(format!(
                "bethkit_group_child_as_record: child at index {index} is a group, not a record"
            ));
            std::ptr::null()
        }
        None => {
            set_last_error(format!(
                "bethkit_group_child_as_record: index {index} out of bounds (len = {})",
                g.0.children().len()
            ));
            std::ptr::null()
        }
    }
}

/// Returns a borrowed pointer to the nested group at `index`, or null if the
/// child is a record or the index is out of bounds.
///
/// The returned pointer is borrowed from the owning plugin and must not be
/// freed.
///
/// # Errors
///
/// Returns null and sets the last error if `group` is null, `index` is out
/// of bounds, or the child is not a group.
#[no_mangle]
pub extern "C" fn bethkit_group_child_as_group(
    group: *const BethkitGroup,
    index: usize,
) -> *const BethkitGroup {
    null_check!(group, "bethkit_group_child_as_group", std::ptr::null());
    // SAFETY: group is non-null.
    let g = unsafe { &*group };
    match g.0.children().get(index) {
        Some(GroupChild::Group(sub)) => sub as *const Group as *const BethkitGroup,
        Some(GroupChild::Record(_)) => {
            set_last_error(format!(
                "bethkit_group_child_as_group: child at index {index} is a record, not a group"
            ));
            std::ptr::null()
        }
        None => {
            set_last_error(format!(
                "bethkit_group_child_as_group: index {index} out of bounds (len = {})",
                g.0.children().len()
            ));
            std::ptr::null()
        }
    }
}
