#![expect(clippy::result_large_err)]

use crate::{
    debugger_panel::DebugPanel,
    persistence::DebuggerPaneItem,
    tests::{active_debug_session_panel, init_test, init_test_workspace, start_debug_session},
};
use dap::{
    StoppedEvent,
    requests::{Initialize, LoadedSources},
};
use gpui::{BackgroundExecutor, TestAppContext, VisualTestContext};
use project::{FakeFs, Project};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use util::path;

fn source(name: &str) -> dap::Source {
    dap::Source {
        name: Some(name.to_owned()),
        path: Some(format!("/project/{name}")),
        source_reference: None,
        presentation_hint: None,
        origin: None,
        sources: None,
        adapter_data: None,
        checksums: None,
    }
}

#[gpui::test]
async fn test_loaded_source_events_refresh_the_list(
    executor: BackgroundExecutor,
    cx: &mut TestAppContext,
) {
    init_test(cx);

    let project = Project::test(FakeFs::new(executor), [path!("/project").as_ref()], cx).await;
    let workspace = init_test_workspace(&project, cx).await;
    workspace
        .update(cx, |workspace, window, cx| {
            workspace.focus_panel::<DebugPanel>(window, cx);
        })
        .unwrap();
    let cx = &mut VisualTestContext::from_window(*workspace, cx);

    let request_count = Arc::new(AtomicUsize::new(0));
    let session = start_debug_session(&workspace, cx, {
        let request_count = request_count.clone();
        move |client| {
            client.on_request::<Initialize, _>(move |_, _| {
                Ok(dap::Capabilities {
                    supports_loaded_sources_request: Some(true),
                    ..Default::default()
                })
            });
            client.on_request::<LoadedSources, _>({
                let request_count = request_count.clone();
                move |_, _| {
                    let source = if request_count.fetch_add(1, Ordering::SeqCst) == 0 {
                        source("old.rs")
                    } else {
                        source("new.rs")
                    };
                    Ok(dap::LoadedSourcesResponse {
                        sources: vec![source],
                    })
                }
            });
        }
    })
    .unwrap();

    let client = session.update(cx, |session, _| session.adapter_client().unwrap());
    client
        .fake_event(dap::messages::Events::Stopped(StoppedEvent {
            reason: dap::StoppedEventReason::Pause,
            description: None,
            thread_id: Some(1),
            preserve_focus_hint: None,
            text: None,
            all_threads_stopped: None,
            hit_breakpoint_ids: None,
        }))
        .await;
    cx.run_until_parked();

    let running_state =
        active_debug_session_panel(workspace, cx).update_in(cx, |item, window, cx| {
            cx.focus_self(window);
            item.running_state().clone()
        });
    running_state.update_in(cx, |state, window, cx| {
        state.activate_item(DebuggerPaneItem::LoadedSources, window, cx);
        cx.refresh_windows();
    });
    cx.run_until_parked();

    let sources = running_state.update(cx, |state, cx| {
        state
            .loaded_source_list()
            .update(cx, |list, cx| list.sources(cx))
    });
    assert_eq!(sources, [source("old.rs")]);
    assert_eq!(request_count.load(Ordering::SeqCst), 1);

    client
        .fake_event(dap::messages::Events::LoadedSource(
            dap::LoadedSourceEvent {
                reason: dap::LoadedSourceEventReason::Changed,
                source: source("new.rs"),
            },
        ))
        .await;
    cx.run_until_parked();

    let sources = running_state.update(cx, |state, cx| {
        state
            .loaded_source_list()
            .update(cx, |list, cx| list.sources(cx))
    });
    assert_eq!(sources, [source("new.rs")]);
    assert_eq!(request_count.load(Ordering::SeqCst), 2);
}
