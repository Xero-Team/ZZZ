use anyhow::{Context as _, Result};
use collections::FxHashMap;
use gpui::{
    Atlas, AtlasBackend, AtlasEpoch, AtlasFrame, AtlasKey, AtlasSnapshot, AtlasTextureDescriptor,
    AtlasTextureId, AtlasTextureKind, AtlasTile, AtlasUpload, AtlasUsage, Bounds, DevicePixels,
    PlatformAtlas, Size,
};
use objc2::rc::Retained;
use objc2_metal::{
    MTLDevice, MTLPixelFormat, MTLRegion, MTLStorageMode, MTLTexture, MTLTextureDescriptor,
    MTLTextureUsage,
};
use std::{borrow::Cow, ptr::NonNull};

/// Sprite atlas backed by Metal textures.
pub struct MetalAtlas(Atlas<MetalAtlasTextures>);

impl MetalAtlas {
    pub fn new(
        device: Retained<objc2::runtime::ProtocolObject<dyn MTLDevice>>,
        supports_shared_storage: bool,
    ) -> Self {
        const MAX_ATLAS_SIZE: Size<DevicePixels> = Size {
            width: DevicePixels(16384),
            height: DevicePixels(16384),
        };
        Self(Atlas::new(
            MetalAtlasTextures {
                device,
                supports_shared_storage,
                textures: FxHashMap::default(),
                upload_calls: 0,
                uploaded_bytes: 0,
            },
            MAX_ATLAS_SIZE,
        ))
    }

    /// Returns the GPU texture backing `id`, or `None` once every tile in it
    /// has been removed. A scene can still reference such a texture when a
    /// cached view replays a paint from before the image was dropped, so
    /// callers must skip those sprites rather than assume the texture exists.
    pub fn metal_texture(
        &self,
        id: AtlasTextureId,
    ) -> Option<Retained<objc2::runtime::ProtocolObject<dyn MTLTexture>>> {
        self.0.with_backend(|textures| {
            textures
                .textures
                .get(&id)
                .map(|texture| texture.metal_texture.clone())
        })
    }
}

struct MetalAtlasTextures {
    device: Retained<objc2::runtime::ProtocolObject<dyn MTLDevice>>,
    supports_shared_storage: bool,
    textures: FxHashMap<AtlasTextureId, MetalAtlasTexture>,
    upload_calls: u64,
    uploaded_bytes: u64,
}

impl PlatformAtlas for MetalAtlas {
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

impl AtlasBackend for MetalAtlasTextures {
    fn create_texture(&mut self, descriptor: AtlasTextureDescriptor) -> Result<()> {
        anyhow::ensure!(
            !self.textures.contains_key(&descriptor.texture_id),
            "atlas texture identity was reused"
        );
        let texture_descriptor = MTLTextureDescriptor::new();
        // SAFETY: Atlas pages are clamped to the 1024..=16384 size limits.
        unsafe {
            texture_descriptor.setWidth(descriptor.size.width.0 as usize);
            texture_descriptor.setHeight(descriptor.size.height.0 as usize);
        }
        let pixel_format = match descriptor.kind {
            AtlasTextureKind::Monochrome => MTLPixelFormat::A8Unorm,
            AtlasTextureKind::Polychrome => MTLPixelFormat::BGRA8Unorm,
            AtlasTextureKind::Subpixel => {
                anyhow::bail!("Metal does not support subpixel atlas textures")
            }
        };
        texture_descriptor.setPixelFormat(pixel_format);
        texture_descriptor.setUsage(MTLTextureUsage::ShaderRead);
        // Shared memory mode can be used only on Apple GPU families
        // https://developer.apple.com/documentation/metal/mtlresourceoptions/storagemodeshared
        texture_descriptor.setStorageMode(if self.supports_shared_storage {
            MTLStorageMode::Shared
        } else {
            MTLStorageMode::Managed
        });
        let metal_texture = self
            .device
            .newTextureWithDescriptor(&texture_descriptor)
            .context("allocating atlas texture")?;
        self.textures
            .insert(descriptor.texture_id, MetalAtlasTexture { metal_texture });
        Ok(())
    }

