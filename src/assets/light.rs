use std::io::{Read, Seek};
use std::sync::Arc;

use byte::ctx::LittleEndian;
use byte::{BytesExt, TryRead};
use hashbrown::HashMap;

use crate::assets::{AssetError, is_chunk_entry_name};

pub struct LightDef {
    pub ambiance_light: [f32; 3],
    pub shadows: [f32; 3],
    #[allow(dead_code)]
    pub lights: [f32; 3],
    #[allow(dead_code)]
    pub allow_outdoor_lighting: bool,
    pub has_shadows: bool,
    #[allow(dead_code)]
    pub night_light: [f32; 3],
}

impl LightDef {
    pub const DEFAULT: LightDef = LightDef {
        allow_outdoor_lighting: false,
        ambiance_light: [1f32, 1f32, 1f32],
        shadows: [1f32, 1f32, 1f32],
        lights: [1f32, 1f32, 1f32],
        has_shadows: false,
        night_light: [0f32, 0f32, 0f32],
    };

    /// The effective cell colour under full daytime shadow intensity.
    ///
    /// Mirrors `CellLightDef.updateShadow(1.0)` followed by the
    /// `ClampShadowLitModifier` (floor `0.3`) and `ClampLightLitModifier`
    /// (ceiling `1.05`) modifiers that the reference light manager always
    /// installs. `ambiance_light` is already stored doubled, matching the
    /// `COLOR_FACTOR` applied in `CellLightDef`.
    pub fn effective_day_color(&self) -> [f32; 3] {
        let mut rgb = self.ambiance_light;
        if self.has_shadows {
            for (channel, shadow) in rgb.iter_mut().zip(self.shadows) {
                *channel = (*channel * shadow * 2.0).clamp(0.0, 2.0);
            }
        }
        for channel in &mut rgb {
            *channel = channel.clamp(0.3, 1.05);
        }
        rgb
    }
}

#[derive(Clone)]
pub struct LightCell {
    pub cell_x: i32,
    pub cell_y: i32,
    pub layer_colors: HashMap<i32, Arc<LightDef>>,
}

impl<'a> TryRead<'a, LittleEndian> for LightCell {
    fn try_read(bytes: &'a [u8], ctx: LittleEndian) -> byte::Result<(Self, usize)> {
        let offset = &mut 0;
        let x = bytes.read::<i16>(offset, ctx)?;
        let y = bytes.read::<i16>(offset, ctx)?;

        let defcount = bytes.read::<u16>(offset, ctx)?;
        let mut layer: Vec<Arc<LightDef>> = Vec::with_capacity(defcount as usize);

        for _ in 0..defcount {
            let outdoor = bytes.read::<u8>(offset, ctx)? != 0;
            let ambiance = bytes.read::<i32>(offset, ctx)?;
            let shadow = bytes.read::<i32>(offset, ctx)?;
            let light = bytes.read::<i32>(offset, ctx)?;
            let night = [0f32, 0f32, 0f32];

            let def = LightDef {
                allow_outdoor_lighting: outdoor,
                ambiance_light: [
                    (ambiance & 0xFF) as f32 / 255f32 * 2f32,
                    (ambiance >> 8 & 0xFF) as f32 / 255f32 * 2f32,
                    (ambiance >> 16 & 0xFF) as f32 / 255f32 * 2f32,
                ],
                shadows: [
                    (shadow & 0xFF) as f32 / 255f32,
                    (shadow >> 8 & 0xFF) as f32 / 255f32,
                    (shadow >> 16 & 0xFF) as f32 / 255f32,
                ],
                lights: [
                    (light & 0xFF) as f32 / 255f32 - 0.5f32,
                    (light >> 8 & 0xFF) as f32 / 255f32 - 0.5f32,
                    (light >> 16 & 0xFF) as f32 / 255f32 - 0.5f32,
                ],
                has_shadows: shadow as u32 != 0xFF80_8080,
                night_light: night,
            };
            layer.push(Arc::new(def));
        }

        let _layer_count = bytes.read::<i16>(offset, ctx)?;
        let lcount = bytes.read::<i16>(offset, ctx)?;

        let mut layer_colors = HashMap::new();
        let default_layer = Arc::new(LightDef::DEFAULT);

        for _ in 0..lcount {
            let k = bytes.read::<u16>(offset, ctx)?;
            let idx = bytes.read::<u16>(offset, ctx)?;
            if (idx as usize) < layer.len() {
                layer_colors.insert(k as i32, layer[idx as usize].clone());
            } else {
                layer_colors.insert(k as i32, default_layer.clone());
            }
        }

        Ok((
            LightCell {
                cell_x: x as i32 * 18,
                cell_y: y as i32 * 18,
                layer_colors,
            },
            *offset,
        ))
    }
}

