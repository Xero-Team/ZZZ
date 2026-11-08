mod check_run_patterns;

use std::{
    fs,
    path::{Path, PathBuf},
};

use annotate_snippets::Renderer;
use anyhow::{Context as _, Result, anyhow};
use clap::Parser;
use itertools::{Either, Itertools};
use serde_yaml::Value;
use strum::IntoEnumIterator;

use crate::tasks::{
    workflow_checks::check_run_patterns::{
        RunValidationError, WorkflowFile, WorkflowValidationError,
    },
    workflows::WorkflowType,
};

pub use check_run_patterns::validate_run_command;

#[derive(Default, Parser)]
pub struct WorkflowValidationArgs {}

pub fn validate(_: WorkflowValidationArgs) -> Result<()> {
    let workflow_files = get_all_workflow_files()?;
    let (parsing_errors, file_errors): (Vec<_>, Vec<_>) = workflow_files
        .into_iter()
        .map(check_workflow)
        .filter_map(Result::err)
        .partition_map(|error| match error {
            WorkflowError::ParseError(error) => Either::Left(error),
            WorkflowError::ValidationError(error) => Either::Right(error),
        });

    if !parsing_errors.is_empty() {
        Err(anyhow!(
            "Failed to read or parse some workflow files: {}",
            parsing_errors.into_iter().join("\n")
        ))
    } else if !file_errors.is_empty() {
        let errors: Vec<_> = file_errors
            .iter()
            .map(|error| error.annotation_group())
            .collect();

        let renderer =
            Renderer::styled().decor_style(annotate_snippets::renderer::DecorStyle::Ascii);
        println!("{}", renderer.render(errors.as_slice()));

        Err(anyhow!("Workflow checks failed!"))
    } else {
        Ok(())
    }
}

enum WorkflowError {
    ParseError(anyhow::Error),
    ValidationError(Box<WorkflowValidationError>),
}

/// Returns every workflow YAML file to validate.
///
/// This used to be `fs::read_dir(...).into_iter()`, which turns a missing or
/// unreadable directory into an *empty* iterator (`Result::into_iter` yields
/// nothing for `Err`), and `filter_map(Result::ok)` swallowed per-entry errors.
/// Either way `validate()` would scan zero files and report success, so a
/// missing/renamed `.github/workflows` silently disabled the whole `run:`
/// injection gate. Reading into a `Result` and returning an error on an empty
/// result set makes the gate fail closed instead.
fn get_all_workflow_files() -> Result<Vec<PathBuf>> {
    let folders: Vec<PathBuf> = WorkflowType::iter()
        .map(|workflow_type| workflow_type.folder_path())
        .collect();
    collect_workflow_files(&folders)
}

/// Takes the folder list explicitly so the fail-closed behaviour can be tested
/// without depending on the process's working directory.
fn collect_workflow_files(folders: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for folder_path in folders {
        files.extend(workflow_files_in(folder_path)?);
    }
    if files.is_empty() {
        return Err(anyhow!(
            "no workflow files found to validate; the workflow directory may be empty"
        ));
    }
    Ok(files)
}

fn workflow_files_in(folder_path: &Path) -> Result<Vec<PathBuf>> {
    let entries = fs::read_dir(folder_path).with_context(|| {
        format!(
            "failed to read workflow directory {}",
            folder_path.display()
        )
    })?;
    let mut files = Vec::new();
    for entry in entries {
        let entry = entry
            .with_context(|| format!("failed to read an entry in {}", folder_path.display()))?;
        let path = entry.path();
        if path
            .extension()
            .is_some_and(|extension| extension == "yaml" || extension == "yml")
        {
            files.push(path);
        }
    }
    Ok(files)
}

fn check_workflow(workflow_file_path: PathBuf) -> Result<(), WorkflowError> {
    fn collect_errors(
        iter: impl Iterator<Item = Result<(), Vec<RunValidationError>>>,
    ) -> Result<(), Vec<RunValidationError>> {
        Some(iter.filter_map(Result::err).flatten().collect::<Vec<_>>())
            .filter(|errors| !errors.is_empty())
            .map_or(Ok(()), Err)
    }

    fn check_recursive(key: &Value, value: &Value) -> Result<(), Vec<RunValidationError>> {
        match value {
            Value::Mapping(mapping) => collect_errors(
                mapping
                    .into_iter()
                    .map(|(key, value)| check_recursive(key, value)),
            ),
            Value::Sequence(sequence) => collect_errors(
                sequence
                    .into_iter()
                    .map(|value| check_recursive(key, value)),
            ),
            Value::String(string) => check_string(key, string).map_err(|error| vec![error]),
            Value::Null | Value::Bool(_) | Value::Number(_) | Value::Tagged(_) => Ok(()),
        }
    }

    let file_content =
        WorkflowFile::load(&workflow_file_path).map_err(WorkflowError::ParseError)?;

    check_recursive(&Value::Null, &file_content.parsed_content).map_err(|errors| {
        WorkflowError::ValidationError(Box::new(WorkflowValidationError::new(
            errors,
            file_content,
            workflow_file_path,
        )))
    })
}

fn check_string(key: &Value, value: &str) -> Result<(), RunValidationError> {
    match key {
        Value::String(key) if key == "run" => validate_run_command(value),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::collect_workflow_files;
    use std::path::PathBuf;

    #[test]
    fn rejects_a_missing_workflow_directory() {
        let folders = vec![PathBuf::from("nonexistent-workflows-dir-xyz")];
        let error = collect_workflow_files(&folders).expect_err("a missing directory must fail");
        assert!(
            error
                .to_string()
                .contains("failed to read workflow directory"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn rejects_an_empty_workflow_directory() {
        let directory = tempfile::tempdir().expect("temp dir should be created");
        let folders = vec![directory.path().to_path_buf()];
        let error = collect_workflow_files(&folders).expect_err("an empty directory must fail");
        assert!(
            error.to_string().contains("no workflow files found"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn accepts_a_directory_with_yaml_files() {
        let directory = tempfile::tempdir().expect("temp dir should be created");
        std::fs::write(directory.path().join("ci.yaml"), "name: CI\n")
            .expect("fixture should be written");
        std::fs::write(directory.path().join("notes.txt"), "ignored\n")
            .expect("fixture should be written");
        let folders = vec![directory.path().to_path_buf()];
        let files = collect_workflow_files(&folders).expect("a yaml file should be found");
        assert_eq!(files.len(), 1, "{files:?}");
        assert!(files[0].ends_with("ci.yaml"), "{files:?}");
    }
}
