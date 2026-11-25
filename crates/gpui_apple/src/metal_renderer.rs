use crate::metal_atlas::MetalAtlas;
#[cfg(any(test, feature = "test-support"))]
use anyhow::Context as _;
use anyhow::Result;
use block2::RcBlock;
use gpui::{
    AtlasTextureId, Background, Bounds, ContentMask, DevicePixels, MonochromeSprite, PaintSurface,
    Path, Point, PolychromeSprite, PrimitiveBatch, Quad, ScaledPixels, Scene, Shadow, Size,
    Underline, point, size,
};
#[cfg(any(test, feature = "test-support"))]
use image::RgbaImage;
use objc2::{Message, rc::Retained, runtime::ProtocolObject};
use objc2_core_foundation::{CFRetained, CGSize};
use objc2_core_graphics::{CGColorSpace, kCGColorSpaceDisplayP3};
use objc2_core_video::{
    CVMetalTexture, CVMetalTextureCache, CVMetalTextureGetTexture, CVPixelBuffer,
    CVPixelBufferGetHeight, CVPixelBufferGetHeightOfPlane, CVPixelBufferGetPixelFormatType,
    CVPixelBufferGetWidth, CVPixelBufferGetWidthOfPlane,
    kCVPixelFormatType_420YpCbCr8BiPlanarFullRange, kCVReturnSuccess,
};
use objc2_foundation::{NSRange, NSString};
#[cfg(target_os = "macos")]
use objc2_metal::MTLCopyAllDevices;
use objc2_metal::{
    MTLBlendFactor, MTLBlendOperation, MTLBuffer, MTLClearColor, MTLCommandBuffer,
    MTLCommandEncoder, MTLCommandQueue, MTLCreateSystemDefaultDevice, MTLDevice, MTLDrawable,
    MTLGPUFamily, MTLLibrary, MTLLoadAction, MTLPixelFormat, MTLPrimitiveType,
    MTLRenderCommandEncoder, MTLRenderPassColorAttachmentDescriptor, MTLRenderPassDescriptor,
    MTLRenderPipelineDescriptor, MTLRenderPipelineState, MTLResourceOptions, MTLStorageMode,
    MTLStoreAction, MTLTexture, MTLTextureDescriptor, MTLTextureUsage, MTLViewport,
};
#[cfg(any(test, feature = "test-support"))]
use objc2_metal::{MTLBlitCommandEncoder, MTLOrigin, MTLRegion, MTLSize};
use objc2_quartz_core::{CAMetalDrawable, CAMetalLayer};
use parking_lot::Mutex;

use std::{cell::Cell, mem, ptr, sync::Arc};

// Exported to metal
pub(crate) type PointF = gpui::Point<f32>;

#[cfg(not(any(feature = "runtime_shaders", gpui_cross_runtime_shaders)))]
const SHADERS_METALLIB: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/shaders.metallib"));
#[cfg(any(feature = "runtime_shaders", gpui_cross_runtime_shaders))]
const SHADERS_SOURCE_FILE: &str = include_str!(concat!(env!("OUT_DIR"), "/stitched_shaders.metal"));
// Use 4x MSAA, all devices support it.
// https://developer.apple.com/documentation/metal/mtldevice/1433355-supportstexturesamplecount
const PATH_SAMPLE_COUNT: u32 = 4;

/// Metal requires offsets passed alongside a buffer binding to be a multiple of
/// 256 bytes, so every batch written into the shared instance buffer starts at
/// an offset rounded up to this alignment.
const INSTANCE_BUFFER_ALIGNMENT: usize = 256;

const INITIAL_INSTANCE_BUFFER_SIZE: usize = 2 * 1024 * 1024;
const MAX_INSTANCE_BUFFER_SIZE: usize = 256 * 1024 * 1024;

pub type Context = Arc<Mutex<InstanceBufferPool>>;
pub type Renderer = MetalRenderer;

pub fn new_renderer(
    context: self::Context,
    transparent: bool,
    use_display_p3: bool,
) -> Result<Renderer> {
    MetalRenderer::new(context, transparent, use_display_p3)
}

pub struct InstanceBufferPool {
    buffer_size: usize,
    buffers: Vec<InstanceBuffer>,
}

impl Default for InstanceBufferPool {
    fn default() -> Self {
        Self {
            buffer_size: INITIAL_INSTANCE_BUFFER_SIZE,
            buffers: Vec::new(),
        }
    }
}

pub(crate) struct InstanceBuffer {
    metal_buffer: Retained<ProtocolObject<dyn MTLBuffer>>,
    size: usize,
}

impl InstanceBuffer {
    /// Copies plain-old-data instances into the buffer, starting at the next
    /// 256-byte aligned offset at or after `offset`. Returns the offset the
    /// instances were written at, or `None` when they do not fit.
    ///
    /// `items` must yield exactly `count` values; a shorter iterator leaves
    /// uninitialized bytes behind, so it is treated as a programmer error.
    fn write<T>(
        &self,
        offset: &mut usize,
        count: usize,
        items: impl Iterator<Item = T>,
    ) -> Option<usize> {
        let start = align_offset(*offset);
        let end = start.checked_add(mem::size_of::<T>().checked_mul(count)?)?;
        if end > self.size {
            return None;
        }

        // SAFETY: `start..end` was bounds-checked against the buffer's length,
        // and the instance types written here are plain-old-data copies of
        // values owned by the caller (`T` is never a reference into the buffer).
        let destination = unsafe {
            self.metal_buffer
                .contents()
                .as_ptr()
                .cast::<u8>()
                .add(start)
                .cast::<T>()
        };
        let mut written = 0;
        for item in items.take(count) {
            unsafe { ptr::write(destination.add(written), item) };
            written += 1;
        }
        if written != count {
            log::error!("instance writer received {written} items but expected {count}");
            return None;
        }

        *offset = end;
        Some(start)
    }

    /// Copies a slice of plain-old-data instances into the buffer at the next
    /// 256-byte aligned offset, returning that offset.
    fn write_slice<T: Copy>(&self, offset: &mut usize, items: &[T]) -> Option<usize> {
        self.write(offset, items.len(), items.iter().copied())
    }
}

impl InstanceBufferPool {
    pub(crate) fn reset(&mut self, buffer_size: usize) {
        self.buffer_size = buffer_size;
        self.buffers.clear();
    }

    /// Returns a buffer with room for at least `required_bytes`.
    ///
    /// Growing here means the common case encodes the whole scene on the first
    /// attempt, instead of discovering the overflow halfway through encoding and
    /// having to render everything again.
    pub(crate) fn acquire(
        &mut self,
        device: &ProtocolObject<dyn MTLDevice>,
        unified_memory: bool,
        required_bytes: usize,
    ) -> Result<InstanceBuffer> {
        if required_bytes > self.buffer_size {
            let growth_target = required_bytes
                .checked_next_power_of_two()
                .unwrap_or(required_bytes);
            self.buffer_size = growth_target.min(MAX_INSTANCE_BUFFER_SIZE);
            self.buffers.clear();
        }

        let buffer = if let Some(buffer) = self.buffers.pop() {
            buffer
        } else {
            let options = if unified_memory {
                MTLResourceOptions::StorageModeShared
                    // Buffers are write only which can benefit from the combined cache
                    // https://developer.apple.com/documentation/metal/mtlresourceoptions/cpucachemodewritecombined
                    | MTLResourceOptions::CPUCacheModeWriteCombined
            } else {
                MTLResourceOptions::StorageModeManaged
            };
            let metal_buffer = device
                .newBufferWithLength_options(self.buffer_size, options)
                .ok_or_else(|| {
                    anyhow::anyhow!("allocating {}-byte instance buffer", self.buffer_size)
                })?;
            InstanceBuffer {
                metal_buffer,
                size: self.buffer_size,
            }
        };
        Ok(buffer)
    }

    pub(crate) fn release(&mut self, buffer: InstanceBuffer) {
        if buffer.size == self.buffer_size {
            self.buffers.push(buffer)
        }
    }
}

enum DisplayLinkMode {
    Detached,
    Attached {
        drawable: Option<Retained<ProtocolObject<dyn CAMetalDrawable>>>,
    },
}

pub struct MetalRenderer {
    device: Retained<ProtocolObject<dyn MTLDevice>>,
    layer: Option<Retained<CAMetalLayer>>,
    is_apple_gpu: bool,
    is_unified_memory: bool,
    presents_with_transaction: bool,
    display_link_mode: DisplayLinkMode,
    /// For headless rendering, tracks whether output should be opaque
    opaque: bool,
    command_queue: Retained<ProtocolObject<dyn MTLCommandQueue>>,
    paths_rasterization_pipeline_state: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
    path_sprites_pipeline_state: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
    shadows_pipeline_state: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
    quads_pipeline_state: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
    underlines_pipeline_state: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
    monochrome_sprites_pipeline_state: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
    polychrome_sprites_pipeline_state: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
    surfaces_pipeline_state: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
    unit_vertices: Retained<ProtocolObject<dyn MTLBuffer>>,
    instance_buffer_pool: Arc<Mutex<InstanceBufferPool>>,
    sprite_atlas: Arc<MetalAtlas>,
    core_video_texture_cache: CFRetained<CVMetalTextureCache>,
    path_intermediate_texture: Option<Retained<ProtocolObject<dyn MTLTexture>>>,
    path_intermediate_msaa_texture: Option<Retained<ProtocolObject<dyn MTLTexture>>>,
    path_sample_count: u32,
}

