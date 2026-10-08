use anyhow::{Context as _, Result};
use collections::FxHashMap;
use gpui::{
    Atlas, AtlasBackend, AtlasEpoch, AtlasFrame, AtlasKey, AtlasPolicy, AtlasSnapshot,
    AtlasTextureDescriptor, AtlasTextureId, AtlasTextureKind, AtlasTile, AtlasUpload, AtlasUsage,
    Bounds, DevicePixels, PlatformAtlas, Size,
};
use std::{borrow::Cow, sync::Arc};

use crate::WgpuContext;

pub struct WgpuAtlas(Atlas<WgpuAtlasTextures>);

struct PendingUpload {
    id: AtlasTextureId,
    bounds: Bounds<DevicePixels>,
    data: Vec<u8>,
}

struct WgpuAtlasTextures {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    color_texture_format: wgpu::TextureFormat,
    storage: FxHashMap<AtlasTextureId, WgpuAtlasTexture>,
    pending_uploads: Vec<PendingUpload>,
    upload_calls: u64,
    uploaded_bytes: u64,
}

pub struct WgpuTextureInfo {
    pub view: wgpu::TextureView,
}

impl WgpuAtlas {
    pub fn new(
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        color_texture_format: wgpu::TextureFormat,
    ) -> Self {
        Self::new_with_policy(device, queue, color_texture_format, AtlasPolicy::default())
    }

    pub fn new_with_policy(
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        color_texture_format: wgpu::TextureFormat,
        policy: AtlasPolicy,
    ) -> Self {
        let max_texture_size = DevicePixels(device.limits().max_texture_dimension_2d as i32);
        Self(Atlas::with_policy(
            WgpuAtlasTextures {
                device,
                queue,
                color_texture_format,
                storage: FxHashMap::default(),
                pending_uploads: Vec::new(),
                upload_calls: 0,
                uploaded_bytes: 0,
            },
            Size {
                width: max_texture_size,
                height: max_texture_size,
            },
            policy,
        ))
    }

    pub fn from_context(context: &WgpuContext) -> Self {
        Self::new(
            context.device.clone(),
            context.queue.clone(),
            context.color_texture_format(),
        )
    }

    pub fn from_context_with_policy(context: &WgpuContext, policy: AtlasPolicy) -> Self {
        Self::new_with_policy(
            context.device.clone(),
            context.queue.clone(),
            context.color_texture_format(),
            policy,
        )
    }

    pub fn flush_uploads(&self) {
        self.0.update_backend(WgpuAtlasTextures::flush_uploads);
    }

    /// Returns the view backing `id`, or `None` once every tile in it has been
    /// removed. A scene can still reference such a texture when a cached view
    /// replays a paint from before the image was dropped, so callers must skip
    /// those sprites rather than assume the texture exists.
    pub fn get_texture_info(&self, id: AtlasTextureId) -> Option<WgpuTextureInfo> {
        self.0.with_backend(|textures| {
            textures.storage.get(&id).map(|texture| WgpuTextureInfo {
                view: texture.view.clone(),
            })
        })
    }

    /// Clears all cached textures and tiles, forcing them to be recreated.
    /// Use this for incremental recovery when the device is still valid.
    pub fn clear(&self) {
        self.0.clear();
    }

    /// Handles device lost by clearing all textures and cached tiles.
    /// The atlas will lazily recreate textures as needed on subsequent frames.
    pub fn handle_device_lost(&self, context: &WgpuContext) {
        let max_texture_size =
            DevicePixels(context.device.limits().max_texture_dimension_2d as i32);
        self.0.reset(
            Size {
                width: max_texture_size,
                height: max_texture_size,
            },
            |textures| {
                textures.device = context.device.clone();
                textures.queue = context.queue.clone();
                textures.color_texture_format = context.color_texture_format();
            },
        );
    }
}

impl PlatformAtlas for WgpuAtlas {
    fn get_or_insert_with<'a>(
        &self,
        key: AtlasKey,
        build: &mut dyn FnMut() -> Result<Option<(Size<DevicePixels>, Cow<'a, [u8]>)>>,
    ) -> Result<Option<AtlasTile>> {
        self.0.get_or_insert_with(key, build)
    }

    fn remove(&self, key: &AtlasKey) {
        self.0.remove(key);
    }

    fn begin_frame(&self) -> AtlasFrame {
        self.0.begin_frame()
    }

    fn finish_frame(&self, frame: AtlasFrame, usage: &AtlasUsage) -> AtlasEpoch {
        self.0.finish_frame(frame, usage)
    }

    fn current_epoch(&self) -> AtlasEpoch {
        self.0.current_epoch()
    }

    fn snapshot(&self) -> AtlasSnapshot {
        self.0.snapshot()
    }
}

