use super::*;

impl WgpuRenderer {
    pub(super) fn draw_quads(
        &self,
        quads: &[Quad],
        instance_offset: &mut u64,
        pass: &mut wgpu::RenderPass<'_>,
    ) -> bool {
        // SAFETY: `Quad` has no padding bytes, as `instance_types_contain_no_padding` asserts, so
        // every byte of the slice is initialized.
        let data = unsafe { Self::instance_bytes(quads) };
        self.draw_instances(
            data,
            quads.len() as u32,
            &self.resources().pipelines.quads,
            instance_offset,
            pass,
        )
    }

    pub(super) fn draw_shadows(
        &self,
        shadows: &[Shadow],
        instance_offset: &mut u64,
        pass: &mut wgpu::RenderPass<'_>,
    ) -> bool {
        // SAFETY: `Shadow` has no padding bytes, as `instance_types_contain_no_padding` asserts, so
        // every byte of the slice is initialized.
        let data = unsafe { Self::instance_bytes(shadows) };
        self.draw_instances(
            data,
            shadows.len() as u32,
            &self.resources().pipelines.shadows,
            instance_offset,
            pass,
        )
    }

    pub(super) fn draw_underlines(
        &self,
        underlines: &[Underline],
        instance_offset: &mut u64,
        pass: &mut wgpu::RenderPass<'_>,
    ) -> bool {
        // SAFETY: `Underline` has no padding bytes, as `instance_types_contain_no_padding` asserts,
        // so every byte of the slice is initialized.
        let data = unsafe { Self::instance_bytes(underlines) };
        self.draw_instances(
            data,
            underlines.len() as u32,
            &self.resources().pipelines.underlines,
            instance_offset,
            pass,
        )
    }

    pub(super) fn draw_monochrome_sprites(
        &self,
        sprites: &[MonochromeSprite],
        texture_id: AtlasTextureId,
        instance_offset: &mut u64,
        pass: &mut wgpu::RenderPass<'_>,
    ) -> bool {
        let Some(tex_info) = self.atlas.get_texture_info(texture_id) else {
            return true;
        };
        // SAFETY: `MonochromeSprite` has no padding bytes, as
        // `instance_types_contain_no_padding` asserts, so every byte of the slice is initialized.
        let data = unsafe { Self::instance_bytes(sprites) };
        self.draw_instances_with_texture(
            data,
            sprites.len() as u32,
            &tex_info.view,
            &self.resources().pipelines.mono_sprites,
            instance_offset,
            pass,
        )
    }

    pub(super) fn draw_subpixel_sprites(
        &self,
        sprites: &[SubpixelSprite],
        texture_id: AtlasTextureId,
        instance_offset: &mut u64,
        pass: &mut wgpu::RenderPass<'_>,
    ) -> bool {
        let Some(tex_info) = self.atlas.get_texture_info(texture_id) else {
            return true;
        };
        // SAFETY: `SubpixelSprite` has no padding bytes, as `instance_types_contain_no_padding`
        // asserts, so every byte of the slice is initialized.
        let data = unsafe { Self::instance_bytes(sprites) };
        let resources = self.resources();
        let pipeline = resources
            .pipelines
            .subpixel_sprites
            .as_ref()
            .unwrap_or(&resources.pipelines.mono_sprites);
        self.draw_instances_with_texture(
            data,
            sprites.len() as u32,
            &tex_info.view,
            pipeline,
            instance_offset,
            pass,
        )
    }

    pub(super) fn draw_polychrome_sprites(
        &self,
        sprites: &[PolychromeSprite],
        texture_id: AtlasTextureId,
        instance_offset: &mut u64,
        pass: &mut wgpu::RenderPass<'_>,
    ) -> bool {
        let Some(tex_info) = self.atlas.get_texture_info(texture_id) else {
            return true;
        };
        // SAFETY: `PolychromeSprite` has no padding bytes, as `instance_types_contain_no_padding`
        // asserts, so every byte of the slice is initialized.
        let data = unsafe { Self::instance_bytes(sprites) };
        self.draw_instances_with_texture(
            data,
            sprites.len() as u32,
            &tex_info.view,
            &self.resources().pipelines.poly_sprites,
            instance_offset,
            pass,
        )
    }

