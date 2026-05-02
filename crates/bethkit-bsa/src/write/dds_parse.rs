// SPDX-License-Identifier: Apache-2.0
//! DDS file parser for use by the BA2 DX10 writer.
//!
//! This module parses the header of a DDS file to extract the metadata needed
//! to write a BA2 DX10 texture record (width, height, mip count, DXGI format,
//! cube map flag) and to slice the pixel data into per-mip byte ranges.
//!
//! Supported pixel formats:
//! - BC1–BC7 (block-compressed)
//! - R8G8B8A8_UNORM, B8G8R8A8_UNORM, B8G8R8X8_UNORM (32-bpp)
//! - R8G8_UNORM (16-bpp)
//! - A8_UNORM, R8_UNORM, R8_SNORM, R8_UINT (8-bpp)
//! - B5G6R5_UNORM, B5G5R5A1_UNORM (16-bpp packed)

use crate::ba2::dds::DxgiFormat;
use crate::error::{BsaError, Result};

/// The four bytes of a DDS magic header.
const DDS_MAGIC: [u8; 4] = *b"DDS ";
/// FourCC signalling a DX10 extension header.
const FOURCC_DX10: [u8; 4] = *b"DX10";

/// Parsed DDS metadata required by the BA2 DX10 writer.
#[derive(Debug, Clone)]
pub struct DdsInfo {
    /// Texture width in pixels.
    pub width: u16,
    /// Texture height in pixels.
    pub height: u16,
    /// Number of mip levels.
    pub num_mips: u8,
    /// DXGI format code (as stored in the BA2 record).
    pub dxgi_format: u8,
    /// Cube map flag: `2049` for cube maps, `0` otherwise.
    pub cube_maps: u16,
    /// Raw mip pixel data (all mip levels concatenated, after the DDS header).
    pub mip_data: Vec<u8>,
    /// Byte offset and size of each mip level within `mip_data`.
    pub mip_ranges: Vec<(usize, usize)>,
}

