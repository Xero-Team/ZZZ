use anyhow::Context as _;
use gpui::App;
use i18n::tr;

use git::repository::{Remote, RemoteCommandOutput};
use ui::SharedString;
use util::ResultExt as _;

const PULL_REQUEST_HINTS: &[(&str, &str, &str)] = &[
    (
        "Create a pull request",
        "git_ui.remote_output.create_pull_request",
        "Create Pull Request",
    ),
    (
        "Create pull request",
        "git_ui.remote_output.create_pull_request",
        "Create Pull Request",
    ),
    (
        "create a merge request",
        "git_ui.remote_output.create_merge_request",
        "Create Merge Request",
    ),
    (
        "View merge request",
        "git_ui.remote_output.view_merge_request",
        "View Merge Request",
    ),
];

#[derive(Clone)]
pub enum RemoteAction {
    Fetch(Option<Remote>),
    Pull(Remote),
    Push(SharedString, Remote),
}

impl RemoteAction {
    pub fn name(&self) -> &'static str {
        match self {
            RemoteAction::Fetch(_) => "fetch",
            RemoteAction::Pull(_) => "pull",
            RemoteAction::Push(_, _) => "push",
        }
    }
}

pub enum SuccessStyle {
    Toast,
    ToastWithLog { output: RemoteCommandOutput },
    PushPullRequestLink { label: String, url: String },
}

pub struct SuccessMessage {
    pub message: String,
    pub style: SuccessStyle,
}

fn extract_pull_request_link(
    output: &RemoteCommandOutput,
    translate: impl Fn(&'static str, &'static str) -> String + Copy,
) -> Option<(String, String)> {
    let mut pending_label = None;

    for line in output.stderr.lines() {
        let Some(remote_line) = line.trim_start().strip_prefix("remote:") else {
            pending_label = None;
            continue;
        };

        if let Some((_, key, fallback)) = PULL_REQUEST_HINTS
            .iter()
            .find(|(hint, _, _)| remote_line.contains(hint))
        {
            pending_label = Some(translate(key, fallback));
        }

        if let Some(url) = extract_url(remote_line)
            && let Some(label) = pending_label.as_ref()
        {
            return Some((label.clone(), url));
        }
    }

    None
}

fn extract_url(line: &str) -> Option<String> {
    let http_index = line.find("https://").or_else(|| line.find("http://"))?;
    let url = line[http_index..]
        .split_whitespace()
        .next()?
        .trim_end_matches(|character| matches!(character, ',' | '.' | ')' | ']' | '>'));

    Some(url.to_string())
}

fn tr_arg_with(
    translate: impl Fn(&'static str, &'static str) -> String,
    key: &'static str,
    fallback: &'static str,
    value: &str,
) -> String {
    translate(key, fallback).replacen("{}", value, 1)
}

fn tr_args_with(
    translate: impl Fn(&'static str, &'static str) -> String,
    key: &'static str,
    fallback: &'static str,
    first: &str,
    second: &str,
) -> String {
    translate(key, fallback)
        .replacen("{}", first, 1)
        .replacen("{}", second, 1)
}

