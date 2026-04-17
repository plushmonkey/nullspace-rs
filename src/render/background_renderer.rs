use crate::render::{camera::Camera, texture::Texture};

use bytemuck::{Pod, Zeroable};
use encase::ShaderType;
use glam::Mat4;
use wgpu::util::DeviceExt;

#[derive(Debug, ShaderType)]
struct UniformState {
    mvp: Mat4,
    camera_position: glam::Vec2,
    seed: u32,
    color: f32,
    speed: f32,
}

impl UniformState {
    fn as_wgsl_bytes(&self) -> encase::internal::Result<Vec<u8>> {
        let mut buffer = encase::UniformBuffer::new(Vec::new());
        buffer.write(self)?;
        encase::internal::Result::Ok(buffer.into_inner())
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug)]
struct Vertex {
    position: [f32; 2],
}

fn create_vertices() -> Vec<Vertex> {
    const START: f32 = -1024.0f32 * 10.0f32;
    const END: f32 = 1024.0f32 * 10.0f32;

    let vertex_data = [
        Vertex {
            position: [START, START],
        },
        Vertex {
            position: [START, END],
        },
        Vertex {
            position: [END, START],
        },
        Vertex {
            position: [END, START],
        },
        Vertex {
            position: [START, END],
        },
        Vertex {
            position: [END, END],
        },
    ];

    vertex_data.to_vec()
}

pub struct BackgroundRenderer {
    pipeline: wgpu::RenderPipeline,

    uniform_states: [UniformState; 2],
    uniform_buffers: [wgpu::Buffer; 2],
    bind_groups: [wgpu::BindGroup; 2],

    vertex_buffer: wgpu::Buffer,
}

impl BackgroundRenderer {
    pub fn new(
        device: &wgpu::Device,
        format: &wgpu::TextureFormat,
        depth_texture: &Texture,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("shaders/background.wgsl"));

        let seed = crate::clock::GameTick::now(0).value();
        let mut rng = crate::rng::VieRng::new(seed as i32);

        let seeds = [rng.next(), rng.next()];
        let speeds = [0.5f32, 0.75f32];
        let colors = if format.is_srgb() {
            [121.0f32 / 255.0f32, 28.0f32 / 255.0f32]
        } else {
            [184.0f32 / 255.0f32, 96.0f32 / 255.0f32]
        };

        let uniform_states = [
            UniformState {
                mvp: Mat4::IDENTITY,
                camera_position: glam::Vec2::new(0.0f32, 0.0f32),
                seed: seeds[0],
                color: colors[0],
                speed: speeds[0],
            },
            UniformState {
                mvp: Mat4::IDENTITY,
                camera_position: glam::Vec2::new(0.0f32, 0.0f32),
                seed: seeds[1],
                color: colors[1],
                speed: speeds[1],
            },
        ];

        let uniform_buffers = [
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("uniform buffer"),
                size: size_of::<UniformState>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("uniform buffer"),
                size: size_of::<UniformState>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
        ];

        let vertex_size = size_of::<Vertex>();
        let vertex_data = create_vertices();

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vertex buffer"),
            contents: bytemuck::cast_slice(&vertex_data),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let bind_groups = [
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffers[0].as_entire_binding(),
                }],
            }),
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffers[1].as_entire_binding(),
                }],
            }),
        ];

        let vertex_buffers = [wgpu::VertexBufferLayout {
            array_stride: vertex_size as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 0,
                // Bind position to location(0)
                shader_location: 0,
            }],
        }];

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pipeline layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &vertex_buffers,
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some((*format).into())],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: depth_texture.texture.format(),
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        Self {
            pipeline,

            uniform_states,
            uniform_buffers,
            bind_groups,

            vertex_buffer,
        }
    }

    pub fn render(
        &mut self,
        renderpass: &mut wgpu::RenderPass,
        camera: &Camera,
        queue: &wgpu::Queue,
    ) {
        renderpass.set_pipeline(&self.pipeline);

        let mvp = camera.projection() * camera.view();

        renderpass.set_vertex_buffer(0, self.vertex_buffer.slice(..));

        for i in 0..2 {
            self.uniform_states[i].mvp = mvp;
            self.uniform_states[i].camera_position = camera.position;

            queue.write_buffer(
                &self.uniform_buffers[i],
                0,
                &self.uniform_states[i]
                    .as_wgsl_bytes()
                    .expect("uniform buffer should transform itself into wgsl bytes"),
            );

            renderpass.set_bind_group(0, Some(&self.bind_groups[i]), &[]);
            renderpass.draw(0..6, 0..1);
        }
    }
}
