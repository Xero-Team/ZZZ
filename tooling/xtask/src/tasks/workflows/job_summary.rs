use crate::tasks::workflows::{runners, steps::named, steps::NamedJob};
use gh_workflow::Job;

pub fn tests_pass(jobs: &[NamedJob], extra_job_names: &[&str]) -> NamedJob {
    let mut script = String::from(indoc::indoc! {r#"
        set +x
        EXIT_CODE=0

        check_result() {
          echo "* $1: $2"
          if [[ "$2" != "skipped" && "$2" != "success" ]]; then EXIT_CODE=1; fi
        }

    "#});

    let all_names: Vec<&str> = jobs
        .iter()
        .map(|job| job.name.as_str())
        .chain(extra_job_names.iter().copied())
        .collect();

    let env_entries: Vec<_> = all_names
        .iter()
        .map(|name| {
            let env_name = format!("RESULT_{}", name.to_uppercase());
            let env_value = format!("${{{{ needs.{}.result }}}}", name);
            (env_name, env_value)
        })
        .collect();

    script.push_str(
        &all_names
            .iter()
            .zip(env_entries.iter())
            .map(|(name, (env_name, _))| format!("check_result \"{}\" \"${}\"", name, env_name))
            .collect::<Vec<_>>()
            .join("\n"),
    );

    script.push_str("\n\nexit $EXIT_CODE\n");

    let job = Job::default()
        .runs_on(runners::LINUX_SMALL)
        .needs(
            all_names
                .iter()
                .map(|name| name.to_string())
                .collect::<Vec<String>>(),
        )
        .add_step(
            env_entries
                .into_iter()
                .fold(named::bash(&script), |step, env_item| {
                    step.add_env(env_item)
                }),
        );

    named::job(job)
}
