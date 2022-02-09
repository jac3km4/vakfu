use std::borrow::Cow;

use anyhow::anyhow;
use bevy::asset::{AssetLoader, BoxedFuture, LoadContext, LoadedAsset};
use bevy::prelude::Image;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use byte::ctx::Bytes;
use byte::{BytesExt, TryRead};

#[derive(Debug)]
pub struct Tgam<'a> {
    width: u16,
    height: u16,
    bytes: Cow<'a, [u8]>,
    mask: AlphaMask<'a>,
}

impl<'a> Tgam<'a> {
    #[inline]
    pub fn bytes(&'a self) -> &'a [u8] {
        &self.bytes
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

impl<'a> TryRead<'a> for Tgam<'a> {
    fn try_read(bytes: &'a [u8], _ctx: ()) -> byte::Result<(Self, usize)> {
        let offset = &mut 0;

        let resize_mask: u8 = bytes.read(offset)?;
        let header: &'a [u8] = bytes.read_with(offset, Bytes::Len(3))?;
        if header != b"AGT" {
            let err = "Invalid TGAM header";
            return Err(byte::Error::BadInput { err });
        }

        let width: u16 = bytes.read(offset)?;
        let height: u16 = bytes.read(offset)?;
        let tga_size: u32 = bytes.read(offset)?;
        let mask_size: u32 = bytes.read(offset)?;
        let mask_resize: u8 = if resize_mask == 109 {
            bytes.read(offset)?
        } else {
            1
        };
        let tga_bytes: &[u8] = bytes.read_with(offset, Bytes::Len(tga_size as usize))?;
        let mask_bytes: &[u8] = bytes.read_with(offset, Bytes::Len(mask_size as usize))?;

        let mask = AlphaMask {
            bytes: Cow::Borrowed(mask_bytes),
            resize: mask_resize,
        };

        let tgam = Tgam {
            width,
            height,
            bytes: Cow::Borrowed(tga_bytes),
            mask,
        };
        Ok((tgam, *offset))
    }
}

#[derive(Debug)]
pub struct AlphaMask<'a> {
    bytes: Cow<'a, [u8]>,
    resize: u8,
}

#[derive(Default)]
pub struct TgamLoader;

impl AssetLoader for TgamLoader {
    fn load<'a>(
        &'a self,
        bytes: &'a [u8],
        load_context: &'a mut LoadContext,
    ) -> BoxedFuture<'a, anyhow::Result<()>> {
        Box::pin(async {
            let tgam: Tgam = bytes
                .read(&mut 0)
                .map_err(|err| anyhow!("Failed to read TGAM: {:?}", err))?;
            let extent = Extent3d {
                width: tgam.width(),
                height: tgam.height(),
                depth_or_array_layers: 1,
            };
            let img = Image::new(
                extent,
                TextureDimension::D2,
                tgam.bytes.to_vec(),
                TextureFormat::Rgba8Unorm,
            );
            load_context.set_default_asset(LoadedAsset::new(img));
            Ok(())
        })
    }

    fn extensions(&self) -> &[&str] {
        &["tgam"]
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
