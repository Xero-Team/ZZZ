use std::{future, sync::Arc, time::Duration};

use fs::{FakeFs, Fs as _};
use gpui::TestAppContext;
use http_client::{AsyncBody, FakeHttpClient, HttpClient, Response};
use project::{AgentRegistryStore, RegistryAgent};
use serde_json::json;

use crate::init_test;

#[gpui::test]
async fn registry_refresh_times_out_when_fetch_never_completes(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let http_client =
        FakeHttpClient::create(|_| future::pending::<anyhow::Result<Response<AsyncBody>>>())
            as Arc<dyn HttpClient>;

    let registry_store =
        cx.update(|cx| AgentRegistryStore::init_global(cx, fs.clone(), http_client));
    cx.run_until_parked();

    registry_store.update(cx, |store, cx| store.refresh(cx));
    cx.run_until_parked();

    cx.executor().advance_clock(Duration::from_secs(31));
    cx.run_until_parked();

    registry_store.update(cx, |store, _| {
        assert!(!store.is_fetching());
        assert!(
            store
                .fetch_error()
                .is_some_and(|error| error.contains("timed out after 30s")),
            "expected registry fetch timeout error, got {:?}",
            store.fetch_error()
        );
    });
}

#[gpui::test]
async fn registry_refresh_does_not_block_sequentially_on_hung_icon_downloads(
    cx: &mut TestAppContext,
) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let http_client = FakeHttpClient::create(|request| async move {
        if request.uri().to_string().contains("registry.json") {
            Ok(Response::builder()
                .status(200)
                .body(AsyncBody::from(
                    serde_json::to_string(&json!({
                        "version": "1",
                        "agents": [
                            {
                                "id": "slow-icon-a",
                                "name": "Slow Icon A",
                                "version": "1.0.0",
                                "description": "An agent with a slow icon.",
                                "icon": "https://example.test/slow-icon-a.svg",
                                "distribution": {
                                    "npx": {
                                        "package": "slow-icon-a"
                                    }
                                }
                            },
                            {
                                "id": "slow-icon-b",
                                "name": "Slow Icon B",
                                "version": "1.0.0",
                                "description": "Another agent with a slow icon.",
                                "icon": "https://example.test/slow-icon-b.svg",
                                "distribution": {
                                    "npx": {
                                        "package": "slow-icon-b"
                                    }
                                }
                            },
                            {
                                "id": "slow-icon-c",
                                "name": "Slow Icon C",
                                "version": "1.0.0",
                                "description": "A third agent with a slow icon.",
                                "icon": "https://example.test/slow-icon-c.svg",
                                "distribution": {
                                    "npx": {
                                        "package": "slow-icon-c"
                                    }
                                }
                            }
                        ]
                    }))
                    .unwrap(),
                ))
                .unwrap())
        } else {
            future::pending::<anyhow::Result<Response<AsyncBody>>>().await
        }
    }) as Arc<dyn HttpClient>;

    let registry_store =
        cx.update(|cx| AgentRegistryStore::init_global(cx, fs.clone(), http_client));
    cx.run_until_parked();

    registry_store.update(cx, |store, cx| store.refresh(cx));
    cx.run_until_parked();

    cx.executor().advance_clock(Duration::from_secs(11));
    cx.run_until_parked();

    registry_store.update(cx, |store, _| {
        assert!(!store.is_fetching());
        assert_eq!(store.agents().len(), 3);
        assert_eq!(store.agents()[0].id().as_ref(), "slow-icon-a");
        assert_eq!(store.agents()[1].id().as_ref(), "slow-icon-b");
        assert_eq!(store.agents()[2].id().as_ref(), "slow-icon-c");
        assert_eq!(store.fetch_error(), None);
    });
}

#[gpui::test]
async fn registry_refresh_rewrites_opencode_v1_to_latest_v2(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let http_client = FakeHttpClient::create(|request| async move {
        let uri = request.uri().to_string();
        if uri.contains("registry.json") {
            ok_json(opencode_registry_json("1.18.31", github_v1_archive))
        } else if uri.contains("update/api/latest/cli/npm") {
            ok_json(json!({ "version": "2.0.7" }))
        } else {
            not_found()
        }
    }) as Arc<dyn HttpClient>;

    let registry_store =
        cx.update(|cx| AgentRegistryStore::init_global(cx, fs.clone(), http_client));
    cx.run_until_parked();

    registry_store.update(cx, |store, cx| store.refresh(cx));
    cx.run_until_parked();

    registry_store.update(cx, |store, _| {
        assert_eq!(store.fetch_error(), None);
        assert_opencode_binary(store, "2.0.7", "https://opencode.ai/files/bin/2.0.7");
    });

    let cached = fs
        .load_bytes(&registry_cache_path())
        .await
        .expect("cached registry");
    let cached = String::from_utf8(cached).expect("cached registry utf8");
    assert!(cached.contains("2.0.7"));
    assert!(cached.contains("https://opencode.ai/files/bin/2.0.7/opencode-linux-x64.tar.gz"));
    assert!(!cached.contains("github.com"));
    assert!(!cached.contains("sha256"));
}

