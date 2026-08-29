use extension_host::ExtensionStore;
use gpui::{App, ClipboardItem, PromptLevel, actions};
use system_specs::{CopySystemSpecsIntoClipboard, SystemSpecs};
use util::ResultExt;
use workspace::Workspace;
use zed_actions::feedback::{FileBugReport, RequestFeature};

actions!(
    zed,
    [
        /// Opens the Zed repository on GitHub.
        OpenZedRepo,
        /// Copies installed extensions to the clipboard for bug reports.
        CopyInstalledExtensionsIntoClipboard
    ]
);

const ZED_REPO_URL: &str = "https://codeberg.org/ZZZEditor/ZZZ";

const REQUEST_FEATURE_URL: &str = "https://codeberg.org/ZZZEditor/ZZZ/issues/new";

fn file_bug_report_url(specs: &SystemSpecs) -> String {
    format!(
        concat!(
            "https://codeberg.org/ZZZEditor/ZZZ/issues/new",
            "?",
            "body=Bug%20report%0A%0AEnvironment%3A%20{}"
        ),
        urlencoding::encode(&specs.to_string())
    )
}

pub fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, _, _| {
        workspace
            .register_action(|_, _: &CopySystemSpecsIntoClipboard, window, cx| {
                let specs = SystemSpecs::new(window, cx);
                let copied_into_clipboard = i18n::tr(
                    cx,
                    "auto.feedback.feedback.prompt.title.copied.into.clipboard",
                    "Copied into clipboard",
                );
                let ok = i18n::tr(cx, "auto.feedback.feedback.prompt_button.ok", "OK");

                cx.spawn_in(window, async move |_, cx| {
                    let specs = specs.await.to_string();

                    cx.update(|_, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(specs.clone()))
                    })
                    .log_err();

                    cx.prompt(
                        PromptLevel::Info,
                        &copied_into_clipboard,
                        Some(&specs),
                        &[ok.as_str()],
                    )
                    .await
                })
                .detach();
            })
            .register_action(|_, _: &CopyInstalledExtensionsIntoClipboard, window, cx| {
                let clipboard_text = format_installed_extensions_for_clipboard(cx);
                cx.write_to_clipboard(ClipboardItem::new_string(clipboard_text.clone()));
                drop(window.prompt(
                    PromptLevel::Info,
                    &i18n::tr(
                        cx,
                        "auto.feedback.feedback.prompt.title.copied.into.clipboard",
                        "Copied into clipboard",
                    ),
                    Some(&clipboard_text),
                    &[gpui::PromptButton::ok(i18n::tr(
                        cx,
                        "auto.feedback.feedback.prompt_button.ok",
                        "OK",
                    ))],
                    cx,
                ));
            })
            .register_action(|_, _: &RequestFeature, _, cx| {
                cx.open_url(REQUEST_FEATURE_URL);
            })
            .register_action(move |_, _: &FileBugReport, window, cx| {
                let specs = SystemSpecs::new(window, cx);
                cx.spawn_in(window, async move |_, cx| {
                    let specs = specs.await;
                    cx.update(|_, cx| {
                        cx.open_url(&file_bug_report_url(&specs));
                    })
                    .log_err();
                })
                .detach();
            })
            .register_action(move |_, _: &OpenZedRepo, _, cx| {
                cx.open_url(ZED_REPO_URL);
            });
    })
    .detach();
}

fn format_installed_extensions_for_clipboard(cx: &mut App) -> String {
    let store = ExtensionStore::global(cx);
    let store = store.read(cx);
    let mut lines = Vec::with_capacity(store.extension_index.extensions.len());

    for (extension_id, entry) in store.extension_index.extensions.iter() {
        let line = format!(
            "- {} ({}) v{}{}",
            entry.manifest.name,
            extension_id,
            entry.manifest.version,
            if entry.dev { " (dev)" } else { "" }
        );
        lines.push(line);
    }

    lines.sort();

    if lines.is_empty() {
        return "No extensions installed.".to_owned();
    }

    format!(
        "Installed extensions ({}):\n{}",
        lines.len(),
        lines.join("\n")
    )
}

#[cfg(test)]
mod tests {
    use release_channel::{AppCommitSha, ReleaseChannel};
    use semver::Version;

    use super::file_bug_report_url;

    fn sample_specs() -> system_specs::SystemSpecs {
        system_specs::SystemSpecs::new_stateless(
            Version::new(1, 2, 3),
            Some(AppCommitSha::new("abcdef0".to_string())),
            ReleaseChannel::Dev,
        )
    }

    #[test]
    fn file_bug_report_url_encodes_system_specs_in_environment_field() {
        let specs = sample_specs();

        let url = file_bug_report_url(&specs);

        assert!(url.starts_with("https://codeberg.org/ZZZEditor/ZZZ/issues/new?"));
        assert!(url.contains("body=Bug%20report"));

        let encoded_environment = url.split("Environment%3A%20").nth(1).unwrap();
        let decoded_environment = urlencoding::decode(encoded_environment).unwrap();
        let rendered_specs = specs.to_string();

        assert_eq!(decoded_environment, rendered_specs);
    }
}
