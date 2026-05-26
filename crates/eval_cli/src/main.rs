//! Headless CLI binary for running Zed's agent in evaluation/benchmark environments.
//!
//! Designed to work inside containerized environments (like Harbor/termbench) where:
//! - The repository is already checked out at the working directory
//! - The model API key is provided via environment variables
//! - Results are written to an output directory (default: `/logs/agent/`)
//!
//! ## Usage
//!
//! ```text
//! eval-cli --workdir /testbed --model anthropic/claude-sonnet-4-6-latest \
//!          --instruction "Fix the bug described in..." --timeout 600
//! ```
//!
//! ## Output
//!
//! Writes to `--output-dir` (default `/logs/agent/`):
//!   - `result.json`  — structured result with status, timing, and token usage
//!   - `thread.md`    — full conversation as markdown
//!   - `thread.json`  — raw thread state as JSON
//!
//! ## Exit codes
//!
//! | Code | Meaning |
//! |------|---------|
//! | 0    | Agent finished |
//! | 1    | Error (model/auth/runtime failure) |
//! | 2    | Timeout |
//! | 3    | Interrupted (SIGTERM/SIGINT) |

mod headless;

use std::path::PathBuf;
use std::process;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use clap::Parser;
use feature_flags::FeatureFlagAppExt as _;

use gpui::{AsyncApp, UpdateGlobal};
use language_model::{
    ANTHROPIC_PROVIDER_ID, LanguageModel, LanguageModelId, LanguageModelProviderId,
    LanguageModelRegistry, SelectedModel,
};
use settings::SettingsStore;

use crate::headless::AgentCliAppState;

#[derive(Parser, Debug)]
#[command(
    name = "eval-cli",
    about = "Run Zed's agent headlessly in evaluation/benchmark environments"
)]
struct Args {
    /// Output current environment variables as JSON to stdout.
    /// Used internally by Zed's shell environment capture.
    #[arg(long, hide = true)]
    printenv: bool,

    /// Path to the repository working directory. Defaults to the current directory.
    #[arg(long, default_value = ".")]
    workdir: PathBuf,

    /// Instruction/prompt text. If omitted, read from --instruction-file or stdin.
    #[arg(long, allow_hyphen_values = true)]
    instruction: Option<String>,

    /// Language model to use, in `provider/model` format.
    #[arg(long, default_value = "anthropic/claude-sonnet-4-6-latest")]
    model: String,

    /// Maximum wall-clock time in seconds for the agent run.
    #[arg(long)]
    timeout: Option<u64>,

    /// Directory for output artifacts (result.json, thread.md, thread.json).
    #[arg(long, default_value = ".")]
    output_dir: PathBuf,

    /// Disable staff mode (staff mode is enabled by default).
    #[arg(long)]
    no_staff: bool,

    /// Reasoning effort level for models that support thinking (low, medium, high).
    /// Defaults to "high" for thinking-capable models.
    #[arg(long)]
    reasoning_effort: Option<String>,

    /// Enable or disable extended thinking. Defaults to model auto-detection if omitted.
    #[arg(long)]
    thinking: Option<bool>,
}

#[derive(serde::Serialize)]
struct EvalResult {
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    duration_secs: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    timeout_secs: Option<u64>,
    model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    input_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    output_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cache_creation_input_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cache_read_input_tokens: Option<u64>,
}

const EXIT_OK: i32 = 0;
const EXIT_ERROR: i32 = 1;
const MODEL_DISCOVERY_TIMEOUT: Duration = Duration::from_secs(30);
const MODEL_DISCOVERY_POLL_INTERVAL: Duration = Duration::from_millis(100);

static TERMINATED: AtomicBool = AtomicBool::new(false);

