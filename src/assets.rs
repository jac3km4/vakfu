mod map;
mod sprite;
mod tgam;

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use std::sync::{Arc, Mutex};

use bevy::asset::io::{ErasedAssetReader, VecReader};
use bevy::asset::{AssetLoader, RenderAssetUsages};
use bevy::image::Image;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use byte::TryRead;
use byte::ctx::LittleEndian;
pub use map::{Map, MapChunk, MapElementDetails, Rgba};
pub use sprite::{Animation, Frame, Frames, MapSpriteDefinition, MapSpriteLibrary};
pub use tgam::Tgam;
use thiserror::Error;

#[derive(Debug, Default)]
pub struct TgamLoader;

impl AssetLoader for TgamLoader {
    type Asset = Image;
    type Error = AssetError;
    type Settings = ();

    async fn load(
        &self,
        reader: &mut dyn bevy::asset::io::Reader,
        _settings: &Self::Settings,
        _load_context: &mut bevy::asset::LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;

        let (tgam, _) = Tgam::try_read(&bytes, LittleEndian)?;
        let extent = Extent3d {
            width: tgam.width(),
            height: tgam.height(),
            depth_or_array_layers: 1,
        };
        let img = Image::new(
            extent,
            TextureDimension::D2,
            tgam.rgba().to_vec(),
            TextureFormat::Rgba8Unorm,
            RenderAssetUsages::RENDER_WORLD,
        );
        Ok(img)
    }

    fn extensions(&self) -> &[&str] {
        &["tgam"]
    }
}

#[derive(Debug, Clone)]
pub struct JarAssetSource {
    archive: Arc<Mutex<zip::ZipArchive<BufReader<File>>>>,
}

impl JarAssetSource {
    pub fn new(path: impl AsRef<Path>) -> Result<Self, AssetError> {
        let file = File::open(path)?;
        let archive = Arc::new(Mutex::new(zip::ZipArchive::new(BufReader::new(file))?));
        Ok(Self { archive })
    }
}

impl ErasedAssetReader for JarAssetSource {
    fn read<'a>(
        &'a self,
        path: &'a std::path::Path,
    ) -> bevy::tasks::BoxedFuture<
        'a,
        Result<Box<dyn bevy::asset::io::Reader + 'a>, bevy::asset::io::AssetReaderError>,
    > {
        Box::pin(async {
            let mut archive = self.archive.lock().unwrap();
            let mut entry = archive
                .by_name(&path.to_string_lossy())
                .map_err(|err| match err {
                    zip::result::ZipError::Io(error) => error,
                    zip::result::ZipError::FileNotFound => std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        format!("'{}' not found ({err})", path.display()),
                    ),
                    _ => std::io::Error::new(std::io::ErrorKind::Other, err),
                })?;
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes)?;
            Ok(Box::new(VecReader::new(bytes)) as Box<dyn bevy::asset::io::Reader>)
        })
    }

    fn read_meta<'a>(
        &'a self,
        _path: &'a std::path::Path,
    ) -> bevy::tasks::BoxedFuture<
        'a,
        Result<Box<dyn bevy::asset::io::Reader + 'a>, bevy::asset::io::AssetReaderError>,
    > {
        unimplemented!()
    }

    fn read_directory<'a>(
        &'a self,
        _path: &'a std::path::Path,
    ) -> bevy::tasks::BoxedFuture<
        'a,
        Result<Box<bevy::asset::io::PathStream>, bevy::asset::io::AssetReaderError>,
    > {
        unimplemented!()
    }

    fn is_directory<'a>(
        &'a self,
        _path: &'a std::path::Path,
    ) -> bevy::tasks::BoxedFuture<'a, Result<bool, bevy::asset::io::AssetReaderError>> {
        unimplemented!()
    }

    fn read_meta_bytes<'a>(
        &'a self,
        path: &'a std::path::Path,
    ) -> bevy::tasks::BoxedFuture<'a, Result<Vec<u8>, bevy::asset::io::AssetReaderError>> {
        Box::pin(async { Err(bevy::asset::io::AssetReaderError::NotFound(path.to_owned())) })
    }
}

#[derive(Debug, Error)]
pub enum AssetError {
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("archive error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("decoding error: {0}")]
    Decoding(#[from] byte::Error),
}
