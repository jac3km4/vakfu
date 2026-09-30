use std::fs::File;
use std::mem;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use bevy::prelude::*;
use bevy::sprite::Anchor;
use hashbrown::{HashMap, HashSet};
use rstar::{AABB, RTree, RTreeObject};

use crate::assets::{
    Animation, Frames, LightMap, Map, MapChunk, MapElementDetails, MapSpriteLibrary, Rgba,
};
use crate::camera::CameraController;
use crate::settings::MapViewSettings;

#[derive(Message, Debug, Clone, Copy)]
pub struct ChangeMapEvent(pub i32);

#[derive(Resource)]
pub struct MapCatalog {
    pub maps_path: std::path::PathBuf,
    pub sprites: Arc<MapSpriteLibrary>,
    pub available_maps: Vec<i32>,
    pub current_map_id: i32,
}

#[derive(Debug, Resource)]
pub struct MapRenderer {
    rtree: RTree<Renderable>,
    last_seen: HashSet<Entity>,
    spare_seen: HashSet<Entity>,
    sprite_assets: HashMap<i32, (Handle<Image>, Handle<TextureAtlasLayout>)>,
    center: Vec2,
}

impl MapRenderer {
    pub fn new(map: &Map, sprites: &MapSpriteLibrary, light_map: &LightMap) -> Self {
        let mut elements = map
            .chunks()
            .iter()
            .flat_map(MapChunk::elements)
            .collect::<Vec<_>>();
        elements.sort_by_key(MapElementDetails::hashcode);

        let mut min_x = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_y = f32::NEG_INFINITY;

        let sprites = elements
            .iter()
            .enumerate()
            .map(|(z_index, elem)| {
                let def = &sprites[elem.definition_id()];

                let (x, y) = elem.screen_position();
                let (origin_x, origin_y) = def.origin();
                let x = x - origin_x as f32;
                let y = y + origin_y as f32;
                let z = z_index as f32 / elements.len() as f32;

                // Preserve the asset format's established mapping: size() is
                // the sampled rectangle, texture_size() is the world-space size.
                let (sample_width, sample_height) = def.size();
                let (display_width, display_height) = def.texture_size();

                min_x = min_x.min(x);
                max_x = max_x.max(x + display_width as f32);
                min_y = min_y.min(y - display_height as f32);
                max_y = max_y.max(y);

                let light_def =
                    light_map.get_color(elem.cell_x(), elem.cell_y(), elem.group().layer() as i32);
                let [r, g, b, a] = Rgba::from(elem.color()).to_f32_array();
                let color = LinearRgba::new(r, g, b, a);
                let color_tinted = {
                    let [r, g, b] = light_def.ambiance_light;
                    LinearRgba::new(color.red * r, color.green * g, color.blue * b, color.alpha)
                };

                Renderable {
                    position: Vec3::new(x, y, z),
                    definition_id: elem.definition_id(),
                    texture_size: UVec2::new(sample_width.into(), sample_height.into()),
                    render_size: UVec2::new(display_width.into(), display_height.into()),
                    color,
                    color_tinted,
                    texture_id: def.texture_id(),
                    flip_x: def.flags().is_flip(),
                    animation: def.animation(),
                    group: elem.group().key(),
                    layer: elem.group().layer(),

                    id: None,
                }
            })
            .collect();

        let center = if elements.is_empty() {
            Vec2::ZERO
        } else {
            Vec2::new((min_x + max_x) * 0.5, (min_y + max_y) * 0.5)
        };

        Self {
            rtree: RTree::bulk_load(sprites),
            last_seen: HashSet::new(),
            spare_seen: HashSet::new(),
            sprite_assets: HashMap::new(),
            center,
        }
    }

    pub fn center(&self) -> Vec2 {
        self.center
    }

    pub fn despawn_visible(&mut self, commands: &mut Commands<'_, '_>) {
        for entity in self.last_seen.drain() {
            if commands.get_entity(entity).is_ok() {
                commands.entity(entity).despawn();
            }
        }
    }
}

#[derive(Debug)]
struct Renderable {
    definition_id: i32,
    position: Vec3,
    texture_size: UVec2,
    render_size: UVec2,
    color: LinearRgba,
    color_tinted: LinearRgba,
    texture_id: i32,
    flip_x: bool,
    animation: Animation,
    group: i32,
    layer: u8,