#[repr(C)]
pub struct PathRasterizationVertex {
    pub xy_position: Point<ScaledPixels>,
    pub st_position: Point<f32>,
    pub color: Background,
    pub bounds: Bounds<ScaledPixels>,
}

impl MetalRenderer {
    /// Creates a new MetalRenderer with a CAMetalLayer for window-based rendering.
    pub fn new(
        instance_buffer_pool: Arc<Mutex<InstanceBufferPool>>,
        transparent: bool,
        use_display_p3: bool,
    ) -> Result<Self> {
        let device = Self::create_device()?;
        let layer = CAMetalLayer::new();
        Self::configure_layer(&layer, &device, transparent, use_display_p3);
        Self::new_internal(device, Some(layer), !transparent, instance_buffer_pool)
    }

    /// Creates a renderer for a CAMetalLayer owned by a platform view, such as
    /// the backing layer UIKit creates for a view whose `layerClass` is
    /// `CAMetalLayer`. The renderer retains the layer.
    pub fn from_layer(
        instance_buffer_pool: Arc<Mutex<InstanceBufferPool>>,
        layer: &CAMetalLayer,
        transparent: bool,
    ) -> Result<Self> {
        let device = Self::create_device()?;
        let layer = layer.retain();
        Self::configure_layer(&layer, &device, transparent, false);
        Self::new_internal(device, Some(layer), !transparent, instance_buffer_pool)
    }

    fn configure_layer(
        layer: &CAMetalLayer,
        device: &ProtocolObject<dyn MTLDevice>,
        transparent: bool,
        use_display_p3: bool,
    ) {
        layer.setDevice(Some(device));
        layer.setPixelFormat(MTLPixelFormat::BGRA8Unorm);
        if use_display_p3 {
            // SAFETY: The constant is extern, from Core Graphics
            let display_p3_name = unsafe { kCGColorSpaceDisplayP3 };
            if let Some(display_p3_color_space) = CGColorSpace::with_name(Some(display_p3_name)) {
                layer.setColorspace(Some(&display_p3_color_space));
            } else {
                log::error!("unable to create the Display P3 color space");
            }
        }
        // Support direct-to-display rendering if the window is not transparent
        // https://developer.apple.com/documentation/metal/managing-your-game-window-for-metal-in-macos
        layer.setOpaque(!transparent);
        layer.setMaximumDrawableCount(3);
        // Allow texture reading for visual tests and UI automation screenshots
        // (captures without ScreenCaptureKit). Only debug builds pay the
        // presentation cost, even when `test-support` is compiled in.
        #[cfg(all(feature = "test-support", debug_assertions))]
        layer.setFramebufferOnly(false);
        layer.setAllowsNextDrawableTimeout(false);
        layer.setNeedsDisplayOnBoundsChange(true);
        // UIKit sizes a view's backing layer itself; only AppKit-hosted
        // layers need to track their superlayer's bounds.
        #[cfg(target_os = "macos")]
        layer.setAutoresizingMask(
            objc2_quartz_core::CAAutoresizingMask::LayerWidthSizable
                | objc2_quartz_core::CAAutoresizingMask::LayerHeightSizable,
        );
    }

    /// Creates a new headless MetalRenderer for offscreen rendering without a window.
    ///
    /// This renderer can render scenes to images without requiring a CAMetalLayer,
    /// window, or AppKit. Use `render_scene_to_image()` to render scenes.
    #[cfg(any(test, feature = "test-support"))]
    pub fn new_headless(instance_buffer_pool: Arc<Mutex<InstanceBufferPool>>) -> Result<Self> {
        let device = Self::create_device()?;
        Self::new_internal(device, None, true, instance_buffer_pool)
    }

    #[cfg(target_os = "macos")]
    fn create_device() -> Result<Retained<ProtocolObject<dyn MTLDevice>>> {
        // Prefer low‐power integrated GPUs on Intel Mac. On Apple
        // Silicon, there is only ever one GPU, so this is equivalent to
        // `metal::Device::system_default()`.
        if let Some(device) = MTLCopyAllDevices()
            .iter()
            .min_by_key(|device| (device.isRemovable(), !device.isLowPower()))
        {
            Ok(device)
        } else {
            // For some reason `all()` can return an empty list, see https://github.com/zed-industries/zed/issues/37689
            // In that case, we fall back to the system default device.
            log::error!(
                "Unable to enumerate Metal devices; attempting to use system default device"
            );
            Self::system_default_device()
        }
    }

    #[cfg(target_os = "ios")]
    fn create_device() -> Result<Retained<ProtocolObject<dyn MTLDevice>>> {
        Self::system_default_device()
    }

    fn system_default_device() -> Result<Retained<ProtocolObject<dyn MTLDevice>>> {
        MTLCreateSystemDefaultDevice()
            .ok_or_else(|| anyhow::anyhow!("unable to access a compatible graphics device"))
    }

