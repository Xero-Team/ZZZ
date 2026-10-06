use anyhow::{Context as _, Result};
use collections::FxHashMap;
use derive_more::{Deref, DerefMut};
use gpui::{
    Atlas, AtlasBackend, AtlasEpoch, AtlasFrame, AtlasKey, AtlasSnapshot, AtlasTextureDescriptor,
    AtlasTextureId, AtlasTextureKind, AtlasTile, AtlasUpload, AtlasUsage, DevicePixels,
    PlatformAtlas, Size,
};
use metal::Device;
use std::borrow::Cow;

pub(crate) struct MetalAtlas(Atlas<MetalAtlasTextures>);

impl MetalAtlas {
    pub(crate) fn new(device: Device, is_apple_gpu: bool) -> Self {
        const MAX_ATLAS_SIZE: Size<DevicePixels> = Size {
            width: DevicePixels(16384),
            height: DevicePixels(16384),
        };
        Self(Atlas::new(
            MetalAtlasTextures {
                device: AssertSend(device),
                is_apple_gpu,
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
    pub(crate) fn metal_texture(&self, id: AtlasTextureId) -> Option<metal::Texture> {
        self.0.with_backend(|textures| {
            textures
                .textures
                .get(&id)
                .map(|texture| texture.metal_texture.clone())
        })
    }
}

struct MetalAtlasTextures {
    device: AssertSend<Device>,
    is_apple_gpu: bool,
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
        let texture_descriptor = metal::TextureDescriptor::new();
        texture_descriptor.set_width(descriptor.size.width.0 as u64);
        texture_descriptor.set_height(descriptor.size.height.0 as u64);
        let pixel_format = match descriptor.kind {
            AtlasTextureKind::Monochrome => metal::MTLPixelFormat::A8Unorm,
            AtlasTextureKind::Polychrome => metal::MTLPixelFormat::BGRA8Unorm,
            AtlasTextureKind::Subpixel => {
                anyhow::bail!("Metal does not support subpixel atlas textures")
            }
        };
        texture_descriptor.set_pixel_format(pixel_format);
        texture_descriptor.set_usage(metal::MTLTextureUsage::ShaderRead);
        texture_descriptor.set_storage_mode(if self.is_apple_gpu {
            metal::MTLStorageMode::Shared
        } else {
            metal::MTLStorageMode::Managed
        });
        let metal_texture = self.device.new_texture(&texture_descriptor);
        self.textures.insert(
            descriptor.texture_id,
            MetalAtlasTexture {
                metal_texture: AssertSend(metal_texture),
            },
        );
        Ok(())
    }

    fn upload(&mut self, upload: AtlasUpload<'_>) -> Result<()> {
        let texture = self
            .textures
            .get(&upload.texture_id)
            .context("atlas upload refers to a missing texture")?;
        texture.upload(upload.bounds, upload.bytes);
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
    metal_texture: AssertSend<metal::Texture>,
}

impl MetalAtlasTexture {
    fn upload(&self, bounds: gpui::Bounds<DevicePixels>, bytes: &[u8]) {
        let region = metal::MTLRegion::new_2d(
            bounds.origin.x.0 as u64,
            bounds.origin.y.0 as u64,
            bounds.size.width.0 as u64,
            bounds.size.height.0 as u64,
        );
        self.metal_texture.replace_region(
            region,
            0,
            bytes.as_ptr().cast(),
            bounds.size.width.to_bytes(self.bytes_per_pixel()) as u64,
        );
    }

    fn bytes_per_pixel(&self) -> u8 {
        use metal::MTLPixelFormat::{A8Unorm, BGRA8Unorm, R8Unorm, RGBA8Unorm};
        match self.metal_texture.pixel_format() {
            A8Unorm | R8Unorm => 1,
            RGBA8Unorm | BGRA8Unorm => 4,
            _ => unreachable!("atlas textures use only A8 or 32-bit color formats"),
        }
    }
}

#[derive(Deref, DerefMut)]
struct AssertSend<T>(T);

// SAFETY: Metal `Device` and `Texture` wrap thread-safe GPU objects; the
// `metal` crate types do not implement `Send`, so this newtype asserts it.
#[allow(
    clippy::non_send_fields_in_send_ty,
    reason = "Metal Device and Texture are thread-safe GPU objects; the metal crate types do not implement Send"
)]
unsafe impl<T> Send for AssertSend<T> {}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::PlatformAtlas;
    use std::borrow::Cow;

    fn create_atlas() -> Option<MetalAtlas> {
        let device = metal::Device::system_default()?;
        Some(MetalAtlas::new(device, true))
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
    fn test_remove_clears_stale_keys_from_tiles_by_key() {
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
    fn test_remove_deallocates_tile_space_for_reuse() {
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
}
