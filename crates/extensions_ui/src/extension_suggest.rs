use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use db::kvp::KeyValueStore;
use editor::Editor;
use extension_host::{ExtensionSettings, ExtensionStore};
use gpui::{App, AppContext as _, Context, Entity, SharedString, Window};
use i18n::tr;
use language::{Buffer, PLAIN_TEXT};
use project::lsp_store::LspStoreEvent;
use settings::Settings as _;
use ui::prelude::*;
use util::ResultExt;
use util::rel_path::RelPath;
use workspace::notifications::simple_message_notification::MessageNotification;
use workspace::{AppState, Event as WorkspaceEvent, Workspace, notifications::NotificationId};

const SUGGESTIONS_BY_EXTENSION_ID: &[(&str, &[&str])] = &[
    ("asciidoc", &["adoc", "asciidoc"]),
    ("astro", &["astro"]),
    ("beancount", &["bean", "beancount"]),
    ("clojure", &["bb", "clj", "cljc", "cljd", "cljs", "edn"]),
    (
        "csharp",
        &["cs", "csproj", "proj", "props", "slnx", "targets"],
    ),
    ("csv", &["csv"]),
    ("cython", &["pyx", "pxd", "pxi"]),
    ("dart", &["dart"]),
    (
        "dockerfile",
        &[
            "Containerfile",
            "Dockerfile",
            "compose.yaml",
            "compose.yml",
            "docker-compose.yaml",
            "docker-compose.yml",
            "dockerfile",
        ],
    ),
    ("elisp", &["el"]),
    (
        "elixir",
        &["eex", "ex", "exs", "heex", "leex", "mix.lock", "neex"],
    ),
    ("elm", &["elm"]),
    ("env", &[".env", ".envrc", "env", "envrc"]),
    (
        "erlang",
        &[
            "app.src",
            "Emakefile",
            "erl",
            "erlang",
            "escript",
            "hrl",
            "rebar.config",
            "xrl",
            "yrl",
        ],
    ),
    ("fish", &["fish"]),
    (
        "gdscript",
        &[
            "gd",
            "gdextension",
            "gdshader",
            "gdshaderinc",
            "godot",
            "tres",
            "tscn",
        ],
    ),
    (
        "git-firefly",
        &[
            ".containerignore",
            ".cursorignore",
            ".dockerignore",
            ".eslintignore",
            ".fdignore",
            ".git-blame-ignore-revs",
            ".gitattributes",
            ".gitconfig",
            ".gitignore",
            ".gitignore_global",
            ".gitmodules",
            ".ignore",
            ".lfsconfig",
            ".npmignore",
            ".prettierignore",
            ".rgignore",
            ".vscodeignore",
            "config.worktree",
            "git-rebase-todo",
            "gitattributes",
        ],
    ),
    ("gleam", &["gleam"]),
    (
        "glsl",
        &[
            "comp", "frag", "geom", "glsl", "mesh", "rcall", "rgen", "rahit", "rchit", "rmiss",
            "rint", "task", "tesc", "tese", "vert",
        ],
    ),
    ("graphql", &["gql", "graphql", "graphqls"]),
    (
        "groovy",
        &["Jenkinsfile", "JenkinsFile", "gradle", "groovy"],
    ),
    ("haskell", &["cabal", "hs", "lhs"]),
    ("html", &["htm", "html", "shtml"]),
    ("ini", &["inf", "ini"]),
    ("java", &["java", "properties"]),
    ("json5", &["json5"]),
    ("julia", &["jl"]),
    ("just", &["JUSTFILE", "Justfile", "just", "justfile"]),
    ("kotlin", &["kt", "kts"]),
    (
        "latex",
        &[
            "bib", "biblatex", "bibtex", "cls", "dtx", "ins", "latex", "sty", "tex",
        ],
    ),
    ("log", &["log"]),
    ("lua", &["lua"]),
    (
        "make",
        &[
            "GNUmakefile",
            "mak",
            "Makefile",
            "makefile",
            "mk",
            "OCamlMakefile",
        ],
    ),
    ("neocmake", &["CMakeLists.txt", "cmake"]),
    ("nginx", &["nginx.conf"]),
    ("nim", &["nim", "nim_format_string", "nimble", "nims"]),
    ("nix", &["nix"]),
    ("nu", &["nu", "nuon"]),
    (
        "ocaml",
        &[
            "dune",
            "dune-project",
            "dune-workspace",
            "ml",
            "mld",
            "mli",
            "mll",
            "mlx",
            "mly",
            "re",
            "rei",
        ],
    ),
    ("odin", &["odin"]),
    ("perl", &["pl", "pm", "pod", "t"]),
    ("php", &["php", "phpt", "phtml"]),
    ("powershell", &["ps1", "psm1"]),
    ("prisma", &["prisma"]),
    ("proto", &["proto"]),
    ("purescript", &["purs"]),
    (
        "python-requirements",
        &["constraints.txt", "requirements.txt"],
    ),
    ("r", &["R", "Rmd", "qmd", "r", "rmd"]),
    ("racket", &["rkt"]),
    ("rescript", &["res", "resi"]),
    ("rst", &["rst"]),
    (
        "ruby",
        &[
            "Appfile",
            "Appraisals",
            "Berksfile",
            "Brewfile",
            "builder",
            "cap",
            "Capfile",
            "capfile",
            "Cheffile",
            "Dangerfile",
            "Deliverfile",
            "erb",
            "Fastfile",
            "Gemfile",
            "gemspec",
            "Guardfile",
            "Gymfile",
            "Hobofile",
            "irbrc",
            "jbuilder",
            "Matchfile",
            "Podfile",
            "pryrc",
            "Puppetfile",
            "rabl",
            "rake",
            "Rakefile",
            "Rantfile",
            "rb",
            "rbs",
            "ru",
            "rxml",
            "Scanfile",
            "simplecov",
            "Snapfile",
            "Steepfile",
            "thor",
            "Thorfile",
            "Vagrantfile",
        ],
    ),
    ("scala", &["mill", "scala", "sbt", "sc"]),
    ("scheme", &["scm", "ss"]),
    ("scss", &["sass", "scss"]),
    ("solidity", &["sol", "yul"]),
    ("sql", &["sql"]),
    ("svelte", &["svelte"]),
    ("swift", &["swift", "swiftinterface"]),
    ("templ", &["templ"]),
    ("terraform", &["hcl", "tf", "tfvars", "tofu"]),
    ("toml", &["Cargo.lock", "Pipfile", "toml", "uv.lock"]),
    ("typst", &["typ", "typst"]),
    ("vue", &["vue"]),
    ("wgsl", &["wgsl"]),
    ("windows-batch", &["bat", "cmd"]),
    ("wit", &["wit"]),
    ("xml", &["xml"]),
    ("zig", &["zig", "zon"]),
];