#[derive(Default)]
pub struct LightMap {
    pub light_maps: HashMap<(i32, i32), LightCell>,
}

impl LightMap {
    pub fn load<R: Read + Seek>(input: R) -> Result<LightMap, AssetError> {
        let mut archive = zip::ZipArchive::new(input)?;
        let mut light_maps = HashMap::new();

        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            let name = entry.name()?;
            if is_chunk_entry_name(&name) {
                let mut buffer = Vec::with_capacity(entry.size() as usize);
                entry.read_to_end(&mut buffer)?;

                let (cell, _) = LightCell::try_read(&buffer, LittleEndian)?;
                let mapx = cell.cell_x.div_euclid(18);
                let mapy = cell.cell_y.div_euclid(18);
                let hash = chunk_key(mapx, mapy);
                light_maps.insert(hash, cell);
            }
        }
        Ok(LightMap { light_maps })
    }

    pub fn get_color(&self, cell_x: i32, cell_y: i32, layer_idx: i32) -> &LightDef {
        let map_x = cell_x.div_euclid(18);
        let map_y = cell_y.div_euclid(18);
        let key = chunk_key(map_x, map_y);

        self.light_maps
            .get(&key)
            .and_then(|cell| {
                let hash =
                    (cell_x - cell.cell_x) + ((cell_y - cell.cell_y) + (layer_idx * 18)) * 18;
                cell.layer_colors.get(&hash)
            })
            .map(Arc::as_ref)
            .unwrap_or(&LightDef::DEFAULT)
    }
}

