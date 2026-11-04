//! Rejects re-entrant borrows inside `ListState::set_scroll_handler` closures.
//!
//! `StateInner::scroll` calls the registered handler while the `ListState`'s
//! `RefCell` is still mutably borrowed (`crates/gpui/src/elements/list.rs`).
//! Any synchronous entity update or `ListState` method call inside the handler
//! borrows that same `RefCell` again and panics with `RefCell already mutably
//! borrowed`. Access must be deferred with `cx.defer` / `window.defer` so it
//! runs after the borrow is released.
//!
//! A plain text search cannot distinguish a deferred closure from a synchronous
//! one, so this gate parses the Rust source with `syn` and inspects only the
//! calls that run synchronously in the handler body.

use anyhow::{Context as _, Result, bail};
use proc_macro2::Span;
use syn::spanned::Spanned as _;
use syn::visit::{self, Visit};
use syn::{Expr, ExprMethodCall, File};

/// Methods on `ListState` that borrow its `RefCell`. Calling any of them
/// synchronously from a scroll handler re-enters the borrow and panics.
const LIST_STATE_METHODS: &[&str] = &[
    "bounds_for_item",
    "is_following_tail",
    "is_scrollbar_dragging",
    "is_scrolled_to_end",
    "item_count",
    "item_is_above_viewport",
    "item_is_below_viewport",
    "logical_scroll_top",
    "max_offset_for_scrollbar",
    "measure_all",
    "pause_following_tail",
    "remeasure",
    "remeasure_items",
    "remeasure_items_with_scroll_anchor",
    "reset",
    "scroll_by",
    "scroll_px_offset_for_scrollbar",
    "scroll_to",
    "scroll_to_end",
    "scroll_to_reveal_item",
    "scrollbar_drag_ended",
    "scrollbar_drag_started",
    "set_follow_mode",
    "set_offset_from_scrollbar",
    "splice",
    "splice_focusable",
    "viewport_bounds",
];

/// Methods that mutate or read an entity and would re-enter an update lease.
const ENTITY_METHODS: &[&str] = &["read", "read_with", "update", "update_in"];

/// Calls that defer their closure until after the current borrow is released.
const DEFER_METHODS: &[&str] = &["defer", "defer_in", "spawn", "background_spawn"];

pub fn check() -> Result<()> {
    let files = super::tracked_files("*.rs")?;
    let mut findings = Vec::new();

    for path in &files {
        // Vendored sources are upstream snapshots, not code this gate governs.
        if path.starts_with("vendor/") || path.starts_with("crates/grammars/vendor/") {
            continue;
        }
        let source = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        if !source.contains("set_scroll_handler") {
            continue;
        }
        // A parse failure must fail the gate, never silently skip a file.
        let file: File = syn::parse_file(&source)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        let label = path.display().to_string();
        findings.extend(scan_file(&file, &label));
    }

    if findings.is_empty() {
        return Ok(());
    }
    findings.sort();
    bail!(
        "scroll handlers access entities or ListState synchronously (defer with cx.defer / window.defer):\n{}",
        findings.join("\n")
    )
}

/// Returns `file:line: method` for every offending synchronous call in `file`.
fn scan_file(file: &File, file_label: &str) -> Vec<String> {
    let mut visitor = SourceVisitor {
        file_label,
        findings: Vec::new(),
    };
    visitor.visit_file(file);
    visitor.findings
}

struct SourceVisitor<'a> {
    file_label: &'a str,
    findings: Vec<String>,
}

impl<'ast> Visit<'ast> for SourceVisitor<'_> {
    fn visit_expr_method_call(&mut self, call: &'ast ExprMethodCall) {
        if call.method == "set_scroll_handler" {
            for argument in &call.args {
                if let Expr::Closure(closure) = argument {
                    let mut finder = BorrowFinder {
                        defer_depth: 0,
                        findings: Vec::new(),
                    };
                    finder.visit_expr(&closure.body);
                    for (span, method) in finder.findings {
                        self.findings.push(format!(
                            "{}:{}: synchronous .{method}() inside a scroll handler",
                            self.file_label,
                            span.start().line,
                        ));
                    }
                }
            }
        }
        visit::visit_expr_method_call(self, call);
    }
}

/// Finds entity/ListState method calls that are not nested inside a defer call.
struct BorrowFinder {
    defer_depth: usize,
    findings: Vec<(Span, String)>,
}

impl<'ast> Visit<'ast> for BorrowFinder {
    fn visit_expr_method_call(&mut self, call: &'ast ExprMethodCall) {
        let method = call.method.to_string();
        let is_defer = DEFER_METHODS.contains(&method.as_str());
        if is_defer {
            self.defer_depth += 1;
        } else if self.defer_depth == 0
            && (ENTITY_METHODS.contains(&method.as_str())
                || LIST_STATE_METHODS.contains(&method.as_str()))
        {
            self.findings.push((call.span(), method));
        }

        visit::visit_expr_method_call(self, call);

        if is_defer {
            self.defer_depth -= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::scan_file;

    fn findings(source: &str) -> Vec<String> {
        let file = syn::parse_file(source).expect("test source should parse");
        scan_file(&file, "test.rs")
    }

    #[test]
    fn rejects_a_synchronous_entity_update() {
        let source = r#"
            fn install(panel: Entity<Panel>, state: ListState) {
                state.set_scroll_handler(move |_, _, cx| {
                    panel.update(cx, |panel, cx| panel.report_viewing());
                });
            }
        "#;
        assert_eq!(
            findings(source),
            vec!["test.rs:4: synchronous .update() inside a scroll handler"]
        );
    }

    #[test]
    fn rejects_a_synchronous_list_state_read() {
        let source = r#"
            fn install(state: ListState) {
                state.set_scroll_handler(move |_, _, _| {
                    let following = state.is_following_tail();
                });
            }
        "#;
        assert_eq!(
            findings(source),
            vec!["test.rs:4: synchronous .is_following_tail() inside a scroll handler"]
        );
    }

    #[test]
    fn accepts_deferred_access() {
        let source = r#"
            fn install(panel: Entity<Panel>, state: ListState) {
                state.set_scroll_handler(move |_, window, cx| {
                    let panel = panel.clone();
                    window.defer(cx, move |window, cx| {
                        panel.update(cx, |panel, cx| panel.report_viewing(window, cx));
                    });
                    cx.defer(move |cx| {
                        panel.update(cx, |panel, cx| panel.maybe_load_older(cx));
                    });
                });
            }
        "#;
        assert!(
            findings(source).is_empty(),
            "deferred calls must be allowed"
        );
    }

    #[test]
    fn accepts_an_empty_handler() {
        let source = r#"
            fn install(state: ListState) {
                state.set_scroll_handler(|_, _, _| {});
            }
        "#;
        assert!(findings(source).is_empty());
    }

    #[test]
    fn ignores_unrelated_method_calls() {
        let source = r#"
            fn install(state: ListState) {
                state.set_scroll_handler(move |event, _, _| {
                    let start = event.visible_range.start;
                });
            }
        "#;
        assert!(findings(source).is_empty());
    }
}
