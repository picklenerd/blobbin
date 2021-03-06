use winit::{event::WindowEvent, window::Window};

use crate::graphics::{
    shaders, Camera, CameraController, GraphicsConfig, Uniforms, Vertex, Object,
};

pub struct State {
    config: GraphicsConfig,
    camera: Camera,
    camera_controller: CameraController,
    uniforms: Uniforms,
    gpu: GpuState,
    size: winit::dpi::PhysicalSize<u32>,
    objects: Vec<Object>,
}

struct GpuState {
    surface: wgpu::Surface,
    device: wgpu::Device,
    queue: wgpu::Queue,
    sc_desc: wgpu::SwapChainDescriptor,
    swap_chain: wgpu::SwapChain,
    render_pipeline: wgpu::RenderPipeline,

    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
}

impl State {
    pub async fn new(window: &Window, config: GraphicsConfig) -> anyhow::Result<Self> {
        let size = window.inner_size();

        let surface = wgpu::Surface::create(window);

        let adapter = wgpu::Adapter::request(
            &wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::Default,
                compatible_surface: Some(&surface),
            },
            wgpu::BackendBit::PRIMARY, // Vulkan + Metal + DX12 + Browser WebGPU
        )
        .await
        .unwrap();

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                extensions: wgpu::Extensions {
                    anisotropic_filtering: false,
                },
                limits: Default::default(),
            })
            .await;

        let sc_desc = wgpu::SwapChainDescriptor {
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
            format: wgpu::TextureFormat::Bgra8UnormSrgb,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Fifo,
        };
        let swap_chain = device.create_swap_chain(&surface, &sc_desc);

        let camera = Camera {
            eye: (0.0, 1.0, 50.0).into(),
            up: cgmath::Vector3::unit_y(),
            aspect: sc_desc.width as f32 / sc_desc.height as f32,
            fovy: 45.0,
            znear: 0.1,
            zfar: 100.0,
        };

        let camera_controller = CameraController::new(0.2);

        let mut uniforms = Uniforms::new();
        uniforms.update_view_proj(&camera);

        let instance_buffer = device.create_buffer_with_data(&[0, 1, 2],  wgpu::BufferUsage::STORAGE_READ);

        let uniform_buffer = device.create_buffer_with_data(
            bytemuck::cast_slice(&[uniforms]),
            wgpu::BufferUsage::UNIFORM | wgpu::BufferUsage::COPY_DST,
        );

        let uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                bindings: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStage::VERTEX,
                        ty: wgpu::BindingType::UniformBuffer { dynamic: false },
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStage::VERTEX,
                        ty: wgpu::BindingType::StorageBuffer {
                            // We don't plan on changing the size of this buffer
                            dynamic: false,
                            // The shader is not allowed to modify it's contents
                            readonly: true,
                        },
                    },
                ],
                label: Some("uniform_bind_group_layout"),
            });

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &uniform_bind_group_layout,
            bindings: &[
                wgpu::Binding {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer {
                        buffer: &uniform_buffer,
                        range: 0..std::mem::size_of_val(&uniforms) as wgpu::BufferAddress,
                    },
                },
                wgpu::Binding {
                    binding: 1,
                    resource: wgpu::BindingResource::Buffer {
                        buffer: &instance_buffer,
                        range: 0..1 as wgpu::BufferAddress,
                    },
                },
            ],
            label: Some("uniform_bind_group"),
        });

        let mut compiler = shaders::ShaderCompiler::new()?;
        let vs_module = shaders::basic::vertex_module(&device, &mut compiler)?;
        let fs_module = shaders::basic::fragment_module(&device, &mut compiler)?;

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                bind_group_layouts: &[&uniform_bind_group_layout],
            });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            layout: &render_pipeline_layout,
            vertex_stage: wgpu::ProgrammableStageDescriptor {
                module: &vs_module,
                entry_point: "main",
            },
            fragment_stage: Some(wgpu::ProgrammableStageDescriptor {
                module: &fs_module,
                entry_point: "main",
            }),
            rasterization_state: Some(wgpu::RasterizationStateDescriptor {
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: wgpu::CullMode::Back,
                depth_bias: 0,
                depth_bias_slope_scale: 0.0,
                depth_bias_clamp: 0.0,
            }),
            primitive_topology: wgpu::PrimitiveTopology::TriangleList,
            color_states: &[wgpu::ColorStateDescriptor {
                format: sc_desc.format,
                color_blend: wgpu::BlendDescriptor::REPLACE,
                alpha_blend: wgpu::BlendDescriptor::REPLACE,
                write_mask: wgpu::ColorWrite::ALL,
            }],
            depth_stencil_state: None,
            vertex_state: wgpu::VertexStateDescriptor {
                index_format: wgpu::IndexFormat::Uint16,
                vertex_buffers: &[Vertex::descriptor()],
            },
            sample_count: 1,
            sample_mask: !0,
            alpha_to_coverage_enabled: false,
        });

        Ok(Self {
            config,
            camera,
            camera_controller,
            uniforms,
            size,
            objects: Vec::new(),
            gpu: GpuState {
                surface,
                device,
                queue,
                sc_desc,
                swap_chain,
                render_pipeline,
                uniform_buffer,
                uniform_bind_group,
            },
        })
    }

    pub fn create_object(&mut self, vertices: &[Vertex], indices: &[u16]) -> usize {
        let object = Object::new(&self.gpu.device, vertices, indices);
        self.objects.push(object);
        self.objects.len() - 1
    }

    pub fn create_instance(&mut self, object_id: usize, position: cgmath::Vector3<f32>, rotation: cgmath::Quaternion<f32>) -> Option<usize> {
        match self.objects.get_mut(object_id) {
            Some(object) => {
                object.add_instance(&self.gpu.device, position, rotation);

                let uniform_bind_group_layout =
                self.gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    bindings: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStage::VERTEX,
                            ty: wgpu::BindingType::UniformBuffer { dynamic: false },
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStage::VERTEX,
                            ty: wgpu::BindingType::StorageBuffer {
                                // We don't plan on changing the size of this buffer
                                dynamic: false,
                                // The shader is not allowed to modify it's contents
                                readonly: true,
                            },
                        },
                    ],
                    label: Some("uniform_bind_group_layout"),
                });
        
                let uniform_bind_group = self.gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    layout: &uniform_bind_group_layout,
                    bindings: &[
                        wgpu::Binding {
                            binding: 0,
                            resource: wgpu::BindingResource::Buffer {
                                buffer: &self.gpu.uniform_buffer,
                                // FYI: you can share a single buffer between bindings.
                                range: 0..std::mem::size_of_val(&self.uniforms) as wgpu::BufferAddress,
                            },
                        },
                        wgpu::Binding {
                            binding: 1,
                            resource: wgpu::BindingResource::Buffer {
                                buffer: &object.instance_buffer(),
                                range: 0..object.instance_buffer_size() as wgpu::BufferAddress,
                            },
                        },
                    ],
                    label: Some("uniform_bind_group"),
                });
        
                self.gpu.uniform_bind_group = uniform_bind_group;

                let mut compiler = shaders::ShaderCompiler::new().unwrap();
                let vs_module = shaders::basic::vertex_module(&self.gpu.device, &mut compiler).unwrap();
                let fs_module = shaders::basic::fragment_module(&self.gpu.device, &mut compiler).unwrap();

                let render_pipeline_layout =
                self.gpu.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    bind_group_layouts: &[&uniform_bind_group_layout],
                });
    
                self.gpu.render_pipeline = self.gpu.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    layout: &render_pipeline_layout,
                    vertex_stage: wgpu::ProgrammableStageDescriptor {
                        module: &vs_module,
                        entry_point: "main",
                    },
                    fragment_stage: Some(wgpu::ProgrammableStageDescriptor {
                        module: &fs_module,
                        entry_point: "main",
                    }),
                    rasterization_state: Some(wgpu::RasterizationStateDescriptor {
                        front_face: wgpu::FrontFace::Ccw,
                        cull_mode: wgpu::CullMode::Back,
                        depth_bias: 0,
                        depth_bias_slope_scale: 0.0,
                        depth_bias_clamp: 0.0,
                    }),
                    primitive_topology: wgpu::PrimitiveTopology::TriangleList,
                    color_states: &[wgpu::ColorStateDescriptor {
                        format: self.gpu.sc_desc.format,
                        color_blend: wgpu::BlendDescriptor::REPLACE,
                        alpha_blend: wgpu::BlendDescriptor::REPLACE,
                        write_mask: wgpu::ColorWrite::ALL,
                    }],
                    depth_stencil_state: None,
                    vertex_state: wgpu::VertexStateDescriptor {
                        index_format: wgpu::IndexFormat::Uint16,
                        vertex_buffers: &[Vertex::descriptor()],
                    },
                    sample_count: 1,
                    sample_mask: !0,
                    alpha_to_coverage_enabled: false,
                });
        
                Some(object.num_instances() - 1)
            },
            None => None
        }
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        self.size = new_size;
        self.gpu.sc_desc.width = new_size.width;
        self.gpu.sc_desc.height = new_size.height;
        self.gpu.swap_chain = self
            .gpu
            .device
            .create_swap_chain(&self.gpu.surface, &self.gpu.sc_desc);
    }

    pub fn input(&mut self, event: &WindowEvent) -> bool {
        self.camera_controller.process_events(event)
    }

    pub fn update(&mut self) {
        self.camera_controller.update_camera(&mut self.camera);
        self.uniforms.update_view_proj(&self.camera);

        // Copy operation's are performed on the gpu, so we'll need
        // a CommandEncoder for that
        let mut encoder = self
            .gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("update encoder"),
            });

        let staging_buffer = self.gpu.device.create_buffer_with_data(
            bytemuck::cast_slice(&[self.uniforms]),
            wgpu::BufferUsage::COPY_SRC,
        );

        encoder.copy_buffer_to_buffer(
            &staging_buffer,
            0,
            &self.gpu.uniform_buffer,
            0,
            std::mem::size_of::<Uniforms>() as wgpu::BufferAddress,
        );

        // We need to remember to submit our CommandEncoder's output
        // otherwise we won't see any change.
        self.gpu.queue.submit(&[encoder.finish()]);
    }

    pub fn render(&mut self) {
        let frame = self
            .gpu
            .swap_chain
            .get_next_texture()
            .expect("Timeout getting texture");

        let mut encoder = self
            .gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[wgpu::RenderPassColorAttachmentDescriptor {
                    attachment: &frame.view,
                    resolve_target: None,
                    load_op: wgpu::LoadOp::Clear,
                    store_op: wgpu::StoreOp::Store,
                    clear_color: self.config.clear_color,
                }],
                depth_stencil_attachment: None,
            });

            render_pass.set_pipeline(&self.gpu.render_pipeline);
            render_pass.set_bind_group(0, &self.gpu.uniform_bind_group, &[]);

            for object in &self.objects {
                let num_instanaces = object.num_instances() as u32;
                if num_instanaces > 0 {
                    render_pass.set_vertex_buffer(0, object.vertex_buffer(), 0, 0);
                    render_pass.set_index_buffer(object.index_buffer(), 0, 0);
                    render_pass.draw_indexed(0..object.num_indices(), 0, 0..num_instanaces);                
                }
            }
        }

        self.gpu.queue.submit(&[encoder.finish()]);
    }
}
