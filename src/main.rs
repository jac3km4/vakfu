#[macro_use]
extern crate vulkano;
#[macro_use]
extern crate vulkano_shader_derive;
extern crate time;
extern crate vulkano_win;
extern crate winit;
extern crate zip;
#[macro_use]
extern crate scan_fmt;

use std::env;
use std::fs::File;
use std::sync::Arc;
use vulkano::command_buffer::{AutoCommandBufferBuilder, DynamicState};
use vulkano::framebuffer::Framebuffer;
use vulkano::pipeline::blend::{AttachmentBlend, BlendFactor, BlendOp};
use vulkano::pipeline::viewport::Viewport;
use vulkano::pipeline::GraphicsPipeline;
use vulkano::sampler::Sampler;
use vulkano::sync::{now, GpuFuture};
use vulkano_win::VkSurfaceBuild;
use wfu::gfx::camera;
use wfu::gfx::map::Map;
use wfu::gfx::world::library::ElementLibrary;
use wfu::io::tgam::TgamLoader;
use wfu::util::timer::Timer;
use wfu::vk::vertex::Vertex;
use wfu::vk::{fragment_shader, vertex_shader};
use wfu::vk::persistent::PersistentDescriptorSet;
use vulkano::buffer::ImmutableBuffer;

pub mod wfu;


const BLENDING: AttachmentBlend = AttachmentBlend {
    enabled: true,
    color_op: BlendOp::Add,
    color_source: BlendFactor::One,
    color_destination: BlendFactor::OneMinusSrcAlpha,
    alpha_op: BlendOp::Add,
    alpha_source: BlendFactor::One,
    alpha_destination: BlendFactor::OneMinusSrcAlpha,
    mask_red: true,
    mask_green: true,
    mask_blue: true,
    mask_alpha: true,
};