/// Parses a DDS file and returns the metadata and mip byte ranges.
///
/// # Arguments
///
/// * `dds` - Raw bytes of the DDS file (including the `"DDS "` magic).
///
/// # Returns
///
/// A [`DdsInfo`] describing the texture.
///
/// # Errors
///
/// Returns [`BsaError::InvalidDds`] for structural problems, or
/// [`BsaError::UnsupportedDxgiFormat`] for unsupported pixel formats.
pub fn parse(dds: &[u8]) -> Result<DdsInfo> {
    if dds.len() < 128 {
        return Err(BsaError::InvalidDds("DDS too short for header".into()));
    }

    if dds[..4] != DDS_MAGIC {
        return Err(BsaError::InvalidDds("not a DDS file".into()));
    }

    // DDS_HEADER starts at byte 4 and is 124 bytes.
    let h = &dds[4..128];
    // dwSize must equal 124.
    let dw_size = u32::from_le_bytes([h[0], h[1], h[2], h[3]]);
    if dw_size != 124 {
        return Err(BsaError::InvalidDds(format!(
            "DDS_HEADER.dwSize expected 124, got {dw_size}"
        )));
    }

    let height = u32::from_le_bytes([h[8], h[9], h[10], h[11]]);
    let width = u32::from_le_bytes([h[12], h[13], h[14], h[15]]);
    let mip_count_raw = u32::from_le_bytes([h[24], h[25], h[26], h[27]]);
    // dwMipMapCount == 0 means no mipmaps → treat as 1 mip.
    let num_mips = mip_count_raw.clamp(1, 255) as u8;

    // DDS_PIXELFORMAT is at byte offset 72 within h (i.e. dds[76..108]).
    let pf = &h[72..104];
    let _pf_size = u32::from_le_bytes([pf[0], pf[1], pf[2], pf[3]]);
    let pf_flags = u32::from_le_bytes([pf[4], pf[5], pf[6], pf[7]]);
    let four_cc: [u8; 4] = [pf[8], pf[9], pf[10], pf[11]];
    let rgb_bit_count = u32::from_le_bytes([pf[12], pf[13], pf[14], pf[15]]);
    let r_mask = u32::from_le_bytes([pf[16], pf[17], pf[18], pf[19]]);
    let _g_mask = u32::from_le_bytes([pf[20], pf[21], pf[22], pf[23]]);
    let _b_mask = u32::from_le_bytes([pf[24], pf[25], pf[26], pf[27]]);
    let a_mask = u32::from_le_bytes([pf[28], pf[29], pf[30], pf[31]]);

    // DDS_HEADER.dwCaps2 (cube map flags): at offset 112 in the 128-byte file header.
    let caps2 = u32::from_le_bytes([dds[112], dds[113], dds[114], dds[115]]);
    // Bit 0x200 = DDSCAPS2_CUBEMAP
    let is_cube = (caps2 & 0x0000_0200) != 0;
    let cube_maps: u16 = if is_cube { 2049 } else { 0 };

    // Determine DXGI format.
    const PF_FOURCC: u32 = 0x0000_0004;
    const PF_RGB: u32 = 0x0000_0040;
    const PF_LUMINANCE: u32 = 0x0002_0000;
    const PF_ALPHA: u32 = 0x0000_0002;

    let (dxgi_format, header_len) = if pf_flags & PF_FOURCC != 0 && four_cc == FOURCC_DX10 {
        // DX10 extension header at byte 128.
        if dds.len() < 148 {
            return Err(BsaError::InvalidDds(
                "DDS too short for DX10 extension".into(),
            ));
        }
        let dxgi_raw = u32::from_le_bytes([dds[128], dds[129], dds[130], dds[131]]);
        (dxgi_raw as u8, 148usize)
    } else if pf_flags & PF_FOURCC != 0 {
        let fmt = fourcc_to_dxgi(&four_cc)
            .ok_or_else(|| BsaError::InvalidDds(format!("unknown FourCC {:?}", four_cc)))?;
        (fmt as u8, 128usize)
    } else if pf_flags & PF_RGB != 0 {
        let fmt = uncompressed_to_dxgi(pf_flags, rgb_bit_count, r_mask, a_mask)?;
        (fmt as u8, 128usize)
    } else if pf_flags & PF_LUMINANCE != 0 {
        let fmt = luminance_to_dxgi(rgb_bit_count, a_mask)?;
        (fmt as u8, 128usize)
    } else if pf_flags & PF_ALPHA != 0 {
        (DxgiFormat::A8_UNORM as u8, 128usize)
    } else {
        return Err(BsaError::InvalidDds("cannot determine pixel format".into()));
    };

    // Validate that the format is supported for writing.
    validate_dxgi_for_write(dxgi_format)?;

    let mip_data = dds[header_len..].to_owned();
    let mip_ranges = compute_mip_ranges(width, height, num_mips, dxgi_format, &mip_data)?;

    let width16 = u16::try_from(width)
        .map_err(|_| BsaError::InvalidDds(format!("width {width} exceeds u16")))?;
    let height16 = u16::try_from(height)
        .map_err(|_| BsaError::InvalidDds(format!("height {height} exceeds u16")))?;

    Ok(DdsInfo {
        width: width16,
        height: height16,
        num_mips,
        dxgi_format,
        cube_maps,
        mip_data,
        mip_ranges,
    })
}

/// Converts a FourCC code to the corresponding [`DxgiFormat`] discriminant.
///
/// Returns `None` for unrecognised codes.
fn fourcc_to_dxgi(four_cc: &[u8; 4]) -> Option<DxgiFormat> {
    match four_cc {
        b"DXT1" => Some(DxgiFormat::BC1_UNORM),
        b"DXT3" => Some(DxgiFormat::BC2_UNORM),
        b"DXT5" => Some(DxgiFormat::BC3_UNORM),
        b"ATI1" | b"BC4U" => Some(DxgiFormat::BC4_UNORM),
        b"BC4S" => Some(DxgiFormat::BC4_SNORM),
        b"ATI2" | b"BC5U" => Some(DxgiFormat::BC5_UNORM),
        b"BC5S" => Some(DxgiFormat::BC5_SNORM),
        _ => None,
    }
}

/// Derives the DXGI format for an uncompressed (RGB-flagged) DDS.
///
/// # Errors
///
/// Returns [`BsaError::InvalidDds`] for unrecognised bit layouts.
fn uncompressed_to_dxgi(
    pf_flags: u32,
    bit_count: u32,
    r_mask: u32,
    a_mask: u32,
) -> Result<DxgiFormat> {
    const PF_ALPHA_PIXELS: u32 = 0x0000_0001;
    let has_alpha = pf_flags & PF_ALPHA_PIXELS != 0;

    match (bit_count, r_mask, has_alpha) {
        (32, 0x0000_00FF, true) => Ok(DxgiFormat::R8G8B8A8_UNORM),
        (32, 0x00FF_0000, true) if a_mask == 0xFF00_0000 => Ok(DxgiFormat::B8G8R8A8_UNORM),
        (32, 0x00FF_0000, false) => Ok(DxgiFormat::B8G8R8X8_UNORM),
        (16, 0x0000_F800, _) => Ok(DxgiFormat::B5G6R5_UNORM),
        (16, 0x0000_7C00, _) => Ok(DxgiFormat::B5G5R5A1_UNORM),
        (16, 0x0000_00FF, _) => Ok(DxgiFormat::R8G8_UNORM),
        _ => Err(BsaError::InvalidDds(format!(
            "unrecognised uncompressed format: {bit_count} bpp, r_mask={r_mask:#010x}"
        ))),
    }
}

