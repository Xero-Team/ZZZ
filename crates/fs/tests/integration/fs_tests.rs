mod fake_git_repo_tests;

use std::{
    collections::BTreeSet,
    ffi::OsString,
    io::Write,
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
    time::Duration,
};

use futures::{FutureExt, StreamExt};

use fs::*;
use gpui::{BackgroundExecutor, TestAppContext};
use serde_json::json;
use tempfile::TempDir;
use util::path;
use vfs::{
    CancellationToken, CaseSensitivity, CopyOptions as VfsCopyOptions, CreateDisposition,
    EntryKind as VfsEntryKind, FileAccess, OpenOptions, OperationContext, OperationId,
    ProviderPath, RenameOptions as VfsRenameOptions, StatOptions, SymbolicLinkMode, VfsErrorCode,
    VfsEventKind, VfsProvider, WatchDepth, WatchRequest, WriteAtOptions,
    test_support::run_provider_conformance,
};

#[gpui::test]
async fn test_fake_fs(executor: BackgroundExecutor) {
    let fs = FakeFs::new(executor.clone());
    fs.insert_tree(
        path!("/root"),
        json!({
            "dir1": {
                "a": "A",
                "b": "B"
            },
            "dir2": {
                "c": "C",
                "dir3": {
                    "d": "D"
                }
            }
        }),
    )
    .await;

    assert_eq!(
        fs.files(),
        vec![
            PathBuf::from(path!("/root/dir1/a")),
            PathBuf::from(path!("/root/dir1/b")),
            PathBuf::from(path!("/root/dir2/c")),
            PathBuf::from(path!("/root/dir2/dir3/d")),
        ]
    );

    fs.create_symlink(path!("/root/dir2/link-to-dir3").as_ref(), "./dir3".into())
        .await
        .unwrap();

    assert_eq!(
        fs.canonicalize(path!("/root/dir2/link-to-dir3").as_ref())
            .await
            .unwrap(),
        PathBuf::from(path!("/root/dir2/dir3")),
    );
    assert_eq!(
        fs.canonicalize(path!("/root/dir2/link-to-dir3/d").as_ref())
            .await
            .unwrap(),
        PathBuf::from(path!("/root/dir2/dir3/d")),
    );
    assert_eq!(
        fs.load(path!("/root/dir2/link-to-dir3/d").as_ref())
            .await
            .unwrap(),
        "D",
    );
}

async fn run_legacy_storage_conformance(fs: Arc<dyn Fs>, root: &Path) -> anyhow::Result<()> {
    let nested_directory = root.join("nested");
    let source_file = root.join("source.bin");
    let copied_file = root.join("copied.bin");
    let renamed_file = root.join("renamed.bin");

    fs.create_dir(root).await?;
    fs.create_dir(&nested_directory).await?;
    fs.write(&source_file, b"vfs-conformance").await?;
    anyhow::ensure!(
        fs.load_bytes(&source_file).await? == b"vfs-conformance",
        "whole-file bytes changed during round trip"
    );

    let Some(metadata) = fs.metadata(&source_file).await? else {
        anyhow::bail!("metadata disappeared for {}", source_file.display());
    };
    anyhow::ensure!(!metadata.is_dir, "file was reported as a directory");
    anyhow::ensure!(metadata.len == 15, "file length did not match content");

    fs.copy_file(&source_file, &copied_file, CopyOptions::default())
        .await?;
    fs.rename(&copied_file, &renamed_file, RenameOptions::default())
        .await?;
    anyhow::ensure!(
        fs.load_bytes(&renamed_file).await? == b"vfs-conformance",
        "copy/rename changed file content"
    );

    let mut children = BTreeSet::new();
    let mut directory_entries = fs.read_dir(root).await?;
    while let Some(entry) = directory_entries.next().await {
        children.insert(entry?);
    }
    anyhow::ensure!(
        children
            == BTreeSet::from([
                nested_directory.clone(),
                renamed_file.clone(),
                source_file.clone(),
            ]),
        "directory listing did not match committed state: {children:?}"
    );

    fs.remove_file(&source_file, RemoveOptions::default())
        .await?;
    fs.remove_file(&renamed_file, RemoveOptions::default())
        .await?;
    fs.remove_dir(&nested_directory, RemoveOptions::default())
        .await?;
    fs.remove_dir(root, RemoveOptions::default()).await?;
    Ok(())
}

#[gpui::test]
async fn test_fake_fs_legacy_storage_conformance(executor: BackgroundExecutor) {
    let filesystem: Arc<dyn Fs> = FakeFs::new(executor);
    let result =
        run_legacy_storage_conformance(filesystem, Path::new(path!("/vfs-conformance"))).await;
    assert!(
        result.is_ok(),
        "fake filesystem conformance failed: {result:?}"
    );
}

#[gpui::test]
async fn test_real_fs_legacy_storage_conformance(
    executor: BackgroundExecutor,
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let temporary_directory = TempDir::new();
    let Ok(temporary_directory) = temporary_directory else {
        panic!("failed to create real filesystem conformance directory: {temporary_directory:?}");
    };
    let filesystem: Arc<dyn Fs> = Arc::new(RealFs::new(None, executor));
    let root = temporary_directory.path().join("vfs-conformance");
    let result = run_legacy_storage_conformance(filesystem, &root).await;
    assert!(
        result.is_ok(),
        "real filesystem conformance failed: {result:?}"
    );
}

#[gpui::test]
async fn test_legacy_fs_provider_conformance(executor: BackgroundExecutor) {
    let filesystem: Arc<dyn Fs> = FakeFs::new(executor);
    let provider: Arc<dyn VfsProvider> = Arc::new(LegacyFsProvider::new(
        "legacy-fake",
        Arc::<Path>::from(Path::new(path!("/vfs-provider"))),
        filesystem.clone(),
        CaseSensitivity::Sensitive,
    ));
    let create_root = filesystem
        .create_dir(Path::new(path!("/vfs-provider")))
        .await;
    assert!(
        create_root.is_ok(),
        "failed to create legacy provider root: {create_root:?}"
    );
    let result = run_provider_conformance(provider).await;
    assert!(
        result.is_ok(),
        "legacy provider conformance failed: {result:?}"
    );
}

#[gpui::test]
async fn test_local_provider_conformance(executor: BackgroundExecutor, cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let temporary_directory = TempDir::new();
    let Ok(temporary_directory) = temporary_directory else {
        panic!("failed to create local provider root: {temporary_directory:?}");
    };
    let filesystem: Arc<dyn Fs> = Arc::new(RealFs::new(None, executor));
    let provider = LocalProvider::new(
        "local-real",
        Arc::<Path>::from(temporary_directory.path()),
        filesystem,
    )
    .await;
    let provider = match provider {
        Ok(provider) => provider,
        Err(error) => panic!("failed to create local provider: {error:?}"),
    };
    let result = run_provider_conformance(Arc::new(provider)).await;
    assert!(
        result.is_ok(),
        "local provider conformance failed: {result:?}"
    );
}