impl AtlasBackend for WgpuAtlasTextures {
    fn create_texture(&mut self, descriptor: AtlasTextureDescriptor) -> Result<()> {
        anyhow::ensure!(
            !self.storage.contains_key(&descriptor.texture_id),
            "atlas texture identity was reused"
        );
        let format = match descriptor.kind {
            AtlasTextureKind::Monochrome => wgpu::TextureFormat::R8Unorm,
            AtlasTextureKind::Subpixel | AtlasTextureKind::Polychrome => self.color_texture_format,
        };
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("atlas"),
            size: wgpu::Extent3d {
                width: descriptor.size.width.0 as u32,
                height: descriptor.size.height.0 as u32,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.storage.insert(
            descriptor.texture_id,
            WgpuAtlasTexture {
                format,
                texture,
                view,
            },
        );
        Ok(())
    }

    fn upload(&mut self, upload: AtlasUpload<'_>) -> Result<()> {
        let texture = self
            .storage
            .get(&upload.texture_id)
            .context("atlas upload refers to a missing texture")?;
        let data = swizzle_upload_data(upload.bytes, texture.format);
        self.pending_uploads.push(PendingUpload {
            id: upload.texture_id,
            bounds: upload.bounds,
            data,
        });
        Ok(())
    }

    fn destroy_texture(&mut self, texture_id: AtlasTextureId) {
        self.storage.remove(&texture_id);
        self.pending_uploads
            .retain(|upload| upload.id != texture_id);
    }

    fn clear_textures(&mut self) {
        self.storage.clear();
        self.pending_uploads.clear();
    }

    fn snapshot(&self) -> AtlasSnapshot {
        AtlasSnapshot {
            pending_uploads: self.pending_uploads.len(),
            pending_upload_bytes: self
                .pending_uploads
                .iter()
                .map(|upload| upload.data.len())
                .sum(),
            upload_calls: self.upload_calls,
            uploaded_bytes: self.uploaded_bytes,
            ..AtlasSnapshot::default()
        }
    }
}

impl WgpuAtlasTextures {
    fn flush_uploads(&mut self) {
        for upload in self.pending_uploads.drain(..) {
            let Some(texture) = self.storage.get(&upload.id) else {
                continue;
            };
            let bytes_per_pixel = texture.bytes_per_pixel();

            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: upload.bounds.origin.x.0 as u32,
                        y: upload.bounds.origin.y.0 as u32,
                        z: 0,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &upload.data,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(
                        upload.bounds.size.width.0 as u32 * u32::from(bytes_per_pixel),
                    ),
                    rows_per_image: None,
                },
                wgpu::Extent3d {
                    width: upload.bounds.size.width.0 as u32,
                    height: upload.bounds.size.height.0 as u32,
                    depth_or_array_layers: 1,
                },
            );
            self.upload_calls = self.upload_calls.saturating_add(1);
            self.uploaded_bytes = self
                .uploaded_bytes
                .saturating_add(u64::try_from(upload.data.len()).unwrap_or(u64::MAX));
        }
    }
}

struct WgpuAtlasTexture {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    format: wgpu::TextureFormat,
}

impl WgpuAtlasTexture {
    fn bytes_per_pixel(&self) -> u8 {
        match self.format {
            wgpu::TextureFormat::R8Unorm => 1,
            wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Rgba8Unorm => 4,
            _ => 4,
        }
    }
}

fn swizzle_upload_data(bytes: &[u8], format: wgpu::TextureFormat) -> Vec<u8> {
    match format {
        wgpu::TextureFormat::Rgba8Unorm => {
            let mut data = bytes.to_vec();
            for pixel in data.chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }
            data
        }
        _ => bytes.to_vec(),
    }
}

