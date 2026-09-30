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
