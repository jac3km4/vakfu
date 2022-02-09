use bevy::prelude::*;
use bevy::sprite::Rect;

use crate::map::chunk::MapChunk;
use crate::map::frames::Frames;
use crate::map::iso_to_screen;

#[derive(Default, Component)]
pub struct MapChunkView {
    rect: Rect,
    children: Vec<Entity>,
    previously_visible: bool,
}

impl MapChunkView {
    const CHUNK_HEIGHT: i32 = 200;

    pub fn new(chunk: &MapChunk, children: Vec<Entity>) -> Self {
        let rect = Rect {
            min: iso_to_screen(IVec2::new(chunk.min_x, chunk.min_y), -Self::CHUNK_HEIGHT),
            max: iso_to_screen(IVec2::new(chunk.max_x, chunk.max_y), 0),
        };
        Self {
            rect,
            children,
            previously_visible: false,
        }
    }
}

pub fn map_chunk_view_system(
    windows: Res<Windows>,
    cameras: Query<&mut Transform, With<Camera>>,
    mut sprites: Query<&mut Visibility>,
    mut chunks: Query<&mut MapChunkView>,
) {
    let camera = cameras.single();
    let window_size = if let Some(window) = windows.get_primary() {
        Vec2::new(window.width(), window.height()) * camera.scale.truncate()
    } else {
        return;
    };
    let camera_rect = Rect {
        min: camera.translation.truncate() - window_size,
        max: camera.translation.truncate() + window_size,
    };

    for mut chunk in chunks.iter_mut() {
        let visible = does_intersect(camera_rect, chunk.rect);
        if chunk.previously_visible != visible {
            for entity in &chunk.children {
                let mut vis = sprites.get_mut(*entity).unwrap();
                vis.is_visible = visible;
            }

            chunk.previously_visible = visible;
        }
    }
}

#[derive(Default, Bundle)]
pub struct AnimatedSpriteBundle {
    pub sprite: TextureAtlasSprite,
    pub texture_atlas: Handle<TextureAtlas>,
    pub animation: Animation,
    pub transform: Transform,
    pub global_transform: GlobalTransform,
    pub visibility: Visibility,
}

#[derive(Default, Component)]
pub struct Animation {
    total_time: u32,
    frame_times: Vec<u16>,
}

impl Animation {
    pub fn new(frames: &Frames) -> Self {
        Self {
            total_time: frames.total_time,
            frame_times: frames.frame_times.clone(),
        }
    }
}

pub fn animation_system(time: Res<Time>, mut query: Query<(&Animation, &mut TextureAtlasSprite)>) {
    let ms = time.time_since_startup().as_millis() as u64;
    for (anim, mut sprite) in query.iter_mut() {
        let passed = ms % anim.total_time as u64;
        let index = anim
            .frame_times
            .binary_search(&(passed as u16))
            .unwrap_or_else(|i| i - 1);
        sprite.index = index;
    }
}

#[inline]
fn does_intersect(r1: Rect, r2: Rect) -> bool {
    !(r1.max.x < r2.min.x || r2.max.x < r1.min.x || r1.max.y < r2.min.y || r2.max.y < r1.min.y)
}
