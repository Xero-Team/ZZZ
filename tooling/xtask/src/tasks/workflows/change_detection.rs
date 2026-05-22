use gh_workflow::{Job, Step};
use indexmap::IndexMap;

use crate::tasks::workflows::{
    runners,
    steps::{self, NamedJob},
    vars::PathCondition,
};

/// Controls which features `orchestrate_impl` includes in the generated script.
#[derive(PartialEq, Eq)]
enum OrchestrateTarget {
    /// For the main Zed repo: includes the cargo package filter and extension
    /// change detection, but no working-directory scoping.
    ZedRepo,
    /// For individual extension repos: scopes changed-file detection to the
    /// working directory, with no package filter or extension detection.
    Extension,
}

// Generates a bash script that checks changed files against regex patterns
// and sets GitHub output variables accordingly
pub fn orchestrate(rules: &[&PathCondition]) -> NamedJob {
    orchestrate_impl(rules, OrchestrateTarget::ZedRepo)
}

pub fn orchestrate_for_extension(rules: &[&PathCondition]) -> NamedJob {
    orchestrate_impl(rules, OrchestrateTarget::Extension)
}

fn orchestrate_impl(rules: &[&PathCondition], target: OrchestrateTarget) -> NamedJob {
    let name = "orchestrate".to_owned();
    let step_name = "filter".to_owned();
    let mut script = String::new();

    script.push_str(indoc::indoc! {r#"
        set -euo pipefail
        if [ -z "$GITHUB_BASE_REF" ]; then
          echo "Not in a PR context (i.e., push to main/stable/preview)"
          COMPARE_REV="$(git rev-parse HEAD~1)"
        else
          echo "In a PR context comparing to pull_request.base.ref"
          git fetch origin "$GITHUB_BASE_REF" --depth=350
          COMPARE_REV="$(git merge-base "origin/${GITHUB_BASE_REF}" HEAD)"
        fi
        CHANGED_FILES="$(git diff --name-only "$COMPARE_REV" "$GITHUB_SHA")"

    "#});

    if target == OrchestrateTarget::Extension {
        script.push_str(indoc::indoc! {r#"
        # When running from a subdirectory, git diff returns repo-root-relative paths.
        # Filter to only files within the current working directory and strip the prefix.
        REPO_SUBDIR="$(git rev-parse --show-prefix)"
        REPO_SUBDIR="${REPO_SUBDIR%/}"
        if [ -n "$REPO_SUBDIR" ]; then
            CHANGED_FILES="$(echo "$CHANGED_FILES" | grep "^${REPO_SUBDIR}/" | sed "s|^${REPO_SUBDIR}/||" || true)"
        fi

    "#});
    }

    script.push_str(indoc::indoc! {r#"
        check_pattern() {
          local output_name="$1"
          local pattern="$2"
          local grep_arg="$3"

          echo "$CHANGED_FILES" | grep "$grep_arg" "$pattern" && \
            echo "${output_name}=true" >> "$GITHUB_OUTPUT" || \
            echo "${output_name}=false" >> "$GITHUB_OUTPUT"
        }

    "#});

    let mut outputs = IndexMap::new();

    if target == OrchestrateTarget::ZedRepo {
        script.push_str(indoc::indoc! {r#"
        # Check for changes that require full rebuild (no filter)
        # Direct pushes to main/stable/preview always run full suite
        if [ -z "$GITHUB_BASE_REF" ]; then
          echo "Not a PR, running full test suite"
          echo "changed_packages=" >> "$GITHUB_OUTPUT"
        elif echo "$CHANGED_FILES" | grep -qP '^(rust-toolchain\.toml|\.cargo/|\.github/|Cargo\.(toml|lock)$)'; then
          echo "Toolchain, cargo config, or root Cargo files changed, will run all tests"
          echo "changed_packages=" >> "$GITHUB_OUTPUT"
        else
          # Extract changed directories from file paths
          CHANGED_DIRS=$(echo "$CHANGED_FILES" | \
            grep -oP '^(crates|tooling)/\K[^/]+' | \
            sort -u || true)

          # Build directory-to-package mapping using cargo metadata
          DIR_TO_PKG=$(cargo metadata --format-version=1 --no-deps 2>/dev/null | \
            jq -r '.packages[] | select(.manifest_path | test("crates/|tooling/")) | "\(.manifest_path | capture("(crates|tooling)/(?<dir>[^/]+)") | .dir)=\(.name)"')

          # Map directory names to package names
          FILE_CHANGED_PKGS=""
          for dir in $CHANGED_DIRS; do
            pkg=$(echo "$DIR_TO_PKG" | grep "^${dir}=" | cut -d= -f2 | head -1)
            if [ -n "$pkg" ]; then
              FILE_CHANGED_PKGS=$(printf '%s\n%s' "$FILE_CHANGED_PKGS" "$pkg")
            else
              # Fall back to directory name if no mapping found
              FILE_CHANGED_PKGS=$(printf '%s\n%s' "$FILE_CHANGED_PKGS" "$dir")
            fi
          done
          FILE_CHANGED_PKGS=$(echo "$FILE_CHANGED_PKGS" | grep -v '^$' | sort -u || true)

          # If assets/ changed, add crates that depend on those assets
          if echo "$CHANGED_FILES" | grep -qP '^assets/'; then
            FILE_CHANGED_PKGS=$(printf '%s\n%s\n%s' "$FILE_CHANGED_PKGS" "settings" "assets" | sort -u)
          fi

          # Combine all changed packages
          ALL_CHANGED_PKGS=$(echo "$FILE_CHANGED_PKGS" | grep -v '^$' || true)

          if [ -z "$ALL_CHANGED_PKGS" ]; then
            echo "No package changes detected, will run all tests"
            echo "changed_packages=" >> "$GITHUB_OUTPUT"
          else
            # Build nextest filterset with rdeps for each package
            FILTERSET=$(echo "$ALL_CHANGED_PKGS" | \
              sed 's/.*/rdeps(&)/' | \
              tr '\n' '|' | \
              sed 's/|$//')
            echo "Changed packages filterset: $FILTERSET"
            echo "changed_packages=$FILTERSET" >> "$GITHUB_OUTPUT"
          fi
        fi

    "#});

        outputs.insert(
            "changed_packages".to_owned(),
            format!("${{{{ steps.{}.outputs.changed_packages }}}}", step_name),
        );
    }

    for rule in rules {
        assert!(
            rule.set_by_step
                .borrow_mut()
                .replace(name.clone())
                .is_none()
        );
        assert!(
            outputs
                .insert(
                    rule.name.to_owned(),
                    format!("${{{{ steps.{}.outputs.{} }}}}", step_name, rule.name)
                )
                .is_none()
        );

        let grep_arg = if rule.invert { "-qvP" } else { "-qP" };
        script.push_str(&format!(
            "check_pattern \"{}\" '{}' {}\n",
            rule.name, rule.pattern, grep_arg
        ));
    }

    if target == OrchestrateTarget::ZedRepo {
        script.push_str(DETECT_CHANGED_EXTENSIONS_SCRIPT);
        script.push_str("echo \"changed_extensions=$EXTENSIONS_JSON\" >> \"$GITHUB_OUTPUT\"\n");

        outputs.insert(
            "changed_extensions".to_owned(),
            format!("${{{{ steps.{}.outputs.changed_extensions }}}}", step_name),
        );
    }

    let job = Job::default()
        .runs_on(runners::LINUX_SMALL)
        .outputs(outputs)
        .add_step(steps::checkout_repo().with_deep_history_on_non_main())
        .add_step(Step::new(step_name.clone()).run(script).id(step_name));

    NamedJob { name, job }
}

/// Bash script snippet that detects changed extension directories from `$CHANGED_FILES`.
/// Assumes `$CHANGED_FILES` is already set. Sets `$EXTENSIONS_JSON` to a JSON array of
/// changed extension paths. Callers are responsible for writing the result to `$GITHUB_OUTPUT`.
pub(crate) const DETECT_CHANGED_EXTENSIONS_SCRIPT: &str = indoc::indoc! {r#"
    # Detect changed extension directories (excluding extensions/workflows)
    CHANGED_EXTENSIONS=$(echo "$CHANGED_FILES" | grep -oP '^extensions/[^/]+(?=/)' | sort -u | grep -v '^extensions/workflows$' || true)
    # Filter out deleted extensions
    EXISTING_EXTENSIONS=""
    for ext in $CHANGED_EXTENSIONS; do
        if [ -f "$ext/extension.toml" ]; then
            EXISTING_EXTENSIONS=$(printf '%s\n%s' "$EXISTING_EXTENSIONS" "$ext")
        fi
    done
    CHANGED_EXTENSIONS=$(echo "$EXISTING_EXTENSIONS" | sed '/^$/d')
    if [ -n "$CHANGED_EXTENSIONS" ]; then
        EXTENSIONS_JSON=$(echo "$CHANGED_EXTENSIONS" | jq -R -s -c 'split("\n") | map(select(length > 0))')
    else
        EXTENSIONS_JSON="[]"
    fi
"#};
