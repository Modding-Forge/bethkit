// SPDX-License-Identifier: Apache-2.0
//!
//! Common archive entry type and the `Archive` trait implemented by all
//! supported archive formats (BSA, BA2).

use std::borrow::Cow;

use crate::error::Result;

/// Metadata describing one file stored inside an archive.
#[derive(Debug, Clone)]
pub struct ArchiveEntry {
    /// Virtual path of the file, normalised to **lowercase** with
    /// **forward-slash** separators (e.g. `"meshes/armor/iron.nif"`).
    pub path: String,
    /// Uncompressed size of the file in bytes.
    pub uncompressed_size: u32,
    /// Compressed (stored) size, or `None` if the file is stored verbatim.
    pub compressed_size: Option<u32>,
}

/// Common interface implemented by every archive type.
///
/// All implementations guarantee that `entries()` and `extract()` are
/// safe to call concurrently from multiple threads (`Send + Sync`).
pub trait Archive: Send + Sync {
    /// Returns the flat list of all file entries in this archive.
    ///
    /// The order is unspecified but stable within a single archive instance.
    fn entries(&self) -> &[ArchiveEntry];

    /// Extracts the raw, decompressed bytes of the file at `path`.
    ///
    /// `path` is matched case-insensitively and accepts both `/` and `\` as
    /// separators.  Returns `None` when the path does not exist in this
    /// archive.
    ///
    /// The returned bytes may be a zero-copy borrow of the memory-mapped file
    /// for uncompressed entries, or an owned `Vec<u8>` for compressed ones.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::BsaError`] if decompression or I/O fails.
    fn extract(&self, path: &str) -> Option<Result<Cow<'_, [u8]>>>;

    /// Returns the number of files stored in this archive.
    fn file_count(&self) -> usize {
        self.entries().len()
    }

    /// A short human-readable name for the archive format, e.g. `"BSA SSE"`.
    fn format_name(&self) -> &'static str;
}

/// Normalises an archive path for use as a lookup key.
///
/// Converts all ASCII bytes to lower-case and replaces `\` with `/`.
/// BSA/BA2 paths are always ASCII, so this is lossless.
///
/// # Arguments
///
/// * `path` - The path to normalise.
///
/// # Returns
///
/// The normalised `String`.
pub(crate) fn normalise_path(path: &str) -> String {
    path.bytes()
        .map(|b| {
            if b == b'\\' {
                b'/'
            } else {
                b.to_ascii_lowercase()
            }
        })
        .map(char::from)
        .collect()
}
