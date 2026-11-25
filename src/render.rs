use std::collections::BTreeMap;
use std::fs::File;
use std::mem;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use bevy::ecs::system::SystemParam;
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::sprite::Anchor;
use bevy::tasks::{AsyncComputeTaskPool, Task, futures_lite::future};
use hashbrown::{HashMap, HashSet};
use rstar::{AABB, RTree, RTreeObject};

use crate::assets::{
    Animation, Frames, GroupVisibility, LightDef, LightMap, LightingState, Map, MapChunk,
    MapSpriteDefinition, MapSpriteLibrary, TINT_SCALE, TgamMaskLibrary, element_hashcode,
    iso_to_screen, round_up_to_power_of_two, screen_to_iso, sorted_altitude_order,
};
use crate::camera::{CameraController, MainCamera, WORLD_LAYER};
use crate::settings::MapViewSettings;

/// Elements are drawn by their world-space Z. Every map element stays inside
/// this range so editor overlays and the camera's far plane never collide with
/// world geometry.
pub const Z_MIN: f32 = 0.0;
pub const Z_MAX: f32 = 500.0;

/// Free space kept at either end of the range when Z values are re-spread, so
/// repeated small edits do not force another re-normalization.
const Z_MARGIN: f32 = 25.0;
/// Gap used when appending past the current top or bottom neighbour.
const Z_STEP: f32 = 0.01;
const Z_MIDPOINT: f32 = (Z_MIN + Z_MAX) * 0.5;

/// Element definition ids animated by the reference `Sea` system
/// (`Sea.WATER_ELEMENTS`).
const WATER_ELEMENTS: [i32; 10] = [
    10208, 11138, 10036, 10211, 17702, 17704, 17712, 17713, 17755, 17756,
];
/// `Sea.m_amplitude`.
const WATER_AMPLITUDE: f32 = 0.001;
/// `Sea.m_speed`.
const WATER_SPEED: f32 = 2.0;
/// `Sea.m_width`.
const WATER_WIDTH: f32 = 10.0;

/// One element's current Z, uid and horizontal envelope, used when re-spreading.
type ZSortEntry = (f32, u64, AABB<(f32, f32)>);

/// Evenly spreads `count` ordered elements over the world Z range.
fn spread_z(index: usize, count: usize) -> f32 {
    if count <= 1 {
        return Z_MIDPOINT;
    }
    let min = Z_MIN + Z_MARGIN;
    let max = Z_MAX - Z_MARGIN;
    min + (index as f32 / (count - 1) as f32) * (max - min)
}

#[derive(Message, Debug, Clone, Copy)]
pub struct ChangeMapEvent(pub i32);

#[derive(Clone, Resource)]
pub struct MapCatalog {
    pub maps_path: std::path::PathBuf,
    pub sprites: Arc<MapSpriteLibrary>,
    pub available_maps: Vec<i32>,
    pub current_map_id: i32,
}

/// Optional indoor/outdoor group visibility metadata.
///
/// When absent (the map jar has no `groups.lib`), every group is rendered as
/// before.
#[derive(Resource, Default)]
pub struct GroupVisibilityState(pub Option<GroupVisibility>);

#[derive(Resource, Default)]
pub struct PendingMapLoad {
    task: Option<(i32, Task<anyhow::Result<MapRenderer>>)>,
}

impl PendingMapLoad {
    /// Returns whether a new task is needed for the latest selection.
    fn request(&mut self, current_map_id: i32, target_map_id: i32) -> bool {
        if target_map_id == current_map_id {
            self.task = None;
            return false;
        }
        !self
            .task
            .as_ref()
            .is_some_and(|(id, _)| *id == target_map_id)
    }
}

#[derive(Debug, Resource)]
pub struct MapRenderer {
    rtree: RTree<Renderable>,
    uid_envelopes: HashMap<u64, AABB<(f32, f32)>>,
    next_uid: u64,
    /// Entities shown in the previous frame; used to hide those that leave the view.
    visible: HashSet<Entity>,
    /// Reused allocation for the current frame's visible set.
    spare_visible: HashSet<Entity>,
    /// Entities awaiting an explicit despawn (element removal or map switch).
    pending_despawns: Vec<Entity>,
    sprite_assets: HashMap<i32, (Handle<Image>, Handle<TextureAtlasLayout>)>,
    center: Vec2,
    used_definitions: Vec<i32>,
    available_layers: Vec<u8>,
    /// Altitude-order counts per cell, for exact placement lookup.
    cell_altitudes: HashMap<(i32, i32), BTreeMap<u8, u32>>,
    /// Static light data, kept so edited/moved elements can resolve their cell
    /// lighting like the reference engine does.
    light_map: LightMap,
    /// Alpha masks decoded from TGAM textures, used for precise picking.
    masks: TgamMaskLibrary,
}

/// The definition-independent inputs for placing a sprite.
#[derive(Clone, Copy)]
pub struct RenderableSpec<'a> {
    pub def: &'a MapSpriteDefinition,
    pub cell_x: i32,
    pub cell_y: i32,
    pub cell_z: i16,
    pub height: u8,
    pub altitude_order: u8,
    pub group: i32,
    pub layer: u8,
    pub color: LinearRgba,
}

impl MapRenderer {
    pub fn new(map: &Map, sprites: &MapSpriteLibrary, light_map: LightMap) -> Self {
        let mut elements = map
            .chunks()
            .iter()
            .flat_map(MapChunk::elements)
            .collect::<Vec<_>>();
        elements.sort_by_key(|elem| {
            let before_mobile = sprites
                .get(elem.definition_id())
                .is_some_and(|def| def.flags().is_before_mobile());
            elem.hashcode(before_mobile)
        });

        let mut min_x = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_y = f32::NEG_INFINITY;

        let mut used_defs_set = HashSet::new();
        let mut layers_set = HashSet::new();
        let mut uid_envelopes = HashMap::with_capacity(elements.len());
        let mut next_uid = 1u64;

        let total_elements = elements.len();
        let renderables: Vec<_> = elements
            .iter()
            .enumerate()
            .filter_map(|(z_index, elem)| {
                let Some(def) = sprites.get(elem.definition_id()) else {
                    warn!("Unknown sprite definition {}", elem.definition_id());
                    return None;
                };
                // Elements without the export bit are editor-only helpers and
                // are hidden by the reference renderer.
                if def.export_mask() & 0x1 == 0 {
                    return None;
                }
                let Some(group) = elem.group() else {
                    warn!("Missing group for element {}", elem.definition_id());
                    return None;
                };
                let before_mobile = def.flags().is_before_mobile();

                let (x, y) = elem.screen_position();
                let (origin_x, origin_y) = def.origin();
                let x = x - origin_x as f32;
                let y = y + origin_y as f32;
                let z = spread_z(z_index, total_elements);

                // Preserve the asset format's established mapping: size() is
                // the sampled rectangle, texture_size() is the world-space size.
                let (sample_width, sample_height) = def.size();
                let (display_width, display_height) = def.texture_size();

                min_x = min_x.min(x);
                max_x = max_x.max(x + display_width as f32);
                min_y = min_y.min(y - display_height as f32);
                max_y = max_y.max(y);

                let light_def =
                    light_map.get_color(elem.cell_x(), elem.cell_y(), group.layer() as i32);
                let [r, g, b, a] = elem.color().to_f32_array();
                let color = LinearRgba::new(r, g, b, a);
                let color_tinted = {
                    let [r, g, b] = light_def.effective_day_color();
                    LinearRgba::new(color.red * r, color.green * g, color.blue * b, color.alpha)
                };

                let uid = next_uid;
                next_uid += 1;

                used_defs_set.insert(elem.definition_id());
                layers_set.insert(group.layer());

                let renderable = Renderable {
                    uid,
                    definition_id: elem.definition_id(),
                    position: Vec3::new(x, y, z),
                    origin: (origin_x, origin_y),
                    cell_x: elem.cell_x(),
                    cell_y: elem.cell_y(),
                    cell_z: elem.cell_z(),
                    height: elem.height(),
                    altitude_order: elem.altitude_order(),
                    before_mobile,
                    hashcode: elem.hashcode(before_mobile),
                    texture_size: UVec2::new(sample_width.into(), sample_height.into()),
                    render_size: UVec2::new(display_width.into(), display_height.into()),
                    color,
                    color_tinted,
                    light: *light_def,
                    texture_id: def.texture_id(),
                    flip_x: def.flags().is_flip(),
                    animation: def.animation(),
                    group: group.key(),
                    layer: group.layer(),
                    visibility_mask: def.visibility_mask(),
                    group_alpha: 1.0,

                    id: None,
                };
                uid_envelopes.insert(uid, renderable.envelope());
                Some(renderable)
            })
            .collect();

        let center = if renderables.is_empty() {
            Vec2::ZERO
        } else {
            Vec2::new((min_x + max_x) * 0.5, (min_y + max_y) * 0.5)
        };

        let mut used_definitions: Vec<i32> = used_defs_set.into_iter().collect();
        used_definitions.sort_unstable();
        let mut available_layers: Vec<u8> = layers_set.into_iter().collect();
        available_layers.sort_unstable();

        let mut cell_altitudes: HashMap<(i32, i32), BTreeMap<u8, u32>> = HashMap::new();
        for elem in &renderables {
            *cell_altitudes
                .entry((elem.cell_x, elem.cell_y))
                .or_default()
                .entry(elem.altitude_order)
                .or_insert(0) += 1;
        }

        Self {
            rtree: RTree::bulk_load(renderables),
            uid_envelopes,
            next_uid,
            visible: HashSet::new(),
            spare_visible: HashSet::new(),
            pending_despawns: Vec::new(),
            sprite_assets: HashMap::new(),
            center,
            used_definitions,
            available_layers,
            cell_altitudes,
            light_map,
            masks: TgamMaskLibrary::default(),
        }
    }

