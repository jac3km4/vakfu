extern crate zip;

use std::fs::File;
use wfu::gfx::world::library::ElementLibrary;
use wfu::gfx::world::light::LightMap;
use wfu::io::tgam::TgamLoader;

pub struct Resources {
    path: String,
}

impl Resources {
    pub fn new(path: String) -> Resources {
        Resources { path }
    }

    pub fn load_tgam_loader(&self) -> TgamLoader<File> {
        let path = format!("{}\\game\\contents\\maps\\gfx.jar", self.path);
        TgamLoader::new(File::open(path).unwrap())
    }

    pub fn load_element_library(&self) -> ElementLibrary {
        let path = format!("{}\\game\\contents\\maps\\data.jar", self.path);
        ElementLibrary::load(File::open(path).unwrap())
    }

    pub fn load_light_map(&self, map_id: i32) -> LightMap {
        let path = format!("{}\\game\\contents\\maps\\light\\{}.jar", self.path, map_id);
        LightMap::load(File::open(path).unwrap())
    }

    pub fn load_gfx_map_archive(&self, map_id: i32) -> zip::ZipArchive<File> {
        let path = format!("{}\\game\\contents\\maps\\gfx\\{}.jar", self.path, map_id);
        zip::ZipArchive::new(File::open(path).unwrap()).unwrap()
    }
}
