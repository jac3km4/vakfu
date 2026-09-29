//! Premultiplied-alpha blending for Bevy 0.19.1's built-in sprite renderer.
//!
//! Add `PremultAlphaPlugin` after `DefaultPlugins`. This affects every `DrawSprite`
//! item (including text rendered as sprites), not meshes or UI. Keep TGAM images
//! premultiplied and `Rgba8Unorm`; the built-in shader and tint behavior are unchanged.

use std::collections::HashMap;

use bevy::core_pipeline::core_2d::Transparent2d;
use bevy::prelude::*;
use bevy::render::render_phase::{DrawFunctions, ViewSortedRenderPhases};
use bevy::render::render_resource::{
    BlendState, CachedRenderPipelineId, PipelineCache, RenderPipelineDescriptor,
};
use bevy::render::{GpuResourceAppExt, Render, RenderApp, RenderSystems};
use bevy::sprite_render::DrawSprite;

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
    mut pipeline_cache: ResMut<'_, PipelineCache>,
    mut replacements: ResMut<'_, PremultAlphaPipelines>,
) {
    let Some(draw_sprite) = draw_functions.read().get_id::<DrawSprite>() else {
        return;
    };

    for phase in phases.0.values_mut() {
        for item in phase.items.values_mut() {
            if item.draw_function != draw_sprite {
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

    #[derive(Default, Resource)]
    struct SmokeFrames(usize);

    fn check_smoke_phase(
        draw_functions: Res<'_, DrawFunctions<Transparent2d>>,
        phases: Res<'_, ViewSortedRenderPhases<Transparent2d>>,
        replacements: Res<'_, PremultAlphaPipelines>,
        mut frames: ResMut<'_, SmokeFrames>,
    ) {
        let draw_sprite = draw_functions.read().id::<DrawSprite>();
        let mut count = 0;
        for phase in phases.0.values() {
            for item in phase.items.values() {
                if item.draw_function == draw_sprite {
                    assert_eq!(replacements.0.get(&item.pipeline), Some(&item.pipeline));
                    count += 1;
                }
            }
        }
        if count > 0 {
            assert_eq!(count, 1);
            frames.0 += 1;
        }
    }

    /// Opt-in: requires a working wgpu adapter/driver, but no window or game assets.
    /// Bounds readiness and updates; use a process timeout too for driver hangs.
    #[test]
    #[ignore = "requires a working GPU adapter; run explicitly with --ignored --nocapture"]
    fn headless_gpu_smoke() {
        use std::time::{Duration, Instant};

        use bevy::app::PluginsState;
        use bevy::asset::RenderAssetUsages;
        use bevy::camera::{CompositingSpace, RenderTarget};
        use bevy::core_pipeline::tonemapping::Tonemapping;
        use bevy::render::error_handler::RenderErrorHandler;
        use bevy::render::pipelined_rendering::PipelinedRenderingPlugin;
        use bevy::render::render_resource::{Extent3d, TextureDimension};
        use bevy::window::{ExitCondition, WindowPlugin};
        use bevy::winit::WinitPlugin;

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
        .insert_resource(RenderErrorHandler(|error, _, _| {
            panic!("GPU smoke render error: {error:?}");
        }));

        let deadline = Instant::now() + Duration::from_secs(15);
        while app.plugins_state() != PluginsState::Ready {
            assert!(Instant::now() < deadline, "GPU initialization timed out");
            bevy::tasks::tick_global_task_pools_on_main_thread();
            std::thread::sleep(Duration::from_millis(10));
        }
        app.finish();
        app.cleanup();
        app.sub_app_mut(RenderApp)
            .init_resource::<SmokeFrames>()
            .add_systems(
                Render,
                check_smoke_phase
                    .after(remap_sprite_pipelines)
                    .before(RenderSystems::PhaseSort),
            );

        let (target, texture) = {
            let mut images = app.world_mut().resource_mut::<Assets<Image>>();
            let target = images.add(Image::new_target_texture(
                16,
                16,
                TextureFormat::Rgba8UnormSrgb,
                None,
            ));
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
        app.world_mut().spawn((
            Camera2d,
            RenderTarget::Image(target.into()),
            CompositingSpace::Linear,
            Tonemapping::None,
            Msaa::Off,
        ));
        app.world_mut().spawn(Sprite {
            image: texture,
            custom_size: Some(Vec2::splat(8.0)),
            color: Color::linear_rgba(0.8, 0.6, 0.4, 0.5),
            ..default()
        });

        let deadline = Instant::now() + Duration::from_secs(15);
        for _ in 0..600 {
            assert!(Instant::now() < deadline, "GPU rendering timed out");
            app.update();
            let world = app.sub_app(RenderApp).world();
            let replacements = &world.resource::<PremultAlphaPipelines>().0;
            let cache = world.resource::<PipelineCache>();
            if world.resource::<SmokeFrames>().0 >= 5 {
                // One original and an identity entry for its replacement: no
                // per-frame pipeline growth, even for retained phase items.
                assert_eq!(replacements.len(), 2);
                let (&original, &replacement) = replacements
                    .iter()
                    .find(|(original, replacement)| original != replacement)
                    .expect("sprite pipeline was not remapped");
                if cache.get_render_pipeline(replacement).is_some() {
                    let mut expected = cache.get_render_pipeline_descriptor(original).clone();
                    set_premultiplied_blend(&mut expected);
                    let actual = cache.get_render_pipeline_descriptor(replacement);
                    assert_eq!(actual, &expected);
                    let defs = format!("{:?}", actual.fragment.as_ref().unwrap().shader_defs);
                    assert!(!defs.contains("SRGB_OUTPUT"));
                    assert!(!defs.contains("OKLAB_OUTPUT"));
                    eprintln!("Headless sprite pipeline compiled and remapped across 5+ frames");
                    return;
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("sprite replacement did not compile within 600 updates");
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