#[gpui::test]
async fn test_local_provider_concurrent_positioned_io(
    executor: BackgroundExecutor,
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let temporary_directory = TempDir::new();
    let Ok(temporary_directory) = temporary_directory else {
        panic!("failed to create positioned I/O root: {temporary_directory:?}");
    };
    let filesystem: Arc<dyn Fs> = Arc::new(RealFs::new(None, executor));
    let provider = LocalProvider::new(
        "local-positioned-io",
        Arc::<Path>::from(temporary_directory.path()),
        filesystem,
    )
    .await;
    let provider = match provider {
        Ok(provider) => provider,
        Err(error) => panic!("failed to create local provider: {error:?}"),
    };
    let path = ProviderPath::from_byte_components(
        provider.descriptor().path_encoding,
        [b"positioned.bin".as_slice()],
    );
    let Ok(path) = path else {
        panic!("positioned I/O path must be valid: {path:?}");
    };
    let file = provider
        .open(
            &path,
            OpenOptions {
                access: FileAccess::ReadWrite,
                create: CreateDisposition::CreateNew,
                expected_version: None,
                context: OperationContext::default(),
            },
        )
        .await;
    let file = match file {
        Ok(file) => file,
        Err(error) => panic!("failed to open positioned I/O fixture: {error:?}"),
    };

    const BLOCK_COUNT: usize = 16;
    const BLOCK_SIZE: usize = 4_096;
    let writes = futures::future::join_all((0..BLOCK_COUNT).map(|block_index| {
        let file = file.clone();
        async move {
            let bytes = vec![block_index as u8; BLOCK_SIZE];
            file.write_at(
                (block_index * BLOCK_SIZE) as u64,
                &bytes,
                WriteAtOptions::default(),
            )
            .await
        }
    }))
    .await;
    assert!(
        writes
            .iter()
            .all(|result| result.as_ref().is_ok_and(|written| *written == BLOCK_SIZE)),
        "concurrent positioned write failed: {writes:?}"
    );

    let reads = futures::future::join_all((0..BLOCK_COUNT).map(|block_index| {
        let file = file.clone();
        async move {
            let mut bytes = vec![0; BLOCK_SIZE];
            let result = file
                .read_at(
                    (block_index * BLOCK_SIZE) as u64,
                    &mut bytes,
                    OperationContext::default(),
                )
                .await;
            (block_index, result, bytes)
        }
    }))
    .await;
    for (block_index, result, bytes) in reads {
        assert!(matches!(result, Ok(BLOCK_SIZE)));
        assert!(
            bytes.iter().all(|byte| *byte == block_index as u8),
            "positioned read returned bytes from another offset"
        );
    }

    let cancellation = CancellationToken::default();
    cancellation.cancel();
    let mut bytes = [0; 1];
    let cancelled = file
        .read_at(
            0,
            &mut bytes,
            OperationContext {
                operation_id: OperationId::new(10_000),
                cancellation,
            },
        )
        .await;
    assert!(
        cancelled
            .as_ref()
            .err()
            .is_some_and(|error| error.code() == VfsErrorCode::Cancelled)
    );
}

#[gpui::test]
async fn test_local_provider_positioned_io_throughput_not_below_legacy(
    executor: BackgroundExecutor,
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let temporary_directory = TempDir::new();
    let Ok(temporary_directory) = temporary_directory else {
        panic!("failed to create positioned I/O benchmark root: {temporary_directory:?}");
    };
    let filesystem: Arc<dyn Fs> = Arc::new(RealFs::new(None, executor));
    let native_root = temporary_directory.path().join("native");
    let legacy_root = temporary_directory.path().join("legacy");
    let create_native_root = filesystem.create_dir(&native_root).await;
    let create_legacy_root = filesystem.create_dir(&legacy_root).await;
    assert!(create_native_root.is_ok(), "failed to create native root");
    assert!(create_legacy_root.is_ok(), "failed to create legacy root");

    let local_provider = LocalProvider::new(
        "local-throughput",
        Arc::<Path>::from(native_root),
        filesystem.clone(),
    )
    .await;
    let local_provider = match local_provider {
        Ok(provider) => provider,
        Err(error) => panic!("failed to create local provider: {error:?}"),
    };
    let legacy_provider = LegacyFsProvider::new(
        "legacy-throughput",
        Arc::<Path>::from(legacy_root),
        filesystem,
        CaseSensitivity::Sensitive,
    );
    let local_path = ProviderPath::from_byte_components(
        local_provider.descriptor().path_encoding,
        [b"throughput.bin".as_slice()],
    );
    let legacy_path = ProviderPath::from_byte_components(
        legacy_provider.descriptor().path_encoding,
        [b"throughput.bin".as_slice()],
    );
    let (Ok(local_path), Ok(legacy_path)) = (local_path, legacy_path) else {
        panic!("throughput fixture paths must be valid");
    };
    let local_file = local_provider
        .open(
            &local_path,
            OpenOptions {
                access: FileAccess::ReadWrite,
                create: CreateDisposition::CreateNew,
                expected_version: None,
                context: OperationContext::default(),
            },
        )
        .await;
    let legacy_file = legacy_provider
        .open(
            &legacy_path,
            OpenOptions {
                access: FileAccess::ReadWrite,
                create: CreateDisposition::CreateNew,
                expected_version: None,
                context: OperationContext::default(),
            },
        )
        .await;
    let local_file = match local_file {
        Ok(file) => file,
        Err(error) => panic!("failed to open local throughput file: {error:?}"),
    };
    let legacy_file = match legacy_file {
        Ok(file) => file,
        Err(error) => panic!("failed to open legacy throughput file: {error:?}"),
    };

    const BLOCK_COUNT: usize = 8;
    const BLOCK_SIZE: usize = 256 * 1_024;
    let legacy_start = std::time::Instant::now();
    let legacy_results = futures::future::join_all((0..BLOCK_COUNT).map(|block_index| {
        let file = legacy_file.clone();
        async move {
            let bytes = vec![block_index as u8; BLOCK_SIZE];
            file.write_at(
                (block_index * BLOCK_SIZE) as u64,
                &bytes,
                WriteAtOptions::default(),
            )
            .await
        }
    }))
    .await;
    let legacy_elapsed = legacy_start.elapsed();
    assert!(legacy_results.iter().all(Result::is_ok));

    let local_start = std::time::Instant::now();
    let local_results = futures::future::join_all((0..BLOCK_COUNT).map(|block_index| {
        let file = local_file.clone();
        async move {
            let bytes = vec![block_index as u8; BLOCK_SIZE];
            file.write_at(
                (block_index * BLOCK_SIZE) as u64,
                &bytes,
                WriteAtOptions::default(),
            )
            .await
        }
    }))
    .await;
    let local_elapsed = local_start.elapsed();
    assert!(local_results.iter().all(Result::is_ok));

    println!(
        "legacy positioned emulation: {legacy_elapsed:?}; native positioned I/O: {local_elapsed:?}"
    );
    assert!(
        local_elapsed.as_secs_f64() <= legacy_elapsed.as_secs_f64() * 1.10,
        "native positioned I/O regressed by more than 10%: legacy={legacy_elapsed:?}, local={local_elapsed:?}"
    );
}