    fn new_internal(
        device: Retained<ProtocolObject<dyn MTLDevice>>,
        layer: Option<Retained<CAMetalLayer>>,
        opaque: bool,
        instance_buffer_pool: Arc<Mutex<InstanceBufferPool>>,
    ) -> Result<Self> {
        #[cfg(any(feature = "runtime_shaders", gpui_cross_runtime_shaders))]
        let library = device
            .newLibraryWithSource_options_error(
                &NSString::from_str(SHADERS_SOURCE_FILE),
                Some(&objc2_metal::MTLCompileOptions::new()),
            )
            .map_err(|error| anyhow::anyhow!("building Metal library: {error}"))?;
        #[cfg(not(any(feature = "runtime_shaders", gpui_cross_runtime_shaders)))]
        let library = device
            .newLibraryWithData_error(&dispatch2::DispatchData::from_static_bytes(
                SHADERS_METALLIB,
            ))
            .map_err(|error| anyhow::anyhow!("loading Metal library: {error}"))?;

        fn to_float2_bits(point: PointF) -> u64 {
            let mut output = point.y.to_bits() as u64;
            output <<= 32;
            output |= point.x.to_bits() as u64;
            output
        }

        // Shared memory can be used only if CPU and GPU share the same memory space.
        // https://developer.apple.com/documentation/metal/setting-resource-storage-modes
        // iOS does not support managed resources. Its simulator may report a
        // non-unified host GPU even though resources must still use shared storage.
        let is_unified_memory = cfg!(target_os = "ios") || device.hasUnifiedMemory();
        // Apple GPU families support memoryless textures, which can significantly reduce
        // memory usage by keeping render targets in on-chip tile memory instead of
        // allocating backing store in system memory.
        // https://developer.apple.com/documentation/metal/mtlgpufamily
        let is_apple_gpu = cfg!(target_os = "ios") || device.supportsFamily(MTLGPUFamily::Apple1);

        let unit_vertices = [
            to_float2_bits(point(0., 0.)),
            to_float2_bits(point(1., 0.)),
            to_float2_bits(point(0., 1.)),
            to_float2_bits(point(0., 1.)),
            to_float2_bits(point(1., 0.)),
            to_float2_bits(point(1., 1.)),
        ];
        // SAFETY: The pointer and length describe `unit_vertices`, which outlives the copy.
        let unit_vertices = unsafe {
            device.newBufferWithBytes_length_options(
                ptr::NonNull::from(&unit_vertices).cast(),
                mem::size_of_val(&unit_vertices),
                if is_unified_memory {
                    MTLResourceOptions::StorageModeShared
                        | MTLResourceOptions::CPUCacheModeWriteCombined
                } else {
                    MTLResourceOptions::StorageModeManaged
                },
            )
        }
        .ok_or_else(|| anyhow::anyhow!("allocating unit vertex buffer"))?;

        let paths_rasterization_pipeline_state = build_path_rasterization_pipeline_state(
            &device,
            &library,
            "paths_rasterization",
            "path_rasterization_vertex",
            "path_rasterization_fragment",
            MTLPixelFormat::BGRA8Unorm,
            PATH_SAMPLE_COUNT,
        )?;
        let path_sprites_pipeline_state = build_path_sprite_pipeline_state(
            &device,
            &library,
            "path_sprites",
            "path_sprite_vertex",
            "path_sprite_fragment",
            MTLPixelFormat::BGRA8Unorm,
        )?;
        let shadows_pipeline_state = build_pipeline_state(
            &device,
            &library,
            "shadows",
            "shadow_vertex",
            "shadow_fragment",
            MTLPixelFormat::BGRA8Unorm,
        )?;
        let quads_pipeline_state = build_pipeline_state(
            &device,
            &library,
            "quads",
            "quad_vertex",
            "quad_fragment",
            MTLPixelFormat::BGRA8Unorm,
        )?;
        let underlines_pipeline_state = build_pipeline_state(
            &device,
            &library,
            "underlines",
            "underline_vertex",
            "underline_fragment",
            MTLPixelFormat::BGRA8Unorm,
        )?;
        let monochrome_sprites_pipeline_state = build_pipeline_state(
            &device,
            &library,
            "monochrome_sprites",
            "monochrome_sprite_vertex",
            "monochrome_sprite_fragment",
            MTLPixelFormat::BGRA8Unorm,
        )?;
        let polychrome_sprites_pipeline_state = build_pipeline_state(
            &device,
            &library,
            "polychrome_sprites",
            "polychrome_sprite_vertex",
            "polychrome_sprite_fragment",
            MTLPixelFormat::BGRA8Unorm,
        )?;
        let surfaces_pipeline_state = build_pipeline_state(
            &device,
            &library,
            "surfaces",
            "surface_vertex",
            "surface_fragment",
            MTLPixelFormat::BGRA8Unorm,
        )?;

        let command_queue = device
            .newCommandQueue()
            .ok_or_else(|| anyhow::anyhow!("unable to create a Metal command queue"))?;
        let supports_shared_storage = cfg!(target_os = "ios") || is_apple_gpu;
        #[expect(
            clippy::arc_with_non_send_sync,
            reason = "PlatformWindow::sprite_atlas requires Arc, not Send or Sync"
        )]
        let sprite_atlas = Arc::new(MetalAtlas::new(device.clone(), supports_shared_storage));
        let mut cache = ptr::null_mut();
        // SAFETY: The output pointer is writable and both optional attribute dictionaries are absent.
        let status = unsafe {
            CVMetalTextureCache::create(None, None, &device, None, ptr::NonNull::from(&mut cache))
        };
        anyhow::ensure!(
            status == kCVReturnSuccess,
            "creating CoreVideo Metal texture cache: status {status}"
        );
        let cache = ptr::NonNull::new(cache)
            .ok_or_else(|| anyhow::anyhow!("retrieving CoreVideo Metal texture cache"))?;
        // SAFETY: The successful create call transfers ownership of this non-null cache.
        let core_video_texture_cache = unsafe { CFRetained::from_raw(cache) };

        Ok(Self {
            device,
            layer,
            presents_with_transaction: false,
            display_link_mode: DisplayLinkMode::Detached,
            is_apple_gpu,
            is_unified_memory,
            opaque,
            command_queue,
            paths_rasterization_pipeline_state,
            path_sprites_pipeline_state,
            shadows_pipeline_state,
            quads_pipeline_state,
            underlines_pipeline_state,
            monochrome_sprites_pipeline_state,
            polychrome_sprites_pipeline_state,
            surfaces_pipeline_state,
            unit_vertices,
            instance_buffer_pool,
            sprite_atlas,
            core_video_texture_cache,
            path_intermediate_texture: None,
            path_intermediate_msaa_texture: None,
            path_sample_count: PATH_SAMPLE_COUNT,
        })
    }

    pub fn layer(&self) -> Option<&CAMetalLayer> {
        self.layer.as_deref()
    }

    pub fn layer_ptr(&self) -> *mut CAMetalLayer {
        self.layer
            .as_ref()
            .map_or(ptr::null_mut(), |layer| Retained::as_ptr(layer).cast_mut())
    }

    pub fn sprite_atlas(&self) -> &Arc<MetalAtlas> {
        &self.sprite_atlas
    }

    /// Draw the next frame into `drawable` instead of asking the layer for one.
    ///
    /// A `CAMetalDisplayLink` hands out each frame's drawable early enough for
    /// the frame to reach the display on time; `nextDrawable` would instead
    /// wait for the compositor to release one. Pass `None` once the frame is
    /// done, so a drawable the frame did not use goes back to the layer.
    ///
    /// Returns the drawable this replaces: after a frame, `Some` means the
    /// frame neither drew nor presented.
    ///
    /// This is a setter rather than a guard scoped to one frame because the
    /// window releases its lock on the renderer while GPUI draws.
    pub fn set_frame_drawable(
        &mut self,
        drawable: Option<Retained<ProtocolObject<dyn CAMetalDrawable>>>,
    ) -> Option<Retained<ProtocolObject<dyn CAMetalDrawable>>> {
        match &mut self.display_link_mode {
            DisplayLinkMode::Attached { drawable: current } => mem::replace(current, drawable),
            DisplayLinkMode::Detached => {
                // Only a live link hands out drawables, and a link is detached
                // only after it has been invalidated.
                debug_assert!(
                    drawable.is_none(),
                    "received a display link drawable while detached"
                );
                None
            }
        }
    }

    /// Record whether a `CAMetalDisplayLink` drives the layer. While one
    /// does, frames are drawn only into the drawables it hands out.
    pub fn set_display_link_attached(&mut self, attached: bool) {
        self.display_link_mode = if attached {
            DisplayLinkMode::Attached { drawable: None }
        } else {
            DisplayLinkMode::Detached
        };
    }

    pub fn is_display_link_attached(&self) -> bool {
        matches!(self.display_link_mode, DisplayLinkMode::Attached { .. })
    }

    pub fn presents_with_transaction(&self) -> bool {
        self.presents_with_transaction
    }

    pub fn set_presents_with_transaction(&mut self, presents_with_transaction: bool) {
        self.presents_with_transaction = presents_with_transaction;
        if let Some(layer) = &self.layer {
            layer.setPresentsWithTransaction(presents_with_transaction);
        }
    }

    pub fn update_drawable_size(&mut self, size: Size<DevicePixels>) {
        if let Some(layer) = &self.layer {
            layer.setDrawableSize(CGSize::new(size.width.0 as f64, size.height.0 as f64));
        }
        self.update_path_intermediate_textures(size);
    }

    fn update_path_intermediate_textures(&mut self, size: Size<DevicePixels>) {
        // We are uncertain when this happens, but sometimes size can be 0 here. Most likely before
        // the layout pass on window creation. Zero-sized texture creation causes SIGABRT.
        // https://github.com/zed-industries/zed/issues/36229
        if size.width.0 <= 0 || size.height.0 <= 0 {
            self.path_intermediate_texture = None;
            self.path_intermediate_msaa_texture = None;
            return;
        }

        // Live resizing reports a new size on every frame while the window is
        // dragged, and the drawable size often stays the same between those
        // reports. Reallocating both full-size render targets each time would
        // churn many megabytes of GPU memory per frame.
        if let Some(texture) = &self.path_intermediate_texture
            && texture.width() == size.width.0 as usize
            && texture.height() == size.height.0 as usize
        {
            return;
        }

        let texture_descriptor = MTLTextureDescriptor::new();
        // SAFETY: Both dimensions are positive here and this descriptor creates a 2D texture.
        unsafe {
            texture_descriptor.setWidth(size.width.0 as usize);
            texture_descriptor.setHeight(size.height.0 as usize);
        }
        texture_descriptor.setPixelFormat(MTLPixelFormat::BGRA8Unorm);
        texture_descriptor.setStorageMode(MTLStorageMode::Private);
        texture_descriptor.setUsage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
        self.path_intermediate_texture = self.device.newTextureWithDescriptor(&texture_descriptor);

        if self.path_sample_count > 1 {
            // https://developer.apple.com/documentation/metal/choosing-a-resource-storage-mode-for-apple-gpus
            // Rendering MSAA textures are done in a single pass, so we can use memory-less storage on Apple Silicon
            let storage_mode = if self.is_apple_gpu {
                MTLStorageMode::Memoryless
            } else {
                MTLStorageMode::Private
            };

            let msaa_descriptor = MTLTextureDescriptor::new();
            // SAFETY: Same dimensions as the resolved target created above.
            unsafe {
                msaa_descriptor.setWidth(size.width.0 as usize);
                msaa_descriptor.setHeight(size.height.0 as usize);
            }
            msaa_descriptor.setPixelFormat(MTLPixelFormat::BGRA8Unorm);
            msaa_descriptor.setUsage(MTLTextureUsage::RenderTarget);
            msaa_descriptor.setTextureType(objc2_metal::MTLTextureType::Type2DMultisample);
            msaa_descriptor.setStorageMode(storage_mode);
            // SAFETY: The sample count is a compile-time constant that every Metal device supports.
            unsafe {
                msaa_descriptor.setSampleCount(self.path_sample_count as usize);
            }
            self.path_intermediate_msaa_texture =
                self.device.newTextureWithDescriptor(&msaa_descriptor);
        } else {
            self.path_intermediate_msaa_texture = None;
        }
    }

    pub fn update_transparency(&mut self, transparent: bool) {
        self.opaque = !transparent;
        if let Some(layer) = &self.layer {
            layer.setOpaque(!transparent);
        }
    }

    pub fn destroy(&self) {
        // nothing to do
    }

    pub fn draw(&mut self, scene: &Scene) {
        let Some(layer) = self.layer.clone() else {
            log::error!("draw() called on headless renderer - use render_scene_to_image() instead");
            return;
        };
        let viewport_size = layer.drawableSize();
        let viewport_size: Size<DevicePixels> = size(
            (viewport_size.width.ceil() as i32).into(),
            (viewport_size.height.ceil() as i32).into(),
        );
        let drawable = match &mut self.display_link_mode {
            DisplayLinkMode::Attached { drawable } => {
                let Some(drawable) = drawable.take() else {
                    log::error!("skipped a frame drawn outside its display link's update");
                    return;
                };
                drawable
            }
            DisplayLinkMode::Detached => {
                let Some(drawable) = layer.nextDrawable() else {
                    log::error!(
                        "failed to retrieve next drawable, drawable size: {:?}",
                        viewport_size
                    );
                    return;
                };
                drawable
            }
        };

        let command_buffer = match self.encode_scene(scene, |renderer, instance_buffer| {
            renderer.draw_primitives(scene, instance_buffer, &drawable.texture(), viewport_size)
        }) {
            Ok(command_buffer) => command_buffer,
            Err(error) => {
                log::error!("failed to render: {error:#}");
                return;
            }
        };

        if self.presents_with_transaction {
            command_buffer.commit();
            command_buffer.waitUntilScheduled();
            drawable.present();
        } else {
            command_buffer.presentDrawable(ProtocolObject::from_ref(&*drawable));
            command_buffer.commit();
        }
    }

    /// Encodes `scene` into a command buffer, growing the shared instance buffer
    /// until the whole scene fits, and returns the buffer to the pool once the
    /// GPU is done with it.
    fn encode_scene(
        &mut self,
        scene: &Scene,
        mut encode: impl FnMut(
            &mut Self,
            &mut InstanceBuffer,
        ) -> Result<Retained<ProtocolObject<dyn MTLCommandBuffer>>>,
    ) -> Result<Retained<ProtocolObject<dyn MTLCommandBuffer>>> {
        let required_bytes = required_instance_bytes(scene);
        loop {
            let mut instance_buffer = {
                let mut pool = self.instance_buffer_pool.lock();
                pool.acquire(&self.device, self.is_unified_memory, required_bytes)?
            };

            match encode(self, &mut instance_buffer) {
                Ok(command_buffer) => {
                    let instance_buffer_pool = self.instance_buffer_pool.clone();
                    let instance_buffer = Cell::new(Some(instance_buffer));
                    let block = RcBlock::new(
                        move |_: ptr::NonNull<ProtocolObject<dyn MTLCommandBuffer>>| {
                            if let Some(instance_buffer) = instance_buffer.take() {
                                instance_buffer_pool.lock().release(instance_buffer);
                            }
                        },
                    );
                    // SAFETY: Metal copies the live block and invokes it once after
                    // this frame completes. Its captures own the frame resources and
                    // use the pool mutex when returning the buffer.
                    unsafe {
                        command_buffer.addCompletedHandler(RcBlock::as_ptr(&block));
                    }
                    return Ok(command_buffer);
                }
                Err(error) => {
                    let mut pool = self.instance_buffer_pool.lock();
                    let buffer_size = pool.buffer_size;
                    if buffer_size >= MAX_INSTANCE_BUFFER_SIZE {
                        anyhow::bail!(
                            "instance buffer grew to {buffer_size} bytes and the scene still did not fit: {error:#}"
                        );
                    }
                    log::error!(
                        "failed to render: {error:#}. retrying with larger instance buffer size"
                    );
                    pool.reset((buffer_size * 2).min(MAX_INSTANCE_BUFFER_SIZE));
                    log::info!("increased instance buffer size to {}", pool.buffer_size);
                }
            }
        }
    }

    /// Renders the scene to a texture and returns the pixel data as an RGBA image.
    /// This does not present the frame to screen - useful for visual testing
    /// where we want to capture what would be rendered without displaying it.
    ///
    /// Note: This requires a layer-backed renderer. For headless rendering,
    /// use `render_scene_to_image()` instead.
    #[cfg(any(test, feature = "test-support"))]
    pub fn render_to_image(&mut self, scene: &Scene) -> Result<RgbaImage> {
        let layer = self
            .layer
            .clone()
            .ok_or_else(|| anyhow::anyhow!("render_to_image requires a layer-backed renderer"))?;
        let viewport_size = layer.drawableSize();
        let viewport_size: Size<DevicePixels> = size(
            (viewport_size.width.ceil() as i32).into(),
            (viewport_size.height.ceil() as i32).into(),
        );
        // The display link owns the layer's drawables, so render into a
        // texture we own instead of taking one from the layer.
        self.render_scene_to_image(scene, viewport_size)
    }

    /// Renders a scene to an image without requiring a window or CAMetalLayer.
    ///
    /// This is the primary method for headless rendering. It creates an offscreen
    /// texture, renders the scene to it, and returns the pixel data as an RGBA image.
    #[cfg(any(test, feature = "test-support"))]
    pub fn render_scene_to_image(
        &mut self,
        scene: &Scene,
        size: Size<DevicePixels>,
    ) -> Result<RgbaImage> {
        if size.width.0 <= 0 || size.height.0 <= 0 {
            anyhow::bail!("Invalid size for render_scene_to_image: {:?}", size);
        }

        // Headless callers do not have a Cocoa event-loop pool to release
        // autoreleased command buffers and render-pass descriptors.
        objc2::rc::autoreleasepool(|_| {
            // Update path intermediate textures for this size
            self.update_path_intermediate_textures(size);

            // Create an offscreen texture as render target
            let texture_descriptor = MTLTextureDescriptor::new();
            // SAFETY: The dimensions are positive and this descriptor creates a 2D texture.
            unsafe {
                texture_descriptor.setWidth(size.width.0 as usize);
                texture_descriptor.setHeight(size.height.0 as usize);
            }
            texture_descriptor.setPixelFormat(MTLPixelFormat::BGRA8Unorm);
            texture_descriptor
                .setUsage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
            // Like the atlas, only Apple GPUs can create shared textures on macOS;
            // Intel Macs cannot, even with unified memory. iOS has no managed storage.
            let uses_shared_storage = cfg!(target_os = "ios") || self.is_apple_gpu;
            texture_descriptor.setStorageMode(if uses_shared_storage {
                MTLStorageMode::Shared
            } else {
                MTLStorageMode::Managed
            });
            let target_texture = self
                .device
                .newTextureWithDescriptor(&texture_descriptor)
                .ok_or_else(|| anyhow::anyhow!("allocating offscreen render target"))?;

            let command_buffer = self.encode_scene(scene, |renderer, instance_buffer| {
                renderer.draw_primitives_to_texture(scene, instance_buffer, &target_texture, size)
            })?;

            // Managed textures require an explicit blit synchronize before the CPU
            // can read back the rendered data. Without this, get_bytes returns
            // stale zeros.
            if !uses_shared_storage {
                let blit = command_buffer
                    .blitCommandEncoder()
                    .ok_or_else(|| anyhow::anyhow!("creating blit command encoder"))?;
                // The texture is a live resource created by this renderer.
                blit.synchronizeResource(ProtocolObject::from_ref(&*target_texture));
                blit.endEncoding();
            }

            command_buffer.commit();
            command_buffer.waitUntilCompleted();

            read_texture_as_rgba(&target_texture)
        })
    }

    fn draw_primitives(
        &mut self,
        scene: &Scene,
        instance_buffer: &mut InstanceBuffer,
        texture: &ProtocolObject<dyn MTLTexture>,
        viewport_size: Size<DevicePixels>,
    ) -> Result<Retained<ProtocolObject<dyn MTLCommandBuffer>>> {
        self.draw_primitives_to_texture(scene, instance_buffer, texture, viewport_size)
    }

    fn draw_primitives_to_texture(
        &mut self,
        scene: &Scene,
        instance_buffer: &mut InstanceBuffer,
        texture: &ProtocolObject<dyn MTLTexture>,
        viewport_size: Size<DevicePixels>,
    ) -> Result<Retained<ProtocolObject<dyn MTLCommandBuffer>>> {
        let command_queue = self.command_queue.clone();
        let command_buffer = command_queue
            .commandBuffer()
            .ok_or_else(|| anyhow::anyhow!("creating command buffer"))?;
        let alpha = if self.opaque { 1. } else { 0. };
        let mut instance_offset = 0;

        let Some(mut command_encoder) = new_command_encoder_for_texture(
            &command_buffer,
            texture,
            viewport_size,
            |color_attachment| {
                color_attachment.setLoadAction(MTLLoadAction::Clear);
                color_attachment.setClearColor(MTLClearColor {
                    red: 0.,
                    green: 0.,
                    blue: 0.,
                    alpha,
                });
            },
        ) else {
            anyhow::bail!("Metal did not provide a color attachment for the drawable texture");
        };

        for batch in scene.batches() {
            let ok = match batch {
                PrimitiveBatch::Shadows(range) => self.draw_shadows(
                    &scene.shadows[range],
                    instance_buffer,
                    &mut instance_offset,
                    viewport_size,
                    &command_encoder,
                ),
                PrimitiveBatch::Quads(range) => self.draw_quads(
                    &scene.quads[range],
                    instance_buffer,
                    &mut instance_offset,
                    viewport_size,
                    &command_encoder,
                ),
                PrimitiveBatch::Paths(range) => {
                    let paths = &scene.paths[range];
                    command_encoder.endEncoding();

                    let did_draw = self.draw_paths_to_intermediate(
                        paths,
                        instance_buffer,
                        &mut instance_offset,
                        viewport_size,
                        &command_buffer,
                    );

                    let Some(encoder) = new_command_encoder_for_texture(
                        &command_buffer,
                        texture,
                        viewport_size,
                        |color_attachment| {
                            color_attachment.setLoadAction(MTLLoadAction::Load);
                        },
                    ) else {
                        anyhow::bail!(
                            "Metal did not provide a color attachment for the drawable texture"
                        );
                    };
                    command_encoder = encoder;

                    if did_draw {
                        self.draw_paths_from_intermediate(
                            paths,
                            instance_buffer,
                            &mut instance_offset,
                            viewport_size,
                            &command_encoder,
                        )
                    } else {
                        false
                    }
                }
                PrimitiveBatch::Underlines(range) => self.draw_underlines(
                    &scene.underlines[range],
                    instance_buffer,
                    &mut instance_offset,
                    viewport_size,
                    &command_encoder,
                ),
                PrimitiveBatch::MonochromeSprites { texture_id, range } => self
                    .draw_monochrome_sprites(
                        texture_id,
                        &scene.monochrome_sprites[range],
                        instance_buffer,
                        &mut instance_offset,
                        viewport_size,
                        &command_encoder,
                    ),
                PrimitiveBatch::PolychromeSprites { texture_id, range } => self
                    .draw_polychrome_sprites(
                        texture_id,
                        &scene.polychrome_sprites[range],
                        instance_buffer,
                        &mut instance_offset,
                        viewport_size,
                        &command_encoder,
                    ),
                PrimitiveBatch::Surfaces(range) => self.draw_surfaces(
                    &scene.surfaces[range],
                    instance_buffer,
                    &mut instance_offset,
                    viewport_size,
                    &command_encoder,
                ),
                PrimitiveBatch::SubpixelSprites { .. } => {
                    log::error!("subpixel sprites are not supported by the Metal renderer");
                    true
                }
            };
            if !ok {
                command_encoder.endEncoding();
                anyhow::bail!(
                    "scene too large: {} paths, {} shadows, {} quads, {} underlines, {} mono, {} poly, {} surfaces",
                    scene.paths.len(),
                    scene.shadows.len(),
                    scene.quads.len(),
                    scene.underlines.len(),
                    scene.monochrome_sprites.len(),
                    scene.polychrome_sprites.len(),
                    scene.surfaces.len(),
                );
            }
        }

        command_encoder.endEncoding();

        if !self.is_unified_memory {
            // Sync the instance buffer to the GPU
            instance_buffer.metal_buffer.didModifyRange(NSRange {
                location: 0,
                length: instance_offset,
            });
        }

        Ok(command_buffer)
    }

    fn draw_paths_to_intermediate(
        &self,
        paths: &[Path<ScaledPixels>],
        instance_buffer: &mut InstanceBuffer,
        instance_offset: &mut usize,
        viewport_size: Size<DevicePixels>,
        command_buffer: &ProtocolObject<dyn MTLCommandBuffer>,
    ) -> bool {
        if paths.is_empty() {
            return true;
        }
        let Some(intermediate_texture) = &self.path_intermediate_texture else {
            return false;
        };

        // Write the vertices before creating the render pass so an overflowing
        // scene bails out without leaving an encoder behind.
        let vertex_count: usize = paths.iter().map(|path| path.vertices.len()).sum();
        let Some(vertices_offset) = instance_buffer.write(
            instance_offset,
            vertex_count,
            paths.iter().flat_map(|path| {
                let color = path.color;
                let bounds = path.bounds.intersect(&path.content_mask.bounds);
                path.vertices
                    .iter()
                    .map(move |vertex| PathRasterizationVertex {
                        xy_position: vertex.xy_position,
                        st_position: vertex.st_position,
                        color,
                        bounds,
                    })
            }),
        ) else {
            return false;
        };

        let render_pass_descriptor = MTLRenderPassDescriptor::new();
        // SAFETY: Index zero is within the render pass's color attachment array.
        let color_attachment = unsafe {
            render_pass_descriptor
                .colorAttachments()
                .objectAtIndexedSubscript(0)
        };
        color_attachment.setLoadAction(MTLLoadAction::Clear);
        color_attachment.setClearColor(MTLClearColor {
            red: 0.,
            green: 0.,
            blue: 0.,
            alpha: 0.,
        });

        if let Some(msaa_texture) = &self.path_intermediate_msaa_texture {
            color_attachment.setTexture(Some(msaa_texture));
            color_attachment.setResolveTexture(Some(intermediate_texture));
            color_attachment.setStoreAction(MTLStoreAction::MultisampleResolve);
        } else {
            color_attachment.setTexture(Some(intermediate_texture));
            color_attachment.setStoreAction(MTLStoreAction::Store);
        }

        let Some(command_encoder) =
            command_buffer.renderCommandEncoderWithDescriptor(&render_pass_descriptor)
        else {
            log::error!("Metal did not provide a render command encoder for the path texture");
            return false;
        };
        command_encoder.setRenderPipelineState(&self.paths_rasterization_pipeline_state);
        // SAFETY: The scene's vertices match the shader layout and slot.
        // Metal retains the buffer, and the caller publishes the writes.
        unsafe {
            command_encoder.setVertexBuffer_offset_atIndex(
                Some(&instance_buffer.metal_buffer),
                vertices_offset,
                PathRasterizationInputIndex::Vertices as usize,
            );
            command_encoder.setVertexBytes_length_atIndex(
                ptr::NonNull::from(&viewport_size).cast(),
                mem::size_of_val(&viewport_size),
                PathRasterizationInputIndex::ViewportSize as usize,
            );
            command_encoder.setFragmentBuffer_offset_atIndex(
                Some(&instance_buffer.metal_buffer),
                vertices_offset,
                PathRasterizationInputIndex::Vertices as usize,
            );
            command_encoder.drawPrimitives_vertexStart_vertexCount(
                MTLPrimitiveType::Triangle,
                0,
                vertex_count,
            );
        }

        command_encoder.endEncoding();
        true
    }

    fn draw_shadows(
        &self,
        shadows: &[Shadow],
        instance_buffer: &mut InstanceBuffer,
        instance_offset: &mut usize,
        viewport_size: Size<DevicePixels>,
        command_encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
    ) -> bool {
        self.draw_instanced(
            command_encoder,
            &self.shadows_pipeline_state,
            instance_buffer,
            instance_offset,
            viewport_size,
            shadows,
            |_| {},
        )
    }

    fn draw_quads(
        &self,
        quads: &[Quad],
        instance_buffer: &mut InstanceBuffer,
        instance_offset: &mut usize,
        viewport_size: Size<DevicePixels>,
        command_encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
    ) -> bool {
        self.draw_instanced(
            command_encoder,
            &self.quads_pipeline_state,
            instance_buffer,
            instance_offset,
            viewport_size,
            quads,
            |_| {},
        )
    }

    fn draw_underlines(
        &self,
        underlines: &[Underline],
        instance_buffer: &mut InstanceBuffer,
        instance_offset: &mut usize,
        viewport_size: Size<DevicePixels>,
        command_encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
    ) -> bool {
        self.draw_instanced(
            command_encoder,
            &self.underlines_pipeline_state,
            instance_buffer,
            instance_offset,
            viewport_size,
            underlines,
            |_| {},
        )
    }

    fn draw_monochrome_sprites(
        &self,
        texture_id: AtlasTextureId,
        sprites: &[MonochromeSprite],
        instance_buffer: &mut InstanceBuffer,
        instance_offset: &mut usize,
        viewport_size: Size<DevicePixels>,
        command_encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
    ) -> bool {
        let Some(texture) = self.sprite_atlas.metal_texture(texture_id) else {
            return true;
        };
        let texture_size = size(
            DevicePixels(texture.width() as i32),
            DevicePixels(texture.height() as i32),
        );
        self.draw_instanced(
            command_encoder,
            &self.monochrome_sprites_pipeline_state,
            instance_buffer,
            instance_offset,
            viewport_size,
            sprites,
            |encoder| {
                // SAFETY: The size matches the shader layout and slot, and the
                // texture is retained by the atlas for the duration of the frame.
                unsafe {
                    encoder.setVertexBytes_length_atIndex(
                        ptr::NonNull::from(&texture_size).cast(),
                        mem::size_of_val(&texture_size),
                        SpriteInputIndex::AtlasTextureSize as usize,
                    );
                    encoder.setFragmentTexture_atIndex(
                        Some(&texture),
                        SpriteInputIndex::AtlasTexture as usize,
                    );
                }
            },
        )
    }

    fn draw_polychrome_sprites(
        &self,
        texture_id: AtlasTextureId,
        sprites: &[PolychromeSprite],
        instance_buffer: &mut InstanceBuffer,
        instance_offset: &mut usize,
        viewport_size: Size<DevicePixels>,
        command_encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
    ) -> bool {
        let Some(texture) = self.sprite_atlas.metal_texture(texture_id) else {
            return true;
        };
        let texture_size = size(
            DevicePixels(texture.width() as i32),
            DevicePixels(texture.height() as i32),
        );
        self.draw_instanced(
            command_encoder,
            &self.polychrome_sprites_pipeline_state,
            instance_buffer,
            instance_offset,
            viewport_size,
            sprites,
            |encoder| {
                // SAFETY: The size matches the shader layout and slot, and the
                // texture is retained by the atlas for the duration of the frame.
                unsafe {
                    encoder.setVertexBytes_length_atIndex(
                        ptr::NonNull::from(&texture_size).cast(),
                        mem::size_of_val(&texture_size),
                        SpriteInputIndex::AtlasTextureSize as usize,
                    );
                    encoder.setFragmentTexture_atIndex(
                        Some(&texture),
                        SpriteInputIndex::AtlasTexture as usize,
                    );
                }
            },
        )
    }

    fn draw_paths_from_intermediate(
        &self,
        paths: &[Path<ScaledPixels>],
        instance_buffer: &mut InstanceBuffer,
        instance_offset: &mut usize,
        viewport_size: Size<DevicePixels>,
        command_encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
    ) -> bool {
        let Some(first_path) = paths.first() else {
            return true;
        };
        let Some(intermediate_texture) = &self.path_intermediate_texture else {
            return false;
        };

        // When copying paths from the intermediate texture to the drawable,
        // each pixel must only be copied once, in case of transparent paths.
        //
        // If all paths have the same draw order, then their bounds are all
        // disjoint, so we can copy each path's bounds individually. If this
        // batch combines different draw orders, we perform a single copy
        // for a minimal spanning rect.
        let (sprite_count, sprites_offset) = if paths
            .last()
            .is_some_and(|path| path.order == first_path.order)
        {
            let offset = instance_buffer.write(
                instance_offset,
                paths.len(),
                paths.iter().map(|path| PathSprite {
                    bounds: path.clipped_bounds(),
                }),
            );
            (paths.len(), offset)
        } else {
            let mut bounds = first_path.clipped_bounds();
            for path in paths.iter().skip(1) {
                bounds = bounds.union(&path.clipped_bounds());
            }
            let offset =
                instance_buffer.write(instance_offset, 1, std::iter::once(PathSprite { bounds }));
            (1, offset)
        };

        let Some(sprites_offset) = sprites_offset else {
            return false;
        };

        self.bind_instanced_state(
            command_encoder,
            &self.path_sprites_pipeline_state,
            instance_buffer,
            sprites_offset,
            viewport_size,
        );
        // SAFETY: The intermediate texture matches the path sprite shader's slot.
        unsafe {
            command_encoder.setFragmentTexture_atIndex(
                Some(intermediate_texture),
                SpriteInputIndex::AtlasTexture as usize,
            );
            command_encoder.drawPrimitives_vertexStart_vertexCount_instanceCount(
                MTLPrimitiveType::Triangle,
                0,
                6,
                sprite_count,
            );
        }

        true
    }

    /// Binds the state every instanced pipeline in `shaders.metal` shares: the
    /// unit quad at buffer slot 0, the per-instance data at slot 1 and the
    /// viewport size at slot 2.
    fn bind_instanced_state(
        &self,
        command_encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
        pipeline_state: &ProtocolObject<dyn MTLRenderPipelineState>,
        instance_buffer: &InstanceBuffer,
        instance_start: usize,
        viewport_size: Size<DevicePixels>,
    ) {
        command_encoder.setRenderPipelineState(pipeline_state);
        // SAFETY: The bound buffer, offset and copied size match every
        // instanced shader's layout, and Metal retains the buffer.
        unsafe {
            command_encoder.setVertexBuffer_offset_atIndex(
                Some(&self.unit_vertices),
                0,
                UNIT_VERTICES_SLOT,
            );
            command_encoder.setVertexBuffer_offset_atIndex(
                Some(&instance_buffer.metal_buffer),
                instance_start,
                INSTANCES_SLOT,
            );
            command_encoder.setFragmentBuffer_offset_atIndex(
                Some(&instance_buffer.metal_buffer),
                instance_start,
                INSTANCES_SLOT,
            );
            command_encoder.setVertexBytes_length_atIndex(
                ptr::NonNull::from(&viewport_size).cast(),
                mem::size_of_val(&viewport_size),
                VIEWPORT_SIZE_SLOT,
            );
        }
    }

    /// Writes `instances` into the shared instance buffer and draws one unit
    /// quad per instance, letting `configure` bind anything else the pipeline
    /// needs. Returns `false` when the instances do not fit in the buffer.
    fn draw_instanced<T: Copy>(
        &self,
        command_encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
        pipeline_state: &ProtocolObject<dyn MTLRenderPipelineState>,
        instance_buffer: &mut InstanceBuffer,
        instance_offset: &mut usize,
        viewport_size: Size<DevicePixels>,
        instances: &[T],
        configure: impl FnOnce(&ProtocolObject<dyn MTLRenderCommandEncoder>),
    ) -> bool {
        if instances.is_empty() {
            return true;
        }

        let Some(instance_start) = instance_buffer.write_slice(instance_offset, instances) else {
            return false;
        };

        self.bind_instanced_state(
            command_encoder,
            pipeline_state,
            instance_buffer,
            instance_start,
            viewport_size,
        );
        configure(command_encoder);
        // SAFETY: One unit quad is drawn per uploaded instance.
        unsafe {
            command_encoder.drawPrimitives_vertexStart_vertexCount_instanceCount(
                MTLPrimitiveType::Triangle,
                0,
                6,
                instances.len(),
            );
        }
        true
    }

    fn draw_surfaces(
        &mut self,
        surfaces: &[PaintSurface],
        instance_buffer: &mut InstanceBuffer,
        instance_offset: &mut usize,
        viewport_size: Size<DevicePixels>,
        command_encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
    ) -> bool {
        use core_foundation::base::TCFType as _;

        for surface in surfaces {
            // The zero-copy surface wrapper is CoreVideo's `CVPixelBuffer`; the
            // generated bindings below take the same reference type.
            let image_buffer: &CVPixelBuffer =
                // SAFETY: The wrapper owns a live buffer and both declarations name the
                // same CoreVideo type.
                unsafe { &*surface.image_buffer.as_concrete_TypeRef().cast() };
            let pixel_format = CVPixelBufferGetPixelFormatType(image_buffer);
            if pixel_format != kCVPixelFormatType_420YpCbCr8BiPlanarFullRange {
                log::error!("skipping surface with unsupported pixel format {pixel_format:#x}");
                continue;
            }

            let texture_size = size(
                DevicePixels::from(CVPixelBufferGetWidth(image_buffer) as i32),
                DevicePixels::from(CVPixelBufferGetHeight(image_buffer) as i32),
            );

            let plane_texture = |plane, format| -> Result<CFRetained<CVMetalTexture>> {
                let width = CVPixelBufferGetWidthOfPlane(image_buffer, plane);
                let height = CVPixelBufferGetHeightOfPlane(image_buffer, plane);
                let mut texture = ptr::null_mut();
                // SAFETY: The buffer has two planes matching these formats and dimensions, and the output pointer is writable.
                let status = unsafe {
                    CVMetalTextureCache::create_texture_from_image(
                        None,
                        &self.core_video_texture_cache,
                        image_buffer,
                        None,
                        format,
                        width,
                        height,
                        plane,
                        ptr::NonNull::from(&mut texture),
                    )
                };
                anyhow::ensure!(
                    status == kCVReturnSuccess,
                    "creating CoreVideo Metal texture for plane {plane}: status {status}"
                );
                let texture = ptr::NonNull::new(texture).ok_or_else(|| {
                    anyhow::anyhow!("retrieving CoreVideo Metal texture for plane {plane}")
                })?;
                // SAFETY: The successful create call transfers ownership of this non-null texture.
                Ok(unsafe { CFRetained::from_raw(texture) })
            };
            let y_texture = match plane_texture(0, MTLPixelFormat::R8Unorm) {
                Ok(texture) => texture,
                Err(error) => {
                    log::error!(
                        "failed to wrap the luma plane of a surface in a texture: {error:#}"
                    );
                    continue;
                }
            };
            let cb_cr_texture = match plane_texture(1, MTLPixelFormat::RG8Unorm) {
                Ok(texture) => texture,
                Err(error) => {
                    log::error!(
                        "failed to wrap the chroma plane of a surface in a texture: {error:#}"
                    );
                    continue;
                }
            };

            let Some(instance_start) = instance_buffer.write(
                instance_offset,
                1,
                std::iter::once(SurfaceBounds {
                    bounds: surface.bounds,
                    content_mask: surface.content_mask,
                }),
            ) else {
                return false;
            };

            self.bind_instanced_state(
                command_encoder,
                &self.surfaces_pipeline_state,
                instance_buffer,
                instance_start,
                viewport_size,
            );
            // SAFETY: The two planes and the copied size match the surface
            // shader's texture slots. The CoreVideo wrappers stay alive until
            // the command buffer completes.
            unsafe {
                command_encoder.setVertexBytes_length_atIndex(
                    ptr::NonNull::from(&texture_size).cast(),
                    mem::size_of_val(&texture_size),
                    SurfaceInputIndex::TextureSize as usize,
                );
                command_encoder.setFragmentTexture_atIndex(
                    CVMetalTextureGetTexture(&y_texture).as_deref(),
                    SurfaceInputIndex::YTexture as usize,
                );
                command_encoder.setFragmentTexture_atIndex(
                    CVMetalTextureGetTexture(&cb_cr_texture).as_deref(),
                    SurfaceInputIndex::CbCrTexture as usize,
                );
                command_encoder.drawPrimitives_vertexStart_vertexCount(
                    MTLPrimitiveType::Triangle,
                    0,
                    6,
                );
            }
        }
        true
    }
}

