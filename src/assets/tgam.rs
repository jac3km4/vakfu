use byte::ctx::{Endianess, Len};
use byte::{BytesExt, TryRead};

/// Represents an image loaded from a TGAM format.
#[derive(Debug)]
pub struct Tgam<'a> {
    /// The physical width of the image.
    width: u16,
    /// The physical height of the image.
    height: u16,
    /// The underlying pixel data in RGBA format.
    rgba: &'a [u8],
    /// The mask data associated with the image.
    mask: &'a [u8],
    /// A value defining how the mask should be resized or mapped.
    mask_resize: u8,
}

impl<'a> Tgam<'a> {
    /// Returns the RGBA pixel data of the image.
    #[inline]
    pub fn rgba(&self) -> &'a [u8] {
        self.rgba
    }

    /// Returns the packed alpha mask associated with the image.
    #[inline]
    pub fn mask(&self) -> &'a [u8] {
        self.mask
    }

    /// Returns the mask resize factor (`1` when the image is not resized).
    #[inline]
    pub fn mask_resize(&self) -> u8 {
        self.mask_resize
    }

    /// Returns the width of the image, rounded up to the nearest power of two.
    #[inline]
    pub fn padded_width(&self) -> u32 {
        round_up_to_power_of_two(self.width.into())
    }

    /// Returns the height of the image, rounded up to the nearest power of two.
    #[inline]
    pub fn padded_height(&self) -> u32 {
        round_up_to_power_of_two(self.height.into())
    }

    /// Returns the physical (unpadded) width of the image.
    #[inline]
    pub fn physical_width(&self) -> u16 {
        self.width
    }

    /// Returns the physical (unpadded) height of the image.
    #[inline]
    pub fn physical_height(&self) -> u16 {
        self.height
    }
}

/// The packed alpha mask attached to a TGAM texture.
///
/// Mirrors `AlphaMask`: bits are MSB-first and each `resize`-sized block maps
/// to one bit. Used for precise hit-testing (`DisplayedScreenElement.fineHitTest`).
#[derive(Debug, Clone)]
pub struct TgamMask {
    data: Vec<u8>,
    resize: u8,
    width: u16,
    height: u16,
}

impl TgamMask {
    /// Builds a mask from raw packed bits.
    pub fn new(data: Vec<u8>, resize: u8, width: u16, height: u16) -> Self {
        Self {
            data,
            resize: resize.max(1),
            width,
            height,
        }
    }

    /// Builds a mask from a parsed TGAM, or `None` when the texture has none.
    pub fn from_tgam(tgam: &Tgam<'_>) -> Option<Self> {
        if tgam.mask().is_empty() {
            return None;
        }
        Some(Self::new(
            tgam.mask().to_vec(),
            tgam.mask_resize(),
            tgam.physical_width(),
            tgam.physical_height(),
        ))
    }

    /// Returns whether the mask bit at `(x, y)` is set.
    pub fn contains(&self, x: u32, y: u32) -> bool {
        if x >= u32::from(self.width) || y >= u32::from(self.height) {
            return false;
        }
        let resize = u32::from(self.resize);
        let mask_width = u32::from(self.width).div_ceil(resize);
        let index = (y / resize) * mask_width + (x / resize);
        let byte = self.data.get((index / 8) as usize).copied().unwrap_or(0);
        byte & (1 << (7 - (index % 8))) != 0
    }
}

impl<'a, C: Copy + Endianess> TryRead<'a, C> for Tgam<'a> {
    fn try_read(bytes: &'a [u8], ctx: C) -> byte::Result<(Self, usize)> {
        let offset = &mut 0;

        let resize_mask: u8 = bytes.read(offset, ctx)?;
        let header: [u8; 3] = bytes.read(offset, ctx)?;
        if &header != b"AGT" {
            return Err(byte::Error::BadInput {
                err: "invalid TGAM header",
            });
        }

        let width: u16 = bytes.read(offset, ctx)?;
        let height: u16 = bytes.read(offset, ctx)?;

        let rgba_size: u32 = bytes.read(offset, ctx)?;
        let mask_size: u32 = bytes.read(offset, ctx)?;

        let pow2_width = round_up_to_power_of_two(width.into()) as usize;
        let pow2_height = round_up_to_power_of_two(height.into()) as usize;
        let expected_rgba_len = pow2_width
            .checked_mul(pow2_height)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or(byte::Error::BadInput {
                err: "TGAM dimensions overflow",
            })?;

        if rgba_size as usize != expected_rgba_len {
            return Err(byte::Error::BadInput {
                err: "unexpected TGAM rgba payload size",
            });
        }

        let mask_resize: u8 = if resize_mask == 0x6D {
            bytes.read(offset, ctx)?
        } else {
            1
        };

        let rgba = bytes.read(offset, Len(rgba_size as usize))?;
        let mask = bytes.read(offset, Len(mask_size as usize))?;

        Ok((
            Self {
                width,
                height,
                rgba,
                mask,
                mask_resize,
            },
            *offset,
        ))
    }
}

