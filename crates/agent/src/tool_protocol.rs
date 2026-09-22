use crate::{TerminalTool, ToolPermissionDecision, UserMessage, decide_permission_from_settings};
use agent_client_protocol::schema::v1 as acp;
use anyhow::{Result, anyhow};
use fs::Fs;
use futures::{
    FutureExt, StreamExt,
    channel::{mpsc, oneshot},
};
use gpui::{App, AsyncApp, Entity, SharedString, Task};
use language_model::{
    LanguageModelProviderId, LanguageModelToolResultContent, LanguageModelToolSchemaFormat,
    LanguageModelToolUseId,
};
use schemars::{JsonSchema, Schema};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use settings::{Settings, SettingsStore, ToolPermissionMode, update_settings_file};
use std::{marker::PhantomData, sync::Arc};

#[derive(Debug)]
pub enum ThreadEvent {
    UserMessage(UserMessage),
    AgentText(String),
    AgentThinking(String),
    ToolCall(acp::ToolCall),
    ToolCallUpdate(acp_thread::ToolCallUpdate),
    Plan(acp::Plan),
    ToolCallAuthorization(ToolCallAuthorization),
    ToolCallAuthorizationResolved {
        tool_call_id: acp::ToolCallId,
        outcome: acp_thread::SelectedPermissionOutcome,
    },
    SubagentSpawned(acp::SessionId),
    Retry(acp_thread::RetryStatus),
    Stop(acp::StopReason),
}

#[derive(Debug, Clone)]
pub struct ToolPermissionContext {
    pub tool_name: String,
    pub input_values: Vec<String>,
    pub scope: ToolPermissionScope,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolPermissionScope {
    ToolInput,
    SymlinkTarget,
}

impl ToolPermissionContext {
    pub fn new(tool_name: impl Into<String>, input_values: Vec<String>) -> Self {
        Self {
            tool_name: tool_name.into(),
            input_values,
            scope: ToolPermissionScope::ToolInput,
        }
    }

    pub fn symlink_target(tool_name: impl Into<String>, target_paths: Vec<String>) -> Self {
        Self {
            tool_name: tool_name.into(),
            input_values: target_paths,
            scope: ToolPermissionScope::SymlinkTarget,
        }
    }

