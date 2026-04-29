// SPDX-License-Identifier: Apache-2.0
//!
//! Memory-mapped file access.
//!
//! [`MappedFile`] opens a file and maps its entire contents into memory using
//! `memmap2`. The mapping is read-only. The underlying file descriptor is kept
//! open for the lifetime of the struct so the mapping remains valid.

use std::{fs::File, path::Path};

use memmap2::Mmap;

use crate::cursor::SliceCursor;
use crate::error::Result;

/// A read-only memory-mapped file.
///
/// Keeping this struct alive ensures that all [`SliceCursor`]s derived from
/// it remain valid. Wrap it in an [`std::sync::Arc`] when sharing across
/// crates.
pub struct MappedFile {
    /// Keeps the file descriptor open for the lifetime of the mapping.
    _file: File,
    mmap: Mmap,
}

impl MappedFile {
    /// Opens `path` and maps the entire file into memory.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the file to open.
    ///
    /// # Returns
    ///
    /// A [`MappedFile`] whose contents are available as a byte slice.
    ///
    /// # Errors
    ///
    /// Returns [`crate::IoError::Io`] if the file cannot be opened or mapped.
    pub fn open(path: &Path) -> Result<Self> {
        let file: File = File::open(path)?;
        // SAFETY: The file is opened read-only and `_file` is kept alive
        // SAFETY: alongside the mapping, ensuring the fd remains valid for
        // SAFETY: the lifetime of the Mmap.
        let mmap: Mmap = unsafe { Mmap::map(&file)? };
        Ok(Self { _file: file, mmap })
    }

    /// Returns the mapped bytes as a slice.
    pub fn as_bytes(&self) -> &[u8] {
        &self.mmap
    }

    /// Returns the length of the mapped file in bytes.
    pub fn len(&self) -> usize {
        self.mmap.len()
    }

    /// Returns `true` if the mapped file is empty.
    pub fn is_empty(&self) -> bool {
        self.mmap.is_empty()
    }

    /// Creates a [`SliceCursor`] positioned at the start of the mapped data.
    pub fn cursor(&self) -> SliceCursor<'_> {
        SliceCursor::new(self.as_bytes())
    }
}