pub fn round_up_to_power_of_two(value: u32) -> u32 {
    if value < 2 {
        value
    } else {
        value.next_power_of_two()
    }
}

#[cfg(test)]
mod tests {
    use byte::ctx::LittleEndian;

    use super::*;

    #[test]
    fn power_of_two_rounding() {
        assert_eq!(round_up_to_power_of_two(0), 0);
        assert_eq!(round_up_to_power_of_two(1), 1);
        assert_eq!(round_up_to_power_of_two(2), 2);
        assert_eq!(round_up_to_power_of_two(3), 4);
        assert_eq!(round_up_to_power_of_two(4), 4);
        assert_eq!(round_up_to_power_of_two(15), 16);
        assert_eq!(round_up_to_power_of_two(16), 16);
        assert_eq!(round_up_to_power_of_two(17), 32);
    }

    #[test]
    fn try_read_valid_tgam() {
        let mut bytes = Vec::new();
        bytes.push(0); // resize_mask
        bytes.extend_from_slice(b"AGT"); // header
        bytes.extend_from_slice(&2u16.to_le_bytes()); // width
        bytes.extend_from_slice(&2u16.to_le_bytes()); // height
        bytes.extend_from_slice(&16u32.to_le_bytes()); // rgba_size (2x2x4 = 16)
        bytes.extend_from_slice(&0u32.to_le_bytes()); // mask_size
        bytes.extend_from_slice(&[255u8; 16]); // rgba pixel payload

        let (tgam, read_len) = Tgam::try_read(&bytes, LittleEndian).unwrap();
        assert_eq!(read_len, bytes.len());
        assert_eq!(tgam.width, 2);
        assert_eq!(tgam.height, 2);
        assert_eq!(tgam.rgba.len(), 16);
    }

    #[test]
    fn try_read_invalid_magic() {
        let bytes = b"INVALID_HEADER_DATA";
        assert!(Tgam::try_read(bytes, LittleEndian).is_err());
    }

    #[test]
    fn try_read_mismatched_rgba_size() {
        let mut bytes = Vec::new();
        bytes.push(0);
        bytes.extend_from_slice(b"AGT");
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&12u32.to_le_bytes()); // expected 16, got 12
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&[255u8; 12]);

        assert!(Tgam::try_read(&bytes, LittleEndian).is_err());
    }

    #[test]
    fn mask_bits_are_msb_first_and_resized() {
        let mask = TgamMask {
            data: vec![0b1000_0000, 0b1000_0000],
            resize: 1,
            width: 9,
            height: 1,
        };
        assert!(mask.contains(0, 0));
        assert!(!mask.contains(1, 0));
        assert!(mask.contains(8, 0));
        assert!(!mask.contains(9, 0));

        let resized = TgamMask {
            data: vec![0b1000_0000],
            resize: 2,
            width: 4,
            height: 2,
        };
        assert!(resized.contains(0, 0));
        assert!(resized.contains(1, 1));
        assert!(!resized.contains(2, 0));
        assert!(!resized.contains(0, 2));
    }

    #[test]
    fn mask_is_built_from_the_tgam_payload() {
        let mut bytes = Vec::new();
        bytes.push(0);
        bytes.extend_from_slice(b"AGT");
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes()); // mask_size
        bytes.extend_from_slice(&[255u8; 16]);
        bytes.push(0b1000_0000);

        let (tgam, _) = Tgam::try_read(&bytes, LittleEndian).unwrap();
        let mask = TgamMask::from_tgam(&tgam).unwrap();
        assert!(mask.contains(0, 0));
        assert!(!mask.contains(1, 0));
    }
}
