use crate::{
    Project, ProjectEntryId, ProjectItem, ProjectPath,
    worktree_store::{WorktreeStore, WorktreeStoreEvent},
};
use anyhow::{Context as _, Result};
use collections::{HashMap, HashSet, hash_map};
use futures::{StreamExt, channel::oneshot};
use gpui::{
    App, AsyncApp, Context, Entity, EventEmitter, Img, Subscription, Task, WeakEntity, prelude::*,
};
pub use image::ImageFormat;
use image::{ExtendedColorType, GenericImageView, ImageReader};
use language::{DiskState, File};
use rpc::ErrorExt as _;
use std::num::NonZeroU64;
use std::path::PathBuf;
use std::sync::Arc;
use util::{ResultExt, rel_path::RelPath};
use vfs::ResourceId;
use worktree::{LoadedBinaryFile, PathChange, Worktree};

#[derive(Clone, Copy, Debug, Hash, PartialEq, PartialOrd, Ord, Eq)]
pub struct ImageId(NonZeroU64);

impl ImageId {
    pub fn to_proto(&self) -> u64 {
        self.0.get()
    }
}

impl std::fmt::Display for ImageId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<NonZeroU64> for ImageId {
    fn from(id: NonZeroU64) -> Self {
        ImageId(id)
    }
}

#[derive(Debug)]
pub enum ImageItemEvent {
    ReloadNeeded,
    Reloaded,
    FileHandleChanged,
    MetadataUpdated,
}

impl EventEmitter<ImageItemEvent> for ImageItem {}

pub enum ImageStoreEvent {
    ImageAdded(Entity<ImageItem>),
}

impl EventEmitter<ImageStoreEvent> for ImageStore {}

#[derive(Debug, Clone, Copy)]
pub struct ImageMetadata {
    pub width: u32,
    pub height: u32,
    pub file_size: u64,
    pub colors: Option<ImageColorInfo>,
    pub format: ImageFormat,
}

#[derive(Debug, Clone, Copy)]
pub struct ImageColorInfo {
    pub channels: u8,
    pub bits_per_channel: u8,
}

impl ImageColorInfo {
    pub fn from_color_type(color_type: impl Into<ExtendedColorType>) -> Option<Self> {
        let (channels, bits_per_channel) = match color_type.into() {
            ExtendedColorType::L8 => (1, 8),
            ExtendedColorType::L16 => (1, 16),
            ExtendedColorType::La8 => (2, 8),
            ExtendedColorType::La16 => (2, 16),
            ExtendedColorType::Rgb8 => (3, 8),
            ExtendedColorType::Rgb16 => (3, 16),
            ExtendedColorType::Rgba8 => (4, 8),
            ExtendedColorType::Rgba16 => (4, 16),
            ExtendedColorType::A8 => (1, 8),
            ExtendedColorType::Bgr8 => (3, 8),
            ExtendedColorType::Bgra8 => (4, 8),
            ExtendedColorType::Cmyk8 => (4, 8),
            _ => return None,
        };

        Some(Self {
            channels,
            bits_per_channel,
        })
    }

    pub const fn bits_per_pixel(&self) -> u8 {
        self.channels * self.bits_per_channel
    }
}

pub struct ImageItem {
    pub id: ImageId,
    pub file: Arc<worktree::File>,
    pub image: Arc<gpui::Image>,
    reload_task: Option<Task<()>>,
    pub image_metadata: Option<ImageMetadata>,
}

impl ImageItem {
    pub fn compute_metadata_from_bytes(image_bytes: &[u8]) -> Result<ImageMetadata> {
        let image_format = image::guess_format(image_bytes)?;

        let mut image_reader = ImageReader::new(std::io::Cursor::new(image_bytes));
        image_reader.set_format(image_format);
        let image = image_reader.decode()?;

        let (width, height) = image.dimensions();

        Ok(ImageMetadata {
            width,
            height,
            file_size: image_bytes.len() as u64,
            format: image_format,
            colors: ImageColorInfo::from_color_type(image.color()),
        })
    }

