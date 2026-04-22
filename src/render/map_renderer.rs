use crate::{
    map::Map,
    render::{camera::Camera, game_sprites::SpriteSet, texture::Texture},
};

use bytemuck::{Pod, Zeroable};
use encase::ShaderType;
use glam::Mat4;
use wgpu::util::DeviceExt;

#[derive(Debug, ShaderType)]
struct UniformState {
    mvp: Mat4,
}

impl UniformState {
    fn as_wgsl_bytes(&self) -> encase::internal::Result<Vec<u8>> {
        let mut buffer = encase::UniformBuffer::new(Vec::new());
        buffer.write(self)?;
        encase::internal::Result::Ok(buffer.into_inner())
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    pos: [f32; 2],
}

fn vertex(pos: [f32; 2]) -> Vertex {
    Vertex { pos }
}

fn create_vertices() -> Vec<Vertex> {
    const START: f32 = -1.0f32;
    const END: f32 = 1025.0f32;

    let vertex_data = [
        vertex([START, START]),
        vertex([START, END]),
        vertex([END, START]),
        vertex([END, START]),
        vertex([START, END]),
        vertex([END, END]),
    ];

    vertex_data.to_vec()
}

pub struct MapTileset {
    pub image: image::RgbaImage,
}

impl MapTileset {
    pub fn new(data: &[u8]) -> Self {
        const DEFAULT_TILESET_DATA: &[u8] = include_bytes!("tiles.bm2");

        let img = match image::load_from_memory(data) {
            Ok(img) => img,
            Err(_) => image::load_from_memory(DEFAULT_TILESET_DATA)
                .expect("default tilesetdata must be a valid image"),
        };

        Self {
            image: img.into_rgba8(),
        }
    }
}

pub struct MapRenderer {
    pipeline: wgpu::RenderPipeline,
    pipeline_flyunder: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,

    uniform_state: UniformState,
    uniform_buffer: wgpu::Buffer,

    vertex_buffer: wgpu::Buffer,

    pub tileset_texture: Texture,
    tiledata_texture: Texture,

    pub door_spriteset: SpriteSet,

    map_loaded: bool,
}

impl MapRenderer {
    pub fn new(
        device: &wgpu::Device,
        format: &wgpu::TextureFormat,
        depth_texture: &Texture,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("shaders/map.wgsl"));
        let shader_flyunder =
            device.create_shader_module(wgpu::include_wgsl!("shaders/map_flyunder.wgsl"));

        let vertex_size = size_of::<Vertex>();

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("uniform buffer"),
            size: size_of::<UniformState>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform_state = UniformState {
            mvp: Mat4::IDENTITY,
        };

        let vertex_data = create_vertices();

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vertex buffer"),
            contents: bytemuck::cast_slice(&vertex_data),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let tileset_view_format = if format.is_srgb() {
            wgpu::TextureFormat::Rgba8UnormSrgb
        } else {
            wgpu::TextureFormat::Rgba8Unorm
        };

        let tiledata_texture = Texture::new_2d(device, 1024, 1024, wgpu::TextureFormat::R8Uint);
        let tileset_texture = Texture::new_2d_array(device, 16, 16, 190, tileset_view_format);
        let tileset_sampler = Texture::create_sampler(device);

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        sample_type: wgpu::TextureSampleType::Uint,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
            ],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&tileset_texture.view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&tileset_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&tiledata_texture.view),
                },
            ],
        });

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

        let pipeline_flyunder = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader_flyunder,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &vertex_buffers,
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader_flyunder,
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

        MapRenderer {
            pipeline,
            pipeline_flyunder,
            bind_group,
            uniform_state,
            uniform_buffer,
            vertex_buffer,

            tileset_texture,
            tiledata_texture,

            door_spriteset: SpriteSet::empty(),

            map_loaded: false,
        }
    }

    pub fn set_map(&mut self, map: &Map, tileset: &MapTileset, queue: &wgpu::Queue) {
        // We need to write into a new data slot so the rows align by 256 bytes.
        let mut custom_data = [0; 64 * 16 * 4];

        let tileset_texels = tileset.image.as_raw().as_slice();

        self.map_loaded = true;

        for tile_id in 0..190 {
            let tile_x = (tile_id % 19) * 16;
            let tile_y = (tile_id / 19) * 16;

            for y in 0..16 {
                let write_index_start: usize = (y * 64 * 4) as usize;
                let write_index_end: usize = write_index_start + 16 * 4;

                let read_index_start: usize = ((tile_y + y) * 304 * 4 + (tile_x * 4)) as usize;
                let read_index_end: usize = read_index_start + (16 * 4) as usize;

                custom_data[write_index_start..write_index_end]
                    .copy_from_slice(&tileset_texels[read_index_start..read_index_end]);
            }

            let mut texture_info = self.tileset_texture.texture.as_image_copy();
            texture_info.origin.z = tile_id;

            queue.write_texture(
                texture_info,
                &custom_data,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(256),
                    rows_per_image: Some(16),
                },
                wgpu::Extent3d {
                    width: 16,
                    height: 16,
                    depth_or_array_layers: 1,
                },
            );
        }

        let mut tiledata = Vec::with_capacity(1024 * 1024);
        for y in 0..1024 {
            for x in 0..1024 {
                tiledata.push(map.tiles[y * 1024 + x]);
            }
        }

        queue.write_texture(
            self.tiledata_texture.texture.as_image_copy(),
            &tiledata,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(1024),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width: 1024,
                height: 1024,
                depth_or_array_layers: 1,
            },
        );
    }

    pub fn render(
        &mut self,
        renderpass: &mut wgpu::RenderPass,
        camera: &Camera,
        queue: &wgpu::Queue,
    ) {
        if !self.map_loaded {
            return;
        }

        self.uniform_state.mvp = camera.projection() * camera.view();

        queue.write_buffer(
            &self.uniform_buffer,
            0,
            &self
                .uniform_state
                .as_wgsl_bytes()
                .expect("uniform buffer should transform itself into wgsl bytes"),
        );

        renderpass.set_pipeline(&self.pipeline);
        renderpass.set_bind_group(0, Some(&self.bind_group), &[]);
        renderpass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        renderpass.draw(0..6, 0..1);

        renderpass.set_pipeline(&self.pipeline_flyunder);
        renderpass.set_bind_group(0, Some(&self.bind_group), &[]);
        renderpass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        renderpass.draw(0..6, 0..1);
    }
}