const EMMET_LANGUAGES: &[&str] = &[
    "Angular",
    "Blade",
    "CSS",
    "Django",
    "ERB",
    "Elixir",
    "HEEx",
    "HTML",
    "HTML+ERB",
    "JavaScript",
    "Jinja2",
    "LESS",
    "Liquid",
    "Nunjucks",
    "PHP",
    "SCSS",
    "Statamic Antlers",
    "TSX",
    "Twig",
    "Vue.js",
];

struct ExtensionSuggestionNotification;

pub(crate) fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, window, cx| {
        if window.is_none() {
            return;
        }
        let lsp_store = workspace.project().read(cx).lsp_store();
        cx.subscribe(&lsp_store, |workspace, _, event, cx| {
            if let LspStoreEvent::LanguageDetected {
                buffer,
                new_language: Some(_),
            } = event
            {
                suggest_emmet_for_buffer(workspace, buffer.clone(), cx);
            }
        })
        .detach();
        cx.subscribe_self(|workspace, event, cx| {
            if let WorkspaceEvent::ItemAdded { item } = event
                && let Some(editor) = item.downcast::<Editor>()
                && let Some(buffer) = editor.read(cx).buffer().read(cx).as_singleton()
            {
                suggest_emmet_for_buffer(workspace, buffer, cx);
            }
        })
        .detach();
        cx.subscribe(
            &ExtensionStore::global(cx),
            |workspace, extension_store, event, cx| {
                if let extension_host::Event::ExtensionsUpdated = event {
                    let installed = extension_store
                        .read(cx)
                        .installed_extensions()
                        .keys()
                        .map(|extension_id| notification_id(extension_id))
                        .collect::<Vec<_>>();
                    for installed in installed {
                        workspace.dismiss_notification(&installed, cx);
                    }
                }
            },
        )
        .detach();
    })
    .detach();
}

