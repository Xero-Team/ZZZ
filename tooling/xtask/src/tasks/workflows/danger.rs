use gh_workflow::*;

use crate::tasks::workflows::steps::{CommonJobConditions, NamedJob, named};

use super::{runners, steps};

/// Generates the danger.yml workflow
pub fn danger() -> Workflow {
    let danger = danger_job();

    named::workflow()
        .on(
            Event::default().pull_request(PullRequest::default().add_branch("main").types([
                PullRequestType::Opened,
                PullRequestType::Synchronize,
                PullRequestType::Reopened,
                PullRequestType::Edited,
            ])),
        )
        .add_job(danger.name, danger.job)
}

fn danger_job() -> NamedJob {
    pub fn install_deps() -> Step<Run> {
        named::bash("pnpm install --dir script/danger")
    }

    pub fn run() -> Step<Run> {
        named::bash("echo 'ZZZ does not use a hosted Danger GitHub API proxy'")
    }

    NamedJob {
        name: "danger".to_owned(),
        job: Job::default()
            .with_repository_owner_guard()
            .runs_on(runners::LINUX_SMALL)
            .add_step(steps::checkout_repo())
            .add_step(steps::setup_pnpm())
            .add_step(
                steps::setup_node()
                    .add_with(("cache", "pnpm"))
                    .add_with(("cache-dependency-path", "script/danger/pnpm-lock.yaml")),
            )
            .add_step(install_deps())
            .add_step(run()),
    }
}
