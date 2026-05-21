use anyhow::Result;
use clock::SystemClock;
use fs::Fs;
use futures::channel::mpsc;
use gpui::App;
use http_client::HttpClientWithUrl;
#[cfg(test)]
use parking_lot::Mutex;
#[cfg(test)]
use settings::SettingsStore;
#[cfg(test)]
use std::collections::HashSet;
use std::sync::Arc;
use worktree::{UpdatedEntriesSet, WorktreeId};

#[cfg(any(test, target_os = "macos"))]
use regex::Regex;

pub struct TelemetrySubscription {
    pub historical_events: Result<HistoricalEvents>,
    pub queued_events: Vec<LoggedTelemetryEvent>,
    pub live_events: mpsc::UnboundedReceiver<LoggedTelemetryEvent>,
}

pub struct HistoricalEvents {
    pub events: Vec<LoggedTelemetryEvent>,
    pub parse_error_count: usize,
}

#[derive(Clone, Debug, Default)]
pub struct LoggedTelemetryEvent;

#[derive(Clone, Debug, Default)]
pub struct AssistantEventData;

pub struct Telemetry {
    #[cfg(test)]
    state: Arc<Mutex<TelemetryState>>,
}

#[cfg(test)]
struct TelemetryState {
    #[cfg(test)]
    worktrees_with_project_type_events_sent: HashSet<WorktreeId>,
}

#[cfg(test)]
static DOTNET_PROJECT_FILES_REGEX: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"^(global\.json|Directory\.Build\.props|.*\.(csproj|fsproj|vbproj|sln))$").unwrap()
});

#[cfg(target_os = "macos")]
static MACOS_VERSION_REGEX: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r"(\s*\(Build [^)]*[0-9]\))").unwrap());

pub fn os_name() -> String {
    #[cfg(target_os = "macos")]
    {
        "macOS".to_string()
    }
    #[cfg(target_os = "linux")]
    {
        format!("Linux {}", gpui::guess_compositor())
    }
    #[cfg(target_os = "freebsd")]
    {
        format!("FreeBSD {}", gpui::guess_compositor())
    }

    #[cfg(target_os = "windows")]
    {
        "Windows".to_string()
    }
}

pub fn os_version() -> String {
    #[cfg(target_os = "macos")]
    {
        use objc2_foundation::NSProcessInfo;
        let process_info = NSProcessInfo::processInfo();
        let version_nsstring = process_info.operatingSystemVersionString();
        let version_string = version_nsstring.to_string().replace("Version ", "");
        MACOS_VERSION_REGEX
            .replace_all(&version_string, "")
            .to_string()
    }
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    {
        use std::path::Path;

        let content = if let Ok(file) = std::fs::read_to_string(&Path::new("/etc/os-release")) {
            file
        } else if let Ok(file) = std::fs::read_to_string(&Path::new("/usr/lib/os-release")) {
            file
        } else if let Ok(file) = std::fs::read_to_string(&Path::new("/var/run/os-release")) {
            file
        } else {
            log::error!(
                "Failed to load /etc/os-release, /usr/lib/os-release, or /var/run/os-release"
            );
            "".to_string()
        };
        let mut name = "unknown";
        let mut version = "unknown";

        for line in content.lines() {
            match line.split_once('=') {
                Some(("ID", val)) => name = val.trim_matches('"'),
                Some(("VERSION_ID", val)) => version = val.trim_matches('"'),
                _ => {}
            }
        }

        format!("{} {}", name, version)
    }

    #[cfg(target_os = "windows")]
    {
        let mut info = unsafe { std::mem::zeroed() };
        let status = unsafe { windows::Wdk::System::SystemServices::RtlGetVersion(&mut info) };
        if status.is_ok() {
            semver::Version::new(
                info.dwMajorVersion as _,
                info.dwMinorVersion as _,
                info.dwBuildNumber as _,
            )
            .to_string()
        } else {
            "unknown".to_string()
        }
    }
}

impl Telemetry {
    pub fn new(
        _clock: Arc<dyn SystemClock>,
        _client: Arc<HttpClientWithUrl>,
        _cx: &mut App,
    ) -> Arc<Self> {
        #[cfg(test)]
        let state = Arc::new(Mutex::new(TelemetryState {
            #[cfg(test)]
            worktrees_with_project_type_events_sent: HashSet::new(),
        }));

        let this = Arc::new(Self {
            #[cfg(test)]
            state,
        });

        this
    }

    pub async fn subscribe_with_history(
        self: &Arc<Self>,
        _fs: Arc<dyn Fs>,
    ) -> TelemetrySubscription {
        let (tx, rx) = mpsc::unbounded();
        drop(tx);

        TelemetrySubscription {
            historical_events: Ok(HistoricalEvents {
                events: Vec::new(),
                parse_error_count: 0,
            }),
            queued_events: Vec::new(),
            live_events: rx,
        }
    }

    pub fn report_assistant_event(self: &Arc<Self>, event: AssistantEventData) {
        let _ = (self, event);
    }

    pub fn log_edit_event(self: &Arc<Self>, environment: &'static str, is_via_ssh: bool) {
        let _ = (self, environment, is_via_ssh);
    }

    pub fn report_discovered_project_type_events(
        self: &Arc<Self>,
        worktree_id: WorktreeId,
        updated_entries_set: &UpdatedEntriesSet,
    ) {
        let _ = (self, worktree_id, updated_entries_set);
    }