    pub async fn load_image_metadata(
        image: Entity<ImageItem>,
        cx: &mut AsyncApp,
    ) -> Result<ImageMetadata> {
        let load_image = cx.update(|cx| {
            let (worktree, path) = {
                let image = image.read(cx);
                (image.file.worktree.clone(), image.file.path.clone())
            };
            worktree.update(cx, |worktree, cx| worktree.load_binary_file(&path, cx))
        });
        let loaded = load_image.await?;
        Self::compute_metadata_from_bytes(&loaded.content)
    }

    pub fn project_path(&self, cx: &App) -> ProjectPath {
        ProjectPath {
            worktree_id: self.file.worktree_id(cx),
            path: self.file.path().clone(),
        }
    }

    pub fn abs_path(&self, cx: &App) -> Option<PathBuf> {
        Some(self.file.as_local()?.abs_path(cx))
    }

    fn file_updated(&mut self, new_file: Arc<worktree::File>, cx: &mut Context<Self>) {
        let mut file_changed = false;

        let old_file = &self.file;
        if new_file.path() != old_file.path() {
            file_changed = true;
        }

        let old_state = old_file.disk_state();
        let new_state = new_file.disk_state();
        if old_state != new_state {
            file_changed = true;
            if matches!(new_state, DiskState::Present { .. }) {
                cx.emit(ImageItemEvent::ReloadNeeded)
            }
        }

        self.file = new_file;
        if file_changed {
            cx.emit(ImageItemEvent::FileHandleChanged);
            cx.notify();
        }
    }

    fn reload(&mut self, cx: &mut Context<Self>) -> Option<oneshot::Receiver<()>> {
        let (tx, rx) = futures::channel::oneshot::channel();
        let file = self.file.clone();
        let content = file
            .worktree
            .update(cx, |worktree, cx| worktree.load_binary_file(&file.path, cx));
        self.reload_task = Some(cx.spawn(async move |this, cx| {
            if let Some((image, image_metadata)) = content
                .await
                .map(|loaded| loaded.content)
                .context("Failed to load image content")
                .and_then(|content| {
                    let image_metadata = Self::compute_metadata_from_bytes(&content).log_err();
                    create_gpui_image(content).map(|image| (image, image_metadata))
                })
                .log_err()
            {
                this.update(cx, |this, cx| {
                    this.image = image;
                    this.image_metadata = image_metadata;
                    cx.emit(ImageItemEvent::Reloaded);
                    cx.emit(ImageItemEvent::MetadataUpdated);
                })
                .log_err();
            }
            if tx.send(()).is_err() {
                log::debug!("image reload observer was dropped");
            }
        }));
        Some(rx)
    }
}

pub fn is_image_file(project: &Entity<Project>, path: &ProjectPath, cx: &App) -> bool {
    let ext = util::maybe!({
        let worktree_abs_path = project
            .read(cx)
            .worktree_for_id(path.worktree_id, cx)?
            .read(cx)
            .abs_path();
        path.path
            .extension()
            .or_else(|| worktree_abs_path.extension()?.to_str())
            .map(str::to_lowercase)
    });

    match ext {
        Some(ext) => Img::extensions().contains(&ext.as_str()) && !ext.contains("svg"),
        None => false,
    }
}

impl ProjectItem for ImageItem {
    fn try_open(
        project: &Entity<Project>,
        path: &ProjectPath,
        cx: &mut App,
    ) -> Option<Task<anyhow::Result<Entity<Self>>>> {
        if is_image_file(project, path, cx) {
            Some(cx.spawn({
                let path = path.clone();
                let project = project.clone();
                async move |cx| {
                    project
                        .update(cx, |project, cx| project.open_image(path, cx))
                        .await
                }
            }))
        } else {
            None
        }
    }

    fn entry_id(&self, _: &App) -> Option<ProjectEntryId> {
        self.file.entry_id
    }

    fn resource_id(&self, _: &App) -> Option<vfs::ResourceId> {
        self.file.resource_id
    }

    fn vfs_path(&self, _: &App) -> Option<vfs::VfsPath> {
        self.file.vfs_path.clone()
    }

    fn project_path(&self, cx: &App) -> Option<ProjectPath> {
        Some(self.project_path(cx))
    }

    fn is_dirty(&self) -> bool {
        false
    }
}

