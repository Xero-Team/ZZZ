use super::restore_file_from_disk_tool::RestoreFileFromDiskTool;
use super::save_file_tool::SaveFileTool;
use super::tool_edit_parser::{ToolEditEvent, ToolEditParser};
use super::tool_permissions::authorize_file_edit;
use crate::{
    AgentTool, Templates, Thread, ToolCallEventStream, ToolInput, ToolInputPayload,
    edit_agent::{
        EditAgent, EditAgentOutputEvent, EditFormat,
        reindent::{Reindenter, compute_indent_delta},
        streaming_fuzzy_matcher::StreamingFuzzyMatcher,
    },
};
use acp_thread::Diff;
use action_log::ActionLog;
use agent_client_protocol::schema as acp;
use anyhow::{Context as _, Result};
use collections::HashSet;
use futures::{FutureExt as _, StreamExt as _};
use gpui::{App, AppContext, AsyncApp, Entity, Task, WeakEntity};
use indoc::formatdoc;
use language::language_settings::{self, FormatOnSave};
use language::{Buffer, LanguageRegistry, ToPoint};
use language_model::{CompletionIntent, LanguageModelToolResultContent};
use project::lsp_store::{FormatTrigger, LspFormatTarget};
use project::{AgentLocation, Project, ProjectPath};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, de::DeserializeOwned};
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;
use streaming_diff::{CharOperation, StreamingDiff};
use text::ToOffset;
use ui::SharedString;
use util::Deferred;
use util::ResultExt;
use util::rel_path::RelPath;

const DEFAULT_UI_TEXT: &str = "Editing file";

/// This is a tool for creating a new file or editing an existing file. For moving or renaming files, you should generally use the `move_path` tool instead.
///
/// Before using this tool, use the `read_file` tool to understand the file's contents and context.
/// To create a new file or overwrite an existing one with completely new contents, use the `write_file` tool instead.
///
/// `read_file` prefixes each line of its output with a line number right-aligned in a
/// 6-character field followed by a single tab, then the line's actual content. When you
/// derive `old_text` or `new_text` from that output, strip this prefix and keep only what
/// comes after the tab, preserving the original indentation (tabs and spaces) exactly.
/// Never include any part of the line number prefix in `old_text` or `new_text`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct EditFileToolInput {
    /// A one-line, user-friendly markdown description of the edit. This will be shown in the UI and also passed to another model to perform the edit.
    ///
    /// Be terse, but also descriptive in what you want to achieve with this edit. Avoid generic instructions.
    ///
    /// NEVER mention the file path in this description.
    ///
    /// <example>Fix API endpoint URLs</example>
    /// <example>Update copyright year in `page_footer`</example>
    ///
    /// Make sure to include this field before all the others in the input object so that we can display it immediately.
    pub display_description: String,

    /// The full path of the file to create or modify in the project.
    ///
    /// WARNING: When specifying which file path need changing, you MUST start each path with one of the project's root directories.
    ///
    /// The following examples assume we have two root directories in the project:
    /// - /a/b/backend
    /// - /c/d/frontend
    ///
    /// <example>
    /// `backend/src/main.rs`
    ///
    /// Notice how the file path starts with `backend`. Without that, the path would be ambiguous and the call would fail!
    /// </example>
    ///
    /// <example>
    /// `frontend/db.js`
    /// </example>
    pub path: PathBuf,
    /// The mode of operation on the file. Possible values:
    /// - 'edit': Make granular edits to an existing file.
    /// - 'create': Create a new file if it doesn't exist.
    /// - 'overwrite': Replace the entire contents of an existing file.
    /// - 'write': Alias for 'overwrite'.
    ///
    /// When a file already exists or you just created it, prefer editing it as opposed to recreating it from scratch.
    #[serde(deserialize_with = "deserialize_maybe_stringified")]
    pub mode: EditFileMode,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,

    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_maybe_stringified"
    )]
    pub edits: Option<Vec<EditOperation>>,
}

#[derive(Clone, Default, Debug, Serialize, Deserialize, JsonSchema)]
struct EditFileToolPartialInput {
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    display_description: Option<String>,
    #[serde(default, deserialize_with = "deserialize_maybe_stringified")]
    mode: Option<EditFileMode>,
    #[serde(default)]
    content: Option<String>,
    #[serde(default, deserialize_with = "deserialize_maybe_stringified")]
    edits: Option<Vec<PartialEditOperation>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
#[schemars(inline)]
pub enum EditFileMode {
    Edit,
    Create,
    #[serde(alias = "write")]
    Overwrite,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct EditOperation {
    pub old_text: String,
    pub new_text: String,
}

#[derive(Clone, Default, Debug, Serialize, Deserialize, JsonSchema)]
pub(crate) struct PartialEditOperation {
    #[serde(default)]
    pub(crate) old_text: Option<String>,
    #[serde(default)]
    pub(crate) new_text: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ValueOrJsonString<T> {
    Value(T),
    String(String),
}

fn deserialize_maybe_stringified<'de, T, D>(deserializer: D) -> Result<T, D::Error>
where
    T: DeserializeOwned,
    D: Deserializer<'de>,
{
    match ValueOrJsonString::<T>::deserialize(deserializer)? {
        ValueOrJsonString::Value(value) => Ok(value),
        ValueOrJsonString::String(string) => serde_json::from_str::<T>(&string).map_err(|error| {
            serde::de::Error::custom(format!("failed to parse stringified value: {error}"))
        }),
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EditFileToolOutput {
    Success {
        #[serde(alias = "original_path")]
        input_path: PathBuf,
        new_text: String,
        old_text: Arc<String>,
        #[serde(default)]
        diff: String,
    },
    Error {
        error: String,
    },
}

impl std::fmt::Display for EditFileToolOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EditFileToolOutput::Success {
                diff, input_path, ..
            } => {
                if diff.is_empty() {
                    write!(f, "No edits were made.")
                } else {
                    write!(
                        f,
                        "Edited {}:\n\n```diff\n{diff}\n```",
                        input_path.display()
                    )
                }
            }
            EditFileToolOutput::Error { error } => write!(f, "{error}"),
        }
    }
}

impl From<EditFileToolOutput> for LanguageModelToolResultContent {
    fn from(output: EditFileToolOutput) -> Self {
        output.to_string().into()
    }
}

pub struct EditFileTool {
    thread: WeakEntity<Thread>,
    language_registry: Arc<LanguageRegistry>,
    project: Entity<Project>,
    templates: Arc<Templates>,
}

enum EditSessionResult {
    Completed(EditSession),
    Fallback(EditFileToolInput),
    Failed {
        error: String,
        session: Option<EditSession>,
    },
}

struct EditSession {
    abs_path: PathBuf,
    input_path: PathBuf,
    buffer: Entity<Buffer>,
    old_text: Arc<String>,
    diff: Entity<Diff>,
    mode: EditFileMode,
    parser: ToolEditParser,
    pipeline: EditPipeline,
    file_changed_since_last_read: bool,
    _finalize_diff_guard: Deferred<Box<dyn FnOnce()>>,
}

struct EditPipeline {
    current_edit: Option<EditPipelineEntry>,
    content_written: bool,
}

enum EditPipelineEntry {
    ResolvingOldText {
        matcher: StreamingFuzzyMatcher,
    },
    StreamingNewText {
        streaming_diff: StreamingDiff,
        edit_cursor: usize,
        reindenter: Reindenter,
        original_snapshot: text::BufferSnapshot,
    },
}

impl EditPipeline {
    fn new() -> Self {
        Self {
            current_edit: None,
            content_written: false,
        }
    }

    fn ensure_resolving_old_text(&mut self, buffer: &Entity<Buffer>, cx: &mut AsyncApp) {
        if self.current_edit.is_none() {
            let snapshot = buffer.read_with(cx, |buffer, _cx| buffer.text_snapshot());
            self.current_edit = Some(EditPipelineEntry::ResolvingOldText {
                matcher: StreamingFuzzyMatcher::new(snapshot),
            });
        }
    }
}

impl EditFileTool {
    pub fn new(
        project: Entity<Project>,
        thread: WeakEntity<Thread>,
        language_registry: Arc<LanguageRegistry>,
        templates: Arc<Templates>,
    ) -> Self {
        Self {
            project,
            thread,
            language_registry,
            templates,
        }
    }

    fn authorize(
        &self,
        input: &EditFileToolInput,
        event_stream: &ToolCallEventStream,
        cx: &mut App,
    ) -> Task<Result<()>> {
        authorize_file_edit(
            Self::NAME,
            &input.path,
            &input.display_description,
            &self.thread,
            event_stream,
            cx,
        )
    }

    fn set_agent_location(&self, buffer: WeakEntity<Buffer>, position: text::Anchor, cx: &mut App) {
        let should_update_agent_location = self
            .thread
            .read_with(cx, |thread, _cx| !thread.is_subagent())
            .unwrap_or_default();
        if should_update_agent_location {
            self.project.update(cx, |project, cx| {
                project.set_agent_location(Some(AgentLocation { buffer, position }), cx);
            });
        }
    }

    async fn ensure_buffer_saved(&self, buffer: &Entity<Buffer>, cx: &mut AsyncApp) {
        let format_on_save_enabled = buffer.read_with(cx, |buffer, cx| {
            let settings = language_settings::LanguageSettings::for_buffer(buffer, cx);
            settings.format_on_save != FormatOnSave::Off
        });

        if format_on_save_enabled {
            self.project
                .update(cx, |project, cx| {
                    project.format(
                        HashSet::from_iter([buffer.clone()]),
                        LspFormatTarget::Buffers,
                        false,
                        FormatTrigger::Save,
                        cx,
                    )
                })
                .await
                .log_err();
        }

        self.project
            .update(cx, |project, cx| project.save_buffer(buffer.clone(), cx))
            .await
            .log_err();

        self.thread
            .update(cx, |thread, cx| {
                thread.action_log().update(cx, |log, cx| {
                    log.buffer_edited(buffer.clone(), cx);
                });
            })
            .ok();
    }

    async fn process_streaming_edits(
        &self,
        input: &mut ToolInput<EditFileToolInput>,
        event_stream: &ToolCallEventStream,
        cx: &mut AsyncApp,
    ) -> EditSessionResult {
        let mut session: Option<EditSession> = None;
        let mut last_partial: Option<EditFileToolPartialInput> = None;

        loop {
            futures::select! {
                payload = input.next().fuse() => {
                    match payload {
                        Ok(payload) => match payload {
                            ToolInputPayload::Partial(partial) => {
                                if let Ok(parsed) = serde_json::from_value::<EditFileToolPartialInput>(partial) {
                                    let path_complete = parsed.path.is_some()
                                        && parsed.path.as_ref() == last_partial.as_ref().and_then(|partial| partial.path.as_ref());

                                    last_partial = Some(parsed.clone());

                                    if session.is_none()
                                        && path_complete
                                        && let EditFileToolPartialInput {
                                            path: Some(path),
                                            display_description: Some(display_description),
                                            mode: Some(mode),
                                            ..
                                        } = &parsed
                                    {
                                        match EditSession::new(
                                            PathBuf::from(path),
                                            display_description,
                                            mode.clone(),
                                            self,
                                            event_stream,
                                            cx,
                                        ).await {
                                            Ok(created_session) => session = Some(created_session),
                                            Err(error) => {
                                                return EditSessionResult::Failed { error, session: None };
                                            }
                                        }
                                    }

                                    if let Some(current_session) = &mut session
                                        && let Err(error) = current_session.process(parsed, self, event_stream, cx)
                                    {
                                        return EditSessionResult::Failed { error, session };
                                    }
                                }
                            }
                            ToolInputPayload::Full(full_input) => {
                                if session.is_none()
                                    && full_input.content.is_none()
                                    && full_input.edits.is_none()
                                {
                                    return EditSessionResult::Fallback(full_input);
                                }
                                let mut session = if let Some(session) = session {
                                    session
                                } else {
                                    match EditSession::new(
                                        full_input.path.clone(),
                                        &full_input.display_description,
                                        full_input.mode.clone(),
                                        self,
                                        event_stream,
                                        cx,
                                    ).await {
                                        Ok(created_session) => created_session,
                                        Err(error) => {
                                            return EditSessionResult::Failed { error, session: None };
                                        }
                                    }
                                };

                                return match session.finalize(full_input, self, event_stream, cx).await {
                                    Ok(()) => EditSessionResult::Completed(session),
                                    Err(error) => EditSessionResult::Failed {
                                        error,
                                        session: Some(session),
                                    },
                                };
                            }
                            ToolInputPayload::InvalidJson { error_message } => {
                                return EditSessionResult::Failed { error: error_message, session };
                            }
                        },
                        Err(error) => {
                            return EditSessionResult::Failed {
                                error: error.to_string(),
                                session,
                            };
                        }
                    }
                }
                _ = event_stream.cancelled_by_user().fuse() => {
                    return EditSessionResult::Failed {
                        error: "Edit cancelled by user".to_string(),
                        session,
                    };
                }
            }
        }
    }
}

impl AgentTool for EditFileTool {
    type Input = EditFileToolInput;
    type Output = EditFileToolOutput;

    const NAME: &'static str = "edit_file";

    fn supports_input_streaming() -> bool {
        true
    }

    fn kind() -> acp::ToolKind {
        acp::ToolKind::Edit
    }

    fn initial_title(
        &self,
        input: Result<Self::Input, serde_json::Value>,
        cx: &mut App,
    ) -> SharedString {
        match input {
            Ok(input) => self
                .project
                .read(cx)
                .find_project_path(&input.path, cx)
                .and_then(|project_path| {
                    self.project
                        .read(cx)
                        .short_full_path_for_project_path(&project_path, cx)
                })
                .unwrap_or(input.path.to_string_lossy().into_owned())
                .into(),
            Err(raw_input) => {
                if let Some(input) =
                    serde_json::from_value::<EditFileToolPartialInput>(raw_input).ok()
                {
                    let path = input.path.unwrap_or_default();
                    let path = path.trim();
                    if !path.is_empty() {
                        return self
                            .project
                            .read(cx)
                            .find_project_path(path, cx)
                            .and_then(|project_path| {
                                self.project
                                    .read(cx)
                                    .short_full_path_for_project_path(&project_path, cx)
                            })
                            .unwrap_or_else(|| path.to_string())
                            .into();
                    }

                    let description = input.display_description.unwrap_or_default();
                    let description = description.trim();
                    if !description.is_empty() {
                        return description.to_string().into();
                    }
                }

                DEFAULT_UI_TEXT.into()
            }
        }
    }