fn suggested_extensions() -> &'static HashMap<&'static str, Arc<str>> {
    static SUGGESTIONS_BY_PATH_SUFFIX: OnceLock<HashMap<&str, Arc<str>>> = OnceLock::new();
    SUGGESTIONS_BY_PATH_SUFFIX.get_or_init(|| {
        SUGGESTIONS_BY_EXTENSION_ID
            .iter()
            .flat_map(|(name, path_suffixes)| {
                let name = Arc::<str>::from(*name);
                path_suffixes
                    .iter()
                    .map(move |suffix| (*suffix, name.clone()))
            })
            .collect()
    })
}

#[derive(Debug, PartialEq, Eq, Clone)]
struct SuggestedExtension {
    pub extension_id: Arc<str>,
    pub file_name_or_extension: Arc<str>,
}

/// Returns the suggested extension for the given [`Path`].
fn suggested_extension(path: &RelPath) -> Option<SuggestedExtension> {
    let file_extension: Option<Arc<str>> = path.extension().map(|extension| extension.into());
    let file_name: Option<Arc<str>> = path.file_name().map(|name| name.into());

    let (file_name_or_extension, extension_id) = None
        // We suggest against file names first, as these suggestions will be more
        // specific than ones based on the file extension.
        .or_else(|| {
            file_name.clone().zip(
                file_name
                    .as_deref()
                    .and_then(|file_name| suggested_extensions().get(file_name)),
            )
        })
        .or_else(|| {
            file_extension.clone().zip(
                file_extension
                    .as_deref()
                    .and_then(|file_extension| suggested_extensions().get(file_extension)),
            )
        })?;

    Some(SuggestedExtension {
        extension_id: extension_id.clone(),
        file_name_or_extension,
    })
}

fn language_extension_key(extension_id: &str) -> String {
    format!("{}_extension_suggest", extension_id)
}

fn notification_id(extension_id: &str) -> NotificationId {
    NotificationId::composite::<ExtensionSuggestionNotification>(SharedString::from(extension_id))
}

fn suggestion_dismissed(extension_id: &str, cx: &App) -> bool {
    KeyValueStore::global(cx)
        .read_kvp(&language_extension_key(extension_id))
        .log_err()
        != Some(None)
}

fn dismiss_suggestion(extension_id: &str, cx: &mut App) {
    let key = language_extension_key(extension_id);
    let kvp = KeyValueStore::global(cx);
    db::write_and_log(cx, move || async move {
        kvp.write_kvp(key, "dismissed".to_string()).await
    });

    let notification_id = notification_id(extension_id);
    let workspaces = AppState::global(cx)
        .workspace_store
        .read(cx)
        .workspaces()
        .cloned()
        .collect::<Vec<_>>();
    for workspace in workspaces {
        workspace
            .update(cx, |workspace, cx| {
                workspace.dismiss_notification(&notification_id, cx);
            })
            .ok();
    }
}

fn is_active_in_some_pane(workspace: &Workspace, buffer: &Entity<Buffer>, cx: &App) -> bool {
    workspace.panes().iter().any(|pane| {
        pane.read(cx)
            .active_item()
            .and_then(|item| item.downcast::<Editor>())
            .is_some_and(|editor| {
                editor.read(cx).buffer().read(cx).as_singleton().as_ref() == Some(buffer)
            })
    })
}