    pub fn center(&self) -> Vec2 {
        self.center
    }

    pub fn element_count(&self) -> usize {
        self.rtree.size()
    }

    pub fn used_definitions(&self) -> &[i32] {
        &self.used_definitions
    }

    pub fn available_layers(&self) -> &[u8] {
        &self.available_layers
    }

    pub fn allocate_uid(&mut self) -> u64 {
        let uid = self.next_uid;
        self.next_uid += 1;
        uid
    }

    /// Returns a Z that places an element with `hashcode` in the correct order
    /// relative to the elements its envelope overlaps.
    ///
    /// When repeated edits have exhausted the gap between two neighbours (or
    /// pushed the range against [`Z_MIN`] / [`Z_MAX`]), the map's Z values are
    /// re-spread order-preservingly before retrying.
    pub fn place_z(
        &mut self,
        envelope: AABB<(f32, f32)>,
        hashcode: i64,
        ignore_uid: Option<u64>,
    ) -> f32 {
        if let Some(z) = self.try_place_z(envelope, hashcode, ignore_uid) {
            return z;
        }
        self.normalize_z();
        self.try_place_z(envelope, hashcode, ignore_uid)
            .unwrap_or(Z_MIDPOINT)
    }

    fn try_place_z(
        &self,
        envelope: AABB<(f32, f32)>,
        hashcode: i64,
        ignore_uid: Option<u64>,
    ) -> Option<f32> {
        let mut max_below: Option<f32> = None;
        let mut min_above: Option<f32> = None;
        for other in self.rtree.locate_in_envelope_intersecting(envelope) {
            if Some(other.uid) == ignore_uid {
                continue;
            }
            if other.hashcode <= hashcode {
                max_below = Some(max_below.map_or(other.position.z, |v| v.max(other.position.z)));
            } else {
                min_above = Some(min_above.map_or(other.position.z, |v| v.min(other.position.z)));
            }
        }
        match (max_below, min_above) {
            (Some(b), Some(a)) => {
                let mid = b + (a - b) * 0.5;
                (mid > b && mid < a).then_some(mid)
            }
            (Some(b), None) => (b + Z_STEP <= Z_MAX).then_some(b + Z_STEP),
            (None, Some(a)) => (a - Z_STEP >= Z_MIN).then_some(a - Z_STEP),
            (None, None) => Some(Z_MIDPOINT),
        }
    }

    /// Returns a Z strictly in front of `min_z`, tucked cleanly below any
    /// element already above `min_z`.
    pub fn place_above(
        &mut self,
        min_z: f32,
        envelope: AABB<(f32, f32)>,
        ignore_uid: Option<u64>,
    ) -> f32 {
        if let Some(z) = self.try_place_above(min_z, envelope, ignore_uid) {
            return z;
        }
        self.normalize_z();
        self.try_place_above(min_z, envelope, ignore_uid)
            .unwrap_or(min_z + Z_STEP)
    }

    fn try_place_above(
        &self,
        min_z: f32,
        envelope: AABB<(f32, f32)>,
        ignore_uid: Option<u64>,
    ) -> Option<f32> {
        let mut next_above: Option<f32> = None;
        for other in self.rtree.locate_in_envelope_intersecting(envelope) {
            if Some(other.uid) == ignore_uid {
                continue;
            }
            if other.position.z > min_z + 1e-5 {
                next_above = Some(next_above.map_or(other.position.z, |v| v.min(other.position.z)));
            }
        }
        match next_above {
            Some(a) => {
                let mid = min_z + (a - min_z) * 0.5;
                (mid > min_z && mid < a).then_some(mid)
            }
            None => (min_z + Z_STEP <= Z_MAX).then_some(min_z + Z_STEP),
        }
    }

    pub fn frontmost_z_for(&mut self, envelope: AABB<(f32, f32)>, ignore_uid: Option<u64>) -> f32 {
        let z = self
            .extreme_z(envelope, ignore_uid, f32::max)
            .map_or(Z_MIDPOINT, |z| z + Z_STEP);
        if z <= Z_MAX {
            return z;
        }
        self.normalize_z();
        self.extreme_z(envelope, ignore_uid, f32::max)
            .map_or(Z_MIDPOINT, |z| z + Z_STEP)
    }

    pub fn backmost_z_for(&mut self, envelope: AABB<(f32, f32)>, ignore_uid: Option<u64>) -> f32 {
        let z = self
            .extreme_z(envelope, ignore_uid, f32::min)
            .map_or(Z_MIN, |z| z - Z_STEP);
        if z >= Z_MIN {
            return z;
        }
        self.normalize_z();
        self.extreme_z(envelope, ignore_uid, f32::min)
            .map_or(Z_MIN, |z| z - Z_STEP)
    }

    fn extreme_z(
        &self,
        envelope: AABB<(f32, f32)>,
        ignore_uid: Option<u64>,
        op: fn(f32, f32) -> f32,
    ) -> Option<f32> {
        self.rtree
            .locate_in_envelope_intersecting(envelope)
            .filter(|other| Some(other.uid) != ignore_uid)
            .map(|other| other.position.z)
            .reduce(op)
    }

    /// Re-spreads every element's Z across the world range while preserving
    /// their current order.
    ///
    /// Z is not part of the R\* tree envelope, so elements can be updated in
    /// place without rebuilding the index.
    fn normalize_z(&mut self) {
        let mut order: Vec<ZSortEntry> = self
            .rtree
            .iter()
            .map(|elem| (elem.position.z, elem.uid, elem.envelope()))
            .collect();
        order.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        let count = order.len();
        for (index, (_, uid, envelope)) in order.into_iter().enumerate() {
            let z = spread_z(index, count);
            if let Some(elem) = self
                .rtree
                .locate_in_envelope_intersecting_mut(envelope)
                .find(|elem| elem.uid == uid)
            {
                elem.position.z = z;
            }
        }
    }

    pub fn default_ground_z(&self) -> i16 {
        let c = self.center;
        let probe = AABB::from_corners((c.x - 128.0, c.y - 128.0), (c.x + 128.0, c.y + 128.0));
        self.rtree
            .locate_in_envelope_intersecting(probe)
            .min_by(|a, b| a.position.z.total_cmp(&b.position.z))
            .map_or(0, |elem| elem.cell_z)
    }

    /// Returns an `altitude_order` higher than any existing element on `(cell_x, cell_y)`.
    pub fn next_altitude_order_at(&self, cell_x: i32, cell_y: i32) -> u8 {
        self.cell_altitudes
            .get(&(cell_x, cell_y))
            .and_then(BTreeMap::last_key_value)
            .map_or(0, |(&max_alt, _)| max_alt.saturating_add(1))
    }

    /// Records an element's altitude order in the per-cell index.
    fn register_altitude(&mut self, cell: (i32, i32), altitude_order: u8) {
        *self
            .cell_altitudes
            .entry(cell)
            .or_default()
            .entry(altitude_order)
            .or_insert(0) += 1;
    }

