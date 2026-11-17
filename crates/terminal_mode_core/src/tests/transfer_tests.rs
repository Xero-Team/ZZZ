use crate::session::SessionRoot;
use crate::transfer::{mount_id_for_session, mount_id_for_session_root};

#[test]
fn session_mount_ids_are_unique_and_stable() {
    let first = mount_id_for_session(0);
    let second = mount_id_for_session(1);

    assert_ne!(first.get(), second.get());
    assert_eq!(first.get(), mount_id_for_session(0).get());
    assert!(first.get() > 0);
}

#[test]
fn alternate_root_mounts_do_not_collide_with_home_mounts() {
    for session_id in 0..1_000 {
        let home = mount_id_for_session_root(session_id, SessionRoot::Home);
        let filesystem = mount_id_for_session_root(session_id, SessionRoot::Filesystem);
        assert_ne!(home, filesystem);
        assert_eq!(home, mount_id_for_session(session_id));
    }
}