fn new_command_encoder_for_texture(
    command_buffer: &ProtocolObject<dyn MTLCommandBuffer>,
    texture: &ProtocolObject<dyn MTLTexture>,
    viewport_size: Size<DevicePixels>,
    configure_color_attachment: impl Fn(&MTLRenderPassColorAttachmentDescriptor),
) -> Option<Retained<ProtocolObject<dyn MTLRenderCommandEncoder>>> {
    let render_pass_descriptor = MTLRenderPassDescriptor::new();
    // SAFETY: Index zero is within the render pass's color attachment array.
    let color_attachment = unsafe {
        render_pass_descriptor
            .colorAttachments()
            .objectAtIndexedSubscript(0)
    };
    color_attachment.setTexture(Some(texture));
    color_attachment.setStoreAction(MTLStoreAction::Store);
    configure_color_attachment(&color_attachment);

    let command_encoder =
        command_buffer.renderCommandEncoderWithDescriptor(&render_pass_descriptor)?;
    command_encoder.setViewport(MTLViewport {
        originX: 0.0,
        originY: 0.0,
        width: i32::from(viewport_size.width) as f64,
        height: i32::from(viewport_size.height) as f64,
        znear: 0.0,
        zfar: 1.0,
    });
    Some(command_encoder)
}

fn build_pipeline_state(
    device: &ProtocolObject<dyn MTLDevice>,
    library: &ProtocolObject<dyn MTLLibrary>,
    label: &str,
    vertex_fn_name: &str,
    fragment_fn_name: &str,
    pixel_format: MTLPixelFormat,
) -> Result<Retained<ProtocolObject<dyn MTLRenderPipelineState>>> {
    let vertex_fn = library
        .newFunctionWithName(&NSString::from_str(vertex_fn_name))
        .ok_or_else(|| anyhow::anyhow!("locating vertex function {vertex_fn_name}"))?;
    let fragment_fn = library
        .newFunctionWithName(&NSString::from_str(fragment_fn_name))
        .ok_or_else(|| anyhow::anyhow!("locating fragment function {fragment_fn_name}"))?;

    let descriptor = MTLRenderPipelineDescriptor::new();
    descriptor.setLabel(Some(&NSString::from_str(label)));
    descriptor.setVertexFunction(Some(vertex_fn.as_ref()));
    descriptor.setFragmentFunction(Some(fragment_fn.as_ref()));
    // SAFETY: Index zero is within the pipeline's color attachment array.
    let color_attachment = unsafe { descriptor.colorAttachments().objectAtIndexedSubscript(0) };
    color_attachment.setPixelFormat(pixel_format);
    color_attachment.setBlendingEnabled(true);
    color_attachment.setRgbBlendOperation(MTLBlendOperation::Add);
    color_attachment.setAlphaBlendOperation(MTLBlendOperation::Add);
    color_attachment.setSourceRGBBlendFactor(MTLBlendFactor::SourceAlpha);
    color_attachment.setSourceAlphaBlendFactor(MTLBlendFactor::One);
    color_attachment.setDestinationRGBBlendFactor(MTLBlendFactor::OneMinusSourceAlpha);
    color_attachment.setDestinationAlphaBlendFactor(MTLBlendFactor::OneMinusSourceAlpha);

    device
        .newRenderPipelineStateWithDescriptor_error(&descriptor)
        .map_err(|error| anyhow::anyhow!("creating {label} render pipeline: {error}"))
}

