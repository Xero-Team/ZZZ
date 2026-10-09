use gpui::{AnyElement, ElementId};
use smallvec::SmallVec;
use ui_component_registry::{Component, ComponentScope, example_group_with_title, single_example};

use crate::{Label, ListHeader, ListItem, prelude::*};

pub enum EmptyMessage {
    Text(SharedString),
    Element(AnyElement),
}

/// Semantic metadata for a list whose children are known to be list items.
#[derive(Clone)]
pub struct ListAccessibility {
    #[cfg(feature = "accessibility")]
    element_id: ElementId,
    #[cfg(feature = "accessibility")]
    label: SharedString,
}

impl ListAccessibility {
    /// Creates the semantic identity and name for a list.
    ///
    /// This is opt-in because [`List`] may also contain structural content such as headers,
    /// placeholders, menus, or non-list children.
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        #[cfg(not(feature = "accessibility"))]
        let _ = (id, label);

        Self {
            #[cfg(feature = "accessibility")]
            element_id: id.into(),
            #[cfg(feature = "accessibility")]
            label: label.into(),
        }
    }
}

#[derive(IntoElement, RegisterComponent)]
pub struct List {
    /// Message to display when the list is empty
    /// Defaults to "No items"
    empty_message: EmptyMessage,
    header: Option<ListHeader>,
    toggle: Option<bool>,
    children: SmallVec<[AnyElement; 2]>,
    #[cfg(feature = "accessibility")]
    accessibility: Option<ListAccessibility>,
}

impl Default for List {
    fn default() -> Self {
        Self::new()
    }
}

impl List {
    pub fn new() -> Self {
        Self {
            empty_message: EmptyMessage::Text("No items".into()),
            header: None,
            toggle: None,
            children: SmallVec::new(),
            #[cfg(feature = "accessibility")]
            accessibility: None,
        }
    }

    pub fn empty_message(mut self, message: impl Into<EmptyMessage>) -> Self {
        self.empty_message = message.into();
        self
    }

    pub fn header(mut self, header: impl Into<Option<ListHeader>>) -> Self {
        self.header = header.into();
        self
    }

    pub fn toggle(mut self, toggle: impl Into<Option<bool>>) -> Self {
        self.toggle = toggle.into();
        self
    }

    /// Adds `Role::List` semantics when every semantic child is a list item.
    pub fn with_accessibility(self, accessibility: ListAccessibility) -> Self {
        #[cfg(feature = "accessibility")]
        {
            let mut list = self;
            list.accessibility = Some(accessibility);
            list
        }
        #[cfg(not(feature = "accessibility"))]
        {
            std::mem::drop(accessibility);
            self
        }
    }
}

impl ParentElement for List {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements)
    }
}

impl From<String> for EmptyMessage {
    fn from(s: String) -> Self {
        EmptyMessage::Text(SharedString::from(s))
    }
}

impl From<&str> for EmptyMessage {
    fn from(s: &str) -> Self {
        EmptyMessage::Text(SharedString::from(s.to_owned()))
    }
}

impl From<AnyElement> for EmptyMessage {
    fn from(e: AnyElement) -> Self {
        EmptyMessage::Element(e)
    }
}

impl RenderOnce for List {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let list = v_flex()
            .w_full()
            .py(DynamicSpacing::Base04.rems(cx))
            .children(self.header)
            .map(|this| match (self.children.is_empty(), self.toggle) {
                (false, _) => this.children(self.children),
                (true, Some(false)) => this,
                (true, _) => match self.empty_message {
                    EmptyMessage::Text(text) => {
                        this.px_2().child(Label::new(text).color(Color::Muted))
                    }
                    EmptyMessage::Element(element) => this.child(element),
                },
            });

        #[cfg(feature = "accessibility")]
        if let Some(accessibility) = self.accessibility {
            return list
                .id(accessibility.element_id)
                .role(gpui::accesskit::Role::List)
                .aria_label(accessibility.label)
                .into_any_element();
        }

        list.into_any_element()
    }
}

impl Component for List {
    fn scope() -> ComponentScope {
        ComponentScope::Layout
    }

    fn description() -> Option<&'static str> {
        Some(
            "A container component for displaying a collection of list items with optional header and empty state.",
        )
    }

    fn preview(_window: &mut Window, _cx: &mut App) -> Option<AnyElement> {
        Some(
            v_flex()
                .gap_6()
                .children(vec![example_group_with_title(
                    "Basic Lists",
                    vec![
                        single_example(
                            "Simple List",
                            List::new()
                                .child(ListItem::new("item1").child(Label::new("Item 1")))
                                .child(ListItem::new("item2").child(Label::new("Item 2")))
                                .child(ListItem::new("item3").child(Label::new("Item 3")))
                                .into_any_element(),
                        ),
                        single_example(
                            "With Header",
                            List::new()
                                .header(ListHeader::new("Section Header"))
                                .child(ListItem::new("item1").child(Label::new("Item 1")))
                                .child(ListItem::new("item2").child(Label::new("Item 2")))
                                .into_any_element(),
                        ),
                        single_example(
                            "Empty List",
                            List::new()
                                .empty_message("No items to display")
                                .into_any_element(),
                        ),
                    ],
                )])
                .into_any_element(),
        )
    }
}
