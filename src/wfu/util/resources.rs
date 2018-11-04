extern crate zip;

use std::fs::File;
use std::path::PathBuf;
use wfu::gfx::world::library::ElementLibrary;
use wfu::gfx::world::light::LightMap;
use wfu::io::tgam::TgamLoader;

pub struct Resources {
    path: PathBuf,
}

impl Resources {
    pub fn new(path: String) -> Resources {
        Resources {
            path: PathBuf::from(path),
        }
    }

    fn maps_root(&self) -> PathBuf {
        self.path.join("game").join("contents").join("maps")
    }

    pub fn load_tgam_loader(&self) -> TgamLoader<File> {
        let path = self.maps_root().join("gfx.jar");
        TgamLoader::new(File::open(path).unwrap())
    }

    pub fn load_element_library(&self) -> ElementLibrary {
        let path = self.maps_root().join("data.jar");
        ElementLibrary::load(File::open(path).unwrap())
    }

    pub fn load_light_map(&self, map_id: i32) -> LightMap {
        let path = self
            .maps_root()
            .join("light")
            .join(format!("{}.jar", map_id));
        LightMap::load(File::open(path).unwrap())
    }

    pub fn load_gfx_map_archive(&self, map_id: i32) -> zip::ZipArchive<File> {
        let path = self.maps_root().join("gfx").join(format!("{}.jar", map_id));
        zip::ZipArchive::new(File::open(path).unwrap()).unwrap()
    }
}