    /// Removes an element's altitude order from the per-cell index.
    fn unregister_altitude(&mut self, cell: (i32, i32), altitude_order: u8) {
        let Some(altitudes) = self.cell_altitudes.get_mut(&cell) else {
            return;
        };
        if let Some(count) = altitudes.get_mut(&altitude_order) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                altitudes.remove(&altitude_order);
            }
        }
        if altitudes.is_empty() {
            self.cell_altitudes.remove(&cell);
        }
    }

    pub fn build_renderable(&mut self, spec: RenderableSpec<'_>) -> Renderable {
        let RenderableSpec {
            def,
            cell_x,
            cell_y,
            cell_z,
            height,
            altitude_order,
            group,
            layer,
            color,
        } = spec;
        let uid = self.allocate_uid();
        let (sx, sy) = iso_to_screen(cell_x, cell_y, i32::from(cell_z) - i32::from(height));
        let (origin_x, origin_y) = def.origin();
        let x = sx - origin_x as f32;
        let y = sy + origin_y as f32;
        let (sample_width, sample_height) = def.size();
        let (display_width, display_height) = def.texture_size();
        let before_mobile = def.flags().is_before_mobile();
        let hashcode = element_hashcode(
            cell_x,
            cell_y,
            sorted_altitude_order(altitude_order, before_mobile),
        );
        let envelope = AABB::from_corners(
            (x, y),
            (x + f32::from(display_width), y - f32::from(display_height)),
        );
        let z = self.place_z(envelope, hashcode, None);
        let light = *self.light_map.get_color(cell_x, cell_y, layer as i32);
        let [lr, lg, lb] = LightingState::default().shade(&light);
        let color_tinted = LinearRgba::new(
            color.red * lr,
            color.green * lg,
            color.blue * lb,
            color.alpha,
        );

        Renderable {
            uid,
            definition_id: def.id(),
            position: Vec3::new(x, y, z),
            origin: (origin_x, origin_y),
            cell_x,
            cell_y,
            cell_z,
            height,
            altitude_order,
            before_mobile,
            hashcode,
            texture_size: UVec2::new(sample_width.into(), sample_height.into()),
            render_size: UVec2::new(display_width.into(), display_height.into()),
            color,
            color_tinted,
            light,
            texture_id: def.texture_id(),
            flip_x: def.flags().is_flip(),
            animation: def.animation(),
            group,
            layer,
            visibility_mask: def.visibility_mask(),
            group_alpha: 1.0,
            id: None,
        }
    }

    pub fn get_element(&self, uid: u64) -> Option<&Renderable> {
        let env = self.uid_envelopes.get(&uid)?;
        self.rtree
            .locate_in_envelope_intersecting(*env)
            .find(|r| r.uid == uid)
    }

    pub fn insert_element(&mut self, mut elem: Renderable) {
        elem.id = None;
        let env = elem.envelope();
        let cell = (elem.cell_x, elem.cell_y);
        let altitude_order = elem.altitude_order;
        self.uid_envelopes.insert(elem.uid, env);
        if let Err(idx) = self.used_definitions.binary_search(&elem.definition_id) {
            self.used_definitions.insert(idx, elem.definition_id);
        }
        if let Err(idx) = self.available_layers.binary_search(&elem.layer) {
            self.available_layers.insert(idx, elem.layer);
        }
        self.rtree.insert(elem);
        self.register_altitude(cell, altitude_order);
    }

    pub fn remove_element(&mut self, uid: u64) -> Option<Renderable> {
        let env = self.uid_envelopes.remove(&uid)?;
        let target = self
            .rtree
            .locate_in_envelope_intersecting(env)
            .find(|r| r.uid == uid)?
            .clone();
        let removed = self.rtree.remove(&target)?;
        self.unregister_altitude((removed.cell_x, removed.cell_y), removed.altitude_order);
        if let Some(entity) = removed.id {
            self.visible.remove(&entity);
            self.spare_visible.remove(&entity);
            self.pending_despawns.push(entity);
        }
        Some(removed)
    }

    #[cfg(test)]
    pub fn attach_entity_for_test(&mut self, uid: u64, entity: Entity) {
        if let Some(&env) = self.uid_envelopes.get(&uid)
            && let Some(elem) = self
                .rtree
                .locate_in_envelope_intersecting_mut(env)
                .find(|r| r.uid == uid)
        {
            elem.id = Some(entity);
        }
    }

    pub fn update_element(&mut self, mut elem: Renderable) -> Option<Renderable> {
        let env = self.uid_envelopes.remove(&elem.uid)?;
        let target = self
            .rtree
            .locate_in_envelope_intersecting(env)
            .find(|r| r.uid == elem.uid)?
            .clone();
        let old = self.rtree.remove(&target)?;
        self.unregister_altitude((old.cell_x, old.cell_y), old.altitude_order);
        elem.id = old.id;
        let new_env = elem.envelope();
        let cell = (elem.cell_x, elem.cell_y);
        let altitude_order = elem.altitude_order;
        self.uid_envelopes.insert(elem.uid, new_env);
        if let Err(idx) = self.available_layers.binary_search(&elem.layer) {
            self.available_layers.insert(idx, elem.layer);
        }
        self.rtree.insert(elem);
        self.register_altitude(cell, altitude_order);
        Some(old)
    }

    pub fn pick_at_world(
        &self,
        wx: f32,
        wy: f32,
        settings: &MapViewSettings,
        images: &Assets<Image>,
        elapsed: Duration,
    ) -> Option<&Renderable> {
        let mut candidates: Vec<&Renderable> = self
            .rtree
            .locate_in_envelope_intersecting(AABB::from_point((wx, wy)))
            .filter(|elem| {
                (!settings.layer_filter_on || settings.layer == elem.layer)
                    && (!settings.group_filter_on || settings.group == elem.group / 1000)
                    && visibility_mask_allows(elem.visibility_mask, settings.visibility_mask)
            })
            .collect();
        candidates.sort_by(|a, b| b.position.z.total_cmp(&a.position.z));
        candidates
            .iter()
            .copied()
            .find(|elem| {
                self.is_opaque_at_world(elem, wx, wy, images, elapsed, 8)
            })
            .or_else(|| {
                candidates
                    .into_iter()
                    .find(|elem| self.is_opaque_at_world(elem, wx, wy, images, elapsed, 1))
            })
    }

    /// Returns whether `elem` has an opaque texel at the given world position.
    fn is_opaque_at_world(
        &self,
        elem: &Renderable,
        wx: f32,
        wy: f32,
        images: &Assets<Image>,
        elapsed: Duration,
        alpha_threshold: u8,
    ) -> bool {
        if elem.color.alpha <= 0.01 {
            return false;
        }
        let (rw, rh) = (elem.render_size.x, elem.render_size.y);
        if rw == 0 || rh == 0 {
            return false;
        }
        let mut u = (wx - elem.position.x) / (rw as f32);
        let v = (elem.position.y - wy) / (rh as f32);
        if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) {
            return false;
        }
        if elem.flip_x {
            u = 1.0 - u;
        }

        let (min_x, min_y, sub_w, sub_h) = match &elem.animation {
            Animation::None => (0, 0, elem.texture_size.x, elem.texture_size.y),
            Animation::Frames(frames) => {
                let idx = frame_index(
                    frames,
                    animation_elapsed(elapsed, animation_phase(elem.uid)),
                );
                if let Some([x, y]) = frames.frame_coords(idx) {
                    (
                        u32::from(x),
                        u32::from(y),
                        u32::from(frames.width()),
                        u32::from(frames.height()),
                    )
                } else {
                    (0, 0, elem.texture_size.x, elem.texture_size.y)
                }
            }
        };
        if sub_w == 0 || sub_h == 0 {
            return false;
        }

        let px = min_x + (u.clamp(0.0, 0.999_999) * sub_w as f32) as u32;
        let py = min_y + (v.clamp(0.0, 0.999_999) * sub_h as f32) as u32;

        // `DisplayedScreenElement.fineHitTest` uses the TGAM alpha mask rather
        // than the RGBA alpha channel.
        if let Some(mask) = self.masks.get(elem.texture_id) {
            return mask.contains(px, py);
        }

        let Some((img_handle, _)) = self.sprite_assets.get(&elem.definition_id) else {
            return true;
        };
        let Some(img) = images.get(img_handle) else {
            return true;
        };
        let Some(data) = img.data.as_ref() else {
            return true;
        };

        let img_w = img.width();
        let img_h = img.height();
        if px >= img_w || py >= img_h {
            return false;
        }

        let alpha_idx = ((py * img_w + px) * 4 + 3) as usize;
        data.get(alpha_idx).is_some_and(|&a| a >= alpha_threshold)
    }

    /// Despawns every sprite entity spawned for this map.
    ///
    /// Off-screen sprites are hidden rather than despawned, so map switches and
    /// teardown must walk the whole tree rather than just the visible set.
    pub fn despawn_all(&mut self, commands: &mut Commands<'_, '_>) {
        for elem in &self.rtree {
            if let Some(entity) = elem.id
                && commands.get_entity(entity).is_ok()
            {
                commands.entity(entity).despawn();
            }
        }
        for entity in self.pending_despawns.drain(..) {
            if commands.get_entity(entity).is_ok() {
                commands.entity(entity).despawn();
            }
        }
        self.visible.clear();
        self.spare_visible.clear();
    }
}

