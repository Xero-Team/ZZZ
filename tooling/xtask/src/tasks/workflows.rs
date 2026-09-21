#![allow(dead_code)]

use anyhow::{Context, Result};
use clap::Parser;
use gh_workflow::Workflow;
use std::fs;
use std::path::{Path, PathBuf};
use strum::IntoEnumIterator;

use crate::tasks::workflow_checks::{self};

mod autofix_pr;
mod bump_patch_version;
mod change_detection;
mod cherry_pick;
mod compare_perf;
mod compliance_check;
mod danger;
mod deploy_docs;
mod extension_auto_bump;
mod extension_bump;
mod extension_tests;
mod extensions;
mod job_summary;
mod nix_build;
mod platform_checks;
mod run_bundling;

mod release;
mod runners;
mod steps;
mod ts_query;
mod vars;

#[derive(Clone)]
pub(crate) struct GitSha(String);

impl AsRef<str> for GitSha {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[allow(
    clippy::disallowed_methods,
    reason = "This runs only in a CLI environment"
)]
fn parse_ref(value: &str) -> Result<GitSha, String> {
    const GIT_SHA_LENGTH: usize = 40;
    (value.len() == GIT_SHA_LENGTH)
        .then_some(value)
        .ok_or_else(|| {
            format!(
                "Git SHA has wrong length! \
                Only SHAs with a full length of {GIT_SHA_LENGTH} are supported, found {len} characters.",
                len = value.len()
            )
        })
        .and_then(|value| {
            let mut tmp = [0; 4];
            value
                .chars()
                .all(|char| u16::from_str_radix(char.encode_utf8(&mut tmp), 16).is_ok()).then_some(value)
                .ok_or_else(|| "Not a valid Git SHA".to_owned())
        })
        .and_then(|sha| {
           std::process::Command::new("git")
               .args([
                   "rev-parse",
                   "--quiet",
                   "--verify",
                   &format!("{sha}^{{commit}}")
               ])
               .output()
               .map_err(|_| "Failed to spawn Git command to verify SHA".to_owned())
               .and_then(|output|
                   output
                       .status.success()
                       .then_some(sha)
                       .ok_or_else(|| format!("SHA {sha} is not a valid Git SHA within this repository!")))
        }).map(|sha| GitSha(sha.to_owned()))
}

#[derive(Parser)]
pub(crate) struct GenerateWorkflowArgs {
    #[arg(value_parser = parse_ref)]
    /// The Git SHA to use when invoking this
    pub(crate) sha: Option<GitSha>,
}

enum WorkflowSource {
    Contextless(fn() -> Workflow),
    WithContext(fn(&GenerateWorkflowArgs) -> Workflow),
}

struct WorkflowFile {
    source: WorkflowSource,
    r#type: WorkflowType,
}

impl WorkflowFile {
    fn zzz(f: fn() -> Workflow) -> WorkflowFile {
        WorkflowFile {
            source: WorkflowSource::Contextless(f),
            r#type: WorkflowType::ZZZ,
        }
    }

    fn generate_file(&self, workflow_args: &GenerateWorkflowArgs) -> Result<()> {
        let workflow = match &self.source {
            WorkflowSource::Contextless(f) => f(),
            WorkflowSource::WithContext(f) => f(workflow_args),
        };
        let workflow_folder = self.r#type.folder_path();

        fs::create_dir_all(&workflow_folder).with_context(|| {
            format!("Failed to create directory: {}", workflow_folder.display())
        })?;

        let workflow_name = workflow
            .name
            .as_ref()
            .expect("Workflow must have a name at this point");
        let filename = format!(
            "{}.yml",
            workflow_name.rsplit("::").next().unwrap_or(workflow_name)
        );

        let workflow_path = workflow_folder.join(filename);

        let content = workflow
            .to_string()
            .map_err(|e| anyhow::anyhow!("{:?}: {:?}", workflow_path, e))?;

        let disclaimer = self.r#type.disclaimer(workflow_name);

        let content = [disclaimer, content].join("\n");
        fs::write(&workflow_path, content).map_err(Into::into)
    }
}

#[derive(PartialEq, Eq, strum::EnumIter)]
pub enum WorkflowType {
    /// Workflows living in the ZZZ repository
    ZZZ,
}

impl WorkflowType {
    const PREAMBLE: &str = "# Generated from xtask::workflows::";

    fn disclaimer(&self, workflow_name: &str) -> String {
        format!(
            concat!(
                "{preamble}{workflow_name}\n",
                "# Rebuild with `cargo xtask workflows`.",
            ),
            preamble = Self::PREAMBLE,
            workflow_name = workflow_name,
        )
    }

    pub fn folder_path(&self) -> PathBuf {
        match self {
            WorkflowType::ZZZ => PathBuf::from(".github/workflows"),
        }
    }

    fn remove_generated_workflows() -> Result<()> {
        for workflow_type in Self::iter() {
            let Ok(entries) = fs::read_dir(workflow_type.folder_path()) else {
                continue;
            };
            for path in entries {
                let entry = path?;
                if entry
                    .file_type()
                    .map_or(true, |file_type| !file_type.is_file())
                {
                    continue;
                }

                let path = entry.path();
                if fs::read_to_string(&path)
                    .is_ok_and(|content| content.starts_with(Self::PREAMBLE))
                {
                    fs::remove_file(path)?;
                }
            }
        }

        Ok(())
    }
}

pub fn run_workflows(args: GenerateWorkflowArgs) -> Result<()> {
    if !Path::new("crates/zzz/").is_dir() {
        anyhow::bail!("xtask workflows must be ran from the project root");
    }

    // Remove all previously generated workflows to ensure these do not become stale.
    WorkflowType::remove_generated_workflows()?;

    let workflows = [
        WorkflowFile::zzz(compliance_check::compliance_check),
        WorkflowFile::zzz(extension_tests::extension_tests),
        WorkflowFile::zzz(release::release),
    ];

    for workflow_file in workflows {
        workflow_file.generate_file(&args)?;
    }

    workflow_checks::validate(Default::default())
}