fn format_output_impl(
    action: &RemoteAction,
    output: RemoteCommandOutput,
    translate: impl Fn(&'static str, &'static str) -> String + Copy,
) -> SuccessMessage {
    match action {
        RemoteAction::Fetch(remote) => {
            if output.stderr.is_empty() {
                SuccessMessage {
                    message: translate(
                        "git_ui.remote_output.fetch_already_up_to_date",
                        "Fetch: Already up to date",
                    ),
                    style: SuccessStyle::Toast,
                }
            } else {
                let message = match remote {
                    Some(remote) => tr_arg_with(
                        translate,
                        "git_ui.remote_output.synchronized_with",
                        "Synchronized with {}",
                        &remote.name,
                    ),
                    None => translate(
                        "git_ui.remote_output.synchronized_with_remotes",
                        "Synchronized with remotes",
                    ),
                };
                SuccessMessage {
                    message,
                    style: SuccessStyle::ToastWithLog { output },
                }
            }
        }
        RemoteAction::Pull(remote_ref) => {
            let get_changes = |output: &RemoteCommandOutput| -> anyhow::Result<u32> {
                let last_line = output
                    .stdout
                    .lines()
                    .last()
                    .context("Failed to get last line of output")?
                    .trim();

                let files_changed = last_line
                    .split_whitespace()
                    .next()
                    .context("Failed to get first word of last line")?
                    .parse()?;

                Ok(files_changed)
            };
            if output.stdout.ends_with("Already up to date.\n") {
                SuccessMessage {
                    message: translate(
                        "git_ui.remote_output.pull_already_up_to_date",
                        "Pull: Already up to date",
                    ),
                    style: SuccessStyle::Toast,
                }
            } else if output.stdout.starts_with("Updating") {
                let files_changed = get_changes(&output).log_err();
                let message = if let Some(files_changed) = files_changed {
                    let files_changed = files_changed.to_string();
                    if files_changed == "1" {
                        tr_args_with(
                            translate,
                            "git_ui.remote_output.received_one_file_change_from",
                            "Received {} file change from {}",
                            &files_changed,
                            &remote_ref.name,
                        )
                    } else {
                        tr_args_with(
                            translate,
                            "git_ui.remote_output.received_many_file_changes_from",
                            "Received {} file changes from {}",
                            &files_changed,
                            &remote_ref.name,
                        )
                    }
                } else {
                    tr_arg_with(
                        translate,
                        "git_ui.remote_output.fast_forwarded_from",
                        "Fast forwarded from {}",
                        &remote_ref.name,
                    )
                };
                SuccessMessage {
                    message,
                    style: SuccessStyle::ToastWithLog { output },
                }
            } else if output.stdout.starts_with("Merge") {
                let files_changed = get_changes(&output).log_err();
                let message = if let Some(files_changed) = files_changed {
                    let files_changed = files_changed.to_string();
                    if files_changed == "1" {
                        tr_args_with(
                            translate,
                            "git_ui.remote_output.merged_one_file_change_from",
                            "Merged {} file change from {}",
                            &files_changed,
                            &remote_ref.name,
                        )
                    } else {
                        tr_args_with(
                            translate,
                            "git_ui.remote_output.merged_many_file_changes_from",
                            "Merged {} file changes from {}",
                            &files_changed,
                            &remote_ref.name,
                        )
                    }
                } else {
                    tr_arg_with(
                        translate,
                        "git_ui.remote_output.merged_from",
                        "Merged from {}",
                        &remote_ref.name,
                    )
                };
                SuccessMessage {
                    message,
                    style: SuccessStyle::ToastWithLog { output },
                }
            } else if output.stdout.contains("Successfully rebased") {
                SuccessMessage {
                    message: tr_arg_with(
                        translate,
                        "git_ui.remote_output.successfully_rebased_from",
                        "Successfully rebased from {}",
                        &remote_ref.name,
                    ),
                    style: SuccessStyle::ToastWithLog { output },
                }
            } else {
                SuccessMessage {
                    message: tr_arg_with(
                        translate,
                        "git_ui.remote_output.successfully_pulled_from",
                        "Successfully pulled from {}",
                        &remote_ref.name,
                    ),
                    style: SuccessStyle::ToastWithLog { output },
                }
            }
        }
        RemoteAction::Push(branch_name, remote_ref) => {
            if output.stderr.ends_with("Everything up-to-date\n") {
                SuccessMessage {
                    message: translate(
                        "git_ui.remote_output.push_everything_up_to_date",
                        "Push: Everything is up-to-date",
                    ),
                    style: SuccessStyle::Toast,
                }
            } else {
                let message = tr_args_with(
                    translate,
                    "git_ui.remote_output.pushed_to",
                    "Pushed {} to {}",
                    branch_name,
                    &remote_ref.name,
                );
                if let Some((label, url)) = extract_pull_request_link(&output, translate) {
                    SuccessMessage {
                        message,
                        style: SuccessStyle::PushPullRequestLink { label, url },
                    }
                } else {
                    SuccessMessage {
                        message,
                        style: SuccessStyle::ToastWithLog { output },
                    }
                }
            }
        }
    }
}

#[cfg(test)]
pub fn format_output(action: &RemoteAction, output: RemoteCommandOutput) -> SuccessMessage {
    format_output_impl(action, output, |_, fallback| fallback.to_owned())
}