#[gpui::test]
async fn test_legacy_provider_watch_sequence_rename_storm_and_overflow(
    executor: BackgroundExecutor,
) {
    let filesystem = FakeFs::new(executor);
    let create_root = filesystem
        .create_dir(Path::new(path!("/watch-provider")))
        .await;
    assert!(create_root.is_ok(), "failed to create watch provider root");
    let provider = LegacyFsProvider::new(
        "legacy-watch",
        Arc::<Path>::from(Path::new(path!("/watch-provider"))),
        filesystem.clone(),
        CaseSensitivity::Sensitive,
    );
    let root = ProviderPath::root(provider.descriptor().path_encoding);
    let initial_path = ProviderPath::from_byte_components(
        provider.descriptor().path_encoding,
        [b"entry-0".as_slice()],
    );
    let Ok(initial_path) = initial_path else {
        panic!("watch fixture path must be valid: {initial_path:?}");
    };
    let watch = provider
        .watch(WatchRequest {
            path: root.clone(),
            depth: WatchDepth::Recursive,
            resume_after_sequence: None,
            context: OperationContext::default(),
        })
        .await;
    let mut watch = match watch {
        Ok(watch) => watch,
        Err(error) => panic!("failed to register provider watch: {error:?}"),
    };
    let file = provider
        .open(
            &initial_path,
            OpenOptions {
                access: FileAccess::ReadWrite,
                create: CreateDisposition::CreateNew,
                expected_version: None,
                context: OperationContext::default(),
            },
        )
        .await;
    assert!(file.is_ok(), "failed to create watched file");

    let mut current_path = initial_path;
    for rename_index in 1..=20 {
        let target = ProviderPath::from_byte_components(
            provider.descriptor().path_encoding,
            [format!("entry-{rename_index}").as_bytes()],
        );
        let Ok(target) = target else {
            panic!("rename target must be valid: {target:?}");
        };
        let rename = provider
            .rename(&current_path, &target, VfsRenameOptions::default())
            .await;
        assert!(rename.is_ok(), "provider rename failed: {rename:?}");
        current_path = target;
    }
    filesystem.emit_fs_event(path!("/watch-provider"), Some(PathEventKind::Rescan));

    let mut last_sequence = 0;
    let mut saw_overflow = false;
    for _ in 0..64 {
        let Some(batch) = watch.next().await else {
            break;
        };
        let Ok(batch) = batch else {
            panic!("provider watch failed: {batch:?}");
        };
        assert!(batch.first_sequence > last_sequence);
        assert!(batch.last_sequence >= batch.first_sequence);
        last_sequence = batch.last_sequence;
        if batch
            .events
            .iter()
            .any(|event| matches!(event.kind, VfsEventKind::Overflow { .. }))
        {
            saw_overflow = true;
            break;
        }
    }
    assert!(
        saw_overflow,
        "watch did not expose backend rescan as overflow"
    );
    let final_metadata = provider.stat(&current_path, StatOptions::default()).await;
    assert!(
        final_metadata.is_ok(),
        "final renamed path was not committed: {final_metadata:?}"
    );
    let reconciled = provider.read_dir(&root, Default::default()).await;
    let Ok(reconciled) = reconciled else {
        panic!("scoped rescan failed: {reconciled:?}");
    };
    assert_eq!(reconciled.entries.len(), 1);
    assert_eq!(
        reconciled.entries.first().map(|entry| &entry.path),
        Some(&current_path)
    );
}

#[gpui::test]
#[cfg(unix)]
async fn test_local_provider_rejects_symlink_escape(
    executor: BackgroundExecutor,
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let temporary_directory = TempDir::new();
    let Ok(temporary_directory) = temporary_directory else {
        panic!("failed to create containment fixture: {temporary_directory:?}");
    };
    let provider_root = temporary_directory.path().join("provider");
    let outside_file = temporary_directory.path().join("outside.txt");
    let create_root = smol::fs::create_dir(&provider_root).await;
    let write_outside = smol::fs::write(&outside_file, b"private").await;
    assert!(create_root.is_ok(), "failed to create provider root");
    assert!(write_outside.is_ok(), "failed to create outside file");
    let filesystem: Arc<dyn Fs> = Arc::new(RealFs::new(None, executor));
    let link_path = provider_root.join("escape-link");
    let create_link = filesystem
        .create_symlink(&link_path, outside_file.clone())
        .await;
    assert!(create_link.is_ok(), "failed to create containment symlink");
    let provider = LocalProvider::new(
        "local-containment",
        Arc::<Path>::from(provider_root),
        filesystem,
    )
    .await;
    let provider = match provider {
        Ok(provider) => provider,
        Err(error) => panic!("failed to create local provider: {error:?}"),
    };
    let path = ProviderPath::from_byte_components(
        provider.descriptor().path_encoding,
        [b"escape-link".as_slice()],
    );
    let Ok(path) = path else {
        panic!("containment path must be valid: {path:?}");
    };
    let metadata = provider
        .stat(
            &path,
            StatOptions {
                symbolic_link_mode: SymbolicLinkMode::DoNotFollow,
                context: OperationContext::default(),
            },
        )
        .await;
    let Ok(metadata) = metadata else {
        panic!("symlink metadata should be visible: {metadata:?}");
    };
    assert_eq!(metadata.kind, VfsEntryKind::SymbolicLink);
    assert!(metadata.symbolic_link_target.is_none());

    let opened = provider.open(&path, OpenOptions::default()).await;
    assert!(
        opened
            .as_ref()
            .err()
            .is_some_and(|error| error.code() == VfsErrorCode::PermissionDenied),
        "symlink escape was not rejected"
    );
    let copied_path = ProviderPath::from_byte_components(
        provider.descriptor().path_encoding,
        [b"copied-link".as_slice()],
    );
    let Ok(copied_path) = copied_path else {
        panic!("copy target path must be valid: {copied_path:?}");
    };
    let copied = provider
        .copy(&path, &copied_path, VfsCopyOptions::default())
        .await;
    assert!(
        copied
            .as_ref()
            .err()
            .is_some_and(|error| error.code() == VfsErrorCode::PermissionDenied),
        "copy followed a symlink outside the provider root"
    );
}

#[gpui::test]
#[cfg(unix)]
async fn test_local_provider_preserves_non_utf8_directory_names(
    executor: BackgroundExecutor,
    cx: &mut TestAppContext,
) {
    use std::os::unix::ffi::OsStringExt as _;

    cx.executor().allow_parking();
    let temporary_directory = TempDir::new();
    let Ok(temporary_directory) = temporary_directory else {
        panic!("failed to create non-UTF-8 fixture: {temporary_directory:?}");
    };
    let exact_name = vec![0xff, b'-', b'f', b'i', b'l', b'e'];
    let file_path = temporary_directory
        .path()
        .join(OsString::from_vec(exact_name.clone()));
    let write_path = file_path.clone();
    let write_result = smol::unblock(move || std::fs::write(write_path, b"bytes")).await;
    assert!(write_result.is_ok(), "failed to write non-UTF-8 fixture");

    let filesystem: Arc<dyn Fs> = Arc::new(RealFs::new(None, executor));
    let provider = LocalProvider::new(
        "local-non-utf8",
        Arc::<Path>::from(temporary_directory.path()),
        filesystem,
    )
    .await;
    let provider = match provider {
        Ok(provider) => provider,
        Err(error) => panic!("failed to create local provider: {error:?}"),
    };
    let root = ProviderPath::root(provider.descriptor().path_encoding);
    let page = provider.read_dir(&root, Default::default()).await;
    let Ok(page) = page else {
        panic!("failed to read non-UTF-8 directory: {page:?}");
    };
    assert_eq!(page.entries.len(), 1);
    let Some(entry) = page.entries.first() else {
        panic!("non-UTF-8 directory entry was missing");
    };
    assert_eq!(
        entry.path.file_name().map(|name| name.as_bytes()),
        Some(exact_name.as_slice())
    );

    let file = provider.open(&entry.path, OpenOptions::default()).await;
    let file = match file {
        Ok(file) => file,
        Err(error) => panic!("failed to open non-UTF-8 path: {error:?}"),
    };
    let mut bytes = [0; 5];
    let read = file
        .read_at(0, &mut bytes, OperationContext::default())
        .await;
    assert!(matches!(read, Ok(5)));
    assert_eq!(&bytes, b"bytes");
}