fn build_path_sprite_pipeline_state(
    device: &ProtocolObject<dyn MTLDevice>,
    library: &ProtocolObject<dyn MTLLibrary>,
    label: &str,
    vertex_fn_name: &str,
    fragment_fn_name: &str,
    pixel_format: MTLPixelFormat,
) -> Result<Retained<ProtocolObject<dyn MTLRenderPipelineState>>> {
    let vertex_fn = library
        .newFunctionWithName(&NSString::from_str(vertex_fn_name))
        .ok_or_else(|| anyhow::anyhow!("locating vertex function {vertex_fn_name}"))?;
    let fragment_fn = library
        .newFunctionWithName(&NSString::from_str(fragment_fn_name))
        .ok_or_else(|| anyhow::anyhow!("locating fragment function {fragment_fn_name}"))?;

    let descriptor = MTLRenderPipelineDescriptor::new();
    descriptor.setLabel(Some(&NSString::from_str(label)));
    descriptor.setVertexFunction(Some(vertex_fn.as_ref()));
    descriptor.setFragmentFunction(Some(fragment_fn.as_ref()));
    // SAFETY: Index zero is within the pipeline's color attachment array.
    let color_attachment = unsafe { descriptor.colorAttachments().objectAtIndexedSubscript(0) };
    color_attachment.setPixelFormat(pixel_format);
    color_attachment.setBlendingEnabled(true);
    color_attachment.setRgbBlendOperation(MTLBlendOperation::Add);
    color_attachment.setAlphaBlendOperation(MTLBlendOperation::Add);
    color_attachment.setSourceRGBBlendFactor(MTLBlendFactor::One);
    color_attachment.setSourceAlphaBlendFactor(MTLBlendFactor::One);
    color_attachment.setDestinationRGBBlendFactor(MTLBlendFactor::OneMinusSourceAlpha);
    color_attachment.setDestinationAlphaBlendFactor(MTLBlendFactor::OneMinusSourceAlpha);

    device
        .newRenderPipelineStateWithDescriptor_error(&descriptor)
        .map_err(|error| anyhow::anyhow!("creating {label} render pipeline: {error}"))
}

