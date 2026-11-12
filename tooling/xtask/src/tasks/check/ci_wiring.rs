//! Requires the CI workflow to keep invoking the repository gates.
//!
//! Every gate in `run_all` is only enforced because `.github/workflows/ci.yaml`
//! calls `cargo xtask check`, and the compiler-backed gates because CI calls
//! clippy, fmt, the test suite, grammars, deny, and zizmor. Nothing stopped a
//! later edit from deleting one of those steps: the Rust gate would still pass
//! locally while CI silently stopped running it. This gate parses ci.yaml and
//! fails when a required command disappears from every `run:` value.
//!
//! Gates that live in their own workflow (`REQUIRED_STEPS`) are checked by
//! reading that file, because the `run:` scan only covers ci.yaml.

use std::fs;
use std::path::Path;

use anyhow::{Context as _, Result, bail};
use serde_yaml::Value;

const CI_WORKFLOW: &str = ".github/workflows/ci.yaml";

/// Workflow files that must keep calling a repository script, and the command
/// each one must keep running.
///
/// These live outside `ci.yaml`, so the `run:` scan below cannot see them.
/// A deleted workflow, or a deleted step inside one, disables the gate the
/// same way a deleted CI step does.
const REQUIRED_STEPS: &[(&str, &str)] = &[(
    ".github/workflows/pr-metadata.yml",
    "script/check-pr-metadata",
)];

/// Stable command signatures the CI pipeline must keep invoking. They are
/// matched as substrings of `run:` values so unrelated flag changes do not
/// produce false positives; removing the step, or the command inside it, is
/// what must fail. Keep in sync with the `run:` steps in ci.yaml.
const REQUIRED_RUNS: &[&str] = &[
    "cargo deny",
    "zizmor",
    "cargo xtask grammars check",
    "cargo xtask check",
    "cargo fmt --all -- --check",
    "cargo xtask clippy",
    "cargo doc --no-deps",
    "cargo nextest run",
];

pub fn check() -> Result<()> {
    let source = fs::read_to_string(Path::new(CI_WORKFLOW))
        .with_context(|| format!("failed to read {CI_WORKFLOW}"))?;
    let parsed: Value = serde_yaml::from_str(&source)
        .with_context(|| format!("failed to parse {CI_WORKFLOW} as YAML"))?;

    let mut runs = Vec::new();
    collect_run_commands(&parsed, &mut runs);
    if runs.is_empty() {
        bail!("{CI_WORKFLOW} has no `run:` commands; the CI gate wiring may be missing");
    }

    let mut failures: Vec<String> = missing_commands(&runs)
        .into_iter()
        .map(|command| {
            format!("{CI_WORKFLOW} no longer runs `{command}` (a step may have been deleted)")
        })
        .collect();
    failures.extend(missing_steps(|path| fs::read_to_string(path).ok()));

    if failures.is_empty() {
        return Ok(());
    }
    bail!(
        "CI no longer invokes required gates:\n{}",
        failures
            .iter()
            .map(|failure| format!("  - {failure}"))
            .collect::<Vec<_>>()
            .join("\n")
    )
}

/// Returns one message per required step whose workflow or command is gone.
fn missing_steps(read: impl Fn(&str) -> Option<String>) -> Vec<String> {
    REQUIRED_STEPS
        .iter()
        .filter_map(|(path, command)| match read(path) {
            Some(source) if source.contains(command) => None,
            Some(_) => Some(format!("{path} no longer runs `{command}`")),
            None => Some(format!("{path} is missing; `{command}` has no caller")),
        })
        .collect()
}

/// Returns the required commands that appear in none of the `run:` values.
fn missing_commands(runs: &[String]) -> Vec<&'static str> {
    REQUIRED_RUNS
        .iter()
        .copied()
        .filter(|required| !runs.iter().any(|run| run.contains(required)))
        .collect()
}

fn collect_run_commands(value: &Value, commands: &mut Vec<String>) {
    match value {
        Value::Mapping(mapping) => {
            for (key, value) in mapping {
                if let Value::String(label) = key
                    && label == "run"
                    && let Value::String(command) = value
                {
                    commands.push(command.clone());
                }
                collect_run_commands(value, commands);
            }
        }
        Value::Sequence(sequence) => {
            for value in sequence {
                collect_run_commands(value, commands);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::{
        REQUIRED_RUNS, REQUIRED_STEPS, collect_run_commands, missing_commands, missing_steps,
    };
    use serde_yaml::Value;

    fn all_runs() -> Vec<String> {
        REQUIRED_RUNS
            .iter()
            .map(|command| format!("env && {command} --some-flag"))
            .collect()
    }

    #[test]
    fn passes_when_every_required_command_is_present() {
        assert!(missing_commands(&all_runs()).is_empty());
    }

    #[test]
    fn reports_each_removed_command() {
        for removed in REQUIRED_RUNS {
            let runs: Vec<String> = all_runs()
                .into_iter()
                .filter(|run| !run.contains(removed))
                .collect();
            let missing = missing_commands(&runs);
            assert_eq!(missing, vec![*removed], "removing {removed}: {missing:?}");
        }
    }

    #[test]
    fn an_empty_run_list_is_not_silently_accepted() {
        assert_eq!(missing_commands(&[]).len(), REQUIRED_RUNS.len());
    }

    #[test]
    fn collects_only_run_values_from_yaml() {
        let value: Value = serde_yaml::from_str(
            "jobs:\n  checks:\n    steps:\n      - run: cargo xtask check\n        name: not a run value\n      - uses: some/action\n        with:\n          tool: cargo-deny,zizmor\n",
        )
        .expect("fixture should parse");
        let mut runs = Vec::new();
        collect_run_commands(&value, &mut runs);
        assert_eq!(runs, vec!["cargo xtask check".to_owned()], "{runs:?}");
    }

    #[test]
    fn collects_multiline_run_blocks() {
        let value: Value = serde_yaml::from_str("run: |\n  set -e\n  cargo xtask clippy\n")
            .expect("fixture should parse");
        let mut runs = Vec::new();
        collect_run_commands(&value, &mut runs);
        assert_eq!(runs.len(), 1, "{runs:?}");
        assert!(runs[0].contains("cargo xtask clippy"), "{runs:?}");
    }

    #[test]
    fn a_present_step_satisfies_the_requirement() {
        let missing = missing_steps(|_| Some("run: ./script/check-pr-metadata\n".to_owned()));
        assert!(missing.is_empty(), "{missing:?}");
    }

    #[test]
    fn a_deleted_step_is_reported() {
        let missing = missing_steps(|_| Some("run: echo hello\n".to_owned()));
        assert_eq!(missing.len(), REQUIRED_STEPS.len(), "{missing:?}");
        assert!(missing[0].contains("no longer runs"), "{missing:?}");
    }

    #[test]
    fn a_deleted_workflow_is_reported() {
        let missing = missing_steps(|_| None);
        assert_eq!(missing.len(), REQUIRED_STEPS.len(), "{missing:?}");
        assert!(missing[0].contains("is missing"), "{missing:?}");
    }
}
