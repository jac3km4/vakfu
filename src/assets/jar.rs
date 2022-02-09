use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use bevy::asset::{AssetIo, AssetIoError, BoxedFuture};

use super::{AssetIoProps, CustomAssetIoPlugin};

pub struct JarAssetIo {
    base: Box<dyn AssetIo>,
    archive: Mutex<zip::ZipArchive<BufReader<File>>>,
}

impl JarAssetIo {
    pub fn plugin<P: AsRef<Path>>(path: P) -> CustomAssetIoPlugin<JarAssetIo, P> {
        CustomAssetIoPlugin::new(path)
    }
}

impl<P: AsRef<Path>> TryFrom<AssetIoProps<P>> for JarAssetIo {
    type Error = anyhow::Error;

    fn try_from(props: AssetIoProps<P>) -> Result<Self, Self::Error> {
        let archive = zip::ZipArchive::new(BufReader::new(File::open(props.props)?))?;
        let io = JarAssetIo {
            base: props.base,
            archive: Mutex::new(archive),
        };
        Ok(io)
    }
}

impl AssetIo for JarAssetIo {
    fn load_path<'a>(&'a self, path: &'a Path) -> BoxedFuture<'a, Result<Vec<u8>, AssetIoError>> {
        Box::pin(async {
            let mut archive = self.archive.lock().unwrap();
            let mut entry = archive
                .by_name(&path.to_string_lossy())
                .map_err(|_| AssetIoError::NotFound(path.to_owned()))?;
            let mut bytes = Vec::with_capacity(entry.size() as usize);
            entry.read_to_end(&mut bytes)?;
            Ok(bytes)
        })
    }

    fn read_directory(
        &self,
        _path: &Path,
    ) -> Result<Box<dyn Iterator<Item = PathBuf>>, AssetIoError> {
        unimplemented!()
    }

    fn is_directory(&self, _path: &Path) -> bool {
        unimplemented!()
    }

    fn watch_path_for_changes(&self, _path: &Path) -> Result<(), AssetIoError> {
        // file watching not implemented
        Ok(())
    }

    fn watch_for_changes(&self) -> Result<(), AssetIoError> {
        unimplemented!()
    }
}
