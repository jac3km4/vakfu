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
    #[allow(dead_code)]
    mask: &'a [u8],
    /// A value defining how the mask should be resized or mapped.
    #[allow(dead_code)]
    mask_resize: u8,
}

impl<'a> Tgam<'a> {
    /// Returns the RGBA pixel data of the image.
    #[inline]
    pub fn rgba(&self) -> &'a [u8] {
        self.rgba
    }

    /// Returns the width of the image, rounded up to the nearest power of two.
    #[inline]
    pub fn width(&self) -> u32 {
        round_up_to_power_of_two(self.width.into())
    }

    /// Returns the height of the image, rounded up to the nearest power of two.
    #[inline]
    pub fn height(&self) -> u32 {
        round_up_to_power_of_two(self.height.into())
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
}
