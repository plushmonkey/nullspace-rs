use std::usize;

use crate::render::{camera::Camera, layer::Layer, texture::Texture};

use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug)]
struct Vertex {
    position: [f32; 3],
    uv: [f32; 2],
}

#[derive(Copy, Clone, Debug)]
pub struct SheetIndex(pub u32);

pub struct SpriteSheet {
    bind_group: wgpu::BindGroup,
    pub width: u32,
    pub height: u32,

    // Each sheet has an index so we can reuse bind_groups during render by comparing the index.
    sheet_index: SheetIndex,
}

impl SpriteSheet {
    pub fn create_renderable(
        &self,
        sheet_start_x: u32,
        sheet_start_y: u32,
        width: u32,
        height: u32,
    ) -> SpriteRenderable {
        let sheet_end_x = sheet_start_x + width;
        let sheet_end_y = sheet_start_y + height;

        let uv_start_x = (sheet_start_x as f32) / (self.width as f32);
        let uv_start_y = (sheet_start_y as f32) / (self.height as f32);
        let uv_end_x = (sheet_end_x as f32) / (self.width as f32);
        let uv_end_y = (sheet_end_y as f32) / (self.height as f32);

        SpriteRenderable {
            uv_start: [uv_start_x, uv_start_y],
            uv_size: [uv_end_x - uv_start_x, uv_end_y - uv_start_y],
            size: [width, height],
            sheet_index: self.sheet_index,
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub struct SpriteRenderable {
    pub uv_start: [f32; 2],
    pub uv_size: [f32; 2],
    pub size: [u32; 2],
    pub sheet_index: SheetIndex,
}

pub struct SpriteRenderer {
    pipeline: wgpu::RenderPipeline,

    vertex_buffer: wgpu::Buffer,

    bind_group_layout: wgpu::BindGroupLayout,

    sampler: wgpu::Sampler,
    linear_sampler: wgpu::Sampler,

    sprite_sheets: Vec<SpriteSheet>,

    // Each sprite sheet has their own push buffer that is stored here.
    // Rendering will go through each one and draw them in one call, then clear it for next frame.
    push_buffers: Vec<Vec<Vertex>>,
}

impl SpriteRenderer {
    // Push size is how many sprites can be rendered at once. The vertex buffer will be allocated to handle this many sprites.
    pub fn new(
        device: &wgpu::Device,
        format: &wgpu::TextureFormat,
        depth_texture: &Texture,
        push_size: usize,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("shaders/sprite.wgsl"));

        let vertex_size = size_of::<Vertex>();
        // There's 6 vertices that make up the two triangles of the sprite.
        let vertex_buffer_size = vertex_size * push_size * 6;

        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sprite vertex buffer"),
            mapped_at_creation: false,
            size: vertex_buffer_size as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });

        let sampler = Texture::create_sampler(device);
        let linear_sampler = Texture::create_linear_sampler(device);

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let vertex_buffers = [wgpu::VertexBufferLayout {
            array_stride: vertex_size as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x3,
                    offset: 0,
                    // Bind position to location(0)
                    shader_location: 0,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 12,
                    // Bind uv to location(1)
                    shader_location: 1,
                },
            ],
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
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
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
            vertex_buffer,

            bind_group_layout,
            sampler,
            linear_sampler,
            sprite_sheets: vec![],
            push_buffers: vec![],
        }
    }

    pub fn draw_with_transform(
        &mut self,
        transform: glam::Mat4,
        scale: f32,
        renderable: &SpriteRenderable,
        x_pixels: f32,
        y_pixels: f32,
        z: f32,
    ) {
        let x = x_pixels * scale;
        let y = y_pixels * scale;

        let width = (renderable.size[0] as f32) * scale;
        let height = (renderable.size[1] as f32) * scale;

        let position = transform * glam::Vec4::new(x, y, z, 1.0f32);
        let position_br = transform * glam::Vec4::new(x + width, y + height, z, 1.0f32);

        let position = glam::Vec3::new(position.x, position.y, position.z);
        let position_br = glam::Vec3::new(position_br.x, position_br.y, position_br.z);

        let uv0 = [renderable.uv_start[0], renderable.uv_start[1]];
        let uv1 = [
            renderable.uv_start[0] + renderable.uv_size[0],
            renderable.uv_start[1],
        ];
        let uv2 = [
            renderable.uv_start[0],
            renderable.uv_start[1] + renderable.uv_size[1],
        ];
        let uv3 = [
            renderable.uv_start[0] + renderable.uv_size[0],
            renderable.uv_start[1] + renderable.uv_size[1],
        ];

        let buffer = &mut self.push_buffers[renderable.sheet_index.0 as usize];

        // Left ccw triangle
        {
            buffer.push(Vertex {
                position: [position.x, position.y, position.z],
                uv: uv0,
            });

            buffer.push(Vertex {
                position: [position.x, position_br.y, position.z],
                uv: uv2,
            });

            buffer.push(Vertex {
                position: [position_br.x, position.y, position.z],
                uv: uv1,
            });
        }

        // Right ccw triangle
        {
            buffer.push(Vertex {
                position: [position_br.x, position.y, position.z],
                uv: uv1,
            });

            buffer.push(Vertex {
                position: [position.x, position_br.y, position.z],
                uv: uv2,
            });

            buffer.push(Vertex {
                position: [position_br.x, position_br.y, position.z],
                uv: uv3,
            });
        }
    }

    pub fn draw_centered(
        &mut self,
        camera: &Camera,
        renderable: &SpriteRenderable,
        x_pixels: i32,
        y_pixels: i32,
        layer: Layer,
    ) {
        let mvp = camera.projection() * camera.view();

        let (width, height) = (renderable.size[0] as f32, renderable.size[1] as f32);

        let x_pixels = x_pixels as f32 - width / 2.0f32;
        let y_pixels = y_pixels as f32 - height / 2.0f32;

        self.draw_with_transform(mvp, camera.scale, renderable, x_pixels, y_pixels, layer.z());
    }

    pub fn draw_centered_with_z(
        &mut self,
        camera: &Camera,
        renderable: &SpriteRenderable,
        x_pixels: i32,
        y_pixels: i32,
        z: f32,
    ) {
        let mvp = camera.projection() * camera.view();

        let (width, height) = (renderable.size[0] as f32, renderable.size[1] as f32);

        let x_pixels = x_pixels as f32 - width / 2.0f32;
        let y_pixels = y_pixels as f32 - height / 2.0f32;

        self.draw_with_transform(mvp, camera.scale, renderable, x_pixels, y_pixels, z);
    }

    pub fn draw(
        &mut self,
        camera: &Camera,
        renderable: &SpriteRenderable,
        x_pixels: i32,
        y_pixels: i32,
        layer: Layer,
    ) {
        let mvp = camera.projection() * camera.view();

        self.draw_with_transform(
            mvp,
            camera.scale,
            renderable,
            x_pixels as f32,
            y_pixels as f32,
            layer.z(),
        );
    }

    pub fn draw_with_z(
        &mut self,
        camera: &Camera,
        renderable: &SpriteRenderable,
        x_pixels: i32,
        y_pixels: i32,
        z: f32,
    ) {
        let mvp = camera.projection() * camera.view();

        self.draw_with_transform(
            mvp,
            camera.scale,
            renderable,
            x_pixels as f32,
            y_pixels as f32,
            z,
        );
    }

    pub fn render(&mut self, renderpass: &mut wgpu::RenderPass, queue: &wgpu::Queue) {
        renderpass.set_pipeline(&self.pipeline);

        let mut offset = 0;

        for i in 0..self.sprite_sheets.len() {
            let push_buffer = &mut self.push_buffers[i];
            let sheet = &self.sprite_sheets[i];

            if push_buffer.is_empty() {
                continue;
            }

            let buffer_size = (push_buffer.len() * size_of::<Vertex>()) as u64;

            // We write out to the part of the vertex buffer we're currently using and then render from it.
            // TODO: Might be better to queue these up for every sheet before we start rendering.
            // Clearing the vertex buffer and writing from the start seems to not work with wgpu.
            // It ends up overwriting the texture before it completes.
            // This is something I've used before in opengl, but maybe it requires some extra synchronization in wgpu.
            queue.write_buffer(
                &self.vertex_buffer,
                offset,
                bytemuck::cast_slice(push_buffer),
            );
            queue.submit([]);

            renderpass.set_bind_group(0, Some(&sheet.bind_group), &[]);
            renderpass.set_vertex_buffer(0, self.vertex_buffer.slice(offset..offset + buffer_size));
            renderpass.draw(0..push_buffer.len() as u32, 0..1);

            offset += buffer_size;
            push_buffer.clear();
        }
    }

    pub fn create_sprite_sheet(
        &mut self,
        device: &wgpu::Device,
        texture: &Texture,
        linear_sampler: bool,
    ) -> SheetIndex {
        let sampler = if linear_sampler {
            &self.linear_sampler
        } else {
            &self.sampler
        };

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        });

        let sheet_index = SheetIndex(self.sprite_sheets.len() as u32);

        let sheet = SpriteSheet {
            bind_group,
            sheet_index,
            width: texture.texture.width(),
            height: texture.texture.height(),
        };

        self.sprite_sheets.push(sheet);
        self.push_buffers.push(vec![]);

        sheet_index
    }

    pub fn get_sheet(&mut self, index: SheetIndex) -> Option<&SpriteSheet> {
        self.sprite_sheets.get(index.0 as usize)
    }

    pub fn change_sheet_texture(
        &mut self,
        index: SheetIndex,
        device: &wgpu::Device,
        texture: &Texture,
    ) {
        if index.0 as usize >= self.sprite_sheets.len() {
            return;
        }

        let sheet = &mut self.sprite_sheets[index.0 as usize];

        sheet.width = texture.texture.width();
        sheet.height = texture.texture.height();

        sheet.bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
    }
}