#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    use super::*;
    use gpui::block_on;
    use gpui::{ImageId, RenderImageParams};
    use std::sync::Arc;

    fn test_device_and_queue() -> anyhow::Result<(Arc<wgpu::Device>, Arc<wgpu::Queue>)> {
        block_on(async {
            let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
                backends: wgpu::Backends::all(),
                flags: wgpu::InstanceFlags::default(),
                backend_options: wgpu::BackendOptions::default(),
                memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
                display: None,
            });
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::LowPower,
                    compatible_surface: None,
                    force_fallback_adapter: false,
                })
                .await
                .map_err(|error| anyhow::anyhow!("failed to request adapter: {error}"))?;
            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor {
                    label: Some("wgpu_atlas_test_device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::downlevel_defaults()
                        .using_resolution(adapter.limits())
                        .using_alignment(adapter.limits()),
                    memory_hints: wgpu::MemoryHints::MemoryUsage,
                    trace: wgpu::Trace::Off,
                    experimental_features: wgpu::ExperimentalFeatures::disabled(),
                })
                .await
                .map_err(|error| anyhow::anyhow!("failed to request device: {error}"))?;
            Ok((Arc::new(device), Arc::new(queue)))
        })
    }

    fn usage(tiles: impl IntoIterator<Item = AtlasTile>) -> AtlasUsage {
        let mut usage = AtlasUsage::default();
        for tile in tiles {
            usage.insert(tile);
        }
        usage
    }

    fn complete_frame(atlas: &WgpuAtlas, tiles: impl IntoIterator<Item = AtlasTile>) {
        let frame = atlas.begin_frame();
        atlas.finish_frame(frame, &usage(tiles));
    }

    #[test]
    fn flush_uploads_skips_uploads_for_retired_texture() -> anyhow::Result<()> {
        let (device, queue) = test_device_and_queue()?;

        let atlas = WgpuAtlas::new(device, queue, wgpu::TextureFormat::Bgra8Unorm);
        let key = AtlasKey::Image(RenderImageParams {
            image_id: ImageId(1),
            frame_index: 0,
        });
        let size = Size {
            width: DevicePixels(1),
            height: DevicePixels(1),
        };
        let mut build = || Ok(Some((size, Cow::Owned(vec![0, 0, 0, 255]))));

        // Regression test: before the fix, this panicked in flush_uploads
        let tile = atlas
            .get_or_insert_with(key.clone(), &mut build)?
            .expect("tile should be created");
        complete_frame(&atlas, [tile]);
        atlas.remove(&key);
        let frame = atlas.begin_frame();
        atlas.finish_frame(frame, &AtlasUsage::default());
        atlas.flush_uploads();
        Ok(())
    }

    #[test]
    fn snapshot_tracks_residency_and_uploads() -> anyhow::Result<()> {
        let (device, queue) = test_device_and_queue()?;
        let atlas = WgpuAtlas::new(device, queue, wgpu::TextureFormat::Bgra8Unorm);
        let key = AtlasKey::Image(RenderImageParams {
            image_id: ImageId(1),
            frame_index: 0,
        });
        let size = Size {
            width: DevicePixels(1),
            height: DevicePixels(1),
        };
        let mut build = || Ok(Some((size, Cow::Owned(vec![0, 0, 0, 255]))));

        let tile = atlas
            .get_or_insert_with(key.clone(), &mut build)?
            .expect("tile should be created");
        assert_eq!(tile.padding, 1);
        assert_eq!(tile.bounds.size, size);
        let pending = atlas.snapshot();
        assert_eq!(pending.page_count, 1);
        assert_eq!(pending.resident_bytes, 1024 * 1024 * 4);
        assert_eq!(pending.entry_count, 1);
        assert_eq!(pending.misses, 1);
        assert_eq!(pending.allocations, 1);
        assert_eq!(pending.pending_uploads, 1);
        assert_eq!(pending.pending_upload_bytes, 3 * 3 * 4);
        assert_eq!(pending.upload_calls, 0);

        atlas.flush_uploads();
        let uploaded = atlas.snapshot();
        assert_eq!(uploaded.pending_uploads, 0);
        assert_eq!(uploaded.pending_upload_bytes, 0);
        assert_eq!(uploaded.upload_calls, 1);
        assert_eq!(uploaded.uploaded_bytes, 3 * 3 * 4);

        atlas
            .get_or_insert_with(key.clone(), &mut || {
                anyhow::bail!("cache hit must not invoke the builder")
            })?
            .expect("cached tile should remain present");
        assert_eq!(atlas.snapshot().hits, 1);

        complete_frame(
            &atlas,
            [atlas
                .get_or_insert_with(key.clone(), &mut build)?
                .context("cached tile should remain present")?],
        );
        atlas.remove(&key);
        let frame = atlas.begin_frame();
        assert_eq!(atlas.snapshot().page_count, 1);
        atlas.finish_frame(frame, &AtlasUsage::default());
        let removed = atlas.snapshot();
        assert_eq!(removed.page_count, 0);
        assert_eq!(removed.resident_bytes, 0);
        assert_eq!(removed.entry_count, 0);
        assert_eq!(removed.removals, 1);
        Ok(())
    }

    #[test]
    fn clear_does_not_rebind_an_old_texture_identity() -> anyhow::Result<()> {
        let (device, queue) = test_device_and_queue()?;
        let atlas = WgpuAtlas::new(device, queue, wgpu::TextureFormat::Bgra8Unorm);
        let size = Size {
            width: DevicePixels(1),
            height: DevicePixels(1),
        };
        let first_key = AtlasKey::Image(RenderImageParams {
            image_id: ImageId(1),
            frame_index: 0,
        });
        let second_key = AtlasKey::Image(RenderImageParams {
            image_id: ImageId(2),
            frame_index: 0,
        });
        let mut build = || Ok(Some((size, Cow::Owned(vec![0, 0, 0, 255]))));

        let first = atlas
            .get_or_insert_with(first_key, &mut build)?
            .context("first tile should be created")?;
        assert!(atlas.get_texture_info(first.texture_id).is_some());

        let epoch_before_clear = atlas.current_epoch();
        atlas.clear();
        assert_ne!(atlas.current_epoch(), epoch_before_clear);
        assert!(atlas.get_texture_info(first.texture_id).is_none());
        let second = atlas
            .get_or_insert_with(second_key, &mut build)?
            .context("second tile should be created")?;
        assert_ne!(first.texture_id, second.texture_id);
        assert_ne!(first.tile_id, second.tile_id);
        assert!(atlas.get_texture_info(second.texture_id).is_some());
        Ok(())
    }

    #[test]
    fn device_lost_rebinds_without_reusing_atlas_identity() -> anyhow::Result<()> {
        let initial_context = WgpuContext::new_headless(false)?;
        let atlas = WgpuAtlas::from_context(&initial_context);
        let size = Size {
            width: DevicePixels(1),
            height: DevicePixels(1),
        };
        let first_key = AtlasKey::Image(RenderImageParams {
            image_id: ImageId(1),
            frame_index: 0,
        });
        let second_key = AtlasKey::Image(RenderImageParams {
            image_id: ImageId(2),
            frame_index: 0,
        });
        let mut build = || Ok(Some((size, Cow::Owned(vec![0, 0, 0, 255]))));

        let first = atlas
            .get_or_insert_with(first_key, &mut build)?
            .context("first tile should be created")?;
        assert!(atlas.get_texture_info(first.texture_id).is_some());

        let epoch_before_recovery = atlas.current_epoch();
        initial_context.device.destroy();
        let recovered_context = WgpuContext::new_headless(false)?;
        atlas.handle_device_lost(&recovered_context);

        assert_ne!(atlas.current_epoch(), epoch_before_recovery);
        assert!(atlas.get_texture_info(first.texture_id).is_none());
        assert_eq!(atlas.snapshot().pending_uploads, 0);

        let second = atlas
            .get_or_insert_with(second_key, &mut build)?
            .context("second tile should be created")?;
        assert_ne!(first.texture_id, second.texture_id);
        assert_ne!(first.tile_id, second.tile_id);
        atlas.flush_uploads();
        assert!(atlas.get_texture_info(second.texture_id).is_some());
        Ok(())
    }

    #[test]
    fn remove_keeps_tile_space_as_a_tombstone() -> anyhow::Result<()> {
        let (device, queue) = test_device_and_queue()?;
        let atlas = WgpuAtlas::new(device, queue, wgpu::TextureFormat::Bgra8Unorm);

        let small = Size {
            width: DevicePixels(64),
            height: DevicePixels(64),
        };
        let big = Size {
            width: DevicePixels(700),
            height: DevicePixels(700),
        };

        let make_key = |image_id: usize| {
            AtlasKey::Image(RenderImageParams {
                image_id: ImageId(image_id),
                frame_index: 0,
            })
        };
        let insert = |key: AtlasKey, size: Size<DevicePixels>| {
            let byte_count = (size.width.0 as usize) * (size.height.0 as usize) * 4;
            atlas
                .get_or_insert_with(key, &mut || {
                    Ok(Some((size, Cow::Owned(vec![0u8; byte_count]))))
                })
                .expect("allocation should succeed")
                .expect("callback returns Some")
        };

        let keeper_key = make_key(1);
        let big_key_a = make_key(2);
        let big_key_b = make_key(3);

        let keeper_tile = insert(keeper_key, small);
        let tile_a = insert(big_key_a.clone(), big);
        assert_eq!(keeper_tile.texture_id, tile_a.texture_id);
        complete_frame(&atlas, [keeper_tile, tile_a]);

        atlas.remove(&big_key_a);
        let frame = atlas.begin_frame();
        let tile_b = insert(big_key_b, big);
        assert_ne!(tile_b.texture_id, keeper_tile.texture_id);
        assert_ne!(tile_b.tile_id, tile_a.tile_id);
        atlas.finish_frame(frame, &usage([keeper_tile, tile_b]));
        Ok(())
    }

    #[test]
    fn swizzle_upload_data_preserves_bgra_uploads() {
        let input = vec![0x10, 0x20, 0x30, 0x40];
        assert_eq!(
            swizzle_upload_data(&input, wgpu::TextureFormat::Bgra8Unorm),
            input
        );
    }

    #[test]
    fn swizzle_upload_data_converts_bgra_to_rgba() {
        let input = vec![0x10, 0x20, 0x30, 0x40, 0xAA, 0xBB, 0xCC, 0xDD];
        assert_eq!(
            swizzle_upload_data(&input, wgpu::TextureFormat::Rgba8Unorm),
            vec![0x30, 0x20, 0x10, 0x40, 0xCC, 0xBB, 0xAA, 0xDD]
        );
    }
}
