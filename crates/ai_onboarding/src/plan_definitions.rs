use gpui::{IntoElement, ParentElement};
use ui::{List, ListBulletItem, prelude::*};

/// Compatibility definitions retained for the onboarding component API.
/// Content is provider-neutral because ZZZ has no built-in plans.
pub struct PlanDefinitions;

impl PlanDefinitions {
    pub fn free_plan(&self) -> impl IntoElement {
        List::new()
            .child(ListBulletItem::new("Use local Ollama or llama.cpp models"))
            .child(ListBulletItem::new("Use your own provider credentials"))
            .child(ListBulletItem::new("Use external agents through ACP"))
    }

    pub fn sign_in_upsell(&self) -> impl IntoElement {
        List::new()
            .child(ListBulletItem::new("Configure a local provider"))
            .child(ListBulletItem::new("No ZZZ account required"))
    }

    pub fn pro_trial(&self, period: bool) -> impl IntoElement {
        List::new()
            .child(ListBulletItem::new("Configure a local provider"))
            .when(period, |this| {
                this.child(ListBulletItem::new("No trial or subscription"))
            })
    }

    pub fn pro_plan(&self) -> impl IntoElement {
        List::new()
            .child(ListBulletItem::new("Usage is controlled by your provider"))
            .child(ListBulletItem::new("No built-in plan or billing"))
    }

    pub fn business_plan(&self) -> impl IntoElement {
        List::new()
            .child(ListBulletItem::new("Usage is controlled by your provider"))
            .child(ListBulletItem::new("Remote access is explicit opt-in"))
    }

    pub fn student_plan(&self) -> impl IntoElement {
        List::new()
            .child(ListBulletItem::new("Use local or user-managed providers"))
            .child(ListBulletItem::new("No account or subscription required"))
    }
}