pub fn format_output_localized(
    action: &RemoteAction,
    output: RemoteCommandOutput,
    cx: &App,
) -> SuccessMessage {
    format_output_impl(action, output, |key, fallback| tr(cx, key, fallback))
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;

    #[test]
    fn test_push_new_branch_pull_request() {
        let action = RemoteAction::Push(
            SharedString::new_static("test_branch"),
            Remote {
                name: SharedString::new_static("test_remote"),
            },
        );

        let output = RemoteCommandOutput {
            stdout: String::new(),
            stderr: indoc! { "
                Total 0 (delta 0), reused 0 (delta 0), pack-reused 0 (from 0)
                remote:
                remote: Create a pull request for 'test' on GitHub by visiting:
                remote:      https://example.com/test/test/pull/new/test
                remote:
                To example.com:test/test.git
                 * [new branch]      test -> test
                "}
            .to_string(),
        };

        let msg = format_output(&action, output);

        assert!(matches!(
            msg.style,
            SuccessStyle::PushPullRequestLink { ref label, ref url }
                if label == "Create Pull Request"
                    && url == "https://example.com/test/test/pull/new/test"
        ));
        assert_eq!(msg.message, "Pushed test_branch to test_remote");
    }

    #[test]
    fn test_push_new_branch_bitbucket_pull_request() {
        let action = RemoteAction::Push(
            SharedString::new_static("test_branch"),
            Remote {
                name: SharedString::new_static("test_remote"),
            },
        );

        let output = RemoteCommandOutput {
            stdout: String::new(),
            stderr: indoc! {"
                remote:
                remote: Create pull request for test:
                remote:   https://bitbucket.example.com/projects/TEST/repos/test/pull-requests?create&sourceBranch=refs/heads/test
                "}
            .to_string(),
        };

        let msg = format_output(&action, output);

        assert!(matches!(
            msg.style,
            SuccessStyle::PushPullRequestLink { ref label, ref url }
                if label == "Create Pull Request"
                    && url == "https://bitbucket.example.com/projects/TEST/repos/test/pull-requests?create&sourceBranch=refs/heads/test"
        ));
    }

    #[test]
    fn test_push_new_branch_merge_request() {
        let action = RemoteAction::Push(
            SharedString::new_static("test_branch"),
            Remote {
                name: SharedString::new_static("test_remote"),
            },
        );

        let output = RemoteCommandOutput {
            stdout: String::new(),
            stderr: indoc! {"
                Total 0 (delta 0), reused 0 (delta 0), pack-reused 0 (from 0)
                remote:
                remote: To create a merge request for test, visit:
                remote:   https://example.com/test/test/-/merge_requests/new?merge_request%5Bsource_branch%5D=test
                remote:
                To example.com:test/test.git
                 * [new branch]      test -> test
                "}
            .to_string()
            };

        let msg = format_output(&action, output);

        assert!(matches!(
            msg.style,
            SuccessStyle::PushPullRequestLink { ref label, ref url }
                if label == "Create Merge Request"
                    && url == "https://example.com/test/test/-/merge_requests/new?merge_request%5Bsource_branch%5D=test"
        ));
        assert_eq!(msg.message, "Pushed test_branch to test_remote");
    }

    #[test]
    fn test_push_branch_existing_merge_request() {
        let action = RemoteAction::Push(
            SharedString::new_static("test_branch"),
            Remote {
                name: SharedString::new_static("test_remote"),
            },
        );

        let output = RemoteCommandOutput {
            stdout: String::new(),
            // Simulate an extraneous link that should not be found in top 3 lines
            stderr: indoc! {"
                ** WARNING: connection is not using a post-quantum key exchange algorithm.
                ** This session may be vulnerable to \"store now, decrypt later\" attacks.
                ** The server may need to be upgraded. See https://openssh.com/pq.html
                Total 0 (delta 0), reused 0 (delta 0), pack-reused 0 (from 0)
                remote:
                remote: View merge request for test:
                remote:    https://example.com/test/test/-/merge_requests/99999
                remote:
                To example.com:test/test.git
                    + 80bd3c83be...e03d499d2e test -> test
                "}
            .to_string(),
        };

        let msg = format_output(&action, output);

        assert!(matches!(
            msg.style,
            SuccessStyle::PushPullRequestLink { ref label, ref url }
                if label == "View Merge Request"
                    && url == "https://example.com/test/test/-/merge_requests/99999"
        ));
        assert_eq!(msg.message, "Pushed test_branch to test_remote");
    }

    #[test]
    fn test_push_new_branch_no_link() {
        let action = RemoteAction::Push(
            SharedString::new_static("test_branch"),
            Remote {
                name: SharedString::new_static("test_remote"),
            },
        );

        let output = RemoteCommandOutput {
            stdout: String::new(),
            stderr: indoc! { "
                To http://example.com/test/test.git
                 * [new branch]      test -> test
                ",
            }
            .to_string(),
        };

        let msg = format_output(&action, output);

        if let SuccessStyle::ToastWithLog { output } = &msg.style {
            assert_eq!(
                output.stderr,
                "To http://example.com/test/test.git\n * [new branch]      test -> test\n"
            );
        } else {
            panic!("Expected ToastWithLog variant");
        }
    }
}
