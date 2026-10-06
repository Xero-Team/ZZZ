use anyhow::{Context as _, Result};
use collections::FxHashMap;
use gpui::{
    Atlas, AtlasBackend, AtlasKey, AtlasSnapshot, AtlasTextureDescriptor, AtlasTextureId,
    AtlasTextureKind, AtlasTile, AtlasUpload, DevicePixels, PlatformAtlas, Size,
};
use windows::Win32::Graphics::{
    Direct3D11::{
        D3D11_BIND_SHADER_RESOURCE, D3D11_BOX, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT,
        ID3D11Device, ID3D11DeviceContext, ID3D11ShaderResourceView, ID3D11Texture2D,
    },
    Dxgi::Common::*,
};

const MAX_ATLAS_SIZE: Size<DevicePixels> = Size {
    width: DevicePixels(16384),
    height: DevicePixels(16384),
};

pub(crate) struct DirectXAtlas(Atlas<DirectXAtlasTextures>);

struct DirectXAtlasTextures {
    device: ID3D11Device,
    device_context: ID3D11DeviceContext,
    textures: FxHashMap<AtlasTextureId, DirectXAtlasTexture>,
    upload_calls: u64,
    uploaded_bytes: u64,
}

struct DirectXAtlasTexture {
    bytes_per_pixel: u32,
    texture: ID3D11Texture2D,
    view: [Option<ID3D11ShaderResourceView>; 1],
}

impl DirectXAtlas {
    pub(crate) fn new(device: &ID3D11Device, device_context: &ID3D11DeviceContext) -> Self {
        Self(Atlas::new(
            DirectXAtlasTextures {
                device: device.clone(),
                device_context: device_context.clone(),
                textures: FxHashMap::default(),
                upload_calls: 0,
                uploaded_bytes: 0,
            },
            MAX_ATLAS_SIZE,
        ))
    }

    /// Returns the view backing `id`, or `None` once every tile in it has been
    /// removed. A scene can still reference such a texture when a cached view
    /// replays a paint from before the image was dropped, so callers must skip
    /// those sprites rather than assume the texture exists.
    pub(crate) fn get_texture_view(
        &self,
        id: AtlasTextureId,
    ) -> Option<[Option<ID3D11ShaderResourceView>; 1]> {
        self.0.with_backend(|textures| {
            textures
                .textures
                .get(&id)
                .map(|texture| texture.view.clone())
        })
    }

    pub(crate) fn handle_device_lost(
        &self,
        device: &ID3D11Device,
        device_context: &ID3D11DeviceContext,
    ) {
        self.0.reset(MAX_ATLAS_SIZE, |textures| {
            textures.device = device.clone();
            textures.device_context = device_context.clone();
        });
    }
}

impl PlatformAtlas for DirectXAtlas {
    fn get_or_insert_with<'a>(
        &self,
        key: AtlasKey,
        build: &mut dyn FnMut() -> Result<Option<(Size<DevicePixels>, std::borrow::Cow<'a, [u8]>)>>,
    ) -> Result<Option<AtlasTile>> {
        self.0.get_or_insert_with(key, build)
    }

    fn remove(&self, key: &AtlasKey) {
        self.0.remove(key);
    }

    fn snapshot(&self) -> AtlasSnapshot {
        self.0.snapshot()
    }
}

impl AtlasBackend for DirectXAtlasTextures {
    fn create_texture(&mut self, descriptor: AtlasTextureDescriptor) -> Result<()> {
        anyhow::ensure!(
            !self.textures.contains_key(&descriptor.texture_id),
            "atlas texture identity was reused"
        );
        let (pixel_format, bytes_per_pixel) = match descriptor.kind {
            AtlasTextureKind::Monochrome => (DXGI_FORMAT_R8_UNORM, 1),
            AtlasTextureKind::Polychrome => (DXGI_FORMAT_B8G8R8A8_UNORM, 4),
            AtlasTextureKind::Subpixel => (DXGI_FORMAT_R8G8B8A8_UNORM, 4),
        };
        let texture_descriptor = D3D11_TEXTURE2D_DESC {
            Width: descriptor.size.width.0 as u32,
            Height: descriptor.size.height.0 as u32,
            MipLevels: 1,
            ArraySize: 1,
            Format: pixel_format,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
            CPUAccessFlags: 0,
            MiscFlags: 0,
        };
        let mut texture = None;
        unsafe {
            self.device
                .CreateTexture2D(&texture_descriptor, None, Some(&mut texture))
        }
        .map_err(|error| anyhow::anyhow!("failed to create atlas texture: {error}"))?;
        let texture = texture.context("DirectX returned no atlas texture")?;
        let mut view = None;
        unsafe {
            self.device
                .CreateShaderResourceView(&texture, None, Some(&mut view))
        }
        .map_err(|error| anyhow::anyhow!("failed to create atlas texture view: {error}"))?;
        self.textures.insert(
            descriptor.texture_id,
            DirectXAtlasTexture {
                bytes_per_pixel,
                texture,
                view: [view],
            },
        );
        Ok(())
    }