#[gpui::test]
async fn test_copy_recursive_with_single_file(executor: BackgroundExecutor) {
    let fs = FakeFs::new(executor.clone());
    fs.insert_tree(
        path!("/outer"),
        json!({
            "a": "A",
            "b": "B",
            "inner": {}
        }),
    )
    .await;

    assert_eq!(
        fs.files(),
        vec![
            PathBuf::from(path!("/outer/a")),
            PathBuf::from(path!("/outer/b")),
        ]
    );

    let source = Path::new(path!("/outer/a"));
    let target = Path::new(path!("/outer/a copy"));
    copy_recursive(fs.as_ref(), source, target, Default::default())
        .await
        .unwrap();

    assert_eq!(
        fs.files(),
        vec![
            PathBuf::from(path!("/outer/a")),
            PathBuf::from(path!("/outer/a copy")),
            PathBuf::from(path!("/outer/b")),
        ]
    );

    let source = Path::new(path!("/outer/a"));
    let target = Path::new(path!("/outer/inner/a copy"));
    copy_recursive(fs.as_ref(), source, target, Default::default())
        .await
        .unwrap();

    assert_eq!(
        fs.files(),
        vec![
            PathBuf::from(path!("/outer/a")),
            PathBuf::from(path!("/outer/a copy")),
            PathBuf::from(path!("/outer/b")),
            PathBuf::from(path!("/outer/inner/a copy")),
        ]
    );
}

#[gpui::test]
async fn test_copy_recursive_with_single_dir(executor: BackgroundExecutor) {
    let fs = FakeFs::new(executor.clone());
    fs.insert_tree(
        path!("/outer"),
        json!({
            "a": "A",
            "empty": {},
            "non-empty": {
                "b": "B",
            }
        }),
    )
    .await;

    assert_eq!(
        fs.files(),
        vec![
            PathBuf::from(path!("/outer/a")),
            PathBuf::from(path!("/outer/non-empty/b")),
        ]
    );
    assert_eq!(
        fs.directories(false),
        vec![
            PathBuf::from(path!("/")),
            PathBuf::from(path!("/outer")),
            PathBuf::from(path!("/outer/empty")),
            PathBuf::from(path!("/outer/non-empty")),
        ]
    );

    let source = Path::new(path!("/outer/empty"));
    let target = Path::new(path!("/outer/empty copy"));
    copy_recursive(fs.as_ref(), source, target, Default::default())
        .await
        .unwrap();

    assert_eq!(
        fs.files(),
        vec![
            PathBuf::from(path!("/outer/a")),
            PathBuf::from(path!("/outer/non-empty/b")),
        ]
    );
    assert_eq!(
        fs.directories(false),
        vec![
            PathBuf::from(path!("/")),
            PathBuf::from(path!("/outer")),
            PathBuf::from(path!("/outer/empty")),
            PathBuf::from(path!("/outer/empty copy")),
            PathBuf::from(path!("/outer/non-empty")),
        ]
    );

    let source = Path::new(path!("/outer/non-empty"));
    let target = Path::new(path!("/outer/non-empty copy"));
    copy_recursive(fs.as_ref(), source, target, Default::default())
        .await
        .unwrap();

    assert_eq!(
        fs.files(),
        vec![
            PathBuf::from(path!("/outer/a")),
            PathBuf::from(path!("/outer/non-empty/b")),
            PathBuf::from(path!("/outer/non-empty copy/b")),
        ]
    );
    assert_eq!(
        fs.directories(false),
        vec![
            PathBuf::from(path!("/")),
            PathBuf::from(path!("/outer")),
            PathBuf::from(path!("/outer/empty")),
            PathBuf::from(path!("/outer/empty copy")),
            PathBuf::from(path!("/outer/non-empty")),
            PathBuf::from(path!("/outer/non-empty copy")),
        ]
    );
}

#[gpui::test]
async fn test_copy_recursive(executor: BackgroundExecutor) {
    let fs = FakeFs::new(executor.clone());
    fs.insert_tree(
        path!("/outer"),
        json!({
            "inner1": {
                "a": "A",
                "b": "B",
                "inner3": {
                    "d": "D",
                },
                "inner4": {}
            },
            "inner2": {
                "c": "C",
            }
        }),
    )
    .await;

    assert_eq!(
        fs.files(),
        vec![
            PathBuf::from(path!("/outer/inner1/a")),
            PathBuf::from(path!("/outer/inner1/b")),
            PathBuf::from(path!("/outer/inner2/c")),
            PathBuf::from(path!("/outer/inner1/inner3/d")),
        ]
    );
    assert_eq!(
        fs.directories(false),
        vec![
            PathBuf::from(path!("/")),
            PathBuf::from(path!("/outer")),
            PathBuf::from(path!("/outer/inner1")),
            PathBuf::from(path!("/outer/inner2")),
            PathBuf::from(path!("/outer/inner1/inner3")),
            PathBuf::from(path!("/outer/inner1/inner4")),
        ]
    );

    let source = Path::new(path!("/outer"));
    let target = Path::new(path!("/outer/inner1/outer"));
    copy_recursive(fs.as_ref(), source, target, Default::default())
        .await
        .unwrap();

    assert_eq!(
        fs.files(),
        vec![
            PathBuf::from(path!("/outer/inner1/a")),
            PathBuf::from(path!("/outer/inner1/b")),
            PathBuf::from(path!("/outer/inner2/c")),
            PathBuf::from(path!("/outer/inner1/inner3/d")),
            PathBuf::from(path!("/outer/inner1/outer/inner1/a")),
            PathBuf::from(path!("/outer/inner1/outer/inner1/b")),
            PathBuf::from(path!("/outer/inner1/outer/inner2/c")),
            PathBuf::from(path!("/outer/inner1/outer/inner1/inner3/d")),
        ]
    );
    assert_eq!(
        fs.directories(false),
        vec![
            PathBuf::from(path!("/")),
            PathBuf::from(path!("/outer")),
            PathBuf::from(path!("/outer/inner1")),
            PathBuf::from(path!("/outer/inner2")),
            PathBuf::from(path!("/outer/inner1/inner3")),
            PathBuf::from(path!("/outer/inner1/inner4")),
            PathBuf::from(path!("/outer/inner1/outer")),
            PathBuf::from(path!("/outer/inner1/outer/inner1")),
            PathBuf::from(path!("/outer/inner1/outer/inner2")),
            PathBuf::from(path!("/outer/inner1/outer/inner1/inner3")),
            PathBuf::from(path!("/outer/inner1/outer/inner1/inner4")),
        ]
    );
}

#[gpui::test]
async fn test_copy_recursive_with_overwriting(executor: BackgroundExecutor) {
    let fs = FakeFs::new(executor.clone());
    fs.insert_tree(
        path!("/outer"),
        json!({
            "inner1": {
                "a": "A",
                "b": "B",
                "outer": {
                    "inner1": {
                        "a": "B"
                    }
                }
            },
            "inner2": {
                "c": "C",
            }
        }),
    )
    .await;

    assert_eq!(
        fs.files(),
        vec![
            PathBuf::from(path!("/outer/inner1/a")),
            PathBuf::from(path!("/outer/inner1/b")),
            PathBuf::from(path!("/outer/inner2/c")),
            PathBuf::from(path!("/outer/inner1/outer/inner1/a")),
        ]
    );
    assert_eq!(
        fs.load(path!("/outer/inner1/outer/inner1/a").as_ref())
            .await
            .unwrap(),
        "B",
    );

    let source = Path::new(path!("/outer"));
    let target = Path::new(path!("/outer/inner1/outer"));
    copy_recursive(
        fs.as_ref(),
        source,
        target,
        CopyOptions {
            overwrite: true,
            ..Default::default()
        },
    )
    .await
    .unwrap();

    assert_eq!(
        fs.files(),
        vec![
            PathBuf::from(path!("/outer/inner1/a")),
            PathBuf::from(path!("/outer/inner1/b")),
            PathBuf::from(path!("/outer/inner2/c")),
            PathBuf::from(path!("/outer/inner1/outer/inner1/a")),
            PathBuf::from(path!("/outer/inner1/outer/inner1/b")),
            PathBuf::from(path!("/outer/inner1/outer/inner2/c")),
            PathBuf::from(path!("/outer/inner1/outer/inner1/outer/inner1/a")),
        ]
    );
    assert_eq!(
        fs.load(path!("/outer/inner1/outer/inner1/a").as_ref())
            .await
            .unwrap(),
        "A"
    );
}