fn build_path_rasterization_pipeline_state(
    device: &ProtocolObject<dyn MTLDevice>,
    library: &ProtocolObject<dyn MTLLibrary>,
    label: &str,
    vertex_fn_name: &str,
    fragment_fn_name: &str,
    pixel_format: MTLPixelFormat,
    path_sample_count: u32,
) -> Result<Retained<ProtocolObject<dyn MTLRenderPipelineState>>> {
    let vertex_fn = library
        .newFunctionWithName(&NSString::from_str(vertex_fn_name))
        .ok_or_else(|| anyhow::anyhow!("locating vertex function {vertex_fn_name}"))?;
    let fragment_fn = library
        .newFunctionWithName(&NSString::from_str(fragment_fn_name))
        .ok_or_else(|| anyhow::anyhow!("locating fragment function {fragment_fn_name}"))?;

    let descriptor = MTLRenderPipelineDescriptor::new();
    descriptor.setLabel(Some(&NSString::from_str(label)));
    descriptor.setVertexFunction(Some(vertex_fn.as_ref()));
    descriptor.setFragmentFunction(Some(fragment_fn.as_ref()));
    if path_sample_count > 1 {
        descriptor.setRasterSampleCount(path_sample_count as usize);
        descriptor.setAlphaToCoverageEnabled(false);
    }
    // SAFETY: Index zero is within the pipeline's color attachment array.
    let color_attachment = unsafe { descriptor.colorAttachments().objectAtIndexedSubscript(0) };
    color_attachment.setPixelFormat(pixel_format);
    color_attachment.setBlendingEnabled(true);
    color_attachment.setRgbBlendOperation(MTLBlendOperation::Add);
    color_attachment.setAlphaBlendOperation(MTLBlendOperation::Add);
    color_attachment.setSourceRGBBlendFactor(MTLBlendFactor::One);
    color_attachment.setSourceAlphaBlendFactor(MTLBlendFactor::One);
    color_attachment.setDestinationRGBBlendFactor(MTLBlendFactor::OneMinusSourceAlpha);
    color_attachment.setDestinationAlphaBlendFactor(MTLBlendFactor::OneMinusSourceAlpha);

    device
        .newRenderPipelineStateWithDescriptor_error(&descriptor)
        .map_err(|error| anyhow::anyhow!("creating {label} render pipeline: {error}"))
}