pub struct ImageStore {
    opened_images: HashMap<ImageId, WeakEntity<ImageItem>>,
    resource_to_image_id: HashMap<ResourceId, ImageId>,
    image_ids_by_path: HashMap<ProjectPath, ImageId>,
    image_ids_by_entry_id: HashMap<ProjectEntryId, ImageId>,
    worktree_store: Entity<WorktreeStore>,
    _subscriptions: Vec<Subscription>,
    #[allow(clippy::type_complexity)]
    loading_images_by_path: HashMap<
        ProjectPath,
        postage::watch::Receiver<Option<Result<Entity<ImageItem>, Arc<anyhow::Error>>>>,
    >,
}

impl ImageStore {
    pub fn local(worktree_store: Entity<WorktreeStore>, cx: &mut Context<Self>) -> Self {
        Self::new(worktree_store, cx)
    }

    pub fn remote(worktree_store: Entity<WorktreeStore>, cx: &mut Context<Self>) -> Self {
        Self::new(worktree_store, cx)
    }

    fn new(worktree_store: Entity<WorktreeStore>, cx: &mut Context<Self>) -> Self {
        let worktree_subscription =
            cx.subscribe(&worktree_store, |this: &mut ImageStore, _, event, cx| {
                if let WorktreeStoreEvent::WorktreeAdded(worktree) = event {
                    this.subscribe_to_worktree(worktree, cx);
                }
            });
        Self {
            opened_images: Default::default(),
            resource_to_image_id: Default::default(),
            image_ids_by_path: Default::default(),
            image_ids_by_entry_id: Default::default(),
            loading_images_by_path: Default::default(),
            worktree_store,
            _subscriptions: vec![worktree_subscription],
        }
    }

