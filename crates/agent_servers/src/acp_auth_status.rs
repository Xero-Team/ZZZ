use super::ClientContext;
use agent_client_protocol::v1::schema as acp;
use gpui::{App, AsyncApp, Task};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// The latest `auth/status` snapshot for one live agent process.
///
/// A snapshot belongs to the process, not to a session or a saved agent
/// configuration, so notifications and query replies for every session share
/// one value.
pub(super) struct ConnectionAuthStatus {
    snapshots: RefCell<watch::Sender<Option<acp::AuthStatusResponse>>>,
    generation: Cell<u64>,
    query: RefCell<Option<Task<()>>>,
}

impl ConnectionAuthStatus {
    pub(super) fn new() -> Rc<Self> {
        Rc::new(Self {
            snapshots: RefCell::new(watch::channel(None).0),
            generation: Cell::new(0),
            query: RefCell::new(None),
        })
    }

    pub(super) fn receiver(&self) -> watch::Receiver<Option<acp::AuthStatusResponse>> {
        self.snapshots.borrow().receiver()
    }

    pub(super) fn query(
        self: &Rc<Self>,
        response: impl Future<Output = Option<acp::AuthStatusResponse>> + 'static,
        cx: &App,
    ) {
        let generation = self.generation.get();
        let status = Rc::downgrade(self);
        *self.query.borrow_mut() = Some(cx.foreground_executor().spawn(async move {
            let response = response.await;
            if let Some(status) = status.upgrade()
                && status.generation.get() == generation
            {
                // An optional probe failing isn't a failed login or a connection error.
                status.publish(
                    response
                        .unwrap_or_else(|| acp::AuthStatusResponse::new(acp::AuthState::Unknown)),
                );
            }
        }));
    }

    fn publish(&self, snapshot: acp::AuthStatusResponse) {
        // Even an identical notification is a newer observation than an outstanding query.
        self.generation.set(self.generation.get() + 1);
        // The latest value is retained even when no view is watching.
        self.snapshots.borrow_mut().send(Some(snapshot)).ok();
    }

    pub(super) fn disconnect(&self) {
        self.query.borrow_mut().take();
        self.publish(acp::AuthStatusResponse::new(acp::AuthState::Unknown));
    }
}

pub(super) fn update(
    update: acp::AuthStatusUpdateNotification,
    _: &mut AsyncApp,
    context: &ClientContext,
) {
    if let Some(status) = context.auth_status.upgrade() {
        status.publish(acp::AuthStatusResponse::new(update.state).accounts(update.accounts));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    #[gpui::test]
    async fn notification_supersedes_pending_auth_status_query(cx: &mut TestAppContext) {
        let status = ConnectionAuthStatus::new();
        let (reply, pending) = futures::channel::oneshot::channel();
        cx.update(|cx| status.query(async move { pending.await.ok() }, cx));
        let updated = acp::AuthStatusResponse::new(acp::AuthState::Unauthenticated);
        status.publish(updated.clone());
        reply
            .send(acp::AuthStatusResponse::new(acp::AuthState::Authenticated))
            .expect("pending query");
        cx.run_until_parked();
        assert_eq!(*status.receiver().borrow(), Some(updated));
    }

    #[gpui::test]
    async fn failed_probe_is_not_an_error_and_disconnect_clears_the_snapshot(
        cx: &mut TestAppContext,
    ) {
        let status = ConnectionAuthStatus::new();
        assert_eq!(*status.receiver().borrow(), None);

        // A probe the agent cannot answer publishes `Unknown` rather than an error.
        cx.update(|cx| status.query(async { None }, cx));
        cx.run_until_parked();
        assert_eq!(
            *status.receiver().borrow(),
            Some(acp::AuthStatusResponse::new(acp::AuthState::Unknown))
        );

        let (reply, pending) = futures::channel::oneshot::channel();
        cx.update(|cx| status.query(async move { pending.await.ok() }, cx));
        status.disconnect();
        assert_eq!(
            *status.receiver().borrow(),
            Some(acp::AuthStatusResponse::new(acp::AuthState::Unknown))
        );
        // The disconnect dropped the outstanding query, so a late reply cannot revive a
        // snapshot for a process that is gone.
        reply
            .send(acp::AuthStatusResponse::new(acp::AuthState::Authenticated))
            .ok();
        cx.run_until_parked();
        assert_eq!(
            *status.receiver().borrow(),
            Some(acp::AuthStatusResponse::new(acp::AuthState::Unknown))
        );
    }
}
