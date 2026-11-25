use std::io::{Read, Seek};
use std::sync::Arc;

use byte::ctx::LittleEndian;
use byte::{BytesExt, TryRead};
use hashbrown::HashMap;

use crate::assets::{AssetError, is_chunk_entry_name};

/// The lower colour bound applied by `ClampShadowLitModifier`.
const SHADOW_MIN: f32 = 0.3;
/// The upper colour bound applied by `ClampLightLitModifier`.
const LIGHT_MAX: f32 = 1.05;

/// Luminance weights from `SunLightModifier.getDayLightIntensity`.
const SUN_LUMINANCE_R: f32 = 0.212_671;
const SUN_LUMINANCE_G: f32 = 0.715_16;
const SUN_LUMINANCE_B: f32 = 0.072_169;
/// Quadratic coefficient from `SunLightModifier.getGlobalLight`.
const GLOBAL_LIGHT_CURVE: f32 = 2.318_223;

/// The per-cell static lighting definition, mirroring `CellLightDef`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LightDef {
    /// Ambient light, already doubled (`COLOR_FACTOR = 2.0`).
    pub ambiance_light: [f32; 3],
    /// Per-channel shadow multipliers.
    pub shadows: [f32; 3],
    /// Night-light colour offset (`byte / 255 - 0.5`).
    pub lights: [f32; 3],
    /// The cell's `allowOutdoorLighting` flag.
    ///
    /// Kept for data fidelity, but the reference renderer never consults it:
    /// `SunLightModifier` is registered per map and multiplies every lit
    /// object. `CellLightDef.m_allowOutdoorLighting` is only compared in
    /// `equals`.
    pub allow_outdoor_lighting: bool,
    pub has_shadows: bool,
    /// Whether the cell defines a night light (its `lights` colour is not gray).
    pub has_night_light: bool,
}

impl LightDef {
    pub const DEFAULT: LightDef = LightDef {
        allow_outdoor_lighting: false,
        ambiance_light: [1f32, 1f32, 1f32],
        shadows: [1f32, 1f32, 1f32],
        lights: [0f32, 0f32, 0f32],
        has_shadows: false,
        has_night_light: false,
    };

    /// The ambient colour after `CellLightDef.updateShadow(shadow_intensity)`.
    ///
    /// `shadow_intensity` is the reference `m_shadowIntensityFactor` in
    /// `[0, 1]`: `1.0` is full daytime shadows and `0.0` disables them.
    pub fn merged_color(&self, shadow_intensity: f32) -> [f32; 3] {
        let mut rgb = self.ambiance_light;
        if shadow_intensity > 0.0 && self.has_shadows {
            let k = shadow_intensity * 2.0;
            let l = 1.0 - shadow_intensity;
            for (channel, shadow) in rgb.iter_mut().zip(self.shadows) {
                *channel = (*channel * (shadow * k + l)).clamp(0.0, 2.0);
            }
        }
        rgb
    }

    /// The additive night-light colour after
    /// `CellLightDef.updateNightLight(night_light_intensity)`.
    pub fn night_light_color(&self, night_light_intensity: f32) -> [f32; 3] {
        if !self.has_night_light || night_light_intensity <= 0.0 {
            return [0.0; 3];
        }
        let k = night_light_intensity * 2.0;
        [
            self.lights[0] * k,
            self.lights[1] * k,
            self.lights[2] * k,
        ]
    }

    /// The effective cell colour under full daytime shadow intensity.
    pub fn effective_day_color(&self) -> [f32; 3] {
        LightingState::default().shade(self)
    }
}

/// Time-of-day lighting, mirroring the factors held by
/// `IsoSceneLightManager` and `SunLightModifier`.
#[derive(Debug, Clone, PartialEq)]
pub struct LightingState {
    /// `m_shadowIntensityFactor`; `1.0` is full daytime shadows.
    pub shadow_intensity: f32,
    /// `m_nightLightIntensityFactor`; `1.0` is full night lights.
    pub night_light_intensity: f32,
    /// `SunLightModifier` colour. It multiplies every lit object; the
    /// per-cell `allowOutdoorLighting` flag is not consulted.
    pub sun_color: [f32; 3],
    /// Dynamic additive point lights (normally empty for static maps).
    pub point_lights: Vec<PointLight2d>,
}

