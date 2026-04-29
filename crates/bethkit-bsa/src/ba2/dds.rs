// SPDX-License-Identifier: Apache-2.0
//!
//! DDS (DirectDraw Surface) header constants and builder used when
//! reassembling textures extracted from BA2 DX10 archives.

use bitflags::bitflags;

/// Magic bytes at the start of every DDS file (`"DDS "`).
pub const DDS_MAGIC: [u8; 4] = *b"DDS ";

bitflags! {
    /// Flags for the `dwFlags` field of the DDS header.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct DdsHeaderFlags: u32 {
        /// Capabilities field is present.
        const CAPS        = 0x0000_0001;
        /// Height field is present.
        const HEIGHT      = 0x0000_0002;
        /// Width field is present.
        const WIDTH       = 0x0000_0004;
        /// Pitch-or-linear-size is a row pitch.
        const PITCH       = 0x0000_0008;
        /// Pixel format is present.
        const PIXELFORMAT = 0x0000_1000;
        /// Mip-map count is present.
        const MIPMAPCOUNT = 0x0002_0000;
        /// Pitch-or-linear-size is a linear size.
        const LINEARSIZE  = 0x0008_0000;
    }
}

bitflags! {
    /// Flags for the `dwCaps` field of the DDS header.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct DdsCaps: u32 {
        /// Surface is not a simple texture.
        const COMPLEX = 0x0000_0008;
        /// Surface is used as a texture.
        const TEXTURE = 0x0000_1000;
        /// Surface contains mipmaps.
        const MIPMAP  = 0x0040_0000;
    }
}

bitflags! {
    /// Flags for the `dwCaps2` field of the DDS header.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct DdsCaps2: u32 {
        /// Surface is a cube map.
        const CUBEMAP   = 0x0000_0200;
        /// Cube map positive-X face is present.
        const POSITIVEX = 0x0000_0400;
        /// Cube map negative-X face is present.
        const NEGATIVEX = 0x0000_0800;
        /// Cube map positive-Y face is present.
        const POSITIVEY = 0x0000_1000;
        /// Cube map negative-Y face is present.
        const NEGATIVEY = 0x0000_2000;
        /// Cube map positive-Z face is present.
        const POSITIVEZ = 0x0000_4000;
        /// Cube map negative-Z face is present.
        const NEGATIVEZ = 0x0000_8000;
    }
}

bitflags! {
    /// Flags for the `dwFlags` field of the `DDS_PIXELFORMAT` structure.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct DdsPixelFormatFlags: u32 {
        /// Alpha channel is present.
        const ALPHAPIXELS = 0x0000_0001;
        /// Alpha-only surface.
        const ALPHA       = 0x0000_0002;
        /// A FourCC code is present.
        const FOURCC      = 0x0000_0004;
        /// Uncompressed RGB.
        const RGB         = 0x0000_0040;
        /// Luminance (greyscale) surface.
        const LUMINANCE   = 0x0002_0000;
    }
}

/// `resourceDimension` value for a 2D texture.
pub const DDS_DIMENSION_TEXTURE2D: u32 = 3;
/// `miscFlags` value indicating a cube map.
pub const DDS_RESOURCE_MISC_TEXTURECUBE: u32 = 4;

/// FourCC codes used in the pixel-format descriptor.
pub const FOURCC_DXT1: [u8; 4] = *b"DXT1";
/// FourCC code for BC2 (DXT3).
pub const FOURCC_DXT3: [u8; 4] = *b"DXT3";
/// FourCC code for BC3 (DXT5).
pub const FOURCC_DXT5: [u8; 4] = *b"DXT5";
/// FourCC code for BC4 unsigned (ATI1).
pub const FOURCC_BC4U: [u8; 4] = *b"ATI1";
/// FourCC code for BC4 signed.
pub const FOURCC_BC4S: [u8; 4] = *b"BC4S";
/// FourCC code for BC5 unsigned (ATI2).
pub const FOURCC_BC5U: [u8; 4] = *b"ATI2";
/// FourCC code for BC5 signed.
pub const FOURCC_BC5S: [u8; 4] = *b"BC5S";
/// FourCC code signalling a DX10 extension header follows.
pub const FOURCC_DX10: [u8; 4] = *b"DX10";