#[derive(Debug, Clone)]
pub struct Renderable {
    pub uid: u64,
    pub definition_id: i32,
    pub position: Vec3,
    pub origin: (i16, i16),
    pub cell_x: i32,
    pub cell_y: i32,
    pub cell_z: i16,
    pub height: u8,
    pub altitude_order: u8,
    pub before_mobile: bool,
    pub hashcode: i64,
    pub texture_size: UVec2,
    pub render_size: UVec2,
    pub color: LinearRgba,
    pub color_tinted: LinearRgba,
    /// Static light definition for this element's cell and layer.
    pub light: LightDef,
    pub texture_id: i32,
    pub flip_x: bool,
    pub animation: Animation,
    pub group: i32,
    pub layer: u8,
    /// Visibility-mask bits this element requires to be shown.
    pub visibility_mask: u8,
    /// Current group fade multiplier (`1.0` visible, `0.0` hidden).
    pub group_alpha: f32,

    pub id: Option<Entity>,
}

impl PartialEq for Renderable {
    fn eq(&self, other: &Self) -> bool {
        self.uid == other.uid
    }
}

impl Renderable {
    /// Recomputes world-space `position` and `hashcode` from `cell_x`, `cell_y`, `cell_z`, `height`, `altitude_order`.
    pub fn recompute_position(&mut self, renderer: &mut MapRenderer) {
        let (sx, sy) = iso_to_screen(
            self.cell_x,
            self.cell_y,
            i32::from(self.cell_z) - i32::from(self.height),
        );
        self.position.x = sx - self.origin.0 as f32;
        self.position.y = sy + self.origin.1 as f32;
        self.hashcode = element_hashcode(
            self.cell_x,
            self.cell_y,
            sorted_altitude_order(self.altitude_order, self.before_mobile),
        );
        self.position.z = renderer.place_z(self.envelope(), self.hashcode, Some(self.uid));
        // Moving an element can change its cell, so refresh the static light.
        self.light = *renderer
            .light_map
            .get_color(self.cell_x, self.cell_y, self.layer as i32);
    }

    /// Recomputes the cached lit colour from `color` and the current lighting.
    pub fn retint(&mut self, lighting: &LightingState) {
        let [r, g, b] = lighting.shade_at(&self.light, self.cell_x as f32, self.cell_y as f32);
        self.color_tinted = LinearRgba::new(
            self.color.red * r,
            self.color.green * g,
            self.color.blue * b,
            self.color.alpha,
        );
    }
}

impl RTreeObject for Renderable {
    type Envelope = AABB<(f32, f32)>;

    fn envelope(&self) -> Self::Envelope {
        AABB::from_corners(
            (self.position.x, self.position.y),
            (
                self.position.x + self.render_size.x as f32,
                self.position.y - self.render_size.y as f32,
            ),
        )
    }
}

#[derive(Component)]
pub struct MapSprite;

/// Marks a map element that the reference `Sea` system scales sinusoidally.
/// The cell coordinates drive the wave phase, matching `Sea.update`.
#[derive(Component)]
pub struct WaterElement {
    cell_x: f32,
    cell_y: f32,
}

#[derive(Component)]
pub struct AnimationState {
    frames: Arc<Frames>,
    /// Per-element start offset in milliseconds, like `DisplayedScreenElement`
    /// seeding each sprite with a random animation time.
    phase_ms: u32,
}

pub fn load_renderer(
    maps_path: &Path,
    map_id: i32,
    sprites: &MapSpriteLibrary,
    masks: &TgamMaskLibrary,
) -> anyhow::Result<MapRenderer> {
    let map_path = maps_path.join("gfx").join(format!("{map_id}.jar"));
    let light_path = maps_path.join("light").join(format!("{map_id}.jar"));

    let map_file = File::open(&map_path).map_err(|err| {
        anyhow::anyhow!("failed to open map archive {}: {err}", map_path.display())
    })?;
    let map = Map::load(map_file)
        .map_err(|err| anyhow::anyhow!("failed to load map from {}: {err}", map_path.display()))?;

    let light_map = if light_path.exists() {
        let light_file = File::open(&light_path).map_err(|err| {
            anyhow::anyhow!(
                "failed to open light archive {}: {err}",
                light_path.display()
            )
        })?;
        LightMap::load(light_file).map_err(|err| {
            anyhow::anyhow!(
                "failed to load light map from {}: {err}",
                light_path.display()
            )
        })?
    } else {
        LightMap::default()
    };

    let mut renderer = MapRenderer::new(&map, sprites, light_map);
    renderer.masks = TgamMaskLibrary::clone(masks);
    Ok(renderer)
}

#[derive(SystemParam)]
pub struct LoadMapParams<'w, 's> {
    commands: Commands<'w, 's>,
    events: MessageReader<'w, 's, ChangeMapEvent>,
    catalog: ResMut<'w, MapCatalog>,
    render_state: ResMut<'w, MapRenderer>,
    camera_controller: ResMut<'w, CameraController>,
    cameras: Query<'w, 's, &'static mut Transform, With<MainCamera>>,
    pending_load: ResMut<'w, PendingMapLoad>,
    groups: ResMut<'w, GroupVisibilityState>,
    masks: Res<'w, TgamMaskLibrary>,
}

pub fn load_map_system(params: LoadMapParams<'_, '_>) {
    let LoadMapParams {
        mut commands,
        mut events,
        mut catalog,
        mut render_state,
        mut camera_controller,
        mut cameras,
        mut pending_load,
        mut groups,
        masks,
    } = params;
    if let Some(&ChangeMapEvent(target_map_id)) = events.read().last()
        && pending_load.request(catalog.current_map_id, target_map_id)
    {
        let pool = AsyncComputeTaskPool::get();
        let maps_path = catalog.maps_path.clone();
        let sprites = catalog.sprites.clone();
        let masks = (*masks).clone();
        let task = pool.spawn(async move {
            load_renderer(&maps_path, target_map_id, &sprites, &masks)
        });
        pending_load.task = Some((target_map_id, task));
    }

    if let Some((map_id, task)) = &mut pending_load.task
        && let Some(result) = future::block_on(future::poll_once(task))
    {
        let map_id = *map_id;
        pending_load.task = None;
        match result {
            Ok(new_renderer) => {
                render_state.despawn_all(&mut commands);
                let center = new_renderer.center();
                camera_controller.current_position = Vec3::new(center.x, center.y, 0.0);
                camera_controller.target_position = Vec3::new(center.x, center.y, 0.0);
                if let Ok(mut transform) = cameras.single_mut() {
                    transform.translation.x = center.x;
                    transform.translation.y = center.y;
                }
                *render_state = new_renderer;
                catalog.current_map_id = map_id;
                match GroupVisibility::load_from_map(&catalog.maps_path, map_id) {
                    Ok(visibility) => groups.0 = Some(visibility),
                    Err(err) => {
                        warn!("could not load group visibility: {err}");
                        groups.0 = None;
                    }
                }
                info!("Switched to map {map_id}");
            }
            Err(err) => {
                error!("Failed to load map {map_id}: {err}");
            }
        }
    }
}

type MapSpriteQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut Sprite,
        &'static mut Transform,
        &'static mut Visibility,
    ),
    (With<MapSprite>, Without<MainCamera>),
>;

#[derive(SystemParam)]
pub struct RenderParams<'w, 's> {
    commands: Commands<'w, 's>,
    assets: Res<'w, AssetServer>,
    cameras: Query<'w, 's, (Ref<'static, Camera>, Ref<'static, Transform>), With<MainCamera>>,
    atlas_layouts: ResMut<'w, Assets<TextureAtlasLayout>>,
    render_state: ResMut<'w, MapRenderer>,
    settings: Res<'w, MapViewSettings>,
    groups: Res<'w, GroupVisibilityState>,
    sprites: MapSpriteQuery<'w, 's>,
}

