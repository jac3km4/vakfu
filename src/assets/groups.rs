use std::io::{Read, Seek};

use byte::BytesExt;
use byte::ctx::LittleEndian;

use super::{AssetError, read_archive_entry};

/// Upper bound on the visibility matrix size, guarding against malformed
/// headers. `Vec<bool>` is bit-packed, so this caps the allocation at 2 MiB.
const MAX_TABLE_LEN: usize = 1 << 24;

/// Indoor/outdoor group visibility parsed from `WorldGroupManager`.
///
/// The file stores a square matrix of group keys: for a given "from" group key
/// (the group the camera or player is in), it lists the group keys that remain
/// visible. `layer_visible(0, key)` mirrors the reference's outdoor default of
/// `key <= 0`.
#[derive(Debug, Clone, PartialEq)]
pub struct GroupVisibility {
    layer_count: usize,
    visible: Vec<bool>,
}

impl GroupVisibility {
    /// Parses the `WorldGroupManager` binary layout.
    ///
    /// Layout (little endian):
    /// - `layer_count`: `u16`, one less than the real count
    /// - `count`: `u16`
    /// - per entry: `from` (`i16`), `layer_count` (`i16`), then that many
    ///   visible group keys (`i16`)
    pub fn parse(bytes: &[u8]) -> Result<Self, byte::Error> {
        let offset = &mut 0;
        let layer_count = usize::from(bytes.read::<u16>(offset, LittleEndian)?) + 1;
        let table_len = layer_count
            .checked_mul(layer_count)
            .filter(|&len| len <= MAX_TABLE_LEN)
            .ok_or(byte::Error::BadInput {
                err: "group visibility table too large",
            })?;
        let count = bytes.read::<u16>(offset, LittleEndian)?;

        let mut visible = vec![false; table_len];
        for _ in 0..count {
            let from = bytes.read::<i16>(offset, LittleEndian)?;
            let layers = bytes.read::<i16>(offset, LittleEndian)?;
            let base = from.unsigned_abs() as usize * layer_count;
            for _ in 0..layers {
                let layer = bytes.read::<i16>(offset, LittleEndian)?;
                if let Some(bit) = visible.get_mut(base + layer.unsigned_abs() as usize) {
                    *bit = true;
                }
            }
        }

        Ok(Self {
            layer_count,
            visible,
        })
    }

    /// Loads `groups.lib` from an already-open map gfx archive.
    ///
    /// The game configuration declares
    /// `groupsFile=jar:file:./contents/maps/gfx/%d.jar!/groups.lib`, so the
    /// visibility table lives in the same per-map jar as the map chunks. The
    /// caller opens that archive once and shares it with the chunk reader.
    pub fn load_from_archive<R: Read + Seek>(
        archive: &mut zip::ZipArchive<R>,
    ) -> Result<Option<Self>, AssetError> {
        let Some(bytes) = read_archive_entry(archive, "groups.lib")? else {
            return Ok(None);
        };
        Ok(Some(Self::parse(&bytes)?))
    }

    /// Returns whether `layer` (a group key) is visible from `from`.
    pub fn layer_visible(&self, from: i32, layer: i32) -> bool {
        if from == 0 {
            return layer <= 0;
        }
        let from = from.unsigned_abs() as usize;
        let layer = layer.unsigned_abs() as usize;
        self.visible
            .get(from * self.layer_count + layer)
            .copied()
            .unwrap_or(false)
    }

    /// Returns the number of group keys in the visibility matrix.
    pub fn layer_count(&self) -> usize {
        self.layer_count
    }
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::Write;
    use std::path::{Path, PathBuf};

    use super::*;

    fn push_u16(buf: &mut Vec<u8>, value: u16) {
        buf.extend_from_slice(&value.to_le_bytes());
    }

    fn push_i16(buf: &mut Vec<u8>, value: i16) {
        buf.extend_from_slice(&value.to_le_bytes());
    }

    #[test]
    fn parses_visibility_matrix_and_outdoor_rule() {
        let mut bytes = Vec::new();
        push_u16(&mut bytes, 2); // layer_count -> 3
        push_u16(&mut bytes, 1); // one entry
        push_i16(&mut bytes, 1); // from group
        push_i16(&mut bytes, 2); // two visible groups
        push_i16(&mut bytes, 0);
        push_i16(&mut bytes, -2);

        let visibility = GroupVisibility::parse(&bytes).unwrap();
        assert_eq!(visibility.layer_count(), 3);
        assert!(visibility.layer_visible(1, 0));
        assert!(visibility.layer_visible(1, -2));
        assert!(!visibility.layer_visible(1, 1));

        // `Math.abs(from)` is used, so negative from values share a row.
        assert!(visibility.layer_visible(-1, 0));
        assert!(!visibility.layer_visible(2, 0));

        // The reference special-cases `from == 0` to `layer <= 0`.
        assert!(visibility.layer_visible(0, 0));
        assert!(visibility.layer_visible(0, -7));
        assert!(!visibility.layer_visible(0, 1));
    }

    #[test]
    fn rejects_oversized_tables_and_truncated_data() {
        // 65536 x 65536 exceeds the guard.
        let mut bytes = Vec::new();
        push_u16(&mut bytes, u16::MAX);
        push_u16(&mut bytes, 0);
        assert!(GroupVisibility::parse(&bytes).is_err());

        // Declares two layers but ends before providing them.
        let mut bytes = Vec::new();
        push_u16(&mut bytes, 0); // layer_count -> 1
        push_u16(&mut bytes, 1);
        push_i16(&mut bytes, 1);
        push_i16(&mut bytes, 2);
        assert!(GroupVisibility::parse(&bytes).is_err());
    }

    fn empty_visibility() -> Vec<u8> {
        let mut bytes = Vec::new();
        push_u16(&mut bytes, 0); // layer_count -> 1
        push_u16(&mut bytes, 0); // no entries
        bytes
    }

    fn temporary_root(name: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "vakfu-groups-{name}-{}-{unique}",
            std::process::id()
        ))
    }

    fn write_jar(path: &Path, entries: &[(&str, &[u8])]) {
        let file = File::create(path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (name, bytes) in entries {
            writer.start_file(*name, options).unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap();
    }

    fn open_groups(path: &Path) -> Result<Option<GroupVisibility>, AssetError> {
        let file = File::open(path).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        GroupVisibility::load_from_archive(&mut archive)
    }

    #[test]
    fn loads_groups_lib_from_the_map_jar() {
        let root = temporary_root("jar");
        let gfx = root.join("gfx");
        std::fs::create_dir_all(&gfx).unwrap();
        let groups = empty_visibility();
        let jar = gfx.join("42.jar");
        write_jar(&jar, &[("groups.lib", &groups)]);

        let visibility = open_groups(&jar).unwrap().unwrap();
        assert_eq!(visibility.layer_count(), 1);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_groups_lib_is_not_an_error() {
        let root = temporary_root("missing");
        let gfx = root.join("gfx");
        std::fs::create_dir_all(&gfx).unwrap();
        let jar = gfx.join("7.jar");
        write_jar(&jar, &[("map", &[])]);

        assert!(open_groups(&jar).unwrap().is_none());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn malformed_groups_lib_reports_a_decode_error() {
        let root = temporary_root("malformed");
        let gfx = root.join("gfx");
        std::fs::create_dir_all(&gfx).unwrap();
        let jar = gfx.join("9.jar");
        write_jar(&jar, &[("groups.lib", &[0, 0, 1])]);

        assert!(matches!(
            open_groups(&jar),
            Err(AssetError::Decoding(_))
        ));

        std::fs::remove_dir_all(root).unwrap();
    }
}