#[gpui::test]
async fn test_copy_recursive_with_ignoring(executor: BackgroundExecutor) {
    let fs = FakeFs::new(executor.clone());
    fs.insert_tree(
        path!("/outer"),
        json!({
            "inner1": {
                "a": "A",
                "b": "B",
                "outer": {
                    "inner1": {
                        "a": "B"
                    }
                }
            },
            "inner2": {
                "c": "C",
            }
        }),
    )
    .await;

    assert_eq!(
        fs.files(),
        vec![
            PathBuf::from(path!("/outer/inner1/a")),
            PathBuf::from(path!("/outer/inner1/b")),
            PathBuf::from(path!("/outer/inner2/c")),
            PathBuf::from(path!("/outer/inner1/outer/inner1/a")),
        ]
    );
    assert_eq!(
        fs.load(path!("/outer/inner1/outer/inner1/a").as_ref())
            .await
            .unwrap(),
        "B",
    );

    let source = Path::new(path!("/outer"));
    let target = Path::new(path!("/outer/inner1/outer"));
    copy_recursive(
        fs.as_ref(),
        source,
        target,
        CopyOptions {
            ignore_if_exists: true,
            ..Default::default()
        },
    )
    .await
    .unwrap();

    assert_eq!(
        fs.files(),
        vec![
            PathBuf::from(path!("/outer/inner1/a")),
            PathBuf::from(path!("/outer/inner1/b")),
            PathBuf::from(path!("/outer/inner2/c")),
            PathBuf::from(path!("/outer/inner1/outer/inner1/a")),
            PathBuf::from(path!("/outer/inner1/outer/inner1/b")),
            PathBuf::from(path!("/outer/inner1/outer/inner2/c")),
            PathBuf::from(path!("/outer/inner1/outer/inner1/outer/inner1/a")),
        ]
    );
    assert_eq!(
        fs.load(path!("/outer/inner1/outer/inner1/a").as_ref())
            .await
            .unwrap(),
        "B"
    );
}

#[gpui::test]
async fn test_realfs_atomic_write(executor: BackgroundExecutor) {
    // With the file handle still open, the file should be replaced
    // https://github.com/zed-industries/zed/issues/30054
    let fs = RealFs::new(None, executor);
    let temp_dir = TempDir::new().unwrap();
    let file_to_be_replaced = temp_dir.path().join("file.txt");
    let mut file = std::fs::File::create_new(&file_to_be_replaced).unwrap();
    file.write_all(b"Hello").unwrap();
    // drop(file);  // We still hold the file handle here
    let content = std::fs::read_to_string(&file_to_be_replaced).unwrap();
    assert_eq!(content, "Hello");
    gpui::block_on(fs.atomic_write(file_to_be_replaced.clone(), "World".into())).unwrap();
    let content = std::fs::read_to_string(&file_to_be_replaced).unwrap();
    assert_eq!(content, "World");
}

#[gpui::test]
async fn test_realfs_atomic_write_non_existing_file(executor: BackgroundExecutor) {
    let fs = RealFs::new(None, executor);
    let temp_dir = TempDir::new().unwrap();
    let file_to_be_replaced = temp_dir.path().join("file.txt");
    gpui::block_on(fs.atomic_write(file_to_be_replaced.clone(), "Hello".into())).unwrap();
    let content = std::fs::read_to_string(&file_to_be_replaced).unwrap();
    assert_eq!(content, "Hello");
}

#[gpui::test]
#[cfg(target_os = "windows")]
async fn test_realfs_canonicalize(executor: BackgroundExecutor) {
    use util::paths::SanitizedPath;

    let fs = RealFs::new(None, executor);
    let temp_dir = TempDir::new().unwrap();
    let file = temp_dir.path().join("test (1).txt");
    let file = SanitizedPath::new(&file);
    std::fs::write(&file, "test").unwrap();

    let canonicalized = fs.canonicalize(file.as_path()).await;
    assert!(canonicalized.is_ok());
}

#[gpui::test]
async fn test_rename(executor: BackgroundExecutor) {
    let fs = FakeFs::new(executor.clone());
    fs.insert_tree(
        path!("/root"),
        json!({
            "src": {
                "file_a.txt": "content a",
                "file_b.txt": "content b"
            }
        }),
    )
    .await;

    fs.rename(
        Path::new(path!("/root/src/file_a.txt")),
        Path::new(path!("/root/src/new/renamed_a.txt")),
        RenameOptions {
            create_parents: true,
            ..Default::default()
        },
    )
    .await
    .unwrap();

    // Assert that the `file_a.txt` file was being renamed and moved to a
    // different directory that did not exist before.
    assert_eq!(
        fs.files(),
        vec![
            PathBuf::from(path!("/root/src/file_b.txt")),
            PathBuf::from(path!("/root/src/new/renamed_a.txt")),
        ]
    );

    let result = fs
        .rename(
            Path::new(path!("/root/src/file_b.txt")),
            Path::new(path!("/root/src/old/renamed_b.txt")),
            RenameOptions {
                create_parents: false,
                ..Default::default()
            },
        )
        .await;

    // Assert that the `file_b.txt` file was not renamed nor moved, as
    // `create_parents` was set to `false`.
    // different directory that did not exist before.
    assert!(result.is_err());
    assert_eq!(
        fs.files(),
        vec![
            PathBuf::from(path!("/root/src/file_b.txt")),
            PathBuf::from(path!("/root/src/new/renamed_a.txt")),
        ]
    );
}

#[gpui::test]
#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
async fn test_realfs_parallel_rename_without_overwrite_preserves_losing_source(
    executor: BackgroundExecutor,
) {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();
    let source_a = root.join("dir_a/shared.txt");
    let source_b = root.join("dir_b/shared.txt");
    let target = root.join("shared.txt");

    std::fs::create_dir_all(source_a.parent().unwrap()).unwrap();
    std::fs::create_dir_all(source_b.parent().unwrap()).unwrap();
    std::fs::write(&source_a, "from a").unwrap();
    std::fs::write(&source_b, "from b").unwrap();

    let fs = RealFs::new(None, executor);
    let (first_result, second_result) = futures::future::join(
        fs.rename(&source_a, &target, RenameOptions::default()),
        fs.rename(&source_b, &target, RenameOptions::default()),
    )
    .await;

    assert_ne!(first_result.is_ok(), second_result.is_ok());
    assert!(target.exists());
    assert_eq!(source_a.exists() as u8 + source_b.exists() as u8, 1);
}

#[gpui::test]
#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
async fn test_realfs_rename_ignore_if_exists_leaves_source_and_target_unchanged(
    executor: BackgroundExecutor,
) {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();
    let source = root.join("source.txt");
    let target = root.join("target.txt");

    std::fs::write(&source, "from source").unwrap();
    std::fs::write(&target, "from target").unwrap();

    let fs = RealFs::new(None, executor);
    let result = fs
        .rename(
            &source,
            &target,
            RenameOptions {
                ignore_if_exists: true,
                ..Default::default()
            },
        )
        .await;

    assert!(result.is_ok());

    assert_eq!(std::fs::read_to_string(&source).unwrap(), "from source");
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "from target");
}

