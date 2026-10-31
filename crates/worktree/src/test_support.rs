use std::{path::PathBuf, sync::Arc};

use anyhow::Result;
use vfs::VfsFile;

pub async fn stage_binary_file_from_vfs(
    file: Arc<dyn VfsFile>,
    length: u64,
) -> Result<(PathBuf, tempfile::TempPath)> {
    crate::stage_vfs_file_to_temp(file.as_ref(), length).await
}
