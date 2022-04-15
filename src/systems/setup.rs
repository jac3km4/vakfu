use std::collections::HashMap;

use bevy::prelude::*;
use bevy::sprite::{Anchor, Rect};
use glam::const_vec2;
use itertools::Itertools;

use super::render::MapChunkView;
use crate::map::element::{ElementLibrary, MapElement};
use crate::map::sprite::MapSprite;
use crate::map::Map;
use crate::systems::render::{
    AnimatedSpriteBundle, Animation, SpriteProperties, StaticSpriteBundle
};

pub fn setup_system(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    library: Res<ElementLibrary>,
    map: Res<Map>,
    mut atlases: ResMut<Assets<TextureAtlas>>,
) {
    let z_orders = compute_z_orders(&map);
    let mut atlas_cache = HashMap::new();

    commands.spawn_bundle(OrthographicCameraBundle::new_2d());

    for chunk in map.chunks() {
        let mut elements = vec![];

        for sprite in &chunk.sprites {
            if let Some(elem) = library.get(sprite.element_id) {
                let z_order = *z_orders.get(&sprite.hashcode()).unwrap();
                let z_pos = z_order as f32 / z_orders.len() as f32;
                let texture = asset_server.load(&format!("gfx/{}.tgam", elem.texture_id));

                let handle = atlas_cache.entry(elem.id).or_insert_with(|| {
                    let rects = elem
                        .animation
                        .as_ref()
                        .map(|frames| frames.frame_rects.as_slice());
                    let atlas =
                        new_atlas(texture, elem.image_size(), rects.unwrap_or(&[elem.rect()]));
                    atlases.add(atlas)
                });
                let entity = spawn_sprite(&mut commands, sprite, elem, handle.clone(), z_pos);
                elements.push(entity);
            }
        }

        commands.spawn().insert(MapChunkView::new(chunk, elements));
    }
}

fn spawn_sprite(
    commands: &mut Commands,
    sprite: &MapSprite,
    element: &MapElement,
    texture_atlas: Handle<TextureAtlas>,
    z_order: f32,
) -> Entity {
    const FLIP_Y: Vec2 = const_vec2!([1., -1.]);
    // size and origin need to be flipped in the Y dimension for rendering
    let pos = sprite.screen_position() - element.origin() * FLIP_Y;
    let transform = Transform::from_translation(pos.extend(z_order));
    let visibility = Visibility { is_visible: false };
    let properties = SpriteProperties {
        layer: sprite.layer,
        group_key: sprite.group_key,
    };
    let sprite = TextureAtlasSprite {
        flip_x: element.flags.is_flip(),
        color: sprite.color,
        anchor: Anchor::TopLeft,
        ..Default::default()
    };

    match &element.animation {
        None => commands
            .spawn_bundle(StaticSpriteBundle {
                sprite,
                texture_atlas,
                transform,
                visibility,
                properties,
                ..Default::default()
            })
            .id(),
        Some(frames) => {
            let animation = Animation::new(frames);
            commands
                .spawn_bundle(AnimatedSpriteBundle {
                    sprite,
                    texture_atlas,
                    transform,
                    animation,
                    visibility,
                    properties,
                    ..Default::default()
                })
                .id()
        }
    }
}

fn new_atlas(image: Handle<Image>, size: Vec2, rects: &[Rect]) -> TextureAtlas {
    let mut atlas = TextureAtlas::new_empty(image, size);
    for rect in rects {
        atlas.add_texture(*rect);
    }
    atlas
}

fn compute_z_orders(map: &Map) -> HashMap<i64, usize> {
    // pre-calculate z-orders for the entire map
    map.chunks()
        .iter()
        .flat_map(|chunk| &chunk.sprites)
        .map(|sprite| sprite.hashcode())
        .sorted()
        .enumerate()
        .map(|(idx, hashcode)| (hashcode, idx))
        .collect()
}