    /// Builds the permission options for this tool context.
    ///
    /// This is the canonical source for permission option generation.
    /// Tests should use this function rather than manually constructing options.
    ///
    /// # Shell Compatibility for Terminal Tool
    ///
    /// For the terminal tool, "Always allow" options are only shown when the user's
    /// shell supports POSIX-like command chaining syntax (`&&`, `||`, `;`, `|`).
    ///
    /// **Why this matters:** When a user sets up an "always allow" pattern like `^cargo`,
    /// we need to parse the command to extract all sub-commands and verify that EVERY
    /// sub-command matches the pattern. Otherwise, an attacker could craft a command like
    /// `cargo build && rm -rf /` that would bypass the security check.
    ///
    /// **Supported shells:** Posix (sh, bash, dash, zsh), Fish 3.0+, PowerShell 7+/Pwsh,
    /// Cmd, Xonsh, Csh, Tcsh
    ///
    /// **Unsupported shells:** Nushell (uses `and`/`or` keywords), Elvish (uses `and`/`or`
    /// keywords), Rc (Plan 9 shell - no `&&`/`||` operators)
    ///
    /// For unsupported shells, we hide the "Always allow" UI options entirely, and if
    /// the user has `always_allow` rules configured in settings, `ToolPermissionDecision::from_input`
    /// will return a `Deny` with an explanatory error message.
    pub fn build_permission_options(&self) -> acp_thread::PermissionOptions {
        use crate::pattern_extraction::*;
        use util::shell::ShellKind;

        let tool_name = &self.tool_name;
        let input_values = &self.input_values;
        if self.scope == ToolPermissionScope::SymlinkTarget {
            return acp_thread::PermissionOptions::Flat(vec![
                acp::PermissionOption::new(
                    acp::PermissionOptionId::new("allow"),
                    "Yes",
                    acp::PermissionOptionKind::AllowOnce,
                ),
                acp::PermissionOption::new(
                    acp::PermissionOptionId::new("deny"),
                    "No",
                    acp::PermissionOptionKind::RejectOnce,
                ),
            ]);
        }

        // Check if the user's shell supports POSIX-like command chaining.
        // See the doc comment above for the full explanation of why this is needed.
        let shell_supports_always_allow = if tool_name == TerminalTool::NAME {
            ShellKind::system().supports_posix_chaining()
        } else {
            true
        };

        // For terminal commands with multiple pipeline commands, use DropdownWithPatterns
        // to let users individually select which command patterns to always allow.
        if tool_name == TerminalTool::NAME && shell_supports_always_allow {
            if let Some(input) = input_values.first() {
                let all_patterns = extract_all_terminal_patterns(input);
                if all_patterns.len() > 1 {
                    let mut choices = Vec::new();
                    choices.push(acp_thread::PermissionOptionChoice {
                        allow: acp::PermissionOption::new(
                            acp::PermissionOptionId::new(format!("always_allow:{}", tool_name)),
                            format!("Always for {}", tool_name.replace('_', " ")),
                            acp::PermissionOptionKind::AllowAlways,
                        ),
                        deny: acp::PermissionOption::new(
                            acp::PermissionOptionId::new(format!("always_deny:{}", tool_name)),
                            format!("Always for {}", tool_name.replace('_', " ")),
                            acp::PermissionOptionKind::RejectAlways,
                        ),
                        sub_patterns: vec![],
                    });
                    choices.push(acp_thread::PermissionOptionChoice {
                        allow: acp::PermissionOption::new(
                            acp::PermissionOptionId::new("allow"),
                            "Only this time",
                            acp::PermissionOptionKind::AllowOnce,
                        ),
                        deny: acp::PermissionOption::new(
                            acp::PermissionOptionId::new("deny"),
                            "Only this time",
                            acp::PermissionOptionKind::RejectOnce,
                        ),
                        sub_patterns: vec![],
                    });
                    return acp_thread::PermissionOptions::DropdownWithPatterns {
                        choices,
                        patterns: all_patterns,
                        tool_name: tool_name.clone(),
                    };
                }
            }
        }

        let extract_for_value = |value: &str| -> (Option<String>, Option<String>) {
            if tool_name == TerminalTool::NAME {
                (
                    extract_terminal_pattern(value),
                    extract_terminal_pattern_display(value),
                )
            } else if tool_name == "copy_path"
                || tool_name == "move_path"
                || tool_name == "edit_file"
                || tool_name == "delete_path"
                || tool_name == "create_directory"
                || tool_name == "save_file"
            {
                (
                    extract_path_pattern(value),
                    extract_path_pattern_display(value),
                )
            } else if tool_name == "fetch" {
                (
                    extract_url_pattern(value),
                    extract_url_pattern_display(value),
                )
            } else {
                (None, None)
            }
        };

        // Extract patterns from all input values. Only offer a pattern-specific
        // "always allow/deny" button when every value produces the same pattern.
        let (pattern, pattern_display) = match input_values.as_slice() {
            [single] => extract_for_value(single),
            _ => {
                let mut iter = input_values.iter().map(|v| extract_for_value(v));
                match iter.next() {
                    Some(first) => {
                        if iter.all(|pair| pair.0 == first.0) {
                            first
                        } else {
                            (None, None)
                        }
                    }
                    None => (None, None),
                }
            }
        };

        let mut choices = Vec::new();

        let mut push_choice =
            |label: String, allow_id, deny_id, allow_kind, deny_kind, sub_patterns: Vec<String>| {
                choices.push(acp_thread::PermissionOptionChoice {
                    allow: acp::PermissionOption::new(
                        acp::PermissionOptionId::new(allow_id),
                        label.clone(),
                        allow_kind,
                    ),
                    deny: acp::PermissionOption::new(
                        acp::PermissionOptionId::new(deny_id),
                        label,
                        deny_kind,
                    ),
                    sub_patterns,
                });
            };

        if shell_supports_always_allow {
            push_choice(
                format!("Always for {}", tool_name.replace('_', " ")),
                format!("always_allow:{}", tool_name),
                format!("always_deny:{}", tool_name),
                acp::PermissionOptionKind::AllowAlways,
                acp::PermissionOptionKind::RejectAlways,
                vec![],
            );

            if let (Some(pattern), Some(display)) = (pattern, pattern_display) {
                let button_text = if tool_name == TerminalTool::NAME {
                    format!("Always for `{}` commands", display)
                } else {
                    format!("Always for `{}`", display)
                };
                push_choice(
                    button_text,
                    format!("always_allow:{}", tool_name),
                    format!("always_deny:{}", tool_name),
                    acp::PermissionOptionKind::AllowAlways,
                    acp::PermissionOptionKind::RejectAlways,
                    vec![pattern],
                );
            }
        }

        push_choice(
            "Only this time".to_owned(),
            "allow".to_owned(),
            "deny".to_owned(),
            acp::PermissionOptionKind::AllowOnce,
            acp::PermissionOptionKind::RejectOnce,
            vec![],
        );

        acp_thread::PermissionOptions::Dropdown(choices)
    }
}

#[derive(Debug)]
pub struct ToolCallAuthorization {
    pub tool_call: acp::ToolCallUpdate,
    pub options: acp_thread::PermissionOptions,
    pub response: oneshot::Sender<acp_thread::SelectedPermissionOutcome>,
    pub context: Option<ToolPermissionContext>,
}

fn auto_resolve_permission_outcome(
    options: &acp_thread::PermissionOptions,
    is_allow: bool,
) -> Result<acp_thread::SelectedPermissionOutcome> {
    let kind = if is_allow {
        acp::PermissionOptionKind::AllowOnce
    } else {
        acp::PermissionOptionKind::RejectOnce
    };
    let option = options
        .first_option_of_kind(kind)
        .ok_or_else(|| anyhow!("permission prompt has no auto-resolution option"))?;

    Ok(acp_thread::SelectedPermissionOutcome::new(
        option.option_id.clone(),
        option.kind,
    ))
}

/// A channel-based wrapper that delivers tool input to a running tool.
///
/// For non-streaming tools, created via `ToolInput::ready()` so `.recv()` resolves immediately.
/// For streaming tools, partial JSON snapshots arrive via `.recv_partial()` as the LLM streams
/// them, followed by the final complete input available through `.recv()`.
pub struct ToolInput<T> {
    rx: mpsc::UnboundedReceiver<ToolInputPayload<serde_json::Value>>,
    _phantom: PhantomData<T>,
}

impl<T: DeserializeOwned> ToolInput<T> {
    #[cfg(any(test, feature = "test-support"))]
    pub fn resolved(input: impl Serialize) -> Self {
        let value = serde_json::to_value(input).expect("failed to serialize tool input");
        Self::ready(value)
    }

