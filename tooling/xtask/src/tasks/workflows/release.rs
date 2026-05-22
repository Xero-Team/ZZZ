use gh_workflow::{Event, Job, Workflow, WorkflowDispatch};

use crate::tasks::workflows::{
    run_bundling::{bundle_mac, bundle_windows},
    runners::{Arch, LINUX_SMALL, ReleaseChannel},
    steps::{NamedJob, named},
    vars::WorkflowInput,
};

pub(crate) fn release() -> Workflow {
    let release_version = WorkflowInput::string("release_version", None)
        .description("Prerelease version for Windows installer.");

    let prerelease_gate = NamedJob {
        name: "prerelease_gate".to_owned(),
        job: Job::default()
            .runs_on(LINUX_SMALL)
            .add_step(named::bash("true")),
    };

    let mac_bundle = bundle_mac(
        Arch::AARCH64,
        Some(ReleaseChannel::Preview),
        &[&prerelease_gate],
    );
    let windows_bundle = bundle_windows(
        Arch::X86_64,
        Some(ReleaseChannel::Preview),
        &[&prerelease_gate],
    );

    named::workflow()
        .name("prerelease")
        .on(Event::default().workflow_dispatch(
            WorkflowDispatch::default().add_input(release_version.name, release_version.input()),
        ))
        .add_env(("CARGO_TERM_COLOR", "always"))
        .add_env(("RUST_BACKTRACE", "1"))
        .add_env(("RELEASE_VERSION", release_version.to_string()))
        .add_env(("CI", ""))
        .add_job(prerelease_gate.name, prerelease_gate.job)
        .add_job(mac_bundle.name, mac_bundle.job)
        .add_job(windows_bundle.name, windows_bundle.job)
}
