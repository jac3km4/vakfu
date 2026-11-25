mod groups;
mod light;
mod map;
mod sprite;
mod tgam;

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use bevy::asset::io::{AssetReaderError, ErasedAssetReader, VecReader};
use bevy::asset::{AssetLoader, RenderAssetUsages};
use bevy::image::Image;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use byte::TryRead;
use byte::ctx::LittleEndian;
pub use groups::GroupVisibility;
pub use light::{LightDef, LightMap, LightingState, PointLight2d};
pub use map::{
    Map, MapChunk, MapElementDetails, Rgba, TINT_SCALE, element_hashcode, iso_cell_diamond,
    iso_to_screen, screen_to_iso, sorted_altitude_order,
};
pub use sprite::{Animation, Frames, MapSpriteDefinition, MapSpriteLibrary};
pub use tgam::{Tgam, round_up_to_power_of_two};
use thiserror::Error;

/// Returns whether a zip entry name refers to a map or light chunk.
///
/// Chunk entries are named after their cell coordinates, e.g. `12-34_`;
/// trimming the numeric coordinates leaves the trailing underscore.
pub(crate) fn is_chunk_entry_name(name: &str) -> bool {
    name.trim_matches(|c: char| c.is_numeric() || c == '-') == "_"
}

#[derive(Debug, Default, bevy::reflect::TypePath)]
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
            width: tgam.padded_width(),
            height: tgam.padded_height(),
            depth_or_array_layers: 1,
        };
        let img = Image::new(
            extent,
            TextureDimension::D2,
            tgam.rgba().to_vec(),
            TextureFormat::Rgba8Unorm,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        );
        Ok(img)
    }

    fn extensions(&self) -> &[&str] {
        &["tgam"]
    }
}

#[derive(Debug, Clone)]
pub struct JarAssetSource {
    path: Arc<PathBuf>,
    /// Idle [`ZipArchive`] handles, each with its own open file and parsed
    /// central directory. Bevy decompresses assets on the async task pool;
    /// a single shared archive behind a mutex serialised every read. Handles
    /// are only borrowed from this pool for the duration of a read.
    idle: Arc<Mutex<Vec<zip::ZipArchive<BufReader<File>>>>>,
}

impl JarAssetSource {
    pub fn new(path: impl AsRef<Path>) -> Result<Self, AssetError> {
        let path = path.as_ref();
        // Validate the archive eagerly so startup fails with a clear error.
        open_archive(path)?;
        Ok(Self {
            path: Arc::new(path.to_path_buf()),
            idle: Arc::new(Mutex::new(Vec::new())),
        })
    }

    /// Takes an idle archive handle, opening a new one when the pool is empty.
    fn acquire(&self) -> Result<zip::ZipArchive<BufReader<File>>, AssetReaderError> {
        let pooled = self.idle.lock().map_err(lock_poisoned)?.pop();
        match pooled {
            Some(archive) => Ok(archive),
            None => open_archive_reader(&self.path),
        }
    }

    /// Returns a handle to the pool once a read has finished.
    fn release(&self, archive: zip::ZipArchive<BufReader<File>>) {
        if let Ok(mut idle) = self.idle.lock() {
            idle.push(archive);
        }
    }
}

/// Opens the archive, formatting I/O failures with the archive path.
fn open_archive(path: &Path) -> Result<zip::ZipArchive<BufReader<File>>, AssetError> {
    let file = File::open(path).map_err(|err| {
        std::io::Error::new(
            err.kind(),
            format!("failed to open archive '{}': {err}", path.display()),
        )
    })?;
    zip::ZipArchive::new(BufReader::new(file)).map_err(|err| match err {
        zip::result::ZipError::Io(io_err) => {
            zip::result::ZipError::Io(std::io::Error::new(
                io_err.kind(),
                format!("failed to read archive '{}': {io_err}", path.display()),
            ))
        }
        other => other,
    })
    .map_err(AssetError::from)
}

/// Opens the archive for a read, formatting I/O failures with the archive path.
fn open_archive_reader(path: &Path) -> Result<zip::ZipArchive<BufReader<File>>, AssetReaderError> {
    let io_error = |err: std::io::Error, verb: &str| {
        AssetReaderError::Io(
            std::io::Error::new(
                err.kind(),
                format!("failed to {verb} archive '{}': {err}", path.display()),
            )
            .into(),
        )
    };
    let file = File::open(path).map_err(|err| io_error(err, "open"))?;
    zip::ZipArchive::new(BufReader::new(file)).map_err(|err| io_error(err.into(), "read"))
}

fn lock_poisoned<T>(_: T) -> AssetReaderError {
    AssetReaderError::Io(std::io::Error::other("asset archive lock poisoned").into())
}

