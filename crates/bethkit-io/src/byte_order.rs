// SPDX-License-Identifier: Apache-2.0
//!
//! Compile-time byte order selection for binary readers and writers.
//!
//! Most Bethesda formats are little-endian, but some (for example Skyrim
//! Papyrus `.pex` files) are big-endian. Parsers take the byte order as a type
//! parameter implementing [`ByteOrder`], so every read is monomorphised to a
//! single conversion without a runtime branch. [`Endianness`] is the runtime
//! counterpart used to select the monomorphised code path once per file.

use std::fmt;

/// A byte order for converting fixed-size integers and floats.
///
/// Implemented by the zero-sized marker types [`LittleEndian`] and
/// [`BigEndian`]. Floats are converted through their raw bit pattern, so every
/// value, including NaN payloads, round-trips bit-exactly.
pub trait ByteOrder: Copy + fmt::Debug + Send + Sync + 'static {
    /// The runtime value describing this byte order.
    const ENDIANNESS: Endianness;

    /// Decodes a `u16` from its byte representation.
    fn u16_from(bytes: [u8; 2]) -> u16;

    /// Decodes a `u32` from its byte representation.
    fn u32_from(bytes: [u8; 4]) -> u32;

    /// Decodes a `u64` from its byte representation.
    fn u64_from(bytes: [u8; 8]) -> u64;

    /// Encodes a `u16` into its byte representation.
    fn u16_to(value: u16) -> [u8; 2];

    /// Encodes a `u32` into its byte representation.
    fn u32_to(value: u32) -> [u8; 4];

    /// Encodes a `u64` into its byte representation.
    fn u64_to(value: u64) -> [u8; 8];
}

/// Little-endian byte order, used by most Bethesda formats.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct LittleEndian;

/// Big-endian byte order, used for example by Skyrim `.pex` files.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct BigEndian;

impl ByteOrder for LittleEndian {
    const ENDIANNESS: Endianness = Endianness::Little;

    #[inline]
    fn u16_from(bytes: [u8; 2]) -> u16 {
        u16::from_le_bytes(bytes)
    }

    #[inline]
    fn u32_from(bytes: [u8; 4]) -> u32 {
        u32::from_le_bytes(bytes)
    }

    #[inline]
    fn u64_from(bytes: [u8; 8]) -> u64 {
        u64::from_le_bytes(bytes)
    }

    #[inline]
    fn u16_to(value: u16) -> [u8; 2] {
        value.to_le_bytes()
    }

    #[inline]
    fn u32_to(value: u32) -> [u8; 4] {
        value.to_le_bytes()
    }

    #[inline]
    fn u64_to(value: u64) -> [u8; 8] {
        value.to_le_bytes()
    }
}

impl ByteOrder for BigEndian {
    const ENDIANNESS: Endianness = Endianness::Big;

    #[inline]
    fn u16_from(bytes: [u8; 2]) -> u16 {
        u16::from_be_bytes(bytes)
    }

    #[inline]
    fn u32_from(bytes: [u8; 4]) -> u32 {
        u32::from_be_bytes(bytes)
    }

    #[inline]
    fn u64_from(bytes: [u8; 8]) -> u64 {
        u64::from_be_bytes(bytes)
    }

    #[inline]
    fn u16_to(value: u16) -> [u8; 2] {
        value.to_be_bytes()
    }

    #[inline]
    fn u32_to(value: u32) -> [u8; 4] {
        value.to_be_bytes()
    }

    #[inline]
    fn u64_to(value: u64) -> [u8; 8] {
        value.to_be_bytes()
    }
}

/// Runtime description of a byte order.
///
/// Use it to pick the [`ByteOrder`] type parameter once per file, for example
/// after inspecting a magic number, and then run fully monomorphised code.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Endianness {
    /// Least significant byte first.
    Little,
    /// Most significant byte first.
    Big,
}

impl fmt::Display for Endianness {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Little => f.write_str("little-endian"),
            Self::Big => f.write_str("big-endian"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that both byte orders encode and decode in the expected order.
    #[test]
    fn byte_orders_convert_in_expected_order() -> Result<(), Box<dyn std::error::Error>> {
        // when
        let little: [u8; 4] = LittleEndian::u32_to(0x0102_0304);
        let big: [u8; 4] = BigEndian::u32_to(0x0102_0304);

        // then
        assert_eq!(little, [0x04, 0x03, 0x02, 0x01]);
        assert_eq!(big, [0x01, 0x02, 0x03, 0x04]);
        assert_eq!(LittleEndian::u32_from(little), 0x0102_0304);
        assert_eq!(BigEndian::u32_from(big), 0x0102_0304);
        assert_eq!(BigEndian::u16_from([0xFA, 0x57]), 0xFA57);
        assert_eq!(
            BigEndian::u64_from(BigEndian::u64_to(u64::MAX - 1)),
            u64::MAX - 1
        );
        assert_eq!(BigEndian::ENDIANNESS, Endianness::Big);
        assert_eq!(Endianness::Little.to_string(), "little-endian");
        Ok(())
    }
}
