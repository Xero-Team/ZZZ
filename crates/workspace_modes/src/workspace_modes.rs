//! Workspace layout modes.
//!
//! A mode is a named dock arrangement rather than a separate window
//! surface: Editor restores the docks that were open before a mode switch,
//! Terminal collapses everything except the bottom dock, and Zen hides every
//! dock. The active mode is persisted per workspace in the key-value store.
//!
//! Adapted from Glass's workspace_modes (GPL-3.0-or-later) at
//! e8ec0832b4f47b9e1420730c7d5bf7f3bd83eda9. Glass registered full-screen
//! browser views; ZZZ has no browser mode, so the registry is replaced by
//! actions that arrange the existing docks.

use db::kvp::KeyValueStore;
use gpui::{App, AppContext as _, Context, Global, SharedString, Window, actions};
use i18n::tr;
use std::sync::Arc;
use util::ResultExt as _;
use workspace::{Workspace, dock::DockPosition};

actions!(
    workspace_modes,
    [
        /// Restores the dock layout used before switching modes.
        SwitchToEditorMode,
        /// Collapses every dock except the bottom one.
        SwitchToTerminalMode,
        /// Hides every dock.
        SwitchToZenMode,
        /// Advances to the next workspace mode.
        CycleMode,
    ]
);

const MODE_KEY_PREFIX: &str = "workspace_mode:";
const DOCK_POSITIONS: [DockPosition; 3] = [
    DockPosition::Left,
    DockPosition::Right,
    DockPosition::Bottom,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModeId {
    Editor,
    Terminal,
    Zen,
}

impl ModeId {
    const ALL: [ModeId; 3] = [ModeId::Editor, ModeId::Terminal, ModeId::Zen];

    pub fn as_str(self) -> &'static str {
        match self {
            ModeId::Editor => "editor",
            ModeId::Terminal => "terminal",
            ModeId::Zen => "zen",
        }
    }

    pub fn from_str(value: &str) -> Self {
        match value {
            "terminal" => ModeId::Terminal,
            "zen" => ModeId::Zen,
            _ => ModeId::Editor,
        }
    }

    pub fn label(self, cx: &App) -> SharedString {
        match self {
            ModeId::Editor => tr(cx, "workspace_modes.editor", "Editor"),
            ModeId::Terminal => tr(cx, "workspace_modes.terminal", "Terminal"),
            ModeId::Zen => tr(cx, "workspace_modes.zen", "Zen"),
        }
        .into()
    }

    fn next(self) -> Self {
        let index = Self::ALL.iter().position(|mode| *mode == self).unwrap_or(0);
        Self::ALL[(index + 1) % Self::ALL.len()]
    }
}

struct ActiveMode {
    mode: ModeId,
    /// The docks that were open in Editor mode, so leaving and returning
    /// restores the user's arrangement instead of a default one.
    editor_docks: Vec<DockPosition>,
}

impl Global for ActiveMode {}

impl Default for ActiveMode {
    fn default() -> Self {
        Self {
            mode: ModeId::Editor,
            editor_docks: Vec::new(),
        }
    }
}

/// Reads the layout mode without requiring [`init`] to have run, so widgets like
/// the title bar's mode control can be rendered in the tests and windows that
/// never install the mode actions. A missing global means "no mode switching is
/// installed", which is the same arrangement as Editor mode.
pub fn active_mode(cx: &App) -> ModeId {
    cx.try_global::<ActiveMode>()
        .map_or(ModeId::Editor, |state| state.mode)
}

/// Observes mode switches so another layout layer can unwind before the docks
/// move.
///
/// Terminal mode fills the same left and right docks that Editor mode
/// restores, so the two layers must not each arrange those docks behind the
/// other's back. `workspace_modes` has no dependency on terminal mode; the
/// assembly layer installs the observer instead.
pub trait ModeObserver: 'static {
    /// Called with the mode being switched to, before any dock changes.
    fn will_switch_mode(
        &self,
        workspace: &mut Workspace,
        target: ModeId,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    );
}

#[derive(Default)]
struct ModeObserverSlot(Option<Arc<dyn ModeObserver>>);

impl Global for ModeObserverSlot {}

