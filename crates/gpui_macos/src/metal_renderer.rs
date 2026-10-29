use crate::metal_atlas::MetalAtlas;
#[cfg(any(test, feature = "test-support"))]
use anyhow::Context as _;
use anyhow::Result;
use block::ConcreteBlock;
use cocoa::{
    base::{NO, YES},
    foundation::{NSSize, NSUInteger},
    quartzcore::AutoresizingMask,
};
use gpui::{
    AtlasTextureId, Background, Bounds, ContentMask, DevicePixels, MonochromeSprite, PaintSurface,
    Path, Point, PolychromeSprite, PrimitiveBatch, Quad, ScaledPixels, Scene, Shadow, Size,
    Underline, point, size,
};
#[cfg(any(test, feature = "test-support"))]
use image::RgbaImage;

use core_foundation::base::TCFType;
use core_graphics::color_space::{CGColorSpace, kCGColorSpaceDisplayP3};
use core_video::{
    image_buffer::TCVImageBuffer, metal_texture::CVMetalTextureGetTexture,
    metal_texture_cache::CVMetalTextureCache,
    pixel_buffer::kCVPixelFormatType_420YpCbCr8BiPlanarFullRange,
};
use foreign_types::{ForeignType, ForeignTypeRef};
use metal::{
    CAMetalLayer, CommandQueue, MTLGPUFamily, MTLPixelFormat, MTLResourceOptions, NSRange,
    RenderPassColorAttachmentDescriptorRef,
};
#[cfg(any(test, feature = "test-support"))]
use objc::rc::autoreleasepool;
use objc::runtime::BOOL;
use objc::{self, msg_send, sel, sel_impl};
use parking_lot::Mutex;

use std::{cell::Cell, ffi::c_void, mem, ptr, sync::Arc};

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

pub(crate) type Context = Arc<Mutex<InstanceBufferPool>>;
pub(crate) type Renderer = MetalRenderer;

pub(crate) unsafe fn new_renderer(
    context: self::Context,
    _native_window: *mut c_void,
    _native_view: *mut c_void,
    _bounds: gpui::Size<f32>,
    transparent: bool,
    use_display_p3: bool,
) -> Renderer {
    MetalRenderer::new(context, transparent, use_display_p3)
}

pub(crate) struct InstanceBufferPool {
    buffer_size: usize,
    buffers: Vec<metal::Buffer>,
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
    metal_buffer: metal::Buffer,
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
        device: &metal::Device,
        unified_memory: bool,
        required_bytes: usize,
    ) -> InstanceBuffer {
        if required_bytes > self.buffer_size {
            let growth_target = required_bytes
                .checked_next_power_of_two()
                .unwrap_or(required_bytes);
            self.buffer_size = growth_target.min(MAX_INSTANCE_BUFFER_SIZE);
            self.buffers.clear();
        }

        let buffer = self.buffers.pop().unwrap_or_else(|| {
            let options = if unified_memory {
                MTLResourceOptions::StorageModeShared
                    // Buffers are write only which can benefit from the combined cache
                    // https://developer.apple.com/documentation/metal/mtlresourceoptions/cpucachemodewritecombined
                    | MTLResourceOptions::CPUCacheModeWriteCombined
            } else {
                MTLResourceOptions::StorageModeManaged
            };

            device.new_buffer(self.buffer_size as u64, options)
        });
        InstanceBuffer {
            metal_buffer: buffer,
            size: self.buffer_size,
        }
    }

    pub(crate) fn release(&mut self, buffer: InstanceBuffer) {
        if buffer.size == self.buffer_size {
            self.buffers.push(buffer.metal_buffer)
        }
    }
}

