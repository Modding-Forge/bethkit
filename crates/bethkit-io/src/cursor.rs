// SPDX-License-Identifier: Apache-2.0
//!
//! Zero-copy cursor over a byte slice.
//!
//! [`SliceCursor`] is the central I/O primitive passed through every parser in
//! bethkit. All reads are little-endian and bounds-checked, returning
//! [`crate::IoError::UnexpectedEof`] on overrun rather than panicking.

use crate::error::{IoError, Result};

/// A position-tracking cursor over a borrowed byte slice.
///
/// All read operations advance the internal position. The lifetime `'a` ties
/// every returned slice back to the original data, enabling zero-copy parsing.
pub struct SliceCursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> SliceCursor<'a> {
    /// Creates a new cursor positioned at the start of `data`.
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    /// Creates a new cursor starting at `offset` within `data`.
    ///
    /// # Errors
    ///
    /// Returns [`IoError::UnexpectedEof`] if `offset` is beyond the end of
    /// `data`.
    pub fn from_offset(data: &'a [u8], offset: usize) -> Result<Self> {
        if offset > data.len() {
            return Err(IoError::UnexpectedEof { offset });
        }
        Ok(Self { data, pos: offset })
    }

    /// Returns the current byte offset within the data.
    pub fn pos(&self) -> usize {
        self.pos
    }

    /// Returns the number of bytes still available for reading.
    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    /// Returns `true` if no bytes remain.
    pub fn is_empty(&self) -> bool {
        self.pos >= self.data.len()
    }

    /// Reads a single `u8`.
    ///
    /// # Errors
    ///
    /// Returns [`IoError::UnexpectedEof`] if fewer than 1 byte remains.
    pub fn read_u8(&mut self) -> Result<u8> {
        let b: [u8; 1] = self.read_array()?;
        Ok(b[0])
    }

    /// Reads a little-endian `u16`.
    ///
    /// # Errors
    ///
    /// Returns [`IoError::UnexpectedEof`] if fewer than 2 bytes remain.
    pub fn read_u16(&mut self) -> Result<u16> {
        let b: [u8; 2] = self.read_array()?;
        Ok(u16::from_le_bytes(b))
    }

    /// Reads a little-endian `u32`.
    ///
    /// # Errors
    ///
    /// Returns [`IoError::UnexpectedEof`] if fewer than 4 bytes remain.
    pub fn read_u32(&mut self) -> Result<u32> {
        let b: [u8; 4] = self.read_array()?;
        Ok(u32::from_le_bytes(b))
    }

    /// Reads a little-endian `i32`.
    ///
    /// # Errors
    ///
    /// Returns [`IoError::UnexpectedEof`] if fewer than 4 bytes remain.
    pub fn read_i32(&mut self) -> Result<i32> {
        let b: [u8; 4] = self.read_array()?;
        Ok(i32::from_le_bytes(b))
    }

    /// Reads a little-endian `u64`.
    ///
    /// # Errors
    ///
    /// Returns [`IoError::UnexpectedEof`] if fewer than 8 bytes remain.
    pub fn read_u64(&mut self) -> Result<u64> {
        let b: [u8; 8] = self.read_array()?;
        Ok(u64::from_le_bytes(b))
    }

    /// Reads a little-endian `f32`.
    ///
    /// # Errors
    ///
    /// Returns [`IoError::UnexpectedEof`] if fewer than 4 bytes remain.
    pub fn read_f32(&mut self) -> Result<f32> {
        let b: [u8; 4] = self.read_array()?;
        Ok(f32::from_le_bytes(b))
    }

    /// Reads exactly `N` bytes into a stack-allocated array without heap
    /// allocation.
    ///
    /// # Errors
    ///
    /// Returns [`IoError::UnexpectedEof`] if fewer than `N` bytes remain.
    pub fn read_array<const N: usize>(&mut self) -> Result<[u8; N]> {
        let end: usize = self.pos + N;
        if end > self.data.len() {
            return Err(IoError::UnexpectedEof { offset: self.pos });
        }
        // SAFETY: we just verified that self.data[self.pos..end] is valid.
        let mut buf = [0u8; N];
        buf.copy_from_slice(&self.data[self.pos..end]);
        self.pos = end;
        Ok(buf)
    }

    /// Returns a zero-copy sub-slice of `len` bytes and advances the position.
    ///
    /// # Errors
    ///
    /// Returns [`IoError::UnexpectedEof`] if fewer than `len` bytes remain.
    pub fn read_slice(&mut self, len: usize) -> Result<&'a [u8]> {
        let end: usize = self.pos + len;
        if end > self.data.len() {
            return Err(IoError::UnexpectedEof { offset: self.pos });
        }
        let slice: &'a [u8] = &self.data[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    /// Peeks at the next `n` bytes without advancing the position.
    ///
    /// Returns `None` if fewer than `n` bytes remain.
    pub fn peek_bytes(&self, n: usize) -> Option<&[u8]> {
        let end: usize = self.pos + n;
        if end > self.data.len() {
            return None;
        }
        Some(&self.data[self.pos..end])
    }

    /// Peeks at the next byte without advancing the position.
    ///
    /// Returns `None` if no bytes remain.
    pub fn peek_u8(&self) -> Option<u8> {
        self.data.get(self.pos).copied()
    }

    /// Advances the position by `n` bytes without reading them.
    ///
    /// # Errors
    ///
    /// Returns [`IoError::UnexpectedEof`] if `n` exceeds the remaining bytes.
    pub fn skip(&mut self, n: usize) -> Result<()> {
        let end: usize = self.pos + n;
        if end > self.data.len() {
            return Err(IoError::UnexpectedEof { offset: self.pos });
        }
        self.pos = end;
        Ok(())
    }

    /// Returns a new cursor spanning the next `len` bytes, and advances this
    /// cursor past them.
    ///
    /// The returned cursor shares the same lifetime as the original data, so
    /// zero-copy slices obtained from the sub-cursor remain valid.
    ///
    /// # Errors
    ///
    /// Returns [`IoError::UnexpectedEof`] if fewer than `len` bytes remain.
    pub fn sub_cursor(&mut self, len: usize) -> Result<SliceCursor<'a>> {
        let slice: &'a [u8] = self.read_slice(len)?;
        Ok(SliceCursor::new(slice))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that reading primitive types advances the position correctly.
    #[test]
    fn read_primitives_advance_position() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let data: &[u8] = &[0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];
        let mut cursor = SliceCursor::new(data);

        // when
        let b: u8 = cursor.read_u8()?;
        let w: u16 = cursor.read_u16()?;
        let dw: u32 = cursor.read_u32()?;

        // then
        assert_eq!(b, 0x01);
        assert_eq!(w, 0x0302_u16);
        assert_eq!(dw, 0x0706_0504_u32);
        assert_eq!(cursor.pos(), 7);
        Ok(())
    }

    /// Verifies that reading past the end returns UnexpectedEof.
    #[test]
    fn read_past_end_returns_eof() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let data: &[u8] = &[0x01];
        let mut cursor = SliceCursor::new(data);

        // when
        let _ok = cursor.read_u8();
        let result = cursor.read_u8();

        // then
        assert!(matches!(result, Err(IoError::UnexpectedEof { .. })));
        Ok(())
    }

    /// Verifies that sub_cursor produces an independent cursor over the region.
    #[test]
    fn sub_cursor_is_bounded() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let data: &[u8] = &[0xAA, 0xBB, 0xCC, 0xDD];
        let mut cursor = SliceCursor::new(data);

        // when
        let mut sub = cursor.sub_cursor(2)?;
        let a: u8 = sub.read_u8()?;
        let b: u8 = sub.read_u8()?;

        // then
        assert_eq!(a, 0xAA);
        assert_eq!(b, 0xBB);
        assert!(sub.is_empty());
        assert_eq!(cursor.pos(), 2);
        Ok(())
    }

    /// Verifies that peek_bytes does not advance the cursor.
    #[test]
    fn peek_does_not_advance() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let data: &[u8] = &[0x11, 0x22, 0x33];
        let cursor = SliceCursor::new(data);

        // when
        let peeked = cursor.peek_bytes(2);

        // then
        assert_eq!(peeked, Some(&[0x11u8, 0x22u8][..]));
        assert_eq!(cursor.pos(), 0);
        Ok(())
    }

    /// Verifies that read_array reads exactly N bytes without heap allocation.
    #[test]
    fn read_array_exact() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let data: &[u8] = &[0xDE, 0xAD, 0xBE, 0xEF];
        let mut cursor = SliceCursor::new(data);

        // when
        let arr: [u8; 4] = cursor.read_array()?;

        // then
        assert_eq!(arr, [0xDE, 0xAD, 0xBE, 0xEF]);
        Ok(())
    }

    /// Verifies that skip advances past the requested bytes.
    #[test]
    fn skip_advances_position() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let data: &[u8] = &[0x00, 0x00, 0xFF];
        let mut cursor = SliceCursor::new(data);

        // when
        cursor.skip(2)?;
        let b: u8 = cursor.read_u8()?;

        // then
        assert_eq!(b, 0xFF);
        Ok(())
    }

    /// Verifies that from_offset starts at the correct position.
    #[test]
    fn from_offset_starts_at_position() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let data: &[u8] = &[0x00, 0x00, 0x42];

        // when
        let mut cursor = SliceCursor::from_offset(data, 2)?;
        let b: u8 = cursor.read_u8()?;

        // then
        assert_eq!(b, 0x42);
        Ok(())
    }
}