// Align to multiples of 256 make Metal happy.
fn align_offset(offset: usize) -> usize {
    offset.next_multiple_of(INSTANCE_BUFFER_ALIGNMENT)
}

/// Upper bound on the bytes `draw_primitives_to_texture` writes into the
/// instance buffer for `scene`, so the buffer can be sized before encoding
/// starts instead of failing halfway through and re-encoding the whole scene.
fn required_instance_bytes(scene: &Scene) -> usize {
    struct Total(usize);

    impl Total {
        /// Starts a batch, which the encoder writes at an aligned offset.
        fn start_batch(&mut self) {
            self.0 = align_offset(self.0);
        }

        fn add<T>(&mut self, count: usize) {
            self.0 = self
                .0
                .saturating_add(mem::size_of::<T>().saturating_mul(count));
        }

        /// Accounts for instances written one at a time, each at its own
        /// aligned offset, as the surfaces batch does.
        fn add_aligned<T>(&mut self, count: usize) {
            let stride = align_offset(mem::size_of::<T>().max(1));
            self.0 = self.0.saturating_add(stride.saturating_mul(count));
        }
    }

    let mut total = Total(0);
    for batch in scene.batches() {
        total.start_batch();
        match batch {
            PrimitiveBatch::Shadows(range) => total.add::<Shadow>(range.len()),
            PrimitiveBatch::Quads(range) => total.add::<Quad>(range.len()),
            PrimitiveBatch::Underlines(range) => total.add::<Underline>(range.len()),
            PrimitiveBatch::MonochromeSprites { range, .. } => {
                total.add::<MonochromeSprite>(range.len())
            }
            PrimitiveBatch::PolychromeSprites { range, .. } => {
                total.add::<PolychromeSprite>(range.len())
            }
            PrimitiveBatch::Surfaces(range) => total.add_aligned::<SurfaceBounds>(range.len()),
            PrimitiveBatch::Paths(range) => {
                let paths = &scene.paths[range];
                total.add::<PathRasterizationVertex>(
                    paths.iter().map(|path| path.vertices.len()).sum(),
                );

                // Compositing the intermediate texture is a separate batch.
                total.start_batch();
                let sprite_count = match (paths.first(), paths.last()) {
                    (Some(first), Some(last)) if first.order == last.order => paths.len(),
                    (Some(_), Some(_)) => 1,
                    _ => 0,
                };
                total.add::<PathSprite>(sprite_count);
            }
            PrimitiveBatch::SubpixelSprites { .. } => {}
        }
    }
    total.0
}

