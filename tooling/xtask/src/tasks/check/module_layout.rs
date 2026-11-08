//! Rejects `mod.rs` module files outside vendored sources.
//!
//! `.rules` and AGENTS.md require `src/some_module.rs` instead of
//! `src/some_module/mod.rs`. Review alone used to be the only thing stopping a
//! new `mod.rs`, so this gate makes the rule machine-enforced. The workspace
//! was cleaned of non-vendored `mod.rs` files when the gate was added, which is
//! why there is no exception list to maintain.

use std::path::Path;

use anyhow::{Result, bail};

pub fn check() -> Result<()> {
    let files = super::tracked_files("*.rs")?;
    let mut findings = Vec::new();

    for path in &files {
        if is_forbidden_module_file(path) {
            findings.push(path.display().to_string());
        }
    }

    if findings.is_empty() {
        return Ok(());
    }
    findings.sort();
    bail!(
        "new `mod.rs` files are not allowed (put the module in a named .rs file):\n{}",
        findings.join("\n")
    )
}

fn is_forbidden_module_file(path: &Path) -> bool {
    if !path.ends_with("mod.rs") {
        return false;
    }
    let normalized = path.to_string_lossy().replace('\\', "/");
    let is_vendored = normalized.starts_with("vendor/") || normalized.contains("/vendor/");
    !is_vendored
}

#[cfg(test)]
mod tests {
    use super::is_forbidden_module_file;
    use std::path::Path;

    fn forbidden(path: &str) -> bool {
        is_forbidden_module_file(Path::new(path))
    }

    #[test]
    fn rejects_a_new_mod_rs() {
        assert!(forbidden("crates/foo/src/widget/mod.rs"));
        assert!(forbidden("tooling/xtask/src/tasks/bundle/mod.rs"));
    }

    #[test]
    fn allows_vendored_mod_rs() {
        assert!(!forbidden("vendor/any_vec/src/ops/mod.rs"));
        assert!(!forbidden("crates/grammars/vendor/tree-sitter-x/mod.rs"));
    }

    #[test]
    fn ignores_ordinary_rust_files() {
        assert!(!forbidden("crates/foo/src/widget.rs"));
        assert!(!forbidden("tooling/xtask/src/tasks/bundle.rs"));
    }
}