/// DXGI format values relevant to BSA/BA2 textures.
// NOTE: non_camel_case_types needed because DXGI format names use
// NOTE: underscored SCREAMING_SNAKE_CASE by convention (e.g. BC1_UNORM);
// NOTE: dead_code allowed because not all variants are actively matched yet.
#[allow(non_camel_case_types, dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DxgiFormat {
    /// BC1 unorm (DXT1).
    BC1_UNORM = 71,
    /// BC2 unorm (DXT3).
    BC2_UNORM = 74,
    /// BC3 unorm (DXT5).
    BC3_UNORM = 77,
    /// BC4 unorm (ATI1/one channel, unsigned).
    BC4_UNORM = 80,
    /// BC4 snorm (one channel, signed).
    BC4_SNORM = 81,
    /// BC5 unorm (ATI2/two channel, unsigned).
    BC5_UNORM = 83,
    /// BC5 snorm (two channel, signed).
    BC5_SNORM = 84,
    /// BC7 unorm.
    BC7_UNORM = 98,
    /// 32-bit RGBA (8 bits per channel, unsigned).
    R8G8B8A8_UNORM = 28,
    /// 32-bit BGRA (8 bits per channel, unsigned).
    B8G8R8A8_UNORM = 87,
    /// 32-bit BGRX (no alpha).
    B8G8R8X8_UNORM = 88,
    /// 16-bit packed RGB (5-6-5).
    B5G6R5_UNORM = 85,
    /// 16-bit packed RGBA (5-5-5-1).
    B5G5R5A1_UNORM = 86,
    /// 16-bit RG (luminance + alpha).
    R8G8_UNORM = 49,
    /// 8-bit alpha-only.
    A8_UNORM = 65,
    /// 8-bit luminance (red channel only).
    R8_UNORM = 61,
    /// 8-bit luminance (signed).
    R8_SNORM = 63,
    /// 8-bit luminance (uint).
    R8_UINT = 62,
    /// Unknown / not handled.
    Unknown = 0,
}

impl From<u8> for DxgiFormat {
    fn from(v: u8) -> Self {
        match v {
            71 => Self::BC1_UNORM,
            74 => Self::BC2_UNORM,
            77 => Self::BC3_UNORM,
            80 => Self::BC4_UNORM,
            81 => Self::BC4_SNORM,
            83 => Self::BC5_UNORM,
            84 => Self::BC5_SNORM,
            98 => Self::BC7_UNORM,
            28 => Self::R8G8B8A8_UNORM,
            87 => Self::B8G8R8A8_UNORM,
            88 => Self::B8G8R8X8_UNORM,
            85 => Self::B5G6R5_UNORM,
            86 => Self::B5G5R5A1_UNORM,
            49 => Self::R8G8_UNORM,
            65 => Self::A8_UNORM,
            61 => Self::R8_UNORM,
            63 => Self::R8_SNORM,
            62 => Self::R8_UINT,
            _ => Self::Unknown,
        }
    }
}

