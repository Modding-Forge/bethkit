// SPDX-License-Identifier: Apache-2.0
//!
//! Decompression utilities for plugin and archive data.
//!
//! Bethesda uses two compression schemes:
//! - **zlib** (deflate): used in ESP/ESL/ESM record data and BSA file entries.
//! - **LZ4 block format**: used in BA2 archive entries.

use flate2::read::ZlibDecoder;
use std::io::Read;

use crate::error::{IoError, Result};

/// Decompresses zlib-compressed data.
///
/// This is the format used by compressed ESP/ESM records and BSA file entries.
/// The input must be a raw zlib stream (with zlib header — **not** raw deflate
/// and **not** gzip).
///
/// # Arguments
///
/// * `input`         - Compressed bytes.
/// * `expected_size` - Expected size of the decompressed output. Used to
///   pre-allocate the output buffer; the actual output may
///   differ only if the data is malformed.
///
/// # Returns
///
/// The decompressed bytes as an owned `Vec<u8>`.
///
/// # Errors
///
/// Returns [`IoError::Decompress`] if decompression fails.
pub fn decompress_zlib(input: &[u8], expected_size: usize) -> Result<Vec<u8>> {
    let mut decoder = ZlibDecoder::new(input);
    let mut output: Vec<u8> = Vec::with_capacity(expected_size);
    decoder
        .read_to_end(&mut output)
        .map_err(|e| IoError::Decompress(e.to_string()))?;
    Ok(output)
}

/// Decompresses LZ4 block-format data.
///
/// This is the format used by BA2 archive entries (Starfield). The input must
/// be a raw LZ4 block (no frame header).
///
/// # Arguments
///
/// * `input`         - Compressed bytes.
/// * `expected_size` - Expected size of the decompressed output.
///
/// # Returns
///
/// The decompressed bytes as an owned `Vec<u8>`.
///
/// # Errors
///
/// Returns [`IoError::Decompress`] if decompression fails.
pub fn decompress_lz4(input: &[u8], expected_size: usize) -> Result<Vec<u8>> {
    lz4_flex::block::decompress(input, expected_size)
        .map_err(|e| IoError::Decompress(e.to_string()))
}

/// Decompresses LZ4 frame-format data.
///
/// This is the format used by compressed SSE/AE BSA file entries. The input
/// must be a valid LZ4 frame stream (begins with the frame magic
/// `0x184D2204`).
///
/// # Arguments
///
/// * `input`         - Compressed bytes including the LZ4 frame header.
/// * `expected_size` - Expected size of the decompressed output, used to
///   pre-allocate the output buffer.
///
/// # Returns
///
/// The decompressed bytes as an owned `Vec<u8>`.
///
/// # Errors
///
/// Returns [`IoError::Decompress`] if decompression fails.
pub fn decompress_lz4_frame(input: &[u8], expected_size: usize) -> Result<Vec<u8>> {
    use lz4_flex::frame::FrameDecoder;
    use std::io::Read;

    let mut decoder = FrameDecoder::new(input);
    let mut output: Vec<u8> = Vec::with_capacity(expected_size);
    decoder
        .read_to_end(&mut output)
        .map_err(|e| IoError::Decompress(e.to_string()))?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that a zlib round-trip produces the original data.
    #[test]
    fn zlib_roundtrip() -> std::result::Result<(), Box<dyn std::error::Error>> {
        use flate2::{write::ZlibEncoder, Compression};
        use std::io::Write;

        // given
        let original: &[u8] = b"Hello, Skyrim! This is a test payload for zlib.";
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(original)?;
        let compressed: Vec<u8> = encoder.finish()?;

        // when
        let decompressed: Vec<u8> = decompress_zlib(&compressed, original.len())?;

        // then
        assert_eq!(decompressed, original);
        Ok(())
    }

    /// Verifies that an LZ4 round-trip produces the original data.
    #[test]
    fn lz4_roundtrip() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let original: &[u8] = b"Hello, Skyrim! This is a test payload for LZ4.";
        let compressed: Vec<u8> = lz4_flex::block::compress(original);

        // when
        let decompressed: Vec<u8> = decompress_lz4(&compressed, original.len())?;

        // then
        assert_eq!(decompressed, original);
        Ok(())
    }

    /// Verifies that passing garbage bytes returns a Decompress error.
    #[test]
    fn zlib_garbage_returns_error() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let garbage: &[u8] = &[0xFF, 0xFE, 0x00, 0x11, 0x22];

        // when
        let result = decompress_zlib(garbage, 16);

        // then
        assert!(matches!(result, Err(IoError::Decompress(_))));
        Ok(())
    }

    /// Verifies that passing garbage bytes to lz4 returns a Decompress error.
    #[test]
    fn lz4_garbage_returns_error() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let garbage: &[u8] = &[0xFF, 0xFE, 0x00, 0x11, 0x22];

        // when
        let result = decompress_lz4(garbage, 16);

        // then
        assert!(matches!(result, Err(IoError::Decompress(_))));
        Ok(())
    }
}
