mod tasks;
mod workspace;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "cargo xtask")]
struct Args {
    #[command(subcommand)]
    command: CliCommand,
}

#[derive(Subcommand)]
enum CliCommand {
    /// Builds artifacts with a single option grammar.
    Build(tasks::build::BuildArgs),
    /// Smoke-tests a distribution build in a container.
    BuildDocker(tasks::local::BuildDockerArgs),
    /// Packages a target with one option grammar for every platform.
    Bundle(tasks::bundle::BundleArgs),
    /// Runs the repository gates that do not need a compiler.
    Check(tasks::check::CheckArgs),
    /// Runs `cargo clippy`.
    Clippy(tasks::clippy::ClippyArgs),
    Compliance(tasks::compliance::ComplianceArgs),
    /// Builds every distribution artifact for a release channel.
    Dist(tasks::build::DistArgs),
    /// Checks that this machine can build a release.
    Doctor(tasks::build::doctor::DoctorArgs),
    /// Fetches the pinned FFmpeg development package.
    Ffmpeg(tasks::ffmpeg::FfmpegArgs),
    Grammars(tasks::grammars::GrammarArgs),
    /// Builds and installs a local Linux tarball into `~/.local`.
    Install(tasks::local::InstallArgs),
    Licenses(tasks::licenses::LicensesArgs),
    /// Checks that packages conform to a set of standards.
    PackageConformity(tasks::package_conformity::PackageConformityArgs),
    /// Profile-guided optimization.
    Pgo(tasks::build::pgo::PgoArgs),
    /// Publishes GPUI and its dependencies to crates.io.
    PublishGpui(tasks::publish_gpui::PublishGpuiArgs),
    /// Builds and caches the embedded remote-server matrix.
    RemoteServer(tasks::build::remote_server::RemoteServerArgs),
    /// Verifies produced artifacts.
    Verify(tasks::build::verify::VerifyArgs),
    /// Builds GPUI web examples and serves them.
    WebExamples(tasks::web_examples::WebExamplesArgs),
    Workflows(tasks::workflows::GenerateWorkflowArgs),
    CheckWorkflows(tasks::workflow_checks::WorkflowValidationArgs),
}

fn main() -> Result<()> {
    let args = Args::parse();

    match args.command {
        CliCommand::Build(args) => tasks::build::run_build(args),
        CliCommand::BuildDocker(args) => tasks::local::run_build_docker(args),
        CliCommand::Bundle(args) => tasks::bundle::run_bundle(args),
        CliCommand::Check(args) => tasks::check::run_check(args),
        CliCommand::Clippy(args) => tasks::clippy::run_clippy(args),
        CliCommand::Compliance(args) => tasks::compliance::check_compliance(args),
        CliCommand::Dist(args) => tasks::build::run_dist(args),
        CliCommand::Doctor(args) => tasks::build::doctor::run_doctor(args),
        CliCommand::Ffmpeg(args) => tasks::ffmpeg::run_ffmpeg(args),
        CliCommand::Grammars(args) => tasks::grammars::run_grammars(args),
        CliCommand::Install(args) => tasks::local::run_install(args),
        CliCommand::Licenses(args) => tasks::licenses::run_licenses(args),
        CliCommand::PackageConformity(args) => {
            tasks::package_conformity::run_package_conformity(args)
        }
        CliCommand::Pgo(args) => tasks::build::pgo::run_pgo(args),
        CliCommand::PublishGpui(args) => tasks::publish_gpui::run_publish_gpui(args),
        CliCommand::RemoteServer(args) => tasks::build::remote_server::run(args),
        CliCommand::Verify(args) => tasks::build::verify::run_verify(args),
        CliCommand::WebExamples(args) => tasks::web_examples::run_web_examples(args),
        CliCommand::Workflows(args) => tasks::workflows::run_workflows(args),
        CliCommand::CheckWorkflows(args) => tasks::workflow_checks::validate(args),
    }
}
