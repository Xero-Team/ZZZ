//! Runs a [`Plan`] with a global job budget.
//!
//! Cargo parallelizes inside one invocation; it does not coordinate several.
//! The scheduler treats `--jobs` as the budget for the whole build and gives
//! each ready task a share of it, so `bundle_linux`, `bundle_mac`, and
//! `bundle_windows` can run at the same time without oversubscribing the
//! machine.

#![allow(
    clippy::disallowed_methods,
    reason = "the scheduler owns child processes; xtask has no UI thread"
)]

use std::collections::BTreeSet;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Result, bail};

use crate::tasks::build::plan::{Plan, Task};

#[derive(Clone, Copy, Debug, Default)]
pub struct ExecuteOptions {
    pub dry_run: bool,
    pub keep_going: bool,
    pub verbose: bool,
}

struct Running {
    id: String,
    child: Child,
    jobs: u32,
    started: Instant,
    /// Container tasks need different cleanup advice: killing the client does
    /// not stop a container that conmon already detached from it.
    in_container: bool,
}

pub fn execute(plan: &Plan, options: ExecuteOptions) -> Result<()> {
    plan.validate()?;

    if options.dry_run {
        for task in &plan.tasks {
            eprintln!("would run [{}] {}", task.id, task.command.join(" "));
        }
        return Ok(());
    }

    create_mount_directories(plan)?;

    let total_jobs = plan.jobs.max(1);
    let total_tasks = plan.tasks.len();
    let mut started = BTreeSet::new();
    let mut finished = BTreeSet::new();
    let mut skipped = BTreeSet::new();
    let mut blocked = BTreeSet::new();
    let mut running: Vec<Running> = Vec::new();
    let mut failures: Vec<String> = Vec::new();
    let build_started = Instant::now();

    loop {
        if finished.len() + skipped.len() == total_tasks {
            break;
        }

        let mut available =
            total_jobs.saturating_sub(running.iter().map(|running| running.jobs).sum::<u32>());
        let mut launched = false;

        for task in &plan.tasks {
            if started.contains(&task.id) {
                continue;
            }
            if !dependencies_are_settled(task, &finished, &skipped, &blocked) {
                continue;
            }
            if task
                .depends_on
                .iter()
                .any(|dependency| blocked.contains(dependency))
            {
                eprintln!("skip [{}] (an earlier task failed)", task.id);
                blocked.insert(task.id.clone());
                started.insert(task.id.clone());
                launched = true;
                continue;
            }
            if let Some(guard) = &task.guard {
                if guard.should_skip()? {
                    eprintln!("skip [{}] (already done)", task.id);
                    skipped.insert(task.id.clone());
                    started.insert(task.id.clone());
                    launched = true;
                    continue;
                }
            }

            let jobs = task.jobs.max(1).min(total_jobs);
            if jobs > available && !running.is_empty() {
                continue;
            }

            if options.verbose {
                eprintln!("run  [{}] {}", task.id, task.command.join(" "));
            } else {
                eprintln!("==>  [{}] {}", task.id, task.description);
            }

            let child = spawn(plan.workspace.as_path(), task)?;
            available = available.saturating_sub(jobs);
            started.insert(task.id.clone());
            running.push(Running {
                id: task.id.clone(),
                child,
                jobs,
                started: Instant::now(),
                in_container: task.container.is_some(),
            });
            launched = true;
        }

        if running.is_empty() {
            if launched {
                continue;
            }
            let pending = plan
                .tasks
                .iter()
                .filter(|task| !started.contains(&task.id))
                .map(|task| task.id.as_str())
                .collect::<Vec<_>>();
            if pending.is_empty() {
                break;
            }
            bail!(
                "the build stalled; these tasks never became ready: {}",
                pending.join(", ")
            );
        }

        std::thread::sleep(Duration::from_millis(100));

        let mut index = running.len();
        while index > 0 {
            index -= 1;
            let Some(status) = running[index].child.try_wait()? else {
                continue;
            };
            let finished_task = running.remove(index);
            let elapsed = finished_task.started.elapsed();
            if status.success() {
                eprintln!("done [{}] ({})", finished_task.id, format_duration(elapsed));
                finished.insert(finished_task.id);
            } else {
                report_failure(&finished_task.id, status, elapsed);
                failures.push(finished_task.id.clone());
                blocked.insert(finished_task.id);
                if !options.keep_going {
                    terminate(&mut running);
                    bail!("build failed at `{}`", failures.join("`, `"));
                }
            }
        }
    }

    let elapsed = build_started.elapsed();
    if failures.is_empty() {
        eprintln!(
            "build finished: {} task(s) in {}",
            total_tasks,
            format_duration(elapsed)
        );
        Ok(())
    } else {
        bail!(
            "build finished with {} failure(s) after {}: {}",
            failures.len(),
            format_duration(elapsed),
            failures.join(", ")
        )
    }
}

fn dependencies_are_settled(
    task: &Task,
    finished: &BTreeSet<String>,
    skipped: &BTreeSet<String>,
    blocked: &BTreeSet<String>,
) -> bool {
    task.depends_on.iter().all(|dependency| {
        finished.contains(dependency)
            || skipped.contains(dependency)
            || blocked.contains(dependency)
    })
}

