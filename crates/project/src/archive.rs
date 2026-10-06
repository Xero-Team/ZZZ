use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context as _, Result};
use clock::ReplicaId;
use gpui::{App, AppContext as _, Context, Entity, Task};
use language::{Buffer, Capability, DiskState, File as LanguageFile, LanguageRegistry};
use text::{BufferId, LineEnding};
use util::{paths::PathStyle, rel_path::RelPath};
use vfs::{ProviderPath, ResourceId, VfsPath, VfsSnapshot, provider_path_to_legacy_utf8};
use worktree::WorktreeId;

use crate::Project;
use crate::image_store::{ImageItem, ImageMetadata, create_gpui_image};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Whether a file type opens as an archive automatically or only by explicit action.
pub enum ArchiveOpenPolicy {
    Automatic,
    ExplicitOnly,
    Unsupported,
}

/// Classifies ZIP-compatible extensions without overriding dedicated document viewers.
pub fn archive_open_policy(path: &Path) -> ArchiveOpenPolicy {
    let Some(extension) = path.extension().and_then(|extension| extension.to_str()) else {
        return ArchiveOpenPolicy::Unsupported;
    };
    if ["zip", "jar", "war", "apk", "aab", "whl", "vsix"]
        .iter()
        .any(|candidate| extension.eq_ignore_ascii_case(candidate))
    {
        ArchiveOpenPolicy::Automatic
    } else if ["docx", "xlsx", "pptx", "odt", "ods", "odp", "epub"]
        .iter()
        .any(|candidate| extension.eq_ignore_ascii_case(candidate))
    {
        ArchiveOpenPolicy::ExplicitOnly
    } else {
        ArchiveOpenPolicy::Unsupported
    }
}

#[derive(Clone)]
pub struct ArchiveMemberFile {
    virtual_worktree_id: WorktreeId,
    source_display_path: PathBuf,
    path: Arc<RelPath>,
    file_name: Arc<str>,
    size: u64,
    resource_id: ResourceId,
    vfs_path: VfsPath,
}

impl LanguageFile for ArchiveMemberFile {
    fn resource_id(&self) -> Option<ResourceId> {
        Some(self.resource_id)
    }

    fn vfs_path(&self) -> Option<&VfsPath> {
        Some(&self.vfs_path)
    }

    fn as_local(&self) -> Option<&dyn language::LocalFile> {
        None
    }

    fn disk_state(&self) -> DiskState {
        DiskState::Present {
            mtime: fs::MTime::from_seconds_and_nanos(0, 0),
            size: self.size,
        }
    }

    fn path(&self) -> &Arc<RelPath> {
        &self.path
    }

    fn full_path(&self, _cx: &App) -> PathBuf {
        self.source_display_path
            .join("!")
            .join(self.path.as_std_path())
    }

    fn path_style(&self, _cx: &App) -> PathStyle {
        PathStyle::Posix
    }

    fn file_name<'a>(&'a self, _cx: &'a App) -> &'a str {
        &self.file_name
    }

    fn worktree_id(&self, _cx: &App) -> WorktreeId {
        self.virtual_worktree_id
    }

    fn to_proto(&self, _cx: &App) -> rpc::proto::File {
        rpc::proto::File {
            worktree_id: self.virtual_worktree_id.to_proto(),
            entry_id: None,
            path: self.path.as_ref().to_proto(),
            mtime: None,
            is_deleted: false,
            is_historic: false,
            vfs_path: Some(rpc::proto::VfsPathV2::from_vfs_path(&self.vfs_path)),
            resource_id: Some(rpc::proto::ResourceIdV2::from_resource_id(self.resource_id)),
        }
    }

    fn is_private(&self) -> bool {
        false
    }

    fn can_open(&self) -> bool {
        true
    }
}

