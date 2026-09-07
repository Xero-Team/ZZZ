use gh_workflow::{Step, Use};

use crate::tasks::workflows::{
    runners,
    steps::{self, FluentBuilder as _, NamedJob, named, release_job},
};

const BUILD_OUTPUT_DIR: &str = "target/deploy";

pub(crate) enum DocsChannel {
    Nightly,
    Preview,
    Stable,
}

impl DocsChannel {
    pub(crate) fn site_url(&self) -> &'static str {
        match self {
            Self::Nightly => "/docs/nightly/",
            Self::Preview => "/docs/preview/",
            Self::Stable => "/docs/",
        }
    }

    pub(crate) fn channel_name(&self) -> &'static str {
        match self {
            Self::Nightly => "nightly",
            Self::Preview => "preview",
            Self::Stable => "stable",
        }
    }
}

pub(crate) fn lychee_link_check(dir: &str) -> Step<Use> {
    named::uses(
        "lycheeverse",
        "lychee-action",
        "82202e5e9c2f4ef1a55a3d02563e1cb6041e5332",
    ) // v2.4.1
    .add_with(("args", format!("--no-progress --exclude '^http' '{dir}'")))
    .add_with(("fail", true))
    .add_with(("jobSummary", false))
}

pub(crate) fn install_mdbook() -> Step<Use> {
    named::uses(
        "peaceiris",
        "actions-mdbook",
        "ee69d230fe19748b7abf22df32acaa93833fad08", // v2
    )
    .with(("mdbook-version", "0.4.37"))
}

pub(crate) fn build_docs_book(docs_channel: String, site_url: String) -> Step<gh_workflow::Run> {
    named::bash(indoc::formatdoc! {r#"
        mkdir -p {BUILD_OUTPUT_DIR}
        mdbook build ./docs --dest-dir=../{BUILD_OUTPUT_DIR}/docs/
    "#})
    .add_env(("DOCS_CHANNEL", docs_channel))
    .add_env(("MDBOOK_BOOK__SITE_URL", site_url))
}

fn docs_build_steps(
    job: gh_workflow::Job,
    checkout_ref: Option<String>,
    docs_channel: impl Into<String>,
    site_url: impl Into<String>,
) -> gh_workflow::Job {
    let docs_channel = docs_channel.into();
    let site_url = site_url.into();

    steps::use_clang(
        job.add_step(
            steps::checkout_repo().when_some(checkout_ref, |step, checkout_ref| {
                step.with_ref(checkout_ref)
            }),
        )
        .runs_on(runners::LINUX_XL)
        .add_step(steps::setup_cargo_config(runners::Platform::Linux))
        .add_step(steps::cache_rust_dependencies_namespace())
        .map(steps::install_linux_dependencies)
        .add_step(steps::script("./script/generate-action-metadata"))
        .add_step(lychee_link_check("./docs/src/**/*"))
        .add_step(install_mdbook())
        .add_step(build_docs_book(docs_channel, site_url))
        .add_step(lychee_link_check(&format!("{BUILD_OUTPUT_DIR}/docs"))),
    )
}

pub(crate) fn check_docs() -> NamedJob {
    NamedJob {
        name: "check_docs".to_owned(),
        job: docs_build_steps(
            release_job(&[]),
            None,
            DocsChannel::Stable.channel_name(),
            DocsChannel::Stable.site_url(),
        ),
    }
}