pub fn rendering_system(params: RenderParams<'_, '_>) -> Result {
    let RenderParams {
        mut commands,
        assets,
        cameras,
        mut atlas_layouts,
        mut render_state,
        settings,
        groups,
        mut sprites,
    } = params;
    if !render_state.pending_despawns.is_empty() {
        for entity in render_state.pending_despawns.drain(..) {
            if commands.get_entity(entity).is_ok() {
                commands.entity(entity).despawn();
            }
        }
    }

    let Ok((camera, transform)) = cameras.single() else {
        return Ok(());
    };

    if !camera.is_changed()
        && !transform.is_changed()
        && !render_state.is_changed()
        && !settings.is_changed()
    {
        return Ok(());
    }
    let Some(aabb) = camera_bounds(&camera, &GlobalTransform::from(*transform)) else {
        return Ok(());
    };

    let sync_existing = settings.is_changed() || render_state.is_changed();
    let MapRenderer {
        rtree,
        visible,
        spare_visible: seen,
        sprite_assets,
        ..
    } = &mut *render_state;
    seen.clear();

    // The reference keys group visibility on the group under the camera (the
    // player's walkable cell). Use the topmost element at the camera centre.
    let group_from = groups
        .0
        .as_ref()
        .map(|_| camera_group(rtree, transform.translation.truncate()));

    for elem in rtree.locate_in_envelope_intersecting_mut(aabb) {
        if settings.layer_filter_on && settings.layer != elem.layer {
            continue;
        }
        if settings.group_filter_on && settings.group != elem.group / 1000 {
            continue;
        }
        if !visibility_mask_allows(elem.visibility_mask, settings.visibility_mask) {
            continue;
        }
        if let (Some(visibility), Some(from)) = (groups.0.as_ref(), group_from)
            && !visibility.layer_visible(from, elem.group)
        {
            continue;
        }

        let entity = match elem.id {
            Some(id) if commands.get_entity(id).is_ok() => {
                // Re-sync a sprite that re-enters the view even when nothing
                // else changed, since its properties may have been edited
                // while it was hidden.
                let was_visible = visible.contains(&id);
                if (sync_existing || !was_visible)
                    && let Ok((mut sprite, mut elem_transform, mut visibility)) =
                        sprites.get_mut(id)
                {
                    let mut active_color = if settings.enable_light {
                        elem.color_tinted
                    } else {
                        elem.color
                    };
                    active_color.alpha *= elem.group_alpha;
                    let color = premultiplied_tint(active_color);
                    if sprite.color != color {
                        sprite.color = color;
                    }
                    if elem_transform.translation != elem.position {
                        elem_transform.translation = elem.position;
                    }
                    visibility.set_if_neq(Visibility::Inherited);
                }
                id
            }
            _ => {
                let entity = render(
                    &mut commands,
                    &assets,
                    &mut atlas_layouts,
                    sprite_assets,
                    elem,
                    settings.enable_light,
                );
                elem.id = Some(entity);
                entity
            }
        };
        seen.insert(entity);
    }

    // Hide sprites that left the view instead of despawning them. Keeping the
    // entities alive avoids per-pan archetype churn and lets Bevy skip them
    // entirely while hidden.
    for entity in visible.iter() {
        if seen.contains(entity) {
            continue;
        }
        if let Ok((_, _, mut visibility)) = sprites.get_mut(*entity) {
            visibility.set_if_neq(Visibility::Hidden);
        }
    }

    mem::swap(visible, seen);

    Ok(())
}

/// Recomputes cached lit colours when the time-of-day lighting changes.
///
/// `rendering_system` re-syncs visible sprites whenever [`MapRenderer`] is
/// changed, so updating the cached colours here is enough to refresh the view.
pub fn lighting_system(
    settings: Res<'_, MapViewSettings>,
    mut render_state: ResMut<'_, MapRenderer>,
    mut last: Local<'_, Option<LightingState>>,
) {
    if last.as_ref() == Some(&settings.lighting) {
        return;
    }
    let lighting = settings.lighting.clone();
    for elem in &mut render_state.rtree {
        elem.retint(&lighting);
    }
    *last = Some(lighting);
}

pub fn animation_system(
    time: Res<'_, Time>,
    mut query: Query<'_, '_, (&AnimationState, &mut Sprite, &Visibility)>,
) {
    for (state, mut sprite, visibility) in &mut query {
        // Off-screen sprites are hidden, so skip their frame bookkeeping too.
        if *visibility == Visibility::Hidden {
            continue;
        }
        let index = frame_index(
            &state.frames,
            animation_elapsed(time.elapsed(), state.phase_ms),
        );
        if let Some(atlas) = sprite.texture_atlas.as_mut()
            && atlas.index != index
        {
            atlas.index = index;
        }
    }
}

/// Returns whether `id` is one of the reference `Sea.WATER_ELEMENTS`.
fn is_water_element(id: i32) -> bool {
    WATER_ELEMENTS.contains(&id)
}

/// The scale factor `Sea.update` applies to a water element.
///
/// The reference accumulates elapsed milliseconds and uses
/// `distance + speed * position / 1000`, which is equivalent to
/// `distance + speed * elapsed_secs`.
fn water_scale(cell_x: f32, cell_y: f32, elapsed_secs: f32) -> f32 {
    let distance = (cell_x * cell_x + cell_y * cell_y).sqrt() + WATER_SPEED * elapsed_secs;
    let mut amplitude = WATER_AMPLITUDE * (std::f32::consts::TAU * distance / WATER_WIDTH).sin();
    if amplitude < 0.0 {
        amplitude *= 0.2;
    }
    1.0 + amplitude
}

/// Applies the reference `Sea` wave to water elements.
pub fn sea_system(
    time: Res<'_, Time>,
    mut query: Query<'_, '_, (&WaterElement, &mut Transform, &Visibility)>,
) {
    let elapsed = time.elapsed_secs();
    for (water, mut transform, visibility) in &mut query {
        // `Sea.update` only touches visible elements.
        if *visibility == Visibility::Hidden {
            continue;
        }
        transform.scale = Vec3::splat(water_scale(water.cell_x, water.cell_y, elapsed));
    }
}

/// Moves `current` toward `target` at the reference
/// `GroupLayerManager.Fader` speed of `1/300` per millisecond.
fn fade_alpha(current: f32, target: f32, delta_secs: f32) -> f32 {
    let step = (delta_secs * 1000.0 / 300.0).clamp(0.0, 1.0);
    if target > current {
        (current + step).min(target)
    } else {
        (current - step).max(target)
    }
}

/// Fades elements when their group becomes visible or hidden.
///
/// `GroupLayerManager` keys this on the group under the camera; elements with
/// a hidden group fade to zero alpha over `FADE_TIME` (300 ms).
pub fn group_fade_system(
    time: Res<'_, Time>,
    cameras: Query<'_, '_, &Transform, With<MainCamera>>,
    groups: Res<'_, GroupVisibilityState>,
    mut render_state: ResMut<'_, MapRenderer>,
) {
    let Some(visibility) = groups.0.as_ref() else {
        return;
    };
    let Ok(transform) = cameras.single() else {
        return;
    };

    let from = camera_group(&render_state.rtree, transform.translation.truncate());
    let needs_update = render_state.rtree.iter().any(|elem| {
        let target = if visibility.layer_visible(from, elem.group) {
            1.0
        } else {
            0.0
        };
        elem.group_alpha != target
    });
    if !needs_update {
        return;
    }

    let delta = time.delta_secs();
    for elem in &mut render_state.rtree {
        let target = if visibility.layer_visible(from, elem.group) {
            1.0
        } else {
            0.0
        };
        elem.group_alpha = fade_alpha(elem.group_alpha, target, delta);
    }
}

fn render(
    commands: &mut Commands<'_, '_>,
    assets: &Res<'_, AssetServer>,
    atlas_layouts: &mut ResMut<'_, Assets<TextureAtlasLayout>>,
    sprite_assets: &mut HashMap<i32, (Handle<Image>, Handle<TextureAtlasLayout>)>,
    renderable: &Renderable,
    enable_light: bool,
) -> Entity {
    let (img, layout) = sprite_assets
        .entry(renderable.definition_id)
        .or_insert_with(|| {
            let img = assets.load::<Image>(format!("gfx://gfx/{}.tgam", renderable.texture_id));
            (img, atlas_layouts.add(atlas_layout(renderable)))
        });

    let mut sprite = Sprite::from_atlas_image(img.clone(), layout.clone().into());
    let mut active_color = if enable_light {
        renderable.color_tinted
    } else {
        renderable.color
    };
    active_color.alpha *= renderable.group_alpha;
    sprite.color = premultiplied_tint(active_color);
    sprite.flip_x = renderable.flip_x;
    sprite.custom_size = Some(renderable.render_size.as_vec2());
    let mut entity = commands.spawn((
        sprite,
        Anchor::TOP_LEFT,
        Transform::from_translation(renderable.position),
        Visibility::Inherited,
        MapSprite,
        RenderLayers::layer(WORLD_LAYER),
    ));

    if let Animation::Frames(frames) = &renderable.animation {
        entity.insert(AnimationState {
            frames: frames.clone(),
            phase_ms: animation_phase(renderable.uid),
        });
    }

    if is_water_element(renderable.definition_id) {
        entity.insert(WaterElement {
            cell_x: renderable.cell_x as f32,
            cell_y: renderable.cell_y as f32,
        });
    }

    entity.id()
}

/// Returns the group key of the topmost element at the camera centre.
fn camera_group(rtree: &RTree<Renderable>, center: Vec2) -> i32 {
    let (cell_x, cell_y) = screen_to_iso(center.x, center.y, 0);
    let point = AABB::from_point((center.x, center.y));
    rtree
        .locate_in_envelope_intersecting(point)
        .filter(|elem| elem.cell_x == cell_x && elem.cell_y == cell_y)
        .max_by(|a, b| a.position.z.total_cmp(&b.position.z))
        .map_or(0, |elem| elem.group)
}