/// Copies `texture` back to the CPU, converting its BGRA pixels to RGBA.
#[cfg(any(test, feature = "test-support"))]
fn read_texture_as_rgba(texture: &ProtocolObject<dyn MTLTexture>) -> Result<RgbaImage> {
    let width = u32::try_from(texture.width()).context("texture width does not fit in a u32")?;
    let height = u32::try_from(texture.height()).context("texture height does not fit in a u32")?;
    let bytes_per_row = usize::try_from(width)
        .ok()
        .and_then(|width| width.checked_mul(4))
        .context("texture row is too large to address")?;
    let buffer_size = bytes_per_row
        .checked_mul(usize::try_from(height).context("texture height is not addressable")?)
        .context("texture is too large to read back")?;

    let mut pixels = vec![0u8; buffer_size];
    let region = MTLRegion {
        origin: MTLOrigin { x: 0, y: 0, z: 0 },
        size: MTLSize {
            width: width as usize,
            height: height as usize,
            depth: 1,
        },
    };
    // SAFETY: `pixels` covers the whole texture at this row stride.
    unsafe {
        texture.getBytes_bytesPerRow_fromRegion_mipmapLevel(
            ptr::NonNull::from(pixels.as_mut_slice()).cast(),
            bytes_per_row,
            region,
            0,
        );
    }

    for pixel in pixels.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }

    RgbaImage::from_raw(width, height, pixels)
        .context("failed to build an image from the rendered pixels")
}

// Every instanced pipeline in `shaders.metal` shares one buffer layout: the unit
// quad at slot 0, the per-instance data at slot 1 and the viewport size at slot
// 2. The assertions below keep the shared layout honest against the per-pipeline
// index enums, so a pipeline that drifts fails to compile.
const UNIT_VERTICES_SLOT: usize = QuadInputIndex::Vertices as usize;
const INSTANCES_SLOT: usize = QuadInputIndex::Quads as usize;
const VIEWPORT_SIZE_SLOT: usize = QuadInputIndex::ViewportSize as usize;

const _: () = {
    assert!(ShadowInputIndex::Vertices as usize == UNIT_VERTICES_SLOT);
    assert!(ShadowInputIndex::Shadows as usize == INSTANCES_SLOT);
    assert!(ShadowInputIndex::ViewportSize as usize == VIEWPORT_SIZE_SLOT);
    assert!(UnderlineInputIndex::Vertices as usize == UNIT_VERTICES_SLOT);
    assert!(UnderlineInputIndex::Underlines as usize == INSTANCES_SLOT);
    assert!(UnderlineInputIndex::ViewportSize as usize == VIEWPORT_SIZE_SLOT);
    assert!(SpriteInputIndex::Vertices as usize == UNIT_VERTICES_SLOT);
    assert!(SpriteInputIndex::Sprites as usize == INSTANCES_SLOT);
    assert!(SpriteInputIndex::ViewportSize as usize == VIEWPORT_SIZE_SLOT);
    assert!(SurfaceInputIndex::Vertices as usize == UNIT_VERTICES_SLOT);
    assert!(SurfaceInputIndex::Surfaces as usize == INSTANCES_SLOT);
    assert!(SurfaceInputIndex::ViewportSize as usize == VIEWPORT_SIZE_SLOT);
};

#[repr(C)]
enum ShadowInputIndex {
    Vertices = 0,
    Shadows = 1,
    ViewportSize = 2,
}

#[repr(C)]
enum QuadInputIndex {
    Vertices = 0,
    Quads = 1,
    ViewportSize = 2,
}

#[repr(C)]
enum UnderlineInputIndex {
    Vertices = 0,
    Underlines = 1,
    ViewportSize = 2,
}

#[repr(C)]
enum SpriteInputIndex {
    Vertices = 0,
    Sprites = 1,
    ViewportSize = 2,
    AtlasTextureSize = 3,
    AtlasTexture = 4,
}

#[repr(C)]
enum SurfaceInputIndex {
    Vertices = 0,
    Surfaces = 1,
    ViewportSize = 2,
    TextureSize = 3,
    YTexture = 4,
    CbCrTexture = 5,
}

#[repr(C)]
enum PathRasterizationInputIndex {
    Vertices = 0,
    ViewportSize = 1,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct PathSprite {
    pub bounds: Bounds<ScaledPixels>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct SurfaceBounds {
    pub bounds: Bounds<ScaledPixels>,
    pub content_mask: ContentMask<ScaledPixels>,
}

#[cfg(any(test, feature = "test-support"))]
pub struct MetalHeadlessRenderer {
    renderer: MetalRenderer,
}

#[cfg(any(test, feature = "test-support"))]
impl MetalHeadlessRenderer {
    pub fn new() -> Result<Self> {
        #[expect(
            clippy::arc_with_non_send_sync,
            reason = "PlatformWindow::sprite_atlas requires Arc, not Send or Sync"
        )]
        let instance_buffer_pool = Arc::new(Mutex::new(InstanceBufferPool::default()));
        let renderer = MetalRenderer::new_headless(instance_buffer_pool)?;
        Ok(Self { renderer })
    }
}

#[cfg(any(test, feature = "test-support"))]
impl gpui::PlatformHeadlessRenderer for MetalHeadlessRenderer {
    fn render_scene_to_image(
        &mut self,
        scene: &Scene,
        size: Size<DevicePixels>,
    ) -> anyhow::Result<image::RgbaImage> {
        self.renderer.render_scene_to_image(scene, size)
    }

    fn sprite_atlas(&self) -> Arc<dyn gpui::PlatformAtlas> {
        self.renderer.sprite_atlas().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_to_image_rejects_zero_sized_layer() -> Result<()> {
        let mut renderer = MetalRenderer::new(Default::default(), false, false)?;
        for (width, height) in [(0, 0), (0, 64), (64, 0)] {
            renderer.update_drawable_size(size(DevicePixels(width), DevicePixels(height)));
            assert!(renderer.render_to_image(&Scene::default()).is_err());
        }
        Ok(())
    }

    #[test]
    fn display_link_mode_consumes_and_clears_drawables() -> Result<()> {
        let mut renderer = MetalRenderer::new(Default::default(), false, false)?;
        renderer.update_drawable_size(size(DevicePixels(64), DevicePixels(64)));
        let drawable = renderer
            .layer()
            .and_then(|layer| layer.nextDrawable())
            .context("window renderer must supply a drawable")?;

        renderer.set_display_link_attached(true);
        assert!(matches!(
            renderer.display_link_mode,
            DisplayLinkMode::Attached { drawable: None }
        ));
        assert!(
            renderer
                .set_frame_drawable(Some(drawable.clone()))
                .is_none()
        );
        assert!(matches!(
            renderer.display_link_mode,
            DisplayLinkMode::Attached { drawable: Some(_) }
        ));
        renderer.draw(&Scene::default());
        assert!(matches!(
            renderer.display_link_mode,
            DisplayLinkMode::Attached { drawable: None }
        ));
        // A frame that drew consumed its drawable; one that didn't hands it back.
        assert!(
            renderer
                .set_frame_drawable(Some(drawable.clone()))
                .is_none()
        );
        assert!(renderer.set_frame_drawable(None).is_some());

        renderer.set_frame_drawable(Some(drawable));
        renderer.set_display_link_attached(false);
        assert!(matches!(
            renderer.display_link_mode,
            DisplayLinkMode::Detached
        ));
        Ok(())
    }
}