/// Returns the key identifying the 18x18 light chunk containing `(x, y)`.
///
/// A `(i32, i32)` pair is used instead of packing into a single `i32`, which
/// truncated coordinates outside `i16` and made distant chunks collide.
pub fn chunk_key(x: i32, y: i32) -> (i32, i32) {
    (x, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_cell_parses_layers_and_decodes_colours() {
        let ambiance = 255i32 | (128 << 8); // r=255, g=128, b=0
        let shadow = 0x0080_8080i32; // r=128, g=128, b=128
        let light = 255i32; // r=255, g=0, b=0

        let mut bytes = Vec::new();
        bytes.extend_from_slice(&2i16.to_le_bytes()); // x -> 36
        bytes.extend_from_slice(&3i16.to_le_bytes()); // y -> 54
        bytes.extend_from_slice(&1u16.to_le_bytes()); // defcount
        bytes.push(1); // allow_outdoor_lighting
        bytes.extend_from_slice(&ambiance.to_le_bytes());
        bytes.extend_from_slice(&shadow.to_le_bytes());
        bytes.extend_from_slice(&light.to_le_bytes());
        bytes.extend_from_slice(&1i16.to_le_bytes()); // layer count (unused)
        bytes.extend_from_slice(&1i16.to_le_bytes()); // lcount
        bytes.extend_from_slice(&1620u16.to_le_bytes()); // lookup hash
        bytes.extend_from_slice(&0u16.to_le_bytes()); // layer index

        let (cell, consumed) = LightCell::try_read(&bytes, LittleEndian).unwrap();
        assert_eq!(consumed, bytes.len());
        assert_eq!((cell.cell_x, cell.cell_y), (36, 54));

        let def = cell.layer_colors.get(&1620).unwrap();
        assert!(def.allow_outdoor_lighting);
        assert_eq!(
            def.ambiance_light,
            [2.0, 128.0 / 255.0 * 2.0, 0.0]
        );
        assert_eq!(def.shadows, [128.0 / 255.0; 3]);
        assert_eq!(def.lights, [0.5, -0.5, -0.5]);
        assert!(def.has_shadows);
        assert_eq!(def.night_light, [0.0, 0.0, 0.0]);
    }

    #[test]
    fn light_map_resolves_layer_hash_and_falls_back() {
        let cell = LightCell {
            cell_x: 36,
            cell_y: 54,
            layer_colors: HashMap::from([(
                1620,
                Arc::new(LightDef {
                    ambiance_light: [0.5; 3],
                    ..LightDef::DEFAULT
                }),
            )]),
        };
        let map = LightMap {
            light_maps: HashMap::from([(chunk_key(2, 3), cell)]),
        };

        // `get_color` reconstructs the (x + (y + layer * 18) * 18) lookup hash.
        assert_eq!(map.get_color(36, 54, 5).ambiance_light, [0.5; 3]);
        // A different layer and a neighbouring cell miss and use the default.
        assert_eq!(
            map.get_color(36, 54, 4).ambiance_light,
            LightDef::DEFAULT.ambiance_light
        );
        assert_eq!(
            map.get_color(37, 54, 5).ambiance_light,
            LightDef::DEFAULT.ambiance_light
        );
    }

    #[test]
    fn effective_day_color_applies_shadows_then_clamps() {
        // No shadows: only the [0.3, 1.05] clamp applies.
        assert_eq!(
            LightDef::DEFAULT.effective_day_color(),
            [1.0, 1.0, 1.0]
        );

        // 1 * 0.5 * 2 = 1.0, 1 * 1.0 * 2 = 2.0 -> 1.05, 1 * 0.1 * 2 = 0.2 -> 0.3.
        let lit = LightDef {
            has_shadows: true,
            ambiance_light: [1.0, 1.0, 1.0],
            shadows: [0.5, 1.0, 0.1],
            ..LightDef::DEFAULT
        };
        assert_eq!(lit.effective_day_color(), [1.0, 1.05, 0.3]);

        // The shadow product is clamped to [0, 2] before the final clamp.
        let bright = LightDef {
            has_shadows: true,
            ambiance_light: [2.0, 0.0, 1.0],
            shadows: [1.0, 1.0, 1.0],
            ..LightDef::DEFAULT
        };
        assert_eq!(bright.effective_day_color(), [1.05, 0.3, 1.05]);
    }

    #[test]
    fn get_color_falls_back_to_default() {
        let map = LightMap::default();
        let default_color = map.get_color(10, 20, 0);
        assert_eq!(
            default_color.ambiance_light,
            LightDef::DEFAULT.ambiance_light
        );
    }

    #[test]
    fn distant_chunks_do_not_collide() {
        let make_cell = |chunk_x: i32, color: f32| {
            let cell_x = chunk_x * 18;
            let mut layer_colors = HashMap::new();
            layer_colors.insert(
                0,
                Arc::new(LightDef {
                    ambiance_light: [color; 3],
                    ..LightDef::DEFAULT
                }),
            );
            LightCell {
                cell_x,
                cell_y: 0,
                layer_colors,
            }
        };

        let mut light_maps = HashMap::new();
        for (chunk_x, color) in [(0, 0.25), (65536, 0.75)] {
            let cell = make_cell(chunk_x, color);
            light_maps.insert(chunk_key(cell.cell_x / 18, cell.cell_y / 18), cell);
        }
        let map = LightMap { light_maps };

        assert_eq!(map.get_color(0, 0, 0).ambiance_light, [0.25; 3]);
        assert_eq!(map.get_color(65536 * 18, 0, 0).ambiance_light, [0.75; 3]);
    }
}
