use gpui::{App, Context, Window, actions};
use paths::home_dir;
use util::ResultExt as _;
use workspace::{Toast, Workspace, notifications::NotificationId};

use crate::{TwinOutcome, ensure_twin};

actions!(
    idea_import,
    [
        /// Imports JetBrains `.idea` run configurations into `.ZZZ/debug.json`.
        ImportIdeaProject,
    ]
);

pub fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, _window, _cx| {
        workspace.register_action(|workspace, _: &ImportIdeaProject, window, cx| {
            import_project(workspace, window, cx);
        });
    })
    .detach();
}

fn import_project(workspace: &mut Workspace, _window: &mut Window, cx: &mut Context<Workspace>) {
    let Some(worktree) = workspace
        .project()
        .read(cx)
        .worktrees(cx)
        .find(|worktree| worktree.read(cx).as_local().is_some())
    else {
        workspace.show_error(
            &anyhow::anyhow!("Importing `.idea` requires a local project."),
            cx,
        );
        return;
    };

    let project_directory = worktree.read(cx).abs_path().as_ref().to_path_buf();
    let fs = workspace.app_state().fs.clone();
    let user_home = home_dir().to_str().map(str::to_owned);
    let workspace_handle = workspace.weak_handle();

    cx.spawn(async move |_workspace, cx| {
        let result = ensure_twin(fs.as_ref(), &project_directory, "", user_home).await;
        workspace_handle
            .update(cx, |workspace, cx| match result {
                Ok(TwinOutcome::Created(report)) => {
                    let message = format!(
                        "Imported {} `.idea` run configuration(s); {} skipped. Report: .ZZZ/idea-import-report.md",
                        report.imported.len(),
                        report.skipped.len()
                    );
                    workspace.show_toast(Toast::new(NotificationId::unique::<TwinOutcome>(), message), cx);
                }
                Ok(TwinOutcome::AlreadyExists) => {
                    workspace.show_toast(
                        Toast::new(
                            NotificationId::unique::<TwinOutcome>(),
                            "`.ZZZ/debug.json` already exists; it was left untouched.",
                        ),
                        cx,
                    );
                }
                Ok(TwinOutcome::NoIdeaFolder) => {
                    workspace.show_toast(
                        Toast::new(
                            NotificationId::unique::<TwinOutcome>(),
                            "No `.idea` folder was found in the project root.",
                        ),
                        cx,
                    );
                }
                Err(error) => workspace.show_error(&error, cx),
            })
            .log_err();
    })
    .detach();
}
