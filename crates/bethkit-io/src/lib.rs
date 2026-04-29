// SPDX-License-Identifier: Apache-2.0
//!
//! Zero-copy file access and compression primitives for bethkit.
//!
//! This crate provides the shared I/O foundation used by all other bethkit
//! crates. It has no knowledge of Bethesda-specific formats — it only provides
//! memory-mapped file access, a cursor over byte slices, and decompression.

mod compression;
mod cursor;
mod error;
mod mmap;

pub use compression::{decompress_lz4, decompress_lz4_frame, decompress_zlib};
pub use cursor::SliceCursor;
pub use error::{IoError, Result};
pub use mmap::MappedFile;