fn main() {
    let args = Args::parse();

    if args.printenv {
        util::shell_env::print_env();
        return;
    }

    env_logger::init();

    ctrlc::set_handler(|| {
        TERMINATED.store(true, Ordering::SeqCst);
    })
    .expect("failed to set signal handler");

    let instruction = read_instruction(&args).unwrap_or_else(|e| {
        eprintln!("Error reading instruction: {e}");
        process::exit(EXIT_ERROR);
    });

    let workdir = args.workdir.canonicalize().unwrap_or_else(|e| {
        eprintln!("Invalid --workdir {:?}: {e}", args.workdir);
        process::exit(EXIT_ERROR);
    });

    let output_dir = args.output_dir.clone();
    if let Err(e) = std::fs::create_dir_all(&output_dir) {
        eprintln!("Error creating output dir {}: {e}", output_dir.display());
        process::exit(EXIT_ERROR);
    }

    let http_client = Arc::new(reqwest_client::ReqwestClient::new());
    let app = gpui_platform::headless().with_http_client(http_client);

    app.run(move |cx| {
        let app_state = headless::init(cx);
        cx.set_staff(!args.no_staff);

        let auth_tasks = LanguageModelRegistry::global(cx).update(cx, |registry, cx| {
            registry
                .providers()
                .iter()
                .map(|p| p.authenticate(cx))
                .collect::<Vec<_>>()
        });

        let model_name = args.model.clone();
        let timeout = args.timeout;
        let thinking_override = args.thinking;
        let reasoning_effort = args.reasoning_effort.clone();

        cx.spawn(async move |cx| {
            futures::future::join_all(auth_tasks).await;

            let start = Instant::now();

            let (outcome, token_usage) = run_agent(
                &app_state,
                &workdir,
                &instruction,
                &model_name,
                timeout,
                thinking_override,
                reasoning_effort.as_deref(),
                Some(&output_dir),
                cx,
            )
            .await;

            let duration = start.elapsed();

            let (status, error, exit_code) = match &outcome {
                Ok(()) => ("completed".to_string(), None, EXIT_OK),
                Err(e) => {
                    eprintln!("Error: {e:#}");
                    ("error".to_string(), Some(format!("{e:#}")), EXIT_ERROR)
                }
            };

            let result = EvalResult {
                status,
                error,
                duration_secs: duration.as_secs_f64(),
                timeout_secs: timeout,
                model: model_name.clone(),
                input_tokens: token_usage.as_ref().map(|u| u.input_tokens),
                output_tokens: token_usage.as_ref().map(|u| u.output_tokens),
                cache_creation_input_tokens: token_usage
                    .as_ref()
                    .filter(|u| u.cache_creation_input_tokens > 0)
                    .map(|u| u.cache_creation_input_tokens),
                cache_read_input_tokens: token_usage
                    .as_ref()
                    .filter(|u| u.cache_read_input_tokens > 0)
                    .map(|u| u.cache_read_input_tokens),
            };

            match serde_json::to_string_pretty(&result) {
                Ok(json) => {
                    if let Err(e) = std::fs::write(output_dir.join("result.json"), &json) {
                        eprintln!("Error writing result.json: {e:#}");
                    }
                    eprintln!("[eval-cli] result: {json}");
                }
                Err(e) => eprintln!("Error serializing result: {e:#}"),
            }

            cx.update(|cx| cx.quit());
            process::exit(exit_code);
        })
        .detach();
    });
}

fn read_instruction(args: &Args) -> Result<String> {
    let text = if let Some(text) = &args.instruction {
        text.clone()
    } else {
        use std::io::Read;
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .context("reading instruction from stdin")?;
        buf
    };
    anyhow::ensure!(!text.trim().is_empty(), "instruction is empty");
    Ok(text)
}