    pub fn images(&self) -> impl '_ + Iterator<Item = Entity<ImageItem>> {
        self.opened_images
            .values()
            .filter_map(|image| image.upgrade())
    }

    pub fn get(&self, image_id: ImageId) -> Option<Entity<ImageItem>> {
        self.opened_images
            .get(&image_id)
            .and_then(|image| image.upgrade())
    }

    pub fn get_by_path(&mut self, path: &ProjectPath) -> Option<Entity<ImageItem>> {
        let image_id = *self.image_ids_by_path.get(path)?;
        self.get_indexed_image(image_id)
    }

    pub fn get_by_resource(&mut self, resource_id: ResourceId) -> Option<Entity<ImageItem>> {
        let image_id = *self.resource_to_image_id.get(&resource_id)?;
        self.get_indexed_image(image_id)
    }

    pub fn open_image(
        &mut self,
        project_path: ProjectPath,
        cx: &mut Context<Self>,
    ) -> Task<Result<Entity<ImageItem>>> {
        let Some(worktree) = self
            .worktree_store
            .read(cx)
            .worktree_for_id(project_path.worktree_id, cx)
        else {
            return Task::ready(Err(anyhow::anyhow!("no such worktree")));
        };
        let resource_id = worktree
            .read(cx)
            .entry_for_path(&project_path.path)
            .and_then(|entry| entry.resource_id);
        if let Some(resource_id) = resource_id {
            if let Some(existing_image) = self.get_by_resource(resource_id) {
                return Task::ready(Ok(existing_image));
            }
        } else if let Some(existing_image) = self.get_by_path(&project_path) {
            return Task::ready(Ok(existing_image));
        }

        let loading_watch = match self.loading_images_by_path.entry(project_path.clone()) {
            // If the given path is already being loaded, then wait for that existing
            // task to complete and return the same image.
            hash_map::Entry::Occupied(e) => e.get().clone(),

            // Otherwise, record the fact that this path is now being loaded.
            hash_map::Entry::Vacant(entry) => {
                let (mut tx, rx) = postage::watch::channel();
                entry.insert(rx.clone());

                let load_image = load_image_item(project_path.path.clone(), worktree, cx);

                cx.spawn(async move |this, cx| {
                    let load_result = load_image.await.and_then(|image| {
                        this.update(cx, |this, cx| this.add_image(image.clone(), cx))?;
                        Ok(image)
                    });
                    *tx.borrow_mut() = Some(this.update(cx, |this, _cx| {
                        // Record the fact that the image is no longer loading.
                        this.loading_images_by_path.remove(&project_path);
                        let image = load_result.map_err(Arc::new)?;
                        Ok(image)
                    })?);
                    anyhow::Ok(())
                })
                .detach();
                rx
            }
        };

        cx.background_spawn(async move {
            Self::wait_for_loading_image(loading_watch)
                .await
                .map_err(|e| e.cloned())
        })
    }

    pub async fn wait_for_loading_image(
        mut receiver: postage::watch::Receiver<
            Option<Result<Entity<ImageItem>, Arc<anyhow::Error>>>,
        >,
    ) -> Result<Entity<ImageItem>, Arc<anyhow::Error>> {
        loop {
            if let Some(result) = receiver.borrow().as_ref() {
                match result {
                    Ok(image) => return Ok(image.to_owned()),
                    Err(e) => return Err(e.to_owned()),
                }
            }
            if receiver.next().await.is_none() {
                return Err(Arc::new(anyhow::anyhow!("image load was cancelled")));
            }
        }
    }

    pub fn reload_images(
        &self,
        images: HashSet<Entity<ImageItem>>,
        cx: &mut Context<ImageStore>,
    ) -> Task<Result<()>> {
        if images.is_empty() {
            return Task::ready(Ok(()));
        }

        reload_image_items(images, cx)
    }

    fn add_image(&mut self, image: Entity<ImageItem>, cx: &mut Context<ImageStore>) {
        let image_id = image.read(cx).id;
        self.opened_images.insert(image_id, image.downgrade());
        self.index_image(&image, cx);
        cx.subscribe(&image, Self::on_image_event).detach();
        cx.emit(ImageStoreEvent::ImageAdded(image));
    }

    fn on_image_event(
        &mut self,
        image: Entity<ImageItem>,
        event: &ImageItemEvent,
        cx: &mut Context<Self>,
    ) {
        if matches!(event, ImageItemEvent::FileHandleChanged) {
            self.index_image(&image, cx);
        }
    }

    fn subscribe_to_worktree(&mut self, worktree: &Entity<Worktree>, cx: &mut Context<Self>) {
        cx.subscribe(worktree, |this, worktree, event, cx| {
            if let worktree::Event::UpdatedEntries(changes) = event {
                this.worktree_entries_changed(&worktree, changes, cx);
            }
        })
        .detach();
    }

    fn worktree_entries_changed(
        &mut self,
        worktree: &Entity<Worktree>,
        changes: &[(Arc<RelPath>, ProjectEntryId, PathChange)],
        cx: &mut Context<Self>,
    ) {
        let snapshot = worktree.read(cx).snapshot();
        for (path, entry_id, _) in changes {
            self.worktree_entry_changed(*entry_id, path, worktree, &snapshot, cx);
        }
    }

    fn worktree_entry_changed(
        &mut self,
        entry_id: ProjectEntryId,
        path: &Arc<RelPath>,
        worktree: &Entity<Worktree>,
        snapshot: &worktree::Snapshot,
        cx: &mut Context<Self>,
    ) -> Option<()> {
        let project_path = ProjectPath {
            worktree_id: snapshot.id(),
            path: path.clone(),
        };
        let image_id = self
            .image_ids_by_entry_id
            .get(&entry_id)
            .copied()
            .or_else(|| self.image_ids_by_path.get(&project_path).copied())?;

        let Some(image) = self.get(image_id) else {
            self.opened_images.remove(&image_id);
            self.remove_image_indexes(image_id);
            return None;
        };

        image.update(cx, |image, cx| {
            let old_file = &image.file;
            if old_file.worktree != *worktree {
                return;
            }

            let snapshot_entry = old_file
                .entry_id
                .and_then(|entry_id| snapshot.entry_for_id(entry_id))
                .or_else(|| snapshot.entry_for_path(old_file.path.as_ref()));

            let mut new_file = if let Some(entry) = snapshot_entry {
                let mut file = worktree::File::from_entry(entry.clone(), worktree.clone(), cx);
                if entry.mtime.is_none() {
                    file.disk_state = old_file.disk_state;
                }
                file
            } else {
                worktree::File {
                    disk_state: DiskState::Deleted,
                    is_local: old_file.is_local,
                    entry_id: old_file.entry_id,
                    path: old_file.path.clone(),
                    worktree: worktree.clone(),
                    is_private: old_file.is_private,
                    resource_id: old_file.resource_id,
                    vfs_path: old_file.vfs_path.clone(),
                }
            };

            if new_file.entry_id == old_file.entry_id
                && new_file.path != old_file.path
                && old_file.resource_id.is_some()
            {
                new_file.resource_id = old_file.resource_id;
            }

            if new_file != **old_file {
                image.file_updated(Arc::new(new_file), cx);
            }
        });
        Some(())
    }

    fn get_indexed_image(&mut self, image_id: ImageId) -> Option<Entity<ImageItem>> {
        let image = self.get(image_id);
        if image.is_none() {
            self.opened_images.remove(&image_id);
            self.remove_image_indexes(image_id);
        }
        image
    }

    fn index_image(&mut self, image: &Entity<ImageItem>, cx: &App) {
        let (image_id, project_path, entry_id, resource_id) = {
            let image = image.read(cx);
            (
                image.id,
                image.project_path(cx),
                image.file.entry_id,
                image.file.resource_id,
            )
        };
        self.remove_image_indexes(image_id);
        self.image_ids_by_path.insert(project_path, image_id);
        if let Some(entry_id) = entry_id {
            self.image_ids_by_entry_id.insert(entry_id, image_id);
        }
        if let Some(resource_id) = resource_id {
            self.resource_to_image_id.insert(resource_id, image_id);
        }
    }

    fn remove_image_indexes(&mut self, image_id: ImageId) {
        self.image_ids_by_path
            .retain(|_, indexed_image_id| *indexed_image_id != image_id);
        self.image_ids_by_entry_id
            .retain(|_, indexed_image_id| *indexed_image_id != image_id);
        self.resource_to_image_id
            .retain(|_, indexed_image_id| *indexed_image_id != image_id);
    }
}

