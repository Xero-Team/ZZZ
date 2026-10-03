use std::sync::Arc;

pub(super) struct WgpuPipelines {
    pub(super) quads: wgpu::RenderPipeline,
    pub(super) shadows: wgpu::RenderPipeline,
    pub(super) path_rasterization: wgpu::RenderPipeline,
    pub(super) paths: wgpu::RenderPipeline,
    pub(super) underlines: wgpu::RenderPipeline,
    pub(super) mono_sprites: wgpu::RenderPipeline,
    pub(super) subpixel_sprites: Option<wgpu::RenderPipeline>,
    pub(super) poly_sprites: wgpu::RenderPipeline,
    #[allow(dead_code)]
    pub(super) surfaces: wgpu::RenderPipeline,
}

pub(super) struct WgpuBindGroupLayouts {
    pub(super) globals: wgpu::BindGroupLayout,
    pub(super) instances: wgpu::BindGroupLayout,
    pub(super) instances_with_texture: wgpu::BindGroupLayout,
    pub(super) surfaces: wgpu::BindGroupLayout,
}

/// GPU resources that must be dropped together during device recovery.
pub(super) struct WgpuResources {
    pub(super) device: Arc<wgpu::Device>,
    pub(super) queue: Arc<wgpu::Queue>,
    pub(super) surface: Option<wgpu::Surface<'static>>,
    pub(super) pipelines: WgpuPipelines,
    pub(super) bind_group_layouts: WgpuBindGroupLayouts,
    pub(super) atlas_sampler: wgpu::Sampler,
    pub(super) globals_buffer: wgpu::Buffer,
    pub(super) globals_bind_group: wgpu::BindGroup,
    pub(super) path_globals_bind_group: wgpu::BindGroup,
    pub(super) instance_buffer: wgpu::Buffer,
    pub(super) path_intermediate_texture: Option<wgpu::Texture>,
    pub(super) path_intermediate_view: Option<wgpu::TextureView>,
    pub(super) path_msaa_texture: Option<wgpu::Texture>,
    pub(super) path_msaa_view: Option<wgpu::TextureView>,
}

impl WgpuResources {
    pub(super) fn invalidate_intermediate_textures(&mut self) {
        self.path_intermediate_texture = None;
        self.path_intermediate_view = None;
        self.path_msaa_texture = None;
        self.path_msaa_view = None;
    }
}
