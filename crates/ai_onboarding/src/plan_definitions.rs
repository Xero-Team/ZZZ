use gpui::{IntoElement, ParentElement};
use ui::{List, ListBulletItem};

/// Provider-neutral copy for the onboarding component API.
pub struct PlanDefinitions;

impl PlanDefinitions {
    pub fn configure_local_provider(&self) -> impl IntoElement {
        List::new()
            .child(ListBulletItem::new("Configure a local provider"))
            .child(ListBulletItem::new("Ollama and llama.cpp are preferred"))
            .child(ListBulletItem::new("No ZZZ account required"))
    }
}
