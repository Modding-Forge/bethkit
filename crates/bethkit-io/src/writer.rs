// SPDX-License-Identifier: Apache-2.0
//!
//! Append-only byte writer with compile-time byte order.
//!
//! [`ByteWriter`] is the write-side counterpart of [`crate::SliceCursor`]. It
//! supports reserving fixed-size slots whose value is only known later, such as
//! the size of a block that is written after its header, and patching them in
//! place without moving any data.

use std::marker::PhantomData;

use crate::byte_order::ByteOrder;
use crate::error::{IoError, Result};

/// A reserved `u32` position inside a [`ByteWriter`], filled in later with
/// [`ByteWriter::patch_u32`].
///
/// Slots are only produced by [`ByteWriter::reserve_u32`] and stay valid for
/// the writer that created them, because the writer never shrinks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct U32Slot(usize);

impl U32Slot {
    /// Returns the byte offset of the slot inside its writer.
    pub fn offset(self) -> usize {
        self.0
    }
}

/// An append-only buffer that encodes values in byte order `O`.
#[derive(Clone, Debug, Default)]
pub struct ByteWriter<O: ByteOrder> {
    buf: Vec<u8>,
    order: PhantomData<O>,
}

impl<O: ByteOrder> ByteWriter<O> {
    /// Creates an empty writer.
    pub fn new() -> Self {
        Self::with_capacity(0)
    }