impl Default for LightingState {
    fn default() -> Self {
        Self {
            shadow_intensity: 1.0,
            night_light_intensity: 0.0,
            sun_color: [1.0; 3],
            point_lights: Vec::new(),
        }
    }
}

impl LightingState {
    /// The sun luminance used by `SunLightModifier.getDayLightIntensity`.
    pub fn day_light_intensity(&self) -> f32 {
        self.sun_color[0] * SUN_LUMINANCE_R
            + self.sun_color[1] * SUN_LUMINANCE_G
            + self.sun_color[2] * SUN_LUMINANCE_B
    }

    /// The combined day/night factor from `SunLightModifier.getGlobalLight`.
    ///
    /// Positive values drive `m_shadowIntensityFactor`; negative values drive
    /// `m_nightLightIntensityFactor`.
    pub fn global_light(&self) -> f32 {
        let intensity = self.day_light_intensity();
        GLOBAL_LIGHT_CURVE * intensity * intensity + 1.0 - GLOBAL_LIGHT_CURVE
    }

    /// Applies `IsoSceneLightManager.setLightIntensityFactor(float)`.
    ///
    /// The reference calls this from `SunLightModifier.update`, so shadow and
    /// night-light intensity follow the sun colour instead of being set
    /// independently.
    pub fn apply_auto_daylight(&mut self) {
        let global = self.global_light();
        self.shadow_intensity = if global > 0.001 { global } else { 0.0 };
        self.night_light_intensity = if global < -0.001 { -global } else { 0.0 };
    }

    /// Computes the lit colour of a static cell, mirroring the modifier order
    /// installed by `IsoSceneLightManager.reset`:
    ///
    /// 1. `SunLightModifier` (priority 0) multiplies every lit object. The
    ///    reference registers it per map; the cell's `allowOutdoorLighting`
    ///    flag is not consulted.
    /// 2. `ClampShadowLitModifier` (300) floors the running colour at 0.3.
    /// 3. `LightningMapManager` (400) multiplies by the merged cell colour and
    ///    adds the night light.
    /// 4. `IsoLightSourceModifier` (600) adds dynamic light contributions.
    /// 5. `ClampLightLitModifier` (800) caps the result at 1.05. It only bounds
    ///    the ceiling; the reference leaves darker results untouched.
    pub fn shade(&self, def: &LightDef) -> [f32; 3] {
        self.shade_at(def, 0.0, 0.0)
    }

    /// Like [`Self::shade`], but evaluates the dynamic point lights at the
    /// given cell position.
    pub fn shade_at(&self, def: &LightDef, cell_x: f32, cell_y: f32) -> [f32; 3] {
        // SunLightModifier (priority 0): multiplies every lit object.
        let mut color = self.sun_color;
        // ClampShadowLitModifier (priority 300).
        for channel in &mut color {
            *channel = channel.max(SHADOW_MIN);
        }
        // LightningMapManager (priority 400): multiplicative cell colour plus
        // the additive night light.
        let merged = def.merged_color(self.shadow_intensity);
        let night = def.night_light_color(self.night_light_intensity);
        for channel in 0..3 {
            color[channel] = color[channel] * merged[channel] + night[channel];
        }
        // IsoLightSourceModifier (priority 600).
        for light in &self.point_lights {
            let contribution = light.contribution_at(cell_x, cell_y);
            for channel in 0..3 {
                color[channel] += contribution[channel];
            }
        }
        // ClampLightLitModifier (priority 800): the reference only caps the
        // ceiling, so results below 0.3 (for example from shadowed cells or
        // subtractive night lights) are left alone.
        for channel in &mut color {
            *channel = channel.min(LIGHT_MAX);
        }
        color
    }
}

/// A dynamic isometric point light, mirroring `IsoLightSource`.
///
/// The reference samples a small 2D intensity map at cell centres:
/// `max(0, 1 - distance / range)`, scaled by `intensity` and the light colour.
/// Contributions are additive and applied before the `1.05` ceiling.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PointLight2d {
    /// Cell-space position `(x, y)` of the light.
    pub position: [f32; 2],
    /// Maximum radius, in cells.
    pub range: f32,
    /// Linear falloff coefficient (`m_attenuation[1]`, `1.0` by default).
    pub attenuation: f32,
    /// Multiplier applied to the falloff.
    pub intensity: f32,
    /// Colour added to a fully lit cell.
    pub color: [f32; 3],
}

