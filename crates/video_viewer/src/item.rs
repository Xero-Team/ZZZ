use std::path::Path;

use editor::{EditorSettings, items::entry_git_aware_label_color};
use file_icons::FileIcons;
use gpui::{AnyElement, App, Entity, Font, Task, Window};
use project::Project;
use settings::Settings;
use theme_settings::ThemeSettings;
use ui::{Icon, IconName, Label, LabelCommon as _, prelude::*};
use workspace::{
    ItemSettings, Pane, WorkspaceId,
    invalid_item_view::InvalidItemView,
    item::{HighlightedText, Item, ItemEvent, ProjectItem, TabContentParams},
};

use crate::{
    VideoItem,
    view::{VideoView, VideoViewEvent},
};

impl Item for VideoView {
    type Event = VideoViewEvent;

    fn to_item_events(event: &Self::Event, f: &mut dyn FnMut(ItemEvent)) {
        match event {
            VideoViewEvent::TitleChanged => {
                f(ItemEvent::UpdateTab);
                f(ItemEvent::UpdateBreadcrumbs);
            }
        }
    }

    fn tab_content_text(&self, _detail: usize, cx: &App) -> SharedString {
        self.file_name(cx).into()
    }

    fn tab_content(&self, params: TabContentParams, _window: &Window, cx: &App) -> AnyElement {
        let project_path = self.project_path(cx);
        let label_color = if ItemSettings::get_global(cx).git_status {
            let git_status = self
                .project
                .read(cx)
                .project_path_git_status(&project_path, cx)
                .map(|status| status.summary())
                .unwrap_or_default();
            self.project
                .read(cx)
                .entry_for_path(&project_path, cx)
                .map(|entry| {
                    entry_git_aware_label_color(git_status, entry.is_ignored, params.selected)
                })
                .unwrap_or_else(|| params.text_color())
        } else {
            params.text_color()
        };

        Label::new(self.tab_content_text(params.detail.unwrap_or_default(), cx))
            .single_line()
            .color(label_color)
            .when(params.preview, |this| this.italic())
            .into_any_element()
    }

    fn tab_icon(&self, _window: &Window, cx: &App) -> Option<Icon> {
        let path = self.relative_path(cx);
        ItemSettings::get_global(cx)
            .file_icons
            .then(|| FileIcons::get_icon(path.as_std_path(), cx))
            .flatten()
            .map(Icon::from_path)
            .or_else(|| Some(Icon::new(IconName::File)))
    }

    fn tab_tooltip_text(&self, cx: &App) -> Option<SharedString> {
        Some(self.metadata_tooltip(cx).into())
    }

    fn for_each_project_item(
        &self,
        cx: &App,
        f: &mut dyn FnMut(gpui::EntityId, &dyn project::ProjectItem),
    ) {
        f(self.video_item.entity_id(), self.video_item.read(cx))
    }

    fn breadcrumb_location(&self, cx: &App) -> workspace::ToolbarItemLocation {
        if EditorSettings::get_global(cx).toolbar.breadcrumbs {
            workspace::ToolbarItemLocation::PrimaryLeft
        } else {
            workspace::ToolbarItemLocation::Hidden
        }
    }

    fn breadcrumbs(&self, cx: &App) -> Option<(Vec<HighlightedText>, Option<Font>)> {
        let path_style = self.project.read(cx).path_style(cx);
        let text = self.relative_path(cx).display(path_style).into_owned();
        let font = ThemeSettings::get_global(cx).buffer_font.clone();
        Some((
            vec![HighlightedText {
                text: text.into(),
                highlights: vec![],
            }],
            Some(font),
        ))
    }

    fn can_split(&self) -> bool {
        true
    }

    fn clone_on_split(
        &self,
        _workspace_id: Option<WorkspaceId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Option<Entity<Self>>> {
        let video_item = self.video_item.clone();
        let project = self.project.clone();
        Task::ready(Some(
            cx.new(|cx| VideoView::new(video_item, project, window, cx)),
        ))
    }

    fn deactivated(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.stop_playback(cx);
    }

    fn on_removed(&self, _cx: &mut Context<Self>) {
        if let Some(player) = &self.audio {
            player.stop();
        }
    }

    fn buffer_kind(&self, _cx: &App) -> workspace::item::ItemBufferKind {
        workspace::item::ItemBufferKind::Singleton
    }
}

impl ProjectItem for VideoView {
    type Item = VideoItem;

    fn for_project_item(
        project: Entity<Project>,
        _pane: Option<&Pane>,
        item: Entity<Self::Item>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::new(item, project, window, cx)
    }

    fn for_broken_project_item(
        abs_path: &Path,
        is_local: bool,
        error: &anyhow::Error,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<InvalidItemView> {
        Some(InvalidItemView::new(abs_path, is_local, error, window, cx))
    }
}
