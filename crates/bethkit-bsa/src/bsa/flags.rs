// SPDX-License-Identifier: Apache-2.0
//!
//! Archive-level and file-level flags for BSA (TES4/FO3/SSE) archives.

use bitflags::bitflags;

bitflags! {
    /// Flags from the BSA header describing archive-wide properties.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct ArchiveFlags: u32 {
        /// Folder names are stored in the archive.
        const PATHNAMES  = 0x0001;
        /// File names are stored in the archive.
        const FILENAMES  = 0x0002;
        /// Files are compressed by default (individual files may invert this).
        const COMPRESS   = 0x0004;
        /// Retain directory names during extraction.
        const RETAIN_DIR = 0x0008;
        /// Retain file names during extraction.
        const RETAIN_FILE= 0x0010;
        /// Retain file name offsets.
        const RETAIN_OFF = 0x0020;
        /// Xbox 360 archive (big-endian).
        const XBOX360    = 0x0040;
        /// Retain strings during startup.
        const RETAIN_STR = 0x0080;
        /// A file name is embedded before each file's data.
        const EMBEDNAME  = 0x0100;
        /// XMem codec compression (Xbox).
        const XMEM       = 0x0200;
    }
}

bitflags! {
    /// Flags describing which content types are present in a BSA archive.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct ContentFlags: u32 {
        /// Archive contains mesh (NIF) files.
        const MESHES     = 0x0001;
        /// Archive contains texture (DDS) files.
        const TEXTURES   = 0x0002;
        /// Archive contains menu (XML/SWF) files.
        const MENUS      = 0x0004;
        /// Archive contains sound files.
        const SOUNDS     = 0x0008;
        /// Archive contains voice (lip/fuz) files.
        const VOICES     = 0x0010;
        /// Archive contains shader files.
        const SHADERS    = 0x0020;
        /// Archive contains tree/LST files.
        const TREES      = 0x0040;
        /// Archive contains font files.
        const FONTS      = 0x0080;
        /// Archive contains miscellaneous files.
        const MISC       = 0x0100;
    }
}

/// The per-file size field uses bit 30 to signal that this file's compression
/// state is the inverse of the archive-wide default.
pub const FILE_COMPRESS_BIT: u32 = 0x4000_0000;
