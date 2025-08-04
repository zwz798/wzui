use std::{iter::once, sync::Arc};

use bytemuck::{Pod, Zeroable}; // <-- 引入 bytemuck
use wgpu::{
    Adapter, Buffer, Color, CommandEncoderDescriptor, Device, DeviceDescriptor, Instance,
    InstanceDescriptor, MemoryHints, Operations, PipelineCompilationOptions, Queue,
    RenderPassColorAttachment, RenderPassDescriptor, RenderPipeline, RequestAdapterOptions,
    Surface, SurfaceConfiguration, SurfaceError, TextureViewDescriptor, util::DeviceExt,
};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event_loop::{self, ActiveEventLoop},
    window::{Window, WindowAttributes},
};

// =================================================================================
// 步骤 1.1: 定义顶点结构体
// =================================================================================
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct Vertex {
    position: [f32; 3], // 从 2D -> 3D，为了着色器中的 vec3
    color: [f32; 3],
}

impl Vertex {
    // 描述顶点在内存中的布局，以便 wgpu 正确读取
    fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0, // 对应着色器中的 @location(0)
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    shader_location: 1, // 对应着色器中的 @location(1)
                    format: wgpu::VertexFormat::Float32x3,
                },
            ],
        }
    }
}

// 定义正方形的顶点和索引
const VERTICES: &[Vertex] = &[
    Vertex {
        position: [-0.5, 0.5, 0.0],
        color: [1.0, 0.0, 0.0],
    }, // 左上, 红色
    Vertex {
        position: [-0.5, -0.5, 0.0],
        color: [0.0, 1.0, 0.0],
    }, // 左下, 绿色
    Vertex {
        position: [0.5, -0.5, 0.0],
        color: [0.0, 0.0, 1.0],
    }, // 右下, 蓝色
    Vertex {
        position: [0.5, 0.5, 0.0],
        color: [1.0, 1.0, 0.0],
    }, // 右上, 黄色
];

const INDICES: &[u16] = &[
    0, 1, 2, // 第一个三角形
    0, 2, 3, // 第二个三角形
];

#[derive(Default)]
struct App {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let window = Arc::new(
                event_loop
                    .create_window(WindowAttributes::default())
                    .unwrap(),
            );
            self.window = Some(window.clone());
            self.renderer = Some(pollster::block_on(Renderer::new(window)));
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        let (Some(window), Some(renderer)) = (self.window.as_mut(), self.renderer.as_mut()) else {
            return;
        };

        if window_id != window.id() {
            return;
        }

        match event {
            winit::event::WindowEvent::CloseRequested => event_loop.exit(),
            winit::event::WindowEvent::Resized(new_size) => renderer.resize(new_size),
            winit::event::WindowEvent::RedrawRequested => {
                window.request_redraw(); // 确保在下一次循环时再次触发重绘
                match renderer.render() {
                    Err(SurfaceError::Lost | SurfaceError::OutOfMemory) => event_loop.exit(),
                    Err(e) => eprintln!("Error rendering: {:?}", e),
                    Ok(_) => {}
                }
            }
            _ => {}
        }
    }
}

// =================================================================================
// 步骤 1.2: 扩展 Renderer 来持有渲染所需资源
// =================================================================================
struct Renderer {
    surface: Surface<'static>,
    config: SurfaceConfiguration,
    size: PhysicalSize<u32>,
    device: Device,
    queue: Queue,
    render_pipeline: RenderPipeline,
    vertex_buffer: Buffer,
    index_buffer: Buffer,
    num_indices: u32,
}

impl Renderer {
    async fn new(window: Arc<Window>) -> Self {
        let size = window.inner_size();
        let instance = Instance::new(&InstanceDescriptor::default());
        let surface = instance.create_surface(window).unwrap();

        let adapter = instance
            .request_adapter(&RequestAdapterOptions::default())
            .await
            .unwrap();

        let (device, queue) = adapter
            .request_device(&DeviceDescriptor {
                label: Some("Device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: MemoryHints::Performance,
                trace: wgpu::Trace::Off,
            })
            .await
            .unwrap();

        let surface_caps = surface.get_capabilities(&adapter);
        let format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Fifo, // VSync
            desired_maximum_frame_latency: 2,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
        };

        surface.configure(&device, &config);

        // =================================================================================
        // 步骤 1.3: 创建着色器、管线和缓冲区
        // =================================================================================

        // 加载 WGSL 着色器代码
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });

        // 创建渲染管线布局
        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[],
                push_constant_ranges: &[],
            });

        // 创建渲染管线
        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"), // 顶点着色器入口函数
                buffers: &[Vertex::desc()],   // 顶点布局描述
                compilation_options: PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"), // 片元着色器入口函数
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // 创建顶点缓冲区
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: bytemuck::cast_slice(VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });

        // 创建索引缓冲区
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Index Buffer"),
            contents: bytemuck::cast_slice(INDICES),
            usage: wgpu::BufferUsages::INDEX,
        });

        let num_indices = INDICES.len() as u32;

        Self {
            surface,
            config,
            size,
            device,
            queue,
            render_pipeline, // <-- 保存管线
            vertex_buffer,   // <-- 保存顶点缓冲区
            index_buffer,    // <-- 保存索引缓冲区
            num_indices,     // <-- 保存索引数量
        }
    }

    fn resize(&mut self, new_size: PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    fn render(&mut self) -> Result<(), SurfaceError> {
        let texture = self.surface.get_current_texture()?;
        let view = texture
            .texture
            .create_view(&TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        // =================================================================================
        // 步骤 1.4: 在渲染通道中执行绘制命令
        // =================================================================================
        {
            let mut render_pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: Operations {
                        load: wgpu::LoadOp::Clear(Color {
                            // 清屏操作依然保留
                            r: 0.1,
                            g: 0.2,
                            b: 0.3,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // 设置渲染管线
            render_pass.set_pipeline(&self.render_pipeline);
            // 设置顶点缓冲区
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            // 设置索引缓冲区
            render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            // 执行绘制！
            render_pass.draw_indexed(0..self.num_indices, 0, 0..1);
        }

        self.queue.submit(once(encoder.finish()));
        texture.present();
        Ok(())
    }
}

fn main() {
    let event_loop = event_loop::EventLoop::new().unwrap();
    let mut app = App::default();
    event_loop.run_app(&mut app).unwrap();
}