pub(crate) struct MetalRenderer {
    device: metal::Device,
    layer: Option<metal::MetalLayer>,
    is_apple_gpu: bool,
    is_unified_memory: bool,
    presents_with_transaction: bool,
    /// For headless rendering, tracks whether output should be opaque
    opaque: bool,
    command_queue: CommandQueue,
    paths_rasterization_pipeline_state: metal::RenderPipelineState,
    path_sprites_pipeline_state: metal::RenderPipelineState,
    shadows_pipeline_state: metal::RenderPipelineState,
    quads_pipeline_state: metal::RenderPipelineState,
    underlines_pipeline_state: metal::RenderPipelineState,
    monochrome_sprites_pipeline_state: metal::RenderPipelineState,
    polychrome_sprites_pipeline_state: metal::RenderPipelineState,
    surfaces_pipeline_state: metal::RenderPipelineState,
    unit_vertices: metal::Buffer,
    #[allow(clippy::arc_with_non_send_sync)]
    instance_buffer_pool: Arc<Mutex<InstanceBufferPool>>,
    sprite_atlas: Arc<MetalAtlas>,
    core_video_texture_cache: core_video::metal_texture_cache::CVMetalTextureCache,
    path_intermediate_texture: Option<metal::Texture>,
    path_intermediate_msaa_texture: Option<metal::Texture>,
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
    ) -> Self {
        let device = Self::create_device();

        let layer = metal::MetalLayer::new();
        layer.set_device(&device);
        layer.set_pixel_format(MTLPixelFormat::BGRA8Unorm);
        if use_display_p3 {
            // SAFETY: The constant is extern, from Core Graphics.
            let display_p3_name = unsafe { kCGColorSpaceDisplayP3 };
            if let Some(display_p3_color_space) = CGColorSpace::create_with_name(display_p3_name) {
                // SAFETY: `layer` is a CAMetalLayer and the color space remains valid for the call.
                unsafe {
                    let _: () = msg_send![
                        &*layer,
                        setColorspace: display_p3_color_space.as_ptr().cast::<c_void>()
                    ];
                }
            } else {
                log::error!("unable to create the Display P3 color space");
            }
        }
        // Support direct-to-display rendering if the window is not transparent
        // https://developer.apple.com/documentation/metal/managing-your-game-window-for-metal-in-macos
        layer.set_opaque(!transparent);
        layer.set_maximum_drawable_count(3);
        // Allow texture reading for visual tests (captures screenshots without ScreenCaptureKit)
        #[cfg(any(test, feature = "test-support"))]
        layer.set_framebuffer_only(false);
        unsafe {
            let _: () = msg_send![&*layer, setAllowsNextDrawableTimeout: NO];
            let _: () = msg_send![&*layer, setNeedsDisplayOnBoundsChange: YES];
            let _: () = msg_send![
                &*layer,
                setAutoresizingMask: AutoresizingMask::WIDTH_SIZABLE
                    | AutoresizingMask::HEIGHT_SIZABLE
            ];
        }

        Self::new_internal(device, Some(layer), !transparent, instance_buffer_pool)
    }

    /// Creates a new headless MetalRenderer for offscreen rendering without a window.
    ///
    /// This renderer can render scenes to images without requiring a CAMetalLayer,
    /// window, or AppKit. Use `render_scene_to_image()` to render scenes.
    #[cfg(any(test, feature = "test-support"))]
    pub fn new_headless(instance_buffer_pool: Arc<Mutex<InstanceBufferPool>>) -> Self {
        let device = Self::create_device();
        Self::new_internal(device, None, true, instance_buffer_pool)
    }

    fn create_device() -> metal::Device {
        // Prefer low‐power integrated GPUs on Intel Mac. On Apple
        // Silicon, there is only ever one GPU, so this is equivalent to
        // `metal::Device::system_default()`.
        let device = if let Some(d) = metal::Device::all()
            .into_iter()
            .min_by_key(|d| (d.is_removable(), !d.is_low_power()))
        {
            d
        } else {
            // For some reason `all()` can return an empty list, see https://github.com/zed-industries/zed/issues/37689
            // In that case, we fall back to the system default device.
            log::error!(
                "Unable to enumerate Metal devices; attempting to use system default device"
            );
            metal::Device::system_default().unwrap_or_else(|| {
                log::error!("unable to access a compatible graphics device");
                std::process::exit(1);
            })
        };

        // Metal serializes pipeline state compilation unless the app opts in to
        // concurrent compilation. This renderer builds eight pipelines while the
        // first window opens, so opting in shortens the time to first frame.
        // https://developer.apple.com/documentation/metal/mtldevice/shouldmaximizeconcurrentcompilation
        unsafe {
            let selector = sel!(setShouldMaximizeConcurrentCompilation:);
            let responds: BOOL = msg_send![&*device, respondsToSelector: selector];
            if responds == YES {
                let _: () = msg_send![&*device, setShouldMaximizeConcurrentCompilation: YES];
            }
        }

        device
    }

    fn new_internal(
        device: metal::Device,
        layer: Option<metal::MetalLayer>,
        opaque: bool,
        instance_buffer_pool: Arc<Mutex<InstanceBufferPool>>,
    ) -> Self {
        #[cfg(any(feature = "runtime_shaders", gpui_cross_runtime_shaders))]
        let library = device
            .new_library_with_source(&SHADERS_SOURCE_FILE, &metal::CompileOptions::new())
            .expect("error building metal library");
        #[cfg(not(any(feature = "runtime_shaders", gpui_cross_runtime_shaders)))]
        let library = device
            .new_library_with_data(SHADERS_METALLIB)
            .expect("error building metal library");

        fn to_float2_bits(point: PointF) -> u64 {
            let mut output = point.y.to_bits() as u64;
            output <<= 32;
            output |= point.x.to_bits() as u64;
            output
        }

        // Shared memory can be used only if CPU and GPU share the same memory space.
        // https://developer.apple.com/documentation/metal/setting-resource-storage-modes
        let is_unified_memory = device.has_unified_memory();
        // Apple GPU families support memoryless textures, which can significantly reduce
        // memory usage by keeping render targets in on-chip tile memory instead of
        // allocating backing store in system memory.
        // https://developer.apple.com/documentation/metal/mtlgpufamily
        let is_apple_gpu = device.supports_family(MTLGPUFamily::Apple1);

        let unit_vertices = [
            to_float2_bits(point(0., 0.)),
            to_float2_bits(point(1., 0.)),
            to_float2_bits(point(0., 1.)),
            to_float2_bits(point(0., 1.)),
            to_float2_bits(point(1., 0.)),
            to_float2_bits(point(1., 1.)),
        ];
        let unit_vertices = device.new_buffer_with_data(
            unit_vertices.as_ptr().cast::<c_void>(),
            mem::size_of_val(&unit_vertices) as u64,
            if is_unified_memory {
                MTLResourceOptions::StorageModeShared
                    | MTLResourceOptions::CPUCacheModeWriteCombined
            } else {
                MTLResourceOptions::StorageModeManaged
            },
        );

        let paths_rasterization_pipeline_state = build_path_rasterization_pipeline_state(
            &device,
            &library,
            "paths_rasterization",
            "path_rasterization_vertex",
            "path_rasterization_fragment",
            MTLPixelFormat::BGRA8Unorm,
            PATH_SAMPLE_COUNT,
        );
        let path_sprites_pipeline_state = build_path_sprite_pipeline_state(
            &device,
            &library,
            "path_sprites",
            "path_sprite_vertex",
            "path_sprite_fragment",
            MTLPixelFormat::BGRA8Unorm,
        );
        let shadows_pipeline_state = build_pipeline_state(
            &device,
            &library,
            "shadows",
            "shadow_vertex",
            "shadow_fragment",
            MTLPixelFormat::BGRA8Unorm,
        );
        let quads_pipeline_state = build_pipeline_state(
            &device,
            &library,
            "quads",
            "quad_vertex",
            "quad_fragment",
            MTLPixelFormat::BGRA8Unorm,
        );
        let underlines_pipeline_state = build_pipeline_state(
            &device,
            &library,
            "underlines",
            "underline_vertex",
            "underline_fragment",
            MTLPixelFormat::BGRA8Unorm,
        );
        let monochrome_sprites_pipeline_state = build_pipeline_state(
            &device,
            &library,
            "monochrome_sprites",
            "monochrome_sprite_vertex",
            "monochrome_sprite_fragment",
            MTLPixelFormat::BGRA8Unorm,
        );
        let polychrome_sprites_pipeline_state = build_pipeline_state(
            &device,
            &library,
            "polychrome_sprites",
            "polychrome_sprite_vertex",
            "polychrome_sprite_fragment",
            MTLPixelFormat::BGRA8Unorm,
        );
        let surfaces_pipeline_state = build_pipeline_state(
            &device,
            &library,
            "surfaces",
            "surface_vertex",
            "surface_fragment",
            MTLPixelFormat::BGRA8Unorm,
        );

        let command_queue = device.new_command_queue();
        let sprite_atlas = Arc::new(MetalAtlas::new(device.clone(), is_apple_gpu));
        let core_video_texture_cache =
            CVMetalTextureCache::new(None, device.clone(), None).expect("value should be present");

        Self {
            device,
            layer,
            presents_with_transaction: false,
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
        }
    }

    pub fn layer(&self) -> Option<&metal::MetalLayerRef> {
        self.layer.as_ref().map(|l| l.as_ref())
    }

    pub fn layer_ptr(&self) -> *mut CAMetalLayer {
        self.layer.as_ref().map_or(ptr::null_mut(), |l| l.as_ptr())
    }

    pub fn sprite_atlas(&self) -> &Arc<MetalAtlas> {
        &self.sprite_atlas
    }

    pub fn set_presents_with_transaction(&mut self, presents_with_transaction: bool) {
        self.presents_with_transaction = presents_with_transaction;
        if let Some(layer) = &self.layer {
            layer.set_presents_with_transaction(presents_with_transaction);
        }
    }

    pub fn update_drawable_size(&mut self, size: Size<DevicePixels>) {
        if let Some(layer) = &self.layer {
            let ns_size = NSSize {
                width: size.width.0 as f64,
                height: size.height.0 as f64,
            };
            unsafe {
                let _: () = msg_send![
                    layer.as_ref(),
                    setDrawableSize: ns_size
                ];
            }
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
            && texture.width() == size.width.0 as u64
            && texture.height() == size.height.0 as u64
        {
            return;
        }

        let texture_descriptor = metal::TextureDescriptor::new();
        texture_descriptor.set_width(size.width.0 as u64);
        texture_descriptor.set_height(size.height.0 as u64);
        texture_descriptor.set_pixel_format(metal::MTLPixelFormat::BGRA8Unorm);
        texture_descriptor.set_storage_mode(metal::MTLStorageMode::Private);
        texture_descriptor
            .set_usage(metal::MTLTextureUsage::RenderTarget | metal::MTLTextureUsage::ShaderRead);
        self.path_intermediate_texture = Some(self.device.new_texture(&texture_descriptor));

        if self.path_sample_count > 1 {
            // https://developer.apple.com/documentation/metal/choosing-a-resource-storage-mode-for-apple-gpus
            // Rendering MSAA textures are done in a single pass, so we can use memory-less storage on Apple Silicon
            let storage_mode = if self.is_apple_gpu {
                metal::MTLStorageMode::Memoryless
            } else {
                metal::MTLStorageMode::Private
            };

            let msaa_descriptor = texture_descriptor;
            msaa_descriptor.set_texture_type(metal::MTLTextureType::D2Multisample);
            msaa_descriptor.set_storage_mode(storage_mode);
            msaa_descriptor.set_sample_count(self.path_sample_count as _);
            self.path_intermediate_msaa_texture = Some(self.device.new_texture(&msaa_descriptor));
        } else {
            self.path_intermediate_msaa_texture = None;
        }
    }

    pub fn update_transparency(&mut self, transparent: bool) {
        self.opaque = !transparent;
        if let Some(layer) = &self.layer {
            layer.set_opaque(!transparent);
        }
    }

    pub fn destroy(&self) {
        // nothing to do
    }

    pub fn draw(&mut self, scene: &Scene) {
        let layer = if let Some(l) = &self.layer {
            l.clone()
        } else {
            log::error!("draw() called on headless renderer - use render_scene_to_image() instead");
            return;
        };
        let viewport_size = layer.drawable_size();
        let viewport_size: Size<DevicePixels> = size(
            (viewport_size.width.ceil() as i32).into(),
            (viewport_size.height.ceil() as i32).into(),
        );
        let Some(drawable) = layer.next_drawable() else {
            log::error!(
                "failed to retrieve next drawable, drawable size: {:?}",
                viewport_size
            );
            return;
        };

        let command_buffer = match self.encode_scene(scene, |renderer, instance_buffer| {
            renderer.draw_primitives(scene, instance_buffer, drawable, viewport_size)
        }) {
            Ok(command_buffer) => command_buffer,
            Err(error) => {
                log::error!("failed to render: {error:#}");
                return;
            }
        };

        if self.presents_with_transaction {
            command_buffer.commit();
            command_buffer.wait_until_scheduled();
            drawable.present();
        } else {
            command_buffer.present_drawable(drawable);
            command_buffer.commit();
        }
    }

    /// Encodes `scene` into a command buffer, growing the shared instance buffer
    /// until the whole scene fits, and returns the buffer to the pool once the
    /// GPU is done with it.
    fn encode_scene(
        &mut self,
        scene: &Scene,
        mut encode: impl FnMut(&mut Self, &mut InstanceBuffer) -> Result<metal::CommandBuffer>,
    ) -> Result<metal::CommandBuffer> {
        let required_bytes = required_instance_bytes(scene);
        loop {
            let mut instance_buffer = {
                let mut pool = self.instance_buffer_pool.lock();
                pool.acquire(&self.device, self.is_unified_memory, required_bytes)
            };

            match encode(self, &mut instance_buffer) {
                Ok(command_buffer) => {
                    let instance_buffer_pool = self.instance_buffer_pool.clone();
                    let instance_buffer = Cell::new(Some(instance_buffer));
                    let block = ConcreteBlock::new(move |_| {
                        if let Some(instance_buffer) = instance_buffer.take() {
                            instance_buffer_pool.lock().release(instance_buffer);
                        }
                    });
                    let block = block.copy();
                    command_buffer.add_completed_handler(&block);
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
        let viewport_size = layer.drawable_size();
        let viewport_size: Size<DevicePixels> = size(
            (viewport_size.width.ceil() as i32).into(),
            (viewport_size.height.ceil() as i32).into(),
        );
        let drawable = layer
            .next_drawable()
            .ok_or_else(|| anyhow::anyhow!("Failed to get drawable for render_to_image"))?;

        let command_buffer = self.encode_scene(scene, |renderer, instance_buffer| {
            renderer.draw_primitives(scene, instance_buffer, drawable, viewport_size)
        })?;

        // Commit and wait for completion without presenting
        command_buffer.commit();
        command_buffer.wait_until_completed();

        read_texture_as_rgba(drawable.texture())
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
        autoreleasepool(|| {
            // Update path intermediate textures for this size
            self.update_path_intermediate_textures(size);

            // Create an offscreen texture as render target
            let texture_descriptor = metal::TextureDescriptor::new();
            texture_descriptor.set_width(size.width.0 as u64);
            texture_descriptor.set_height(size.height.0 as u64);
            texture_descriptor.set_pixel_format(MTLPixelFormat::BGRA8Unorm);
            texture_descriptor.set_usage(
                metal::MTLTextureUsage::RenderTarget | metal::MTLTextureUsage::ShaderRead,
            );
            texture_descriptor.set_storage_mode(metal::MTLStorageMode::Managed);
            let target_texture = self.device.new_texture(&texture_descriptor);

            let command_buffer = self.encode_scene(scene, |renderer, instance_buffer| {
                renderer.draw_primitives_to_texture(scene, instance_buffer, &target_texture, size)
            })?;

            // On discrete GPUs (non-unified memory), Managed textures
            // require an explicit blit synchronize before the CPU can
            // read back the rendered data. Without this, get_bytes
            // returns stale zeros.
            if !self.is_unified_memory {
                let blit = command_buffer.new_blit_command_encoder();
                blit.synchronize_resource(&target_texture);
                blit.end_encoding();
            }

            command_buffer.commit();
            command_buffer.wait_until_completed();

            read_texture_as_rgba(&target_texture)
        })
    }

    fn draw_primitives(
        &mut self,
        scene: &Scene,
        instance_buffer: &mut InstanceBuffer,
        drawable: &metal::MetalDrawableRef,
        viewport_size: Size<DevicePixels>,
    ) -> Result<metal::CommandBuffer> {
        self.draw_primitives_to_texture(scene, instance_buffer, drawable.texture(), viewport_size)
    }

    fn draw_primitives_to_texture(
        &mut self,
        scene: &Scene,
        instance_buffer: &mut InstanceBuffer,
        texture: &metal::TextureRef,
        viewport_size: Size<DevicePixels>,
    ) -> Result<metal::CommandBuffer> {
        let command_queue = self.command_queue.clone();
        let command_buffer = command_queue.new_command_buffer();
        let alpha = if self.opaque { 1. } else { 0. };
        let mut instance_offset = 0;

        let Some(mut command_encoder) = new_command_encoder_for_texture(
            command_buffer,
            texture,
            viewport_size,
            |color_attachment| {
                color_attachment.set_load_action(metal::MTLLoadAction::Clear);
                color_attachment.set_clear_color(metal::MTLClearColor::new(0., 0., 0., alpha));
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
                    command_encoder,
                ),
                PrimitiveBatch::Quads(range) => self.draw_quads(
                    &scene.quads[range],
                    instance_buffer,
                    &mut instance_offset,
                    viewport_size,
                    command_encoder,
                ),
                PrimitiveBatch::Paths(range) => {
                    let paths = &scene.paths[range];
                    command_encoder.end_encoding();

                    let did_draw = self.draw_paths_to_intermediate(
                        paths,
                        instance_buffer,
                        &mut instance_offset,
                        viewport_size,
                        command_buffer,
                    );

                    let Some(encoder) = new_command_encoder_for_texture(
                        command_buffer,
                        texture,
                        viewport_size,
                        |color_attachment| {
                            color_attachment.set_load_action(metal::MTLLoadAction::Load);
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
                            command_encoder,
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
                    command_encoder,
                ),
                PrimitiveBatch::MonochromeSprites { texture_id, range } => self
                    .draw_monochrome_sprites(
                        texture_id,
                        &scene.monochrome_sprites[range],
                        instance_buffer,
                        &mut instance_offset,
                        viewport_size,
                        command_encoder,
                    ),
                PrimitiveBatch::PolychromeSprites { texture_id, range } => self
                    .draw_polychrome_sprites(
                        texture_id,
                        &scene.polychrome_sprites[range],
                        instance_buffer,
                        &mut instance_offset,
                        viewport_size,
                        command_encoder,
                    ),
                PrimitiveBatch::Surfaces(range) => self.draw_surfaces(
                    &scene.surfaces[range],
                    instance_buffer,
                    &mut instance_offset,
                    viewport_size,
                    command_encoder,
                ),
                PrimitiveBatch::SubpixelSprites { .. } => {
                    log::error!("subpixel sprites are not supported by the Metal renderer");
                    true
                }
            };
            if !ok {
                command_encoder.end_encoding();
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

        command_encoder.end_encoding();

        if !self.is_unified_memory {
            // Sync the instance buffer to the GPU
            instance_buffer.metal_buffer.did_modify_range(NSRange {
                location: 0,
                length: instance_offset as NSUInteger,
            });
        }

        Ok(command_buffer.to_owned())
    }

    fn draw_paths_to_intermediate(
        &self,
        paths: &[Path<ScaledPixels>],
        instance_buffer: &mut InstanceBuffer,
        instance_offset: &mut usize,
        viewport_size: Size<DevicePixels>,
        command_buffer: &metal::CommandBufferRef,
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

        let render_pass_descriptor = metal::RenderPassDescriptor::new();
        let Some(color_attachment) = render_pass_descriptor.color_attachments().object_at(0) else {
            log::error!("Metal did not provide a color attachment for the path texture");
            return false;
        };
        color_attachment.set_load_action(metal::MTLLoadAction::Clear);
        color_attachment.set_clear_color(metal::MTLClearColor::new(0., 0., 0., 0.));

        if let Some(msaa_texture) = &self.path_intermediate_msaa_texture {
            color_attachment.set_texture(Some(msaa_texture));
            color_attachment.set_resolve_texture(Some(intermediate_texture));
            color_attachment.set_store_action(metal::MTLStoreAction::MultisampleResolve);
        } else {
            color_attachment.set_texture(Some(intermediate_texture));
            color_attachment.set_store_action(metal::MTLStoreAction::Store);
        }

        let command_encoder = command_buffer.new_render_command_encoder(render_pass_descriptor);
        command_encoder.set_render_pipeline_state(&self.paths_rasterization_pipeline_state);
        command_encoder.set_vertex_buffer(
            PathRasterizationInputIndex::Vertices as u64,
            Some(&instance_buffer.metal_buffer),
            vertices_offset as u64,
        );
        command_encoder.set_vertex_bytes(
            PathRasterizationInputIndex::ViewportSize as u64,
            mem::size_of_val(&viewport_size) as u64,
            (&raw const viewport_size).cast(),
        );
        command_encoder.set_fragment_buffer(
            PathRasterizationInputIndex::Vertices as u64,
            Some(&instance_buffer.metal_buffer),
            vertices_offset as u64,
        );
        command_encoder.draw_primitives(metal::MTLPrimitiveType::Triangle, 0, vertex_count as u64);

        command_encoder.end_encoding();
        true
    }

    fn draw_shadows(
        &self,
        shadows: &[Shadow],
        instance_buffer: &mut InstanceBuffer,
        instance_offset: &mut usize,
        viewport_size: Size<DevicePixels>,
        command_encoder: &metal::RenderCommandEncoderRef,
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
        command_encoder: &metal::RenderCommandEncoderRef,
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
        command_encoder: &metal::RenderCommandEncoderRef,
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
        command_encoder: &metal::RenderCommandEncoderRef,
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
                encoder.set_vertex_bytes(
                    SpriteInputIndex::AtlasTextureSize as u64,
                    mem::size_of_val(&texture_size) as u64,
                    (&raw const texture_size).cast(),
                );
                encoder.set_fragment_texture(SpriteInputIndex::AtlasTexture as u64, Some(&texture));
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
        command_encoder: &metal::RenderCommandEncoderRef,
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
                encoder.set_vertex_bytes(
                    SpriteInputIndex::AtlasTextureSize as u64,
                    mem::size_of_val(&texture_size) as u64,
                    (&raw const texture_size).cast(),
                );
                encoder.set_fragment_texture(SpriteInputIndex::AtlasTexture as u64, Some(&texture));
            },
        )
    }

    fn draw_paths_from_intermediate(
        &self,
        paths: &[Path<ScaledPixels>],
        instance_buffer: &mut InstanceBuffer,
        instance_offset: &mut usize,
        viewport_size: Size<DevicePixels>,
        command_encoder: &metal::RenderCommandEncoderRef,
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
        command_encoder.set_fragment_texture(
            SpriteInputIndex::AtlasTexture as u64,
            Some(intermediate_texture),
        );
        command_encoder.draw_primitives_instanced(
            metal::MTLPrimitiveType::Triangle,
            0,
            6,
            sprite_count as u64,
        );

        true
    }

    /// Binds the state every instanced pipeline in `shaders.metal` shares: the
    /// unit quad at buffer slot 0, the per-instance data at slot 1 and the
    /// viewport size at slot 2.
    fn bind_instanced_state(
        &self,
        command_encoder: &metal::RenderCommandEncoderRef,
        pipeline_state: &metal::RenderPipelineState,
        instance_buffer: &InstanceBuffer,
        instance_start: usize,
        viewport_size: Size<DevicePixels>,
    ) {
        command_encoder.set_render_pipeline_state(pipeline_state);
        command_encoder.set_vertex_buffer(UNIT_VERTICES_SLOT, Some(&self.unit_vertices), 0);
        command_encoder.set_vertex_buffer(
            INSTANCES_SLOT,
            Some(&instance_buffer.metal_buffer),
            instance_start as u64,
        );
        command_encoder.set_fragment_buffer(
            INSTANCES_SLOT,
            Some(&instance_buffer.metal_buffer),
            instance_start as u64,
        );
        command_encoder.set_vertex_bytes(
            VIEWPORT_SIZE_SLOT,
            mem::size_of_val(&viewport_size) as u64,
            (&raw const viewport_size).cast(),
        );
    }

    /// Writes `instances` into the shared instance buffer and draws one unit
    /// quad per instance, letting `configure` bind anything else the pipeline
    /// needs. Returns `false` when the instances do not fit in the buffer.
    fn draw_instanced<T: Copy>(
        &self,
        command_encoder: &metal::RenderCommandEncoderRef,
        pipeline_state: &metal::RenderPipelineState,
        instance_buffer: &mut InstanceBuffer,
        instance_offset: &mut usize,
        viewport_size: Size<DevicePixels>,
        instances: &[T],
        configure: impl FnOnce(&metal::RenderCommandEncoderRef),
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
        command_encoder.draw_primitives_instanced(
            metal::MTLPrimitiveType::Triangle,
            0,
            6,
            instances.len() as u64,
        );
        true
    }

    fn draw_surfaces(
        &mut self,
        surfaces: &[PaintSurface],
        instance_buffer: &mut InstanceBuffer,
        instance_offset: &mut usize,
        viewport_size: Size<DevicePixels>,
        command_encoder: &metal::RenderCommandEncoderRef,
    ) -> bool {
        for surface in surfaces {
            if surface.image_buffer.get_pixel_format()
                != kCVPixelFormatType_420YpCbCr8BiPlanarFullRange
            {
                log::error!(
                    "skipping surface with unsupported pixel format {:?}",
                    surface.image_buffer.get_pixel_format()
                );
                continue;
            }

            let texture_size = size(
                DevicePixels::from(surface.image_buffer.get_width() as i32),
                DevicePixels::from(surface.image_buffer.get_height() as i32),
            );

            let image_buffer = surface.image_buffer.as_image_buffer();
            let y_texture = match self.core_video_texture_cache.create_texture_from_image(
                &image_buffer,
                None,
                MTLPixelFormat::R8Unorm,
                surface.image_buffer.get_width_of_plane(0),
                surface.image_buffer.get_height_of_plane(0),
                0,
            ) {
                Ok(texture) => texture,
                Err(error) => {
                    log::error!(
                        "failed to wrap the luma plane of a surface in a texture: {error:?}"
                    );
                    continue;
                }
            };
            let cb_cr_texture = match self.core_video_texture_cache.create_texture_from_image(
                &image_buffer,
                None,
                MTLPixelFormat::RG8Unorm,
                surface.image_buffer.get_width_of_plane(1),
                surface.image_buffer.get_height_of_plane(1),
                1,
            ) {
                Ok(texture) => texture,
                Err(error) => {
                    log::error!(
                        "failed to wrap the chroma plane of a surface in a texture: {error:?}"
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
            command_encoder.set_vertex_bytes(
                SurfaceInputIndex::TextureSize as u64,
                mem::size_of_val(&texture_size) as u64,
                (&raw const texture_size).cast(),
            );
            command_encoder.set_fragment_texture(SurfaceInputIndex::YTexture as u64, unsafe {
                let texture = CVMetalTextureGetTexture(y_texture.as_concrete_TypeRef());
                Some(metal::TextureRef::from_ptr(texture.cast()))
            });
            command_encoder.set_fragment_texture(SurfaceInputIndex::CbCrTexture as u64, unsafe {
                let texture = CVMetalTextureGetTexture(cb_cr_texture.as_concrete_TypeRef());
                Some(metal::TextureRef::from_ptr(texture.cast()))
            });
            command_encoder.draw_primitives(metal::MTLPrimitiveType::Triangle, 0, 6);
        }
        true
    }
}

fn new_command_encoder_for_texture<'a>(
    command_buffer: &'a metal::CommandBufferRef,
    texture: &'a metal::TextureRef,
    viewport_size: Size<DevicePixels>,
    configure_color_attachment: impl Fn(&RenderPassColorAttachmentDescriptorRef),
) -> Option<&'a metal::RenderCommandEncoderRef> {
    let render_pass_descriptor = metal::RenderPassDescriptor::new();
    let color_attachment = render_pass_descriptor.color_attachments().object_at(0)?;
    color_attachment.set_texture(Some(texture));
    color_attachment.set_store_action(metal::MTLStoreAction::Store);
    configure_color_attachment(color_attachment);

    let command_encoder = command_buffer.new_render_command_encoder(render_pass_descriptor);
    command_encoder.set_viewport(metal::MTLViewport {
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
    device: &metal::DeviceRef,
    library: &metal::LibraryRef,
    label: &str,
    vertex_fn_name: &str,
    fragment_fn_name: &str,
    pixel_format: metal::MTLPixelFormat,
) -> metal::RenderPipelineState {
    let vertex_fn = library
        .get_function(vertex_fn_name, None)
        .expect("error locating vertex function");
    let fragment_fn = library
        .get_function(fragment_fn_name, None)
        .expect("error locating fragment function");

    let descriptor = metal::RenderPipelineDescriptor::new();
    descriptor.set_label(label);
    descriptor.set_vertex_function(Some(vertex_fn.as_ref()));
    descriptor.set_fragment_function(Some(fragment_fn.as_ref()));
    let color_attachment = descriptor
        .color_attachments()
        .object_at(0)
        .expect("object_at should be present");
    color_attachment.set_pixel_format(pixel_format);
    color_attachment.set_blending_enabled(true);
    color_attachment.set_rgb_blend_operation(metal::MTLBlendOperation::Add);
    color_attachment.set_alpha_blend_operation(metal::MTLBlendOperation::Add);
    color_attachment.set_source_rgb_blend_factor(metal::MTLBlendFactor::SourceAlpha);
    color_attachment.set_source_alpha_blend_factor(metal::MTLBlendFactor::One);
    color_attachment.set_destination_rgb_blend_factor(metal::MTLBlendFactor::OneMinusSourceAlpha);
    color_attachment.set_destination_alpha_blend_factor(metal::MTLBlendFactor::OneMinusSourceAlpha);

    device
        .new_render_pipeline_state(&descriptor)
        .expect("could not create render pipeline state")
}

fn build_path_sprite_pipeline_state(
    device: &metal::DeviceRef,
    library: &metal::LibraryRef,
    label: &str,
    vertex_fn_name: &str,
    fragment_fn_name: &str,
    pixel_format: metal::MTLPixelFormat,
) -> metal::RenderPipelineState {
    let vertex_fn = library
        .get_function(vertex_fn_name, None)
        .expect("error locating vertex function");
    let fragment_fn = library
        .get_function(fragment_fn_name, None)
        .expect("error locating fragment function");

    let descriptor = metal::RenderPipelineDescriptor::new();
    descriptor.set_label(label);
    descriptor.set_vertex_function(Some(vertex_fn.as_ref()));
    descriptor.set_fragment_function(Some(fragment_fn.as_ref()));
    let color_attachment = descriptor
        .color_attachments()
        .object_at(0)
        .expect("object_at should be present");
    color_attachment.set_pixel_format(pixel_format);
    color_attachment.set_blending_enabled(true);
    color_attachment.set_rgb_blend_operation(metal::MTLBlendOperation::Add);
    color_attachment.set_alpha_blend_operation(metal::MTLBlendOperation::Add);
    color_attachment.set_source_rgb_blend_factor(metal::MTLBlendFactor::One);
    color_attachment.set_source_alpha_blend_factor(metal::MTLBlendFactor::One);
    color_attachment.set_destination_rgb_blend_factor(metal::MTLBlendFactor::OneMinusSourceAlpha);
    color_attachment.set_destination_alpha_blend_factor(metal::MTLBlendFactor::OneMinusSourceAlpha);

    device
        .new_render_pipeline_state(&descriptor)
        .expect("could not create render pipeline state")
}

fn build_path_rasterization_pipeline_state(
    device: &metal::DeviceRef,
    library: &metal::LibraryRef,
    label: &str,
    vertex_fn_name: &str,
    fragment_fn_name: &str,
    pixel_format: metal::MTLPixelFormat,
    path_sample_count: u32,
) -> metal::RenderPipelineState {
    let vertex_fn = library
        .get_function(vertex_fn_name, None)
        .expect("error locating vertex function");
    let fragment_fn = library
        .get_function(fragment_fn_name, None)
        .expect("error locating fragment function");

    let descriptor = metal::RenderPipelineDescriptor::new();
    descriptor.set_label(label);
    descriptor.set_vertex_function(Some(vertex_fn.as_ref()));
    descriptor.set_fragment_function(Some(fragment_fn.as_ref()));
    if path_sample_count > 1 {
        descriptor.set_raster_sample_count(path_sample_count as _);
        descriptor.set_alpha_to_coverage_enabled(false);
    }
    let color_attachment = descriptor
        .color_attachments()
        .object_at(0)
        .expect("object_at should be present");
    color_attachment.set_pixel_format(pixel_format);
    color_attachment.set_blending_enabled(true);
    color_attachment.set_rgb_blend_operation(metal::MTLBlendOperation::Add);
    color_attachment.set_alpha_blend_operation(metal::MTLBlendOperation::Add);
    color_attachment.set_source_rgb_blend_factor(metal::MTLBlendFactor::One);
    color_attachment.set_source_alpha_blend_factor(metal::MTLBlendFactor::One);
    color_attachment.set_destination_rgb_blend_factor(metal::MTLBlendFactor::OneMinusSourceAlpha);
    color_attachment.set_destination_alpha_blend_factor(metal::MTLBlendFactor::OneMinusSourceAlpha);

    device
        .new_render_pipeline_state(&descriptor)
        .expect("could not create render pipeline state")
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
fn read_texture_as_rgba(texture: &metal::TextureRef) -> Result<RgbaImage> {
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
    let region = metal::MTLRegion {
        origin: metal::MTLOrigin { x: 0, y: 0, z: 0 },
        size: metal::MTLSize {
            width: width as u64,
            height: height as u64,
            depth: 1,
        },
    };
    texture.get_bytes(
        pixels.as_mut_ptr().cast::<c_void>(),
        bytes_per_row as u64,
        region,
        0,
    );

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
const UNIT_VERTICES_SLOT: u64 = QuadInputIndex::Vertices as u64;
const INSTANCES_SLOT: u64 = QuadInputIndex::Quads as u64;
const VIEWPORT_SIZE_SLOT: u64 = QuadInputIndex::ViewportSize as u64;

const _: () = {
    assert!(ShadowInputIndex::Vertices as u64 == UNIT_VERTICES_SLOT);
    assert!(ShadowInputIndex::Shadows as u64 == INSTANCES_SLOT);
    assert!(ShadowInputIndex::ViewportSize as u64 == VIEWPORT_SIZE_SLOT);
    assert!(UnderlineInputIndex::Vertices as u64 == UNIT_VERTICES_SLOT);
    assert!(UnderlineInputIndex::Underlines as u64 == INSTANCES_SLOT);
    assert!(UnderlineInputIndex::ViewportSize as u64 == VIEWPORT_SIZE_SLOT);
    assert!(SpriteInputIndex::Vertices as u64 == UNIT_VERTICES_SLOT);
    assert!(SpriteInputIndex::Sprites as u64 == INSTANCES_SLOT);
    assert!(SpriteInputIndex::ViewportSize as u64 == VIEWPORT_SIZE_SLOT);
    assert!(SurfaceInputIndex::Vertices as u64 == UNIT_VERTICES_SLOT);
    assert!(SurfaceInputIndex::Surfaces as u64 == INSTANCES_SLOT);
    assert!(SurfaceInputIndex::ViewportSize as u64 == VIEWPORT_SIZE_SLOT);
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
    pub fn new() -> Self {
        let instance_buffer_pool = Arc::new(Mutex::new(InstanceBufferPool::default()));
        let renderer = MetalRenderer::new_headless(instance_buffer_pool);
        Self { renderer }
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