    fn upload(&mut self, upload: AtlasUpload<'_>) -> Result<()> {
        let texture = self
            .textures
            .get(&upload.texture_id)
            .context("atlas upload refers to a missing texture")?;
        texture.upload(upload.bounds, upload.bytes)?;
        self.upload_calls = self.upload_calls.saturating_add(1);
        self.uploaded_bytes = self
            .uploaded_bytes
            .saturating_add(u64::try_from(upload.bytes.len()).unwrap_or(u64::MAX));
        Ok(())
    }

    fn destroy_texture(&mut self, texture_id: AtlasTextureId) {
        self.textures.remove(&texture_id);
    }

    fn clear_textures(&mut self) {
        self.textures.clear();
    }

    fn snapshot(&self) -> AtlasSnapshot {
        AtlasSnapshot {
            upload_calls: self.upload_calls,
            uploaded_bytes: self.uploaded_bytes,
            ..AtlasSnapshot::default()
        }
    }
}

struct MetalAtlasTexture {
    metal_texture: Retained<objc2::runtime::ProtocolObject<dyn MTLTexture>>,
}

impl MetalAtlasTexture {
    fn upload(&self, bounds: Bounds<DevicePixels>, bytes: &[u8]) -> Result<()> {
        let bytes_per_pixel = self.bytes_per_pixel();
        let bytes_per_row = (bounds.size.width.0 as usize)
            .checked_mul(bytes_per_pixel.into())
            .context("calculating atlas row byte count")?;
        let byte_count = bytes_per_row
            .checked_mul(bounds.size.height.0 as usize)
            .context("calculating atlas byte count")?;
        anyhow::ensure!(
            bytes.len() >= byte_count,
            "atlas upload byte length is {}, expected at least {byte_count}",
            bytes.len()
        );
        anyhow::ensure!(
            bounds.origin.x.0 >= 0
                && bounds.origin.y.0 >= 0
                && (bounds.origin.x.0 as usize)
                    .checked_add(bounds.size.width.0 as usize)
                    .is_some_and(|end| end <= self.metal_texture.width())
                && (bounds.origin.y.0 as usize)
                    .checked_add(bounds.size.height.0 as usize)
                    .is_some_and(|end| end <= self.metal_texture.height()),
            "atlas upload is outside texture bounds"
        );

        let region = MTLRegion {
            origin: objc2_metal::MTLOrigin {
                x: bounds.origin.x.0 as usize,
                y: bounds.origin.y.0 as usize,
                z: 0,
            },
            size: objc2_metal::MTLSize {
                width: bounds.size.width.0 as usize,
                height: bounds.size.height.0 as usize,
                depth: 1,
            },
        };
        // SAFETY: Validated bytes cover every row of the checked region in this CPU-accessible texture.
        // The borrowed bytes remain live for the copy; GPU ordering is a separate caller requirement.
        unsafe {
            self.metal_texture
                .replaceRegion_mipmapLevel_withBytes_bytesPerRow(
                    region,
                    0,
                    NonNull::from(bytes).cast(),
                    bytes_per_row,
                );
        }
        Ok(())
    }