    fn run(
        self: Arc<Self>,
        mut input: ToolInput<Self::Input>,
        event_stream: ToolCallEventStream,
        cx: &mut App,
    ) -> Task<Result<Self::Output, Self::Output>> {
        cx.spawn(async move |cx: &mut AsyncApp| {
            match self.process_streaming_edits(&mut input, &event_stream, cx).await {
                EditSessionResult::Completed(session) => {
                    self.ensure_buffer_saved(&session.buffer, cx).await;
                    let (new_text, diff) = session.compute_new_text_and_diff(cx).await;
                    Ok(EditFileToolOutput::Success {
                        old_text: session.old_text.clone(),
                        new_text,
                        input_path: session.input_path,
                        diff,
                    })
                }
                EditSessionResult::Failed {
                    error,
                    session: Some(session),
                } => {
                    self.ensure_buffer_saved(&session.buffer, cx).await;
                    let (_new_text, diff) = session.compute_new_text_and_diff(cx).await;
                    Err(EditFileToolOutput::Error {
                        error: format!("{error}\nEdited {}:\n\n```diff\n{diff}\n```", session.input_path.display()),
                    })
                }
                EditSessionResult::Fallback(input) => {
                    let project = self
                        .thread
                        .read_with(cx, |thread, _cx| thread.project().clone())
                        .map_err(|_| EditFileToolOutput::Error {
                            error: "thread was dropped".to_string(),
                        })?;

                    let result: anyhow::Result<EditFileToolOutput> = async {
                        let (project_path, abs_path, allow_thinking, update_agent_location, authorize) =
                            cx.update(|cx| {
                                let project_path = resolve_path(&input, project.clone(), cx)
                                    .map_err(|err| anyhow::anyhow!(err.to_string()))?;
                                let abs_path = project.read(cx).absolute_path(&project_path, cx);
                                if let Some(abs_path) = abs_path.clone() {
                                    event_stream.update_fields(
                                        acp::ToolCallUpdateFields::new()
                                            .locations(vec![acp::ToolCallLocation::new(abs_path)]),
                                    );
                                }
                                let allow_thinking = self
                                    .thread
                                    .read_with(cx, |thread, _cx| thread.thinking_enabled())
                                    .unwrap_or(true);

                                let update_agent_location = self.thread.read_with(cx, |thread, _cx| !thread.is_subagent()).unwrap_or_default();

                                let authorize = self.authorize(&input, &event_stream, cx);
                                Ok::<_, anyhow::Error>((project_path, abs_path, allow_thinking, update_agent_location, authorize))
                            })?;

                        authorize.await?;

                        let action_log = self.thread.read_with(cx, |thread, _cx| thread.action_log().clone())?;

                        let buffer = project
                            .update(cx, |project, cx| project.open_buffer(project_path.clone(), cx))
                            .await?;

                        if let Some(abs_path) = abs_path.as_ref() {
                            let last_read_mtime = action_log.read_with(cx, |log, _| log.file_read_time(abs_path));
                            let (current_mtime, is_dirty, has_save_tool, has_restore_tool) = self.thread.read_with(cx, |thread, cx| {
                                let current = buffer.read(cx).file().and_then(|file| file.disk_state().mtime());
                                let dirty = buffer.read(cx).is_dirty();
                                let has_save = thread.has_tool(SaveFileTool::NAME);
                                let has_restore = thread.has_tool(RestoreFileFromDiskTool::NAME);
                                (current, dirty, has_save, has_restore)
                            })?;

                            if is_dirty {
                                let message = match (has_save_tool, has_restore_tool) {
                                    (true, true) => "This file has unsaved changes. Ask the user whether they want to keep or discard those changes. If they want to keep them, ask for confirmation then use the save_file tool to save the file, then retry this edit. If they want to discard them, ask for confirmation then use the restore_file_from_disk tool to restore the on-disk contents, then retry this edit.",
                                    (true, false) => "This file has unsaved changes. Ask the user whether they want to keep or discard those changes. If they want to keep them, ask for confirmation then use the save_file tool to save the file, then retry this edit. If they want to discard them, ask the user to manually revert the file, then inform you when it's ok to proceed.",
                                    (false, true) => "This file has unsaved changes. Ask the user whether they want to keep or discard those changes. If they want to keep them, ask the user to manually save the file, then inform you when it's ok to proceed. If they want to discard them, ask for confirmation then use the restore_file_from_disk tool to restore the on-disk contents, then retry this edit.",
                                    (false, false) => "This file has unsaved changes. Ask the user whether they want to keep or discard those changes, then ask them to save or revert the file manually and inform you when it's ok to proceed.",
                                };
                                anyhow::bail!("{message}");
                            }

                            if let (Some(last_read), Some(current)) = (last_read_mtime, current_mtime)
                                && current != last_read
                            {
                                anyhow::bail!(
                                    "The file {} has been modified since you last read it. Please read the file again to get the current state before editing it.",
                                    input.path.display()
                                );
                            }
                        }

                        let diff = cx.new(|cx| Diff::new(buffer.clone(), cx));
                        event_stream.update_diff(diff.clone());
                        let _finalize_diff = util::defer({
                            let diff = diff.downgrade();
                            let mut cx = cx.clone();
                            move || {
                                diff.update(&mut cx, |diff, cx| diff.finalize(cx)).ok();
                            }
                        });

                        let old_snapshot = buffer.read_with(cx, |buffer, _cx| buffer.snapshot());
                        let old_text = cx.background_spawn({
                            let old_snapshot = old_snapshot.clone();
                            async move { Arc::new(old_snapshot.text()) }
                        }).await;

                        let (request, model) = self.thread.update(cx, |thread, cx| {
                            let request = thread.build_completion_request(CompletionIntent::ToolResults, cx);
                            (request, thread.model().cloned())
                        })?;
                        let request = request?;
                        let model = model.context("No language model configured")?;
                        let edit_format = EditFormat::from_model(model.clone())?;
                        let edit_agent = EditAgent::new(
                            model,
                            project.clone(),
                            action_log.clone(),
                            self.templates.clone(),
                            edit_format,
                            allow_thinking,
                            update_agent_location,
                        );

                        let (output, mut events) = if matches!(input.mode, EditFileMode::Edit) {
                            edit_agent.edit(buffer.clone(), input.display_description.clone(), &request, cx)
                        } else {
                            edit_agent.overwrite(buffer.clone(), input.display_description.clone(), &request, cx)
                        };

                        let mut hallucinated_old_text = false;
                        let mut ambiguous_ranges = Vec::new();
                        let mut emitted_location = false;
                        loop {
                            let event = futures::select! {
                                event = events.next().fuse() => match event {
                                    Some(event) => event,
                                    None => break,
                                },
                                _ = event_stream.cancelled_by_user().fuse() => {
                                    anyhow::bail!("Edit cancelled by user");
                                }
                            };
                            match event {
                                EditAgentOutputEvent::Edited(range) => {
                                    if !emitted_location {
                                        let line = Some(buffer.update(cx, |buffer, _cx| range.start.to_point(&buffer.snapshot()).row));
                                        if let Some(abs_path) = abs_path.clone() {
                                            event_stream.update_fields(acp::ToolCallUpdateFields::new().locations(vec![acp::ToolCallLocation::new(abs_path).line(line)]));
                                        }
                                        emitted_location = true;
                                    }
                                }
                                EditAgentOutputEvent::UnresolvedEditRange => hallucinated_old_text = true,
                                EditAgentOutputEvent::AmbiguousEditRange(ranges) => ambiguous_ranges = ranges,
                                EditAgentOutputEvent::ResolvingEditRange(range) => {
                                    diff.update(cx, |card, cx| card.reveal_range(range.clone(), cx));
                                }
                            }
                        }

                        output.await?;

                        let format_on_save_enabled = buffer.read_with(cx, |buffer, cx| {
                            let settings = language_settings::LanguageSettings::for_buffer(buffer, cx);
                            settings.format_on_save != FormatOnSave::Off
                        });

                        if format_on_save_enabled {
                            action_log.update(cx, |log, cx| log.buffer_edited(buffer.clone(), cx));
                            let format_task = project.update(cx, |project, cx| {
                                project.format(
                                    HashSet::from_iter([buffer.clone()]),
                                    LspFormatTarget::Buffers,
                                    false,
                                    FormatTrigger::Save,
                                    cx,
                                )
                            });
                            format_task.await.log_err();
                        }

                        project.update(cx, |project, cx| project.save_buffer(buffer.clone(), cx)).await?;
                        action_log.update(cx, |log, cx| log.buffer_edited(buffer.clone(), cx));

                        let new_snapshot = buffer.read_with(cx, |buffer, _cx| buffer.snapshot());
                        let (new_text, unified_diff) = cx.background_spawn({
                            let new_snapshot = new_snapshot.clone();
                            let old_text = old_text.clone();
                            async move {
                                let new_text = new_snapshot.text();
                                let diff = language::unified_diff(&old_text, &new_text);
                                (new_text, diff)
                            }
                        }).await;

                        let input_path = input.path.display();
                        if unified_diff.is_empty() {
                            anyhow::ensure!(
                                !hallucinated_old_text,
                                formatdoc!("Some edits were produced but none of them could be applied. Read the relevant sections of {input_path} again so that I can perform the requested edits.")
                            );
                            anyhow::ensure!(ambiguous_ranges.is_empty(), {
                                let line_numbers = ambiguous_ranges.iter().map(|range| range.start.to_string()).collect::<Vec<_>>().join(", ");
                                formatdoc!("<old_text> matches more than one position in the file (lines: {line_numbers}). Read the relevant sections of {input_path} again and extend <old_text> so that I can perform the requested edits.")
                            });
                        }

                        anyhow::Ok(EditFileToolOutput::Success {
                            input_path: input.path,
                            new_text,
                            old_text,
                            diff: unified_diff,
                        })
                    }.await;

                    result.map_err(|e| EditFileToolOutput::Error { error: e.to_string() })
                }
                EditSessionResult::Failed {
                    error,
                    session: None,
                } => Err(EditFileToolOutput::Error { error }),
            }
        })
    }

    fn replay(
        &self,
        _input: Self::Input,
        output: Self::Output,
        event_stream: ToolCallEventStream,
        cx: &mut App,
    ) -> Result<()> {
        match output {
            EditFileToolOutput::Success {
                input_path,
                old_text,
                new_text,
                ..
            } => {
                event_stream.update_diff(cx.new(|cx| {
                    Diff::finalized(
                        input_path.to_string_lossy().into_owned(),
                        Some(old_text.to_string()),
                        new_text,
                        self.language_registry.clone(),
                        cx,
                    )
                }));
                Ok(())
            }
            EditFileToolOutput::Error { .. } => Ok(()),
        }
    }
}

impl EditSession {
    async fn new(
        path: PathBuf,
        display_description: &str,
        mode: EditFileMode,
        tool: &EditFileTool,
        event_stream: &ToolCallEventStream,
        cx: &mut AsyncApp,
    ) -> Result<Self, String> {
        let input = EditFileToolInput {
            display_description: display_description.to_string(),
            path: path.clone(),
            mode: mode.clone(),
            content: None,
            edits: None,
        };
        let project_path = cx.update(|cx| {
            resolve_path(&input, tool.project.clone(), cx).map_err(|error| error.to_string())
        })?;

        let Some(abs_path) = cx.update(|cx| tool.project.read(cx).absolute_path(&project_path, cx))
        else {
            return Err(format!(
                "Worktree at '{}' does not exist",
                path.to_string_lossy()
            ));
        };

        event_stream.update_fields(
            acp::ToolCallUpdateFields::new()
                .locations(vec![acp::ToolCallLocation::new(abs_path.clone())]),
        );

        cx.update(|cx| tool.authorize(&input, event_stream, cx))
            .await
            .map_err(|error| error.to_string())?;

        let buffer = tool
            .project
            .update(cx, |project, cx| project.open_buffer(project_path, cx))
            .await
            .map_err(|error| error.to_string())?;

        let file_changed_since_last_read =
            ensure_buffer_saved(&buffer, &abs_path, &tool.thread, cx)?;

        let diff = cx.new(|cx| Diff::new(buffer.clone(), cx));
        event_stream.update_diff(diff.clone());
        let finalize_diff_guard = util::defer(Box::new({
            let diff = diff.downgrade();
            let mut cx = cx.clone();
            move || {
                diff.update(&mut cx, |diff, cx| diff.finalize(cx)).ok();
            }
        }) as Box<dyn FnOnce()>);

        let old_snapshot = buffer.read_with(cx, |buffer, _cx| buffer.snapshot());
        let old_text = cx
            .background_spawn({
                let old_snapshot = old_snapshot.clone();
                async move { Arc::new(old_snapshot.text()) }
            })
            .await;

        Ok(Self {
            abs_path,
            input_path: path,
            buffer,
            old_text,
            diff,
            mode,
            parser: ToolEditParser::default(),
            pipeline: EditPipeline::new(),
            file_changed_since_last_read,
            _finalize_diff_guard: finalize_diff_guard,
        })
    }

    async fn finalize(
        &mut self,
        input: EditFileToolInput,
        tool: &EditFileTool,
        event_stream: &ToolCallEventStream,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        match input.mode {
            EditFileMode::Overwrite | EditFileMode::Create => {
                let content = input
                    .content
                    .ok_or_else(|| "'content' field is required for write mode".to_string())?;
                let events = self.parser.finalize_content(&content);
                self.process_events(&events, tool, event_stream, cx)
            }
            EditFileMode::Edit => {
                let edits = input
                    .edits
                    .ok_or_else(|| "'edits' field is required for edit mode".to_string())?;
                let events = self.parser.finalize_edits(&edits);
                self.process_events(&events, tool, event_stream, cx)
            }
        }
    }

