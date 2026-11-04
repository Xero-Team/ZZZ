//! Turns a driver's flat `EXPLAIN` rows into a readable tree.
//!
//! Drivers report a plan as `(id, parent, detail)` rows. Rendering them in id
//! order hides the shape of the query, so this module rebuilds the parent/child
//! structure once, in a way that cannot loop on malformed input.

use crate::driver::{ExplainNode, ExplainPlan};

/// One row of the rendered plan: a detail line and how deep it sits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplainLine {
    pub depth: usize,
    /// The driver's raw detail text, already trimmed.
    pub detail: String,
    /// The driver's node id, shown so the plan can be matched to `EXPLAIN` output.
    pub id: i64,
}

/// Depth-first flatten of the plan. Parents always come before their children.
///
/// Malformed plans (a parent id that does not exist, a self-parent, or a cycle)
/// must still produce output: unreached nodes are appended at depth zero instead
/// of being dropped or recursing forever.
pub fn flatten_plan(plan: &ExplainPlan) -> Vec<ExplainLine> {
    let ids: std::collections::HashSet<i64> = plan.nodes.iter().map(|node| node.id).collect();

    let mut children: std::collections::HashMap<i64, Vec<&ExplainNode>> =
        std::collections::HashMap::new();
    let mut roots: Vec<&ExplainNode> = Vec::new();
    for node in &plan.nodes {
        // A node is a root when its parent is not part of the plan, when it
        // claims to be its own parent, or when the driver uses the conventional
        // parentless id of 0.
        let is_root = node.parent == node.id || node.parent == 0 || !ids.contains(&node.parent);
        if is_root {
            roots.push(node);
        } else {
            children.entry(node.parent).or_default().push(node);
        }
    }

    let mut lines = Vec::with_capacity(plan.nodes.len());
    let mut visited = std::collections::HashSet::new();
    for root in roots {
        visit(root, 0, &children, &mut visited, &mut lines);
    }
    // Anything left was part of a cycle reachable only from another cycle.
    for node in &plan.nodes {
        if !visited.contains(&node.id) {
            visit(node, 0, &children, &mut visited, &mut lines);
        }
    }
    lines
}

fn visit<'a>(
    node: &'a ExplainNode,
    depth: usize,
    children: &std::collections::HashMap<i64, Vec<&'a ExplainNode>>,
    visited: &mut std::collections::HashSet<i64>,
    lines: &mut Vec<ExplainLine>,
) {
    if !visited.insert(node.id) {
        return;
    }
    lines.push(ExplainLine {
        depth,
        detail: node.detail.trim().to_owned(),
        id: node.id,
    });
    if let Some(nodes) = children.get(&node.id) {
        for child in nodes {
            visit(child, depth + 1, children, visited, lines);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: i64, parent: i64, detail: &str) -> ExplainNode {
        ExplainNode {
            id,
            parent,
            detail: detail.to_owned(),
        }
    }

    #[test]
    fn nests_children_under_their_parent() {
        let plan = ExplainPlan {
            nodes: vec![
                node(2, 0, "SCAN users"),
                node(3, 2, "USE TEMP B-TREE FOR ORDER BY"),
                node(4, 0, "SEARCH orders"),
            ],
        };
        assert_eq!(
            flatten_plan(&plan),
            vec![
                ExplainLine {
                    depth: 0,
                    detail: "SCAN users".to_owned(),
                    id: 2
                },
                ExplainLine {
                    depth: 1,
                    detail: "USE TEMP B-TREE FOR ORDER BY".to_owned(),
                    id: 3
                },
                ExplainLine {
                    depth: 0,
                    detail: "SEARCH orders".to_owned(),
                    id: 4
                },
            ]
        );
    }

    #[test]
    fn nodes_with_a_missing_parent_become_roots() {
        let plan = ExplainPlan {
            nodes: vec![node(5, 99, "SCAN t")],
        };
        assert_eq!(flatten_plan(&plan)[0].depth, 0);
    }

    #[test]
    fn a_cycle_is_rendered_once_and_does_not_recurse() {
        let plan = ExplainPlan {
            nodes: vec![node(1, 2, "A"), node(2, 1, "B"), node(3, 3, "C")],
        };
        let lines = flatten_plan(&plan);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].detail, "C");
        assert_eq!(lines[0].depth, 0);
        assert_eq!(lines[1].detail, "A");
        assert_eq!(lines[2].detail, "B");
    }

    #[test]
    fn empty_plan_flattens_to_nothing() {
        assert!(flatten_plan(&ExplainPlan { nodes: Vec::new() }).is_empty());
    }
}
