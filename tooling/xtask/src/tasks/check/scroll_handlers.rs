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
//!
//! The `ListState` method list below is maintained by hand, so an upstream
//! change that adds or removes a method could leave it stale while the gate
//! still reports success. `check_method_list` re-derives the method names from
//! the `impl ListState` block in the gpui source and fails on any drift.

use std::collections::BTreeSet;
use std::path::Path;

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
    "set_scroll_handler",
    "splice",
    "splice_focusable",
    "viewport_bounds",
];

/// Methods that mutate or read an entity and would re-enter an update lease.
const ENTITY_METHODS: &[&str] = &["read", "read_with", "update", "update_in"];

/// Calls that defer their closure until after the current borrow is released.
/// GPUI exposes window-scoped and priority-scoped variants; each runs the
/// passed closure (or async block) later rather than synchronously.
const DEFER_METHODS: &[&str] = &[
    "defer",
    "defer_in",
    "spawn",
    "spawn_in",
    "spawn_in_with_priority",
    "spawn_with_priority",
    "background_spawn",
];

/// Whether an argument to a defer call is a closure or async block whose body
/// runs later. Other arguments (a window handle, a context, a computed value)
/// are evaluated synchronously while the handler still holds the borrow.
fn is_deferred_body(argument: &Expr) -> bool {
    matches!(argument, Expr::Closure(_) | Expr::Async(_))
}

/// The source that defines `ListState`. Its method list is the authority the
/// hardcoded `LIST_STATE_METHODS` must be kept in sync with.
const LIST_STATE_SOURCE: &str = "crates/gpui/src/elements/list.rs";

/// Collects the inherent `impl ListState` instance-method names from a parsed
/// file. Returns `None` when no such impl block exists, so a refactor that
/// moves or removes it cannot leave the method-list check silently disabled.
fn impl_list_state_methods(file: &syn::File) -> Option<BTreeSet<String>> {
    let mut collector = ListStateMethodCollector {
        found_impl: false,
        methods: BTreeSet::new(),
    };
    collector.visit_file(file);
    collector.found_impl.then_some(collector.methods)
}

struct ListStateMethodCollector {
    found_impl: bool,
    methods: BTreeSet<String>,
}

impl<'ast> Visit<'ast> for ListStateMethodCollector {
    fn visit_item_impl(&mut self, item_impl: &'ast syn::ItemImpl) {
        if is_inherent_list_state_impl(item_impl) {
            self.found_impl = true;
            for member in &item_impl.items {
                if let syn::ImplItem::Fn(method) = member
                    // Associated functions such as `new` take no `self`: they
                    // construct a fresh state and cannot re-enter an existing
                    // handler's borrow, so they are deliberately not gated.
                    && method.sig.receiver().is_some()
                {
                    self.methods.insert(method.sig.ident.to_string());
                }
            }
        }
        visit::visit_item_impl(self, item_impl);
    }
}

fn is_inherent_list_state_impl(item_impl: &syn::ItemImpl) -> bool {
    if item_impl.trait_.is_some() {
        return false;
    }
    matches!(
        item_impl.self_ty.as_ref(),
        syn::Type::Path(type_path)
            if type_path.qself.is_none() && type_path.path.is_ident("ListState")
    )
}

/// Fails when `LIST_STATE_METHODS` drifts from the real `ListState` API. An
/// upstream change that adds an instance method and forgets to register it
/// would otherwise let a re-entrant call through while this gate still passes.
fn check_method_list() -> Result<()> {
    let source = std::fs::read_to_string(Path::new(LIST_STATE_SOURCE))
        .with_context(|| format!("failed to read {LIST_STATE_SOURCE}"))?;
    let file: syn::File =
        syn::parse_file(&source).with_context(|| format!("failed to parse {LIST_STATE_SOURCE}"))?;
    let derived = impl_list_state_methods(&file).with_context(|| {
        format!(
            "{LIST_STATE_SOURCE} has no inherent `impl ListState` block; the scroll-handler method list cannot be verified"
        )
    })?;

    let failures = method_list_drift(&derived);
    if failures.is_empty() {
        return Ok(());
    }
    bail!(
        "LIST_STATE_METHODS is out of sync with {LIST_STATE_SOURCE}:\n{}",
        failures.join("\n")
    )
}

/// Returns one finding for every method the gate list is missing (a real
/// `ListState` instance method whose re-entrant call would go unchecked) and
/// every stale entry (a method that no longer exists).
fn method_list_drift(derived: &BTreeSet<String>) -> Vec<String> {
    let gate: BTreeSet<String> = LIST_STATE_METHODS
        .iter()
        .map(|method| (*method).to_owned())
        .collect();

    let mut failures = Vec::new();
    for method in derived.difference(&gate) {
        failures.push(format!(
            "  - `{method}` is a ListState instance method but is missing from LIST_STATE_METHODS"
        ));
    }
    for method in gate.difference(derived) {
        failures.push(format!(
            "  - `{method}` is in LIST_STATE_METHODS but is no longer a ListState instance method"
        ));
    }
    failures
}