    id: Option<Entity>,
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
pub struct AnimationState {
    frames: Arc<Frames>,
}

pub fn load_renderer(
    maps_path: &Path,
    map_id: i32,
    sprites: &MapSpriteLibrary,
) -> anyhow::Result<MapRenderer> {
    let map_path = maps_path.join("gfx").join(format!("{map_id}.jar"));
    let light_path = maps_path.join("light").join(format!("{map_id}.jar"));

    let map = Map::load(File::open(map_path)?)?;

    let light_map = if light_path.exists() {
        LightMap::load(File::open(light_path)?)?
    } else {
        LightMap::default()
    };

    Ok(MapRenderer::new(&map, sprites, &light_map))
}

pub fn load_map_system(
    mut commands: Commands<'_, '_>,
    mut events: MessageReader<'_, '_, ChangeMapEvent>,
    mut catalog: ResMut<'_, MapCatalog>,
    mut render_state: ResMut<'_, MapRenderer>,
    mut camera_controller: ResMut<'_, CameraController>,
    mut cameras: Query<'_, '_, &mut Transform, With<Camera>>,
) {
    for &ChangeMapEvent(map_id) in events.read() {
        if map_id == catalog.current_map_id {
            continue;
        }

        match load_renderer(&catalog.maps_path, map_id, &catalog.sprites) {
            Ok(new_renderer) => {
                render_state.despawn_visible(&mut commands);
                let center = new_renderer.center();
                camera_controller.current_position = Vec3::new(center.x, center.y, 0.0);
                camera_controller.target_position = Vec3::new(center.x, center.y, 0.0);
                if let Ok(mut transform) = cameras.single_mut() {
                    transform.translation.x = center.x;
                    transform.translation.y = center.y;
                }
                *render_state = new_renderer;
                catalog.current_map_id = map_id;
                info!("Switched to map {map_id}");
            }
            Err(err) => {
                error!("Failed to load map {map_id}: {err}");
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn rendering_system(
    mut commands: Commands<'_, '_>,
    assets: Res<'_, AssetServer>,
    cameras: Query<'_, '_, (Ref<'_, Camera>, Ref<'_, Transform>)>,
    mut atlas_layouts: ResMut<'_, Assets<TextureAtlasLayout>>,
    mut render_state: ResMut<'_, MapRenderer>,
    settings: Res<'_, MapViewSettings>,
    mut sprites: Query<'_, '_, &mut Sprite>,
) -> Result {
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

    let update_color = settings.is_changed();
    let MapRenderer {
        rtree,
        last_seen,
        spare_seen: seen,
        sprite_assets,
        ..
    } = &mut *render_state;
    seen.clear();

    for elem in rtree.locate_in_envelope_intersecting_mut(aabb) {
        if settings.layer_filter_on && settings.layer != elem.layer {
            continue;
        }
        if settings.group_filter_on && settings.group != elem.group / 1000 {
            continue;
        }

        let entity = match elem.id {
            Some(id) if commands.get_entity(id).is_ok() => {
                if update_color && let Ok(mut sprite) = sprites.get_mut(id) {
                    let active_color = if settings.enable_light {
                        elem.color_tinted
                    } else {
                        elem.color
                    };
                    let color = premultiplied_tint(active_color);
                    if sprite.color != color {
                        sprite.color = color;
                    }
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

    for elem in last_seen.difference(seen) {
        commands.entity(*elem).despawn();
    }

    mem::swap(last_seen, seen);

    Ok(())
}

pub fn animation_system(
    time: Res<'_, Time>,
    mut query: Query<'_, '_, (&AnimationState, &mut Sprite)>,
) {
    for (state, mut sprite) in &mut query {
        let index = frame_index(&state.frames, time.elapsed());
        if sprite
            .texture_atlas
            .as_ref()
            .is_some_and(|atlas| atlas.index != index)
        {
            sprite.texture_atlas.as_mut().unwrap().index = index;
        }
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
    let active_color = if enable_light {
        renderable.color_tinted
    } else {
        renderable.color
    };
    sprite.color = premultiplied_tint(active_color);
    sprite.flip_x = renderable.flip_x;
    sprite.custom_size = Some(renderable.render_size.as_vec2());
    let mut entity = commands.spawn((
        sprite,
        Anchor::TOP_LEFT,
        Transform::from_translation(renderable.position),
    ));

    if let Animation::Frames(frames) = &renderable.animation {
        entity.insert(AnimationState {
            frames: frames.clone(),
        });
    }

    entity.id()
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
    // A shared clock keeps instances synchronized, including after culling.
    if frames.total_time() == 0 {
        return 0;
    }
    let elapsed = elapsed.as_millis() % u128::from(frames.total_time());
    frames
        .iter()
        .take_while(|frame| u128::from(frame.time) <= elapsed)
        .count()
        .saturating_sub(1)
}

fn premultiplied_tint(color: LinearRgba) -> Color {
    // The texture is already premultiplied, but Bevy's sprite shader also
    // multiplies it by this tint. Tint opacity must attenuate RGB as well.
    Color::linear_rgba(
        color.red * color.alpha,
        color.green * color.alpha,
        color.blue * color.alpha,
        color.alpha,
    )
}

fn atlas_layout(renderable: &Renderable) -> TextureAtlasLayout {
    let mut layout = TextureAtlasLayout::new_empty(renderable.texture_size);
    match &renderable.animation {
        Animation::None => {
            layout.add_texture(URect {
                min: UVec2::ZERO,
                max: renderable.texture_size,
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