fn should_skip_suggestion(workspace: &Workspace, extension_id: &str, cx: &App) -> bool {
    if workspace.has_notification(&notification_id(extension_id)) {
        return true;
    }

    let extension_store = ExtensionStore::global(cx);
    let extension_store = extension_store.read(cx);
    extension_store
        .installed_extensions()
        .contains_key(extension_id)
        || extension_store
            .outstanding_operations()
            .contains_key(extension_id)
        || ExtensionSettings::get_global(cx)
            .auto_install_extensions
            .get(extension_id)
            == Some(&true)
        || suggestion_dismissed(extension_id, cx)
}

fn suggest_emmet_for_buffer(
    workspace: &mut Workspace,
    buffer: Entity<Buffer>,
    cx: &mut Context<Workspace>,
) {
    if !is_active_in_some_pane(workspace, &buffer, cx) {
        return;
    }

    let language_name = buffer
        .read(cx)
        .language()
        .filter(|language| **language != *PLAIN_TEXT)
        .map(|language| language.name());
    let Some(language_name) = language_name else {
        return;
    };
    if !EMMET_LANGUAGES.contains(&language_name.as_ref()) {
        return;
    }
    if should_skip_suggestion(workspace, "emmet", cx) {
        return;
    }

    let extension_id = Arc::<str>::from("emmet");
    workspace.show_notification(notification_id("emmet"), cx, |cx| {
        cx.new(|cx| {
            MessageNotification::new(
                tr(
                    cx,
                    "extensions_ui.extension_suggest.emmet_description",
                    "Emmet expands abbreviations such as `ul>li*3` into HTML and `m10` into CSS.",
                ),
                cx,
            )
            .with_title(tr(
                cx,
                "extensions_ui.extension_suggest.emmet_title",
                "Emmet is available for this file",
            ))
            .primary_message(tr(
                cx,
                "extensions_ui.extension_suggest.install_emmet",
                "Install Emmet",
            ))
            .primary_icon(IconName::Check)
            .primary_icon_color(Color::Success)
            .primary_on_click({
                let extension_id = extension_id.clone();
                move |_window, cx| {
                    ExtensionStore::global(cx).update(cx, |store, cx| {
                        store.install_latest_extension(extension_id.clone(), cx);
                    });
                }
            })
            .secondary_message(tr(
                cx,
                "extensions_ui.extension_suggest.dont_show_again",
                "Don't show again",
            ))
            .secondary_icon(IconName::Close)
            .secondary_icon_color(Color::Error)
            .secondary_on_click(move |_window, cx| dismiss_suggestion(&extension_id, cx))
        })
    });
}