fn spawn(workspace: &Path, task: &Task) -> Result<Child> {
    let (program, arguments) = task
        .command
        .split_first()
        .ok_or_else(|| anyhow::anyhow!("task `{}` has an empty command", task.id))?;

    Command::new(program)
        .args(arguments)
        .current_dir(workspace)
        .envs(&task.environment)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|error| {
            anyhow::anyhow!(
                "failed to start `{}` for task `{}`: {error}",
                program,
                task.id
            )
        })
}

fn report_failure(id: &str, status: ExitStatus, elapsed: Duration) {
    match status.code() {
        Some(code) => eprintln!(
            "fail [{}] exited with {code} ({})",
            id,
            format_duration(elapsed)
        ),
        None => eprintln!("fail [{}] was terminated by a signal", id),
    }
}

fn terminate(running: &mut [Running]) {
    for task in running.iter_mut() {
        eprintln!("kill [{}]", task.id);
        if let Err(error) = task.child.kill() {
            // The child may have exited between `try_wait` and this call; that
            // is the common case and not worth failing over, but say so.
            eprintln!("note [{}] could not be killed: {error}", task.id);
        }
        if task.in_container {
            // The container outlives the engine client, so `--rm` only removes
            // it once it finishes on its own.
            eprintln!(
                "note [{}] runs in a container; `podman ps` or `docker ps` shows it \
                 if it keeps running",
                task.id
            );
        }
    }
}

fn create_mount_directories(plan: &Plan) -> Result<()> {
    if plan.tasks.iter().all(|task| task.container.is_none()) {
        return Ok(());
    }
    // Create the mount points here so the container engine does not create them
    // as root: the build then writes into a directory the caller owns.
    for directory in [
        plan.workspace.join("target/container"),
        plan.workspace.join(".cache/container-cargo/registry"),
        plan.workspace.join(".cache/container-cargo/git"),
    ] {
        std::fs::create_dir_all(&directory).map_err(|error| {
            anyhow::anyhow!("failed to create {}: {error}", directory.display())
        })?;
    }
    Ok(())
}

fn format_duration(duration: Duration) -> String {
    let seconds = duration.as_secs();
    if seconds >= 60 {
        format!("{}m{:02}s", seconds / 60, seconds % 60)
    } else {
        format!("{seconds}s")
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::tasks::build::model::{Channel, Profile};
    use crate::tasks::build::plan::{RepositoryState, SCHEMA_VERSION};

    fn echo_task(id: &str, depends_on: &[&str], message: &str) -> Task {
        Task {
            id: id.to_owned(),
            description: message.to_owned(),
            depends_on: depends_on
                .iter()
                .map(|dependency| (*dependency).to_owned())
                .collect(),
            jobs: 1,
            guard: None,
            command: vec![
                "sh".to_owned(),
                "-c".to_owned(),
                format!("echo {message} >> order.log"),
            ],
            environment: BTreeMap::new(),
            container: None,
        }
    }

    fn plan(workspace: &Path, tasks: Vec<Task>, jobs: u32) -> Plan {
        Plan {
            schema_version: SCHEMA_VERSION,
            workspace: workspace.to_path_buf(),
            generated_by: "test".to_owned(),
            repository: RepositoryState {
                commit: "test".to_owned(),
                dirty: false,
                version: "0.0.0".to_owned(),
            },
            channel: Channel::Dev,
            profile: Profile::Release,
            jobs,
            targets: Vec::new(),
            tasks,
        }
    }

    #[test]
    fn dependencies_run_before_their_dependents() {
        let directory = tempfile::tempdir().expect("temp dir");
        let plan = plan(
            directory.path(),
            vec![
                echo_task("first", &[], "first"),
                echo_task("second", &[], "second"),
                echo_task("last", &["first", "second"], "last"),
            ],
            2,
        );

        execute(
            &plan,
            ExecuteOptions {
                dry_run: false,
                keep_going: false,
                verbose: false,
            },
        )
        .expect("plan runs");

        let log = std::fs::read_to_string(directory.path().join("order.log")).expect("log");
        let lines = log.lines().collect::<Vec<_>>();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[2], "last");
        assert!(lines[..2].contains(&"first"));
        assert!(lines[..2].contains(&"second"));
    }

    #[test]
    fn a_failing_task_stops_its_dependents() {
        let directory = tempfile::tempdir().expect("temp dir");
        let mut failing = echo_task("failing", &[], "failing");
        failing.command = vec!["sh".to_owned(), "-c".to_owned(), "exit 3".to_owned()];
        let plan = plan(
            directory.path(),
            vec![failing, echo_task("after", &["failing"], "after")],
            1,
        );

        let result = execute(
            &plan,
            ExecuteOptions {
                dry_run: false,
                keep_going: false,
                verbose: false,
            },
        );

        assert!(result.is_err());
        assert!(!directory.path().join("order.log").exists());
    }

    #[test]
    fn dry_runs_start_nothing() {
        let directory = tempfile::tempdir().expect("temp dir");
        let plan = plan(directory.path(), vec![echo_task("only", &[], "only")], 1);

        execute(
            &plan,
            ExecuteOptions {
                dry_run: true,
                keep_going: false,
                verbose: false,
            },
        )
        .expect("dry run");

        assert!(!directory.path().join("order.log").exists());
    }
}
