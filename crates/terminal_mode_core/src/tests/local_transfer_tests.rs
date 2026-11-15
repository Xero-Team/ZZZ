use std::path::Path;

use crate::local_transfer::{LOCAL_MOUNT_ID, local_vfs_path};
use crate::transfer::mount_id_for_session;

#[test]
fn local_vfs_paths_are_rooted_at_the_filesystem_root() {
    #[cfg(unix)]
    {
        let path = local_vfs_path(Path::new("/tmp/zzz-terminal-mode/file.txt"));
        let path = path.expect("local path must encode");
        assert_eq!(path.mount_id(), LOCAL_MOUNT_ID);
        assert_eq!(
            path.provider_path().display(),
            "tmp/zzz-terminal-mode/file.txt"
        );
    }
}

#[test]
fn local_mount_id_never_collides_with_session_mounts() {
    for session_id in 0..1_000 {
        assert_ne!(mount_id_for_session(session_id), LOCAL_MOUNT_ID);
    }
}
