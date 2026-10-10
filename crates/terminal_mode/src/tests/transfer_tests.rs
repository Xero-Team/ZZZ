use crate::transfer::mount_id_for_session;

#[test]
fn session_mount_ids_are_unique_and_stable() {
    let first = mount_id_for_session(0);
    let second = mount_id_for_session(1);

    assert_ne!(first.get(), second.get());
    assert_eq!(first.get(), mount_id_for_session(0).get());
    assert!(first.get() > 0);
}