    async fn compute_new_text_and_diff(&self, cx: &mut AsyncApp) -> (String, String) {
        let new_snapshot = self.buffer.read_with(cx, |buffer, _cx| buffer.snapshot());
        cx.background_spawn({
            let new_snapshot = new_snapshot.clone();
            let old_text = self.old_text.clone();
            async move {
                let new_text = new_snapshot.text();
                let diff = language::unified_diff(&old_text, &new_text);
                (new_text, diff)
            }
        })
        .await
    }

    fn process(
        &mut self,
        partial: EditFileToolPartialInput,
        tool: &EditFileTool,
        event_stream: &ToolCallEventStream,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        match &self.mode {
            EditFileMode::Overwrite | EditFileMode::Create => {
                if let Some(content) = &partial.content {
                    let events = self.parser.push_content(content);
                    self.process_events(&events, tool, event_stream, cx)?;
                }
            }
            EditFileMode::Edit => {
                if let Some(edits) = partial.edits {
                    let events = self.parser.push_edits(&edits);
                    self.process_events(&events, tool, event_stream, cx)?;
                }
            }
        }
        Ok(())
    }

    fn process_events(
        &mut self,
        events: &[ToolEditEvent],
        tool: &EditFileTool,
        event_stream: &ToolCallEventStream,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let action_log = tool
            .thread
            .read_with(cx, |thread, _cx| thread.action_log().clone())
            .map_err(|_| "thread was dropped".to_string())?;
        for event in events {
            match event {
                ToolEditEvent::ContentChunk { chunk } => {
                    let (buffer_id, buffer_len) = self
                        .buffer
                        .read_with(cx, |buffer, _cx| (buffer.remote_id(), buffer.len()));
                    let edit_range = if self.pipeline.content_written {
                        buffer_len..buffer_len
                    } else {
                        0..buffer_len
                    };
                    agent_edit_buffer(
                        &self.buffer,
                        [(edit_range, chunk.as_str())],
                        &action_log,
                        cx,
                    );
                    cx.update(|cx| {
                        tool.set_agent_location(
                            self.buffer.downgrade(),
                            text::Anchor::max_for_buffer(buffer_id),
                            cx,
                        );
                    });
                    self.pipeline.content_written = true;
                }
                ToolEditEvent::OldTextChunk {
                    chunk, done: false, ..
                } => {
                    self.pipeline.ensure_resolving_old_text(&self.buffer, cx);
                    if let Some(EditPipelineEntry::ResolvingOldText { matcher }) =
                        &mut self.pipeline.current_edit
                        && !chunk.is_empty()
                        && let Some(match_range) = matcher.push(chunk, None)
                    {
                        let anchor_range = self.buffer.read_with(cx, |buffer, _cx| {
                            buffer.anchor_range_outside(match_range.clone())
                        });
                        self.diff
                            .update(cx, |diff, cx| diff.reveal_range(anchor_range, cx));
                    }
                }
                ToolEditEvent::OldTextChunk {
                    edit_index,
                    chunk,
                    done: true,
                } => {
                    self.pipeline.ensure_resolving_old_text(&self.buffer, cx);
                    let Some(EditPipelineEntry::ResolvingOldText { matcher }) =
                        &mut self.pipeline.current_edit
                    else {
                        continue;
                    };
                    if !chunk.is_empty() {
                        matcher.push(chunk, None);
                    }
                    let range = extract_match(
                        matcher.finish(),
                        &self.buffer,
                        *edit_index,
                        self.file_changed_since_last_read,
                        cx,
                    )?;
                    let anchor_range = self
                        .buffer
                        .read_with(cx, |buffer, _cx| buffer.anchor_range_outside(range.clone()));
                    self.diff
                        .update(cx, |diff, cx| diff.reveal_range(anchor_range, cx));

                    let snapshot = self.buffer.read_with(cx, |buffer, _cx| buffer.snapshot());
                    let line = snapshot.offset_to_point(range.start).row;
                    event_stream.update_fields(acp::ToolCallUpdateFields::new().locations(vec![
                        acp::ToolCallLocation::new(&self.abs_path).line(Some(line)),
                    ]));

                    let buffer_indent = snapshot.line_indent_for_row(line);
                    let query_indent = text::LineIndent::from_iter(
                        matcher
                            .query_lines()
                            .first()
                            .map(|s| s.as_str())
                            .unwrap_or("")
                            .chars(),
                    );
                    let indent_delta = compute_indent_delta(buffer_indent, query_indent);
                    let old_text_in_buffer =
                        snapshot.text_for_range(range.clone()).collect::<String>();
                    let text_snapshot = self
                        .buffer
                        .read_with(cx, |buffer, _cx| buffer.text_snapshot());
                    self.pipeline.current_edit = Some(EditPipelineEntry::StreamingNewText {
                        streaming_diff: StreamingDiff::new(old_text_in_buffer),
                        edit_cursor: range.start,
                        reindenter: Reindenter::new(indent_delta),
                        original_snapshot: text_snapshot,
                    });
                }
                ToolEditEvent::NewTextChunk {
                    chunk, done: false, ..
                } => {
                    let Some(EditPipelineEntry::StreamingNewText {
                        streaming_diff,
                        edit_cursor,
                        reindenter,
                        original_snapshot,
                    }) = &mut self.pipeline.current_edit
                    else {
                        continue;
                    };
                    let reindented = reindenter.push(chunk);
                    if reindented.is_empty() {
                        continue;
                    }
                    let char_ops = streaming_diff.push_new(&reindented);
                    apply_char_operations(
                        &char_ops,
                        &self.buffer,
                        original_snapshot,
                        edit_cursor,
                        &action_log,
                        cx,
                    );
                }
                ToolEditEvent::NewTextChunk {
                    chunk, done: true, ..
                } => {
                    let Some(EditPipelineEntry::StreamingNewText {
                        mut streaming_diff,
                        mut edit_cursor,
                        mut reindenter,
                        original_snapshot,
                    }) = self.pipeline.current_edit.take()
                    else {
                        continue;
                    };
                    let mut final_text = reindenter.push(chunk);
                    final_text.push_str(&reindenter.finish());
                    if !final_text.is_empty() {
                        let char_ops = streaming_diff.push_new(&final_text);
                        apply_char_operations(
                            &char_ops,
                            &self.buffer,
                            &original_snapshot,
                            &mut edit_cursor,
                            &action_log,
                            cx,
                        );
                    }
                    let remaining_ops = streaming_diff.finish();
                    apply_char_operations(
                        &remaining_ops,
                        &self.buffer,
                        &original_snapshot,
                        &mut edit_cursor,
                        &action_log,
                        cx,
                    );
                }
            }
        }
        Ok(())
    }
}

/// Validate that the file path is valid, meaning:
///
/// - For `edit`, the path must point to an existing file.
/// - For `create`, the file must not already exist, but it's parent dir must exist.
/// - For `overwrite`, the path may point to an existing file or a new file in an existing parent dir.
fn resolve_path(
    input: &EditFileToolInput,
    project: Entity<Project>,
    cx: &mut App,
) -> Result<ProjectPath> {
    match input.mode {
        EditFileMode::Edit => {
            let project = project.read(cx);
            let path = project
                .find_project_path(&input.path, cx)
                .context("Can't edit file: path not found")?;

            let entry = project
                .entry_for_path(&path, cx)
                .context("Can't edit file: path not found")?;

            anyhow::ensure!(entry.is_file(), "Can't edit file: path is a directory");
            Ok(path)
        }

        EditFileMode::Overwrite => {
            let project_entity = project;
            let project = project_entity.read(cx);
            if let Some(path) = project.find_project_path(&input.path, cx) {
                if let Some(entry) = project.entry_for_path(&path, cx) {
                    anyhow::ensure!(entry.is_file(), "Can't write to file: path is a directory");
                    Ok(path)
                } else {
                    let _ = project;
                    resolve_new_file_path(input, project_entity, cx)
                }
            } else {
                let _ = project;
                resolve_new_file_path(input, project_entity, cx)
            }
        }

        EditFileMode::Create => {
            let project_entity = project;
            let project = project_entity.read(cx);
            if let Some(path) = project.find_project_path(&input.path, cx) {
                anyhow::ensure!(
                    project.entry_for_path(&path, cx).is_none(),
                    "Can't create file: file already exists"
                );
            }

            let _ = project;
            resolve_new_file_path(input, project_entity, cx)
        }
    }
}

fn resolve_new_file_path(
    input: &EditFileToolInput,
    project: Entity<Project>,
    cx: &mut App,
) -> Result<ProjectPath> {
    let project = project.read(cx);
    let parent_path = input
        .path
        .parent()
        .context("Can't create file: incorrect path")?;

    let parent_project_path = project.find_project_path(&parent_path, cx);

    let parent_entry = parent_project_path
        .as_ref()
        .and_then(|path| project.entry_for_path(path, cx))
        .context("Can't create file: parent directory doesn't exist")?;

    anyhow::ensure!(
        parent_entry.is_dir(),
        "Can't create file: parent is not a directory"
    );

    let file_name = input
        .path
        .file_name()
        .and_then(|file_name| file_name.to_str())
        .and_then(|file_name| RelPath::unix(file_name).ok())
        .context("Can't create file: invalid filename")?;

    let new_file_path = parent_project_path.map(|parent| ProjectPath {
        path: parent.path.join(file_name),
        ..parent
    });

    new_file_path.context("Can't create file")
}

fn ensure_buffer_saved(
    buffer: &Entity<Buffer>,
    abs_path: &PathBuf,
    thread: &WeakEntity<Thread>,
    cx: &mut AsyncApp,
) -> Result<bool, String> {
    let last_read_mtime = thread
        .read_with(cx, |thread, _cx| {
            thread
                .action_log()
                .read_with(cx, |log, _| log.file_read_time(abs_path))
        })
        .map_err(|_| "thread was dropped".to_string())?;

    let (current_mtime, is_dirty, has_save_tool, has_restore_tool) = thread
        .read_with(cx, |thread, cx| {
            let current = buffer
                .read(cx)
                .file()
                .and_then(|file| file.disk_state().mtime());
            let dirty = buffer.read(cx).is_dirty();
            let has_save = thread.has_tool(SaveFileTool::NAME);
            let has_restore = thread.has_tool(RestoreFileFromDiskTool::NAME);
            (current, dirty, has_save, has_restore)
        })
        .map_err(|_| "thread was dropped".to_string())?;

    if is_dirty {
        let message = match (has_save_tool, has_restore_tool) {
            (true, true) => {
                "This file has unsaved changes. Ask the user whether they want to keep or discard those changes. If they want to keep them, ask for confirmation then use the save_file tool to save the file, then retry this edit. If they want to discard them, ask for confirmation then use the restore_file_from_disk tool to restore the on-disk contents, then retry this edit."
            }
            (true, false) => {
                "This file has unsaved changes. Ask the user whether they want to keep or discard those changes. If they want to keep them, ask for confirmation then use the save_file tool to save the file, then retry this edit. If they want to discard them, ask the user to manually revert the file, then inform you when it's ok to proceed."
            }
            (false, true) => {
                "This file has unsaved changes. Ask the user whether they want to keep or discard those changes. If they want to keep them, ask the user to manually save the file, then inform you when it's ok to proceed. If they want to discard them, ask for confirmation then use the restore_file_from_disk tool to restore the on-disk contents, then retry this edit."
            }
            (false, false) => {
                "This file has unsaved changes. Ask the user whether they want to keep or discard those changes, then ask them to save or revert the file manually and inform you when it's ok to proceed."
            }
        };
        return Err(message.to_string());
    }

    if let (Some(last_read), Some(current)) = (last_read_mtime, current_mtime)
        && current != last_read
    {
        return Ok(true);
    }

    Ok(false)
}

fn apply_char_operations(
    ops: &[CharOperation],
    buffer: &Entity<Buffer>,
    snapshot: &text::BufferSnapshot,
    edit_cursor: &mut usize,
    action_log: &Entity<ActionLog>,
    cx: &mut AsyncApp,
) {
    for op in ops {
        match op {
            CharOperation::Insert { text } => {
                let anchor = snapshot.anchor_after(*edit_cursor);
                agent_edit_buffer(buffer, [(anchor..anchor, text.as_str())], action_log, cx);
            }
            CharOperation::Delete { bytes } => {
                let delete_end = *edit_cursor + bytes;
                let anchor_range = snapshot.anchor_range_inside(*edit_cursor..delete_end);
                agent_edit_buffer(buffer, [(anchor_range, "")], action_log, cx);
                *edit_cursor = delete_end;
            }
            CharOperation::Keep { bytes } => {
                *edit_cursor += bytes;
            }
        }
    }
}

fn extract_match(
    matches: Vec<Range<usize>>,
    buffer: &Entity<Buffer>,
    edit_index: usize,
    file_changed_since_last_read: bool,
    cx: &mut AsyncApp,
) -> Result<Range<usize>, String> {
    let changed_message = if file_changed_since_last_read {
        " The file has changed on disk since you last read it."
    } else {
        ""
    };

    match matches.len() {
        0 => Err(format!(
            "Could not find matching text for edit at index {}. The old_text did not match any content in the file.{} Please read the file again to get the current content.",
            edit_index, changed_message,
        )),
        1 => Ok(matches.into_iter().next().expect("single match exists")),
        _ => {
            let snapshot = buffer.read_with(cx, |buffer, _cx| buffer.snapshot());
            let lines = matches
                .iter()
                .map(|range| (snapshot.offset_to_point(range.start).row + 1).to_string())
                .collect::<Vec<_>>()
                .join(", ");
            Err(format!(
                "Edit {} matched multiple locations in the file at lines: {}. Please provide more context in old_text to uniquely identify the location.",
                edit_index, lines,
            ))
        }
    }
}

fn agent_edit_buffer<I, S, T>(
    buffer: &Entity<Buffer>,
    edits: I,
    action_log: &Entity<ActionLog>,
    cx: &mut AsyncApp,
) where
    I: IntoIterator<Item = (Range<S>, T)>,
    S: ToOffset,
    T: Into<Arc<str>>,
{
    cx.update(|cx| {
        buffer.update(cx, |buffer, cx| {
            buffer.edit(edits, None, cx);
        });
        action_log.update(cx, |log, cx| log.buffer_edited(buffer.clone(), cx));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::tool_permissions::{SensitiveSettingsKind, sensitive_settings_kind};
    use crate::{ContextServerRegistry, Templates};
    use fs::Fs as _;
    use gpui::{TestAppContext, UpdateGlobal};
    use language_model::fake_provider::FakeLanguageModel;
    use prompt_store::ProjectContext;
    use serde_json::json;
    use settings::Settings;
    use settings::SettingsStore;
    use util::{path, rel_path::rel_path};

    #[gpui::test]
    async fn test_edit_nonexistent_file(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = project::FakeFs::new(cx.executor());
        fs.insert_tree("/root", json!({})).await;
        let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
        let language_registry = project.read_with(cx, |project, _cx| project.languages().clone());
        let context_server_registry =
            cx.new(|cx| ContextServerRegistry::new(project.read(cx).context_server_store(), cx));
        let model = Arc::new(FakeLanguageModel::default());
        let thread = cx.new(|cx| {
            Thread::new(
                project.clone(),
                cx.new(|_cx| ProjectContext::default()),
                context_server_registry,
                Templates::new(),
                Some(model),
                cx,
            )
        });
        let result = cx
            .update(|cx| {
                let input = EditFileToolInput {
                    display_description: "Some edit".into(),
                    path: "root/nonexistent_file.txt".into(),
                    mode: EditFileMode::Edit,
                    content: None,
                    edits: None,
                };
                Arc::new(EditFileTool::new(
                    project,
                    thread.downgrade(),
                    language_registry,
                    Templates::new(),
                ))
                .run(
                    ToolInput::resolved(input),
                    ToolCallEventStream::test().0,
                    cx,
                )
            })
            .await;
        assert_eq!(
            result.unwrap_err().to_string(),
            "Can't edit file: path not found"
        );
    }

    #[gpui::test]
    async fn test_resolve_path_for_creating_file(cx: &mut TestAppContext) {
        let mode = &EditFileMode::Create;

        let result = test_resolve_path(mode, "root/new.txt", cx);
        assert_resolved_path_eq(result.await, rel_path("new.txt"));

        let result = test_resolve_path(mode, "new.txt", cx);
        assert_resolved_path_eq(result.await, rel_path("new.txt"));

        let result = test_resolve_path(mode, "dir/new.txt", cx);
        assert_resolved_path_eq(result.await, rel_path("dir/new.txt"));

        let result = test_resolve_path(mode, "root/dir/subdir/existing.txt", cx);
        assert_eq!(
            result.await.unwrap_err().to_string(),
            "Can't create file: file already exists"
        );

        let result = test_resolve_path(mode, "root/dir/nonexistent_dir/new.txt", cx);
        assert_eq!(
            result.await.unwrap_err().to_string(),
            "Can't create file: parent directory doesn't exist"
        );
    }

    #[gpui::test]
    async fn test_resolve_path_for_overwriting_new_file(cx: &mut TestAppContext) {
        let mode = &EditFileMode::Overwrite;

        let result = test_resolve_path(mode, "root/new.txt", cx);
        assert_resolved_path_eq(result.await, rel_path("new.txt"));
    }

    #[gpui::test]
    async fn test_resolve_path_for_editing_file(cx: &mut TestAppContext) {
        let mode = &EditFileMode::Edit;

        let path_with_root = "root/dir/subdir/existing.txt";
        let path_without_root = "dir/subdir/existing.txt";
        let result = test_resolve_path(mode, path_with_root, cx);
        assert_resolved_path_eq(result.await, rel_path(path_without_root));

        let result = test_resolve_path(mode, path_without_root, cx);
        assert_resolved_path_eq(result.await, rel_path(path_without_root));

        let result = test_resolve_path(mode, "root/nonexistent.txt", cx);
        assert_eq!(
            result.await.unwrap_err().to_string(),
            "Can't edit file: path not found"
        );

        let result = test_resolve_path(mode, "root/dir", cx);
        assert_eq!(
            result.await.unwrap_err().to_string(),
            "Can't edit file: path is a directory"
        );
    }

    async fn test_resolve_path(
        mode: &EditFileMode,
        path: &str,
        cx: &mut TestAppContext,
    ) -> anyhow::Result<ProjectPath> {
        init_test(cx);

        let fs = project::FakeFs::new(cx.executor());
        fs.insert_tree(
            "/root",
            json!({
                "dir": {
                    "subdir": {
                        "existing.txt": "hello"
                    }
                }
            }),
        )
        .await;
        let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;

        let input = EditFileToolInput {
            display_description: "Some edit".into(),
            path: path.into(),
            mode: mode.clone(),
            content: None,
            edits: None,
        };

        cx.update(|cx| resolve_path(&input, project, cx))
    }

    #[track_caller]
    fn assert_resolved_path_eq(path: anyhow::Result<ProjectPath>, expected: &RelPath) {
        let actual = path.expect("Should return valid path").path;
        assert_eq!(actual.as_ref(), expected);
    }

    #[test]
    fn test_deserialize_write_alias_and_stringified_edits() {
        let input = serde_json::from_value::<EditFileToolInput>(json!({
            "display_description": "Update text",
            "path": "root/file.txt",
            "mode": "write",
            "content": "hello"
        }))
        .unwrap();

        assert!(matches!(input.mode, EditFileMode::Overwrite));
        assert_eq!(input.content.as_deref(), Some("hello"));

        let input = serde_json::from_value::<EditFileToolInput>(json!({
            "display_description": "Update text",
            "path": "root/file.txt",
            "mode": "edit",
            "edits": "[{\"old_text\":\"a\",\"new_text\":\"b\"}]"
        }))
        .unwrap();

        assert!(matches!(input.mode, EditFileMode::Edit));
        assert_eq!(input.edits.as_ref().map(Vec::len), Some(1));
        let edit = &input.edits.unwrap()[0];
        assert_eq!(edit.old_text, "a");
        assert_eq!(edit.new_text, "b");
    }

    #[test]
    fn test_partial_input_deserializes_streaming_fields() {
        let input = serde_json::from_value::<EditFileToolPartialInput>(json!({
            "display_description": "Partial update",
            "path": "root/file.txt",
            "mode": "edit",
            "edits": "[{\"old_text\":\"a\"}]"
        }))
        .unwrap();

        assert_eq!(input.display_description.as_deref(), Some("Partial update"));
        assert_eq!(input.path.as_deref(), Some("root/file.txt"));
        assert!(matches!(input.mode, Some(EditFileMode::Edit)));
        assert_eq!(input.edits.as_ref().map(Vec::len), Some(1));
    }

    #[gpui::test]
    async fn test_format_on_save(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = project::FakeFs::new(cx.executor());
        fs.insert_tree("/root", json!({"src": {}})).await;

        let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;

        // Set up a Rust language with LSP formatting support
        let rust_language = Arc::new(language::Language::new(
            language::LanguageConfig {
                name: "Rust".into(),
                matcher: language::LanguageMatcher {
                    path_suffixes: vec!["rs".to_string()],
                    ..Default::default()
                },
                ..Default::default()
            },
            None,
        ));

        // Register the language and fake LSP
        let language_registry = project.read_with(cx, |project, _| project.languages().clone());
        language_registry.add(rust_language);

        let mut fake_language_servers = language_registry.register_fake_lsp(
            "Rust",
            language::FakeLspAdapter {
                capabilities: lsp::ServerCapabilities {
                    document_formatting_provider: Some(lsp::OneOf::Left(true)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );

        // Create the file
        fs.save(
            path!("/root/src/main.rs").as_ref(),
            &"initial content".into(),
            language::LineEnding::Unix,
        )
        .await
        .unwrap();

        // Open the buffer to trigger LSP initialization
        let buffer = project
            .update(cx, |project, cx| {
                project.open_local_buffer(path!("/root/src/main.rs"), cx)
            })
            .await
            .unwrap();

        // Register the buffer with language servers
        let _handle = project.update(cx, |project, cx| {
            project.register_buffer_with_language_servers(&buffer, cx)
        });

        const UNFORMATTED_CONTENT: &str = "fn main() {println!(\"Hello!\");}\n";
        const FORMATTED_CONTENT: &str =
            "This file was formatted by the fake formatter in the test.\n";

        // Get the fake language server and set up formatting handler
        let fake_language_server = fake_language_servers.next().await.unwrap();
        fake_language_server.set_request_handler::<lsp::request::Formatting, _, _>({
            |_, _| async move {
                Ok(Some(vec![lsp::TextEdit {
                    range: lsp::Range::new(lsp::Position::new(0, 0), lsp::Position::new(1, 0)),
                    new_text: FORMATTED_CONTENT.to_string(),
                }]))
            }
        });

        let context_server_registry =
            cx.new(|cx| ContextServerRegistry::new(project.read(cx).context_server_store(), cx));
        let model = Arc::new(FakeLanguageModel::default());
        let thread = cx.new(|cx| {
            Thread::new(
                project.clone(),
                cx.new(|_cx| ProjectContext::default()),
                context_server_registry,
                Templates::new(),
                Some(model.clone()),
                cx,
            )
        });

        // First, test with format_on_save enabled
        cx.update(|cx| {
            SettingsStore::update_global(cx, |store, cx| {
                store.update_user_settings(cx, |settings| {
                    settings.project.all_languages.defaults.format_on_save = Some(FormatOnSave::On);
                    settings.project.all_languages.defaults.formatter =
                        Some(language::language_settings::FormatterList::default());
                });
            });
        });

        // Have the model stream unformatted content
        let edit_result = {
            let edit_task = cx.update(|cx| {
                let input = EditFileToolInput {
                    display_description: "Create main function".into(),
                    path: "root/src/main.rs".into(),
                    mode: EditFileMode::Overwrite,
                    content: None,
                    edits: None,
                };
                Arc::new(EditFileTool::new(
                    project.clone(),
                    thread.downgrade(),
                    language_registry.clone(),
                    Templates::new(),
                ))
                .run(
                    ToolInput::resolved(input),
                    ToolCallEventStream::test().0,
                    cx,
                )
            });

            // Stream the unformatted content
            cx.executor().run_until_parked();
            model.send_last_completion_stream_text_chunk(UNFORMATTED_CONTENT.to_string());
            model.end_last_completion_stream();

            edit_task.await
        };
        assert!(edit_result.is_ok());

        // Wait for any async operations (e.g. formatting) to complete
        cx.executor().run_until_parked();

        // Read the file to verify it was formatted automatically
        let new_content = fs.load(path!("/root/src/main.rs").as_ref()).await.unwrap();
        assert_eq!(
            // Ignore carriage returns on Windows
            new_content.replace("\r\n", "\n"),
            FORMATTED_CONTENT,
            "Code should be formatted when format_on_save is enabled"
        );

        let stale_buffer_count = thread
            .read_with(cx, |thread, _cx| thread.action_log.clone())
            .read_with(cx, |log, cx| log.stale_buffers(cx).count());

        assert_eq!(
            stale_buffer_count, 0,
            "BUG: Buffer is incorrectly marked as stale after format-on-save. Found {} stale buffers. \
             This causes the agent to think the file was modified externally when it was just formatted.",
            stale_buffer_count
        );

        // Next, test with format_on_save disabled
        cx.update(|cx| {
            SettingsStore::update_global(cx, |store, cx| {
                store.update_user_settings(cx, |settings| {
                    settings.project.all_languages.defaults.format_on_save =
                        Some(FormatOnSave::Off);
                });
            });
        });

        // Stream unformatted edits again
        let edit_result = {
            let edit_task = cx.update(|cx| {
                let input = EditFileToolInput {
                    display_description: "Update main function".into(),
                    path: "root/src/main.rs".into(),
                    mode: EditFileMode::Overwrite,
                    content: None,
                    edits: None,
                };
                Arc::new(EditFileTool::new(
                    project.clone(),
                    thread.downgrade(),
                    language_registry,
                    Templates::new(),
                ))
                .run(
                    ToolInput::resolved(input),
                    ToolCallEventStream::test().0,
                    cx,
                )
            });

            // Stream the unformatted content
            cx.executor().run_until_parked();
            model.send_last_completion_stream_text_chunk(UNFORMATTED_CONTENT.to_string());
            model.end_last_completion_stream();

            edit_task.await
        };
        assert!(edit_result.is_ok());

        // Wait for any async operations (e.g. formatting) to complete
        cx.executor().run_until_parked();

        // Verify the file was not formatted
        let new_content = fs.load(path!("/root/src/main.rs").as_ref()).await.unwrap();
        assert_eq!(
            // Ignore carriage returns on Windows
            new_content.replace("\r\n", "\n"),
            UNFORMATTED_CONTENT,
            "Code should not be formatted when format_on_save is disabled"
        );
    }

    #[gpui::test]
    async fn test_remove_trailing_whitespace(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = project::FakeFs::new(cx.executor());
        fs.insert_tree("/root", json!({"src": {}})).await;

        // Create a simple file with trailing whitespace
        fs.save(
            path!("/root/src/main.rs").as_ref(),
            &"initial content".into(),
            language::LineEnding::Unix,
        )
        .await
        .unwrap();

        let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
        let context_server_registry =
            cx.new(|cx| ContextServerRegistry::new(project.read(cx).context_server_store(), cx));
        let language_registry = project.read_with(cx, |project, _cx| project.languages().clone());
        let model = Arc::new(FakeLanguageModel::default());
        let thread = cx.new(|cx| {
            Thread::new(
                project.clone(),
                cx.new(|_cx| ProjectContext::default()),
                context_server_registry,
                Templates::new(),
                Some(model.clone()),
                cx,
            )
        });

        // First, test with remove_trailing_whitespace_on_save enabled
        cx.update(|cx| {
            SettingsStore::update_global(cx, |store, cx| {
                store.update_user_settings(cx, |settings| {
                    settings
                        .project
                        .all_languages
                        .defaults
                        .remove_trailing_whitespace_on_save = Some(true);
                });
            });
        });

        const CONTENT_WITH_TRAILING_WHITESPACE: &str =
            "fn main() {  \n    println!(\"Hello!\");  \n}\n";

        // Have the model stream content that contains trailing whitespace
        let edit_result = {
            let edit_task = cx.update(|cx| {
                let input = EditFileToolInput {
                    display_description: "Create main function".into(),
                    path: "root/src/main.rs".into(),
                    mode: EditFileMode::Overwrite,
                    content: None,
                    edits: None,
                };
                Arc::new(EditFileTool::new(
                    project.clone(),
                    thread.downgrade(),
                    language_registry.clone(),
                    Templates::new(),
                ))
                .run(
                    ToolInput::resolved(input),
                    ToolCallEventStream::test().0,
                    cx,
                )
            });

            // Stream the content with trailing whitespace
            cx.executor().run_until_parked();
            model.send_last_completion_stream_text_chunk(
                CONTENT_WITH_TRAILING_WHITESPACE.to_string(),
            );
            model.end_last_completion_stream();

            edit_task.await
        };
        assert!(edit_result.is_ok());

        // Wait for any async operations (e.g. formatting) to complete
        cx.executor().run_until_parked();

        // Read the file to verify trailing whitespace was removed automatically
        assert_eq!(
            // Ignore carriage returns on Windows
            fs.load(path!("/root/src/main.rs").as_ref())
                .await
                .unwrap()
                .replace("\r\n", "\n"),
            "fn main() {\n    println!(\"Hello!\");\n}\n",
            "Trailing whitespace should be removed when remove_trailing_whitespace_on_save is enabled"
        );

        // Next, test with remove_trailing_whitespace_on_save disabled
        cx.update(|cx| {
            SettingsStore::update_global(cx, |store, cx| {
                store.update_user_settings(cx, |settings| {
                    settings
                        .project
                        .all_languages
                        .defaults
                        .remove_trailing_whitespace_on_save = Some(false);
                });
            });
        });

        // Stream edits again with trailing whitespace
        let edit_result = {
            let edit_task = cx.update(|cx| {
                let input = EditFileToolInput {
                    display_description: "Update main function".into(),
                    path: "root/src/main.rs".into(),
                    mode: EditFileMode::Overwrite,
                    content: None,
                    edits: None,
                };
                Arc::new(EditFileTool::new(
                    project.clone(),
                    thread.downgrade(),
                    language_registry,
                    Templates::new(),
                ))
                .run(
                    ToolInput::resolved(input),
                    ToolCallEventStream::test().0,
                    cx,
                )
            });

            // Stream the content with trailing whitespace
            cx.executor().run_until_parked();
            model.send_last_completion_stream_text_chunk(
                CONTENT_WITH_TRAILING_WHITESPACE.to_string(),
            );
            model.end_last_completion_stream();

            edit_task.await
        };
        assert!(edit_result.is_ok());

        // Wait for any async operations (e.g. formatting) to complete
        cx.executor().run_until_parked();

        // Verify the file still has trailing whitespace
        // Read the file again - it should still have trailing whitespace
        let final_content = fs.load(path!("/root/src/main.rs").as_ref()).await.unwrap();
        assert_eq!(
            // Ignore carriage returns on Windows
            final_content.replace("\r\n", "\n"),
            CONTENT_WITH_TRAILING_WHITESPACE,
            "Trailing whitespace should remain when remove_trailing_whitespace_on_save is disabled"
        );
    }

    #[gpui::test]
    async fn test_authorize(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = project::FakeFs::new(cx.executor());
        let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
        let context_server_registry =
            cx.new(|cx| ContextServerRegistry::new(project.read(cx).context_server_store(), cx));
        let language_registry = project.read_with(cx, |project, _cx| project.languages().clone());
        let model = Arc::new(FakeLanguageModel::default());
        let thread = cx.new(|cx| {
            Thread::new(
                project.clone(),
                cx.new(|_cx| ProjectContext::default()),
                context_server_registry,
                Templates::new(),
                Some(model.clone()),
                cx,
            )
        });
        let tool = Arc::new(EditFileTool::new(
            project.clone(),
            thread.downgrade(),
            language_registry,
            Templates::new(),
        ));
        fs.insert_tree("/root", json!({})).await;

        // Test 1: Path with .ZZZ component should require confirmation
        let (stream_tx, mut stream_rx) = ToolCallEventStream::test();
        let _auth = cx.update(|cx| {
            tool.authorize(
                &EditFileToolInput {
                    display_description: "test 1".into(),
                    path: ".ZZZ/settings.json".into(),
                    mode: EditFileMode::Edit,
                    content: None,
                    edits: None,
                },
                &stream_tx,
                cx,
            )
        });

        let event = stream_rx.expect_authorization().await;
        assert_eq!(
            event.tool_call.fields.title,
            Some("test 1 (local settings)".into())
        );

        // Test 2: Path outside project should require confirmation
        let (stream_tx, mut stream_rx) = ToolCallEventStream::test();
        let _auth = cx.update(|cx| {
            tool.authorize(
                &EditFileToolInput {
                    display_description: "test 2".into(),
                    path: "/etc/hosts".into(),
                    mode: EditFileMode::Edit,
                    content: None,
                    edits: None,
                },
                &stream_tx,
                cx,
            )
        });

        let event = stream_rx.expect_authorization().await;
        assert_eq!(event.tool_call.fields.title, Some("test 2".into()));

        // Test 3: Relative path without .ZZZ should not require confirmation
        let (stream_tx, mut stream_rx) = ToolCallEventStream::test();
        cx.update(|cx| {
            tool.authorize(
                &EditFileToolInput {
                    display_description: "test 3".into(),
                    path: "root/src/main.rs".into(),
                    mode: EditFileMode::Edit,
                    content: None,
                    edits: None,
                },
                &stream_tx,
                cx,
            )
        })
        .await
        .unwrap();
        assert!(stream_rx.try_recv().is_err());

        // Test 4: Path with .ZZZ in the middle should require confirmation
        let (stream_tx, mut stream_rx) = ToolCallEventStream::test();
        let _auth = cx.update(|cx| {
            tool.authorize(
                &EditFileToolInput {
                    display_description: "test 4".into(),
                    path: "root/.ZZZ/tasks.json".into(),
                    mode: EditFileMode::Edit,
                    content: None,
                    edits: None,
                },
                &stream_tx,
                cx,
            )
        });
        let event = stream_rx.expect_authorization().await;
        assert_eq!(
            event.tool_call.fields.title,
            Some("test 4 (local settings)".into())
        );

        // Test 5: When global default is allow, sensitive and outside-project
        // paths still require confirmation
        cx.update(|cx| {
            let mut settings = agent_settings::AgentSettings::get_global(cx).clone();
            settings.tool_permissions.default = settings::ToolPermissionMode::Allow;
            agent_settings::AgentSettings::override_global(settings, cx);
        });

        // 5.1: .ZZZ/settings.json is a sensitive path — still prompts
        let (stream_tx, mut stream_rx) = ToolCallEventStream::test();
        let _auth = cx.update(|cx| {
            tool.authorize(
                &EditFileToolInput {
                    display_description: "test 5.1".into(),
                    path: ".ZZZ/settings.json".into(),
                    mode: EditFileMode::Edit,
                    content: None,
                    edits: None,
                },
                &stream_tx,
                cx,
            )
        });
        let event = stream_rx.expect_authorization().await;
        assert_eq!(
            event.tool_call.fields.title,
            Some("test 5.1 (local settings)".into())
        );

        // 5.2: /etc/hosts is outside the project, but Allow auto-approves
        let (stream_tx, mut stream_rx) = ToolCallEventStream::test();
        cx.update(|cx| {
            tool.authorize(
                &EditFileToolInput {
                    display_description: "test 5.2".into(),
                    path: "/etc/hosts".into(),
                    mode: EditFileMode::Edit,
                    content: None,
                    edits: None,
                },
                &stream_tx,
                cx,
            )
        })
        .await
        .unwrap();
        assert!(stream_rx.try_recv().is_err());

        // 5.3: Normal in-project path with allow — no confirmation needed
        let (stream_tx, mut stream_rx) = ToolCallEventStream::test();
        cx.update(|cx| {
            tool.authorize(
                &EditFileToolInput {
                    display_description: "test 5.3".into(),
                    path: "root/src/main.rs".into(),
                    mode: EditFileMode::Edit,
                    content: None,
                    edits: None,
                },
                &stream_tx,
                cx,
            )
        })
        .await
        .unwrap();
        assert!(stream_rx.try_recv().is_err());

        // 5.4: With Confirm default, non-project paths still prompt
        cx.update(|cx| {
            let mut settings = agent_settings::AgentSettings::get_global(cx).clone();
            settings.tool_permissions.default = settings::ToolPermissionMode::Confirm;
            agent_settings::AgentSettings::override_global(settings, cx);
        });

        let (stream_tx, mut stream_rx) = ToolCallEventStream::test();
        let _auth = cx.update(|cx| {
            tool.authorize(
                &EditFileToolInput {
                    display_description: "test 5.4".into(),
                    path: "/etc/hosts".into(),
                    mode: EditFileMode::Edit,
                    content: None,
                    edits: None,
                },
                &stream_tx,
                cx,
            )
        });

        let event = stream_rx.expect_authorization().await;
        assert_eq!(event.tool_call.fields.title, Some("test 5.4".into()));
    }

    #[gpui::test]
    async fn test_authorize_create_under_symlink_with_allow(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = project::FakeFs::new(cx.executor());
        fs.insert_tree("/root", json!({})).await;
        fs.insert_tree("/outside", json!({})).await;
        fs.insert_symlink("/root/link", PathBuf::from("/outside"))
            .await;

        let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
        let context_server_registry =
            cx.new(|cx| ContextServerRegistry::new(project.read(cx).context_server_store(), cx));
        let language_registry = project.read_with(cx, |project, _cx| project.languages().clone());
        let model = Arc::new(FakeLanguageModel::default());
        let thread = cx.new(|cx| {
            Thread::new(
                project.clone(),
                cx.new(|_cx| ProjectContext::default()),
                context_server_registry,
                Templates::new(),
                Some(model),
                cx,
            )
        });
        let tool = Arc::new(EditFileTool::new(
            project,
            thread.downgrade(),
            language_registry,
            Templates::new(),
        ));

        cx.update(|cx| {
            let mut settings = agent_settings::AgentSettings::get_global(cx).clone();
            settings.tool_permissions.default = settings::ToolPermissionMode::Allow;
            agent_settings::AgentSettings::override_global(settings, cx);
        });

        let (stream_tx, mut stream_rx) = ToolCallEventStream::test();
        let authorize_task = cx.update(|cx| {
            tool.authorize(
                &EditFileToolInput {
                    display_description: "create through symlink".into(),
                    path: "link/new.txt".into(),
                    mode: EditFileMode::Create,
                    content: None,
                    edits: None,
                },
                &stream_tx,
                cx,
            )
        });

        let event = stream_rx.expect_authorization().await;
        assert!(
            event
                .tool_call
                .fields
                .title
                .as_deref()
                .is_some_and(|title| title.contains("points outside the project")),
            "Expected symlink escape authorization for create under external symlink"
        );

        event
            .response
            .send(acp_thread::SelectedPermissionOutcome::new(
                acp::PermissionOptionId::new("allow"),
                acp::PermissionOptionKind::AllowOnce,
            ))
            .unwrap();
        authorize_task.await.unwrap();
    }

    #[gpui::test]
    async fn test_edit_file_symlink_escape_requests_authorization(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = project::FakeFs::new(cx.executor());
        fs.insert_tree(
            path!("/root"),
            json!({
                "src": { "main.rs": "fn main() {}" }
            }),
        )
        .await;
        fs.insert_tree(
            path!("/outside"),
            json!({
                "config.txt": "old content"
            }),
        )
        .await;
        fs.create_symlink(
            path!("/root/link_to_external").as_ref(),
            PathBuf::from("/outside"),
        )
        .await
        .unwrap();

        let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
        cx.executor().run_until_parked();

        let language_registry = project.read_with(cx, |project, _| project.languages().clone());
        let context_server_registry =
            cx.new(|cx| ContextServerRegistry::new(project.read(cx).context_server_store(), cx));
        let model = Arc::new(FakeLanguageModel::default());
        let thread = cx.new(|cx| {
            Thread::new(
                project.clone(),
                cx.new(|_cx| ProjectContext::default()),
                context_server_registry,
                Templates::new(),
                Some(model),
                cx,
            )
        });
        let tool = Arc::new(EditFileTool::new(
            project.clone(),
            thread.downgrade(),
            language_registry,
            Templates::new(),
        ));

        let (stream_tx, mut stream_rx) = ToolCallEventStream::test();
        let _authorize_task = cx.update(|cx| {
            tool.authorize(
                &EditFileToolInput {
                    display_description: "edit through symlink".into(),
                    path: PathBuf::from("link_to_external/config.txt"),
                    mode: EditFileMode::Edit,
                    content: None,
                    edits: None,
                },
                &stream_tx,
                cx,
            )
        });

        let auth = stream_rx.expect_authorization().await;
        let title = auth.tool_call.fields.title.as_deref().unwrap_or("");
        assert!(
            title.contains("points outside the project"),
            "title should mention symlink escape, got: {title}"
        );
    }

    #[gpui::test]
    async fn test_edit_file_symlink_escape_denied(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = project::FakeFs::new(cx.executor());
        fs.insert_tree(
            path!("/root"),
            json!({
                "src": { "main.rs": "fn main() {}" }
            }),
        )
        .await;
        fs.insert_tree(
            path!("/outside"),
            json!({
                "config.txt": "old content"
            }),
        )
        .await;
        fs.create_symlink(
            path!("/root/link_to_external").as_ref(),
            PathBuf::from("/outside"),
        )
        .await
        .unwrap();

        let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
        cx.executor().run_until_parked();

        let language_registry = project.read_with(cx, |project, _| project.languages().clone());
        let context_server_registry =
            cx.new(|cx| ContextServerRegistry::new(project.read(cx).context_server_store(), cx));
        let model = Arc::new(FakeLanguageModel::default());
        let thread = cx.new(|cx| {
            Thread::new(
                project.clone(),
                cx.new(|_cx| ProjectContext::default()),
                context_server_registry,
                Templates::new(),
                Some(model),
                cx,
            )
        });
        let tool = Arc::new(EditFileTool::new(
            project.clone(),
            thread.downgrade(),
            language_registry,
            Templates::new(),
        ));

        let (stream_tx, mut stream_rx) = ToolCallEventStream::test();
        let authorize_task = cx.update(|cx| {
            tool.authorize(
                &EditFileToolInput {
                    display_description: "edit through symlink".into(),
                    path: PathBuf::from("link_to_external/config.txt"),
                    mode: EditFileMode::Edit,
                    content: None,
                    edits: None,
                },
                &stream_tx,
                cx,
            )
        });

        let auth = stream_rx.expect_authorization().await;
        drop(auth); // deny by dropping

        let result = authorize_task.await;
        assert!(result.is_err(), "should fail when denied");
    }

    #[gpui::test]
    async fn test_edit_file_symlink_escape_honors_deny_policy(cx: &mut TestAppContext) {
        init_test(cx);
        cx.update(|cx| {
            let mut settings = agent_settings::AgentSettings::get_global(cx).clone();
            settings.tool_permissions.tools.insert(
                "edit_file".into(),
                agent_settings::ToolRules {
                    default: Some(settings::ToolPermissionMode::Deny),
                    ..Default::default()
                },
            );
            agent_settings::AgentSettings::override_global(settings, cx);
        });

        let fs = project::FakeFs::new(cx.executor());
        fs.insert_tree(
            path!("/root"),
            json!({
                "src": { "main.rs": "fn main() {}" }
            }),
        )
        .await;
        fs.insert_tree(
            path!("/outside"),
            json!({
                "config.txt": "old content"
            }),
        )
        .await;
        fs.create_symlink(
            path!("/root/link_to_external").as_ref(),
            PathBuf::from("/outside"),
        )
        .await
        .unwrap();

        let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
        cx.executor().run_until_parked();

        let language_registry = project.read_with(cx, |project, _| project.languages().clone());
        let context_server_registry =
            cx.new(|cx| ContextServerRegistry::new(project.read(cx).context_server_store(), cx));
        let model = Arc::new(FakeLanguageModel::default());
        let thread = cx.new(|cx| {
            Thread::new(
                project.clone(),
                cx.new(|_cx| ProjectContext::default()),
                context_server_registry,
                Templates::new(),
                Some(model),
                cx,
            )
        });
        let tool = Arc::new(EditFileTool::new(
            project.clone(),
            thread.downgrade(),
            language_registry,
            Templates::new(),
        ));

        let (stream_tx, mut stream_rx) = ToolCallEventStream::test();
        let result = cx
            .update(|cx| {
                tool.authorize(
                    &EditFileToolInput {
                        display_description: "edit through symlink".into(),
                        path: PathBuf::from("link_to_external/config.txt"),
                        mode: EditFileMode::Edit,
                        content: None,
                        edits: None,
                    },
                    &stream_tx,
                    cx,
                )
            })
            .await;

        assert!(result.is_err(), "Tool should fail when policy denies");
        assert!(
            !matches!(
                stream_rx.try_recv(),
                Ok(Ok(crate::ThreadEvent::ToolCallAuthorization(_)))
            ),
            "Deny policy should not emit symlink authorization prompt",
        );
    }

    #[gpui::test]
    async fn test_authorize_global_config(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = project::FakeFs::new(cx.executor());
        fs.insert_tree("/project", json!({})).await;
        let project = Project::test(fs.clone(), [path!("/project").as_ref()], cx).await;
        let language_registry = project.read_with(cx, |project, _cx| project.languages().clone());
        let context_server_registry =
            cx.new(|cx| ContextServerRegistry::new(project.read(cx).context_server_store(), cx));
        let model = Arc::new(FakeLanguageModel::default());
        let thread = cx.new(|cx| {
            Thread::new(
                project.clone(),
                cx.new(|_cx| ProjectContext::default()),
                context_server_registry,
                Templates::new(),
                Some(model.clone()),
                cx,
            )
        });
        let tool = Arc::new(EditFileTool::new(
            project.clone(),
            thread.downgrade(),
            language_registry,
            Templates::new(),
        ));

        // Test global config paths - these should require confirmation if they exist and are outside the project
        let test_cases = vec![
            (
                "/etc/hosts",
                true,
                "System file should require confirmation",
            ),
            (
                "/usr/local/bin/script",
                true,
                "System bin file should require confirmation",
            ),
            (
                "project/normal_file.rs",
                false,
                "Normal project file should not require confirmation",
            ),
        ];

        for (path, should_confirm, description) in test_cases {
            let (stream_tx, mut stream_rx) = ToolCallEventStream::test();
            let auth = cx.update(|cx| {
                tool.authorize(
                    &EditFileToolInput {
                        display_description: "Edit file".into(),
                        path: path.into(),
                        mode: EditFileMode::Edit,
                        content: None,
                        edits: None,
                    },
                    &stream_tx,
                    cx,
                )
            });

            if should_confirm {
                stream_rx.expect_authorization().await;
            } else {
                auth.await.unwrap();
                assert!(
                    stream_rx.try_recv().is_err(),
                    "Failed for case: {} - path: {} - expected no confirmation but got one",
                    description,
                    path
                );
            }
        }
    }

    #[gpui::test]
    async fn test_needs_confirmation_with_multiple_worktrees(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = project::FakeFs::new(cx.executor());

        // Create multiple worktree directories
        fs.insert_tree(
            "/workspace/frontend",
            json!({
                "src": {
                    "main.js": "console.log('frontend');"
                }
            }),
        )
        .await;
        fs.insert_tree(
            "/workspace/backend",
            json!({
                "src": {
                    "main.rs": "fn main() {}"
                }
            }),
        )
        .await;
        fs.insert_tree(
            "/workspace/shared",
            json!({
                ".ZZZ": {
                    "settings.json": "{}"
                }
            }),
        )
        .await;

        // Create project with multiple worktrees
        let project = Project::test(
            fs.clone(),
            [
                path!("/workspace/frontend").as_ref(),
                path!("/workspace/backend").as_ref(),
                path!("/workspace/shared").as_ref(),
            ],
            cx,
        )
        .await;
        let language_registry = project.read_with(cx, |project, _cx| project.languages().clone());
        let context_server_registry =
            cx.new(|cx| ContextServerRegistry::new(project.read(cx).context_server_store(), cx));
        let model = Arc::new(FakeLanguageModel::default());
        let thread = cx.new(|cx| {
            Thread::new(
                project.clone(),
                cx.new(|_cx| ProjectContext::default()),
                context_server_registry.clone(),
                Templates::new(),
                Some(model.clone()),
                cx,
            )
        });
        let tool = Arc::new(EditFileTool::new(
            project.clone(),
            thread.downgrade(),
            language_registry,
            Templates::new(),
        ));

        // Test files in different worktrees
        let test_cases = vec![
            ("frontend/src/main.js", false, "File in first worktree"),
            ("backend/src/main.rs", false, "File in second worktree"),
            (
                "shared/.ZZZ/settings.json",
                true,
                ".ZZZ file in third worktree",
            ),
            ("/etc/hosts", true, "Absolute path outside all worktrees"),
            (
                "../outside/file.txt",
                true,
                "Relative path outside worktrees",
            ),
        ];

        for (path, should_confirm, description) in test_cases {
            let (stream_tx, mut stream_rx) = ToolCallEventStream::test();
            let auth = cx.update(|cx| {
                tool.authorize(
                    &EditFileToolInput {
                        display_description: "Edit file".into(),
                        path: path.into(),
                        mode: EditFileMode::Edit,
                        content: None,
                        edits: None,
                    },
                    &stream_tx,
                    cx,
                )
            });

            if should_confirm {
                stream_rx.expect_authorization().await;
            } else {
                auth.await.unwrap();
                assert!(
                    stream_rx.try_recv().is_err(),
                    "Failed for case: {} - path: {} - expected no confirmation but got one",
                    description,
                    path
                );
            }
        }
    }

    #[gpui::test]
    async fn test_needs_confirmation_edge_cases(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = project::FakeFs::new(cx.executor());
        fs.insert_tree(
            "/project",
            json!({
                ".ZZZ": {
                    "settings.json": "{}"
                },
                "src": {
                    ".ZZZ": {
                        "local.json": "{}"
                    }
                }
            }),
        )
        .await;
        let project = Project::test(fs.clone(), [path!("/project").as_ref()], cx).await;
        let language_registry = project.read_with(cx, |project, _cx| project.languages().clone());
        let context_server_registry =
            cx.new(|cx| ContextServerRegistry::new(project.read(cx).context_server_store(), cx));
        let model = Arc::new(FakeLanguageModel::default());
        let thread = cx.new(|cx| {
            Thread::new(
                project.clone(),
                cx.new(|_cx| ProjectContext::default()),
                context_server_registry.clone(),
                Templates::new(),
                Some(model.clone()),
                cx,
            )
        });
        let tool = Arc::new(EditFileTool::new(
            project.clone(),
            thread.downgrade(),
            language_registry,
            Templates::new(),
        ));

        // Test edge cases
        let test_cases = vec![
            // Empty path - find_project_path returns Some for empty paths
            ("", false, "Empty path is treated as project root"),
            // Root directory
            ("/", true, "Root directory should be outside project"),
            // Parent directory references - find_project_path resolves these
            (
                "project/../other",
                true,
                "Path with .. that goes outside of root directory",
            ),
            (
                "project/./src/file.rs",
                false,
                "Path with . should work normally",
            ),
            // Windows-style paths (if on Windows)
            #[cfg(target_os = "windows")]
            ("C:\\Windows\\System32\\hosts", true, "Windows system path"),
            #[cfg(target_os = "windows")]
            ("project\\src\\main.rs", false, "Windows-style project path"),
        ];

        for (path, should_confirm, description) in test_cases {
            let (stream_tx, mut stream_rx) = ToolCallEventStream::test();
            let auth = cx.update(|cx| {
                tool.authorize(
                    &EditFileToolInput {
                        display_description: "Edit file".into(),
                        path: path.into(),
                        mode: EditFileMode::Edit,
                        content: None,
                        edits: None,
                    },
                    &stream_tx,
                    cx,
                )
            });

            cx.run_until_parked();

            if should_confirm {
                stream_rx.expect_authorization().await;
            } else {
                assert!(
                    stream_rx.try_recv().is_err(),
                    "Failed for case: {} - path: {} - expected no confirmation but got one",
                    description,
                    path
                );
                auth.await.unwrap();
            }
        }
    }

    #[gpui::test]
    async fn test_needs_confirmation_with_different_modes(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = project::FakeFs::new(cx.executor());
        fs.insert_tree(
            "/project",
            json!({
                "existing.txt": "content",
                ".ZZZ": {
                    "settings.json": "{}"
                }
            }),
        )
        .await;
        let project = Project::test(fs.clone(), [path!("/project").as_ref()], cx).await;
        let language_registry = project.read_with(cx, |project, _cx| project.languages().clone());
        let context_server_registry =
            cx.new(|cx| ContextServerRegistry::new(project.read(cx).context_server_store(), cx));
        let model = Arc::new(FakeLanguageModel::default());
        let thread = cx.new(|cx| {
            Thread::new(
                project.clone(),
                cx.new(|_cx| ProjectContext::default()),
                context_server_registry.clone(),
                Templates::new(),
                Some(model.clone()),
                cx,
            )
        });
        let tool = Arc::new(EditFileTool::new(
            project.clone(),
            thread.downgrade(),
            language_registry,
            Templates::new(),
        ));

        // Test different EditFileMode values
        let modes = vec![
            EditFileMode::Edit,
            EditFileMode::Create,
            EditFileMode::Overwrite,
        ];

        for mode in modes {
            // Test .ZZZ path with different modes
            let (stream_tx, mut stream_rx) = ToolCallEventStream::test();
            let _auth = cx.update(|cx| {
                tool.authorize(
                    &EditFileToolInput {
                        display_description: "Edit settings".into(),
                        path: "project/.ZZZ/settings.json".into(),
                        mode: mode.clone(),
                        content: None,
                        edits: None,
                    },
                    &stream_tx,
                    cx,
                )
            });

            stream_rx.expect_authorization().await;

            // Test outside path with different modes
            let (stream_tx, mut stream_rx) = ToolCallEventStream::test();
            let _auth = cx.update(|cx| {
                tool.authorize(
                    &EditFileToolInput {
                        display_description: "Edit file".into(),
                        path: "/outside/file.txt".into(),
                        mode: mode.clone(),
                        content: None,
                        edits: None,
                    },
                    &stream_tx,
                    cx,
                )
            });

            stream_rx.expect_authorization().await;

            // Test normal path with different modes
            let (stream_tx, mut stream_rx) = ToolCallEventStream::test();
            cx.update(|cx| {
                tool.authorize(
                    &EditFileToolInput {
                        display_description: "Edit file".into(),
                        path: "project/normal.txt".into(),
                        mode: mode.clone(),
                        content: None,
                        edits: None,
                    },
                    &stream_tx,
                    cx,
                )
            })
            .await
            .unwrap();
            assert!(stream_rx.try_recv().is_err());
        }
    }

    #[gpui::test]
    async fn test_initial_title_with_partial_input(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = project::FakeFs::new(cx.executor());
        let project = Project::test(fs.clone(), [path!("/project").as_ref()], cx).await;
        let language_registry = project.read_with(cx, |project, _cx| project.languages().clone());
        let context_server_registry =
            cx.new(|cx| ContextServerRegistry::new(project.read(cx).context_server_store(), cx));
        let model = Arc::new(FakeLanguageModel::default());
        let thread = cx.new(|cx| {
            Thread::new(
                project.clone(),
                cx.new(|_cx| ProjectContext::default()),
                context_server_registry,
                Templates::new(),
                Some(model.clone()),
                cx,
            )
        });
        let tool = Arc::new(EditFileTool::new(
            project,
            thread.downgrade(),
            language_registry,
            Templates::new(),
        ));

        cx.update(|cx| {
            // ...
            assert_eq!(
                tool.initial_title(
                    Err(json!({
                        "path": "src/main.rs",
                        "display_description": "",
                        "old_string": "old code",
                        "new_string": "new code"
                    })),
                    cx
                ),
                "src/main.rs"
            );
            assert_eq!(
                tool.initial_title(
                    Err(json!({
                        "path": "",
                        "display_description": "Fix error handling",
                        "old_string": "old code",
                        "new_string": "new code"
                    })),
                    cx
                ),
                "Fix error handling"
            );
            assert_eq!(
                tool.initial_title(
                    Err(json!({
                        "path": "src/main.rs",
                        "display_description": "Fix error handling",
                        "old_string": "old code",
                        "new_string": "new code"
                    })),
                    cx
                ),
                "src/main.rs"
            );
            assert_eq!(
                tool.initial_title(
                    Err(json!({
                        "path": "",
                        "display_description": "",
                        "old_string": "old code",
                        "new_string": "new code"
                    })),
                    cx
                ),
                DEFAULT_UI_TEXT
            );
            assert_eq!(
                tool.initial_title(Err(serde_json::Value::Null), cx),
                DEFAULT_UI_TEXT
            );
        });
    }

    #[gpui::test]
    async fn test_diff_finalization(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = project::FakeFs::new(cx.executor());
        fs.insert_tree("/", json!({"main.rs": ""})).await;

        let project = Project::test(fs.clone(), [path!("/").as_ref()], cx).await;
        let languages = project.read_with(cx, |project, _cx| project.languages().clone());
        let context_server_registry =
            cx.new(|cx| ContextServerRegistry::new(project.read(cx).context_server_store(), cx));
        let model = Arc::new(FakeLanguageModel::default());
        let thread = cx.new(|cx| {
            Thread::new(
                project.clone(),
                cx.new(|_cx| ProjectContext::default()),
                context_server_registry.clone(),
                Templates::new(),
                Some(model.clone()),
                cx,
            )
        });

        // Ensure the diff is finalized after the edit completes.
        {
            let tool = Arc::new(EditFileTool::new(
                project.clone(),
                thread.downgrade(),
                languages.clone(),
                Templates::new(),
            ));
            let (stream_tx, mut stream_rx) = ToolCallEventStream::test();
            let edit = cx.update(|cx| {
                tool.run(
                    ToolInput::resolved(EditFileToolInput {
                        display_description: "Edit file".into(),
                        path: path!("/main.rs").into(),
                        mode: EditFileMode::Edit,
                        content: None,
                        edits: None,
                    }),
                    stream_tx,
                    cx,
                )
            });
            stream_rx.expect_update_fields().await;
            let diff = stream_rx.expect_diff().await;
            diff.read_with(cx, |diff, _| assert!(matches!(diff, Diff::Pending(_))));
            cx.run_until_parked();
            model.end_last_completion_stream();
            edit.await.unwrap();
            diff.read_with(cx, |diff, _| assert!(matches!(diff, Diff::Finalized(_))));
        }

        // Ensure the diff is finalized if an error occurs while editing.
        {
            model.forbid_requests();
            let tool = Arc::new(EditFileTool::new(
                project.clone(),
                thread.downgrade(),
                languages.clone(),
                Templates::new(),
            ));
            let (stream_tx, mut stream_rx) = ToolCallEventStream::test();
            let edit = cx.update(|cx| {
                tool.run(
                    ToolInput::resolved(EditFileToolInput {
                        display_description: "Edit file".into(),
                        path: path!("/main.rs").into(),
                        mode: EditFileMode::Edit,
                        content: None,
                        edits: None,
                    }),
                    stream_tx,
                    cx,
                )
            });
            stream_rx.expect_update_fields().await;
            let diff = stream_rx.expect_diff().await;
            diff.read_with(cx, |diff, _| assert!(matches!(diff, Diff::Pending(_))));
            edit.await.unwrap_err();
            diff.read_with(cx, |diff, _| assert!(matches!(diff, Diff::Finalized(_))));
            model.allow_requests();
        }

        // Ensure the diff is finalized if the tool call gets dropped.
        {
            let tool = Arc::new(EditFileTool::new(
                project.clone(),
                thread.downgrade(),
                languages.clone(),
                Templates::new(),
            ));
            let (stream_tx, mut stream_rx) = ToolCallEventStream::test();
            let edit = cx.update(|cx| {
                tool.run(
                    ToolInput::resolved(EditFileToolInput {
                        display_description: "Edit file".into(),
                        path: path!("/main.rs").into(),
                        mode: EditFileMode::Edit,
                        content: None,
                        edits: None,
                    }),
                    stream_tx,
                    cx,
                )
            });
            stream_rx.expect_update_fields().await;
            let diff = stream_rx.expect_diff().await;
            diff.read_with(cx, |diff, _| assert!(matches!(diff, Diff::Pending(_))));
            drop(edit);
            cx.run_until_parked();
            diff.read_with(cx, |diff, _| assert!(matches!(diff, Diff::Finalized(_))));
        }
    }

    #[gpui::test]
    async fn test_file_read_times_tracking(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = project::FakeFs::new(cx.executor());
        fs.insert_tree(
            "/root",
            json!({
                "test.txt": "original content"
            }),
        )
        .await;
        let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
        let context_server_registry =
            cx.new(|cx| ContextServerRegistry::new(project.read(cx).context_server_store(), cx));
        let model = Arc::new(FakeLanguageModel::default());
        let thread = cx.new(|cx| {
            Thread::new(
                project.clone(),
                cx.new(|_cx| ProjectContext::default()),
                context_server_registry,
                Templates::new(),
                Some(model.clone()),
                cx,
            )
        });
        let action_log = thread.read_with(cx, |thread, _| thread.action_log().clone());

        // Initially, file_read_times should be empty
        let is_empty = action_log.read_with(cx, |action_log, _| {
            action_log
                .file_read_time(path!("/root/test.txt").as_ref())
                .is_none()
        });
        assert!(is_empty, "file_read_times should start empty");

        // Create read tool
        let read_tool = Arc::new(crate::ReadFileTool::new(
            project.clone(),
            action_log.clone(),
            true,
        ));

        // Read the file to record the read time
        cx.update(|cx| {
            read_tool.clone().run(
                ToolInput::resolved(crate::ReadFileToolInput {
                    path: "root/test.txt".to_string(),
                    start_line: None,
                    end_line: None,
                }),
                ToolCallEventStream::test().0,
                cx,
            )
        })
        .await
        .unwrap();

        // Verify that file_read_times now contains an entry for the file
        let has_entry = action_log.read_with(cx, |log, _| {
            log.file_read_time(path!("/root/test.txt").as_ref())
                .is_some()
        });
        assert!(
            has_entry,
            "file_read_times should contain an entry after reading the file"
        );

        // Read the file again - should update the entry
        cx.update(|cx| {
            read_tool.clone().run(
                ToolInput::resolved(crate::ReadFileToolInput {
                    path: "root/test.txt".to_string(),
                    start_line: None,
                    end_line: None,
                }),
                ToolCallEventStream::test().0,
                cx,
            )
        })
        .await
        .unwrap();

        // Should still have an entry after re-reading
        let has_entry = action_log.read_with(cx, |log, _| {
            log.file_read_time(path!("/root/test.txt").as_ref())
                .is_some()
        });
        assert!(
            has_entry,
            "file_read_times should still have an entry after re-reading"
        );
    }

    fn init_test(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let settings_store = SettingsStore::test(cx);
            cx.set_global(settings_store);
        });
    }

    #[gpui::test]
    async fn test_consecutive_edits_work(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = project::FakeFs::new(cx.executor());
        fs.insert_tree(
            "/root",
            json!({
                "test.txt": "original content"
            }),
        )
        .await;
        let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
        let context_server_registry =
            cx.new(|cx| ContextServerRegistry::new(project.read(cx).context_server_store(), cx));
        let model = Arc::new(FakeLanguageModel::default());
        let thread = cx.new(|cx| {
            Thread::new(
                project.clone(),
                cx.new(|_cx| ProjectContext::default()),
                context_server_registry,
                Templates::new(),
                Some(model.clone()),
                cx,
            )
        });
        let languages = project.read_with(cx, |project, _| project.languages().clone());
        let action_log = thread.read_with(cx, |thread, _| thread.action_log().clone());

        let read_tool = Arc::new(crate::ReadFileTool::new(project.clone(), action_log, true));
        let edit_tool = Arc::new(EditFileTool::new(
            project.clone(),
            thread.downgrade(),
            languages,
            Templates::new(),
        ));

        // Read the file first
        cx.update(|cx| {
            read_tool.clone().run(
                ToolInput::resolved(crate::ReadFileToolInput {
                    path: "root/test.txt".to_string(),
                    start_line: None,
                    end_line: None,
                }),
                ToolCallEventStream::test().0,
                cx,
            )
        })
        .await
        .unwrap();

        // First edit should work
        let edit_result = {
            let edit_task = cx.update(|cx| {
                edit_tool.clone().run(
                    ToolInput::resolved(EditFileToolInput {
                        display_description: "First edit".into(),
                        path: "root/test.txt".into(),
                        mode: EditFileMode::Edit,
                        content: None,
                        edits: None,
                    }),
                    ToolCallEventStream::test().0,
                    cx,
                )
            });

            cx.executor().run_until_parked();
            model.send_last_completion_stream_text_chunk(
                "<old_text>original content</old_text><new_text>modified content</new_text>"
                    .to_string(),
            );
            model.end_last_completion_stream();

            edit_task.await
        };
        assert!(
            edit_result.is_ok(),
            "First edit should succeed, got error: {:?}",
            edit_result.as_ref().err()
        );

        // Second edit should also work because the edit updated the recorded read time
        let edit_result = {
            let edit_task = cx.update(|cx| {
                edit_tool.clone().run(
                    ToolInput::resolved(EditFileToolInput {
                        display_description: "Second edit".into(),
                        path: "root/test.txt".into(),
                        mode: EditFileMode::Edit,
                        content: None,
                        edits: None,
                    }),
                    ToolCallEventStream::test().0,
                    cx,
                )
            });

            cx.executor().run_until_parked();
            model.send_last_completion_stream_text_chunk(
                "<old_text>modified content</old_text><new_text>further modified content</new_text>".to_string(),
            );
            model.end_last_completion_stream();

            edit_task.await
        };
        assert!(
            edit_result.is_ok(),
            "Second consecutive edit should succeed, got error: {:?}",
            edit_result.as_ref().err()
        );
    }

    #[gpui::test]
    async fn test_external_modification_detected(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = project::FakeFs::new(cx.executor());
        fs.insert_tree(
            "/root",
            json!({
                "test.txt": "original content"
            }),
        )
        .await;
        let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
        let context_server_registry =
            cx.new(|cx| ContextServerRegistry::new(project.read(cx).context_server_store(), cx));
        let model = Arc::new(FakeLanguageModel::default());
        let thread = cx.new(|cx| {
            Thread::new(
                project.clone(),
                cx.new(|_cx| ProjectContext::default()),
                context_server_registry,
                Templates::new(),
                Some(model.clone()),
                cx,
            )
        });
        let languages = project.read_with(cx, |project, _| project.languages().clone());
        let action_log = thread.read_with(cx, |thread, _| thread.action_log().clone());

        let read_tool = Arc::new(crate::ReadFileTool::new(project.clone(), action_log, true));
        let edit_tool = Arc::new(EditFileTool::new(
            project.clone(),
            thread.downgrade(),
            languages,
            Templates::new(),
        ));

        // Read the file first
        cx.update(|cx| {
            read_tool.clone().run(
                ToolInput::resolved(crate::ReadFileToolInput {
                    path: "root/test.txt".to_string(),
                    start_line: None,
                    end_line: None,
                }),
                ToolCallEventStream::test().0,
                cx,
            )
        })
        .await
        .unwrap();

        // Simulate external modification - advance time and save file
        cx.background_executor
            .advance_clock(std::time::Duration::from_secs(2));
        fs.save(
            path!("/root/test.txt").as_ref(),
            &"externally modified content".into(),
            language::LineEnding::Unix,
        )
        .await
        .unwrap();

        // Reload the buffer to pick up the new mtime
        let project_path = project
            .read_with(cx, |project, cx| {
                project.find_project_path("root/test.txt", cx)
            })
            .expect("Should find project path");
        let buffer = project
            .update(cx, |project, cx| project.open_buffer(project_path, cx))
            .await
            .unwrap();
        buffer
            .update(cx, |buffer, cx| buffer.reload(cx))
            .await
            .unwrap();

        cx.executor().run_until_parked();

        // Try to edit - should fail because file was modified externally
        let result = cx
            .update(|cx| {
                edit_tool.clone().run(
                    ToolInput::resolved(EditFileToolInput {
                        display_description: "Edit after external change".into(),
                        path: "root/test.txt".into(),
                        mode: EditFileMode::Edit,
                        content: None,
                        edits: None,
                    }),
                    ToolCallEventStream::test().0,
                    cx,
                )
            })
            .await;

        assert!(
            result.is_err(),
            "Edit should fail after external modification"
        );
        let error_msg = result.unwrap_err().to_string();
        assert!(
            error_msg.contains("has been modified since you last read it"),
            "Error should mention file modification, got: {}",
            error_msg
        );
    }

    #[gpui::test]
    async fn test_dirty_buffer_detected(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = project::FakeFs::new(cx.executor());
        fs.insert_tree(
            "/root",
            json!({
                "test.txt": "original content"
            }),
        )
        .await;
        let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
        let context_server_registry =
            cx.new(|cx| ContextServerRegistry::new(project.read(cx).context_server_store(), cx));
        let model = Arc::new(FakeLanguageModel::default());
        let thread = cx.new(|cx| {
            Thread::new(
                project.clone(),
                cx.new(|_cx| ProjectContext::default()),
                context_server_registry,
                Templates::new(),
                Some(model.clone()),
                cx,
            )
        });
        let languages = project.read_with(cx, |project, _| project.languages().clone());
        let action_log = thread.read_with(cx, |thread, _| thread.action_log().clone());

        let read_tool = Arc::new(crate::ReadFileTool::new(project.clone(), action_log, true));
        let edit_tool = Arc::new(EditFileTool::new(
            project.clone(),
            thread.downgrade(),
            languages,
            Templates::new(),
        ));

        // Read the file first
        cx.update(|cx| {
            read_tool.clone().run(
                ToolInput::resolved(crate::ReadFileToolInput {
                    path: "root/test.txt".to_string(),
                    start_line: None,
                    end_line: None,
                }),
                ToolCallEventStream::test().0,
                cx,
            )
        })
        .await
        .unwrap();

        // Open the buffer and make it dirty by editing without saving
        let project_path = project
            .read_with(cx, |project, cx| {
                project.find_project_path("root/test.txt", cx)
            })
            .expect("Should find project path");
        let buffer = project
            .update(cx, |project, cx| project.open_buffer(project_path, cx))
            .await
            .unwrap();

        // Make an in-memory edit to the buffer (making it dirty)
        buffer.update(cx, |buffer, cx| {
            let end_point = buffer.max_point();
            buffer.edit([(end_point..end_point, " added text")], None, cx);
        });

        // Verify buffer is dirty
        let is_dirty = buffer.read_with(cx, |buffer, _| buffer.is_dirty());
        assert!(is_dirty, "Buffer should be dirty after in-memory edit");

        // Try to edit - should fail because buffer has unsaved changes
        let result = cx
            .update(|cx| {
                edit_tool.clone().run(
                    ToolInput::resolved(EditFileToolInput {
                        display_description: "Edit with dirty buffer".into(),
                        path: "root/test.txt".into(),
                        mode: EditFileMode::Edit,
                        content: None,
                        edits: None,
                    }),
                    ToolCallEventStream::test().0,
                    cx,
                )
            })
            .await;

        assert!(result.is_err(), "Edit should fail when buffer is dirty");
        let error_msg = result.unwrap_err().to_string();
        assert!(
            error_msg.contains("This file has unsaved changes."),
            "Error should mention unsaved changes, got: {}",
            error_msg
        );
        assert!(
            error_msg.contains("keep or discard"),
            "Error should ask whether to keep or discard changes, got: {}",
            error_msg
        );
        // Since save_file and restore_file_from_disk tools aren't added to the thread,
        // the error message should ask the user to manually save or revert
        assert!(
            error_msg.contains("save or revert the file manually"),
            "Error should ask user to manually save or revert when tools aren't available, got: {}",
            error_msg
        );
    }

    #[gpui::test]
    async fn test_sensitive_settings_kind_detects_nonexistent_subdirectory(
        cx: &mut TestAppContext,
    ) {
        let fs = project::FakeFs::new(cx.executor());
        let config_dir = paths::config_dir();
        fs.insert_tree(&*config_dir.to_string_lossy(), json!({}))
            .await;
        let path = config_dir.join("nonexistent_subdir_xyz").join("evil.json");
        assert!(
            matches!(
                sensitive_settings_kind(&path, fs.as_ref()).await,
                Some(SensitiveSettingsKind::Global)
            ),
            "Path in non-existent subdirectory of config dir should be detected as sensitive: {:?}",
            path
        );
    }

    #[gpui::test]
    async fn test_sensitive_settings_kind_detects_deeply_nested_nonexistent_subdirectory(
        cx: &mut TestAppContext,
    ) {
        let fs = project::FakeFs::new(cx.executor());
        let config_dir = paths::config_dir();
        fs.insert_tree(&*config_dir.to_string_lossy(), json!({}))
            .await;
        let path = config_dir.join("a").join("b").join("c").join("evil.json");
        assert!(
            matches!(
                sensitive_settings_kind(&path, fs.as_ref()).await,
                Some(SensitiveSettingsKind::Global)
            ),
            "Path in deeply nested non-existent subdirectory of config dir should be detected as sensitive: {:?}",
            path
        );
    }

    #[gpui::test]
    async fn test_sensitive_settings_kind_returns_none_for_non_config_path(
        cx: &mut TestAppContext,
    ) {
        let fs = project::FakeFs::new(cx.executor());
        let path = PathBuf::from("/tmp/not_a_config_dir/some_file.json");
        assert!(
            sensitive_settings_kind(&path, fs.as_ref()).await.is_none(),
            "Path outside config dir should not be detected as sensitive: {:?}",
            path
        );
    }

    #[gpui::test]
    async fn test_write_mode_uses_content_without_model(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = project::FakeFs::new(cx.executor());
        fs.insert_tree("/root", json!({})).await;
        let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
        let language_registry = project.read_with(cx, |project, _cx| project.languages().clone());
        let context_server_registry =
            cx.new(|cx| ContextServerRegistry::new(project.read(cx).context_server_store(), cx));
        let thread = cx.new(|cx| {
            Thread::new(
                project.clone(),
                cx.new(|_cx| ProjectContext::default()),
                context_server_registry,
                Templates::new(),
                None,
                cx,
            )
        });

        let result = cx
            .update(|cx| {
                Arc::new(EditFileTool::new(
                    project.clone(),
                    thread.downgrade(),
                    language_registry,
                    Templates::new(),
                ))
                .run(
                    ToolInput::resolved(EditFileToolInput {
                        display_description: "Write file".into(),
                        path: "root/file.txt".into(),
                        mode: EditFileMode::Overwrite,
                        content: Some("hello\nworld\n".into()),
                        edits: None,
                    }),
                    ToolCallEventStream::test().0,
                    cx,
                )
            })
            .await;

        assert!(result.is_ok());
        assert_eq!(
            fs.load(path!("/root/file.txt").as_ref()).await.unwrap(),
            "hello\nworld\n"
        );
    }

    #[gpui::test]
    async fn test_edit_mode_uses_edits_without_model(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = project::FakeFs::new(cx.executor());
        fs.insert_tree("/root", json!({"file.txt": "alpha\nbeta\n"}))
            .await;
        let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
        let language_registry = project.read_with(cx, |project, _cx| project.languages().clone());
        let context_server_registry =
            cx.new(|cx| ContextServerRegistry::new(project.read(cx).context_server_store(), cx));
        let thread = cx.new(|cx| {
            Thread::new(
                project.clone(),
                cx.new(|_cx| ProjectContext::default()),
                context_server_registry,
                Templates::new(),
                None,
                cx,
            )
        });

        let result = cx
            .update(|cx| {
                Arc::new(EditFileTool::new(
                    project.clone(),
                    thread.downgrade(),
                    language_registry,
                    Templates::new(),
                ))
                .run(
                    ToolInput::resolved(EditFileToolInput {
                        display_description: "Edit file".into(),
                        path: "root/file.txt".into(),
                        mode: EditFileMode::Edit,
                        content: None,
                        edits: Some(vec![EditOperation {
                            old_text: "beta".into(),
                            new_text: "gamma".into(),
                        }]),
                    }),
                    ToolCallEventStream::test().0,
                    cx,
                )
            })
            .await;

        assert!(result.is_ok());
        assert_eq!(
            fs.load(path!("/root/file.txt").as_ref()).await.unwrap(),
            "alpha\ngamma\n"
        );
    }

    #[gpui::test]
    async fn test_edit_mode_with_ambiguous_old_text_fails(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = project::FakeFs::new(cx.executor());
        fs.insert_tree("/root", json!({"file.txt": "dup\nkeep\ndup\n"}))
            .await;
        let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
        let language_registry = project.read_with(cx, |project, _cx| project.languages().clone());
        let context_server_registry =
            cx.new(|cx| ContextServerRegistry::new(project.read(cx).context_server_store(), cx));
        let thread = cx.new(|cx| {
            Thread::new(
                project.clone(),
                cx.new(|_cx| ProjectContext::default()),
                context_server_registry,
                Templates::new(),
                None,
                cx,
            )
        });

        let result = cx
            .update(|cx| {
                Arc::new(EditFileTool::new(
                    project.clone(),
                    thread.downgrade(),
                    language_registry,
                    Templates::new(),
                ))
                .run(
                    ToolInput::resolved(EditFileToolInput {
                        display_description: "Edit file".into(),
                        path: "root/file.txt".into(),
                        mode: EditFileMode::Edit,
                        content: None,
                        edits: Some(vec![EditOperation {
                            old_text: "dup".into(),
                            new_text: "changed".into(),
                        }]),
                    }),
                    ToolCallEventStream::test().0,
                    cx,
                )
            })
            .await;

        let error = result.unwrap_err().to_string();
        assert!(error.contains("matched multiple locations"), "{error}");
    }

    #[gpui::test]
    async fn test_streaming_create_content_streamed(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = project::FakeFs::new(cx.executor());
        fs.insert_tree("/root", json!({"dir": {}})).await;
        let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
        let language_registry = project.read_with(cx, |project, _cx| project.languages().clone());
        let context_server_registry =
            cx.new(|cx| ContextServerRegistry::new(project.read(cx).context_server_store(), cx));
        let thread = cx.new(|cx| {
            Thread::new(
                project.clone(),
                cx.new(|_cx| ProjectContext::default()),
                context_server_registry,
                Templates::new(),
                None,
                cx,
            )
        });
        let tool = Arc::new(EditFileTool::new(
            project.clone(),
            thread.downgrade(),
            language_registry,
            Templates::new(),
        ));

        let (mut sender, input) = ToolInput::<EditFileToolInput>::test();
        let (event_stream, _receiver) = ToolCallEventStream::test();
        let task = cx.update(|cx| tool.clone().run(input, event_stream, cx));

        sender.send_partial(json!({
            "display_description": "Create new file",
            "path": "root/dir/new_file.txt",
            "mode": "write"
        }));
        cx.run_until_parked();

        sender.send_partial(json!({
            "display_description": "Create new file",
            "path": "root/dir/new_file.txt",
            "mode": "write",
            "content": "line 1\n"
        }));
        cx.run_until_parked();

        let buffer = project.update(cx, |project, cx| {
            let path = project
                .find_project_path("root/dir/new_file.txt", cx)
                .unwrap();
            project.get_open_buffer(&path, cx).unwrap()
        });
        assert_eq!(buffer.read_with(cx, |buffer, _| buffer.text()), "line 1\n");

        sender.send_partial(json!({
            "display_description": "Create new file",
            "path": "root/dir/new_file.txt",
            "mode": "write",
            "content": "line 1\nline 2\n"
        }));
        cx.run_until_parked();
        assert_eq!(
            buffer.read_with(cx, |buffer, _| buffer.text()),
            "line 1\nline 2\n"
        );

        sender.send_full(json!({
            "display_description": "Create new file",
            "path": "root/dir/new_file.txt",
            "mode": "write",
            "content": "line 1\nline 2\nline 3\n"
        }));

        let result = task.await;
        let EditFileToolOutput::Success { new_text, .. } = result.unwrap() else {
            panic!("expected success");
        };
        assert_eq!(new_text, "line 1\nline 2\nline 3\n");
    }

    #[gpui::test]
    async fn test_streaming_edit_with_multiple_partials(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = project::FakeFs::new(cx.executor());
        fs.insert_tree(
            "/root",
            json!({"file.txt": "line 1\nline 2\nline 3\nline 4\nline 5\n"}),
        )
        .await;
        let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
        let language_registry = project.read_with(cx, |project, _cx| project.languages().clone());
        let context_server_registry =
            cx.new(|cx| ContextServerRegistry::new(project.read(cx).context_server_store(), cx));
        let thread = cx.new(|cx| {
            Thread::new(
                project.clone(),
                cx.new(|_cx| ProjectContext::default()),
                context_server_registry,
                Templates::new(),
                None,
                cx,
            )
        });
        let tool = Arc::new(EditFileTool::new(
            project.clone(),
            thread.downgrade(),
            language_registry,
            Templates::new(),
        ));

        let (mut sender, input) = ToolInput::<EditFileToolInput>::test();
        let (event_stream, _receiver) = ToolCallEventStream::test();
        let task = cx.update(|cx| tool.clone().run(input, event_stream, cx));

        sender.send_partial(json!({"display_description": "Edit multiple"}));
        cx.run_until_parked();
        sender.send_partial(json!({
            "display_description": "Edit multiple lines",
            "path": "root/file.txt"
        }));
        cx.run_until_parked();
        sender.send_partial(json!({
            "display_description": "Edit multiple lines",
            "path": "root/file.txt",
            "mode": "edit"
        }));
        cx.run_until_parked();
        sender.send_partial(json!({
            "display_description": "Edit multiple lines",
            "path": "root/file.txt",
            "mode": "edit",
            "edits": [{"old_text": "line 1"}]
        }));
        cx.run_until_parked();
        sender.send_partial(json!({
            "display_description": "Edit multiple lines",
            "path": "root/file.txt",
            "mode": "edit",
            "edits": [
                {"old_text": "line 1", "new_text": "modified line 1"},
                {"old_text": "line 5"}
            ]
        }));
        cx.run_until_parked();

        sender.send_full(json!({
            "display_description": "Edit multiple lines",
            "path": "root/file.txt",
            "mode": "edit",
            "edits": [
                {"old_text": "line 1", "new_text": "modified line 1"},
                {"old_text": "line 5", "new_text": "modified line 5"}
            ]
        }));

        let result = task.await;
        let EditFileToolOutput::Success { new_text, .. } = result.unwrap() else {
            panic!("expected success");
        };
        assert_eq!(
            new_text,
            "modified line 1\nline 2\nline 3\nline 4\nmodified line 5\n"
        );
    }
}