    fn upload(&mut self, upload: AtlasUpload<'_>) -> Result<()> {
        let texture = self
            .textures
            .get(&upload.texture_id)
            .context("atlas upload refers to a missing texture")?;
        let uploaded_bytes = texture.upload(&self.device_context, upload.bounds, upload.bytes)?;
        self.upload_calls = self.upload_calls.saturating_add(1);
        self.uploaded_bytes = self
            .uploaded_bytes
            .saturating_add(u64::try_from(uploaded_bytes).unwrap_or(u64::MAX));
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

impl DirectXAtlasTexture {
    fn upload(
        &self,
        device_context: &ID3D11DeviceContext,
        bounds: gpui::Bounds<DevicePixels>,
        bytes: &[u8],
    ) -> Result<usize> {
        let row_bytes = bounds.size.width.to_bytes(self.bytes_per_pixel as u8) as usize;
        let expected = row_bytes.saturating_mul(bounds.size.height.0.max(0) as usize);
        anyhow::ensure!(
            bytes.len() >= expected,
            "DirectX atlas upload source has {} bytes but the {}x{} region requires {} bytes",
            bytes.len(),
            bounds.size.width.0,
            bounds.size.height.0,
            expected,
        );
        unsafe {
            device_context.UpdateSubresource(
                &self.texture,
                0,
                Some(&D3D11_BOX {
                    left: bounds.left().0 as u32,
                    top: bounds.top().0 as u32,
                    front: 0,
                    right: bounds.right().0 as u32,
                    bottom: bounds.bottom().0 as u32,
                    back: 1,
                }),
                bytes.as_ptr().cast(),
                bounds.size.width.to_bytes(self.bytes_per_pixel as u8),
                0,
            );
        }
        Ok(expected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{ImageId, RenderImageParams};
    use std::borrow::Cow;
    use windows::Win32::{
        Foundation::HMODULE,
        Graphics::{
            Direct3D::D3D_DRIVER_TYPE_WARP,
            Direct3D11::{D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11CreateDevice},
        },
    };

    fn create_atlas() -> Option<DirectXAtlas> {
        let mut device: Option<ID3D11Device> = None;
        let mut device_context: Option<ID3D11DeviceContext> = None;
        unsafe {
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_WARP,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut device_context),
            )
        }
        .ok()?;
        Some(DirectXAtlas::new(&device?, &device_context?))
    }

    fn make_image_key(image_id: usize) -> AtlasKey {
        AtlasKey::Image(RenderImageParams {
            image_id: ImageId(image_id),
            frame_index: 0,
        })
    }

    fn insert_tile(atlas: &DirectXAtlas, key: AtlasKey, size: Size<DevicePixels>) -> AtlasTile {
        atlas
            .get_or_insert_with(key, &mut || {
                let byte_count = (size.width.0 as usize) * (size.height.0 as usize) * 4;
                Ok(Some((size, Cow::Owned(vec![0u8; byte_count]))))
            })
            .expect("allocation should succeed")
            .expect("callback returns Some")
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

        let keeper_key = make_image_key(1);
        let big_key_a = make_image_key(2);
        let big_key_b = make_image_key(3);

        let keeper_tile = insert_tile(&atlas, keeper_key, small);
        let tile_a = insert_tile(&atlas, big_key_a.clone(), big);
        assert_eq!(keeper_tile.texture_id, tile_a.texture_id);

        atlas.remove(&big_key_a);

        let tile_b = insert_tile(&atlas, big_key_b, big);
        assert_eq!(tile_b.texture_id, keeper_tile.texture_id);
        assert_ne!(tile_b.tile_id, tile_a.tile_id);
    }
}
