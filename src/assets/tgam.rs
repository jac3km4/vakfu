use byte::ctx::{Endianess, Len};
use byte::{BytesExt, TryRead};

#[derive(Debug)]
pub struct Tgam<'a> {
    width: u16,
    height: u16,
    rgba: &'a [u8],
    mask: &'a [u8],
    mask_resize: u8,
}

impl<'a> Tgam<'a> {
    #[inline]
    pub fn rgba(&self) -> &'a [u8] {
        self.rgba
    }

    #[inline]
    pub fn width(&self) -> u32 {
        round_up_to_power_of_two(self.width.into())
    }

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
                err: "invalid TGA header",
            });
        }

        let width: u16 = bytes.read(offset, ctx)?;
        let height: u16 = bytes.read(offset, ctx)?;

        let rgba_size: u32 = bytes.read(offset, ctx)?;
        let mask_size: u32 = bytes.read(offset, ctx)?;

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

fn round_up_to_power_of_two(value: u32) -> u32 {
    if value < 2 {
        return value;
    }
    let mut v = value - 1;
    v = v | v >> 1;
    v = v | v >> 2;
    v = v | v >> 4;
    v = v | v >> 8;
    v = v | v >> 16;
    v + 1
}