pub(crate) fn suggest(buffer: Entity<Buffer>, window: &mut Window, cx: &mut Context<Workspace>) {
    let Some(file) = buffer.read(cx).file().cloned() else {
        return;
    };

    let Some(SuggestedExtension {
        extension_id,
        file_name_or_extension,
    }) = suggested_extension(file.path())
    else {
        return;
    };

    let key = language_extension_key(&extension_id);
    let kvp = KeyValueStore::global(cx);
    let Ok(None) = kvp.read_kvp(&key) else {
        return;
    };

    cx.on_next_frame(window, move |workspace, _, cx| {
        let Some(editor) = workspace.active_item_as::<Editor>(cx) else {
            return;
        };

        if editor.read(cx).buffer().read(cx).as_singleton().as_ref() != Some(&buffer) {
            return;
        }

        struct ExtensionSuggestionNotification;

        let notification_id = NotificationId::composite::<ExtensionSuggestionNotification>(
            SharedString::from(extension_id.clone()),
        );

        workspace.show_notification(notification_id, cx, |cx| {
            cx.new(move |cx| {
                MessageNotification::new(
                    tr(
                        cx,
                        "extensions_ui.extension_suggest.prompt",
                        "Do you want to install the recommended '{}' extension for '{}' files?",
                    )
                    .replacen("{}", &extension_id, 1)
                    .replacen("{}", &file_name_or_extension, 1),
                    cx,
                )
                .primary_message(tr(
                    cx,
                    "extensions_ui.extension_suggest.install",
                    "Yes, install extension",
                ))
                .primary_icon(IconName::Check)
                .primary_icon_color(Color::Success)
                .primary_on_click({
                    let extension_id = extension_id.clone();
                    move |_window, cx| {
                        let extension_id = extension_id.clone();
                        let extension_store = ExtensionStore::global(cx);
                        extension_store.update(cx, move |store, cx| {
                            store.install_latest_extension(extension_id, cx);
                        });
                    }
                })
                .secondary_message(tr(
                    cx,
                    "extensions_ui.extension_suggest.dismiss",
                    "No, don't install it",
                ))
                .secondary_icon(IconName::Close)
                .secondary_icon_color(Color::Error)
                .secondary_on_click(move |_window, cx| {
                    let key = language_extension_key(&extension_id);
                    let kvp = KeyValueStore::global(cx);
                    cx.background_spawn(async move {
                        kvp.write_kvp(key, "dismissed".to_owned()).await.log_err()
                    })
                    .detach();
                })
            })
        });
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use util::rel_path::rel_path;

    #[test]
    pub fn test_suggested_extension() {
        assert_eq!(
            suggested_extension(rel_path("Cargo.toml")),
            Some(SuggestedExtension {
                extension_id: "toml".into(),
                file_name_or_extension: "toml".into()
            })
        );
        assert_eq!(
            suggested_extension(rel_path("Cargo.lock")),
            Some(SuggestedExtension {
                extension_id: "toml".into(),
                file_name_or_extension: "Cargo.lock".into()
            })
        );
        assert_eq!(
            suggested_extension(rel_path("Dockerfile")),
            Some(SuggestedExtension {
                extension_id: "dockerfile".into(),
                file_name_or_extension: "Dockerfile".into()
            })
        );
        assert_eq!(
            suggested_extension(rel_path("a/b/c/d/.gitignore")),
            Some(SuggestedExtension {
                extension_id: "git-firefly".into(),
                file_name_or_extension: ".gitignore".into()
            })
        );
        assert_eq!(
            suggested_extension(rel_path("a/b/c/d/test.gleam")),
            Some(SuggestedExtension {
                extension_id: "gleam".into(),
                file_name_or_extension: "gleam".into()
            })
        );
        assert_eq!(
            suggested_extension(rel_path("a/b/c/d/test.sol")),
            Some(SuggestedExtension {
                extension_id: "solidity".into(),
                file_name_or_extension: "sol".into()
            })
        );
        assert_eq!(
            suggested_extension(rel_path("a/b/c/d/test.jl")),
            Some(SuggestedExtension {
                extension_id: "julia".into(),
                file_name_or_extension: "jl".into()
            })
        );
        assert_eq!(
            suggested_extension(rel_path("script.pl")),
            Some(SuggestedExtension {
                extension_id: "perl".into(),
                file_name_or_extension: "pl".into()
            })
        );
        assert_eq!(
            suggested_extension(rel_path("app/uv.lock")),
            Some(SuggestedExtension {
                extension_id: "toml".into(),
                file_name_or_extension: "uv.lock".into()
            })
        );
        // Dotfiles have no `Path::extension`, so they match by name.
        assert_eq!(
            suggested_extension(rel_path(".envrc")),
            Some(SuggestedExtension {
                extension_id: "env".into(),
                file_name_or_extension: ".envrc".into()
            })
        );
        assert_eq!(
            suggested_extension(rel_path(".gitattributes")),
            Some(SuggestedExtension {
                extension_id: "git-firefly".into(),
                file_name_or_extension: ".gitattributes".into()
            })
        );
    }

    #[test]
    pub fn suggested_path_suffixes_are_unique() {
        let mut claims: HashMap<&str, &str> = HashMap::new();
        for (extension_id, path_suffixes) in SUGGESTIONS_BY_EXTENSION_ID {
            for suffix in *path_suffixes {
                let previous = claims.insert(suffix, extension_id);
                assert!(
                    previous.is_none(),
                    "duplicate suffix `{suffix}` is claimed by both `{}` and `{extension_id}`",
                    previous.unwrap()
                );
            }
        }
    }

    #[test]
    pub fn table_is_sorted_by_extension_id() {
        assert!(
            SUGGESTIONS_BY_EXTENSION_ID
                .iter()
                .map(|(extension_id, _)| *extension_id)
                .is_sorted(),
            "suggested extensions must be sorted by id"
        );
    }
}