impl Default for PointLight2d {
    fn default() -> Self {
        Self {
            position: [0.0; 2],
            range: 3.0,
            attenuation: 1.0,
            intensity: 1.0,
            color: [1.0; 3],
        }
    }
}

impl PointLight2d {
    /// Returns this light's additive contribution at a cell.
    pub fn contribution_at(&self, cell_x: f32, cell_y: f32) -> [f32; 3] {
        if self.range <= 0.0 || self.intensity == 0.0 {
            return [0.0; 3];
        }
        let dx = cell_x - self.position[0];
        let dy = cell_y - self.position[1];
        let distance = (dx * dx + dy * dy).sqrt();
        if distance > self.range {
            return [0.0; 3];
        }
        // `LightSource` default attenuation is `{0, 1, 0}`, i.e.
        // `1 / (attenuation * range)`.
        let falloff = self.attenuation * self.range;
        let strength = if falloff > 0.0 {
            (1.0 - distance / falloff).max(0.0)
        } else if distance == 0.0 {
            1.0
        } else {
            0.0
        } * self.intensity;
        [
            self.color[0] * strength,
            self.color[1] * strength,
            self.color[2] * strength,
        ]
    }
}


#[derive(Debug, Clone)]
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
                has_night_light: light as u32 != 0xFF80_8080,
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

