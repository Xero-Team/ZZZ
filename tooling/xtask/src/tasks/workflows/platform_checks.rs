use gh_workflow::{Container, Port};

use crate::tasks::workflows::{
    runners::{self, Arch, Platform},
    steps::{self, FluentBuilder, NamedJob, release_job, use_clang},
};

pub(crate) fn clippy(platform: Platform, arch: Option<Arch>) -> NamedJob {
    let target = arch.map(|arch| match (platform, arch) {
        (Platform::Mac, Arch::X86_64) => "x86_64-apple-darwin",
        (Platform::Mac, Arch::AARCH64) => "aarch64-apple-darwin",
        _ => unimplemented!("cross-arch clippy not supported for {platform}/{arch}"),
    });
    let runner = match platform {
        Platform::Windows => runners::WINDOWS_DEFAULT,
        Platform::Linux => runners::LINUX_DEFAULT,
        Platform::Mac => runners::MAC_DEFAULT,
    };
    let mut job = release_job(&[])
        .runs_on(runner)
        .add_step(steps::checkout_repo())
        .add_step(steps::setup_cargo_config(platform))
        .when(
            platform == Platform::Linux || platform == Platform::Mac,
            |this| this.add_step(steps::cache_rust_dependencies_namespace()),
        )
        .when(
            platform == Platform::Linux,
            steps::install_linux_dependencies,
        )
        .when_some(target, |this, target| {
            this.add_step(steps::install_rustup_target(target))
        })
        .add_step(steps::setup_sccache(platform))
        .add_step(steps::clippy(platform, target))
        .add_step(steps::show_sccache_stats(platform));
    if platform == Platform::Linux {
        job = use_clang(job);
    }
    let name = match arch {
        Some(arch) => format!("clippy_{platform}_{arch}"),
        None => format!("clippy_{platform}"),
    };
    NamedJob { name, job }
}

pub(crate) fn run_platform_tests(platform: Platform) -> NamedJob {
    run_platform_tests_impl(platform, true)
}

pub(crate) fn run_platform_tests_no_filter(platform: Platform) -> NamedJob {
    run_platform_tests_impl(platform, false)
}

fn run_platform_tests_impl(platform: Platform, filter_packages: bool) -> NamedJob {
    let runner = match platform {
        Platform::Windows => runners::WINDOWS_DEFAULT,
        Platform::Linux => runners::LINUX_DEFAULT,
        Platform::Mac => runners::MAC_DEFAULT,
    };
    NamedJob {
        name: format!("run_tests_{platform}"),
        job: release_job(&[])
            .runs_on(runner)
            .when(platform == Platform::Linux, |job| {
                job.add_service(
                    "postgres",
                    Container::new("postgres:15")
                        .add_env(("POSTGRES_HOST_AUTH_METHOD", "trust"))
                        .ports(vec![Port::Name("5432:5432".into())])
                        .options(
                            "--health-cmd pg_isready \
                             --health-interval 500ms \
                             --health-timeout 5s \
                             --health-retries 10",
                        ),
                )
            })
            .add_step(steps::checkout_repo())
            .add_step(steps::setup_cargo_config(platform))
            .when(platform == Platform::Mac, |this| {
                this.add_step(steps::cache_rust_dependencies_namespace())
            })
            .when(platform == Platform::Linux, |this| {
                use_clang(this.add_step(steps::cache_rust_dependencies_namespace()))
            })
            .when(
                platform == Platform::Linux,
                steps::install_linux_dependencies,
            )
            .add_step(steps::setup_node())
            .when(
                platform == Platform::Linux || platform == Platform::Mac,
                |job| job.add_step(steps::cargo_install_nextest()),
            )
            .add_step(steps::clear_target_dir_if_large(platform))
            .add_step(steps::setup_sccache(platform))
            .when(filter_packages, |job| {
                job.add_step(
                    steps::cargo_nextest(platform).with_changed_packages_filter("orchestrate"),
                )
            })
            .when(!filter_packages, |job| {
                job.add_step(steps::cargo_nextest(platform))
            })
            .add_step(steps::show_sccache_stats(platform))
            .add_step(steps::cleanup_cargo_config(platform)),
    }
}
