use anyhow::Context as _;
use gpui::App;
use i18n::tr;

use git::repository::{Remote, RemoteCommandOutput};
use ui::SharedString;
use util::ResultExt as _;

/// Whether a pull request link printed by the remote points at a page for
/// creating a new pull request or at an already existing one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PullRequestLinkKind {
    Create,
    View,
}

struct PullRequestHint {
    needle: &'static str,
    label_key: &'static str,
    label_fallback: &'static str,
    kind: PullRequestLinkKind,
}

const PULL_REQUEST_HINTS: &[PullRequestHint] = &[
    // GitHub: "Create a pull request for 'branch' on GitHub by visiting:"
    PullRequestHint {
        needle: "Create a pull request",
        label_key: "git_ui.remote_output.create_pull_request",
        label_fallback: "Create Pull Request",
        kind: PullRequestLinkKind::Create,
    },
    // Bitbucket: "Create pull request for branch:"
    PullRequestHint {
        needle: "Create pull request",
        label_key: "git_ui.remote_output.create_pull_request",
        label_fallback: "Create Pull Request",
        kind: PullRequestLinkKind::Create,
    },
    // GitLab: "To create a merge request for branch, visit:"
    PullRequestHint {
        needle: "create a merge request",
        label_key: "git_ui.remote_output.create_merge_request",
        label_fallback: "Create Merge Request",
        kind: PullRequestLinkKind::Create,
    },
    // GitLab: "View merge request for branch:"
    PullRequestHint {
        needle: "View merge request",
        label_key: "git_ui.remote_output.view_merge_request",
        label_fallback: "View Merge Request",
        kind: PullRequestLinkKind::View,
    },
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
    ToastWithLog {
        output: RemoteCommandOutput,
    },
    /// The push created a branch for which a new pull request can be opened.
    /// The URL is built from the git hosting provider, the same way the
    /// `git: create pull request` command does, and `fallback_url` (the link
    /// printed by the remote) is only used if that fails.
    CreatePullRequest {
        label: String,
        fallback_url: String,
    },
    /// The push references an existing pull request, which can only be reached
    /// via the link printed by the remote.
    OpenPullRequest {
        label: String,
        url: String,
    },
}

pub struct SuccessMessage {
    pub message: String,
    pub style: SuccessStyle,
}

#[derive(Debug, PartialEq, Eq)]
struct PullRequestLink {
    label: String,
    url: String,
    kind: PullRequestLinkKind,
}