/// Mirrors `DisplayedScreenElement.checkVisibilityMask`: the reference shows an
/// element when all of its own mask bits are present in the requested LOD mask.
fn visibility_mask_allows(element_mask: u8, requested: u8) -> bool {
    element_mask & requested == element_mask
}

fn camera_bounds(camera: &Camera, transform: &GlobalTransform) -> Option<AABB<(f32, f32)>> {
    // Use all four corners: two diagonal corners do not bound a rotated view.
    let mut min = Vec2::splat(f32::INFINITY);
    let mut max = Vec2::splat(f32::NEG_INFINITY);
    for corner in [
        Vec2::NEG_ONE,
        Vec2::new(-1.0, 1.0),
        Vec2::ONE,
        Vec2::new(1.0, -1.0),
    ] {
        let point = camera
            .ndc_to_world(transform, corner.extend(1.0))?
            .truncate();
        if !point.is_finite() {
            return None;
        }
        min = min.min(point);
        max = max.max(point);
    }
    Some(AABB::from_corners((min.x, min.y), (max.x, max.y)))
}

fn frame_index(frames: &Frames, elapsed: Duration) -> usize {
    // Callers pass the element's phase-adjusted clock.
    if frames.total_time() == 0 {
        return 0;
    }
    let elapsed = (elapsed.as_millis() % u128::from(frames.total_time())) as u32;
    frames.frame_index_at(elapsed)
}

/// Returns the animation clock for an element with a per-instance phase.
fn animation_elapsed(base: Duration, phase_ms: u32) -> Duration {
    base + Duration::from_millis(u64::from(phase_ms))
}

/// Derives a stable 16-bit animation phase from the element uid.
///
/// `DisplayedScreenElement.createSprite` seeds each element with a random
/// 16-bit time (`MathHelper.random(Integer.MAX_VALUE)` truncated to a short),
/// so instances of the same sprite do not animate in lockstep.
fn animation_phase(uid: u64) -> u32 {
    let mut x = uid.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x ^= x >> 33;
    x = x.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
    x ^= x >> 33;
    (x >> 16) as u32 & 0xFFFF
}

pub fn premultiplied_tint(color: LinearRgba) -> Color {
    // The texture is already premultiplied, but Bevy's sprite shader also
    // multiplies it by this tint. Tint opacity must attenuate RGB as well.
    //
    // The reference renderer clamps the combined tint to `[0, 1]` before the
    // fixed pipeline applies its `gColorScale` of 2.0, so the folded-in tint
    // is capped at `TINT_SCALE` rather than 1.0.
    let alpha = color.alpha.clamp(0.0, 1.0);
    Color::linear_rgba(
        (color.red * alpha).clamp(0.0, TINT_SCALE),
        (color.green * alpha).clamp(0.0, TINT_SCALE),
        (color.blue * alpha).clamp(0.0, TINT_SCALE),
        alpha,
    )
}

pub fn atlas_layout(renderable: &Renderable) -> TextureAtlasLayout {
    atlas_layout_parts(renderable.texture_size, &renderable.animation)
}

/// Builds a texture atlas for a sprite.
///
/// `sample_size` is the sampled rectangle for a static sprite (`render_width`
/// x `render_height`). The atlas canvas must match the padded power-of-two
/// dimensions of the loaded TGAM image, otherwise the UVs are normalised
/// against the wrong size: for static sprites that is `pow2(sample_size)`, and
/// for animated sprites the full sprite sheet `pow2(full_width x full_height)`.
///
/// The reference engine insets its UVs by half a texel and subtracts half a
/// texel from the padded height (`AnimData.computeTextureCoords`). That is a
/// fixed-pipeline `GL_NEAREST` pixel-centre adjustment; Bevy's atlas uses
/// exact texel-edge rectangles, which sample the same texels without the
/// legacy offset.
pub fn atlas_layout_parts(sample_size: UVec2, animation: &Animation) -> TextureAtlasLayout {
    let canvas = match animation {
        Animation::None => padded(sample_size),
        Animation::Frames(frames) => UVec2::new(
            round_up_to_power_of_two(frames.full_width().into()),
            round_up_to_power_of_two(frames.full_height().into()),
        ),
    };
    let mut layout = TextureAtlasLayout::new_empty(canvas);
    match animation {
        Animation::None => {
            layout.add_texture(URect {
                min: UVec2::ZERO,
                max: sample_size,
            });
        }
        Animation::Frames(frames) => {
            for frame in frames.iter() {
                let min = UVec2::new(frame.x.into(), frame.y.into());
                let max = UVec2::new(
                    u32::from(frame.x) + u32::from(frames.width()),
                    u32::from(frame.y) + u32::from(frames.height()),
                );
                layout.add_texture(URect { min, max });
            }
        }
    };
    layout
}