fn load_image_item(
    path: Arc<RelPath>,
    worktree: Entity<Worktree>,
    cx: &mut Context<ImageStore>,
) -> Task<Result<Entity<ImageItem>>> {
    let load_file = worktree.update(cx, |worktree, cx| {
        worktree.load_binary_file(path.as_ref(), cx)
    });
    cx.spawn(async move |_image_store, cx| {
        let LoadedBinaryFile { file, content } = load_file.await?;
        let image_metadata = ImageItem::compute_metadata_from_bytes(&content).log_err();
        let image = create_gpui_image(content)?;
        let entity = cx.new(|cx| ImageItem {
            id: cx.entity_id().as_non_zero_u64().into(),
            file: file.clone(),
            image,
            image_metadata,
            reload_task: None,
        });
        Ok(entity)
    })
}

fn reload_image_items(
    images: HashSet<Entity<ImageItem>>,
    cx: &mut Context<ImageStore>,
) -> Task<Result<()>> {
    cx.spawn(async move |_, cx| {
        for image in images {
            if let Some(reload) = image.update(cx, |image, cx| image.reload(cx)) {
                reload.await?;
            }
        }
        Ok(())
    })
}

pub(crate) fn create_gpui_image(content: Vec<u8>) -> anyhow::Result<Arc<gpui::Image>> {
    let format = image::guess_format(&content)?;

    Ok(Arc::new(gpui::Image::from_bytes(
        match format {
            image::ImageFormat::Png => gpui::ImageFormat::Png,
            image::ImageFormat::Jpeg => gpui::ImageFormat::Jpeg,
            image::ImageFormat::WebP => gpui::ImageFormat::Webp,
            image::ImageFormat::Gif => gpui::ImageFormat::Gif,
            image::ImageFormat::Bmp => gpui::ImageFormat::Bmp,
            image::ImageFormat::Tiff => gpui::ImageFormat::Tiff,
            image::ImageFormat::Ico => gpui::ImageFormat::Ico,
            image::ImageFormat::Pnm => gpui::ImageFormat::Pnm,
            format => anyhow::bail!("Image format {format:?} not supported"),
        },
        content,
    )))
}
