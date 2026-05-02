// SPDX-License-Identifier: Apache-2.0
//! Shared binary builders for unit tests.
//!
//! Every function produces raw bytes in the Bethesda plugin format so tests
//! can construct minimal plugin files without external fixtures.  The helpers
//! are compile-time gated to `#[cfg(test)]` and intentionally use no
//! production API so they remain independent from refactors.

/// Serialises the 12-byte HEDR payload: f32 version, u32 num_records,
/// u32 next_object_id.
pub(crate) fn build_hedr(version: f32, num_records: u32, next_id: u32) -> Vec<u8> {
    let mut v: Vec<u8> = Vec::new();
    v.extend_from_slice(&version.to_le_bytes());
    v.extend_from_slice(&num_records.to_le_bytes());
    v.extend_from_slice(&next_id.to_le_bytes());
    v
}

/// Serialises a single subrecord: 4-byte sig + u16 size + data.
pub(crate) fn build_subrecord(sig: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut v: Vec<u8> = Vec::new();
    v.extend_from_slice(sig);
    v.extend_from_slice(&(data.len() as u16).to_le_bytes());
    v.extend_from_slice(data);
    v
}

/// Serialises a 24-byte record header followed by `data`.
///
/// Layout: sig(4) + data_size(4) + flags(4) + form_id(4) +
///         version_control(4) + form_version(2) + unknown(2) + data.
pub(crate) fn build_record(sig: &[u8; 4], flags: u32, form_id: u32, data: &[u8]) -> Vec<u8> {
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

/// Serialises a GRUP record: GRUP(4) + group_size(4) + label(4) +
/// group_type(4) + version_control(4) + unknown(4) + children.
pub(crate) fn build_grup(label: &[u8; 4], group_type: i32, children: &[u8]) -> Vec<u8> {
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
