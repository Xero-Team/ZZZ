use std::sync::Arc;

use gpui::{DevicePixels, PlatformAtlas, PlatformHeadlessRenderer, Scene, Size};

use crate::WgpuContext;

use super::{ReadbackCopy, WgpuRenderer};

struct OffscreenTarget {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    readback: wgpu::Buffer,
    padded_bytes_per_row: u32,
    size: Size<DevicePixels>,
}

impl OffscreenTarget {
    fn new(renderer: &WgpuRenderer, size: Size<DevicePixels>) -> Self {
        let width = size.width.0 as u32;
        let height = size.height.0 as u32;
        let padded_bytes_per_row = (width * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let resources = renderer.resources();
        let texture = resources.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("gpui_offscreen_target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: renderer.surface_config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let readback = resources.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gpui_offscreen_readback"),
            size: u64::from(padded_bytes_per_row) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        Self {
            texture,
            view,
            readback,
            padded_bytes_per_row,
            size,
        }
    }

    fn readback_copy(&self) -> ReadbackCopy<'_> {
        ReadbackCopy {
            texture: &self.texture,
            buffer: &self.readback,
            bytes_per_row: self.padded_bytes_per_row,
            width: self.size.width.0 as u32,
            height: self.size.height.0 as u32,
        }
    }
}

/// A surface-free WGPU renderer for visual tests.
pub struct WgpuHeadlessRenderer {
    renderer: WgpuRenderer,
    target: Option<OffscreenTarget>,
}

impl WgpuHeadlessRenderer {
    /// Creates a renderer using the preferred hardware adapter.
    pub fn new() -> anyhow::Result<Self> {
        Self::new_with_fallback(false)
    }

    /// Creates a renderer, optionally forcing WGPU's fallback adapter.
    pub fn new_with_fallback(force_fallback_adapter: bool) -> anyhow::Result<Self> {
        let context = WgpuContext::new_headless(force_fallback_adapter)?;
        let renderer = WgpuRenderer::new_headless(
            &context,
            Size {
                width: DevicePixels(1),
                height: DevicePixels(1),
            },
        )?;
        Ok(Self {
            renderer,
            target: None,
        })
    }

    /// Returns the adapter selected for this renderer.
    pub fn gpu_specs(&self) -> gpui::GpuSpecs {
        self.renderer.gpu_specs()
    }

    fn ensure_target(&mut self, size: Size<DevicePixels>) -> anyhow::Result<()> {
        anyhow::ensure!(
            size.width.0 > 0 && size.height.0 > 0,
            "headless render target must have positive dimensions"
        );
        if self
            .target
            .as_ref()
            .is_some_and(|target| target.size == size)
        {
            return Ok(());
        }
        self.renderer.update_drawable_size(size);
        self.target = Some(OffscreenTarget::new(&self.renderer, size));
        Ok(())
    }

    fn read_target(
        renderer: &WgpuRenderer,
        target: &OffscreenTarget,
        submission: wgpu::SubmissionIndex,
    ) -> anyhow::Result<image::RgbaImage> {
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        target
            .readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                if sender.send(result).is_err() {
                    log::error!("headless readback receiver dropped");
                }
            });
        renderer
            .resources()
            .device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: None,
            })
            .map_err(|error| anyhow::anyhow!("failed to poll headless readback: {error}"))?;
        receiver
            .recv()
            .map_err(|error| anyhow::anyhow!("headless readback callback dropped: {error}"))?
            .map_err(|error| anyhow::anyhow!("failed to map headless readback: {error}"))?;

        let width = target.size.width.0 as u32;
        let height = target.size.height.0 as u32;
        let bytes_per_row = width * 4;
        let mapped = target.readback.slice(..).get_mapped_range();
        let mut pixels = Vec::with_capacity(bytes_per_row as usize * height as usize);
        for row in mapped.chunks_exact(target.padded_bytes_per_row as usize) {
            pixels.extend_from_slice(&row[..bytes_per_row as usize]);
        }
        drop(mapped);
        target.readback.unmap();
        if renderer.surface_config.format == wgpu::TextureFormat::Bgra8Unorm {
            for pixel in pixels.chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }
        }
        image::RgbaImage::from_raw(width, height, pixels)
            .ok_or_else(|| anyhow::anyhow!("headless readback dimensions did not match its data"))
    }
}

impl PlatformHeadlessRenderer for WgpuHeadlessRenderer {
    fn render_scene_to_image(
        &mut self,
        scene: &Scene,
        size: Size<DevicePixels>,
    ) -> anyhow::Result<image::RgbaImage> {
        self.ensure_target(size)?;
        let Self { renderer, target } = self;
        let target = target.as_ref().expect("headless target was ensured");
        let submission = renderer
            .render_to_view(scene, &target.view, Some(target.readback_copy()))
            .ok_or_else(|| anyhow::anyhow!("failed to render headless scene"))?;
        Self::read_target(renderer, target, submission)
    }

    fn sprite_atlas(&self) -> Arc<dyn PlatformAtlas> {
        self.renderer.sprite_atlas().clone()
    }
}
