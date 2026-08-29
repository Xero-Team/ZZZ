use language_model::LanguageModel;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct AnthropicEventData {
    pub completion_type: AnthropicCompletionType,
    pub event: AnthropicEventType,
    pub language_name: Option<String>,
    pub message_id: Option<String>,
}

#[derive(Clone, Debug)]
pub enum AnthropicCompletionType {
    Editor,
    Terminal,
    Panel,
}

#[derive(Clone, Debug)]
pub enum AnthropicEventType {
    Invoked,
    Response,
    Accept,
    Reject,
}

pub fn report_anthropic_event(
    model: &Arc<dyn LanguageModel>,
    event: AnthropicEventData,
    cx: &gpui::App,
) {
    let _ = (model, event, cx);
}

/// Compatibility shim retained for callers compiled against the upstream API.
/// It intentionally performs no network I/O or persistence.
#[derive(Clone, Default)]
pub struct AnthropicEventReporter;

impl AnthropicEventReporter {
    pub fn new(_model: &Arc<dyn LanguageModel>, _cx: &gpui::App) -> Self {
        Self
    }

    pub fn report(&self, _event: AnthropicEventData) {}
}
