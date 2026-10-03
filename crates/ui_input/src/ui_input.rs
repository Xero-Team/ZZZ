//! This crate provides UI components that can be used for form-like scenarios, such as a input and number field.
//!
//! It can't be located in the `ui` crate because it depends on `editor`.
//!
mod input_field;

use std::{any::Any, fmt, sync::Arc};

use gpui::{FocusHandle, Global, Subscription};
pub use input_field::*;
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