pub fn check() -> Result<()> {
    check_method_list()?;

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

/// Finds entity/ListState method calls that run synchronously in the handler.
///
/// A defer/spawn call only postpones its closure or async block. Its receiver
/// and any other arguments are still evaluated synchronously while the
/// `ListState` `RefCell` is borrowed, so those are scanned normally; only the
/// deferred bodies are skipped, because everything inside them runs after the
/// borrow is released.
struct BorrowFinder {
    findings: Vec<(Span, String)>,
}

impl<'ast> Visit<'ast> for BorrowFinder {
    fn visit_expr_method_call(&mut self, call: &'ast ExprMethodCall) {
        let method = call.method.to_string();

        if ENTITY_METHODS.contains(&method.as_str())
            || LIST_STATE_METHODS.contains(&method.as_str())
        {
            self.findings.push((call.span(), method.clone()));
        }

        if DEFER_METHODS.contains(&method.as_str()) {
            // Visit the synchronously-evaluated parts, but not the closure or
            // async block whose body runs after the borrow is released.
            self.visit_expr(&call.receiver);
            for argument in &call.args {
                if !is_deferred_body(argument) {
                    self.visit_expr(argument);
                }
            }
        } else {
            visit::visit_expr_method_call(self, call);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{LIST_STATE_METHODS, impl_list_state_methods, method_list_drift, scan_file};
    use std::collections::BTreeSet;

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
    fn rejects_a_nested_scroll_handler_registration() {
        // `set_scroll_handler` calls `self.0.borrow_mut()` in list.rs, so
        // re-registering a handler synchronously inside a running handler
        // re-enters the same RefCell and panics just like any other borrow.
        let source = r#"
            fn install(state: ListState) {
                state.set_scroll_handler(move |_, _, _| {
                    state.set_scroll_handler(|_, _, _| {});
                });
            }
        "#;
        assert_eq!(
            findings(source),
            vec!["test.rs:4: synchronous .set_scroll_handler() inside a scroll handler"]
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

    #[test]
    fn accepts_a_deferred_spawn_in_body() {
        // `spawn_in` is the window-scoped sibling of `spawn`; its async body
        // runs later, so entity updates inside it are not re-entrant.
        let source = r#"
            fn install(panel: Entity<Panel>, state: ListState, window: &mut Window) {
                state.set_scroll_handler(move |_, _, cx| {
                    let panel = panel.clone();
                    cx.spawn_in(window, async move |this, cx| {
                        panel.update(cx, |panel, cx| panel.refresh(cx));
                    }).detach();
                });
            }
        "#;
        assert!(
            findings(source).is_empty(),
            "spawn_in bodies are deferred and must be allowed"
        );
    }

    #[test]
    fn rejects_a_synchronous_borrow_in_a_defer_argument() {
        // Only the closure handed to `defer` runs later. An argument used to
        // build that closure is evaluated synchronously, inside the handler,
        // so a ListState borrow there still re-enters the borrow and panics.
        let source = r#"
            fn install(state: ListState) {
                state.set_scroll_handler(move |_, _, cx| {
                    cx.defer(build_handler(state.is_following_tail()));
                });
            }
        "#;
        assert_eq!(
            findings(source),
            vec!["test.rs:4: synchronous .is_following_tail() inside a scroll handler"]
        );
    }

    #[test]
    fn derives_inherent_instance_methods_only() {
        let source = r#"
            impl ListState {
                pub fn new() -> Self { Self }
                pub fn scroll_to(&self) {}
                fn private_helper(&self) {}
            }
            impl SomeTrait for ListState {
                fn trait_method(&self) {}
            }
            impl OtherType {
                pub fn unrelated(&self) {}
            }
        "#;
        let file = syn::parse_file(source).expect("test source should parse");
        let derived = impl_list_state_methods(&file).expect("an impl ListState block exists");
        // `new` takes no `self`, `SomeTrait for ListState` is a trait impl, and
        // `OtherType` is a different self type. Private inherent methods with a
        // receiver still count: they borrow state too.
        let expected: BTreeSet<String> = ["private_helper", "scroll_to"]
            .iter()
            .map(|name| (*name).to_owned())
            .collect();
        assert_eq!(derived, expected, "{derived:?}");
    }

    #[test]
    fn missing_impl_block_is_not_silently_accepted() {
        let source = "struct ListState;";
        let file = syn::parse_file(source).expect("test source should parse");
        assert!(impl_list_state_methods(&file).is_none());
    }

    #[test]
    fn reports_both_directions_of_method_list_drift() {
        let derived: BTreeSet<String> = ["item_count", "brand_new_method"]
            .iter()
            .map(|name| (*name).to_owned())
            .collect();
        let failures = method_list_drift(&derived);
        // `brand_new_method` exists in the API but not the gate list.
        assert!(
            failures
                .iter()
                .any(|failure| failure.contains("brand_new_method") && failure.contains("missing")),
            "{failures:?}"
        );
        // Every gate entry except `item_count` is now stale.
        assert!(
            failures
                .iter()
                .any(|failure| failure.contains("is_scrollbar_dragging")
                    && failure.contains("no longer")),
            "{failures:?}"
        );
        // The one method present on both sides is not reported at all.
        assert!(
            !failures
                .iter()
                .any(|failure| failure.contains("`item_count`")),
            "{failures:?}"
        );
    }

    #[test]
    fn accepts_an_exact_method_list_match() {
        let derived: BTreeSet<String> = LIST_STATE_METHODS
            .iter()
            .map(|method| (*method).to_owned())
            .collect();
        assert!(
            method_list_drift(&derived).is_empty(),
            "{:?}",
            method_list_drift(&derived)
        );
    }
}