#[gpui::test]
async fn test_fake_fs_rename_ignore_if_exists_leaves_source_and_target_unchanged(
    executor: BackgroundExecutor,
) {
    let fs = FakeFs::new(executor);
    fs.insert_tree(
        path!("/root"),
        json!({
            "source.txt": "from source",
            "target.txt": "from target",
        }),
    )
    .await;

    let handle = fs
        .open_handle(Path::new(path!("/root/source.txt")))
        .await
        .unwrap();

    let result = fs
        .rename(
            Path::new(path!("/root/source.txt")),
            Path::new(path!("/root/target.txt")),
            RenameOptions {
                ignore_if_exists: true,
                ..Default::default()
            },
        )
        .await;

    assert!(result.is_ok());

    assert_eq!(
        fs.load(Path::new(path!("/root/source.txt"))).await.unwrap(),
        "from source"
    );
    assert_eq!(
        fs.load(Path::new(path!("/root/target.txt"))).await.unwrap(),
        "from target"
    );

    // A handle held across an ignored rename must keep reporting the path the
    // file is actually at, not the one it never went to.
    assert_eq!(
        handle.current_path(&(fs.clone() as Arc<dyn Fs>)).unwrap(),
        PathBuf::from(path!("/root/source.txt"))
    );
}

async fn assert_copy_and_remove_semantics(root: &Path, fs: &dyn Fs) {
    let source = root.join("source.txt");
    let target = root.join("target.txt");
    let overwrite = CopyOptions {
        overwrite: true,
        ignore_if_exists: false,
    };

    let target_inode_before = fs.metadata(&target).await.unwrap().unwrap().inode;
    fs.copy_file(&source, &target, overwrite).await.unwrap();
    assert_eq!(fs.load(&target).await.unwrap(), "from source");

    cfg_select! {
        unix => assert_eq!(
            fs.metadata(&target).await.unwrap().unwrap().inode,
            target_inode_before
        ),
        _ => { let _ = target_inode_before; }
    }

    fs.copy_file(&source, &target, CopyOptions::default())
        .await
        .unwrap_err();
    fs.copy_file(
        &source,
        &target,
        CopyOptions {
            overwrite: false,
            ignore_if_exists: true,
        },
    )
    .await
    .unwrap();

    fs.copy_file(&source, &root.join("dir"), overwrite)
        .await
        .unwrap_err();
    assert!(fs.is_dir(&root.join("dir")).await);
}

/// Removing a symlink to a directory removes the link, not the directory.
async fn assert_remove_file_unlinks_symlink(root: &Path, fs: &dyn Fs) {
    let link = root.join("link");
    fs.remove_file(&link, RemoveOptions::default())
        .await
        .unwrap();
    assert!(fs.metadata(&link).await.unwrap().is_none());
    assert!(fs.is_dir(&root.join("dir")).await);
    assert_eq!(
        fs.load(&root.join("dir").join("inner.txt")).await.unwrap(),
        "inner"
    );
}

#[gpui::test]
#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
async fn test_realfs_copy_and_remove_semantics(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let executor = cx.executor();
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();
    std::fs::write(root.join("source.txt"), "from source").unwrap();
    std::fs::write(root.join("target.txt"), "from target").unwrap();
    std::fs::create_dir(root.join("dir")).unwrap();
    std::fs::write(root.join("dir").join("inner.txt"), "inner").unwrap();

    let fs = RealFs::new(None, executor);
    assert_copy_and_remove_semantics(root, &fs).await;

    // Creating symlinks requires elevated privileges on Windows, so like the
    // watcher tests above, skip that part when it is not possible.
    match make_dir_symlink(&root.join("dir"), &root.join("link")) {
        Ok(()) => assert_remove_file_unlinks_symlink(root, &fs).await,
        Err(error) => eprintln!("skipping symlink removal check (cannot symlink: {error})"),
    }
}

#[gpui::test]
async fn test_fake_fs_copy_and_remove_semantics(executor: BackgroundExecutor) {
    let fs = FakeFs::new(executor);
    fs.insert_tree(
        path!("/root"),
        json!({
            "source.txt": "from source",
            "target.txt": "from target",
            "dir": { "inner.txt": "inner" },
        }),
    )
    .await;
    fs.insert_symlink(path!("/root/link"), PathBuf::from(path!("/root/dir")))
        .await;

    let root = Path::new(path!("/root"));
    assert_copy_and_remove_semantics(root, fs.as_ref()).await;
    assert_remove_file_unlinks_symlink(root, fs.as_ref()).await;
}

#[gpui::test]
async fn test_fake_fs_rename_onto_itself_keeps_the_file(executor: BackgroundExecutor) {
    let fs = FakeFs::new(executor);
    fs.insert_tree(
        path!("/root"),
        json!({
            "a.txt": "content",
        }),
    )
    .await;

    let path = Path::new(path!("/root/a.txt"));
    let result = fs
        .rename(
            path,
            path,
            RenameOptions {
                overwrite: true,
                ..Default::default()
            },
        )
        .await;

    assert!(result.is_ok());
    assert_eq!(fs.load(path).await.unwrap(), "content");
}

#[gpui::test]
#[cfg(unix)]
async fn test_realfs_executable_metadata(executor: BackgroundExecutor) {
    use std::os::unix::fs::PermissionsExt as _;

    let tempdir = TempDir::new().unwrap();
    let path = tempdir.path();
    let non_executable_path = path.join("non-executable.sh");
    let executable_path = path.join("executable.sh");
    let symlink_path = path.join("executable-symlink.sh");

    std::fs::write(&non_executable_path, "#!/bin/sh\n").unwrap();
    std::fs::write(&executable_path, "#!/bin/sh\n").unwrap();
    let mut permissions = std::fs::metadata(&executable_path).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&executable_path, permissions).unwrap();

    let fs = RealFs::new(None, executor);
    gpui::block_on(fs.create_symlink(&symlink_path, PathBuf::from("executable.sh"))).unwrap();

    let non_executable_metadata = fs
        .metadata(&non_executable_path)
        .await
        .expect("metadata call succeeds")
        .expect("metadata returned");
    assert!(!non_executable_metadata.is_executable);

    let executable_metadata = fs
        .metadata(&executable_path)
        .await
        .expect("metadata call succeeds")
        .expect("metadata returned");
    assert!(executable_metadata.is_executable);

    let symlink_metadata = fs
        .metadata(&symlink_path)
        .await
        .expect("metadata call succeeds")
        .expect("metadata returned");
    assert!(symlink_metadata.is_symlink);
    assert!(symlink_metadata.is_executable);
}

#[gpui::test]
#[cfg(unix)]
async fn test_realfs_broken_symlink_metadata(executor: BackgroundExecutor) {
    let tempdir = TempDir::new().unwrap();
    let path = tempdir.path();
    let fs = RealFs::new(None, executor);
    let symlink_path = path.join("symlink");
    gpui::block_on(fs.create_symlink(&symlink_path, PathBuf::from("file_a.txt"))).unwrap();
    let metadata = fs
        .metadata(&symlink_path)
        .await
        .expect("metadata call succeeds")
        .expect("metadata returned");
    assert!(metadata.is_symlink);
    assert!(!metadata.is_dir);
    assert!(!metadata.is_fifo);
    assert!(!metadata.is_executable);
    // don't care about len or mtime on symlinks?
}

