//! Premultiplied-alpha blending for Bevy 0.20's built-in sprite renderer.
//!
//! Add `PremultAlphaPlugin` after `DefaultPlugins`. This affects every `DrawSprite`
//! item (including text rendered as sprites) and Bevy 0.20's `SpriteMeshMaterial`
//! sprites, not other mesh materials or UI. Keep TGAM images premultiplied and
//! `Rgba8Unorm`; the built-in shaders and tint behavior are unchanged.

use std::any::TypeId;
use std::collections::HashMap;

use bevy::core_pipeline::core_2d::Transparent2d;
use bevy::prelude::*;
use bevy::render::render_phase::{DrawFunctions, ViewSortedRenderPhases};
use bevy::render::render_resource::{
    BlendState, CachedRenderPipelineId, PipelineCache, RenderPipelineDescriptor,
};
use bevy::render::{GpuResourceAppExt, Render, RenderApp, RenderSystems};
use bevy::sprite_render::{DrawSprite, RenderMaterial2dInstances, SpriteMeshMaterial};

pub struct PremultAlphaPlugin;

#[derive(Default, Resource)]
struct PremultAlphaPipelines(HashMap<CachedRenderPipelineId, CachedRenderPipelineId>);

impl Plugin for PremultAlphaPlugin {
    fn build(&self, app: &mut App) {
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app
                // Pipeline IDs belong to the device's cache; reset on recovery too.
                .init_gpu_resource::<PremultAlphaPipelines>()
                .add_systems(
                    Render,
                    remap_sprite_pipelines
                        .after(RenderSystems::Queue)
                        .before(RenderSystems::PhaseSort),
                );
        }
    }
}

fn remap_sprite_pipelines(
    draw_functions: Res<'_, DrawFunctions<Transparent2d>>,
    mut phases: ResMut<'_, ViewSortedRenderPhases<Transparent2d>>,
    material_instances: Res<'_, RenderMaterial2dInstances>,
    mut pipeline_cache: ResMut<'_, PipelineCache>,
    mut replacements: ResMut<'_, PremultAlphaPipelines>,
) {
    let draw_sprite = draw_functions.read().get_id::<DrawSprite>();

    for phase in phases.0.values_mut() {
        for item in phase.items.values_mut() {
            // Ordinary Sprites use the mesh/material path in Bevy 0.20.
            // Its draw function is shared with other Material2d types, so filter
            // by the actual material asset type rather than that draw ID.
            let sprite_material = material_instances
                .get(&item.entity.1)
                .is_some_and(|id| id.type_id() == TypeId::of::<SpriteMeshMaterial>());
            if Some(item.draw_function) != draw_sprite && !sprite_material {
                continue;
            }

            let original = item.pipeline;
            let replacement = if let Some(&replacement) = replacements.0.get(&original) {
                replacement
            } else {
                // queue_sprites can create IDs this frame. Descriptor access would
                // panic before process_queue drains them into the readable cache.
                // This also processes pending pipelines earlier than Bevy's
                // usual Render step, using its configured compilation mode.
                if original.id() >= pipeline_cache.pipelines().count() {
                    pipeline_cache.process_queue();
                }
                let mut descriptor = pipeline_cache
                    .get_render_pipeline_descriptor(original)
                    .clone();
                set_premultiplied_blend(&mut descriptor);
                let replacement = pipeline_cache.queue_render_pipeline(descriptor);
                replacements.0.insert(original, replacement);
                // Sorted phase items may persist; never remap our own pipeline.
                replacements.0.insert(replacement, replacement);
                replacement
            };
            item.pipeline = replacement;
        }
    }
}