    /// Creates an empty writer that can hold `capacity` bytes without
    /// reallocating.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            buf: Vec::with_capacity(capacity),
            order: PhantomData,
        }
    }

    /// Returns the number of bytes written so far.
    pub fn len(&self) -> usize {
        self.buf.len()
    }

    /// Returns `true` if nothing has been written yet.
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    /// Returns the bytes written so far.
    pub fn as_slice(&self) -> &[u8] {
        &self.buf
    }

    /// Consumes the writer and returns the written bytes.
    pub fn into_inner(self) -> Vec<u8> {
        self.buf
    }

    /// Reserves space for at least `additional` more bytes.
    pub fn reserve(&mut self, additional: usize) {
        self.buf.reserve(additional);
    }

    /// Appends a `u8`.
    #[inline]
    pub fn write_u8(&mut self, value: u8) {
        self.buf.push(value);
    }

    /// Appends a `u16` in byte order `O`.
    #[inline]
    pub fn write_u16(&mut self, value: u16) {
        self.buf.extend_from_slice(&O::u16_to(value));
    }

    /// Appends an `i16` in byte order `O`.
    #[inline]
    pub fn write_i16(&mut self, value: i16) {
        self.write_u16(u16::from_ne_bytes(value.to_ne_bytes()));
    }

    /// Appends a `u32` in byte order `O`.
    #[inline]
    pub fn write_u32(&mut self, value: u32) {
        self.buf.extend_from_slice(&O::u32_to(value));
    }

    /// Appends an `i32` in byte order `O`.
    #[inline]
    pub fn write_i32(&mut self, value: i32) {
        self.write_u32(u32::from_ne_bytes(value.to_ne_bytes()));
    }

    /// Appends a `u64` in byte order `O`.
    #[inline]
    pub fn write_u64(&mut self, value: u64) {
        self.buf.extend_from_slice(&O::u64_to(value));
    }

    /// Appends an `f32` in byte order `O` through its raw bit pattern, so the
    /// output is bit-identical to the input value.
    #[inline]
    pub fn write_f32(&mut self, value: f32) {
        self.write_u32(value.to_bits());
    }

    /// Appends raw bytes unchanged.
    #[inline]
    pub fn write_bytes(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    /// Appends a `u16` length in byte order `O` followed by `bytes`.
    ///
    /// # Errors
    ///
    /// Returns [`IoError::LengthOverflow`] if `bytes` is longer than
    /// `u16::MAX`; nothing is written in that case.
    pub fn write_u16_prefixed(&mut self, bytes: &[u8]) -> Result<()> {
        let len: u16 = u16::try_from(bytes.len()).map_err(|_| IoError::LengthOverflow {
            len: bytes.len(),
            max: usize::from(u16::MAX),
        })?;
        self.write_u16(len);
        self.write_bytes(bytes);
        Ok(())
    }

    /// Appends a zeroed `u32` placeholder and returns its slot.
    pub fn reserve_u32(&mut self) -> U32Slot {
        let slot = U32Slot(self.buf.len());
        self.write_u32(0);
        slot
    }

    /// Overwrites a reserved `u32` slot with `value`.
    ///
    /// # Errors
    ///
    /// Returns [`IoError::InvalidSlot`] if the slot does not lie inside the
    /// written data, which only happens for a slot from another writer.
    pub fn patch_u32(&mut self, slot: U32Slot, value: u32) -> Result<()> {
        let target: &mut [u8] = slot
            .0
            .checked_add(4)
            .and_then(|end| self.buf.get_mut(slot.0..end))
            .ok_or(IoError::InvalidSlot { offset: slot.0 })?;
        target.copy_from_slice(&O::u32_to(value));
        Ok(())
    }

    /// Fills a reserved slot with the number of bytes written since the slot
    /// started, including the slot itself.
    ///
    /// This is the common "block size includes its own size field" layout.
    ///
    /// # Errors
    ///
    /// Returns [`IoError::InvalidSlot`] if the slot does not belong to this
    /// writer, or [`IoError::LengthOverflow`] if the block exceeds `u32::MAX`
    /// bytes.
    pub fn patch_u32_with_size_since(&mut self, slot: U32Slot) -> Result<()> {
        let size: usize = self
            .buf
            .len()
            .checked_sub(slot.0)
            .ok_or(IoError::InvalidSlot { offset: slot.0 })?;
        let size: u32 = u32::try_from(size).map_err(|_| IoError::LengthOverflow {
            len: size,
            max: usize::try_from(u32::MAX).unwrap_or(usize::MAX),
        })?;
        self.patch_u32(slot, size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::byte_order::{BigEndian, LittleEndian};
    use crate::cursor::SliceCursor;

    /// Verifies that every written value reads back identically in both orders.
    #[test]
    fn values_round_trip_through_cursor() -> std::result::Result<(), Box<dyn std::error::Error>> {
        fn round_trip<O: ByteOrder>() -> Result<()> {
            // given
            let mut writer: ByteWriter<O> = ByteWriter::with_capacity(32);
            writer.write_u8(7);
            writer.write_u16(0xFA57);
            writer.write_i16(-2);
            writer.write_u32(0xFA57_C0DE);
            writer.write_i32(-3);
            writer.write_u64(u64::MAX - 5);
            writer.write_f32(f32::from_bits(0x7FA0_0001));
            writer.write_u16_prefixed(b"name")?;
            let bytes: Vec<u8> = writer.into_inner();

            // when
            let mut cursor = SliceCursor::new(&bytes);

            // then
            assert_eq!(cursor.read_u8()?, 7);
            assert_eq!(cursor.read_u16_as::<O>()?, 0xFA57);
            assert_eq!(cursor.read_i16_as::<O>()?, -2);
            assert_eq!(cursor.read_u32_as::<O>()?, 0xFA57_C0DE);
            assert_eq!(cursor.read_i32_as::<O>()?, -3);
            assert_eq!(cursor.read_u64_as::<O>()?, u64::MAX - 5);
            assert_eq!(cursor.read_f32_as::<O>()?.to_bits(), 0x7FA0_0001);
            assert_eq!(cursor.read_u16_prefixed_as::<O>()?, b"name");
            assert!(cursor.is_empty());
            Ok(())
        }

        round_trip::<LittleEndian>()?;
        round_trip::<BigEndian>()?;
        Ok(())
    }

    /// Verifies that a size slot covers itself and everything written after it.
    #[test]
    fn size_slot_includes_itself() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let mut writer: ByteWriter<BigEndian> = ByteWriter::new();
        writer.write_u8(0xAA);
        let slot: U32Slot = writer.reserve_u32();
        writer.write_bytes(&[1, 2, 3]);

        // when
        writer.patch_u32_with_size_since(slot)?;

        // then
        assert_eq!(slot.offset(), 1);
        assert_eq!(writer.as_slice(), &[0xAA, 0, 0, 0, 7, 1, 2, 3]);
        Ok(())
    }

    /// Verifies that oversized strings and foreign slots are rejected.
    #[test]
    fn invalid_writes_are_rejected() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let mut writer: ByteWriter<LittleEndian> = ByteWriter::new();
        let oversized: Vec<u8> = vec![0; usize::from(u16::MAX) + 1];
        let mut other: ByteWriter<LittleEndian> = ByteWriter::new();
        other.write_bytes(&[0; 16]);
        let foreign: U32Slot = other.reserve_u32();

        // when
        let too_long = writer.write_u16_prefixed(&oversized);
        let bad_patch = writer.patch_u32(foreign, 1);

        // then
        assert!(matches!(
            too_long,
            Err(IoError::LengthOverflow {
                len: 65536,
                max: 65535
            })
        ));
        assert!(matches!(
            bad_patch,
            Err(IoError::InvalidSlot { offset: 16 })
        ));
        assert!(writer.is_empty());
        Ok(())
    }
}