#[gpui::test]
#[cfg(unix)]
async fn test_realfs_symlink_loop_metadata(executor: BackgroundExecutor) {
    let tempdir = TempDir::new().unwrap();
    let path = tempdir.path();
    let fs = RealFs::new(None, executor);
    let symlink_path = path.join("symlink");
    gpui::block_on(fs.create_symlink(&symlink_path, PathBuf::from("symlink"))).unwrap();
    let metadata = fs
        .metadata(&symlink_path)
        .await
        .expect("metadata call succeeds")
        .expect("metadata returned");
    assert!(metadata.is_symlink);
    assert!(!metadata.is_dir);
    assert!(!metadata.is_fifo);
    assert!(!metadata.is_executable);
    // don't care about len or mtime on symlinks?
}

#[gpui::test]
async fn test_fake_fs_trash(executor: BackgroundExecutor) {
    let fs = FakeFs::new(executor.clone());
    fs.insert_tree(
        path!("/root"),
        json!({
            "src": {
                "file_c.txt": "File C",
                "file_d.txt": "File D"
            },
            "file_a.txt": "File A",
            "file_b.txt": "File B",
        }),
    )
    .await;

    // Trashing a file.
    let root_path = PathBuf::from(path!("/root"));
    let path = path!("/root/file_a.txt").as_ref();
    let trashed_entry = fs
        .trash(path, Default::default())
        .await
        .expect("should be able to trash {path:?}");

    assert_eq!(trashed_entry.name, "file_a.txt");
    assert_eq!(trashed_entry.original_parent, root_path);
    assert_eq!(
        fs.files(),
        vec![
            PathBuf::from(path!("/root/file_b.txt")),
            PathBuf::from(path!("/root/src/file_c.txt")),
            PathBuf::from(path!("/root/src/file_d.txt"))
        ]
    );

    let trash_entries = fs.trash_entries();
    assert_eq!(trash_entries.len(), 1);
    assert_eq!(trash_entries[0].name, "file_a.txt");
    assert_eq!(trash_entries[0].original_parent, root_path);

    // Trashing a directory.
    let path = path!("/root/src").as_ref();
    let trashed_entry = fs
        .trash(
            path,
            RemoveOptions {
                recursive: true,
                ..Default::default()
            },
        )
        .await
        .expect("should be able to trash {path:?}");

    assert_eq!(trashed_entry.name, "src");
    assert_eq!(trashed_entry.original_parent, root_path);
    assert_eq!(fs.files(), vec![PathBuf::from(path!("/root/file_b.txt"))]);

    let trash_entries = fs.trash_entries();
    assert_eq!(trash_entries.len(), 2);
    assert_eq!(trash_entries[1].name, "src");
    assert_eq!(trash_entries[1].original_parent, root_path);
}

#[gpui::test]
async fn test_fake_fs_restore(executor: BackgroundExecutor) {
    let fs = FakeFs::new(executor.clone());
    fs.insert_tree(
        path!("/root"),
        json!({
            "src": {
                "file_a.txt": "File A",
                "file_b.txt": "File B",
            },
            "file_c.txt": "File C",
        }),
    )
    .await;

    // Providing a non-existent `TrashedEntry` should result in an error.
    let id = OsString::from("/trash/file_c.txt");
    let name = OsString::from("file_c.txt");
    let original_parent = PathBuf::from(path!("/root"));
    let trashed_entry = TrashedEntry {
        id,
        name,
        original_parent,
    };
    let result = fs.restore(trashed_entry).await;
    assert!(matches!(result, Err(TrashRestoreError::NotFound { .. })));

    // Attempt deleting a file, asserting that the filesystem no longer reports
    // it as part of its list of files, restore it and verify that the list of
    // files and trash has been updated accordingly.
    let path = path!("/root/src/file_a.txt").as_ref();
    let trashed_entry = fs.trash(path, Default::default()).await.unwrap();

    assert_eq!(fs.trash_entries().len(), 1);
    assert_eq!(
        fs.files(),
        vec![
            PathBuf::from(path!("/root/file_c.txt")),
            PathBuf::from(path!("/root/src/file_b.txt"))
        ]
    );

    fs.restore(trashed_entry).await.unwrap();

    assert_eq!(fs.trash_entries().len(), 0);
    assert_eq!(
        fs.files(),
        vec![
            PathBuf::from(path!("/root/file_c.txt")),
            PathBuf::from(path!("/root/src/file_a.txt")),
            PathBuf::from(path!("/root/src/file_b.txt"))
        ]
    );

    // Deleting and restoring a directory should also remove all of its files
    // but create a single trashed entry, which should be removed after
    // restoration.
    let options = RemoveOptions {
        recursive: true,
        ..Default::default()
    };
    let path = path!("/root/src/").as_ref();
    let trashed_entry = fs.trash(path, options).await.unwrap();

    assert_eq!(fs.trash_entries().len(), 1);
    assert_eq!(fs.files(), vec![PathBuf::from(path!("/root/file_c.txt"))]);

    fs.restore(trashed_entry).await.unwrap();

    assert_eq!(
        fs.files(),
        vec![
            PathBuf::from(path!("/root/file_c.txt")),
            PathBuf::from(path!("/root/src/file_a.txt")),
            PathBuf::from(path!("/root/src/file_b.txt"))
        ]
    );
    assert_eq!(fs.trash_entries().len(), 0);

    // A collision error should be returned in case a file is being restored to
    // a path where a file already exists.
    let path = path!("/root/src/file_a.txt").as_ref();
    let trashed_entry = fs.trash(path, Default::default()).await.unwrap();

    assert_eq!(fs.trash_entries().len(), 1);
    assert_eq!(
        fs.files(),
        vec![
            PathBuf::from(path!("/root/file_c.txt")),
            PathBuf::from(path!("/root/src/file_b.txt"))
        ]
    );

    fs.write(path, b"New File A").await.unwrap();

    assert_eq!(fs.trash_entries().len(), 1);
    assert_eq!(
        fs.files(),
        vec![
            PathBuf::from(path!("/root/file_c.txt")),
            PathBuf::from(path!("/root/src/file_a.txt")),
            PathBuf::from(path!("/root/src/file_b.txt"))
        ]
    );

    let file_contents = fs.files_with_contents(path);
    assert!(fs.restore(trashed_entry).await.is_err());
    assert_eq!(
        file_contents,
        vec![(PathBuf::from(path), b"New File A".to_vec())]
    );

    // A collision error should be returned in case a directory is being
    // restored to a path where a directory already exists.
    let options = RemoveOptions {
        recursive: true,
        ..Default::default()
    };
    let path = path!("/root/src/").as_ref();
    let trashed_entry = fs.trash(path, options).await.unwrap();

    assert_eq!(fs.trash_entries().len(), 2);
    assert_eq!(fs.files(), vec![PathBuf::from(path!("/root/file_c.txt"))]);

    fs.create_dir(path).await.unwrap();

    assert_eq!(fs.files(), vec![PathBuf::from(path!("/root/file_c.txt"))]);
    assert_eq!(fs.trash_entries().len(), 2);

    let result = fs.restore(trashed_entry).await;
    assert!(result.is_err());

    assert_eq!(fs.files(), vec![PathBuf::from(path!("/root/file_c.txt"))]);
    assert_eq!(fs.trash_entries().len(), 2);
}

fn make_dir_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link)
    }
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_dir(target, link)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (target, link);
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "symlinks are not supported on this platform",
        ))
    }
}