async fn wait_for_model(selected: &SelectedModel, cx: &mut AsyncApp) -> Result<()> {
    let started_at = Instant::now();

    loop {
        let found = cx.update(|cx| find_available_model(selected, cx).is_some());
        if found {
            return Ok(());
        }

        cx.update(|cx| ensure_provider_authenticated(selected, cx))?;

        let selected_provider_has_models = cx.update(|cx| {
            LanguageModelRegistry::global(cx)
                .read(cx)
                .available_models(cx)
                .any(|model| model.provider_id() == selected.provider)
        });
        let should_wait_for_discovery =
            selected.provider == ANTHROPIC_PROVIDER_ID || !selected_provider_has_models;

        if !should_wait_for_discovery || started_at.elapsed() >= MODEL_DISCOVERY_TIMEOUT {
            return Err(cx.update(|cx| model_not_found_error(&selected_model_name(selected), cx)));
        }

        cx.background_executor()
            .timer(MODEL_DISCOVERY_POLL_INTERVAL)
            .await;
    }
}

fn ensure_provider_authenticated(selected: &SelectedModel, cx: &gpui::App) -> Result<()> {
    let registry = LanguageModelRegistry::global(cx);
    let provider = registry
        .read(cx)
        .provider(&selected.provider)
        .ok_or_else(|| anyhow::anyhow!("Provider {} not found", selected.provider.0))?;

    anyhow::ensure!(
        provider.is_authenticated(cx),
        "Provider {} is not authenticated",
        selected.provider.0
    );

    Ok(())
}

fn find_available_model(
    selected: &SelectedModel,
    cx: &gpui::App,
) -> Option<Arc<dyn LanguageModel>> {
    let registry = LanguageModelRegistry::global(cx);
    let models = registry.read(cx).available_models(cx).collect::<Vec<_>>();

    if let Some(model) = models
        .iter()
        .find(|model| model.provider_id() == selected.provider && model.id() == selected.model)
    {
        return Some(model.clone());
    }

    models
        .into_iter()
        .filter(|model| {
            model.provider_id() == selected.provider
                && model_id_matches_selected(&model.provider_id(), &model.id(), &selected.model)
        })
        .max_by(|left, right| left.id().0.to_string().cmp(&right.id().0.to_string()))
}

fn model_id_matches_selected(
    provider_id: &LanguageModelProviderId,
    available: &LanguageModelId,
    selected: &LanguageModelId,
) -> bool {
    if available == selected {
        return true;
    }

    if provider_id != &ANTHROPIC_PROVIDER_ID {
        return false;
    }

    anthropic_model_ids_match(available.0.as_ref(), selected.0.as_ref())
}

fn anthropic_model_ids_match(available: &str, selected: &str) -> bool {
    let available = anthropic_model_alias_base(available);
    let selected = anthropic_model_alias_base(selected);

    available == selected || anthropic_dated_model_id_matches_base(available, selected)
}

fn anthropic_model_alias_base(mut model_id: &str) -> &str {
    if let Some(stripped) = model_id.strip_suffix("-latest") {
        model_id = stripped;
    }
    if let Some(stripped) = model_id.strip_suffix("-thinking") {
        model_id = stripped;
    }
    if let Some(stripped) = model_id.strip_suffix("-1m-context") {
        model_id = stripped;
    }
    model_id
}

fn anthropic_dated_model_id_matches_base(available: &str, selected: &str) -> bool {
    let Some(suffix) = available.strip_prefix(selected) else {
        return false;
    };
    let Some(date) = suffix.strip_prefix('-') else {
        return false;
    };

    date.len() == 8 && date.chars().all(|character| character.is_ascii_digit())
}

fn selected_model_name(selected: &SelectedModel) -> String {
    format!("{}/{}", selected.provider.0, selected.model.0)
}