/// Installs the observer that runs before a mode switch rearranges the docks.
pub fn set_mode_observer(observer: Option<Arc<dyn ModeObserver>>, cx: &mut App) {
    if cx.has_global::<ModeObserverSlot>() {
        cx.global_mut::<ModeObserverSlot>().0 = observer;
    } else {
        cx.set_global(ModeObserverSlot(observer));
    }
}

/// Switches `workspace` to Editor mode.
///
/// A layer that arranges docks on top of the layout mode (such as terminal
/// mode) calls this before taking the docks over, so the title bar's control
/// keeps matching the docks that are actually open.
pub fn switch_to_editor(
    workspace: &mut Workspace,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    switch(workspace, ModeId::Editor, window, cx);
}

fn active_mode_mut(cx: &mut App) -> &mut ActiveMode {
    if cx.try_global::<ActiveMode>().is_none() {
        cx.set_global(ActiveMode::default());
    }
    cx.global_mut::<ActiveMode>()
}

pub fn init(cx: &mut App) {
    cx.set_global(ActiveMode::default());

    cx.observe_new(|workspace: &mut Workspace, window, cx| {
        if let Some(window) = window {
            restore_persisted_mode(workspace, window, cx);
        }

        workspace.register_action(|workspace, _: &SwitchToEditorMode, window, cx| {
            switch(workspace, ModeId::Editor, window, cx);
        });
        workspace.register_action(|workspace, _: &SwitchToTerminalMode, window, cx| {
            switch(workspace, ModeId::Terminal, window, cx);
        });
        workspace.register_action(|workspace, _: &SwitchToZenMode, window, cx| {
            switch(workspace, ModeId::Zen, window, cx);
        });
        workspace.register_action(|workspace, _: &CycleMode, window, cx| {
            let next = active_mode(cx).next();
            switch(workspace, next, window, cx);
        });
    })
    .detach();
}

fn mode_key(workspace: &Workspace) -> String {
    match workspace.database_id() {
        Some(id) => format!("{MODE_KEY_PREFIX}{}", i64::from(id)),
        None => MODE_KEY_PREFIX.to_owned(),
    }
}

fn restore_persisted_mode(
    workspace: &mut Workspace,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let key = mode_key(workspace);
    let Some(value) = KeyValueStore::global(cx).read_kvp(&key).log_err().flatten() else {
        return;
    };
    let mode = ModeId::from_str(&value);
    if mode != ModeId::Editor {
        switch(workspace, mode, window, cx);
    }
}

fn switch(
    workspace: &mut Workspace,
    mode: ModeId,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let previous = active_mode(cx);
    if previous == mode {
        return;
    }

    // Let another layout layer unwind while the mode still names what it owns,
    // and before the docks are snapshotted, so a snapshot never records docks
    // that only that layer opened.
    let observer = cx
        .try_global::<ModeObserverSlot>()
        .and_then(|slot| slot.0.clone());
    if let Some(observer) = observer {
        observer.will_switch_mode(workspace, mode, window, cx);
    }

    let mut open_positions = DOCK_POSITIONS
        .into_iter()
        .filter(|position| workspace.dock_at_position(*position).read(cx).is_open())
        .collect::<Vec<_>>();

    if previous == ModeId::Editor {
        active_mode_mut(cx).editor_docks = open_positions.clone();
    }

    // Collapse every open dock, then open only the ones the target mode wants.
    for position in open_positions.drain(..) {
        workspace.toggle_dock(position, window, cx);
    }

    let target_positions: Vec<DockPosition> = match mode {
        ModeId::Editor => {
            let saved = cx
                .try_global::<ActiveMode>()
                .map_or_else(Vec::new, |state| state.editor_docks.clone());
            if saved.is_empty() {
                vec![DockPosition::Left, DockPosition::Right]
            } else {
                saved
            }
        }
        ModeId::Terminal => vec![DockPosition::Bottom],
        ModeId::Zen => Vec::new(),
    };

    for position in target_positions {
        if !workspace.dock_at_position(position).read(cx).is_open() {
            workspace.toggle_dock(position, window, cx);
        }
    }

    active_mode_mut(cx).mode = mode;

    let key = mode_key(workspace);
    let value = mode.as_str().to_owned();
    let kvp = KeyValueStore::global(cx);
    cx.background_spawn(async move { kvp.write_kvp(key, value).await })
        .detach_and_log_err(cx);

    cx.notify();
}
