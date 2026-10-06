use crate::{
    MountId, ProviderId, SnapshotBudgets, VfsError, VfsErrorCode, VfsOperation, VfsProvider,
    VfsResult, VfsSnapshot,
};
use parking_lot::RwLock;
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, Default)]
pub struct VfsManager {
    snapshots: Arc<RwLock<BTreeMap<MountId, VfsSnapshot>>>,
}

impl VfsManager {
    pub fn mount(
        &self,
        provider: Arc<dyn VfsProvider>,
        budgets: SnapshotBudgets,
    ) -> VfsResult<VfsSnapshot> {
        let snapshot = VfsSnapshot::mount(provider, budgets)?;
        self.register(snapshot.clone())?;
        Ok(snapshot)
    }

    pub fn register(&self, snapshot: VfsSnapshot) -> VfsResult<()> {
        let mount_id = snapshot.registry().mount_id();
        match self.snapshots.write().entry(mount_id) {
            std::collections::btree_map::Entry::Occupied(_) => Err(manager_error(
                VfsErrorCode::AlreadyExists,
                mount_id,
                "VFS mount ID is already registered",
            )),
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(snapshot);
                Ok(())
            }
        }
    }

    pub fn snapshot(&self, mount_id: MountId) -> VfsResult<VfsSnapshot> {
        self.snapshots
            .read()
            .get(&mount_id)
            .cloned()
            .ok_or_else(|| {
                manager_error(
                    VfsErrorCode::NotFound,
                    mount_id,
                    "VFS mount is not registered",
                )
            })
    }

    pub fn unmount(&self, mount_id: MountId) -> VfsResult<VfsSnapshot> {
        self.snapshots.write().remove(&mount_id).ok_or_else(|| {
            manager_error(
                VfsErrorCode::NotFound,
                mount_id,
                "VFS mount is not registered",
            )
        })
    }

    pub fn mount_count(&self) -> usize {
        self.snapshots.read().len()
    }
}

fn manager_error(code: VfsErrorCode, mount_id: MountId, detail: &'static str) -> VfsError {
    VfsError::new(code, VfsOperation::Stat, ProviderId::new("vfs-manager"))
        .with_detail(format!("{detail}: {}", mount_id.get()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MemoryProvider, PathEncoding};

    #[test]
    fn manager_registers_and_unmounts_snapshots() {
        let manager = VfsManager::default();
        let snapshot = manager.mount(
            Arc::new(MemoryProvider::new(
                "manager-test",
                PathEncoding::PortableUtf8,
            )),
            SnapshotBudgets::default(),
        );
        let snapshot = match snapshot {
            Ok(snapshot) => snapshot,
            Err(error) => panic!("manager mount failed: {error:?}"),
        };
        let mount_id = snapshot.registry().mount_id();
        assert_eq!(manager.mount_count(), 1);
        assert!(manager.snapshot(mount_id).is_ok());
        assert!(
            manager
                .register(snapshot)
                .as_ref()
                .err()
                .is_some_and(|error| error.code() == VfsErrorCode::AlreadyExists)
        );
        assert!(manager.unmount(mount_id).is_ok());
        assert_eq!(manager.mount_count(), 0);
    }
}