/// Builds the DDS file header (128 bytes, or 148 bytes if a DX10 extension
/// header is required) for a BA2 texture.
///
/// # Arguments
///
/// * `width`       - Texture width in pixels.
/// * `height`      - Texture height in pixels.
/// * `num_mips`    - Number of mip-map levels.
/// * `dxgi_raw`    - Raw DXGI format byte as stored in the BA2.
/// * `cube_maps`   - Value of the `CubeMaps` field from the BA2 record.
///
/// # Returns
///
/// A `Vec<u8>` containing the header bytes to prepend to the texture data.
pub fn build_dds_header(
    width: u16,
    height: u16,
    num_mips: u8,
    dxgi_raw: u8,
    cube_maps: u16,
) -> Vec<u8> {
    let fmt = DxgiFormat::from(dxgi_raw);

    // Determine the pixel-format / FourCC.
    #[derive(Debug)]
    enum PixelFmt {
        FourCC([u8; 4]),
        Dx10,
        Rgb {
            flags: DdsPixelFormatFlags,
            bit_count: u32,
            r: u32,
            g: u32,
            b: u32,
            a: u32,
        },
        Luminance {
            flags: DdsPixelFormatFlags,
            bit_count: u32,
            r: u32,
            a: u32,
        },
        Alpha {
            bit_count: u32,
            a: u32,
        },
    }

    let (ddsd_extra, pitch_or_linear, pixel_fmt) = match fmt {
        DxgiFormat::BC1_UNORM => (
            DdsHeaderFlags::LINEARSIZE,
            (width as u32 * height as u32 / 2).max(8),
            PixelFmt::FourCC(FOURCC_DXT1),
        ),
        DxgiFormat::BC2_UNORM => (
            DdsHeaderFlags::LINEARSIZE,
            (width as u32 * height as u32).max(16),
            PixelFmt::FourCC(FOURCC_DXT3),
        ),
        DxgiFormat::BC3_UNORM => (
            DdsHeaderFlags::LINEARSIZE,
            (width as u32 * height as u32).max(16),
            PixelFmt::FourCC(FOURCC_DXT5),
        ),
        DxgiFormat::BC4_UNORM => (
            DdsHeaderFlags::LINEARSIZE,
            (width as u32 * height as u32 / 2).max(8),
            PixelFmt::FourCC(FOURCC_BC4U),
        ),
        DxgiFormat::BC4_SNORM => (
            DdsHeaderFlags::LINEARSIZE,
            (width as u32 * height as u32 / 2).max(8),
            PixelFmt::FourCC(FOURCC_BC4S),
        ),
        DxgiFormat::BC5_UNORM => (
            DdsHeaderFlags::LINEARSIZE,
            (width as u32 * height as u32).max(16),
            PixelFmt::FourCC(FOURCC_BC5U),
        ),
        DxgiFormat::BC5_SNORM => (
            DdsHeaderFlags::LINEARSIZE,
            (width as u32 * height as u32).max(16),
            PixelFmt::FourCC(FOURCC_BC5S),
        ),
        DxgiFormat::BC7_UNORM => (
            DdsHeaderFlags::LINEARSIZE,
            (width as u32 * height as u32).max(16),
            PixelFmt::Dx10,
        ),
        DxgiFormat::R8G8B8A8_UNORM => (
            DdsHeaderFlags::PITCH,
            width as u32 * 4,
            PixelFmt::Rgb {
                flags: DdsPixelFormatFlags::RGB | DdsPixelFormatFlags::ALPHAPIXELS,
                bit_count: 32,
                r: 0x0000_00FF,
                g: 0x0000_FF00,
                b: 0x00FF_0000,
                a: 0xFF00_0000,
            },
        ),
        DxgiFormat::B8G8R8A8_UNORM => (
            DdsHeaderFlags::PITCH,
            width as u32 * 4,
            PixelFmt::Rgb {
                flags: DdsPixelFormatFlags::RGB | DdsPixelFormatFlags::ALPHAPIXELS,
                bit_count: 32,
                r: 0x00FF_0000,
                g: 0x0000_FF00,
                b: 0x0000_00FF,
                a: 0xFF00_0000,
            },
        ),
        DxgiFormat::B8G8R8X8_UNORM => (
            DdsHeaderFlags::PITCH,
            width as u32 * 4,
            PixelFmt::Rgb {
                flags: DdsPixelFormatFlags::RGB,
                bit_count: 32,
                r: 0x00FF_0000,
                g: 0x0000_FF00,
                b: 0x0000_00FF,
                a: 0,
            },
        ),
        DxgiFormat::B5G6R5_UNORM => (
            DdsHeaderFlags::PITCH,
            width as u32 * 2,
            PixelFmt::Rgb {
                flags: DdsPixelFormatFlags::RGB,
                bit_count: 16,
                r: 0x0000_F800,
                g: 0x0000_07E0,
                b: 0x0000_001F,
                a: 0,
            },
        ),
        DxgiFormat::B5G5R5A1_UNORM => (
            DdsHeaderFlags::PITCH,
            width as u32 * 2,
            PixelFmt::Rgb {
                flags: DdsPixelFormatFlags::RGB | DdsPixelFormatFlags::ALPHAPIXELS,
                bit_count: 16,
                r: 0x0000_7C00,
                g: 0x0000_03E0,
                b: 0x0000_001F,
                a: 0x0000_8000,
            },
        ),
        DxgiFormat::R8G8_UNORM => (
            DdsHeaderFlags::PITCH,
            width as u32 * 2,
            PixelFmt::Luminance {
                flags: DdsPixelFormatFlags::LUMINANCE | DdsPixelFormatFlags::ALPHAPIXELS,
                bit_count: 16,
                r: 0x0000_00FF,
                a: 0x0000_FF00,
            },
        ),
        DxgiFormat::A8_UNORM => (
            DdsHeaderFlags::PITCH,
            width as u32,
            PixelFmt::Alpha {
                bit_count: 8,
                a: 0x0000_00FF,
            },
        ),
        DxgiFormat::R8_UNORM => (
            DdsHeaderFlags::PITCH,
            width as u32,
            PixelFmt::Luminance {
                flags: DdsPixelFormatFlags::LUMINANCE,
                bit_count: 8,
                r: 0x0000_00FF,
                a: 0,
            },
        ),
        DxgiFormat::R8_SNORM | DxgiFormat::R8_UINT => {
            (DdsHeaderFlags::PITCH, width as u32, PixelFmt::Dx10)
        }
        _ => (DdsHeaderFlags::LINEARSIZE, 0, PixelFmt::Dx10),
    };

    let needs_dx10 = matches!(pixel_fmt, PixelFmt::Dx10);

    // Cube-map handling.
    let is_cube = cube_maps == 2049;
    let (dwcaps, dwcaps2) = if is_cube {
        (
            DdsCaps::TEXTURE | DdsCaps::COMPLEX,
            DdsCaps2::CUBEMAP
                | DdsCaps2::POSITIVEX
                | DdsCaps2::NEGATIVEX
                | DdsCaps2::POSITIVEY
                | DdsCaps2::NEGATIVEY
                | DdsCaps2::POSITIVEZ
                | DdsCaps2::NEGATIVEZ,
        )
    } else if num_mips > 1 {
        (DdsCaps::TEXTURE | DdsCaps::MIPMAP | DdsCaps::COMPLEX, DdsCaps2::empty())
    } else {
        (DdsCaps::TEXTURE, DdsCaps2::empty())
    };

    let mut flags = DdsHeaderFlags::CAPS
        | DdsHeaderFlags::PIXELFORMAT
        | DdsHeaderFlags::WIDTH
        | DdsHeaderFlags::HEIGHT
        | DdsHeaderFlags::MIPMAPCOUNT
        | ddsd_extra;

    // Build pixel-format block (32 bytes).
    let (pf_flags, pf_fourcc, pf_rgb_bit_count, pf_r, pf_g, pf_b, pf_a) = match &pixel_fmt {
        PixelFmt::FourCC(cc) => (
            DdsPixelFormatFlags::FOURCC,
            *cc,
            0u32,
            0u32,
            0u32,
            0u32,
            0u32,
        ),
        PixelFmt::Dx10 => (DdsPixelFormatFlags::FOURCC, FOURCC_DX10, 0, 0, 0, 0, 0),
        PixelFmt::Rgb {
            flags,
            bit_count,
            r,
            g,
            b,
            a,
        } => (*flags, [0u8; 4], *bit_count, *r, *g, *b, *a),
        PixelFmt::Luminance {
            flags,
            bit_count,
            r,
            a,
        } => (*flags, [0u8; 4], *bit_count, *r, 0, 0, *a),
        PixelFmt::Alpha { bit_count, a } => {
            (DdsPixelFormatFlags::ALPHA, [0u8; 4], *bit_count, 0, 0, 0, *a)
        }
    };

    // dwFlags is always DDSD_MIPMAPCOUNT; remove if 0 mips (shouldn't happen).
    if num_mips == 0 {
        flags.remove(DdsHeaderFlags::MIPMAPCOUNT);
    }

    // Write the 128-byte DDS header as raw little-endian bytes.
    let mut out: Vec<u8> = Vec::with_capacity(if needs_dx10 { 148 } else { 128 });

    // Magic "DDS "
    out.extend_from_slice(&DDS_MAGIC);
    // dwSize = 124 (= SizeOf minus the 4-byte magic)
    out.extend_from_slice(&124u32.to_le_bytes());
    // dwFlags
    out.extend_from_slice(&flags.bits().to_le_bytes());
    // dwHeight
    out.extend_from_slice(&(height as u32).to_le_bytes());
    // dwWidth
    out.extend_from_slice(&(width as u32).to_le_bytes());
    // dwPitchOrLinearSize
    out.extend_from_slice(&pitch_or_linear.to_le_bytes());
    // dwDepth = 1
    out.extend_from_slice(&1u32.to_le_bytes());
    // dwMipMapCount
    out.extend_from_slice(&(num_mips as u32).to_le_bytes());
    // dwReserved1[11]
    out.extend_from_slice(&[0u8; 44]);

    // Pixel format (ddspf) — 32 bytes:
    out.extend_from_slice(&32u32.to_le_bytes()); // ddspf.dwSize
    out.extend_from_slice(&pf_flags.bits().to_le_bytes());
    out.extend_from_slice(&pf_fourcc);
    out.extend_from_slice(&pf_rgb_bit_count.to_le_bytes());
    out.extend_from_slice(&pf_r.to_le_bytes());
    out.extend_from_slice(&pf_g.to_le_bytes());
    out.extend_from_slice(&pf_b.to_le_bytes());
    out.extend_from_slice(&pf_a.to_le_bytes());

    // dwCaps, dwCaps2, dwCaps3, dwCaps4, dwReserved2
    out.extend_from_slice(&dwcaps.bits().to_le_bytes());
    out.extend_from_slice(&dwcaps2.bits().to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // dwCaps3
    out.extend_from_slice(&0u32.to_le_bytes()); // dwCaps4
    out.extend_from_slice(&0u32.to_le_bytes()); // dwReserved2

    debug_assert_eq!(out.len(), 128);

    // DX10 extension header (20 bytes), only when needed.
    if needs_dx10 {
        let misc_flags: u32 = if is_cube {
            DDS_RESOURCE_MISC_TEXTURECUBE
        } else {
            0
        };
        out.extend_from_slice(&(dxgi_raw as u32).to_le_bytes()); // dxgiFormat
        out.extend_from_slice(&DDS_DIMENSION_TEXTURE2D.to_le_bytes()); // resourceDimension
        out.extend_from_slice(&misc_flags.to_le_bytes()); // miscFlags
        out.extend_from_slice(&1u32.to_le_bytes()); // arraySize
        out.extend_from_slice(&0u32.to_le_bytes()); // miscFlags2

        debug_assert_eq!(out.len(), 148);
    }

    out
}