fn extract_pull_request_link(
    output: &RemoteCommandOutput,
    translate: impl Fn(&'static str, &'static str) -> String + Copy,
) -> Option<PullRequestLink> {
    let mut pending_hint: Option<&'static PullRequestHint> = None;

    for line in output.stderr.lines() {
        let Some(remote_line) = line.trim_start().strip_prefix("remote:") else {
            pending_hint = None;
            continue;
        };

        if let Some(hint) = PULL_REQUEST_HINTS
            .iter()
            .find(|hint| remote_line.contains(hint.needle))
        {
            pending_hint = Some(hint);
        }

        if let Some(url) = extract_url(remote_line)
            && let Some(hint) = pending_hint
        {
            return Some(PullRequestLink {
                label: translate(hint.label_key, hint.label_fallback),
                url,
                kind: hint.kind,
            });
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

/// Number of files git reported changing, parsed from the summary line at the
/// end of a `pull`'s stdout (for example `"3 files changed, 10 insertions(+)"`).
fn changed_file_count(output: &RemoteCommandOutput) -> anyhow::Result<u32> {
    let last_line = output
        .stdout
        .lines()
        .last()
        .context("Failed to get last line of output")?
        .trim();

    last_line
        .split_whitespace()
        .next()
        .context("Failed to get first word of last line")?
        .parse()
        .context("Failed to parse the number of changed files")
}

/// Wording used to report how many files a pull changed. `no_count` is used
/// when git's output carries no summary line.
#[derive(Clone, Copy)]
struct ChangeSummaryKeys {
    one_file: (&'static str, &'static str),
    many_files: (&'static str, &'static str),
    no_count: (&'static str, &'static str),
}

fn summarize_changes(
    output: &RemoteCommandOutput,
    remote_name: &str,
    keys: ChangeSummaryKeys,
    translate: impl Fn(&'static str, &'static str) -> String + Copy,
) -> String {
    match changed_file_count(output).log_err() {
        Some(1) => tr_args_with(
            translate,
            keys.one_file.0,
            keys.one_file.1,
            "1",
            remote_name,
        ),
        Some(files_changed) => {
            let files_changed = files_changed.to_string();
            tr_args_with(
                translate,
                keys.many_files.0,
                keys.many_files.1,
                &files_changed,
                remote_name,
            )
        }
        None => tr_arg_with(translate, keys.no_count.0, keys.no_count.1, remote_name),
    }
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
            if output.stdout.ends_with("Already up to date.\n") {
                SuccessMessage {
                    message: translate(
                        "git_ui.remote_output.pull_already_up_to_date",
                        "Pull: Already up to date",
                    ),
                    style: SuccessStyle::Toast,
                }
            } else if output.stdout.starts_with("Updating") {
                let message = summarize_changes(
                    &output,
                    &remote_ref.name,
                    ChangeSummaryKeys {
                        one_file: (
                            "git_ui.remote_output.received_one_file_change_from",
                            "Received {} file change from {}",
                        ),
                        many_files: (
                            "git_ui.remote_output.received_many_file_changes_from",
                            "Received {} file changes from {}",
                        ),
                        no_count: (
                            "git_ui.remote_output.fast_forwarded_from",
                            "Fast forwarded from {}",
                        ),
                    },
                    translate,
                );
                SuccessMessage {
                    message,
                    style: SuccessStyle::ToastWithLog { output },
                }
            } else if output.stdout.starts_with("Merge") {
                let message = summarize_changes(
                    &output,
                    &remote_ref.name,
                    ChangeSummaryKeys {
                        one_file: (
                            "git_ui.remote_output.merged_one_file_change_from",
                            "Merged {} file change from {}",
                        ),
                        many_files: (
                            "git_ui.remote_output.merged_many_file_changes_from",
                            "Merged {} file changes from {}",
                        ),
                        no_count: ("git_ui.remote_output.merged_from", "Merged from {}"),
                    },
                    translate,
                );
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
                if let Some(PullRequestLink { label, url, kind }) =
                    extract_pull_request_link(&output, translate)
                {
                    let style = match kind {
                        PullRequestLinkKind::Create => SuccessStyle::CreatePullRequest {
                            label,
                            fallback_url: url,
                        },
                        PullRequestLinkKind::View => SuccessStyle::OpenPullRequest { label, url },
                    };
                    SuccessMessage { message, style }
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

        if let SuccessStyle::CreatePullRequest {
            label,
            fallback_url,
        } = msg.style
        {
            assert_eq!(msg.message, "Pushed test_branch to test_remote");
            assert_eq!(label, "Create Pull Request");
            assert_eq!(fallback_url, "https://example.com/test/test/pull/new/test");
        } else {
            panic!("Expected CreatePullRequest variant");
        }
    }

    #[test]
    fn test_push_new_branch_bitbucket_pull_request() {
        let output = RemoteCommandOutput {
            stdout: String::new(),
            stderr: indoc! {"
                remote:
                remote: Create pull request for test:
                remote:   https://bitbucket.example.com/projects/TEST/repos/test/pull-requests?create&sourceBranch=refs/heads/test
                "}
            .to_string(),
        };

        assert_eq!(
            extract_pull_request_link(&output, |_, fallback| fallback.to_owned()),
            Some(PullRequestLink {
                label: "Create Pull Request".to_string(),
                url: "https://bitbucket.example.com/projects/TEST/repos/test/pull-requests?create&sourceBranch=refs/heads/test".to_string(),
                kind: PullRequestLinkKind::Create,
            })
        );
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

        if let SuccessStyle::CreatePullRequest {
            label,
            fallback_url,
        } = msg.style
        {
            assert_eq!(msg.message, "Pushed test_branch to test_remote");
            assert_eq!(label, "Create Merge Request");
            assert_eq!(
                fallback_url,
                "https://example.com/test/test/-/merge_requests/new?merge_request%5Bsource_branch%5D=test"
            )
        } else {
            panic!("Expected CreatePullRequest variant")
        }
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

        if let SuccessStyle::OpenPullRequest { label, url } = msg.style {
            assert_eq!(msg.message, "Pushed test_branch to test_remote");
            assert_eq!(label, "View Merge Request");
            assert_eq!(url, "https://example.com/test/test/-/merge_requests/99999");
        } else {
            panic!("Expected OpenPullRequest variant")
        }
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
            assert_eq!(
                extract_pull_request_link(output, |_, fallback| fallback.to_owned()),
                None
            );
        } else {
            panic!("Expected ToastWithLog variant");
        }
    }
}