/// Reads a single archive entry into memory.
fn read_entry(
    archive: &mut zip::ZipArchive<BufReader<File>>,
    name: &str,
) -> Result<Box<dyn bevy::asset::io::Reader>, AssetReaderError> {
    let mut entry = archive.by_name(name).map_err(|err| match err {
        zip::result::ZipError::FileNotFound => {
            AssetReaderError::NotFound(Path::new(name).to_owned())
        }
        zip::result::ZipError::Io(error) => AssetReaderError::Io(error.into()),
        _ => AssetReaderError::Io(std::io::Error::other(err).into()),
    })?;
    let mut bytes = Vec::with_capacity(usize::try_from(entry.size()).unwrap_or(0));
    entry.read_to_end(&mut bytes)?;
    Ok(Box::new(VecReader::new(bytes)))
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
            // JAR entries always use forward slashes, but be defensive about
            // platform-native paths.
            let name = path.to_string_lossy().replace('\\', "/");
            let mut archive = self.acquire()?;
            let result = read_entry(&mut archive, &name);
            self.release(archive);
            result
        })
    }

    fn read_meta<'a>(
        &'a self,
        path: &'a std::path::Path,
    ) -> bevy::tasks::BoxedFuture<
        'a,
        Result<Box<dyn bevy::asset::io::Reader + 'a>, bevy::asset::io::AssetReaderError>,
    > {
        // Wakfu JARs have no Bevy metadata; NotFound selects default loader settings.
        Box::pin(async { Err(bevy::asset::io::AssetReaderError::NotFound(path.to_owned())) })
    }

    fn read_directory<'a>(
        &'a self,
        path: &'a std::path::Path,
    ) -> bevy::tasks::BoxedFuture<
        'a,
        Result<Box<bevy::asset::io::PathStream>, bevy::asset::io::AssetReaderError>,
    > {
        Box::pin(async { Err(bevy::asset::io::AssetReaderError::NotFound(path.to_owned())) })
    }

    fn is_directory<'a>(
        &'a self,
        _path: &'a std::path::Path,
    ) -> bevy::tasks::BoxedFuture<'a, Result<bool, bevy::asset::io::AssetReaderError>> {
        Box::pin(async { Ok(false) })
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

#[cfg(test)]
mod tests {
    use std::io::Write;

    use bevy::asset::io::{AssetReaderError, Reader as _};

    use super::*;

    fn temporary_archive(name: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "vakfu-{name}-{}-{unique}.jar",
            std::process::id()
        ))
    }

    fn read_bytes(source: &JarAssetSource, name: &str) -> Result<Vec<u8>, AssetReaderError> {
        bevy::tasks::block_on(async {
            let mut reader = source.read(Path::new(name)).await?;
            let mut bytes = Vec::new();
            reader.read_to_end(&mut bytes).await?;
            Ok(bytes)
        })
    }

    #[test]
    fn recognises_chunk_entry_names() {
        assert!(is_chunk_entry_name("12-34_"));
        assert!(is_chunk_entry_name("_"));
        assert!(!is_chunk_entry_name("12-34_data"));
        assert!(!is_chunk_entry_name("gfx/1.tgam"));
    }

    #[test]
    fn reads_entries_and_reuses_pooled_archives() {
        let archive_path = temporary_archive("entries");
        {
            let file = File::create_new(&archive_path).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            writer.start_file("gfx/1.tgam", options).unwrap();
            writer.write_all(b"first").unwrap();
            writer.start_file("gfx/2.tgam", options).unwrap();
            writer.write_all(b"second").unwrap();
            writer.finish().unwrap();
        }

        let source = JarAssetSource::new(&archive_path).unwrap();
        // Read each entry twice so the second round reuses pooled handles.
        for _ in 0..2 {
            assert_eq!(read_bytes(&source, "gfx/1.tgam").unwrap(), b"first");
            assert_eq!(read_bytes(&source, "gfx/2.tgam").unwrap(), b"second");
        }
        assert!(matches!(
            read_bytes(&source, "gfx/missing.tgam"),
            Err(AssetReaderError::NotFound(_))
        ));

        drop(source);
        std::fs::remove_file(archive_path).unwrap();
    }

    #[test]
    fn jar_metadata_is_not_found() {
        let archive_path = temporary_archive("metadata");
        let file = File::create_new(&archive_path).unwrap();
        drop(zip::ZipWriter::new(file).finish().unwrap());
        let source = JarAssetSource::new(&archive_path).unwrap();
        let path = Path::new("gfx/123.tgam");
        let metadata_error = bevy::tasks::block_on(source.read_meta(path)).err();
        let metadata_bytes = bevy::tasks::block_on(source.read_meta_bytes(path));
        drop(source);
        std::fs::remove_file(archive_path).unwrap();

        assert_eq!(
            metadata_error,
            Some(AssetReaderError::NotFound(path.to_owned()))
        );
        assert_eq!(metadata_bytes, Err(AssetReaderError::NotFound(path.to_owned())));
    }
}
