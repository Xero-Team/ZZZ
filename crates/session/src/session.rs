use db::kvp::KeyValueStore;
use gpui::{App, AppContext as _, Context, Subscription, Task, WindowId};
use util::ResultExt;

pub struct Session {
    session_id: String,
    old_session_id: Option<String>,
    old_window_ids: Option<Vec<WindowId>>,
}

const SESSION_ID_KEY: &str = "session_id";
const SESSION_WINDOW_STACK_KEY: &str = "session_window_stack";

impl Session {
    pub async fn new(session_id: String, db: KeyValueStore) -> Self {
        let old_session_id = db.read_kvp(SESSION_ID_KEY).ok().flatten();

        db.write_kvp(SESSION_ID_KEY.to_owned(), session_id.clone())
            .await
            .log_err();

        let old_window_ids = db
            .read_kvp(SESSION_WINDOW_STACK_KEY)
            .ok()
            .flatten()
            .and_then(|json| serde_json::from_str::<Vec<u64>>(&json).ok())
            .map(|vec: Vec<u64>| {
                vec.into_iter()
                    .map(WindowId::from)
                    .collect::<Vec<WindowId>>()
            });

        Self {
            session_id,
            old_session_id,
            old_window_ids,
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn test() -> Self {
        Self {
            session_id: uuid::Uuid::new_v4().to_string(),
            old_session_id: None,
            old_window_ids: None,
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn test_with_old_session(old_session_id: String) -> Self {
        Self {
            session_id: uuid::Uuid::new_v4().to_string(),
            old_session_id: Some(old_session_id),
            old_window_ids: None,
        }
    }

    pub fn id(&self) -> &str {
        &self.session_id
    }
}

pub struct AppSession {
    session: Session,
    _serialization_task: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl AppSession {
    pub fn new(session: Session, cx: &Context<Self>) -> Self {
        let _subscriptions = vec![cx.on_app_quit(Self::app_will_quit)];

        let _serialization_task = if cfg!(not(any(test, feature = "test-support"))) {
            let db = KeyValueStore::global(cx);
            cx.spawn(async move |_, cx| {
                // Disabled in tests: the infinite loop bypasses "parking forbidden" checks,
                // causing tests to hang instead of panicking.
                {
                    let mut current_window_stack = Vec::new();
                    loop {
                        if let Some(windows) = cx.update(|cx| window_stack(cx))
                            && windows != current_window_stack
                        {
                            store_window_stack(db.clone(), &windows).await;
                            current_window_stack = windows;
                        }

                        cx.background_executor()
                            .timer(std::time::Duration::from_millis(500))
                            .await;
                    }
                }
            })
        } else {
            Task::ready(())
        };

        Self {
            session,
            _subscriptions,
            _serialization_task,
        }
    }

    fn app_will_quit(&mut self, cx: &mut Context<Self>) -> Task<()> {
        if let Some(window_stack) = window_stack(cx) {
            let db = KeyValueStore::global(cx);
            cx.background_spawn(async move { store_window_stack(db, &window_stack).await })
        } else {
            Task::ready(())
        }
    }

    pub fn id(&self) -> &str {
        self.session.id()
    }

    pub fn last_session_id(&self) -> Option<&str> {
        self.session.old_session_id.as_deref()
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn replace_session_for_test(&mut self, session: Session) {
        self.session = session;
    }

    pub fn last_session_window_stack(&self) -> Option<Vec<WindowId>> {
        self.session.old_window_ids.clone()
    }
}

fn window_stack(cx: &App) -> Option<Vec<u64>> {
    Some(
        cx.window_stack()?
            .into_iter()
            .map(|window| window.window_id().as_u64())
            .collect(),
    )
}

async fn store_window_stack(db: KeyValueStore, windows: &[u64]) {
    if let Ok(window_ids_json) = serde_json::to_string(windows) {
        db.write_kvp(SESSION_WINDOW_STACK_KEY.to_owned(), window_ids_json)
            .await
            .log_err();
    }
}

#[cfg(test)]
mod tests {
    use super::{SESSION_ID_KEY, SESSION_WINDOW_STACK_KEY, Session, store_window_stack};
    use db::kvp::KeyValueStore;
    use gpui::WindowId;

    #[gpui::test]
    async fn session_new_restores_previous_values() {
        let db = KeyValueStore::open_test_db("session_restores_previous_values").await;
        db.write_kvp(SESSION_ID_KEY.to_string(), "old-session".to_string())
            .await
            .unwrap();
        db.write_kvp(
            SESSION_WINDOW_STACK_KEY.to_string(),
            "[7,11,42]".to_string(),
        )
        .await
        .unwrap();

        let session = Session::new("new-session".to_string(), db.clone()).await;

        assert_eq!(session.id(), "new-session");
        assert_eq!(session.old_session_id.as_deref(), Some("old-session"));
        assert_eq!(
            session.old_window_ids,
            Some(vec![
                WindowId::from(7_u64),
                WindowId::from(11_u64),
                WindowId::from(42_u64),
            ])
        );
        assert_eq!(
            db.read_kvp(SESSION_ID_KEY).unwrap(),
            Some("new-session".to_string())
        );
    }

    #[gpui::test]
    async fn session_new_ignores_invalid_window_stack_json() {
        let db = KeyValueStore::open_test_db("session_ignores_invalid_window_stack").await;
        db.write_kvp(SESSION_WINDOW_STACK_KEY.to_string(), "not-json".to_string())
            .await
            .unwrap();

        let session = Session::new("new-session".to_string(), db).await;

        assert_eq!(session.old_window_ids, None);
    }

    #[gpui::test]
    async fn store_window_stack_writes_json_array() {
        let db = KeyValueStore::open_test_db("session_store_window_stack").await;

        store_window_stack(db.clone(), &[3, 5, 8]).await;

        assert_eq!(
            db.read_kvp(SESSION_WINDOW_STACK_KEY).unwrap(),
            Some("[3,5,8]".to_string())
        );
    }
}