    fn bytes_per_pixel(&self) -> u8 {
        match self.metal_texture.pixelFormat() {
            MTLPixelFormat::A8Unorm | MTLPixelFormat::R8Unorm => 1,
            MTLPixelFormat::RGBA8Unorm | MTLPixelFormat::BGRA8Unorm => 4,
            pixel_format => {
                log::error!("unexpected atlas pixel format {pixel_format:?}; assuming 32 bits");
                4
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{AtlasContentKind, PlatformAtlas, point, size};
    use objc2_metal::{MTLCreateSystemDefaultDevice, MTLGPUFamily};
    use std::borrow::Cow;

    fn create_atlas() -> Option<MetalAtlas> {
        let device = MTLCreateSystemDefaultDevice()?;
        let supports_shared_storage =
            cfg!(target_os = "ios") || device.supportsFamily(MTLGPUFamily::Apple1);
        Some(MetalAtlas::new(device, supports_shared_storage))
    }

    fn make_image_key(image_id: usize, frame_index: usize) -> AtlasKey {
        AtlasKey::Image(gpui::RenderImageParams {
            image_id: gpui::ImageId(image_id),
            frame_index,
        })
    }

    fn insert_tile(atlas: &MetalAtlas, key: AtlasKey, size: Size<DevicePixels>) -> AtlasTile {
        atlas
            .get_or_insert_with(key, &mut || {
                let byte_count = (size.width.0 as usize) * (size.height.0 as usize) * 4;
                Ok(Some((size, Cow::Owned(vec![0u8; byte_count]))))
            })
            .expect("allocation should succeed")
            .expect("callback returns Some")
    }

    fn usage(tiles: impl IntoIterator<Item = AtlasTile>) -> AtlasUsage {
        let mut usage = AtlasUsage::default();
        for tile in tiles {
            usage.insert(tile);
        }
        usage
    }

    fn complete_frame(atlas: &MetalAtlas, tiles: impl IntoIterator<Item = AtlasTile>) {
        let frame = atlas.begin_frame();
        atlas.finish_frame(frame, &usage(tiles));
    }

    #[test]
    fn test_remove_retires_keys_at_frame_boundary() {
        let Some(atlas) = create_atlas() else {
            return;
        };

        let small = Size {
            width: DevicePixels(64),
            height: DevicePixels(64),
        };

        let key_a = make_image_key(1, 0);
        let key_b = make_image_key(2, 0);
        let key_c = make_image_key(3, 0);

        let tile_a = insert_tile(&atlas, key_a.clone(), small);
        let tile_b = insert_tile(&atlas, key_b.clone(), small);
        let tile_c = insert_tile(&atlas, key_c.clone(), small);

        assert_eq!(tile_a.texture_id, tile_b.texture_id);
        assert_eq!(tile_b.texture_id, tile_c.texture_id);
        complete_frame(&atlas, [tile_a, tile_b, tile_c]);

        // Removal requests remain pending until the next frame boundary.
        atlas.remove(&key_a);
        atlas.remove(&key_b);
        atlas.remove(&key_c);
        let frame = atlas.begin_frame();
        assert!(atlas.metal_texture(tile_a.texture_id).is_some());

        // Re-inserting A must allocate a fresh tile identity without reusing
        // the retired region still referenced by the previous scene.
        let tile_a2 = insert_tile(&atlas, key_a, small);
        assert_ne!(tile_a.tile_id, tile_a2.tile_id);
        assert_ne!(
            (tile_a.texture_id, tile_a.bounds),
            (tile_a2.texture_id, tile_a2.bounds)
        );
        atlas.finish_frame(frame, &usage([tile_a2]));

        assert!(atlas.metal_texture(tile_a2.texture_id).is_some());
    }

    #[test]
    fn test_metal_texture_is_none_after_last_tile_removed() {
        let Some(atlas) = create_atlas() else {
            return;
        };

        let key = make_image_key(1, 0);
        let tile = insert_tile(
            &atlas,
            key.clone(),
            Size {
                width: DevicePixels(64),
                height: DevicePixels(64),
            },
        );
        assert!(atlas.metal_texture(tile.texture_id).is_some());
        complete_frame(&atlas, [tile]);

        atlas.remove(&key);
        let frame = atlas.begin_frame();
        assert!(atlas.metal_texture(tile.texture_id).is_some());
        atlas.finish_frame(frame, &AtlasUsage::default());
        assert!(atlas.metal_texture(tile.texture_id).is_none());
    }

    #[test]
    fn test_removed_tile_space_remains_tombstoned() {
        let Some(atlas) = create_atlas() else {
            return;
        };

        let small = Size {
            width: DevicePixels(64),
            height: DevicePixels(64),
        };
        let big = Size {
            width: DevicePixels(700),
            height: DevicePixels(700),
        };

        let keeper_key = make_image_key(1, 0);
        let big_key_a = make_image_key(2, 0);
        let big_key_b = make_image_key(3, 0);

        let keeper_tile = insert_tile(&atlas, keeper_key, small);
        let tile_a = insert_tile(&atlas, big_key_a.clone(), big);
        assert_eq!(keeper_tile.texture_id, tile_a.texture_id);
        complete_frame(&atlas, [keeper_tile, tile_a]);

        atlas.remove(&big_key_a);
        let frame = atlas.begin_frame();
        let tile_b = insert_tile(&atlas, big_key_b, big);
        assert_ne!(tile_b.texture_id, keeper_tile.texture_id);
        assert_ne!(tile_b.tile_id, tile_a.tile_id);
        atlas.finish_frame(frame, &usage([keeper_tile, tile_b]));
    }

    #[test]
    fn test_remove_nonexistent_key_is_noop() {
        let Some(atlas) = create_atlas() else {
            return;
        };
        let key = make_image_key(999, 0);
        atlas.remove(&key);
    }

    #[test]
    fn test_texture_upload_preserves_pixel_layout() -> Result<()> {
        let Some(atlas) = create_atlas() else {
            return Ok(());
        };
        for (kind, bytes_per_pixel) in [
            (AtlasTextureKind::Monochrome, 1),
            (AtlasTextureKind::Polychrome, 4),
        ] {
            let texture_id = AtlasTextureId { index: 0, kind };
            atlas.0.update_backend(|backend| {
                backend.create_texture(AtlasTextureDescriptor {
                    texture_id,
                    size: size(DevicePixels(8), DevicePixels(8)),
                    content_kind: AtlasContentKind::Image,
                    kind,
                })
            })?;
            atlas.0.update_backend(|backend| {
                backend.upload(AtlasUpload {
                    texture_id,
                    bounds: Bounds {
                        origin: point(DevicePixels(0), DevicePixels(0)),
                        size: size(DevicePixels(8), DevicePixels(8)),
                    },
                    bytes: &vec![0; 8 * 8 * bytes_per_pixel],
                })
            })?;
            let pixels = (1..=3 * 2 * bytes_per_pixel)
                .map(|value| value as u8)
                .collect::<Vec<_>>();
            atlas.0.update_backend(|backend| {
                backend.upload(AtlasUpload {
                    texture_id,
                    bounds: Bounds {
                        origin: point(DevicePixels(2), DevicePixels(3)),
                        size: size(DevicePixels(3), DevicePixels(2)),
                    },
                    bytes: &pixels,
                })
            })?;

            let texture = atlas
                .metal_texture(texture_id)
                .context("looking up created atlas texture")?;
            let mut actual = vec![0; 8 * 8 * bytes_per_pixel];
            // SAFETY: `actual` is large enough for the whole 8x8 texture at this row stride.
            unsafe {
                texture.getBytes_bytesPerRow_fromRegion_mipmapLevel(
                    NonNull::from(actual.as_mut_slice()).cast(),
                    8 * bytes_per_pixel,
                    MTLRegion {
                        origin: objc2_metal::MTLOrigin { x: 0, y: 0, z: 0 },
                        size: objc2_metal::MTLSize {
                            width: 8,
                            height: 8,
                            depth: 1,
                        },
                    },
                    0,
                );
            }
            let mut expected = vec![0; actual.len()];
            for row in 0..2 {
                let start = ((row + 3) * 8 + 2) * bytes_per_pixel;
                let source = row * 3 * bytes_per_pixel;
                expected[start..start + 3 * bytes_per_pixel]
                    .copy_from_slice(&pixels[source..source + 3 * bytes_per_pixel]);
            }
            assert_eq!(actual, expected);
            atlas
                .0
                .update_backend(|backend| backend.destroy_texture(texture_id));
        }
        Ok(())
    }
}
