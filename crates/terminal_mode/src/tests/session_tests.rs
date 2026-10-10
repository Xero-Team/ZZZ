use gpui::{AppContext as _, TestAppContext, WeakEntity};

use crate::session::{SessionId, SessionStore};

fn test_store(cx: &mut TestAppContext) -> gpui::Entity<SessionStore> {
    cx.new(|_| SessionStore::new(WeakEntity::new_invalid(), None))
}

#[gpui::test]
fn session_ids_are_monotonic_and_never_reused(cx: &mut TestAppContext) {
    let store = test_store(cx);
    let first = store.update(cx, |store, _| store.next_session_id());
    let second = store.update(cx, |store, _| store.next_session_id());
    let third = store.update(cx, |store, _| store.next_session_id());

    assert_eq!(first, SessionId(0));
    assert_eq!(second, SessionId(1));
    assert_eq!(third, SessionId(2));
}

#[gpui::test]
fn set_active_session_tracks_the_selection(cx: &mut TestAppContext) {
    let store = test_store(cx);
    let id = SessionId(4);

    store.update(cx, |store, cx| store.set_active_session(id, cx));
    assert_eq!(
        store.read_with(cx, |store, _| store.active_session),
        Some(id)
    );

    store.update(cx, |store, cx| store.set_active_session(id, cx));
    assert_eq!(
        store.read_with(cx, |store, _| store.active_session),
        Some(id)
    );
}

#[gpui::test]
fn removing_an_unknown_session_returns_none(cx: &mut TestAppContext) {
    let store = test_store(cx);
    let removed = store.update(cx, |store, cx| store.remove_session(SessionId(7), cx));
    assert!(removed.is_none());
    assert_eq!(store.read_with(cx, |store, _| store.active_session), None);
}

#[gpui::test]
fn unknown_session_lookup_returns_none(cx: &mut TestAppContext) {
    let store = test_store(cx);
    assert!(store.read_with(cx, |store, _| store.session(SessionId(0)).is_none()));
}