fn main() {
    let args: Vec<String> = env::args().collect();

    let path = args.get(1).expect("Expecting path as 1st param");
    let map_id = args.get(2).expect("Expecting map id as 2nd param");

    let mut textures = TgamLoader::new(File::open(format!("{}\\{}", path, "gfx.jar")).unwrap());

    let element_library =
        ElementLibrary::load(File::open(format!("{}\\{}", path, "data.jar")).unwrap());

    // vulkan startup below...

    let extensions = vulkano_win::required_extensions();

    let instance = vulkano::instance::Instance::new(None, &extensions, None)
        .expect("failed to create instance");

    let physical = vulkano::instance::PhysicalDevice::enumerate(&instance)
        .next()
        .expect("no device available");
    println!(
        "Using device: {} (type: {:?})",
        physical.name(),
        physical.ty()
    );

    let mut events_loop = winit::EventsLoop::new();
    let surface = winit::WindowBuilder::new()
        .build_vk_surface(&events_loop, instance.clone())
        .unwrap();

    let mut dimensions;

    let queue_family = physical
        .queue_families()
        .find(|&q| q.supports_graphics() && surface.is_supported(q).unwrap_or(false))
        .expect("couldn't find a graphical queue family");

    let device_ext = vulkano::device::DeviceExtensions {
        khr_swapchain: true,
        ..vulkano::device::DeviceExtensions::none()
    };
    let (device, mut queues) = vulkano::device::Device::new(
        physical,
        physical.supported_features(),
        &device_ext,
        [(queue_family, 0.5)].iter().cloned(),
    ).expect("failed to create device");
    let queue = queues.next().unwrap();

    let (mut map, imgs) = Map::load(
        queue.clone(),
        File::open(format!("{}\\gfx\\{}.jar", path, map_id)).unwrap(),
        element_library,
        &mut textures,
    );

    let (mut swapchain, mut images) = {
        let caps = surface
            .capabilities(physical)
            .expect("failed to get surface capabilities");

        dimensions = caps.current_extent.unwrap_or([1024, 768]);
        let usage = caps.supported_usage_flags;
        let alpha = caps.supported_composite_alpha.iter().next().unwrap();
        let format = caps.supported_formats[0].0;

        vulkano::swapchain::Swapchain::new(
            device.clone(),
            surface.clone(),
            caps.min_image_count,
            format,
            dimensions,
            1,
            usage,
            &queue,
            vulkano::swapchain::SurfaceTransform::Identity,
            alpha,
            vulkano::swapchain::PresentMode::Fifo,
            true,
            None,
        ).expect("failed to create swapchain")
    };

    let vs = vertex_shader::Shader::load(device.clone()).expect("failed to create shader module");
    let fs = fragment_shader::Shader::load(device.clone()).expect("failed to create shader module");

    let renderpass = Arc::new(
        single_pass_renderpass!(device.clone(),
            attachments: {
                color: {
                    load: Clear,
                    store: Store,
                    format: swapchain.format(),
                    samples: 1,
                }
            },
            pass: {
                color: [color],
                depth_stencil: {}
            }
        ).unwrap(),
    );

    let sampler = Sampler::new(
        device.clone(),
        vulkano::sampler::Filter::Linear,
        vulkano::sampler::Filter::Linear,
        vulkano::sampler::MipmapMode::Nearest,
        vulkano::sampler::SamplerAddressMode::Repeat,
        vulkano::sampler::SamplerAddressMode::Repeat,
        vulkano::sampler::SamplerAddressMode::Repeat,
        0.0,
        1.0,
        0.0,
        0.0,
    ).unwrap();

    let pipeline = Arc::new(
        GraphicsPipeline::start()
            .vertex_input_single_buffer::<Vertex>()
            .vertex_shader(vs.main_entry_point(), ())
            .triangle_list()
            .viewports_dynamic_scissors_irrelevant(1)
            .fragment_shader(fs.main_entry_point(), ())
            .blend_collective(BLENDING)
            .cull_mode_back()
            .depth_stencil_disabled()
            .render_pass(vulkano::framebuffer::Subpass::from(renderpass.clone(), 0).unwrap())
            .build(device.clone())
            .unwrap(),
    );

    let desc =
        Arc::new(PersistentDescriptorSet::start(pipeline.clone(), 0)
            .add_sampled_images(imgs, sampler)
            .unwrap()
            .build()
            .unwrap());

    let mut framebuffers: Option<Vec<Arc<Framebuffer<_, _>>>> = None;

    let mut recreate_swapchain = false;

    let mut previous_frame_end = Box::new(now(device.clone())) as Box<GpuFuture>;

    let mut dynamic_state = DynamicState {
        line_width: None,
        viewports: Some(vec![Viewport {
            origin: [0.0, 0.0],
            dimensions: [dimensions[0] as f32, dimensions[1] as f32],
            depth_range: 0.0..1.0,
        }]),
        scissors: None,
    };

    let mut timer = Timer::new();

    let mut camera = camera::with_ease_in_out_quad();

    let all_vertices =
        map.get_sprites()
            .iter()
            .flat_map(|sprite| &sprite.vertex)
            .cloned()
            .collect::<Vec<_>>();

    loop {
        let delta = timer.tick();
        camera.update(delta);
        previous_frame_end.cleanup_finished();
        if recreate_swapchain {
            dimensions = surface
                .capabilities(physical)
                .expect("failed to get surface capabilities")
                .current_extent
                .unwrap_or([1280, 1024]);

            let (new_swapchain, new_images) = match swapchain.recreate_with_dimension(dimensions) {
                Ok(r) => r,
                Err(vulkano::swapchain::SwapchainCreationError::UnsupportedDimensions) => {
                    continue;
                }
                Err(err) => panic!("{:?}", err),
            };

            swapchain = new_swapchain;
            images = new_images;

            framebuffers = None;

            dynamic_state.viewports = Some(vec![Viewport {
                origin: [0.0, 0.0],
                dimensions: [dimensions[0] as f32, dimensions[1] as f32],
                depth_range: 0.0..1.0,
            }]);

            recreate_swapchain = false;
        }

        if framebuffers.is_none() {
            framebuffers = Some(
                images
                    .iter()
                    .map(|image| {
                        Arc::new(
                            Framebuffer::start(renderpass.clone())
                                .add(image.clone())
                                .unwrap()
                                .build()
                                .unwrap(),
                        )
                    }).collect::<Vec<_>>(),
            );
        }

        let (image_num, future) =
            match vulkano::swapchain::acquire_next_image(swapchain.clone(), None) {
                Ok(r) => r,
                Err(vulkano::swapchain::AcquireError::OutOfDate) => {
                    recreate_swapchain = true;
                    continue;
                }
                Err(err) => panic!("{:?}", err),
            };

        let matrix = vertex_shader::ty::Matrix {
            value: camera.get_matrix(dimensions[0], dimensions[1]).into(),
        };


        let (vertex_buffer, cmd) = ImmutableBuffer::<[Vertex]>::from_iter(
            all_vertices.iter().cloned(),
            vulkano::buffer::BufferUsage::all(),
            queue.clone(),
        ).expect("failed to create buffer");


        let commands =
            AutoCommandBufferBuilder::primary_one_time_submit(device.clone(), queue.family())
                .unwrap()
                .begin_render_pass(
                    framebuffers.as_ref().unwrap()[image_num].clone(),
                    false,
                    vec![[0.0, 0.0, 0.0, 1.0].into()],
                ).unwrap()
                .draw(
                    pipeline.clone(),
                    &dynamic_state,
                    vertex_buffer.clone(),
                    desc.clone(),
                    matrix,
                ).unwrap().end_render_pass()
                .unwrap()
                .build()
                .unwrap();

        let future = previous_frame_end
            .join(future)
            .join(cmd)
            .then_execute(queue.clone(), commands)
            .unwrap()
            .then_swapchain_present(queue.clone(), swapchain.clone(), image_num)
            .then_signal_fence_and_flush();

        match future {
            Ok(future) => {
                previous_frame_end = Box::new(future) as Box<_>;
            }
            Err(vulkano::sync::FlushError::OutOfDate) => {
                recreate_swapchain = true;
                previous_frame_end = Box::new(now(device.clone())) as Box<_>;
            }
            Err(e) => {
                println!("{:?}", e);
                previous_frame_end = Box::new(now(device.clone())) as Box<_>;
            }
        }

        let mut done = false;
        events_loop.poll_events(|ev| match ev {
            winit::Event::WindowEvent {
                event: winit::WindowEvent::CloseRequested,
                ..
            } => done = true,
            winit::Event::DeviceEvent { event, device_id } => {
                camera.handle(event);
            }
            _ => (),
        });
        if done {
            return;
        }
    }
}
