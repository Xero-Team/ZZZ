use gpui::{AppContext as _, TestAppContext, WeakEntity};

use crate::session::{SessionId, SessionStore};

#[gpui::test]
fn activating_the_same_session_is_idempotent(cx: &mut TestAppContext) {
    let store = cx.new(|_| SessionStore::new(WeakEntity::new_invalid(), None));
    let session_id = SessionId(3);

    store.update(cx, |store, cx| store.set_active_session(session_id, cx));
    store.update(cx, |store, cx| store.set_active_session(session_id, cx));

    assert_eq!(
        store.read_with(cx, |store, _| store.active_session),
        Some(session_id)
    );
}
