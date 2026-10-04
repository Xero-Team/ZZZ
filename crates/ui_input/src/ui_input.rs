//! This crate provides UI components for form-like scenarios through an application-scoped
//! editor adapter.
mod input_field;

use std::{any::Any, fmt, sync::Arc};

use gpui::{FocusHandle, Global, Subscription, div, prelude::*};
pub use input_field::*;
use parking_lot::Mutex;
use ui::{AnyElement, App, Window};

pub trait ErasedEditor: 'static {
    fn text(&self, cx: &App) -> String;
    fn set_text(&self, text: &str, window: &mut Window, cx: &mut App);
    fn clear(&self, window: &mut Window, cx: &mut App);
    fn set_placeholder_text(&self, text: &str, window: &mut Window, _: &mut App);
    fn move_selection_to_end(&self, window: &mut Window, _: &mut App);
    fn select_all(&self, window: &mut Window, cx: &mut App);
    fn set_masked(&self, masked: bool, window: &mut Window, cx: &mut App);

    fn focus_handle(&self, cx: &App) -> FocusHandle;

    fn subscribe(
        &self,
        callback: Box<dyn FnMut(ErasedEditorEvent, &mut Window, &mut App) + 'static>,
        window: &mut Window,
        cx: &mut App,
    ) -> Subscription;
    fn render(&self, window: &mut Window, cx: &App) -> AnyElement;
    fn as_any(&self) -> &dyn Any;
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ErasedEditorEvent {
    BufferEdited,
    Blurred,
}
/// Per-application factory for the editor adapter used by generic input fields.
pub struct ErasedEditorFactory(pub fn(&mut Window, &mut App) -> Arc<dyn ErasedEditor>);

impl Global for ErasedEditorFactory {}

/// Error returned when an application has not registered an editor adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EditorFactoryUnavailable;

impl fmt::Display for EditorFactoryUnavailable {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("no ErasedEditorFactory is registered for this application")
    }
}

impl std::error::Error for EditorFactoryUnavailable {}

pub fn set_editor_factory(
    cx: &mut App,
    factory: fn(&mut Window, &mut App) -> Arc<dyn ErasedEditor>,
) {
    cx.set_global(ErasedEditorFactory(factory));
}

/// Creates an editor through the adapter registered for the current application.
pub fn create_editor(
    window: &mut Window,
    cx: &mut App,
) -> Result<Arc<dyn ErasedEditor>, EditorFactoryUnavailable> {
    let factory = cx
        .try_global::<ErasedEditorFactory>()
        .map(|factory| factory.0)
        .ok_or(EditorFactoryUnavailable)?;
    Ok(factory(window, cx))
}

/// Creates an editor, or a non-panicking local fallback when the application adapter is absent.
pub fn create_editor_or_fallback(
    window: &mut Window,
    cx: &mut App,
    placeholder: &str,
) -> Arc<dyn ErasedEditor> {
    create_editor(window, cx).unwrap_or_else(|error| {
        log::error!("failed to create editor adapter: {error}");
        Arc::new(UnavailableEditor::new(cx, placeholder))
    })
}

struct UnavailableEditor {
    focus_handle: FocusHandle,
    text: Mutex<String>,
    placeholder: Mutex<String>,
}

impl UnavailableEditor {
    fn new(cx: &App, placeholder: &str) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            text: Mutex::new(String::new()),
            placeholder: Mutex::new(placeholder.to_owned()),
        }
    }
}

impl ErasedEditor for UnavailableEditor {
    fn text(&self, _: &App) -> String {
        self.text.lock().clone()
    }

    fn set_text(&self, text: &str, _: &mut Window, _: &mut App) {
        *self.text.lock() = text.to_owned();
    }

    fn clear(&self, _: &mut Window, _: &mut App) {
        self.text.lock().clear();
    }

    fn set_placeholder_text(&self, text: &str, _: &mut Window, _: &mut App) {
        *self.placeholder.lock() = text.to_owned();
    }

    fn move_selection_to_end(&self, _: &mut Window, _: &mut App) {}

    fn select_all(&self, _: &mut Window, _: &mut App) {}

    fn set_masked(&self, _: bool, _: &mut Window, _: &mut App) {}

    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }

    fn subscribe(
        &self,
        _: Box<dyn FnMut(ErasedEditorEvent, &mut Window, &mut App) + 'static>,
        _: &mut Window,
        _: &mut App,
    ) -> Subscription {
        Subscription::new(|| {})
    }

    fn render(&self, _: &mut Window, _: &App) -> AnyElement {
        let text = self.text.lock();
        let display_text = if text.is_empty() {
            self.placeholder.lock().clone()
        } else {
            text.clone()
        };
        div()
            .track_focus(&self.focus_handle)
            .child(display_text)
            .into_any_element()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