impl Project {
    pub fn list_archive_directory_page(
        &mut self,
        archive: VfsSnapshot,
        directory: ProviderPath,
        request: vfs::DirPageRequest,
        cx: &mut Context<Self>,
    ) -> Task<Result<vfs::DirPage>> {
        cx.background_spawn(async move {
            archive
                .provider()
                .read_dir(&directory, request)
                .await
                .map_err(Into::into)
        })
    }

    pub fn load_archive_member_bytes(
        &mut self,
        archive: VfsSnapshot,
        member_path: ProviderPath,
        cx: &mut Context<Self>,
    ) -> Task<Result<Vec<u8>>> {
        cx.background_spawn(async move {
            archive
                .load_bytes(&member_path, vfs::OperationContext::default())
                .await
                .map_err(Into::into)
        })
    }

    pub fn load_archive_image(
        &mut self,
        archive: VfsSnapshot,
        member_path: ProviderPath,
        cx: &mut Context<Self>,
    ) -> Task<Result<(Arc<gpui::Image>, ImageMetadata)>> {
        cx.background_spawn(async move {
            let bytes = archive
                .load_bytes(&member_path, vfs::OperationContext::default())
                .await?;
            let metadata = ImageItem::compute_metadata_from_bytes(&bytes)?;
            let image = create_gpui_image(bytes)?;
            Ok((image, metadata))
        })
    }

    pub fn open_archive_text_member(
        &mut self,
        archive: VfsSnapshot,
        source_display_path: PathBuf,
        member_path: ProviderPath,
        cx: &mut Context<Self>,
    ) -> Task<Result<Entity<Buffer>>> {
        let language_registry = self.languages().clone();
        cx.spawn(async move |this, cx| {
            let bytes = archive
                .load_bytes(&member_path, vfs::OperationContext::default())
                .await?;
            let size = u64::try_from(bytes.len()).context("archive member is too large")?;
            let mut text = String::from_utf8(bytes).context("archive member is not UTF-8 text")?;
            let line_ending = LineEnding::detect(&text);
            LineEnding::normalize(&mut text);
            let relative_path = provider_path_to_legacy_utf8(&member_path)?;
            let relative_path = RelPath::from_proto(&relative_path)?;
            let file_name = relative_path
                .file_name()
                .context("archive member has no file name")?
                .to_owned();
            let resource_id = archive
                .registry()
                .id_for_path(&member_path)
                .context("archive member has no resource identity")?;
            let vfs_path = VfsPath::new(archive.registry().mount_id(), member_path);
            let virtual_worktree_id = WorktreeId::from_proto(vfs_path.mount_id().get());
            let language = load_member_language(language_registry.clone(), file_name.clone()).await;

            let buffer = cx.update(|cx| {
                let reservation = cx.reserve_entity::<Buffer>();
                let buffer_id = BufferId::from(reservation.entity_id().as_non_zero_u64());
                let text_buffer = text::Buffer::new(ReplicaId::LOCAL, buffer_id, text);
                cx.insert_entity(reservation, |cx| {
                    let mut buffer = Buffer::build(
                        text_buffer,
                        Some(Arc::new(ArchiveMemberFile {
                            virtual_worktree_id,
                            source_display_path,
                            path: relative_path,
                            file_name: file_name.into(),
                            size,
                            resource_id,
                            vfs_path,
                        })),
                        Capability::Read,
                    );
                    buffer.set_language_registry(language_registry);
                    buffer.set_line_ending(line_ending, cx);
                    if let Some(language) = language {
                        buffer.set_language(Some(language), cx);
                    }
                    buffer
                })
            });
            this.update(cx, |project, cx| {
                project.buffer_store.update(cx, |buffer_store, cx| {
                    buffer_store.add_virtual_buffer(buffer.clone(), cx)
                })
            })??;
            Ok(buffer)
        })
    }
}

async fn load_member_language(
    language_registry: Arc<LanguageRegistry>,
    file_name: String,
) -> Option<Arc<language::Language>> {
    language_registry
        .load_language_for_file_path(Path::new(&file_name))
        .await
        .ok()
}