#[gpui::test]
async fn registry_refresh_uses_opencode_v2_fallback_when_latest_api_fails(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let http_client = FakeHttpClient::create(|request| async move {
        let uri = request.uri().to_string();
        if uri.contains("registry.json") {
            ok_json(opencode_registry_json("1.18.31", github_v1_archive))
        } else {
            not_found()
        }
    }) as Arc<dyn HttpClient>;

    let registry_store =
        cx.update(|cx| AgentRegistryStore::init_global(cx, fs.clone(), http_client));
    cx.run_until_parked();

    registry_store.update(cx, |store, cx| store.refresh(cx));
    cx.run_until_parked();

    registry_store.update(cx, |store, _| {
        assert_eq!(store.fetch_error(), None);
        assert_opencode_binary(store, "2.0.6", "https://opencode.ai/files/bin/2.0.6");
    });
}

#[gpui::test]
async fn registry_refresh_keeps_opencode_v2_archives_from_opencode_ai(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let http_client = FakeHttpClient::create(|request| async move {
        let uri = request.uri().to_string();
        if uri.contains("registry.json") {
            ok_json(opencode_registry_json("2.0.5", opencode_ai_archive))
        } else if uri.contains("update/api/latest/cli/npm") {
            ok_json(json!({ "version": "2.0.7" }))
        } else {
            not_found()
        }
    }) as Arc<dyn HttpClient>;

    let registry_store =
        cx.update(|cx| AgentRegistryStore::init_global(cx, fs.clone(), http_client));
    cx.run_until_parked();

    registry_store.update(cx, |store, cx| store.refresh(cx));
    cx.run_until_parked();

    registry_store.update(cx, |store, _| {
        assert_eq!(store.fetch_error(), None);
        assert_opencode_binary(store, "2.0.5", "https://opencode.ai/files/bin/2.0.5");
    });
}

#[gpui::test]
async fn registry_refresh_rewrites_opencode_v2_github_archives(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let http_client = FakeHttpClient::create(|request| async move {
        let uri = request.uri().to_string();
        if uri.contains("registry.json") {
            ok_json(opencode_registry_json("2.0.6", github_v1_archive))
        } else if uri.contains("update/api/latest/cli/npm") {
            ok_json(json!({ "version": "2.0.7" }))
        } else {
            not_found()
        }
    }) as Arc<dyn HttpClient>;

    let registry_store =
        cx.update(|cx| AgentRegistryStore::init_global(cx, fs.clone(), http_client));
    cx.run_until_parked();

    registry_store.update(cx, |store, cx| store.refresh(cx));
    cx.run_until_parked();

    registry_store.update(cx, |store, _| {
        assert_eq!(store.fetch_error(), None);
        assert_opencode_binary(store, "2.0.6", "https://opencode.ai/files/bin/2.0.6");
    });
}

#[gpui::test]
async fn registry_cache_load_rewrites_stale_opencode_v1(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let cache_dir = paths::external_agents_dir().join("registry");
    fs.create_dir(&cache_dir).await.expect("registry cache dir");
    fs.write(
        &cache_dir.join("registry.json"),
        serde_json::to_string(&opencode_registry_json("1.18.31", github_v1_archive))
            .unwrap()
            .as_bytes(),
    )
    .await
    .expect("cached registry");

    let http_client = FakeHttpClient::with_404_response() as Arc<dyn HttpClient>;
    let registry_store =
        cx.update(|cx| AgentRegistryStore::init_global(cx, fs.clone(), http_client));
    cx.run_until_parked();

    registry_store.update(cx, |store, _| {
        assert_eq!(store.fetch_error(), None);
        assert_opencode_binary(store, "2.0.6", "https://opencode.ai/files/bin/2.0.6");
    });

    let cached = fs
        .load_bytes(&registry_cache_path())
        .await
        .expect("cached registry");
    let cached = String::from_utf8(cached).expect("cached registry utf8");
    assert!(cached.contains("https://opencode.ai/files/bin/2.0.6/opencode-linux-x64.tar.gz"));
    assert!(!cached.contains("github.com"));
}