/// Derives the DXGI format for a luminance-flagged DDS.
fn luminance_to_dxgi(bit_count: u32, a_mask: u32) -> Result<DxgiFormat> {
    match (bit_count, a_mask) {
        (8, 0) => Ok(DxgiFormat::R8_UNORM),
        (16, 0xFF00) => Ok(DxgiFormat::R8G8_UNORM),
        _ => Err(BsaError::InvalidDds(format!(
            "unrecognised luminance format: {bit_count} bpp, a_mask={a_mask:#010x}"
        ))),
    }
}

/// Returns an error if `dxgi_format` is not supported by the BA2 DX10 writer.
fn validate_dxgi_for_write(dxgi_format: u8) -> Result<()> {
    let fmt = DxgiFormat::from(dxgi_format);
    match fmt {
        DxgiFormat::Unknown => Err(BsaError::UnsupportedDxgiFormat {
            format: dxgi_format,
        }),
        _ => Ok(()),
    }
}

/// Returns the size in bytes of a single mip level.
///
/// # Arguments
///
/// * `w`     - Mip width.
/// * `h`     - Mip height.
/// * `fmt`   - Raw DXGI format byte.
///
/// # Errors
///
/// Returns [`BsaError::UnsupportedDxgiFormat`] for unknown formats.
pub fn mip_byte_size(w: u32, h: u32, fmt: u8) -> Result<usize> {
    let dxgi = DxgiFormat::from(fmt);
    let size = match dxgi {
        // Block-compressed formats.
        DxgiFormat::BC1_UNORM | DxgiFormat::BC4_UNORM | DxgiFormat::BC4_SNORM => {
            let bw = w.max(1).div_ceil(4);
            let bh = h.max(1).div_ceil(4);
            (bw * bh * 8) as usize
        }
        DxgiFormat::BC2_UNORM
        | DxgiFormat::BC3_UNORM
        | DxgiFormat::BC5_UNORM
        | DxgiFormat::BC5_SNORM
        | DxgiFormat::BC7_UNORM => {
            let bw = w.max(1).div_ceil(4);
            let bh = h.max(1).div_ceil(4);
            (bw * bh * 16) as usize
        }
        // 32-bpp uncompressed.
        DxgiFormat::R8G8B8A8_UNORM | DxgiFormat::B8G8R8A8_UNORM | DxgiFormat::B8G8R8X8_UNORM => {
            (w.max(1) * h.max(1) * 4) as usize
        }
        // 16-bpp.
        DxgiFormat::R8G8_UNORM | DxgiFormat::B5G6R5_UNORM | DxgiFormat::B5G5R5A1_UNORM => {
            (w.max(1) * h.max(1) * 2) as usize
        }
        // 8-bpp.
        DxgiFormat::A8_UNORM
        | DxgiFormat::R8_UNORM
        | DxgiFormat::R8_SNORM
        | DxgiFormat::R8_UINT => (w.max(1) * h.max(1)) as usize,
        DxgiFormat::Unknown => {
            return Err(BsaError::UnsupportedDxgiFormat { format: fmt });
        }
    };
    Ok(size)
}

/// Computes byte ranges `(offset, size)` within `mip_data` for each mip level.
///
/// # Errors
///
/// Returns an error if `mip_data` is too short or the format is unsupported.
fn compute_mip_ranges(
    width: u32,
    height: u32,
    num_mips: u8,
    dxgi_format: u8,
    mip_data: &[u8],
) -> Result<Vec<(usize, usize)>> {
    let mut ranges = Vec::with_capacity(num_mips as usize);
    let mut offset = 0usize;
    let mut w = width;
    let mut h = height;

    for _ in 0..num_mips {
        let size = mip_byte_size(w, h, dxgi_format)?;
        if offset + size > mip_data.len() {
            return Err(BsaError::InvalidDds(format!(
                "DDS mip data too short: need {} bytes at offset {offset}, \
                 have {}",
                size,
                mip_data.len()
            )));
        }
        ranges.push((offset, size));
        offset += size;
        w = (w / 2).max(1);
        h = (h / 2).max(1);
    }

    Ok(ranges)
}