    #[cfg(test)]
    fn detect_project_types(
        self: &Arc<Self>,
        worktree_id: WorktreeId,
        updated_entries_set: &UpdatedEntriesSet,
    ) -> Option<Vec<String>> {
        let mut state = self.state.lock();

        if state
            .worktrees_with_project_type_events_sent
            .contains(&worktree_id)
        {
            return None;
        }

        let mut project_types: HashSet<&str> = HashSet::new();

        for (path, _, _) in updated_entries_set.iter() {
            let Some(file_name) = path.file_name() else {
                continue;
            };

            let project_type = match file_name {
                "pnpm-lock.yaml" => Some("pnpm"),
                "yarn.lock" => Some("yarn"),
                "package.json" => Some("node"),
                _ if DOTNET_PROJECT_FILES_REGEX.is_match(file_name) => Some("dotnet"),
                _ => None,
            };

            if let Some(project_type) = project_type {
                project_types.insert(project_type);
            }
        }

        if !project_types.is_empty() {
            state
                .worktrees_with_project_type_events_sent
                .insert(worktree_id);
        }

        let mut project_types: Vec<_> = project_types.into_iter().map(String::from).collect();
        project_types.sort();
        Some(project_types)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clock::FakeSystemClock;
    use gpui::TestAppContext;
    use http_client::FakeHttpClient;
    use util::rel_path::RelPath;
    use worktree::{PathChange, ProjectEntryId, WorktreeId};

    #[gpui::test]
    fn test_project_discovery_does_not_double_report(cx: &mut gpui::TestAppContext) {
        init_test(cx);

        let clock = Arc::new(FakeSystemClock::new());
        let http = FakeHttpClient::with_200_response();
        let telemetry = cx.update(|cx| Telemetry::new(clock.clone(), http, cx));
        let worktree_id = 1;

        test_project_discovery_helper(telemetry.clone(), vec![], Some(vec![]), worktree_id);
        test_project_discovery_helper(
            telemetry.clone(),
            vec!["package.json"],
            Some(vec!["node"]),
            worktree_id,
        );
        test_project_discovery_helper(telemetry, vec!["package.json"], None, worktree_id);
    }

    #[gpui::test]
    fn test_pnpm_project_discovery(cx: &mut gpui::TestAppContext) {
        init_test(cx);

        let clock = Arc::new(FakeSystemClock::new());
        let http = FakeHttpClient::with_200_response();
        let telemetry = cx.update(|cx| Telemetry::new(clock.clone(), http, cx));

        test_project_discovery_helper(
            telemetry,
            vec!["package.json", "pnpm-lock.yaml"],
            Some(vec!["node", "pnpm"]),
            1,
        );
    }

    #[gpui::test]
    fn test_yarn_project_discovery(cx: &mut gpui::TestAppContext) {
        init_test(cx);

        let clock = Arc::new(FakeSystemClock::new());
        let http = FakeHttpClient::with_200_response();
        let telemetry = cx.update(|cx| Telemetry::new(clock.clone(), http, cx));

        test_project_discovery_helper(
            telemetry,
            vec!["package.json", "yarn.lock"],
            Some(vec!["node", "yarn"]),
            1,
        );
    }

    #[gpui::test]
    fn test_dotnet_project_discovery(cx: &mut gpui::TestAppContext) {
        init_test(cx);

        let clock = Arc::new(FakeSystemClock::new());
        let http = FakeHttpClient::with_200_response();
        let telemetry = cx.update(|cx| Telemetry::new(clock.clone(), http, cx));

        test_project_discovery_helper(
            telemetry.clone(),
            vec!["global.json"],
            Some(vec!["dotnet"]),
            1,
        );
        test_project_discovery_helper(
            telemetry.clone(),
            vec!["Directory.Build.props"],
            Some(vec!["dotnet"]),
            2,
        );
        test_project_discovery_helper(
            telemetry.clone(),
            vec!["file.csproj"],
            Some(vec!["dotnet"]),
            3,
        );
        test_project_discovery_helper(
            telemetry.clone(),
            vec!["file.fsproj"],
            Some(vec!["dotnet"]),
            4,
        );
        test_project_discovery_helper(
            telemetry.clone(),
            vec!["file.vbproj"],
            Some(vec!["dotnet"]),
            5,
        );
        test_project_discovery_helper(telemetry.clone(), vec!["file.sln"], Some(vec!["dotnet"]), 6);
        test_project_discovery_helper(
            telemetry,
            vec!["global.json", "Directory.Build.props"],
            Some(vec!["dotnet"]),
            7,
        );
    }

    fn init_test(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let settings_store = SettingsStore::test(cx);
            cx.set_global(settings_store);
        });
    }

    fn test_project_discovery_helper(
        telemetry: Arc<Telemetry>,
        file_paths: Vec<&str>,
        expected_project_types: Option<Vec<&str>>,
        worktree_id_num: usize,
    ) {
        let worktree_id = WorktreeId::from_usize(worktree_id_num);
        let entries: Vec<_> = file_paths
            .into_iter()
            .enumerate()
            .filter_map(|(i, path)| {
                Some((
                    Arc::from(RelPath::unix(path).ok()?),
                    ProjectEntryId::from_proto(i as u64 + 1),
                    PathChange::Added,
                ))
            })
            .collect();
        let updated_entries: UpdatedEntriesSet = Arc::from(entries.as_slice());

        let detected_project_types = telemetry.detect_project_types(worktree_id, &updated_entries);
        let expected_project_types =
            expected_project_types.map(|types| types.iter().map(|&t| t.to_string()).collect());

        assert_eq!(detected_project_types, expected_project_types);
    }
}