#[derive(Debug, Default)]
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
        assert!(def.has_night_light);
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
    fn effective_day_color_applies_shadows_then_caps() {
        // No shadows: the ambient colour is used unchanged.
        assert_eq!(
            LightDef::DEFAULT.effective_day_color(),
            [1.0, 1.0, 1.0]
        );

        // 1 * 0.5 * 2 = 1.0, 1 * 1.0 * 2 = 2.0 -> 1.05, 1 * 0.1 * 2 = 0.2.
        // `ClampLightLitModifier` only caps the ceiling, so 0.2 is kept.
        let lit = LightDef {
            has_shadows: true,
            ambiance_light: [1.0, 1.0, 1.0],
            shadows: [0.5, 1.0, 0.1],
            ..LightDef::DEFAULT
        };
        assert_eq!(lit.effective_day_color(), [1.0, 1.05, 0.2]);

        // The shadow product is clamped to [0, 2] before the final cap.
        let bright = LightDef {
            has_shadows: true,
            ambiance_light: [2.0, 0.0, 1.0],
            shadows: [1.0, 1.0, 1.0],
            ..LightDef::DEFAULT
        };
        assert_eq!(bright.effective_day_color(), [1.05, 0.0, 1.05]);
    }

    fn assert_close(actual: [f32; 3], expected: [f32; 3]) {
        for (a, e) in actual.into_iter().zip(expected) {
            assert!((a - e).abs() < 1e-6, "{actual:?} != {expected:?}");
        }
    }

    #[test]
    fn shadow_intensity_scales_between_ambiance_and_full_shadow() {
        let def = LightDef {
            has_shadows: true,
            ambiance_light: [1.0; 3],
            shadows: [0.0, 0.5, 1.0],
            ..LightDef::DEFAULT
        };

        // Full shadows reproduce the daytime colour. The 0.3 floor is applied
        // before the light-map multiply, so a zero shadow multiplier leaves the
        // channel at 0; only the 1.05 ceiling applies afterwards.
        assert_eq!(def.effective_day_color(), [0.0, 1.0, 1.05]);

        // Half intensity: k = 1, l = 0.5, so the multipliers are 0.5/1.0/1.5.
        let half = LightingState {
            shadow_intensity: 0.5,
            ..LightingState::default()
        };
        assert_close(half.shade(&def), [0.5, 1.0, 1.05]);

        // Disabling shadows returns the raw ambiance.
        let none = LightingState {
            shadow_intensity: 0.0,
            ..LightingState::default()
        };
        assert_close(none.shade(&def), [1.0, 1.0, 1.0]);
    }

    #[test]
    fn sun_color_applies_to_every_cell() {
        // `SunLightModifier` multiplies every lit object; the reference never
        // consults `CellLightDef.m_allowOutdoorLighting`.
        let outdoor = LightDef {
            allow_outdoor_lighting: true,
            ..LightDef::DEFAULT
        };
        let indoor = LightDef {
            allow_outdoor_lighting: false,
            ..LightDef::DEFAULT
        };
        let state = LightingState {
            sun_color: [0.5, 1.0, 0.25],
            ..LightingState::default()
        };

        // The 0.25 channel is raised to the 0.3 shadow floor before the
        // light-map multiply.
        assert_close(state.shade(&outdoor), [0.5, 1.0, 0.3]);
        assert_close(state.shade(&indoor), [0.5, 1.0, 0.3]);
    }

    #[test]
    fn auto_daylight_follows_sun_luminance() {
        // White sun: full daytime shadows and no night lights.
        let mut day = LightingState::default();
        day.apply_auto_daylight();
        assert!((day.shadow_intensity - 1.0).abs() < 1e-6);
        assert_eq!(day.night_light_intensity, 0.0);

        // Black sun: no shadows and the full negative intensity becomes night.
        let mut night = LightingState {
            sun_color: [0.0; 3],
            ..LightingState::default()
        };
        night.apply_auto_daylight();
        assert_eq!(night.shadow_intensity, 0.0);
        assert!((night.night_light_intensity - (GLOBAL_LIGHT_CURVE - 1.0)).abs() < 1e-6);

        // Mid grey is already dark enough to activate the night lights.
        let mut mid = LightingState {
            sun_color: [0.5; 3],
            ..LightingState::default()
        };
        mid.apply_auto_daylight();
        assert_eq!(mid.shadow_intensity, 0.0);
        assert!(mid.night_light_intensity > 0.0);
    }

    #[test]
    fn night_light_adds_the_offset_colour() {
        let def = LightDef {
            lights: [0.5, -0.5, -0.5],
            has_night_light: true,
            ..LightDef::DEFAULT
        };

        // k = 1, so the offset is added directly; the additive result is only
        // capped at 1.05 by `ClampLightLitModifier`.
        let night = LightingState {
            night_light_intensity: 0.5,
            ..LightingState::default()
        };
        assert_close(night.shade(&def), [1.05, 0.5, 0.5]);

        // A cell without a night light ignores the intensity.
        let lit = LightDef {
            has_night_light: false,
            ..def
        };
        assert_close(night.shade(&lit), [1.0, 1.0, 1.0]);
    }

    #[test]
    fn point_lights_fall_off_linearly_with_range() {
        let def = LightDef::DEFAULT;
        let state = LightingState {
            point_lights: vec![PointLight2d {
                position: [0.0, 0.0],
                range: 2.0,
                attenuation: 1.0,
                intensity: 1.0,
                color: [0.04, 0.02, 0.0],
            }],
            ..LightingState::default()
        };

        assert_close(state.shade_at(&def, 0.0, 0.0), [1.04, 1.02, 1.0]);
        // Halfway across the range the contribution is halved.
        assert_close(state.shade_at(&def, 1.0, 0.0), [1.02, 1.01, 1.0]);
        // At the range edge and beyond the light contributes nothing.
        assert_close(state.shade_at(&def, 2.0, 0.0), [1.0, 1.0, 1.0]);
        assert_close(state.shade_at(&def, 3.0, 0.0), [1.0, 1.0, 1.0]);
    }

    #[test]
    fn gray_shadow_and_light_disable_the_cell_effects() {
        let gray = 0xFF80_8080u32 as i32;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&0i16.to_le_bytes());
        bytes.extend_from_slice(&0i16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes()); // defcount
        bytes.push(0); // indoor
        bytes.extend_from_slice(&0i32.to_le_bytes()); // ambient
        bytes.extend_from_slice(&gray.to_le_bytes()); // shadows = GRAY
        bytes.extend_from_slice(&gray.to_le_bytes()); // lights = GRAY
        bytes.extend_from_slice(&1i16.to_le_bytes()); // layer count (unused)
        bytes.extend_from_slice(&1i16.to_le_bytes()); // lcount
        bytes.extend_from_slice(&0u16.to_le_bytes()); // lookup hash
        bytes.extend_from_slice(&0u16.to_le_bytes()); // layer index

        let (cell, _) = LightCell::try_read(&bytes, LittleEndian).unwrap();
        let def = cell.layer_colors.values().next().unwrap();
        assert!(!def.has_shadows);
        assert!(!def.has_night_light);
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