async fn watcher_delivered_event(
    events: &mut (impl futures::Stream<Item = Vec<PathEvent>> + Unpin),
    executor: &BackgroundExecutor,
    timeout: Duration,
    path_matches: &(dyn Fn(&Path) -> bool + Send + Sync),
) -> bool {
    let timeout = executor.timer(timeout).fuse();
    futures::pin_mut!(timeout);
    loop {
        futures::select_biased! {
            batch = events.next().fuse() => {
                let Some(batch) = batch else { return false };
                let covered = batch.iter().any(|event| {
                    path_matches(&event.path) || event.kind == Some(PathEventKind::Rescan)
                });
                if covered {
                    return true;
                }
            }
            _ = timeout => return false,
        }
    }
}

#[gpui::test]
async fn test_realfs_watch_aliased_watch_paths_deliver_events(
    executor: BackgroundExecutor,
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();

    let fs = RealFs::new(None, executor.clone());
    let temp_dir = TempDir::new().expect("create temp dir");
    let root = temp_dir.path().to_path_buf();
    let latency = Duration::from_millis(10);

    std::fs::create_dir_all(root.join("CaseProbe")).expect("create case probe dir");
    let case_insensitive = root.join("caseprobe").exists();

    struct Scenario {
        name: &'static str,
        events: Pin<Box<dyn Send + futures::Stream<Item = Vec<PathEvent>>>>,
        _watcher: Arc<dyn Watcher>,
        path_matches: Box<dyn Fn(&Path) -> bool + Send + Sync>,
        action: Option<Box<dyn FnOnce() + Send>>,
    }

    let mut scenarios: Vec<Scenario> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();

    {
        let real = root.join("ancestor_real");
        let inner = real.join("inner");
        std::fs::create_dir_all(&inner).expect("create symlinked-ancestor target");
        let link = root.join("ancestor_link");
        match make_dir_symlink(&real, &link) {
            Ok(()) => {
                let (events, watcher) = fs.watch(&link.join("inner"), latency).await;
                let file = inner.join("symlink_ancestor.txt");
                scenarios.push(Scenario {
                    name: "symlink_ancestor",
                    events,
                    _watcher: watcher,
                    path_matches: Box::new(|path| {
                        path.ends_with(Path::new("inner/symlink_ancestor.txt"))
                    }),
                    action: Some(Box::new(move || {
                        std::fs::write(&file, b"x").expect("write symlink-ancestor file");
                    })),
                });
            }
            Err(error) => skipped.push(format!("symlink_ancestor (cannot symlink: {error})")),
        }
    }

    {
        let real = root.join("root_real");
        std::fs::create_dir_all(&real).expect("create symlinked-root target");
        let link = root.join("root_link");
        match make_dir_symlink(&real, &link) {
            Ok(()) => {
                let (events, watcher) = fs.watch(&link, latency).await;
                let file = real.join("symlink_root.txt");
                scenarios.push(Scenario {
                    name: "symlink_root",
                    events,
                    _watcher: watcher,
                    path_matches: Box::new(|path| path.ends_with(Path::new("symlink_root.txt"))),
                    action: Some(Box::new(move || {
                        std::fs::write(&file, b"x").expect("write symlink-root file");
                    })),
                });
            }
            Err(error) => skipped.push(format!("symlink_root (cannot symlink: {error})")),
        }
    }

    if case_insensitive {
        let real = root.join("CaseAlpha");
        std::fs::create_dir_all(&real).expect("create wrong-case root");
        let lower = PathBuf::from(real.to_string_lossy().to_lowercase());
        let (events, watcher) = fs.watch(&lower, latency).await;
        let file = real.join("alpha.txt");
        scenarios.push(Scenario {
            name: "wrong_case_root",
            events,
            _watcher: watcher,
            path_matches: Box::new(|path| path.ends_with(Path::new("alpha.txt"))),
            action: Some(Box::new(move || {
                std::fs::write(&file, b"x").expect("write wrong-case-root file");
            })),
        });
    } else {
        skipped.push("wrong_case_root (case-sensitive fs)".to_owned());
    }

    if case_insensitive {
        let real = root.join("CaseBravo").join("Inner");
        std::fs::create_dir_all(&real).expect("create wrong-case nested dir");
        let lower = PathBuf::from(real.to_string_lossy().to_lowercase());
        let (events, watcher) = fs.watch(&lower, latency).await;
        let file = real.join("bravo.txt");
        scenarios.push(Scenario {
            name: "nested_wrong_case",
            events,
            _watcher: watcher,
            path_matches: Box::new(|path| path.ends_with(Path::new("bravo.txt"))),
            action: Some(Box::new(move || {
                std::fs::write(&file, b"x").expect("write nested-wrong-case file");
            })),
        });
    } else {
        skipped.push("nested_wrong_case (case-sensitive fs)".to_owned());
    }

    let mut failures = Vec::new();
    for scenario in &mut scenarios {
        if let Some(action) = scenario.action.take() {
            action();
        }
        if !watcher_delivered_event(
            &mut scenario.events,
            &executor,
            Duration::from_secs(10),
            scenario.path_matches.as_ref(),
        )
        .await
        {
            failures.push(scenario.name);
        }
    }

    assert!(
        failures.is_empty(),
        "watch scenarios that never delivered an event: {failures:?}; skipped: {skipped:?}"
    );
}

#[gpui::test]
#[ignore = "stress test; run explicitly when needed"]
async fn test_realfs_watch_stress_reports_missed_paths(
    executor: BackgroundExecutor,
    cx: &mut TestAppContext,
) {
    const FILE_COUNT: usize = 32000;
    cx.executor().allow_parking();

    let fs = RealFs::new(None, executor.clone());
    let temp_dir = TempDir::new().expect("create temp dir");
    let root = temp_dir.path();

    let mut file_paths = Vec::with_capacity(FILE_COUNT);
    let mut expected_paths = BTreeSet::new();

    for index in 0..FILE_COUNT {
        let dir_path = root.join(format!("dir-{index:04}"));
        let file_path = dir_path.join("file.txt");
        fs.create_dir(&dir_path).await.expect("create watched dir");
        fs.write(&file_path, b"before")
            .await
            .expect("create initial file");
        expected_paths.insert(file_path.clone());
        file_paths.push(file_path);
    }

    let (mut events, watcher) = fs.watch(root, Duration::from_millis(10)).await;
    let _watcher = watcher;

    for file_path in &expected_paths {
        _watcher
            .add(file_path.parent().expect("file has parent"))
            .expect("add explicit directory watch");
    }

    for (index, file_path) in file_paths.iter().enumerate() {
        let content = format!("after-{index}");
        fs.write(file_path, content.as_bytes())
            .await
            .expect("modify watched file");
    }

    let mut changed_paths = BTreeSet::new();
    let mut rescan_count: u32 = 0;
    let timeout = executor.timer(Duration::from_secs(10)).fuse();

    futures::pin_mut!(timeout);

    let mut ticks = 0;
    while ticks < 1000 {
        if let Some(batch) = events.next().fuse().now_or_never().flatten() {
            for event in batch {
                if event.kind == Some(PathEventKind::Rescan) {
                    rescan_count += 1;
                }
                if expected_paths.contains(&event.path) {
                    changed_paths.insert(event.path);
                }
            }
            if changed_paths.len() == expected_paths.len() {
                break;
            }
            ticks = 0;
        } else {
            ticks += 1;
            executor.timer(Duration::from_millis(10)).await;
        }
    }

    let missed_paths: BTreeSet<_> = expected_paths.difference(&changed_paths).cloned().collect();

    eprintln!(
        "realfs watch stress: expected={}, observed={}, missed={}, rescan={}",
        expected_paths.len(),
        changed_paths.len(),
        missed_paths.len(),
        rescan_count
    );

    assert!(
        missed_paths.is_empty() || rescan_count > 0,
        "missed {} paths without rescan being reported",
        missed_paths.len()
    );
}
