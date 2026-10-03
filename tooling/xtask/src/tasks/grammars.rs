use anyhow::{Context as _, Result, bail};
use clap::{Args, Subcommand};
use serde::Deserialize;
use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::{BufRead as _, BufReader, Write as _},
    path::{Component, Path, PathBuf},
};

const MANIFEST_PATH: &str = "crates/grammars/vendor/manifest.toml";
const VENDOR_DIRECTORY: &str = "crates/grammars/vendor";

#[derive(Args)]
pub struct GrammarArgs {
    #[command(subcommand)]
    command: GrammarCommand,
}

#[derive(Subcommand)]
enum GrammarCommand {
    /// Regenerates all vendored parsers and Rust bindings.
    Generate,
    /// Verifies that all generated files are current.
    Check,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
struct GrammarManifest {
    tree_sitter_cli_version: String,
    abi: u32,
    packages: Vec<GrammarPackage>,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
struct GrammarPackage {
    directory: PathBuf,
    compile_name: String,
    display_name: String,
    #[serde(default)]
    extra_rerun_paths: Vec<PathBuf>,
    grammars: Vec<Grammar>,
    #[serde(default)]
    queries: Vec<Query>,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
struct Grammar {
    input: PathBuf,
    output: PathBuf,
    symbol: String,
    language_constant: String,
    node_types_constant: String,
    display_name: String,
}

#[derive(Deserialize)]
struct Query {
    constant: String,
    path: PathBuf,
}

struct GeneratedFile {
    staged_path: PathBuf,
    destination_path: PathBuf,
}

enum GenerationMode {
    Generate,
    Check,
}

pub fn run_grammars(args: GrammarArgs) -> Result<()> {
    let mode = match args.command {
        GrammarCommand::Generate => GenerationMode::Generate,
        GrammarCommand::Check => GenerationMode::Check,
    };

    let workspace_root =
        std::env::current_dir().context("failed to locate the current directory")?;
    let manifest_path = workspace_root.join(MANIFEST_PATH);
    if !manifest_path.is_file() {
        bail!("cargo xtask grammars must be run from the workspace root");
    }

    let manifest_source = fs::read_to_string(&manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;
    let manifest: GrammarManifest = toml::from_str(&manifest_source)
        .with_context(|| format!("failed to parse {}", manifest_path.display()))?;

    validate_manifest(&workspace_root, &manifest)?;
    verify_tree_sitter_version(&manifest.tree_sitter_cli_version)?;

    let staging_directory = tempfile::tempdir().context("failed to create a staging directory")?;
    let generated_files = generate_all(&workspace_root, staging_directory.path(), &manifest)?;

    match mode {
        GenerationMode::Generate => install_generated_files(&generated_files),
        GenerationMode::Check => check_generated_files(&generated_files),
    }
}

fn validate_manifest(workspace_root: &Path, manifest: &GrammarManifest) -> Result<()> {
    if manifest.abi != 15 {
        bail!("vendored Tree-sitter grammars must use ABI 15");
    }

    let vendor_directory = workspace_root.join(VENDOR_DIRECTORY);
    let mut package_directories = BTreeSet::new();
    let mut output_directories = BTreeSet::new();

    for package in &manifest.packages {
        validate_relative_path(&package.directory)?;
        if !package_directories.insert(package.directory.clone()) {
            bail!("duplicate grammar package {}", package.directory.display());
        }

        let package_directory = vendor_directory.join(&package.directory);
        if !package_directory.join("tree-sitter.json").is_file() {
            bail!(
                "{} is missing tree-sitter.json",
                package_directory.display()
            );
        }

        for grammar in &package.grammars {
            validate_relative_path(&grammar.input)?;
            validate_relative_path(&grammar.output)?;

            let input_path = package_directory.join(&grammar.input);
            if !input_path.is_file() {
                bail!("missing grammar input {}", input_path.display());
            }

            let output_path = package.directory.join(&grammar.output);
            if !output_directories.insert(output_path.clone()) {
                bail!("duplicate grammar output {}", output_path.display());
            }
        }

        for query in &package.queries {
            validate_relative_path(&query.path)?;
            let query_path = package_directory.join(&query.path);
            if !query_path.is_file() {
                bail!("missing query {}", query_path.display());
            }
        }

        for extra_path in &package.extra_rerun_paths {
            validate_relative_path(extra_path)?;
            let extra_path = package_directory.join(extra_path);
            if !extra_path.is_file() {
                bail!("missing build input {}", extra_path.display());
            }
        }
    }

    Ok(())
}

fn validate_relative_path(path: &Path) -> Result<()> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        bail!(
            "manifest path must be relative and contained: {}",
            path.display()
        );
    }
    Ok(())
}

fn verify_tree_sitter_version(expected_version: &str) -> Result<()> {
    let mut command = smol::process::Command::new("tree-sitter");
    command.arg("--version");
    let output = smol::block_on(command.output()).with_context(|| {
        format!(
            "failed to run tree-sitter --version; install it with `cargo install --locked --version {expected_version} tree-sitter-cli`"
        )
    })?;
    if !output.status.success() {
        bail!("tree-sitter --version failed with {}", output.status);
    }

    let actual_version = String::from_utf8(output.stdout)
        .context("tree-sitter --version returned non-UTF-8 output")?;
    let expected_output = format!("tree-sitter {expected_version}");
    if actual_version.trim() != expected_output {
        bail!(
            "expected {expected_output}, found {}; install the required version with `cargo install --locked --version {expected_version} tree-sitter-cli`",
            actual_version.trim(),
        );
    }

    Ok(())
}

fn generate_all(
    workspace_root: &Path,
    staging_root: &Path,
    manifest: &GrammarManifest,
) -> Result<Vec<GeneratedFile>> {
    let vendor_directory = workspace_root.join(VENDOR_DIRECTORY);
    let mut generated_files = Vec::new();

    for package in &manifest.packages {
        let package_directory = vendor_directory.join(&package.directory);
        let staged_package_directory = staging_root.join(&package.directory);

        for grammar in &package.grammars {
            let staged_output_directory = staged_package_directory.join(&grammar.output);
            fs::create_dir_all(&staged_output_directory).with_context(|| {
                format!(
                    "failed to create staging directory {}",
                    staged_output_directory.display()
                )
            })?;

            generate_parser(
                &package_directory,
                &grammar.input,
                &staged_output_directory,
                manifest.abi,
            )?;
            verify_parser_abi(&staged_output_directory.join("parser.c"), manifest.abi)?;

            for relative_path in [
                Path::new("parser.c"),
                Path::new("node-types.json"),
                Path::new("tree_sitter/alloc.h"),
                Path::new("tree_sitter/array.h"),
                Path::new("tree_sitter/parser.h"),
            ] {
                let staged_path = staged_output_directory.join(relative_path);
                if !staged_path.is_file() {
                    bail!("tree-sitter did not generate {}", staged_path.display());
                }
                generated_files.push(GeneratedFile {
                    staged_path,
                    destination_path: package_directory.join(&grammar.output).join(relative_path),
                });
            }
        }

        let staged_binding_directory = staged_package_directory.join("bindings/rust");
        fs::create_dir_all(&staged_binding_directory).with_context(|| {
            format!(
                "failed to create staging directory {}",
                staged_binding_directory.display()
            )
        })?;

        let library_path = staged_binding_directory.join("lib.rs");
        fs::write(&library_path, render_rust_library(package)).with_context(|| {
            format!(
                "failed to write generated binding {}",
                library_path.display()
            )
        })?;
        format_rust_file(workspace_root, &library_path)?;
        generated_files.push(GeneratedFile {
            staged_path: library_path,
            destination_path: package_directory.join("bindings/rust/lib.rs"),
        });

        let build_script_path = staged_binding_directory.join("build.rs");
        fs::write(
            &build_script_path,
            render_rust_build_script(&package_directory, package),
        )
        .with_context(|| {
            format!(
                "failed to write generated build script {}",
                build_script_path.display()
            )
        })?;
        format_rust_file(workspace_root, &build_script_path)?;
        generated_files.push(GeneratedFile {
            staged_path: build_script_path,
            destination_path: package_directory.join("bindings/rust/build.rs"),
        });
    }

    Ok(generated_files)
}

fn format_rust_file(workspace_root: &Path, file_path: &Path) -> Result<()> {
    let mut command = smol::process::Command::new("rustfmt");
    command
        .arg("--edition")
        .arg("2024")
        .arg("--config-path")
        .arg(workspace_root.join("rustfmt.toml"))
        .arg(file_path);

    let status = smol::block_on(command.status())
        .with_context(|| format!("failed to format {}", file_path.display()))?;
    if !status.success() {
        bail!("rustfmt failed for {}", file_path.display());
    }
    Ok(())
}

fn generate_parser(
    package_directory: &Path,
    grammar_input: &Path,
    output_directory: &Path,
    abi: u32,
) -> Result<()> {
    let mut command = smol::process::Command::new("tree-sitter");
    command
        .current_dir(package_directory)
        .arg("generate")
        .arg("--abi")
        .arg(abi.to_string())
        .arg("--output")
        .arg(output_directory)
        .arg(grammar_input);

    let status = smol::block_on(command.status()).with_context(|| {
        format!(
            "failed to generate {} from {}",
            output_directory.display(),
            grammar_input.display()
        )
    })?;
    if !status.success() {
        bail!(
            "tree-sitter failed to generate {} from {}",
            output_directory.display(),
            package_directory.join(grammar_input).display()
        );
    }

    Ok(())
}

fn verify_parser_abi(parser_path: &Path, expected_abi: u32) -> Result<()> {
    let parser_file = File::open(parser_path)
        .with_context(|| format!("failed to open generated parser {}", parser_path.display()))?;
    let reader = BufReader::new(parser_file);

    for line in reader.lines().take(32) {
        let line = line.with_context(|| format!("failed to read {}", parser_path.display()))?;
        if let Some(version) = line.strip_prefix("#define LANGUAGE_VERSION ") {
            if version == expected_abi.to_string() {
                return Ok(());
            }
            bail!(
                "{} has ABI {version}, expected {expected_abi}",
                parser_path.display()
            );
        }
    }

    bail!(
        "{} does not declare LANGUAGE_VERSION",
        parser_path.display()
    )
}

fn render_rust_library(package: &GrammarPackage) -> String {
    let mut output = String::new();
    output.push_str("// @generated by `cargo xtask grammars generate`.\n\n");
    output.push_str(&format!(
        "//! Tree-sitter language support for {}.\n\n",
        package.display_name
    ));
    output.push_str("use tree_sitter_language::LanguageFn;\n\n");
    output.push_str("unsafe extern \"C\" {\n");
    for grammar in &package.grammars {
        output.push_str(&format!("    fn {}() -> *const ();\n", grammar.symbol));
    }
    output.push_str("}\n\n");

    for grammar in &package.grammars {
        output.push_str(&format!(
            "/// The Tree-sitter language function for {}.\n",
            grammar.display_name
        ));
        output.push_str("// SAFETY: The symbol is provided by the generated parser and uses Tree-sitter's language ABI.\n");
        output.push_str(&format!(
            "pub const {}: LanguageFn = unsafe {{ LanguageFn::from_raw({}) }};\n\n",
            grammar.language_constant, grammar.symbol
        ));
        output.push_str(&format!(
            "/// The generated node types for {}.\n",
            grammar.display_name
        ));
        let node_types_path = Path::new("../..")
            .join(&grammar.output)
            .join("node-types.json");
        output.push_str(&format!(
            "pub const {}: &str = include_str!({});\n\n",
            grammar.node_types_constant,
            rust_string_literal(&path_with_forward_slashes(&node_types_path))
        ));
    }

    for query in &package.queries {
        let query_path = Path::new("../..").join(&query.path);
        output.push_str(&format!(
            "pub const {}: &str = include_str!({});\n",
            query.constant,
            rust_string_literal(&path_with_forward_slashes(&query_path))
        ));
    }

    if !package.queries.is_empty() {
        output.push('\n');
    }

    output.push_str("#[cfg(test)]\nmod tests {\n    #[test]\n    fn languages_can_load() {\n        let mut parser = tree_sitter::Parser::new();\n");
    for grammar in &package.grammars {
        output.push_str(&format!(
            "        parser\n            .set_language(&super::{}.into())\n            .expect({});\n",
            grammar.language_constant,
            rust_string_literal(&format!("load {} grammar", grammar.display_name))
        ));
    }
    output.push_str("    }\n}\n");
    output
}

fn render_rust_build_script(package_directory: &Path, package: &GrammarPackage) -> String {
    let mut include_directories = BTreeSet::new();
    let mut c_sources = Vec::new();
    let mut cpp_sources = Vec::new();

    for grammar in &package.grammars {
        include_directories.insert(grammar.output.clone());
        c_sources.push(grammar.output.join("parser.c"));

        let c_scanner = grammar.output.join("scanner.c");
        if package_directory.join(&c_scanner).is_file() {
            c_sources.push(c_scanner);
        }

        let cpp_scanner = grammar.output.join("scanner.cc");
        if package_directory.join(&cpp_scanner).is_file() {
            cpp_sources.push(cpp_scanner);
        }
    }

    let mut output = String::new();
    output.push_str("// @generated by `cargo xtask grammars generate`.\n\n");
    output.push_str("fn main() -> Result<(), Box<dyn std::error::Error>> {\n");
    output.push_str("    let mut c_compiler = cc::Build::new();\n");
    output.push_str("    c_compiler\n        .std(\"c11\")\n        .flag_if_supported(\"-Wno-unused-parameter\")\n        .flag_if_supported(\"-Wno-unused-but-set-variable\")\n        .flag_if_supported(\"-Wno-trigraphs\")\n        .flag_if_supported(\"-Wno-macro-redefined\");\n");
    output.push_str("    #[cfg(target_env = \"msvc\")]\n    c_compiler.flag(\"-utf-8\");\n\n");

    for include_directory in &include_directories {
        output.push_str(&format!(
            "    c_compiler.include({});\n",
            rust_string_literal(&path_with_forward_slashes(include_directory))
        ));
    }

    output.push_str("\n    let target = std::env::var(\"TARGET\")?;\n    if target.starts_with(\"wasm32-unknown\") {\n        let wasm_headers = std::env::var(\"DEP_TREE_SITTER_LANGUAGE_WASM_HEADERS\")?;\n        c_compiler.include(wasm_headers);\n    }\n\n");

    for source in &c_sources {
        let source = path_with_forward_slashes(source);
        output.push_str(&format!(
            "    c_compiler.file({});\n    println!(\"cargo:rerun-if-changed={}\");\n",
            rust_string_literal(&source),
            source
        ));
    }
    output.push_str(&format!(
        "    c_compiler.compile({});\n",
        rust_string_literal(&package.compile_name)
    ));

    if !cpp_sources.is_empty() {
        output.push_str("\n    let mut cpp_compiler = cc::Build::new();\n    cpp_compiler\n        .cpp(true)\n        .flag_if_supported(\"-Wno-unused-parameter\")\n        .flag_if_supported(\"-Wno-unused-but-set-variable\");\n");
        for include_directory in &include_directories {
            output.push_str(&format!(
                "    cpp_compiler.include({});\n",
                rust_string_literal(&path_with_forward_slashes(include_directory))
            ));
        }
        for source in &cpp_sources {
            let source = path_with_forward_slashes(source);
            output.push_str(&format!(
                "    cpp_compiler.file({});\n    println!(\"cargo:rerun-if-changed={}\");\n",
                rust_string_literal(&source),
                source
            ));
        }
        output.push_str(&format!(
            "    cpp_compiler.compile({});\n",
            rust_string_literal(&format!("{}-scanner", package.compile_name))
        ));
    }

    for extra_path in &package.extra_rerun_paths {
        output.push_str(&format!(
            "    println!(\"cargo:rerun-if-changed={}\");\n",
            path_with_forward_slashes(extra_path)
        ));
    }

    output.push_str("\n    Ok(())\n}\n");
    output
}

fn rust_string_literal(value: &str) -> String {
    format!("{value:?}")
}

fn path_with_forward_slashes(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn install_generated_files(generated_files: &[GeneratedFile]) -> Result<()> {
    let mut updated_files = 0usize;

    for generated_file in generated_files {
        if files_match(
            &generated_file.staged_path,
            &generated_file.destination_path,
        )? {
            continue;
        }

        persist_generated_file(generated_file)?;
        println!("updated {}", generated_file.destination_path.display());
        updated_files += 1;
    }

    println!(
        "generated {} files; {} changed",
        generated_files.len(),
        updated_files
    );
    Ok(())
}

fn check_generated_files(generated_files: &[GeneratedFile]) -> Result<()> {
    let mut stale_files = Vec::new();
    for generated_file in generated_files {
        if !files_match(
            &generated_file.staged_path,
            &generated_file.destination_path,
        )? {
            stale_files.push(generated_file.destination_path.display().to_string());
        }
    }

    if stale_files.is_empty() {
        println!(
            "all {} generated grammar files are current",
            generated_files.len()
        );
        return Ok(());
    }

    bail!(
        "generated grammar files are stale:\n{}\nrun `cargo xtask grammars generate`",
        stale_files.join("\n")
    )
}

fn files_match(left: &Path, right: &Path) -> Result<bool> {
    if !right.is_file() {
        return Ok(false);
    }

    let left_contents =
        fs::read(left).with_context(|| format!("failed to read {}", left.display()))?;
    let right_contents =
        fs::read(right).with_context(|| format!("failed to read {}", right.display()))?;
    Ok(left_contents == right_contents)
}

fn persist_generated_file(generated_file: &GeneratedFile) -> Result<()> {
    let destination_parent = generated_file.destination_path.parent().with_context(|| {
        format!(
            "generated path has no parent: {}",
            generated_file.destination_path.display()
        )
    })?;
    fs::create_dir_all(destination_parent).with_context(|| {
        format!(
            "failed to create destination directory {}",
            destination_parent.display()
        )
    })?;

    let mut temporary_file =
        tempfile::NamedTempFile::new_in(destination_parent).with_context(|| {
            format!(
                "failed to create a file in {}",
                destination_parent.display()
            )
        })?;
    let mut staged_file = File::open(&generated_file.staged_path).with_context(|| {
        format!(
            "failed to open generated file {}",
            generated_file.staged_path.display()
        )
    })?;
    std::io::copy(&mut staged_file, temporary_file.as_file_mut()).with_context(|| {
        format!(
            "failed to stage generated file {}",
            generated_file.destination_path.display()
        )
    })?;
    temporary_file.as_file_mut().flush().with_context(|| {
        format!(
            "failed to flush {}",
            generated_file.destination_path.display()
        )
    })?;
    let permissions = fs::metadata(&generated_file.staged_path)
        .with_context(|| format!("failed to inspect {}", generated_file.staged_path.display()))?
        .permissions();
    temporary_file
        .as_file()
        .set_permissions(permissions)
        .with_context(|| {
            format!(
                "failed to set permissions for {}",
                generated_file.destination_path.display()
            )
        })?;
    temporary_file
        .persist(&generated_file.destination_path)
        .map_err(|error| error.error)
        .with_context(|| {
            format!(
                "failed to install generated file {}",
                generated_file.destination_path.display()
            )
        })?;
    Ok(())
}