/// Rounds both components up to the nearest power of two.
pub fn padded(size: UVec2) -> UVec2 {
    UVec2::new(
        round_up_to_power_of_two(size.x),
        round_up_to_power_of_two(size.y),
    )
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn test_renderable(uid: u64, z: f32, hashcode: i64) -> Renderable {
        Renderable {
            uid,
            definition_id: 1,
            position: Vec3::new(0.0, 0.0, z),
            origin: (0, 0),
            cell_x: 0,
            cell_y: 0,
            cell_z: 0,
            height: 0,
            altitude_order: 0,
            before_mobile: false,
            hashcode,
            texture_size: UVec2::splat(10),
            render_size: UVec2::splat(10),
            color: LinearRgba::WHITE,
            color_tinted: LinearRgba::WHITE,
            light: LightDef::DEFAULT,
            texture_id: 1,
            flip_x: false,
            animation: Animation::None,
            group: 0,
            layer: 0,
            visibility_mask: 0,
            group_alpha: 1.0,
            id: None,
        }
    }

    fn test_renderer(elements: Vec<Renderable>) -> MapRenderer {
        MapRenderer {
            next_uid: elements.iter().map(|elem| elem.uid + 1).max().unwrap_or(1),
            uid_envelopes: elements
                .iter()
                .map(|elem| (elem.uid, elem.envelope()))
                .collect(),
            rtree: RTree::bulk_load(elements),
            visible: HashSet::new(),
            spare_visible: HashSet::new(),
            pending_despawns: Vec::new(),
            sprite_assets: HashMap::new(),
            center: Vec2::ZERO,
            used_definitions: Vec::new(),
            available_layers: Vec::new(),
            cell_altitudes: HashMap::new(),
            light_map: LightMap::default(),
            masks: TgamMaskLibrary::default(),
        }
    }

    fn pending_task(pool: &bevy::tasks::TaskPool) -> Task<anyhow::Result<MapRenderer>> {
        pool.spawn(future::pending())
    }

    #[test]
    fn selecting_current_map_cancels_pending_switch() {
        use byte::TryRead;
        use byte::ctx::LittleEndian;

        let pool = bevy::tasks::TaskPool::new();
        let (sprites, _) = MapSpriteLibrary::try_read(&0u32.to_le_bytes(), LittleEndian).unwrap();
        let mut app = App::new();
        app.add_message::<ChangeMapEvent>()
            .insert_resource(MapCatalog {
                maps_path: std::path::PathBuf::new(),
                sprites: Arc::new(sprites),
                available_maps: vec![1, 2],
                current_map_id: 1,
            })
            .insert_resource(MapRenderer {
                rtree: RTree::new(),
                uid_envelopes: HashMap::new(),
                next_uid: 1,
                visible: HashSet::new(),
                spare_visible: HashSet::new(),
                pending_despawns: Vec::new(),
                sprite_assets: HashMap::new(),
                center: Vec2::ZERO,
                used_definitions: Vec::new(),
                available_layers: Vec::new(),
                cell_altitudes: HashMap::new(),
            light_map: LightMap::default(),
            masks: TgamMaskLibrary::default(),
            })
            .insert_resource(CameraController::default())
            .insert_resource(PendingMapLoad {
                task: Some((2, pending_task(&pool))),
            })
            .insert_resource(GroupVisibilityState::default())
            .insert_resource(TgamMaskLibrary::default())
            .add_systems(Update, load_map_system);

        // The last selection wins, including when it is the displayed map.
        app.world_mut().write_message(ChangeMapEvent(2));
        app.world_mut().write_message(ChangeMapEvent(1));
        app.update();
        assert!(app.world().resource::<PendingMapLoad>().task.is_none());
        assert_eq!(app.world().resource::<MapCatalog>().current_map_id, 1);
    }

    #[test]
    fn repeated_pending_map_selection_keeps_existing_task() {
        let pool = bevy::tasks::TaskPool::new();
        let mut pending = PendingMapLoad {
            task: Some((2, pending_task(&pool))),
        };
        assert!(!pending.request(1, 2));
        assert_eq!(pending.task.as_ref().unwrap().0, 2);
        assert!(pending.request(1, 3));
        assert!(!pending.request(1, 1));
        assert!(pending.task.is_none());
        assert!(pending.request(1, 2));
    }

    #[test]
    fn atlas_layout_static_sprite_uses_padded_canvas() {
        let layout = atlas_layout_parts(UVec2::new(30, 20), &Animation::None);
        // The loaded TGAM is padded to a power of two, so the atlas must match.
        assert_eq!(layout.size, UVec2::new(32, 32));
        assert_eq!(
            layout.textures,
            vec![URect {
                min: UVec2::ZERO,
                max: UVec2::new(30, 20),
            }]
        );
    }

    #[test]
    fn atlas_layout_animated_sprite_uses_padded_sheet_and_frame_rects() {
        use byte::TryRead;
        use byte::ctx::LittleEndian;

        let mut bytes = Vec::new();
        bytes.extend_from_slice(&300u32.to_le_bytes()); // total_time
        // width, height, full_width, full_height, 2 durations, 2 coordinates.
        for value in [20u16, 12, 40, 24, 100, 200, 0, 0, 20, 12] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        let (frames, _) = Frames::try_read(&bytes, (LittleEndian, 2u8)).unwrap();

        let layout = atlas_layout_parts(
            UVec2::new(20, 12),
            &Animation::Frames(Arc::new(frames)),
        );
        assert_eq!(layout.size, UVec2::new(64, 32));
        assert_eq!(
            layout.textures,
            vec![
                URect {
                    min: UVec2::ZERO,
                    max: UVec2::new(20, 12),
                },
                URect {
                    min: UVec2::new(20, 12),
                    max: UVec2::new(40, 24),
                },
            ]
        );
    }

    #[test]
    fn before_mobile_elements_sort_above_ground_layers() {
        let ground = element_hashcode(3, 4, sorted_altitude_order(0, false));
        let before_mobile = element_hashcode(3, 4, sorted_altitude_order(0, true));
        assert!(before_mobile > ground);
        // `is_before_mobile` shifts by `LAYER_COUNT - 1` (12) layers.
        assert_eq!(
            sorted_altitude_order(0, true) - sorted_altitude_order(0, false),
            12
        );
        // Higher altitude orders still sort above lower ones in the same cell.
        assert!(element_hashcode(3, 4, 5) > element_hashcode(3, 4, 4));
    }

    #[test]
    fn group_fade_moves_at_reference_speed() {
        assert!((fade_alpha(1.0, 0.0, 0.15) - 0.5).abs() < 1e-6);
        assert_eq!(fade_alpha(0.0, 1.0, 0.3), 1.0);
        assert_eq!(fade_alpha(0.5, 0.0, 0.0), 0.5);
        assert!((fade_alpha(0.0, 1.0, 0.001) - (0.001 * 1000.0 / 300.0)).abs() < 1e-6);
    }

    #[test]
    fn visibility_mask_requires_all_element_bits() {
        // Mirrors `DisplayedScreenElement.checkVisibilityMask`.
        assert!(visibility_mask_allows(1, 1));
        assert!(visibility_mask_allows(1, 3));
        assert!(visibility_mask_allows(3, 3));
        assert!(visibility_mask_allows(3, 7));
        assert!(visibility_mask_allows(7, 7));

        assert!(!visibility_mask_allows(3, 1));
        assert!(!visibility_mask_allows(7, 3));

        // Elements without a mask are always visible.
        assert!(visibility_mask_allows(0, 0));
        assert!(visibility_mask_allows(0, 1));
    }

    #[test]
    fn sea_wave_scales_water_elements_like_the_reference() {
        assert!(is_water_element(10208));
        assert!(!is_water_element(1));

        // At the wave origin the sine is zero, so the scale is exactly 1.
        assert!((water_scale(0.0, 0.0, 0.0) - 1.0).abs() < 1e-6);
        // A quarter wavelength of distance produces the full positive amplitude.
        assert!((water_scale(WATER_WIDTH / 4.0, 0.0, 0.0) - 1.001).abs() < 1e-6);
        // Elapsed time advances the phase at `m_speed` cells per second.
        let quarter_time = (WATER_WIDTH / 4.0) / WATER_SPEED;
        assert!((water_scale(0.0, 0.0, quarter_time) - 1.001).abs() < 1e-6);
        // At three quarters the negative lobe is damped to a fifth.
        assert!((water_scale(WATER_WIDTH * 0.75, 0.0, 0.0) - 0.9998).abs() < 1e-6);
    }

    #[test]
    fn animation_phase_is_stable_and_in_range() {
        assert_eq!(animation_phase(42), animation_phase(42));
        for uid in 0..1000 {
            assert!(animation_phase(uid) <= 0xFFFF);
        }
        assert_ne!(animation_phase(1), animation_phase(2));
        assert_eq!(
            animation_elapsed(Duration::from_millis(10), 5),
            Duration::from_millis(15)
        );
    }

    #[test]
    fn test_frame_index_calculations() {
        let frames = Frames::new_for_test(300, vec![100, 100, 100]);
        assert_eq!(frame_index(&frames, Duration::from_millis(0)), 0);
        assert_eq!(frame_index(&frames, Duration::from_millis(50)), 0);
        assert_eq!(frame_index(&frames, Duration::from_millis(100)), 1);
        assert_eq!(frame_index(&frames, Duration::from_millis(150)), 1);
        assert_eq!(frame_index(&frames, Duration::from_millis(200)), 2);
        assert_eq!(frame_index(&frames, Duration::from_millis(299)), 2);
        // Loops around at 300ms
        assert_eq!(frame_index(&frames, Duration::from_millis(300)), 0);
        assert_eq!(frame_index(&frames, Duration::from_millis(450)), 1);
    }

    #[test]
    fn test_frame_index_zero_total_time() {
        let frames = Frames::new_for_test(0, vec![]);
        assert_eq!(frame_index(&frames, Duration::from_millis(100)), 0);
    }

    #[test]
    fn test_premultiplied_tint() {
        let tint = premultiplied_tint(LinearRgba::new(1.0, 0.5, 0.25, 0.5));
        assert_eq!(tint, Color::linear_rgba(0.5, 0.25, 0.125, 0.5));

        // Test clamping of alpha > 1.0 or < 0.0
        let clamped_high = premultiplied_tint(LinearRgba::new(1.0, 1.0, 1.0, 2.0));
        assert_eq!(clamped_high, Color::linear_rgba(1.0, 1.0, 1.0, 1.0));

        let clamped_low = premultiplied_tint(LinearRgba::new(1.0, 1.0, 1.0, -0.5));
        assert_eq!(clamped_low, Color::linear_rgba(0.0, 0.0, 0.0, 0.0));

        // The reference clamps the tint to [0, 1] before its 2x RGB scale, so
        // the folded-in tint caps at 2.0 and never goes negative.
        let clamped_rgb = premultiplied_tint(LinearRgba::new(3.0, 1.0, -1.0, 1.0));
        assert_eq!(clamped_rgb, Color::linear_rgba(2.0, 1.0, 0.0, 1.0));
    }

    #[test]
    fn test_pick_at_world_selects_frontmost_sprite_and_respects_alpha() {
        use bevy::asset::RenderAssetUsages;
        use bevy::image::Image;
        use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

        let mut images = Assets::<Image>::default();

        // 2x2 image: top-left transparent, rest opaque
        // Pixels in RGBA: (0,0)->(0,0,0,0), (1,0)->(255,255,255,255), (0,1)->(255,255,255,255), (1,1)->(255,255,255,255)
        let pixels = vec![
            0, 0, 0, 0, // (0,0) transparent
            255, 255, 255, 255, // (1,0) opaque
            255, 255, 255, 255, // (0,1) opaque
            255, 255, 255, 255, // (1,1) opaque
        ];
        let img = Image::new(
            Extent3d {
                width: 2,
                height: 2,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            pixels,
            TextureFormat::Rgba8Unorm,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        );
        let img_handle = images.add(img);

        let mut sprite_assets = HashMap::new();
        let mut atlas_layouts = Assets::<TextureAtlasLayout>::default();
        let layout_handle = atlas_layouts.add(TextureAtlasLayout::new_empty(UVec2::splat(2)));
        sprite_assets.insert(10, (img_handle, layout_handle));

        // Two overlapping sprites:
        // elem_bottom has higher Z (10.0) but is transparent at (0, 0)
        let elem_top_z = Renderable {
            uid: 1,
            definition_id: 10,
            position: Vec3::new(0.0, 20.0, 10.0),
            origin: (0, 0),
            cell_x: 0,
            cell_y: 0,
            cell_z: 0,
            height: 0,
            altitude_order: 1,
            before_mobile: false,
            hashcode: 1,
            texture_size: UVec2::new(2, 2),
            render_size: UVec2::new(20, 20),
            color: LinearRgba::WHITE,
            color_tinted: LinearRgba::WHITE,
            light: LightDef::DEFAULT,
            texture_id: 1,
            flip_x: false,
            animation: Animation::None,
            group: 0,
            layer: 0,
            visibility_mask: 0,
            group_alpha: 1.0,
            id: None,
        };

        // elem_solid has lower Z (5.0) and is solid (no image mapped, treated as opaque)
        let elem_solid = Renderable {
            uid: 2,
            definition_id: 99, // not in sprite_assets -> treated as opaque
            position: Vec3::new(0.0, 20.0, 5.0),
            origin: (0, 0),
            cell_x: 0,
            cell_y: 0,
            cell_z: 0,
            height: 0,
            altitude_order: 0,
            before_mobile: false,
            hashcode: 0,
            texture_size: UVec2::new(2, 2),
            render_size: UVec2::new(20, 20),
            color: LinearRgba::WHITE,
            color_tinted: LinearRgba::WHITE,
            light: LightDef::DEFAULT,
            texture_id: 2,
            flip_x: false,
            animation: Animation::None,
            group: 0,
            layer: 0,
            visibility_mask: 0,
            group_alpha: 1.0,
            id: None,
        };

        let mut renderer = MapRenderer {
            rtree: RTree::bulk_load(vec![elem_top_z.clone(), elem_solid.clone()]),
            uid_envelopes: HashMap::new(),
            next_uid: 3,
            visible: HashSet::new(),
            spare_visible: HashSet::new(),
            pending_despawns: Vec::new(),
            sprite_assets,
            center: Vec2::ZERO,
            used_definitions: vec![10, 99],
            available_layers: vec![0],
            cell_altitudes: HashMap::new(),
            light_map: LightMap::default(),
            masks: TgamMaskLibrary::default(),
        };
        renderer.uid_envelopes.insert(1, elem_top_z.envelope());
        renderer.uid_envelopes.insert(2, elem_solid.envelope());

        let settings = MapViewSettings::default();

        // Clicking at top-left (wx=2, wy=18): elem_top_z is transparent there -> picks elem_solid (uid 2)!
        let picked_trans = renderer.pick_at_world(2.0, 18.0, &settings, &images, Duration::ZERO);
        assert_eq!(picked_trans.map(|r| r.uid), Some(2));

        // Clicking at bottom-right (wx=18, wy=2): elem_top_z is opaque there -> picks elem_top_z (uid 1)!
        let picked_solid = renderer.pick_at_world(18.0, 2.0, &settings, &images, Duration::ZERO);
        assert_eq!(picked_solid.map(|r| r.uid), Some(1));

        // An opaque TGAM mask overrides the RGBA alpha channel.
        renderer
            .masks
            .insert(1, crate::assets::TgamMask::new(vec![0xFF], 1, 2, 2));
        let picked_masked = renderer.pick_at_world(2.0, 18.0, &settings, &images, Duration::ZERO);
        assert_eq!(picked_masked.map(|r| r.uid), Some(1));

        // A fully transparent mask hides an otherwise-opaque element.
        renderer
            .masks
            .insert(1, crate::assets::TgamMask::new(vec![0x00], 1, 2, 2));
        let picked_hidden = renderer.pick_at_world(18.0, 2.0, &settings, &images, Duration::ZERO);
        assert_eq!(picked_hidden.map(|r| r.uid), Some(2));
    }

    #[test]
    fn altitude_index_tracks_inserts_updates_and_removals() {
        let mut renderer = MapRenderer {
            rtree: RTree::new(),
            uid_envelopes: HashMap::new(),
            next_uid: 1,
            visible: HashSet::new(),
            spare_visible: HashSet::new(),
            pending_despawns: Vec::new(),
            sprite_assets: HashMap::new(),
            center: Vec2::ZERO,
            used_definitions: Vec::new(),
            available_layers: Vec::new(),
            cell_altitudes: HashMap::new(),
            light_map: LightMap::default(),
            masks: TgamMaskLibrary::default(),
        };
        assert_eq!(renderer.next_altitude_order_at(1, 2), 0);

        renderer.register_altitude((1, 2), 3);
        renderer.register_altitude((1, 2), 3);
        renderer.register_altitude((1, 2), 250);
        assert_eq!(renderer.next_altitude_order_at(1, 2), 251);

        renderer.unregister_altitude((1, 2), 250);
        assert_eq!(renderer.next_altitude_order_at(1, 2), 4);
        renderer.unregister_altitude((1, 2), 3);
        assert_eq!(renderer.next_altitude_order_at(1, 2), 4);
        renderer.unregister_altitude((1, 2), 3);
        assert_eq!(renderer.next_altitude_order_at(1, 2), 0);
        assert!(renderer.cell_altitudes.is_empty());

        // Saturates rather than wrapping at the top of the range.
        renderer.register_altitude((1, 2), u8::MAX);
        assert_eq!(renderer.next_altitude_order_at(1, 2), u8::MAX);
    }

    #[test]
    fn test_compute_z_above_places_cleanly_on_top() {
        let envelope = AABB::from_corners((0.0, 0.0), (50.0, 50.0));
        let elem1 = test_renderable(1, 10.0, 0);
        let mut elem2 = test_renderable(2, 20.0, 1);
        elem2.altitude_order = 1;

        let mut renderer = test_renderer(vec![elem1, elem2]);

        // Placing above z=10.0 should land between 10.0 and 20.0 (15.0)
        let z_between = renderer.place_above(10.0, envelope, None);
        assert!(z_between > 10.0 && z_between < 20.0);

        // Placing above z=20.0 (highest) should land above 20.0
        let z_top = renderer.place_above(20.0, envelope, None);
        assert!(z_top > 20.0);

        // frontmost_z_for should be strictly greater than max existing (20.0)
        assert!(renderer.frontmost_z_for(envelope, None) > 20.0);
    }

    #[test]
    fn normalize_z_preserves_visual_order_and_respects_bounds() {
        let mut renderer = test_renderer(vec![
            test_renderable(1, 7.5, 10),
            test_renderable(2, 3.0, 20),
            test_renderable(3, 7.6, 30),
        ]);

        renderer.normalize_z();

        let z1 = renderer.get_element(1).unwrap().position.z;
        let z2 = renderer.get_element(2).unwrap().position.z;
        let z3 = renderer.get_element(3).unwrap().position.z;

        assert!(z2 < z1, "uid 2 started below uid 1");
        assert!(z1 < z3, "uid 3 started above uid 1");
        assert!((Z_MIN + Z_MARGIN - z2).abs() < 1e-3);
        assert!((Z_MAX - Z_MARGIN - z3).abs() < 1e-3);
    }

    #[test]
    fn collapsed_z_gap_renormalizes_instead_of_inverting_order() {
        let mut renderer = test_renderer(vec![
            test_renderable(1, 250.0, 10),
            test_renderable(2, 250.0, 20),
        ]);
        let envelope = AABB::from_point((0.0, 0.0));

        let placed = renderer.place_z(envelope, 15, None);
        let below = renderer.get_element(1).unwrap().position.z;
        let above = renderer.get_element(2).unwrap().position.z;

        assert!(below < placed, "placed below the lower neighbour");
        assert!(placed < above, "placed above the upper neighbour");
        assert!((Z_MIN..=Z_MAX).contains(&placed));
    }

    #[test]
    fn bring_front_and_send_back_stay_inside_the_world_z_range() {
        let envelope = AABB::from_point((0.0, 0.0));
        let mut renderer = test_renderer(vec![test_renderable(0, 250.0, 0)]);
        let mut next_uid = 1;

        for _ in 0..600 {
            let z = renderer.frontmost_z_for(envelope, None);
            let max = renderer
                .rtree
                .iter()
                .map(|elem| elem.position.z)
                .reduce(f32::max)
                .unwrap();
            assert!(
                z > max,
                "frontmost {z} is not above the current top {max}"
            );
            assert!((Z_MIN..=Z_MAX).contains(&z));
            renderer.insert_element(test_renderable(next_uid, z, next_uid as i64));
            next_uid += 1;
        }

        for _ in 0..600 {
            let z = renderer.backmost_z_for(envelope, None);
            let min = renderer
                .rtree
                .iter()
                .map(|elem| elem.position.z)
                .reduce(f32::min)
                .unwrap();
            assert!(z < min, "backmost {z} is not below the current bottom {min}");
            assert!((Z_MIN..=Z_MAX).contains(&z));
            renderer.insert_element(test_renderable(next_uid, z, next_uid as i64));
            next_uid += 1;
        }
    }
}
