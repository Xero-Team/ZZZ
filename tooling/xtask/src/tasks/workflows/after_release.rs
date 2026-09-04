use gh_workflow::*;

use crate::tasks::workflows::{
    runners,
    steps::{CommonJobConditions, NamedJob, named},
    vars::WorkflowInput,
};

const TAG_NAME_ENV: &str = "${{ github.event.release.tag_name || inputs.tag_name }}";

pub fn after_release() -> Workflow {
    let tag_name = WorkflowInput::string("tag_name", None);
    let prerelease = WorkflowInput::bool("prerelease", None);
    let body = WorkflowInput::string("body", Some(String::new()));

    let no_hosted_deploy = no_hosted_deploy();

    named::workflow()
        .add_env(("TAG_NAME", TAG_NAME_ENV))
        .on(Event::default()
            .release(Release::default().types(vec![ReleaseType::Published]))
            .workflow_dispatch(
                WorkflowDispatch::default()
                    .add_input(tag_name.name, tag_name.input())
                    .add_input(prerelease.name, prerelease.input())
                    .add_input(body.name, body.input()),
            ))
        .add_job(no_hosted_deploy.name, no_hosted_deploy.job)
}

fn no_hosted_deploy() -> NamedJob {
    named::job(
        Job::default()
            .runs_on(runners::LINUX_SMALL)
            .with_repository_owner_guard()
            .add_step(named::bash(
                "echo 'ZZZ has no hosted Discord, Vercel, winget, or zed.dev deploy after release'",
            )),
    )
}