fn model_not_found_error(model_name: &str, cx: &gpui::App) -> anyhow::Error {
    let available = LanguageModelRegistry::global(cx)
        .read(cx)
        .available_models(cx)
        .map(|model| format!("{}/{}", model.provider_id().0, model.id().0))
        .collect::<Vec<_>>();
    let available = if available.is_empty() {
        "(none)".to_string()
    } else {
        available.join(", ")
    };

    anyhow::anyhow!("Model {model_name} not found. Available: {available}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anthropic_latest_alias_matches_listed_base_model() {
        assert!(model_id_matches_selected(
            &ANTHROPIC_PROVIDER_ID,
            &LanguageModelId("claude-sonnet-4-6".into()),
            &LanguageModelId("claude-sonnet-4-6-latest".into()),
        ));
    }

    #[test]
    fn anthropic_thinking_alias_matches_listed_base_model() {
        assert!(model_id_matches_selected(
            &ANTHROPIC_PROVIDER_ID,
            &LanguageModelId("claude-sonnet-4-6".into()),
            &LanguageModelId("claude-sonnet-4-6-1m-context-thinking-latest".into()),
        ));
    }

    #[test]
    fn anthropic_latest_alias_matches_listed_dated_model() {
        assert!(model_id_matches_selected(
            &ANTHROPIC_PROVIDER_ID,
            &LanguageModelId("claude-sonnet-4-6-20260518".into()),
            &LanguageModelId("claude-sonnet-4-6-latest".into()),
        ));
    }

    #[test]
    fn non_anthropic_models_require_exact_ids() {
        assert!(!model_id_matches_selected(
            &LanguageModelProviderId("other".into()),
            &LanguageModelId("claude-sonnet-4-6".into()),
            &LanguageModelId("claude-sonnet-4-6-latest".into()),
        ));
    }
}

async fn run_agent(
    app_state: &Arc<AgentCliAppState>,
    workdir: &std::path::Path,
    instruction: &str,
    model_name: &str,
    timeout: Option<u64>,
    thinking_override: Option<bool>,
    reasoning_effort: Option<&str>,
    output_dir: Option<&std::path::Path>,
    cx: &mut AsyncApp,
) -> (Result<()>, Option<language_model::TokenUsage>) {
    let selected = match SelectedModel::from_str(model_name).map_err(|e| anyhow::anyhow!("{e}")) {
        Ok(selected) => selected,
        Err(e) => return (Err(e), None),
    };

    if let Err(e) = wait_for_model(&selected, cx).await {
        return (Err(e), None);
    }

    let setup_result: Result<()> = cx.update(|cx| {
        let registry = LanguageModelRegistry::global(cx);
        let model = find_available_model(&selected, cx)
            .ok_or_else(|| model_not_found_error(model_name, cx))?;
        let provider = registry
            .read(cx)
            .provider(&model.provider_id())
            .context("Provider not found")?;

        let supports_thinking = model.supports_thinking();
        let model_id = model.id().0.to_string();

        registry.update(cx, |registry, cx| {
            registry.set_default_model(
                Some(language_model::ConfiguredModel { provider, model }),
                cx,
            );
        });

        let enable_thinking = thinking_override.unwrap_or(supports_thinking);
        let effort = if enable_thinking {
            match reasoning_effort {
                Some(level) => format!("\"{level}\""),
                None => "\"high\"".to_string(),
            }
        } else {
            "null".to_string()
        };
        let provider_id = selected.provider.0.to_string();
        SettingsStore::update_global(cx, |store, cx| {
            let settings = format!(
                r#"{{
                    "agent": {{
                        "tool_permissions": {{"default": "allow"}},
                        "default_model": {{
                            "provider": "{provider_id}",
                            "model": "{model_id}",
                            "enable_thinking": {enable_thinking},
                            "effort": {effort}
                        }}
                    }},
                    "autosave": "off",
                    "format_on_save": "off"
                }}"
                "#
            );
            store.set_user_settings(&settings, cx).result()
        })
        .context("updating agent settings")?;

        anyhow::Ok(())
    });

    if let Err(e) = setup_result {
        return (Err(e), None);
    }

    let _ = (app_state, workdir, instruction, timeout, output_dir, cx);

    (
        Err(anyhow::anyhow!(
            "eval-cli is unavailable because the native agent implementation has been removed"
        )),
        None,
    )
}
