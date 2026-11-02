// SPDX-License-Identifier: Apache-2.0
//!
//! Zero-copy file access and compression primitives for bethkit.
//!
//! This crate provides the shared I/O foundation used by all other bethkit
//! crates. It has no knowledge of Bethesda-specific formats - it only provides
//! memory-mapped file access, a cursor over byte slices, an append-only byte
//! writer, compile-time byte order selection, and decompression.

mod byte_order;
mod compression;
mod cursor;
mod error;
mod mmap;
mod writer;

pub use byte_order::{BigEndian, ByteOrder, Endianness, LittleEndian};
pub use compression::{decompress_lz4, decompress_lz4_frame, decompress_zlib};
pub use cursor::SliceCursor;
pub use error::{IoError, Result};
pub use mmap::MappedFile;
pub use writer::{ByteWriter, U32Slot};
