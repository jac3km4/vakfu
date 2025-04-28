use std::sync::Arc;
use std::time::Duration;

use bevy::prelude::*;
use bevy::sprite::Anchor;
use bevy::window::PrimaryWindow;
use hashbrown::HashSet;
use rstar::{AABB, RTree, RTreeObject};

use crate::assets::{Animation, Frames, Map, MapChunk, MapElementDetails, MapSpriteLibrary, Rgba};
use crate::settings::MapViewSettings;

#[derive(Debug, Resource)]
pub struct MapRenderer {
    rtree: RTree<Renderable>,
    last_seen: HashSet<Entity>,
}

impl MapRenderer {
    pub fn new(map: &Map, sprites: &MapSpriteLibrary) -> Self {
        let mut elements = map
            .chunks()
            .iter()
            .flat_map(MapChunk::elements)
            .collect::<Vec<_>>();
        elements.sort_by_key(MapElementDetails::hashcode);

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

                let (width, height) = def.size();
                let (texture_width, texture_height) = def.texture_size();

                Renderable {
                    position: Vec3::new(x, y, z),
                    texture_size: UVec2::new(width.into(), height.into()),
                    render_size: UVec2::new(texture_width.into(), texture_height.into()),
                    color: elem.color().into(),
                    texture_id: def.texture_id(),
                    flip_x: def.flags().is_flip(),
                    animation: def.animation(),
                    group: elem.group().key(),
                    layer: elem.group().layer(),

                    id: None,
                }
            })
            .collect();

        Self {
            rtree: RTree::bulk_load(sprites),
            last_seen: HashSet::new(),
        }
    }
}

#[derive(Debug)]
struct Renderable {
    position: Vec3,
    texture_size: UVec2,
    render_size: UVec2,
    color: Rgba,
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
    timer: Timer,
    frames: Arc<Frames>,
}

pub fn rendering_system(
    mut commands: Commands<'_, '_>,
    assets: Res<'_, AssetServer>,
    window: Query<'_, '_, &Window, With<PrimaryWindow>>,
    cameras: Query<'_, '_, &mut Transform, (With<Camera>, Changed<Transform>)>,
    mut atlas_layouts: ResMut<'_, Assets<TextureAtlasLayout>>,
    mut render_state: ResMut<'_, MapRenderer>,
    settings: Res<'_, MapViewSettings>,
) -> Result {
    let Ok(camera) = cameras.single() else {
        return Ok(());
    };

    let window = window.single()?;
    let view_size = window.size() * camera.scale.truncate();
    let view_pos = camera.translation.truncate() - view_size / 2.;
    let aabb = AABB::from_corners(
        (view_pos.x, view_pos.y),
        (view_pos.x + view_size.x, view_pos.y + view_size.y),
    );

    let mut seen = HashSet::new();
    let MapRenderer { rtree, last_seen } = &mut *render_state;

    for elem in rtree.locate_in_envelope_intersecting_mut(&aabb) {
        if settings.layer_filter_on && settings.layer != elem.layer {
            continue;
        }
        if settings.group_filter_on && settings.group != elem.group / 1000 {
            continue;
        }

        let entity = match elem.id {
            Some(id) if commands.get_entity(id).is_ok() => id,
            _ => {
                let entity = render(&mut commands, &assets, &mut atlas_layouts, elem);
                elem.id = Some(entity);
                entity
            }
        };
        seen.insert(entity);
    }

    for elem in last_seen.difference(&seen) {
        commands.entity(*elem).despawn();
    }

    *last_seen = seen;

    Ok(())
}

pub fn animation_system(
    time: Res<'_, Time>,
    mut query: Query<'_, '_, (&mut AnimationState, &mut Sprite)>,
) {
    for (mut state, mut sprite) in &mut query {
        let Some(atlas) = &mut sprite.texture_atlas else {
            continue;
        };

        state.timer.tick(time.delta());

        if let Some((i, _)) = state
            .frames
            .iter()
            .enumerate()
            .take_while(|(_, f)| u128::from(f.time) < state.timer.elapsed().as_millis())
            .last()
        {
            atlas.index = i;
        }
    }
}

fn render(
    commands: &mut Commands<'_, '_>,
    assets: &Res<'_, AssetServer>,
    atlas_layouts: &mut ResMut<'_, Assets<TextureAtlasLayout>>,
    renderable: &Renderable,
) -> Entity {
    let img = assets.load::<Image>(format!("gfx://gfx/{}.tgam", renderable.texture_id));

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
                    (frame.x + frames.width()).into(),
                    (frame.y + frames.height()).into(),
                );
                layout.add_texture(URect { min, max });
            }
        }
    };
    let layout = atlas_layouts.add(layout);

    let mut sprite = Sprite::from_atlas_image(img, layout.into());
    let [r, g, b, a] = renderable.color.to_f32_array();
    sprite.color = Color::linear_rgba(r, g, b, a);
    sprite.flip_x = renderable.flip_x;
    sprite.custom_size = Some(renderable.render_size.as_vec2());
    sprite.anchor = Anchor::TopLeft;

    let mut entity = commands.spawn((sprite, Transform::from_translation(renderable.position)));

    if let Animation::Frames(frames) = &renderable.animation {
        entity.insert(AnimationState {
            timer: Timer::new(
                Duration::from_millis(frames.total_time().into()),
                TimerMode::Repeating,
            ),
            frames: frames.clone(),
        });
    }

    entity.id()
}