    pub(super) fn draw_instances(
        &self,
        data: &[u8],
        instance_count: u32,
        pipeline: &wgpu::RenderPipeline,
        instance_offset: &mut u64,
        pass: &mut wgpu::RenderPass<'_>,
    ) -> bool {
        if instance_count == 0 {
            return true;
        }
        let Some((offset, size)) = self.write_to_instance_buffer(instance_offset, data) else {
            return false;
        };
        let resources = self.resources();
        let bind_group = resources
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &resources.bind_group_layouts.instances,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.instance_binding(offset, size),
                }],
            });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &resources.globals_bind_group, &[]);
        pass.set_bind_group(1, &bind_group, &[]);
        pass.draw(0..4, 0..instance_count);
        true
    }

    pub(super) fn draw_instances_with_texture(
        &self,
        data: &[u8],
        instance_count: u32,
        texture_view: &wgpu::TextureView,
        pipeline: &wgpu::RenderPipeline,
        instance_offset: &mut u64,
        pass: &mut wgpu::RenderPass<'_>,
    ) -> bool {
        if instance_count == 0 {
            return true;
        }
        let Some((offset, size)) = self.write_to_instance_buffer(instance_offset, data) else {
            return false;
        };
        let resources = self.resources();
        let bind_group = resources
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &resources.bind_group_layouts.instances_with_texture,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.instance_binding(offset, size),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(texture_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&resources.atlas_sampler),
                    },
                ],
            });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &resources.globals_bind_group, &[]);
        pass.set_bind_group(1, &bind_group, &[]);
        pass.draw(0..4, 0..instance_count);
        true
    }

    pub(super) fn draw_paths_from_intermediate(
        &self,
        paths: &[Path<ScaledPixels>],
        instance_offset: &mut u64,
        pass: &mut wgpu::RenderPass<'_>,
    ) -> bool {
        let first_path = &paths[0];
        let sprites: Vec<PathSprite> = if paths.last().map(|p| &p.order) == Some(&first_path.order)
        {
            paths
                .iter()
                .map(|p| PathSprite {
                    bounds: p.clipped_bounds(),
                })
                .collect()
        } else {
            let mut bounds = first_path.clipped_bounds();
            for path in paths.iter().skip(1) {
                bounds = bounds.union(&path.clipped_bounds());
            }
            vec![PathSprite { bounds }]
        };

        let resources = self.resources();
        let Some(path_intermediate_view) = resources.path_intermediate_view.as_ref() else {
            return true;
        };

        // SAFETY: `PathSprite` has no padding bytes, as `instance_types_contain_no_padding`
        // asserts, so every byte of the slice is initialized.
        let sprite_data = unsafe { Self::instance_bytes(&sprites) };
        self.draw_instances_with_texture(
            sprite_data,
            sprites.len() as u32,
            path_intermediate_view,
            &resources.pipelines.paths,
            instance_offset,
            pass,
        )
    }

    pub(super) fn draw_paths_to_intermediate(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        paths: &[Path<ScaledPixels>],
        instance_offset: &mut u64,
    ) -> bool {
        let mut vertices = Vec::new();
        for path in paths {
            let bounds = path.clipped_bounds();
            vertices.extend(path.vertices.iter().map(|v| PathRasterizationVertex {
                xy_position: v.xy_position,
                st_position: v.st_position,
                color: path.color,
                bounds,
            }));
        }

        if vertices.is_empty() {
            return true;
        }

        // SAFETY: `PathRasterizationVertex` has no padding bytes, as
        // `instance_types_contain_no_padding` asserts, so every byte of the slice is initialized.
        let vertex_data = unsafe { Self::instance_bytes(&vertices) };
        let Some((vertex_offset, vertex_size)) =
            self.write_to_instance_buffer(instance_offset, vertex_data)
        else {
            return false;
        };

        let resources = self.resources();
        let data_bind_group = resources
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("path_rasterization_bind_group"),
                layout: &resources.bind_group_layouts.instances,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.instance_binding(vertex_offset, vertex_size),
                }],
            });

        let Some(path_intermediate_view) = resources.path_intermediate_view.as_ref() else {
            return true;
        };

        let (target_view, resolve_target) = if let Some(ref msaa_view) = resources.path_msaa_view {
            (msaa_view, Some(path_intermediate_view))
        } else {
            (path_intermediate_view, None)
        };

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("path_rasterization_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target_view,
                    resolve_target,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            });

            pass.set_pipeline(&resources.pipelines.path_rasterization);
            pass.set_bind_group(0, &resources.path_globals_bind_group, &[]);
            pass.set_bind_group(1, &data_bind_group, &[]);
            pass.draw(0..vertices.len() as u32, 0..1);
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::size_of;

    /// Every instance type below reaches `WgpuRenderer::instance_bytes`, which reinterprets a
    /// `&[T]` as a `&[u8]`. That is only sound while all of the bytes of `T` are initialized, so a
    /// single byte of compiler-inserted padding would make the reinterpretation undefined
    /// behavior. A `#[repr(C)]` struct is padding free exactly when its size equals the sum of its
    /// fields' sizes, which is what this macro asserts.
    ///
    /// The fix for a failure is an explicit pad field (as `PaddedBool32` and the `pad: u32` fields
    /// in `gpui::scene` do), not an adjusted expectation.
    macro_rules! assert_no_padding {
        ($t:ty, $($field:ty),+ $(,)?) => {{
            let field_bytes = 0 $(+ size_of::<$field>())+;
            assert_eq!(
                size_of::<$t>(),
                field_bytes,
                concat!(
                    stringify!($t),
                    " has compiler-inserted padding, so reinterpreting it as &[u8] is undefined \
                     behavior. Add an explicit pad field instead.",
                ),
            );
        }};
    }

    #[test]
    fn instance_types_contain_no_padding() {
        // Leaked to the instance buffer by `drawing.rs`.
        assert_no_padding!(
            gpui::Quad,
            u32,
            gpui::BorderStyle,
            gpui::Bounds<gpui::ScaledPixels>,
            gpui::ContentMask<gpui::ScaledPixels>,
            gpui::Background,
            gpui::Hsla,
            gpui::Corners<gpui::ScaledPixels>,
            gpui::Edges<gpui::ScaledPixels>,
        );
        assert_no_padding!(
            gpui::Shadow,
            u32,
            gpui::ScaledPixels,
            gpui::Bounds<gpui::ScaledPixels>,
            gpui::Corners<gpui::ScaledPixels>,
            gpui::ContentMask<gpui::ScaledPixels>,
            gpui::Hsla,
        );
        assert_no_padding!(
            gpui::Underline,
            u32,
            u32,
            gpui::Bounds<gpui::ScaledPixels>,
            gpui::ContentMask<gpui::ScaledPixels>,
            gpui::Hsla,
            gpui::ScaledPixels,
            gpui::PaddedBool32,
        );
        assert_no_padding!(
            gpui::MonochromeSprite,
            u32,
            u32,
            gpui::AtlasTile,
            gpui::Bounds<gpui::ScaledPixels>,
            gpui::ContentMask<gpui::ScaledPixels>,
            gpui::Hsla,
            gpui::TransformationMatrix,
        );
        assert_no_padding!(
            gpui::SubpixelSprite,
            u32,
            u32,
            gpui::AtlasTile,
            gpui::Bounds<gpui::ScaledPixels>,
            gpui::ContentMask<gpui::ScaledPixels>,
            gpui::Hsla,
            gpui::TransformationMatrix,
        );
        assert_no_padding!(
            gpui::PolychromeSprite,
            u32,
            u32,
            gpui::AtlasTile,
            gpui::PaddedBool32,
            f32,
            gpui::Bounds<gpui::ScaledPixels>,
            gpui::ContentMask<gpui::ScaledPixels>,
            gpui::Corners<gpui::ScaledPixels>,
        );
        assert_no_padding!(
            gpui::PathVertex<gpui::ScaledPixels>,
            gpui::Point<gpui::ScaledPixels>,
            gpui::Point<f32>,
            gpui::ContentMask<gpui::ScaledPixels>,
        );
        assert_no_padding!(PathSprite, gpui::Bounds<gpui::ScaledPixels>);
        assert_no_padding!(
            PathRasterizationVertex,
            gpui::Point<gpui::ScaledPixels>,
            gpui::Point<f32>,
            gpui::Background,
            gpui::Bounds<gpui::ScaledPixels>,
        );

        // Nested types: their padding would be invisible to the assertions above, because those
        // sum `size_of` per field rather than the sizes of the fields' own fields.
        assert_no_padding!(
            gpui::Bounds<gpui::ScaledPixels>,
            gpui::Point<gpui::ScaledPixels>,
            gpui::Size<gpui::ScaledPixels>,
        );
        assert_no_padding!(
            gpui::ContentMask<gpui::ScaledPixels>,
            gpui::Bounds<gpui::ScaledPixels>,
        );
        assert_no_padding!(gpui::Hsla, f32, f32, f32, f32);
        assert_no_padding!(
            gpui::Corners<gpui::ScaledPixels>,
            gpui::ScaledPixels,
            gpui::ScaledPixels,
            gpui::ScaledPixels,
            gpui::ScaledPixels,
        );
        assert_no_padding!(
            gpui::Edges<gpui::ScaledPixels>,
            gpui::ScaledPixels,
            gpui::ScaledPixels,
            gpui::ScaledPixels,
            gpui::ScaledPixels,
        );
        assert_no_padding!(
            gpui::AtlasTile,
            gpui::AtlasTextureId,
            gpui::TileId,
            u32,
            gpui::Bounds<gpui::DevicePixels>,
        );
        assert_no_padding!(gpui::TransformationMatrix, [[f32; 2]; 2], [f32; 2],);
        assert_no_padding!(
            gpui::Background,
            u32,
            u32,
            gpui::Hsla,
            f32,
            gpui::LinearColorStop,
            gpui::LinearColorStop,
            u32,
        );
        assert_no_padding!(gpui::LinearColorStop, gpui::Hsla, f32);
    }
}