fn set_premultiplied_blend(descriptor: &mut RenderPipelineDescriptor) {
    if let Some(fragment) = &mut descriptor.fragment {
        for target in fragment.targets.iter_mut().flatten() {
            target.blend = Some(BlendState::PREMULTIPLIED_ALPHA_BLENDING);
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy::render::render_resource::{
        ColorTargetState, ColorWrites, FragmentState, TextureFormat,
    };

    use super::*;

    /// Exercises queueing, pipeline remapping, batching and the shader, not just
    /// descriptor mutation. No Wakfu assets or window are needed.
    #[test]
    #[ignore = "requires a working GPU adapter; run explicitly with --ignored --nocapture"]
    fn headless_premultiplied_pixels() {
        use std::time::{Duration, Instant};

        use bevy::app::PluginsState;
        use bevy::asset::RenderAssetUsages;
        use bevy::camera::{CompositingSpace, RenderTarget};
        use bevy::core_pipeline::tonemapping::Tonemapping;
        use bevy::render::error_handler::RenderErrorHandler;
        use bevy::render::gpu_readback::{Readback, ReadbackComplete};
        use bevy::render::pipelined_rendering::PipelinedRenderingPlugin;
        use bevy::render::render_resource::{Extent3d, TextureDimension, TextureUsages};
        use bevy::window::{ExitCondition, WindowPlugin};
        use bevy::winit::WinitPlugin;

        #[derive(Default, Resource)]
        struct Pixels(Vec<u8>);

        let mut app = App::new();
        app.add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: None,
                    exit_condition: ExitCondition::DontExit,
                    ..default()
                })
                .disable::<WinitPlugin>()
                .disable::<PipelinedRenderingPlugin>()
                .disable::<bevy::log::LogPlugin>(),
        )
        .add_plugins(PremultAlphaPlugin)
        .init_resource::<Pixels>()
        .insert_resource(RenderErrorHandler(|error, _, _| {
            panic!("GPU pixel test render error: {error:?}");
        }));

        let deadline = Instant::now() + Duration::from_secs(30);
        while app.plugins_state() != PluginsState::Ready {
            assert!(Instant::now() < deadline, "GPU initialization timed out");
            bevy::tasks::tick_global_task_pools_on_main_thread();
            std::thread::sleep(Duration::from_millis(10));
        }
        app.finish();
        app.cleanup();

        let (target, texture) = {
            let mut images = app.world_mut().resource_mut::<Assets<Image>>();
            // A 64-pixel RGBA row is exactly wgpu's 256-byte copy alignment.
            let mut target = Image::new_target_texture(64, 16, TextureFormat::Rgba8Unorm, None);
            target.texture_descriptor.usage |= TextureUsages::COPY_SRC;
            let target = images.add(target);
            let texture = images.add(Image::new_fill(
                Extent3d {
                    width: 2,
                    height: 2,
                    depth_or_array_layers: 1,
                },
                TextureDimension::D2,
                &[64, 32, 16, 128],
                TextureFormat::Rgba8Unorm,
                RenderAssetUsages::default(),
            ));
            (target, texture)
        };
        app.world_mut()
            .spawn(Readback::texture(target.clone()))
            .observe(
                |event: On<'_, '_, ReadbackComplete>, mut pixels: ResMut<'_, Pixels>| {
                    pixels.0.clone_from(&event.data);
                },
            );
        app.world_mut().spawn((
            Camera2d,
            Camera {
                clear_color: ClearColorConfig::Custom(Color::linear_rgba(0.125, 0.25, 0.5, 1.0)),
                ..default()
            },
            RenderTarget::Image(target.into()),
            CompositingSpace::Linear,
            Tonemapping::None,
            Msaa::Off,
        ));
        // Two isolated samples, plus an overlap, test texture alpha, tint alpha
        // and painter's order/batching with a shared texture.
        for (x, z, tint) in [
            (-4.0, 0.0, Color::WHITE),
            (4.0, 1.0, Color::linear_rgba(0.5, 0.5, 0.5, 0.5)),
        ] {
            app.world_mut().spawn((
                Sprite {
                    image: texture.clone(),
                    custom_size: Some(Vec2::splat(16.0)),
                    color: tint,
                    ..default()
                },
                Transform::from_xyz(x, 0.0, z),
            ));
        }

        // An unrelated material uses the same transparent mesh draw command,
        // but must retain straight-alpha blending.
        let mesh = app
            .world_mut()
            .resource_mut::<Assets<Mesh>>()
            .add(Rectangle::from_size(Vec2::splat(8.0)));
        let material = app
            .world_mut()
            .resource_mut::<Assets<bevy::sprite_render::ColorMaterial>>()
            .add(bevy::sprite_render::ColorMaterial {
                texture: Some(texture),
                ..default()
            });
        app.world_mut().spawn((
            Mesh2d(mesh),
            MeshMaterial2d(material),
            Transform::from_xyz(-24.0, 0.0, 0.0),
        ));

        // Linear target: source + destination * (1 - source alpha).
        // Right sprite's tint is already premultiplied, as in render.rs.
        let background = [0.125, 0.25, 0.5];
        let source = [64.0 / 255.0, 32.0 / 255.0, 16.0 / 255.0];
        let over = |dst: [f32; 3], tint: f32| {
            std::array::from_fn(|i| source[i] * tint + dst[i] * (1.0 - 128.0 / 255.0 * tint))
        };
        let straight = std::array::from_fn(|i| {
            source[i] * (128.0 / 255.0) + background[i] * (1.0 - 128.0 / 255.0)
        });
        let expected = [
            (8, straight),
            (18, background),
            (22, over(background, 1.0)),
            (32, over(over(background, 1.0), 0.5)),
            (42, over(background, 0.5)),
        ];
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut matching_frames = 0;
        let mut stable_pipeline_count = None;
        for _ in 0..1000 {
            assert!(Instant::now() < deadline, "GPU readback timed out");
            app.update();
            let pixels = &app.world().resource::<Pixels>().0;
            let matches = expected.iter().all(|(x, rgb)| {
                let offset = (8 * 64 + x) * 4;
                pixels.get(offset..offset + 4).is_some_and(|pixel| {
                    pixel[3] == 255
                        && (0..3).all(|i| (f32::from(pixel[i]) - rgb[i] * 255.0).abs() <= 2.0)
                })
            });
            if matches {
                let render_world = app.sub_app(RenderApp).world();
                let replacements = &render_world.resource::<PremultAlphaPipelines>().0;
                assert!(!replacements.is_empty(), "no sprite pipeline was remapped");
                if let Some(count) = stable_pipeline_count {
                    assert_eq!(
                        replacements.len(),
                        count,
                        "replacement pipelines grew across frames"
                    );
                } else {
                    stable_pipeline_count = Some(replacements.len());
                }
                // Retained mesh phase items must keep their replacement without
                // spawning new pipelines on subsequent frames.
                matching_frames += 1;
                if matching_frames == 20 {
                    return;
                }
            } else {
                matching_frames = 0;
                stable_pipeline_count = None;
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        let pixels = &app.world().resource::<Pixels>().0;
        let samples: Vec<_> = expected
            .iter()
            .map(|(x, _)| {
                let offset = (8 * 64 + x) * 4;
                (*x, pixels.get(offset..offset + 4))
            })
            .collect();
        panic!("premultiplied pixels never stabilized: {samples:?}; expected {expected:?}");
    }

    #[test]
    fn changes_only_present_targets_blend() {
        let target = ColorTargetState {
            format: TextureFormat::Rgba8Unorm,
            blend: Some(BlendState::ALPHA_BLENDING),
            write_mask: ColorWrites::RED,
        };
        let mut descriptor = RenderPipelineDescriptor {
            fragment: Some(FragmentState {
                targets: vec![Some(target.clone()), None, Some(target)],
                ..default()
            }),
            ..default()
        };
        let original = descriptor.clone();
        set_premultiplied_blend(&mut descriptor);
        let fragment = descriptor.fragment.as_ref().unwrap();
        assert!(fragment.targets[1].is_none());
        for target in fragment.targets.iter().flatten() {
            assert_eq!(target.blend, Some(BlendState::PREMULTIPLIED_ALPHA_BLENDING));
            assert_eq!(target.format, TextureFormat::Rgba8Unorm);
            assert_eq!(target.write_mask, ColorWrites::RED);
        }
        // Restore just the blend to verify shaders, definitions and all other
        // descriptor settings remain unchanged.
        for target in descriptor
            .fragment
            .as_mut()
            .unwrap()
            .targets
            .iter_mut()
            .flatten()
        {
            target.blend = Some(BlendState::ALPHA_BLENDING);
        }
        assert_eq!(descriptor, original);
    }

    #[test]
    fn leaves_descriptor_without_fragment_unchanged() {
        let mut descriptor = RenderPipelineDescriptor::default();
        let original = descriptor.clone();
        set_premultiplied_blend(&mut descriptor);
        assert_eq!(descriptor, original);
    }
}
