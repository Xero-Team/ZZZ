use acp_thread::{MentionUri, UserMessageId};
use agent_client_protocol::schema::v1 as acp;
use collections::IndexMap;
use language_model::{
    LanguageModelImage, LanguageModelRequestMessage, LanguageModelToolResult,
    LanguageModelToolResultContent, LanguageModelToolUse, LanguageModelToolUseId, Role,
};
use serde::{Deserialize, Serialize};
use std::{fmt::Write, ops::RangeInclusive, path::Path};
use util::{debug_panic, markdown::MarkdownCodeBlock, paths::PathStyle};

/// Context passed to a subagent thread for lifecycle management
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SubagentContext {
    /// ID of the parent thread
    pub parent_thread_id: acp::SessionId,

    /// Current depth level (0 = root agent, 1 = first-level subagent, etc.)
    pub depth: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Message {
    User(UserMessage),
    Agent(AgentMessage),
    Resume,
}

impl Message {
    pub fn as_agent_message(&self) -> Option<&AgentMessage> {
        match self {
            Message::Agent(agent_message) => Some(agent_message),
            _ => None,
        }
    }

    pub fn to_request(&self) -> Vec<LanguageModelRequestMessage> {
        match self {
            Message::User(message) => {
                if message.content.is_empty() {
                    vec![]
                } else {
                    vec![message.to_request()]
                }
            }
            Message::Agent(message) => message.to_request(),
            Message::Resume => vec![LanguageModelRequestMessage {
                role: Role::User,
                content: vec!["Continue where you left off".into()],
                cache: false,
                reasoning_details: None,
            }],
        }
    }

    pub fn to_markdown(&self) -> String {
        match self {
            Message::User(message) => message.to_markdown(),
            Message::Agent(message) => message.to_markdown(),
            Message::Resume => "[resume]\n".into(),
        }
    }

    pub fn role(&self) -> Role {
        match self {
            Message::User(_) | Message::Resume => Role::User,
            Message::Agent(_) => Role::Assistant,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserMessage {
    pub id: UserMessageId,
    pub content: Vec<UserMessageContent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UserMessageContent {
    Text(String),
    Mention { uri: MentionUri, content: String },
    Image(LanguageModelImage),
}

impl UserMessage {
    pub fn to_markdown(&self) -> String {
        let mut markdown = String::new();

        for content in &self.content {
            match content {
                UserMessageContent::Text(text) => {
                    markdown.push_str(text);
                    markdown.push('\n');
                }
                UserMessageContent::Image(_) => {
                    markdown.push_str("<image />\n");
                }
                UserMessageContent::Mention { uri, content } => {
                    if !content.is_empty() {
                        let _ = writeln!(&mut markdown, "{}\n\n{}", uri.as_link(), content);
                    } else {
                        let _ = writeln!(&mut markdown, "{}", uri.as_link());
                    }
                }
            }
        }

        markdown
    }

    fn to_request(&self) -> LanguageModelRequestMessage {
        let mut message = LanguageModelRequestMessage {
            role: Role::User,
            content: Vec::with_capacity(self.content.len()),
            cache: false,
            reasoning_details: None,
        };

        const OPEN_CONTEXT: &str = "<context>\n\
            The following items were attached by the user. \
            They are up-to-date and don't need to be re-read.\n\n";

        const OPEN_FILES_TAG: &str = "<files>";
        const OPEN_DIRECTORIES_TAG: &str = "<directories>";
        const OPEN_SYMBOLS_TAG: &str = "<symbols>";
        const OPEN_SELECTIONS_TAG: &str = "<selections>";
        const OPEN_THREADS_TAG: &str = "<threads>";
        const OPEN_FETCH_TAG: &str = "<fetched_urls>";
        const OPEN_RULES_TAG: &str =
            "<rules>\nThe user has specified the following rules that should be applied:\n";
        const OPEN_DIAGNOSTICS_TAG: &str = "<diagnostics>";
        const OPEN_DIFFS_TAG: &str = "<diffs>";
        const MERGE_CONFLICT_TAG: &str = "<merge_conflicts>";

        let mut file_context = OPEN_FILES_TAG.to_owned();
        let mut directory_context = OPEN_DIRECTORIES_TAG.to_owned();
        let mut symbol_context = OPEN_SYMBOLS_TAG.to_owned();
        let mut selection_context = OPEN_SELECTIONS_TAG.to_owned();
        let mut thread_context = OPEN_THREADS_TAG.to_owned();
        let mut fetch_context = OPEN_FETCH_TAG.to_owned();
        let mut rules_context = OPEN_RULES_TAG.to_owned();
        let mut diagnostics_context = OPEN_DIAGNOSTICS_TAG.to_owned();
        let mut diffs_context = OPEN_DIFFS_TAG.to_owned();
        let mut merge_conflict_context = MERGE_CONFLICT_TAG.to_owned();

        for chunk in &self.content {
            let chunk = match chunk {
                UserMessageContent::Text(text) => {
                    language_model::MessageContent::Text(text.clone())
                }
                UserMessageContent::Image(value) => {
                    language_model::MessageContent::Image(value.clone())
                }
                UserMessageContent::Mention { uri, content } => {
                    match uri {
                        MentionUri::File { abs_path } => {
                            write!(
                                &mut file_context,
                                "\n{}",
                                MarkdownCodeBlock {
                                    tag: &codeblock_tag(abs_path, None),
                                    text: &content.to_string(),
                                }
                            )
                            .ok();
                        }
                        MentionUri::PastedImage { .. } => {
                            debug_panic!("pasted image URI should not be used in mention content")
                        }
                        MentionUri::Directory { .. } => {
                            write!(&mut directory_context, "\n{}\n", content).ok();
                        }
                        MentionUri::Symbol {
                            abs_path: path,
                            line_range,
                            ..
                        } => {
                            write!(
                                &mut symbol_context,
                                "\n{}",
                                MarkdownCodeBlock {
                                    tag: &codeblock_tag(path, Some(line_range)),
                                    text: content
                                }
                            )
                            .ok();
                        }
                        MentionUri::Selection {
                            abs_path: path,
                            line_range,
                            ..
                        } => {
                            write!(
                                &mut selection_context,
                                "\n{}",
                                MarkdownCodeBlock {
                                    tag: &codeblock_tag(
                                        path.as_deref().unwrap_or("Untitled".as_ref()),
                                        Some(line_range)
                                    ),
                                    text: content
                                }
                            )
                            .ok();
                        }
                        MentionUri::Thread { .. } => {
                            write!(&mut thread_context, "\n{}\n", content).ok();
                        }
                        MentionUri::Rule { .. } => {
                            write!(
                                &mut rules_context,
                                "\n{}",
                                MarkdownCodeBlock {
                                    tag: "",
                                    text: content
                                }
                            )
                            .ok();
                        }
                        MentionUri::Fetch { url } => {
                            write!(&mut fetch_context, "\nFetch: {}\n\n{}", url, content).ok();
                        }
                        MentionUri::Diagnostics { .. } => {
                            write!(&mut diagnostics_context, "\n{}\n", content).ok();
                        }
                        MentionUri::TerminalSelection { .. } => {
                            write!(
                                &mut selection_context,
                                "\n{}",
                                MarkdownCodeBlock {
                                    tag: "console",
                                    text: content
                                }
                            )
                            .ok();
                        }
                        MentionUri::GitDiff { base_ref } => {
                            write!(
                                &mut diffs_context,
                                "\nBranch diff against {}:\n{}",
                                base_ref,
                                MarkdownCodeBlock {
                                    tag: "diff",
                                    text: content
                                }
                            )
                            .ok();
                        }
                        MentionUri::MergeConflict { file_path } => {
                            write!(
                                &mut merge_conflict_context,
                                "\nMerge conflict in {}:\n{}",
                                file_path,
                                MarkdownCodeBlock {
                                    tag: "diff",
                                    text: content
                                }
                            )
                            .ok();
                        }
                    }

                    language_model::MessageContent::Text(uri.as_link().to_string())
                }
            };

            message.content.push(chunk);
        }

        let len_before_context = message.content.len();

        if file_context.len() > OPEN_FILES_TAG.len() {
            file_context.push_str("</files>\n");
            message
                .content
                .push(language_model::MessageContent::Text(file_context));
        }

        if directory_context.len() > OPEN_DIRECTORIES_TAG.len() {
            directory_context.push_str("</directories>\n");
            message
                .content
                .push(language_model::MessageContent::Text(directory_context));
        }

        if symbol_context.len() > OPEN_SYMBOLS_TAG.len() {
            symbol_context.push_str("</symbols>\n");
            message
                .content
                .push(language_model::MessageContent::Text(symbol_context));
        }

        if selection_context.len() > OPEN_SELECTIONS_TAG.len() {
            selection_context.push_str("</selections>\n");
            message
                .content
                .push(language_model::MessageContent::Text(selection_context));
        }

        if diffs_context.len() > OPEN_DIFFS_TAG.len() {
            diffs_context.push_str("</diffs>\n");
            message
                .content
                .push(language_model::MessageContent::Text(diffs_context));
        }

        if thread_context.len() > OPEN_THREADS_TAG.len() {
            thread_context.push_str("</threads>\n");
            message
                .content
                .push(language_model::MessageContent::Text(thread_context));
        }

        if fetch_context.len() > OPEN_FETCH_TAG.len() {
            fetch_context.push_str("</fetched_urls>\n");
            message
                .content
                .push(language_model::MessageContent::Text(fetch_context));
        }

        if rules_context.len() > OPEN_RULES_TAG.len() {
            rules_context.push_str("</user_rules>\n");
            message
                .content
                .push(language_model::MessageContent::Text(rules_context));
        }

        if diagnostics_context.len() > OPEN_DIAGNOSTICS_TAG.len() {
            diagnostics_context.push_str("</diagnostics>\n");
            message
                .content
                .push(language_model::MessageContent::Text(diagnostics_context));
        }

        if merge_conflict_context.len() > MERGE_CONFLICT_TAG.len() {
            merge_conflict_context.push_str("</merge_conflicts>\n");
            message
                .content
                .push(language_model::MessageContent::Text(merge_conflict_context));
        }

        if message.content.len() > len_before_context {
            message.content.insert(
                len_before_context,
                language_model::MessageContent::Text(OPEN_CONTEXT.into()),
            );
            message
                .content
                .push(language_model::MessageContent::Text("</context>".into()));
        }

        message
    }
}

fn codeblock_tag(full_path: &Path, line_range: Option<&RangeInclusive<u32>>) -> String {
    let mut result = String::new();

    if let Some(extension) = full_path.extension().and_then(|ext| ext.to_str()) {
        let _ = write!(result, "{} ", extension);
    }

    let _ = write!(result, "{}", full_path.display());

    if let Some(range) = line_range {
        if range.start() == range.end() {
            let _ = write!(result, ":{}", range.start() + 1);
        } else {
            let _ = write!(result, ":{}-{}", range.start() + 1, range.end() + 1);
        }
    }

    result
}

impl AgentMessage {
    pub fn to_markdown(&self) -> String {
        let mut markdown = String::new();

        for content in &self.content {
            match content {
                AgentMessageContent::Text(text) => {
                    markdown.push_str(text);
                    markdown.push('\n');
                }
                AgentMessageContent::Thinking { text, .. } => {
                    markdown.push_str("<think>");
                    markdown.push_str(text);
                    markdown.push_str("</think>\n");
                }
                AgentMessageContent::RedactedThinking(_) => {
                    markdown.push_str("<redacted_thinking />\n")
                }
                AgentMessageContent::ToolUse(tool_use) => {
                    markdown.push_str(&format!(
                        "**Tool Use**: {} (ID: {})\n",
                        tool_use.name, tool_use.id
                    ));
                    markdown.push_str(&format!(
                        "{}\n",
                        MarkdownCodeBlock {
                            tag: "json",
                            text: &format!("{:#}", tool_use.input)
                        }
                    ));
                }
            }
        }

        for tool_result in self.tool_results.values() {
            markdown.push_str(&format!(
                "**Tool Result**: {} (ID: {})\n\n",
                tool_result.tool_name, tool_result.tool_use_id
            ));
            if tool_result.is_error {
                markdown.push_str("**ERROR:**\n");
            }

            for part in &tool_result.content {
                match part {
                    LanguageModelToolResultContent::Text(text) => {
                        writeln!(markdown, "{text}\n").ok();
                    }
                    LanguageModelToolResultContent::Image(_) => {
                        writeln!(markdown, "<image />\n").ok();
                    }
                }
            }

            if let Some(output) = tool_result.output.as_ref() {
                writeln!(
                    markdown,
                    "**Debug Output**:\n\n```json\n{}\n```\n",
                    serde_json::to_string_pretty(output).unwrap()
                )
                .unwrap();
            }
        }

        markdown
    }

    pub fn to_request(&self) -> Vec<LanguageModelRequestMessage> {
        let mut assistant_message = LanguageModelRequestMessage {
            role: Role::Assistant,
            content: Vec::with_capacity(self.content.len()),
            cache: false,
            reasoning_details: self.reasoning_details.clone(),
        };
        for chunk in &self.content {
            match chunk {
                AgentMessageContent::Text(text) => {
                    assistant_message
                        .content
                        .push(language_model::MessageContent::Text(text.clone()));
                }
                AgentMessageContent::Thinking { text, signature } => {
                    assistant_message
                        .content
                        .push(language_model::MessageContent::Thinking {
                            text: text.clone(),
                            signature: signature.clone(),
                        });
                }
                AgentMessageContent::RedactedThinking(value) => {
                    assistant_message.content.push(
                        language_model::MessageContent::RedactedThinking(value.clone()),
                    );
                }
                AgentMessageContent::ToolUse(tool_use) => {
                    if self.tool_results.contains_key(&tool_use.id) {
                        assistant_message
                            .content
                            .push(language_model::MessageContent::ToolUse(tool_use.clone()));
                    }
                }
            };
        }

        let mut user_message = LanguageModelRequestMessage {
            role: Role::User,
            content: Vec::new(),
            cache: false,
            reasoning_details: None,
        };

        for tool_result in self.tool_results.values() {
            let mut tool_result = tool_result.clone();
            // Surprisingly, the API fails if we return an empty string here.
            // It thinks we are sending a tool use without a tool result.
            if tool_result.is_content_empty() {
                tool_result.content = vec!["<Tool returned an empty string>".into()];
            }
            user_message
                .content
                .push(language_model::MessageContent::ToolResult(tool_result));
        }

        let mut messages = Vec::new();
        if !assistant_message.content.is_empty() {
            messages.push(assistant_message);
        }
        if !user_message.content.is_empty() {
            messages.push(user_message);
        }
        messages
    }
}

#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentMessage {
    pub content: Vec<AgentMessageContent>,
    pub tool_results: IndexMap<LanguageModelToolUseId, LanguageModelToolResult>,
    pub reasoning_details: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentMessageContent {
    Text(String),
    Thinking {
        text: String,
        signature: Option<String>,
    },
    RedactedThinking(String),
    ToolUse(LanguageModelToolUse),
}

impl From<&str> for UserMessageContent {
    fn from(text: &str) -> Self {
        Self::Text(text.into())
    }
}

impl From<String> for UserMessageContent {
    fn from(text: String) -> Self {
        Self::Text(text)
    }
}

impl UserMessageContent {
    pub fn from_content_block(value: acp::ContentBlock, path_style: PathStyle) -> Self {
        match value {
            acp::ContentBlock::Text(text_content) => Self::Text(text_content.text),
            acp::ContentBlock::Image(image_content) => Self::Image(convert_image(image_content)),
            acp::ContentBlock::Audio(_) => {
                // TODO
                Self::Text("[audio]".to_owned())
            }
            acp::ContentBlock::ResourceLink(resource_link) => {
                match MentionUri::parse(&resource_link.uri, path_style) {
                    Ok(uri) => Self::Mention {
                        uri,
                        content: String::new(),
                    },
                    Err(err) => {
                        log::error!("Failed to parse mention link: {}", err);
                        Self::Text(format!("[{}]({})", resource_link.name, resource_link.uri))
                    }
                }
            }
            acp::ContentBlock::Resource(resource) => match resource.resource {
                acp::EmbeddedResourceResource::TextResourceContents(resource) => {
                    match MentionUri::parse(&resource.uri, path_style) {
                        Ok(uri) => Self::Mention {
                            uri,
                            content: resource.text,
                        },
                        Err(err) => {
                            log::error!("Failed to parse mention link: {}", err);
                            Self::Text(
                                MarkdownCodeBlock {
                                    tag: &resource.uri,
                                    text: &resource.text,
                                }
                                .to_string(),
                            )
                        }
                    }
                }
                acp::EmbeddedResourceResource::BlobResourceContents(_) => {
                    // TODO
                    Self::Text("[blob]".to_owned())
                }
                other => {
                    log::warn!("Unexpected content type: {:?}", other);
                    Self::Text("[unknown]".to_owned())
                }
            },
            other => {
                log::warn!("Unexpected content type: {:?}", other);
                Self::Text("[unknown]".to_owned())
            }
        }
    }
}

impl From<UserMessageContent> for acp::ContentBlock {
    fn from(content: UserMessageContent) -> Self {
        match content {
            UserMessageContent::Text(text) => text.into(),
            UserMessageContent::Image(image) => {
                acp::ContentBlock::Image(acp::ImageContent::new(image.source, "image/png"))
            }
            UserMessageContent::Mention { uri, content } => acp::ContentBlock::Resource(
                acp::EmbeddedResource::new(acp::EmbeddedResourceResource::TextResourceContents(
                    acp::TextResourceContents::new(content, uri.to_uri().to_string()),
                )),
            ),
        }
    }
}

fn convert_image(image_content: acp::ImageContent) -> LanguageModelImage {
    LanguageModelImage {
        source: image_content.data.into(),
    }
}
