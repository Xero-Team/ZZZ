use bytemuck::{Pod, Zeroable};
use gpui::{Background, Bounds, Point, ScaledPixels};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct GlobalParams {
    pub(super) viewport_size: [f32; 2],
    pub(super) premultiplied_alpha: u32,
    pub(super) pad: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct PodBounds {
    pub(super) origin: [f32; 2],
    pub(super) size: [f32; 2],
}

impl From<Bounds<ScaledPixels>> for PodBounds {
    fn from(bounds: Bounds<ScaledPixels>) -> Self {
        Self {
            origin: [bounds.origin.x.0, bounds.origin.y.0],
            size: [bounds.size.width.0, bounds.size.height.0],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct SurfaceParams {
    pub(super) bounds: PodBounds,
    pub(super) content_mask: PodBounds,
}

pub(super) struct ReadbackCopy<'a> {
    pub(super) texture: &'a wgpu::Texture,
    pub(super) buffer: &'a wgpu::Buffer,
    pub(super) bytes_per_row: u32,
    pub(super) width: u32,
    pub(super) height: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct GammaParams {
    pub(super) gamma_ratios: [f32; 4],
    pub(super) grayscale_enhanced_contrast: f32,
    pub(super) subpixel_enhanced_contrast: f32,
    pub(super) is_bgr: u32,
    pub(super) _pad: u32,
}

#[derive(Clone, Debug)]
#[repr(C)]
pub(super) struct PathSprite {
    pub(super) bounds: Bounds<ScaledPixels>,
}

#[derive(Clone, Debug)]
#[repr(C)]
pub(super) struct PathRasterizationVertex {
    pub(super) xy_position: Point<ScaledPixels>,
    pub(super) st_position: Point<f32>,
    pub(super) color: Background,
    pub(super) bounds: Bounds<ScaledPixels>,
}
