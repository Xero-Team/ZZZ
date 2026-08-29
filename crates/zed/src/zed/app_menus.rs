use git_ui::git_panel;
use gpui::{App, Menu, MenuItem, OsAction};
use i18n::tr;
use release_channel::ReleaseChannel;
use terminal_view::terminal_panel;
use zed_actions::{debug_panel, dev};

pub fn app_menus(cx: &mut App) -> Vec<Menu> {
    use zed_actions::Quit;

    let mut view_items = vec![
        MenuItem::action(
            tr(cx, "menu.view.zoom_in", "Zoom In"),
            zed_actions::IncreaseBufferFontSize { persist: false },
        ),
        MenuItem::action(
            tr(cx, "menu.view.zoom_out", "Zoom Out"),
            zed_actions::DecreaseBufferFontSize { persist: false },
        ),
        MenuItem::action(
            tr(cx, "menu.view.reset_zoom", "Reset Zoom"),
            zed_actions::ResetBufferFontSize { persist: false },
        ),
        MenuItem::action(
            tr(cx, "menu.view.reset_all_zoom", "Reset All Zoom"),
            zed_actions::ResetAllZoom { persist: false },
        ),
        MenuItem::separator(),
        MenuItem::action(
            tr(cx, "menu.view.toggle_left_dock", "Toggle Left Dock"),
            workspace::ToggleLeftDock,
        ),
        MenuItem::action(
            tr(cx, "menu.view.toggle_right_dock", "Toggle Right Dock"),
            workspace::ToggleRightDock,
        ),
        MenuItem::action(
            tr(cx, "menu.view.toggle_bottom_dock", "Toggle Bottom Dock"),
            workspace::ToggleBottomDock,
        ),
        MenuItem::action(
            tr(cx, "menu.view.toggle_all_docks", "Toggle All Docks"),
            workspace::ToggleAllDocks,
        ),
        MenuItem::submenu(Menu {
            name: tr(cx, "menu.view.editor_layout", "Editor Layout").into(),
            disabled: false,
            items: vec![
                MenuItem::action(
                    tr(cx, "menu.view.split_up", "Split Up"),
                    workspace::SplitUp::default(),
                ),
                MenuItem::action(
                    tr(cx, "menu.view.split_down", "Split Down"),
                    workspace::SplitDown::default(),
                ),
                MenuItem::action(
                    tr(cx, "menu.view.split_left", "Split Left"),
                    workspace::SplitLeft::default(),
                ),
                MenuItem::action(
                    tr(cx, "menu.view.split_right", "Split Right"),
                    workspace::SplitRight::default(),
                ),
            ],
        }),
        MenuItem::separator(),
        MenuItem::action(
            tr(cx, "menu.view.project_panel", "Project Panel"),
            zed_actions::project_panel::ToggleFocus,
        ),
        MenuItem::action(
            tr(cx, "menu.view.outline_panel", "Outline Panel"),
            outline_panel::ToggleFocus,
        ),
        MenuItem::action(
            tr(cx, "menu.view.terminal_panel", "Terminal Panel"),
            terminal_panel::ToggleFocus,
        ),
        MenuItem::action(
            tr(cx, "menu.view.debugger_panel", "Debugger Panel"),
            debug_panel::ToggleFocus,
        ),
        MenuItem::action(
            tr(cx, "menu.view.git_panel", "Git Panel"),
            git_panel::ToggleFocus,
        ),
        MenuItem::separator(),
        MenuItem::action(
            tr(cx, "menu.view.diagnostics", "Diagnostics"),
            diagnostics::Deploy,
        ),
        MenuItem::separator(),
    ];

    if ReleaseChannel::try_global(cx) == Some(ReleaseChannel::Dev) {
        view_items.push(MenuItem::action(
            tr(
                cx,
                "menu.view.toggle_gpui_inspector",
                "Toggle GPUI Inspector",
            ),
            dev::ToggleInspector,
        ));
        view_items.push(MenuItem::separator());
    }

    vec![
        Menu {
            name: tr(cx, "menu.app", "ZZZ").into(),
            disabled: false,
            items: vec![
                MenuItem::action(tr(cx, "menu.about", "About ZZZ"), zed_actions::About),
                MenuItem::separator(),
                MenuItem::submenu(Menu::new(tr(cx, "menu.settings", "Settings")).items([
                    MenuItem::action(
                        tr(cx, "menu.settings.open", "Open Settings"),
                        zed_actions::OpenSettings,
                    ),
                    MenuItem::action(
                        tr(cx, "menu.settings.open_file", "Open Settings File"),
                        super::OpenSettingsFile,
                    ),
                    MenuItem::action(
                        tr(cx, "menu.settings.open_project", "Open Project Settings"),
                        zed_actions::OpenProjectSettings,
                    ),
                    MenuItem::action(
                        tr(
                            cx,
                            "menu.settings.open_project_file",
                            "Open Project Settings File",
                        ),
                        super::OpenProjectSettingsFile,
                    ),
                    MenuItem::action(
                        tr(cx, "menu.settings.open_default", "Open Default Settings"),
                        super::OpenDefaultSettings,
                    ),
                    MenuItem::separator(),
                    MenuItem::action(
                        tr(cx, "menu.settings.open_keymap", "Open Keymap"),
                        zed_actions::OpenKeymap,
                    ),
                    MenuItem::action(
                        tr(cx, "menu.settings.open_keymap_file", "Open Keymap File"),
                        zed_actions::OpenKeymapFile,
                    ),
                    MenuItem::action(
                        tr(
                            cx,
                            "menu.settings.open_default_key_bindings",
                            "Open Default Key Bindings",
                        ),
                        zed_actions::OpenDefaultKeymap,
                    ),
                    MenuItem::separator(),
                    MenuItem::action(
                        tr(cx, "menu.settings.select_theme", "Select Theme..."),
                        zed_actions::theme_selector::Toggle::default(),
                    ),
                    MenuItem::action(
                        tr(
                            cx,
                            "menu.settings.select_icon_theme",
                            "Select Icon Theme...",
                        ),
                        zed_actions::icon_theme_selector::Toggle::default(),
                    ),
                ])),
                MenuItem::separator(),
                #[cfg(target_os = "macos")]
                MenuItem::os_submenu("Services", gpui::SystemMenuType::Services),
                MenuItem::separator(),
                MenuItem::action(
                    tr(cx, "menu.extensions", "Extensions"),
                    zed_actions::Extensions::default(),
                ),
                #[cfg(not(target_os = "windows"))]
                MenuItem::action(
                    tr(cx, "menu.install_cli", "Install CLI"),
                    install_cli::InstallCliBinary,
                ),
                MenuItem::separator(),
                #[cfg(target_os = "macos")]
                MenuItem::action(tr(cx, "menu.hide", "Hide ZZZ"), super::Hide),
                #[cfg(target_os = "macos")]
                MenuItem::action(tr(cx, "menu.hide_others", "Hide Others"), super::HideOthers),
                #[cfg(target_os = "macos")]
                MenuItem::action(tr(cx, "menu.show_all", "Show All"), super::ShowAll),
                MenuItem::separator(),
                MenuItem::action(tr(cx, "menu.quit", "Quit ZZZ"), Quit),
            ],
        },
        Menu {
            name: tr(cx, "menu.file", "File").into(),
            disabled: false,
            items: vec![
                MenuItem::action(tr(cx, "menu.file.new", "New"), workspace::NewFile),
                MenuItem::action(
                    tr(cx, "menu.file.new_window", "New Window"),
                    workspace::NewWindow,
                ),
                MenuItem::separator(),
                #[cfg(not(target_os = "macos"))]
                MenuItem::action(
                    tr(cx, "menu.file.open_file", "Open File..."),
                    workspace::OpenFiles,
                ),
                MenuItem::action(
                    if cfg!(not(target_os = "macos")) {
                        tr(cx, "menu.file.open_folder", "Open Folder...")
                    } else {
                        tr(cx, "menu.file.open", "Open...")
                    },
                    workspace::Open::default(),
                ),
                MenuItem::action(
                    tr(cx, "menu.file.open_recent", "Open Recent..."),
                    zed_actions::OpenRecent::default(),
                ),
                MenuItem::action(
                    tr(cx, "menu.file.open_remote", "Open Remote..."),
                    zed_actions::OpenRemote::default(),
                ),
                MenuItem::separator(),
                MenuItem::action(
                    tr(cx, "menu.file.add_folder", "Add Folder to Project..."),
                    workspace::AddFolderToProject,
                ),
                MenuItem::separator(),
                MenuItem::action(
                    tr(cx, "menu.file.save", "Save"),
                    workspace::Save { save_intent: None },
                ),
                MenuItem::action(tr(cx, "menu.file.save_as", "Save As..."), workspace::SaveAs),
                MenuItem::action(
                    tr(cx, "menu.file.save_all", "Save All"),
                    workspace::SaveAll { save_intent: None },
                ),
                MenuItem::separator(),
                MenuItem::action(
                    tr(cx, "menu.file.close_editor", "Close Editor"),
                    workspace::CloseActiveItem {
                        save_intent: None,
                        close_pinned: true,
                    },
                ),
                MenuItem::action(
                    tr(cx, "menu.file.close_project", "Close Project"),
                    workspace::CloseProject,
                ),
                MenuItem::action(
                    tr(cx, "menu.file.close_window", "Close Window"),
                    workspace::CloseWindow,
                ),
            ],
        },
        Menu {
            name: tr(cx, "menu.edit", "Edit").into(),
            disabled: false,
            items: vec![
                MenuItem::os_action(
                    tr(cx, "menu.edit.undo", "Undo"),
                    editor::actions::Undo,
                    OsAction::Undo,
                ),
                MenuItem::os_action(
                    tr(cx, "menu.edit.redo", "Redo"),
                    editor::actions::Redo,
                    OsAction::Redo,
                ),
                MenuItem::separator(),
                MenuItem::os_action(
                    tr(cx, "menu.edit.cut", "Cut"),
                    editor::actions::Cut,
                    OsAction::Cut,
                ),
                MenuItem::os_action(
                    tr(cx, "menu.edit.copy", "Copy"),
                    editor::actions::Copy,
                    OsAction::Copy,
                ),
                MenuItem::action(
                    tr(cx, "menu.edit.copy_trim", "Copy and Trim"),
                    editor::actions::CopyAndTrim,
                ),
                MenuItem::os_action(
                    tr(cx, "menu.edit.paste", "Paste"),
                    editor::actions::Paste,
                    OsAction::Paste,
                ),
                MenuItem::separator(),
                MenuItem::action(
                    tr(cx, "menu.edit.find", "Find"),
                    search::buffer_search::Deploy::find(),
                ),
                MenuItem::action(
                    tr(cx, "menu.edit.find_project", "Find in Project"),
                    workspace::DeploySearch::default(),
                ),
                MenuItem::separator(),
                MenuItem::action(
                    tr(cx, "menu.edit.toggle_line_comment", "Toggle Line Comment"),
                    editor::actions::ToggleComments::default(),
                ),
            ],
        },
        Menu {
            name: tr(cx, "menu.selection", "Selection").into(),
            disabled: false,
            items: vec![
                MenuItem::os_action(
                    tr(cx, "menu.selection.select_all", "Select All"),
                    editor::actions::SelectAll,
                    OsAction::SelectAll,
                ),
                MenuItem::action(
                    tr(cx, "menu.selection.expand", "Expand Selection"),
                    editor::actions::SelectLargerSyntaxNode,
                ),
                MenuItem::action(
                    tr(cx, "menu.selection.shrink", "Shrink Selection"),
                    editor::actions::SelectSmallerSyntaxNode,
                ),
                MenuItem::action(
                    tr(cx, "menu.selection.next_sibling", "Select Next Sibling"),
                    editor::actions::SelectNextSyntaxNode,
                ),
                MenuItem::action(
                    tr(
                        cx,
                        "menu.selection.previous_sibling",
                        "Select Previous Sibling",
                    ),
                    editor::actions::SelectPreviousSyntaxNode,
                ),
                MenuItem::separator(),
                MenuItem::action(
                    tr(cx, "menu.selection.add_cursor_above", "Add Cursor Above"),
                    editor::actions::AddSelectionAbove {
                        skip_soft_wrap: true,
                    },
                ),
                MenuItem::action(
                    tr(cx, "menu.selection.add_cursor_below", "Add Cursor Below"),
                    editor::actions::AddSelectionBelow {
                        skip_soft_wrap: true,
                    },
                ),
                MenuItem::action(
                    tr(
                        cx,
                        "menu.selection.next_occurrence",
                        "Select Next Occurrence",
                    ),
                    editor::actions::SelectNext {
                        replace_newest: false,
                    },
                ),
                MenuItem::action(
                    tr(
                        cx,
                        "menu.selection.previous_occurrence",
                        "Select Previous Occurrence",
                    ),
                    editor::actions::SelectPrevious {
                        replace_newest: false,
                    },
                ),
                MenuItem::action(
                    tr(
                        cx,
                        "menu.selection.all_occurrences",
                        "Select All Occurrences",
                    ),
                    editor::actions::SelectAllMatches,
                ),
                MenuItem::separator(),
                MenuItem::action(
                    tr(cx, "menu.selection.move_line_up", "Move Line Up"),
                    editor::actions::MoveLineUp,
                ),
                MenuItem::action(
                    tr(cx, "menu.selection.move_line_down", "Move Line Down"),
                    editor::actions::MoveLineDown,
                ),
                MenuItem::action(
                    tr(cx, "menu.selection.duplicate", "Duplicate Selection"),
                    editor::actions::DuplicateLineDown,
                ),
            ],
        },
        Menu {
            name: tr(cx, "menu.view", "View").into(),
            disabled: false,
            items: view_items,
        },
        Menu {
            name: tr(cx, "menu.go", "Go").into(),
            disabled: false,
            items: vec![
                MenuItem::action(tr(cx, "menu.go.back", "Back"), workspace::GoBack),
                MenuItem::action(tr(cx, "menu.go.forward", "Forward"), workspace::GoForward),
                MenuItem::separator(),
                MenuItem::action(
                    tr(cx, "menu.go.command_palette", "Command Palette..."),
                    zed_actions::command_palette::Toggle,
                ),
                MenuItem::separator(),
                MenuItem::action(
                    tr(cx, "menu.go.go_to_file", "Go to File..."),
                    workspace::ToggleFileFinder::default(),
                ),
                // MenuItem::action("Go to Symbol in Project", project_symbols::Toggle),
                MenuItem::action(
                    tr(
                        cx,
                        "menu.go.go_to_symbol_in_editor",
                        "Go to Symbol in Editor...",
                    ),
                    zed_actions::outline::ToggleOutline,
                ),
                MenuItem::action(
                    tr(cx, "menu.go.go_to_line_column", "Go to Line/Column..."),
                    editor::actions::ToggleGoToLine,
                ),
                MenuItem::separator(),
                MenuItem::action(
                    tr(cx, "menu.go.go_to_definition", "Go to Definition"),
                    editor::actions::GoToDefinition,
                ),
                MenuItem::action(
                    tr(cx, "menu.go.go_to_declaration", "Go to Declaration"),
                    editor::actions::GoToDeclaration,
                ),
                MenuItem::action(
                    tr(cx, "menu.go.go_to_type_definition", "Go to Type Definition"),
                    editor::actions::GoToTypeDefinition,
                ),
                MenuItem::action(
                    tr(cx, "menu.go.find_all_references", "Find All References"),
                    editor::actions::FindAllReferences::default(),
                ),
                MenuItem::separator(),
                MenuItem::action(
                    tr(cx, "menu.go.next_problem", "Next Problem"),
                    editor::actions::GoToDiagnostic::default(),
                ),
                MenuItem::action(
                    tr(cx, "menu.go.previous_problem", "Previous Problem"),
                    editor::actions::GoToPreviousDiagnostic::default(),
                ),
            ],
        },
        Menu {
            name: tr(cx, "menu.run", "Run").into(),
            disabled: false,
            items: vec![
                MenuItem::action(
                    tr(cx, "menu.run.spawn_task", "Spawn Task"),
                    zed_actions::Spawn::ViaModal {
                        reveal_target: None,
                    },
                ),
                MenuItem::action(
                    tr(cx, "menu.run.start_debugger", "Start Debugger"),
                    debugger_ui::Start,
                ),
                MenuItem::separator(),
                MenuItem::action(
                    tr(cx, "menu.run.edit_tasks_json", "Edit tasks.json..."),
                    crate::zed::OpenProjectTasks,
                ),
                MenuItem::action(
                    tr(cx, "menu.run.edit_debug_json", "Edit debug.json..."),
                    zed_actions::OpenProjectDebugTasks,
                ),
                MenuItem::separator(),
                MenuItem::action(
                    tr(cx, "menu.run.continue", "Continue"),
                    debugger_ui::Continue,
                ),
                MenuItem::action(
                    tr(cx, "menu.run.step_over", "Step Over"),
                    debugger_ui::StepOver,
                ),
                MenuItem::action(
                    tr(cx, "menu.run.step_into", "Step Into"),
                    debugger_ui::StepInto,
                ),
                MenuItem::action(
                    tr(cx, "menu.run.step_out", "Step Out"),
                    debugger_ui::StepOut,
                ),
                MenuItem::separator(),
                MenuItem::action(
                    tr(cx, "menu.run.toggle_breakpoint", "Toggle Breakpoint"),
                    editor::actions::ToggleBreakpoint,
                ),
                MenuItem::action(
                    tr(cx, "menu.run.edit_breakpoint", "Edit Breakpoint"),
                    editor::actions::EditLogBreakpoint,
                ),
                MenuItem::action(
                    tr(
                        cx,
                        "menu.run.clear_all_breakpoints",
                        "Clear All Breakpoints",
                    ),
                    debugger_ui::ClearAllBreakpoints,
                ),
            ],
        },
        Menu {
            name: tr(cx, "menu.window", "Window").into(),
            disabled: false,
            items: vec![
                MenuItem::action(tr(cx, "menu.window.minimize", "Minimize"), super::Minimize),
                MenuItem::action(tr(cx, "menu.window.zoom", "Zoom"), super::Zoom),
                MenuItem::separator(),
            ],
        },
        Menu {
            name: tr(cx, "menu.help", "Help").into(),
            disabled: false,
            items: vec![
                MenuItem::action(
                    tr(
                        cx,
                        "menu.help.view_dependency_licenses",
                        "View Dependency Licenses",
                    ),
                    zed_actions::OpenLicenses,
                ),
                MenuItem::action(
                    tr(cx, "menu.help.show_welcome", "Show Welcome"),
                    onboarding::ShowWelcome,
                ),
                MenuItem::separator(),
                MenuItem::action(
                    tr(cx, "menu.help.file_bug_report", "File Bug Report..."),
                    zed_actions::feedback::FileBugReport,
                ),
                MenuItem::action(
                    tr(cx, "menu.help.request_feature", "Request Feature..."),
                    zed_actions::feedback::RequestFeature,
                ),
                MenuItem::separator(),
                MenuItem::action(
                    tr(cx, "menu.help.zed_repository", "ZZZ Repository"),
                    feedback::OpenZedRepo,
                ),
            ],
        },
    ]
}