    pub fn ready(value: serde_json::Value) -> Self {
        let (tx, rx) = mpsc::unbounded();
        tx.unbounded_send(ToolInputPayload::Full(value)).ok();
        Self {
            rx,
            _phantom: PhantomData,
        }
    }

    pub fn invalid_json(error_message: String) -> Self {
        let (tx, rx) = mpsc::unbounded();
        tx.unbounded_send(ToolInputPayload::InvalidJson { error_message })
            .ok();
        Self {
            rx,
            _phantom: PhantomData,
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn test() -> (ToolInputSender, Self) {
        let (sender, input) = ToolInputSender::channel();
        (sender, input.cast())
    }

    /// Wait for the final deserialized input, ignoring all partial updates.
    /// Non-streaming tools can use this to wait until the whole input is available.
    pub async fn recv(mut self) -> Result<T> {
        while let Ok(value) = self.next().await {
            match value {
                ToolInputPayload::Full(value) => return Ok(value),
                ToolInputPayload::Partial(_) => {}
                ToolInputPayload::InvalidJson { error_message } => {
                    return Err(anyhow!(error_message));
                }
            }
        }
        Err(anyhow!("tool input was not fully received"))
    }

    pub async fn next(&mut self) -> Result<ToolInputPayload<T>> {
        let value = self
            .rx
            .next()
            .await
            .ok_or_else(|| anyhow!("tool input was not fully received"))?;

        Ok(match value {
            ToolInputPayload::Partial(payload) => ToolInputPayload::Partial(payload),
            ToolInputPayload::Full(payload) => {
                ToolInputPayload::Full(serde_json::from_value(payload)?)
            }
            ToolInputPayload::InvalidJson { error_message } => {
                ToolInputPayload::InvalidJson { error_message }
            }
        })
    }

    fn cast<U: DeserializeOwned>(self) -> ToolInput<U> {
        ToolInput {
            rx: self.rx,
            _phantom: PhantomData,
        }
    }
}

pub enum ToolInputPayload<T> {
    Partial(serde_json::Value),
    Full(T),
    InvalidJson { error_message: String },
}

pub struct ToolInputSender {
    tx: mpsc::UnboundedSender<ToolInputPayload<serde_json::Value>>,
}

impl ToolInputSender {
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn channel() -> (Self, ToolInput<serde_json::Value>) {
        let (tx, rx) = mpsc::unbounded();
        let sender = Self { tx };
        let input = ToolInput {
            rx,
            _phantom: PhantomData,
        };
        (sender, input)
    }

    pub fn send_partial(&mut self, payload: serde_json::Value) {
        self.tx
            .unbounded_send(ToolInputPayload::Partial(payload))
            .ok();
    }

    pub fn send_full(&mut self, payload: serde_json::Value) {
        self.tx.unbounded_send(ToolInputPayload::Full(payload)).ok();
    }

    pub fn send_invalid_json(&mut self, error_message: String) {
        self.tx
            .unbounded_send(ToolInputPayload::InvalidJson { error_message })
            .ok();
    }
}

pub trait AgentTool
where
    Self: 'static + Sized,
{
    type Input: for<'de> Deserialize<'de> + Serialize + JsonSchema;
    type Output: for<'de> Deserialize<'de> + Serialize + Into<LanguageModelToolResultContent>;

    const NAME: &'static str;

    fn description() -> SharedString {
        let schema = schemars::schema_for!(Self::Input);
        SharedString::new(
            schema
                .get("description")
                .and_then(|description| description.as_str())
                .unwrap_or_default(),
        )
    }

    fn kind() -> acp::ToolKind;

    /// The initial tool title to display. Can be updated during the tool run.
    fn initial_title(
        &self,
        input: Result<Self::Input, serde_json::Value>,
        cx: &mut App,
    ) -> SharedString;

    /// Returns the JSON schema that describes the tool's input.
    fn input_schema(format: LanguageModelToolSchemaFormat) -> Schema {
        language_model::tool_schema::root_schema_for::<Self::Input>(format)
    }

    /// Returns whether the tool supports streaming of tool use parameters.
    fn supports_input_streaming() -> bool {
        false
    }

    /// Some tools rely on a provider for the underlying billing or other reasons.
    /// Allow the tool to check if they are compatible, or should be filtered out.
    fn supports_provider(_provider: &LanguageModelProviderId) -> bool {
        true
    }

    /// Runs the tool with the provided input.
    ///
    /// Returns `Result<Self::Output, Self::Output>` rather than `Result<Self::Output, anyhow::Error>`
    /// because tool errors are sent back to the model as tool results. This means error output must
    /// be structured and readable by the agent — not an arbitrary `anyhow::Error`. Returning the
    /// same `Output` type for both success and failure lets tools provide structured data while
    /// still signaling whether the invocation succeeded or failed.
    fn run(
        self: Arc<Self>,
        input: ToolInput<Self::Input>,
        event_stream: ToolCallEventStream,
        cx: &mut App,
    ) -> Task<Result<Self::Output, Self::Output>>;

    /// Emits events for a previous execution of the tool.
    fn replay(
        &self,
        _input: Self::Input,
        _output: Self::Output,
        _event_stream: ToolCallEventStream,
        _cx: &mut App,
    ) -> Result<()> {
        Ok(())
    }

    fn erase(self) -> Arc<dyn AnyAgentTool> {
        Arc::new(Erased(Arc::new(self)))
    }
}

pub struct Erased<T>(T);

pub struct AgentToolOutput {
    pub llm_output: Vec<LanguageModelToolResultContent>,
    pub raw_output: serde_json::Value,
}

impl AgentToolOutput {
    pub fn from_error(message: impl Into<String>) -> Self {
        let message = message.into();
        let llm_output = vec![LanguageModelToolResultContent::Text(Arc::from(
            message.as_str(),
        ))];
        let raw_output = serde_json::to_value(&llm_output).unwrap_or_else(|e| {
            log::error!("Failed to serialize tool output: {e}");
            serde_json::Value::Null
        });
        Self {
            raw_output,
            llm_output,
        }
    }
}

impl From<anyhow::Error> for AgentToolOutput {
    fn from(error: anyhow::Error) -> Self {
        let llm_output = vec![error.into()];
        let raw_output = serde_json::to_value(&llm_output).unwrap_or_else(|e| {
            log::error!("Failed to serialize tool output: {e}");
            serde_json::Value::Null
        });
        Self {
            raw_output,
            llm_output,
        }
    }
}

pub trait AnyAgentTool {
    fn name(&self) -> SharedString;
    fn description(&self) -> SharedString;
    fn kind(&self) -> acp::ToolKind;
    fn initial_title(&self, input: serde_json::Value, _cx: &mut App) -> SharedString;
    fn input_schema(&self, format: LanguageModelToolSchemaFormat) -> Result<serde_json::Value>;
    fn supports_input_streaming(&self) -> bool {
        false
    }
    fn supports_provider(&self, _provider: &LanguageModelProviderId) -> bool {
        true
    }
    /// See [`AgentTool::run`] for why this returns `Result<AgentToolOutput, AgentToolOutput>`.
    fn run(
        self: Arc<Self>,
        input: ToolInput<serde_json::Value>,
        event_stream: ToolCallEventStream,
        cx: &mut App,
    ) -> Task<Result<AgentToolOutput, AgentToolOutput>>;
    fn replay(
        &self,
        input: serde_json::Value,
        output: serde_json::Value,
        event_stream: ToolCallEventStream,
        cx: &mut App,
    ) -> Result<()>;
}

impl<T> AnyAgentTool for Erased<Arc<T>>
where
    T: AgentTool,
{
    fn name(&self) -> SharedString {
        T::NAME.into()
    }

    fn description(&self) -> SharedString {
        T::description()
    }

    fn kind(&self) -> acp::ToolKind {
        T::kind()
    }

    fn supports_input_streaming(&self) -> bool {
        T::supports_input_streaming()
    }

    fn initial_title(&self, input: serde_json::Value, _cx: &mut App) -> SharedString {
        let parsed_input = serde_json::from_value(input.clone()).map_err(|_| input);
        self.0.initial_title(parsed_input, _cx)
    }

    fn input_schema(&self, format: LanguageModelToolSchemaFormat) -> Result<serde_json::Value> {
        let mut json = serde_json::to_value(T::input_schema(format))?;
        language_model::tool_schema::adapt_schema_to_format(&mut json, format)?;
        Ok(json)
    }

    fn supports_provider(&self, provider: &LanguageModelProviderId) -> bool {
        T::supports_provider(provider)
    }

    fn run(
        self: Arc<Self>,
        input: ToolInput<serde_json::Value>,
        event_stream: ToolCallEventStream,
        cx: &mut App,
    ) -> Task<Result<AgentToolOutput, AgentToolOutput>> {
        let tool_input: ToolInput<T::Input> = input.cast();
        let task = self.0.clone().run(tool_input, event_stream, cx);
        cx.spawn(async move |_cx| match task.await {
            Ok(output) => {
                let raw_output = serde_json::to_value(&output).unwrap_or_else(|e| {
                    log::error!("Failed to serialize tool output: {e}");
                    serde_json::Value::Null
                });
                Ok(AgentToolOutput {
                    raw_output,
                    llm_output: vec![output.into()],
                })
            }
            Err(error_output) => {
                let raw_output = serde_json::to_value(&error_output).unwrap_or_else(|e| {
                    log::error!("Failed to serialize tool error output: {e}");
                    serde_json::Value::Null
                });
                Err(AgentToolOutput {
                    llm_output: vec![error_output.into()],
                    raw_output,
                })
            }
        })
    }

    fn replay(
        &self,
        input: serde_json::Value,
        output: serde_json::Value,
        event_stream: ToolCallEventStream,
        cx: &mut App,
    ) -> Result<()> {
        let input = serde_json::from_value(input)?;
        let output = serde_json::from_value(output)?;
        self.0.replay(input, output, event_stream, cx)
    }
}

#[derive(Clone)]
struct ThreadEventStream(mpsc::UnboundedSender<Result<ThreadEvent>>);

impl ThreadEventStream {
    fn update_tool_call_fields(
        &self,
        tool_use_id: &LanguageModelToolUseId,
        fields: acp::ToolCallUpdateFields,
        meta: Option<acp::Meta>,
    ) {
        self.0
            .unbounded_send(Ok(ThreadEvent::ToolCallUpdate(
                acp::ToolCallUpdate::new(tool_use_id.to_string(), fields)
                    .meta(meta)
                    .into(),
            )))
            .ok();
    }

    fn resolve_tool_call_authorization(
        &self,
        tool_use_id: &LanguageModelToolUseId,
        outcome: acp_thread::SelectedPermissionOutcome,
    ) {
        self.0
            .unbounded_send(Ok(ThreadEvent::ToolCallAuthorizationResolved {
                tool_call_id: acp::ToolCallId::new(tool_use_id.to_string()),
                outcome,
            }))
            .ok();
    }

    fn send_plan(&self, plan: acp::Plan) {
        self.0.unbounded_send(Ok(ThreadEvent::Plan(plan))).ok();
    }
}

#[derive(Clone)]
pub struct ToolCallEventStream {
    tool_use_id: LanguageModelToolUseId,
    stream: ThreadEventStream,
    fs: Option<Arc<dyn Fs>>,
    cancellation_rx: watch::Receiver<bool>,
}

impl ToolCallEventStream {
    #[cfg(any(test, feature = "test-support"))]
    pub fn test() -> (Self, ToolCallEventStreamReceiver) {
        let (stream, receiver, _cancellation_tx) = Self::test_with_cancellation();
        (stream, receiver)
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn test_with_cancellation() -> (Self, ToolCallEventStreamReceiver, watch::Sender<bool>) {
        let (events_tx, events_rx) = mpsc::unbounded::<Result<ThreadEvent>>();
        let (cancellation_tx, cancellation_rx) = watch::channel(false);

        let stream = ToolCallEventStream::new(
            "test_id".into(),
            ThreadEventStream(events_tx),
            None,
            cancellation_rx,
        );

        (
            stream,
            ToolCallEventStreamReceiver(events_rx),
            cancellation_tx,
        )
    }

    /// Signal cancellation for this event stream. Only available in tests.
    #[cfg(any(test, feature = "test-support"))]
    pub fn signal_cancellation_with_sender(cancellation_tx: &mut watch::Sender<bool>) {
        cancellation_tx.send(true).ok();
    }

    #[cfg(any(test, feature = "test-support"))]
    fn new(
        tool_use_id: LanguageModelToolUseId,
        stream: ThreadEventStream,
        fs: Option<Arc<dyn Fs>>,
        cancellation_rx: watch::Receiver<bool>,
    ) -> Self {
        Self {
            tool_use_id,
            stream,
            fs,
            cancellation_rx,
        }
    }

    /// Returns a future that resolves when the user cancels the tool call.
    /// Tools should select on this alongside their main work to detect user cancellation.
    pub fn cancelled_by_user(&self) -> impl std::future::Future<Output = ()> + '_ {
        let mut rx = self.cancellation_rx.clone();
        async move {
            loop {
                if *rx.borrow() {
                    return;
                }
                if rx.changed().await.is_err() {
                    // Sender dropped, will never be cancelled
                    std::future::pending::<()>().await;
                }
            }
        }
    }

    /// Returns true if the user has cancelled this tool call.
    /// This is useful for checking cancellation state after an operation completes,
    /// to determine if the completion was due to user cancellation.
    pub fn was_cancelled_by_user(&self) -> bool {
        *self.cancellation_rx.clone().borrow()
    }

    pub fn tool_use_id(&self) -> &LanguageModelToolUseId {
        &self.tool_use_id
    }

    pub fn update_fields(&self, fields: acp::ToolCallUpdateFields) {
        self.stream
            .update_tool_call_fields(&self.tool_use_id, fields, None);
    }

    pub fn update_fields_with_meta(
        &self,
        fields: acp::ToolCallUpdateFields,
        meta: Option<acp::Meta>,
    ) {
        self.stream
            .update_tool_call_fields(&self.tool_use_id, fields, meta);
    }

    pub fn resolve_authorization(&self, outcome: acp_thread::SelectedPermissionOutcome) {
        self.stream
            .resolve_tool_call_authorization(&self.tool_use_id, outcome);
    }

    pub fn update_diff(&self, diff: Entity<acp_thread::Diff>) {
        self.stream
            .0
            .unbounded_send(Ok(ThreadEvent::ToolCallUpdate(
                acp_thread::ToolCallUpdateDiff {
                    id: acp::ToolCallId::new(self.tool_use_id.to_string()),
                    diff,
                }
                .into(),
            )))
            .ok();
    }

    pub fn subagent_spawned(&self, id: acp::SessionId) {
        self.stream
            .0
            .unbounded_send(Ok(ThreadEvent::SubagentSpawned(id)))
            .ok();
    }

    pub fn update_plan(&self, plan: acp::Plan) {
        self.stream.send_plan(plan);
    }

    /// Authorize a third-party tool (e.g., MCP tool from a context server).
    ///
    /// Unlike built-in tools, third-party tools don't support pattern-based permissions.
    /// They only support `default` (allow/deny/confirm) per tool.
    ///
    /// Uses the dropdown authorization flow with two granularities:
    /// - "Always for <display_name> MCP tool" → sets `tools.<tool_id>.default = "allow"` or "deny"
    /// - "Only this time" → allow/deny once
    pub fn authorize_third_party_tool(
        &self,
        title: impl Into<String>,
        tool_id: String,
        display_name: String,
        cx: &mut App,
    ) -> Task<Result<()>> {
        let title = title.into();
        let options = acp_thread::PermissionOptions::Dropdown(vec![
            acp_thread::PermissionOptionChoice {
                allow: acp::PermissionOption::new(
                    acp::PermissionOptionId::new(format!("always_allow_mcp:{tool_id}")),
                    format!("Always for {display_name} MCP tool"),
                    acp::PermissionOptionKind::AllowAlways,
                ),
                deny: acp::PermissionOption::new(
                    acp::PermissionOptionId::new(format!("always_deny_mcp:{tool_id}")),
                    format!("Always for {display_name} MCP tool"),
                    acp::PermissionOptionKind::RejectAlways,
                ),
                sub_patterns: vec![],
            },
            acp_thread::PermissionOptionChoice {
                allow: acp::PermissionOption::new(
                    acp::PermissionOptionId::new("allow"),
                    "Only this time",
                    acp::PermissionOptionKind::AllowOnce,
                ),
                deny: acp::PermissionOption::new(
                    acp::PermissionOptionId::new("deny"),
                    "Only this time",
                    acp::PermissionOptionKind::RejectOnce,
                ),
                sub_patterns: vec![],
            },
        ]);

        // MCP tools are gated only by tool id (no per-input pattern
        // matching), so we pass a single empty input value just to satisfy
        // `decide_permission_from_settings`' signature.
        let check_settings: Box<dyn Fn(&App) -> ToolPermissionDecision> =
            Box::new(move |cx: &App| {
                let settings = agent_settings::AgentSettings::get_global(cx);
                decide_permission_from_settings(&tool_id, &[String::new()], settings)
            });

        self.run_authorization_loop(title, options, None, Some(check_settings), cx)
    }

    /// Gate a tool call on user permission, driven by the agent's
    /// tool-permission settings.
    ///
    /// Evaluates the current settings up-front: returns `Ok(())` immediately
    /// if the tool is already allowed, an error if it is denied, and
    /// otherwise prompts the user for a decision. While a prompt is pending,
    /// a subscription to `SettingsStore` watches for changes (for example,
    /// when the user clicks "Always for ..." on a sibling tool call and the
    /// new rule becomes globally visible). When settings change, the current
    /// prompt is dismissed and the decision is re-evaluated. This closes the
    /// gap where an "Always for ..." decision on one pending tool call would
    /// not propagate to other pending tool calls in the same turn or in
    /// subagent turns.
    ///
    /// For authorizations that must always prompt regardless of settings
    /// (e.g. symlink-escape confirmations, sensitive settings-file edits),
    /// use [`Self::prompt`] instead.
    pub fn authorize(
        &self,
        title: impl Into<String>,
        context: ToolPermissionContext,
        cx: &mut App,
    ) -> Task<Result<()>> {
        let title = title.into();
        let options = context.build_permission_options();

        let tool_name = context.tool_name.clone();
        let input_values = context.input_values.clone();
        let check_settings: Box<dyn Fn(&App) -> ToolPermissionDecision> =
            Box::new(move |cx: &App| {
                decide_permission_from_settings(
                    &tool_name,
                    &input_values,
                    agent_settings::AgentSettings::get_global(cx),
                )
            });

        self.run_authorization_loop(title, options, Some(context), Some(check_settings), cx)
    }

    /// Like [`Self::authorize`], but always prompts the user without
    /// consulting settings. Use this for authorizations that must be
    /// confirmed even when the user has configured `always_allow` rules —
    /// for example, symlink-escape confirmations or edits that target
    /// sensitive settings files.
    pub fn authorize_always_prompt(
        &self,
        title: impl Into<String>,
        context: ToolPermissionContext,
        cx: &mut App,
    ) -> Task<Result<()>> {
        let title = title.into();
        let options = context.build_permission_options();
        self.run_authorization_loop(title, options, Some(context), None, cx)
    }

    /// Prompts the user for authorization.
    ///
    /// When `check_settings` is `Some`, this gate is settings-driven: the
    /// settings are evaluated up-front (an Allow or Deny result resolves the
    /// task immediately without prompting), and while a prompt is pending a
    /// `SettingsStore` subscription watches for changes. A subsequent Allow
    /// or Deny dismisses the prompt UI and resolves the task without user
    /// interaction.
    ///
    /// When `check_settings` is `None`, the user is always prompted and
    /// settings changes are ignored. This suits prompts that aren't
    /// settings-driven (e.g. symlink-escape confirmations).
    fn run_authorization_loop(
        &self,
        title: String,
        options: acp_thread::PermissionOptions,
        context: Option<ToolPermissionContext>,
        check_settings: Option<Box<dyn Fn(&App) -> ToolPermissionDecision>>,
        cx: &mut App,
    ) -> Task<Result<()>> {
        // Short-circuit when current settings yield a definitive answer.
        if let Some(check) = check_settings.as_ref() {
            match check(cx) {
                ToolPermissionDecision::Allow => return Task::ready(Ok(())),
                ToolPermissionDecision::Deny(reason) => {
                    return Task::ready(Err(anyhow!(reason)));
                }
                ToolPermissionDecision::Confirm => {}
            }
        }

        let fs = self.fs.clone();
        let stream = self.stream.clone();
        let tool_use_id = self.tool_use_id.clone();
        cx.spawn(async move |cx| {
            let authorization_options = options.clone();
            let (response_tx, mut response_rx) = oneshot::channel();
            if let Err(error) = stream
                .0
                .unbounded_send(Ok(ThreadEvent::ToolCallAuthorization(
                    ToolCallAuthorization {
                        tool_call: acp::ToolCallUpdate::new(
                            tool_use_id.to_string(),
                            acp::ToolCallUpdateFields::new().title(title),
                        ),
                        options,
                        response: response_tx,
                        context,
                    },
                )))
            {
                log::error!("Failed to send tool call authorization: {error}");
                return Err(anyhow!("Failed to send tool call authorization: {error}"));
            }

            let Some(check_settings) = check_settings else {
                let outcome = response_rx
                    .await
                    .map_err(|_| anyhow!("authorization channel closed"))?;

                return Self::persist_permission_outcome(&outcome, fs, cx);
            };

            let (mut settings_tx, mut settings_rx) = watch::channel(());
            let _settings_subscription = cx.update(|cx| {
                cx.observe_global::<SettingsStore>(move |_cx| {
                    settings_tx.send(()).ok();
                })
            });

            // Race the user's response against settings changes. On each
            // settings change, re-evaluate `check_settings`: if it now
            // yields a definitive Allow or Deny, resolve the prompt
            // without user interaction. Otherwise keep waiting on the
            // same prompt.
            loop {
                let settings_changed = async {
                    if settings_rx.changed().await.is_err() {
                        std::future::pending::<()>().await;
                    }
                };
                futures::select_biased! {
                    outcome = (&mut response_rx).fuse() => {
                        let outcome = outcome
                            .map_err(|_| anyhow!("authorization channel closed"))?;
                        return Self::persist_permission_outcome(&outcome, fs.clone(), cx);
                    }
                    _ = settings_changed.fuse() => {
                        match cx.update(|cx| check_settings(cx)) {
                            ToolPermissionDecision::Allow => {
                                drop(response_rx);
                                stream.resolve_tool_call_authorization(
                                    &tool_use_id,
                                    auto_resolve_permission_outcome(&authorization_options, true)?,
                                );
                                return Ok(());
                            }
                            ToolPermissionDecision::Deny(reason) => {
                                drop(response_rx);
                                stream.resolve_tool_call_authorization(
                                    &tool_use_id,
                                    auto_resolve_permission_outcome(&authorization_options, false)?,
                                );
                                return Err(anyhow!(reason));
                            }
                            ToolPermissionDecision::Confirm => {}
                        }
                    }
                }
            }
        })
    }

    /// Interprets a `SelectedPermissionOutcome` and persists any settings changes.
    /// Returns `true` if the tool call should be allowed, `false` if denied.
    fn persist_permission_outcome(
        outcome: &acp_thread::SelectedPermissionOutcome,
        fs: Option<Arc<dyn Fs>>,
        cx: &AsyncApp,
    ) -> Result<()> {
        let option_id = outcome.option_id.0.as_ref();
        let err = || Err(anyhow!("Permission to run tool denied by user"));

        let always_permission = option_id
            .strip_prefix("always_allow:")
            .map(|tool| (tool, ToolPermissionMode::Allow))
            .or_else(|| {
                option_id
                    .strip_prefix("always_deny:")
                    .map(|tool| (tool, ToolPermissionMode::Deny))
            })
            .or_else(|| {
                option_id
                    .strip_prefix("always_allow_mcp:")
                    .map(|tool| (tool, ToolPermissionMode::Allow))
            })
            .or_else(|| {
                option_id
                    .strip_prefix("always_deny_mcp:")
                    .map(|tool| (tool, ToolPermissionMode::Deny))
            });

        if let Some((tool, mode)) = always_permission {
            let params = outcome.params.as_ref();
            Self::persist_always_permission(tool, mode, params, fs, cx);
            return if mode == ToolPermissionMode::Allow {
                Ok(())
            } else {
                err()
            };
        }

        // Handle simple "allow" / "deny" (once, no persistence)
        if option_id == "allow" || option_id == "deny" {
            debug_assert!(
                outcome.params.is_none(),
                "unexpected params for once-only permission"
            );
            return if option_id == "allow" { Ok(()) } else { err() };
        }

        debug_assert!(false, "unexpected permission option_id: {option_id}");

        err()
    }

    /// Persists an "always allow" or "always deny" permission, using sub_patterns
    /// from params when present.
    fn persist_always_permission(
        tool: &str,
        mode: ToolPermissionMode,
        params: Option<&acp_thread::SelectedPermissionParams>,
        fs: Option<Arc<dyn Fs>>,
        cx: &AsyncApp,
    ) {
        let Some(fs) = fs else {
            return;
        };

        match params {
            Some(acp_thread::SelectedPermissionParams::Terminal {
                patterns: sub_patterns,
            }) => {
                debug_assert!(
                    !sub_patterns.is_empty(),
                    "empty sub_patterns for tool {tool} — callers should pass None instead"
                );
                let tool = tool.to_owned();
                let sub_patterns = sub_patterns.clone();
                cx.update(|cx| {
                    update_settings_file(fs, cx, move |settings, _| {
                        let agent = settings.agent.get_or_insert_default();
                        for pattern in sub_patterns {
                            match mode {
                                ToolPermissionMode::Allow => {
                                    agent.add_tool_allow_pattern(&tool, pattern);
                                }
                                ToolPermissionMode::Deny => {
                                    agent.add_tool_deny_pattern(&tool, pattern);
                                }
                                // If there's no matching pattern this will
                                // default to confirm, so falling through is
                                // fine here.
                                ToolPermissionMode::Confirm => (),
                            }
                        }
                    });
                });
            }
            None => {
                let tool = tool.to_owned();
                cx.update(|cx| {
                    update_settings_file(fs, cx, move |settings, _| {
                        settings
                            .agent
                            .get_or_insert_default()
                            .set_tool_default_permission(&tool, mode);
                    });
                });
            }
        }
    }
}

#[cfg(any(test, feature = "test-support"))]
pub struct ToolCallEventStreamReceiver(mpsc::UnboundedReceiver<Result<ThreadEvent>>);

#[cfg(any(test, feature = "test-support"))]
impl ToolCallEventStreamReceiver {
    pub async fn expect_authorization(&mut self) -> ToolCallAuthorization {
        let event = self.0.next().await;
        if let Some(Ok(ThreadEvent::ToolCallAuthorization(auth))) = event {
            auth
        } else {
            panic!("Expected ToolCallAuthorization but got: {:?}", event);
        }
    }

    pub async fn expect_authorization_resolved(
        &mut self,
    ) -> (acp::ToolCallId, acp_thread::SelectedPermissionOutcome) {
        let event = self.0.next().await;
        if let Some(Ok(ThreadEvent::ToolCallAuthorizationResolved {
            tool_call_id,
            outcome,
        })) = event
        {
            (tool_call_id, outcome)
        } else {
            panic!("Expected authorization resolved but got: {:?}", event);
        }
    }

    pub async fn expect_update_fields(&mut self) -> acp::ToolCallUpdateFields {
        let event = self.0.next().await;
        if let Some(Ok(ThreadEvent::ToolCallUpdate(acp_thread::ToolCallUpdate::UpdateFields(
            update,
        )))) = event
        {
            update.fields
        } else {
            panic!("Expected update fields but got: {:?}", event);
        }
    }

    pub async fn expect_diff(&mut self) -> Entity<acp_thread::Diff> {
        let event = self.0.next().await;
        if let Some(Ok(ThreadEvent::ToolCallUpdate(acp_thread::ToolCallUpdate::UpdateDiff(
            update,
        )))) = event
        {
            update.diff
        } else {
            panic!("Expected diff but got: {:?}", event);
        }
    }

    pub async fn expect_terminal(&mut self) -> Entity<acp_thread::Terminal> {
        let event = self.0.next().await;
        if let Some(Ok(ThreadEvent::ToolCallUpdate(acp_thread::ToolCallUpdate::UpdateTerminal(
            update,
        )))) = event
        {
            update.terminal
        } else {
            panic!("Expected terminal but got: {:?}", event);
        }
    }

    pub async fn expect_plan(&mut self) -> acp::Plan {
        let event = self.0.next().await;
        if let Some(Ok(ThreadEvent::Plan(plan))) = event {
            plan
        } else {
            panic!("Expected plan but got: {:?}", event);
        }
    }
}

#[cfg(any(test, feature = "test-support"))]
impl std::ops::Deref for ToolCallEventStreamReceiver {
    type Target = mpsc::UnboundedReceiver<Result<ThreadEvent>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg(any(test, feature = "test-support"))]
impl std::ops::DerefMut for ToolCallEventStreamReceiver {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
