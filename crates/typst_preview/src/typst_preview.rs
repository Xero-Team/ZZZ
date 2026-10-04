use gpui::{App, actions};
use workspace::Workspace;

pub mod typst_preview_view;
mod typst_world;

pub use zzz_actions::preview::typst::{OpenPreview, OpenPreviewToTheSide};

actions!(
    typst,
    [
        /// Opens a following Typst preview that tracks the active Typst editor.
        OpenFollowingPreview,
        /// Closes the Typst preview and returns focus to its source editor.
        CloseAndReturnToEditor,
        /// Scrolls up by one page in the Typst preview.
        ScrollPageUp,
        /// Scrolls down by one page in the Typst preview.
        ScrollPageDown,
        /// Scrolls up in the Typst preview.
        ScrollUp,
        /// Scrolls down in the Typst preview.
        ScrollDown,
        /// Scrolls to the top of the Typst preview.
        ScrollToTop,
        /// Scrolls to the bottom of the Typst preview.
        ScrollToBottom
    ]
);

pub fn init(cx: &mut App) {
    workspace::register_serializable_item::<typst_preview_view::TypstPreviewView>(cx);

    cx.observe_new(|workspace: &mut Workspace, window, cx| {
        let Some(window) = window else {
            return;
        };
        typst_preview_view::TypstPreviewView::register(workspace, window, cx);
    })
    .detach();
}