fn assert_opencode_binary(store: &AgentRegistryStore, version: &str, archive_prefix: &str) {
    let Some(RegistryAgent::Binary(agent)) = store
        .agents()
        .iter()
        .find(|agent| agent.id().as_ref() == "opencode")
    else {
        panic!("expected OpenCode binary agent, got {:?}", store.agents());
    };

    assert_eq!(agent.metadata.version.as_ref(), version);
    assert_eq!(
        archive_for(agent, "darwin-aarch64"),
        format!("{archive_prefix}/opencode-darwin-arm64.zip")
    );
    assert_eq!(
        archive_for(agent, "darwin-x86_64"),
        format!("{archive_prefix}/opencode-darwin-x64.zip")
    );
    assert_eq!(
        archive_for(agent, "linux-aarch64"),
        format!("{archive_prefix}/opencode-linux-arm64.tar.gz")
    );
    assert_eq!(
        archive_for(agent, "linux-x86_64"),
        format!("{archive_prefix}/opencode-linux-x64.tar.gz")
    );
    assert_eq!(
        archive_for(agent, "windows-aarch64"),
        format!("{archive_prefix}/opencode-windows-arm64.zip")
    );
    assert_eq!(
        archive_for(agent, "windows-x86_64"),
        format!("{archive_prefix}/opencode-windows-x64.zip")
    );

    let linux = agent
        .targets
        .get("linux-x86_64")
        .expect("linux-x86_64 target");
    assert_eq!(linux.cmd, "./opencode");
    assert_eq!(linux.args, vec!["acp".to_string()]);
    assert_eq!(linux.sha256, None);

    let windows = agent
        .targets
        .get("windows-x86_64")
        .expect("windows-x86_64 target");
    assert_eq!(windows.cmd, "./opencode.exe");
    assert_eq!(windows.args, vec!["acp".to_string()]);
}

fn archive_for(
    agent: &project::agent_registry_store::RegistryBinaryAgent,
    platform: &str,
) -> String {
    agent
        .targets
        .get(platform)
        .unwrap_or_else(|| panic!("missing {platform} target"))
        .archive
        .clone()
}

fn opencode_registry_json(version: &str, archive: fn(&str, &str) -> String) -> serde_json::Value {
    json!({
        "version": "1",
        "agents": [
            {
                "id": "opencode",
                "name": "OpenCode",
                "version": version,
                "description": "The open source coding agent",
                "distribution": {
                    "binary": {
                        "darwin-aarch64": {
                            "archive": archive(version, "opencode-darwin-arm64.zip"),
                            "cmd": "./opencode",
                            "args": ["acp"],
                            "sha256": "darwin-arm64-hash"
                        },
                        "darwin-x86_64": {
                            "archive": archive(version, "opencode-darwin-x64.zip"),
                            "cmd": "./opencode",
                            "args": ["acp"],
                            "sha256": "darwin-x64-hash"
                        },
                        "linux-aarch64": {
                            "archive": archive(version, "opencode-linux-arm64.tar.gz"),
                            "cmd": "./opencode",
                            "args": ["acp"],
                            "sha256": "linux-arm64-hash"
                        },
                        "linux-x86_64": {
                            "archive": archive(version, "opencode-linux-x64.tar.gz"),
                            "cmd": "./opencode",
                            "args": ["acp"],
                            "sha256": "linux-x64-hash"
                        },
                        "windows-aarch64": {
                            "archive": archive(version, "opencode-windows-arm64.zip"),
                            "cmd": "./opencode",
                            "args": ["acp"],
                            "sha256": "windows-arm64-hash"
                        },
                        "windows-x86_64": {
                            "archive": archive(version, "opencode-windows-x64.zip"),
                            "cmd": "./opencode.exe",
                            "args": ["acp"],
                            "sha256": "windows-x64-hash"
                        }
                    }
                }
            }
        ]
    })
}

fn github_v1_archive(version: &str, file_name: &str) -> String {
    format!("https://github.com/anomalyco/opencode/releases/download/v{version}/{file_name}")
}

fn opencode_ai_archive(version: &str, file_name: &str) -> String {
    format!("https://opencode.ai/files/bin/{version}/{file_name}")
}

fn registry_cache_path() -> std::path::PathBuf {
    paths::external_agents_dir()
        .join("registry")
        .join("registry.json")
}

fn ok_json(value: serde_json::Value) -> anyhow::Result<Response<AsyncBody>> {
    Ok(Response::builder()
        .status(200)
        .body(AsyncBody::from(serde_json::to_string(&value).unwrap()))
        .unwrap())
}

fn not_found() -> anyhow::Result<Response<AsyncBody>> {
    Ok(Response::builder()
        .status(404)
        .body(AsyncBody::default())
        .unwrap())
}
