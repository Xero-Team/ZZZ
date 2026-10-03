//! Core accessibility tree and action routing primitives.
//!
//! This module deliberately stops at the toolkit boundary. Platform adapters
//! consume [`SemanticSnapshot`] values in later platform-specific layers; the
//! core does not import a native accessibility runtime.

use accesskit::{Action, ActionData, Node, NodeId, Role, Tree, TreeId, TreeUpdate};
use std::{
    collections::{HashMap, HashSet},
    hash::{Hash, Hasher},
};

/// The stable root node used by a window's semantic tree.
pub const ROOT_NODE_ID: NodeId = NodeId(0);

/// Derives a stable node ID from a caller-owned key.
pub fn stable_semantic_node_id(key: impl Hash) -> NodeId {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut hasher);
    let value = hasher.finish();
    NodeId(if value == ROOT_NODE_ID.0 { 1 } else { value })
}

/// A deterministic semantic tree snapshot ready for an AccessKit adapter.
#[derive(Clone, Debug, PartialEq)]
pub struct SemanticSnapshot {
    /// The AccessKit update for the complete tree.
    pub update: TreeUpdate,
    /// The node that owns keyboard focus in this snapshot.
    pub focused_node: NodeId,
}

/// Builds one complete semantic tree per frame.
pub struct SemanticTreeBuilder {
    nodes: HashMap<NodeId, Node>,
    focused_node: NodeId,
}

impl Default for SemanticTreeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl SemanticTreeBuilder {
    /// Creates a builder with a window root node.
    pub fn new() -> Self {
        let mut builder = Self {
            nodes: HashMap::new(),
            focused_node: ROOT_NODE_ID,
        };
        builder.nodes.insert(ROOT_NODE_ID, Node::new(Role::Window));
        builder
    }

    /// Adds or replaces a node in the current frame.
    pub fn set_node(&mut self, node_id: NodeId, node: Node) {
        self.nodes.insert(node_id, node);
    }

    /// Sets the children of an existing node.
    pub fn set_children(
        &mut self,
        parent_id: NodeId,
        children: impl IntoIterator<Item = NodeId>,
    ) -> Result<(), SemanticTreeError> {
        let parent = self
            .nodes
            .get_mut(&parent_id)
            .ok_or(SemanticTreeError::MissingNode(parent_id))?;
        parent.set_children(children.into_iter().collect::<Vec<_>>());
        Ok(())
    }

    /// Sets the focused node for this frame.
    pub fn set_focus(&mut self, node_id: NodeId) -> Result<(), SemanticTreeError> {
        if !self.nodes.contains_key(&node_id) {
            return Err(SemanticTreeError::MissingNode(node_id));
        }
        self.focused_node = node_id;
        Ok(())
    }

    /// Returns the number of nodes in the current frame.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Finalizes an immutable snapshot and starts no implicit next frame.
    pub fn snapshot(&self) -> SemanticSnapshot {
        let mut nodes = self
            .nodes
            .iter()
            .map(|(id, node)| (*id, node.clone()))
            .collect::<Vec<_>>();
        nodes.sort_by_key(|(id, _)| *id);
        SemanticSnapshot {
            update: TreeUpdate {
                nodes,
                tree: Some(Tree::new(ROOT_NODE_ID)),
                tree_id: TreeId::ROOT,
                focus: self.focused_node,
            },
            focused_node: self.focused_node,
        }
    }
}

/// Errors produced while assembling a semantic tree.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SemanticTreeError {
    /// The referenced node was not registered in this frame.
    MissingNode(NodeId),
}

/// Routes one AccessKit action to at most one registered handler.
#[derive(Default)]
pub struct SemanticActionRouter {
    handlers: HashMap<(NodeId, Action), Box<dyn FnMut(Option<&ActionData>)>>,
}

impl SemanticActionRouter {
    /// Registers or replaces a handler for a node/action pair.
    pub fn register(
        &mut self,
        node_id: NodeId,
        action: Action,
        handler: impl FnMut(Option<&ActionData>) + 'static,
    ) {
        self.handlers.insert((node_id, action), Box::new(handler));
    }

    /// Dispatches an action exactly once and reports whether a handler ran.
    pub fn dispatch(&mut self, node_id: NodeId, action: Action, data: Option<&ActionData>) -> bool {
        if let Some(handler) = self.handlers.get_mut(&(node_id, action)) {
            handler(data);
            true
        } else {
            false
        }
    }

    /// Returns the set of actions registered for a node.
    pub fn actions_for(&self, node_id: NodeId) -> HashSet<Action> {
        self.handlers
            .keys()
            .filter_map(|(id, action)| (*id == node_id).then_some(*action))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AccessibilityBridge, AccessibilityUpdate};
    use std::cell::Cell;
    use std::rc::Rc;

    #[test]
    fn stable_ids_are_repeatable_and_do_not_alias_the_root() {
        let first = stable_semantic_node_id("settings.font");
        assert_eq!(first, stable_semantic_node_id("settings.font"));
        assert_ne!(first, ROOT_NODE_ID);
        assert_ne!(first, stable_semantic_node_id("settings.theme"));
    }

    #[test]
    fn snapshot_contains_sorted_nodes_children_and_focus() {
        let button_id = stable_semantic_node_id("button");
        let mut builder = SemanticTreeBuilder::new();
        builder.set_node(button_id, Node::new(Role::Button));
        builder
            .set_children(ROOT_NODE_ID, [button_id])
            .expect("root should exist");
        builder.set_focus(button_id).expect("button should exist");

        let snapshot = builder.snapshot();
        assert_eq!(snapshot.focused_node, button_id);
        assert_eq!(snapshot.update.nodes.len(), 2);
        assert_eq!(snapshot.update.focus, button_id);
        assert_eq!(snapshot.update.tree_id, TreeId::ROOT);
        assert_eq!(snapshot.update.nodes[0].0, ROOT_NODE_ID);
        assert_eq!(snapshot.update.nodes[1].0, button_id);
        assert_eq!(snapshot.update.nodes[0].1.children(), &[button_id]);
    }

    #[test]
    fn action_router_dispatches_once() {
        let node_id = stable_semantic_node_id("button");
        let count = Rc::new(Cell::new(0));
        let mut router = SemanticActionRouter::default();
        router.register(node_id, Action::Click, {
            let count = count.clone();
            move |_| count.set(count.get() + 1)
        });

        assert!(router.dispatch(node_id, Action::Click, None));
        assert_eq!(count.get(), 1);
        assert!(!router.dispatch(node_id, Action::Focus, None));
        assert_eq!(router.actions_for(node_id), HashSet::from([Action::Click]));
    }

    #[test]
    fn unsupported_bridge_accepts_empty_and_rejects_semantic_updates() {
        struct UnsupportedBridge;
        impl AccessibilityBridge for UnsupportedBridge {}

        let mut bridge = UnsupportedBridge;
        assert!(
            bridge
                .update_accessibility(AccessibilityUpdate::default())
                .is_ok()
        );

        let snapshot = SemanticTreeBuilder::new().snapshot();
        let update = AccessibilityUpdate::from_semantic_snapshot(snapshot.clone());
        assert_eq!(update.semantic_snapshot(), Some(&snapshot));
        assert!(bridge.update_accessibility(update).is_err());
    }
}
