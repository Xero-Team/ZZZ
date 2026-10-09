//! Core accessibility tree and action routing primitives.
//!
//! This module deliberately stops at the toolkit boundary. Platform adapters
//! consume [`SemanticSnapshot`] values in later platform-specific layers; the
//! core does not import a native accessibility runtime.

use crate::{App, Window};
use accesskit::{Action, ActionData, Node, NodeId, Role, TreeId, TreeInfo, TreeUpdate};
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
    node_stack: Vec<Option<NodeId>>,
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
            node_stack: vec![Some(ROOT_NODE_ID)],
            focused_node: ROOT_NODE_ID,
        };
        builder.nodes.insert(ROOT_NODE_ID, Node::new(Role::Window));
        builder
    }

    /// Adds or replaces a node in the current frame.
    pub fn set_node(&mut self, node_id: NodeId, node: Node) {
        self.nodes.insert(node_id, node);
    }

    /// Adds a node below the current parent and makes it the parent for nested elements.
    ///
    /// A duplicate node ID suppresses this node and all of its descendants for the rest of the
    /// scope. This prevents a duplicate element ID from attaching descendants to an unrelated
    /// sibling that happened to use the same semantic node ID first.
    pub(crate) fn push_node(&mut self, node_id: NodeId, node: Node) -> Option<NodeId> {
        let Some(parent_id) = self.node_stack.last().copied().flatten() else {
            self.node_stack.push(None);
            return None;
        };

        if self.nodes.contains_key(&node_id) {
            self.node_stack.push(None);
            return None;
        }
        if let Some(parent) = self.nodes.get_mut(&parent_id) {
            let mut children = parent.children().to_vec();
            children.push(node_id);
            parent.set_children(children);
        }
        self.nodes.insert(node_id, node);
        self.node_stack.push(Some(node_id));
        Some(node_id)
    }

    /// Finishes the current element node and restores its parent.
    pub(crate) fn pop_node(&mut self) {
        debug_assert!(self.node_stack.len() > 1);
        if self.node_stack.len() > 1 {
            self.node_stack.pop();
        }
    }

    /// Clears frame-local nodes while preserving the stable window root.
    pub(crate) fn clear(&mut self) {
        self.nodes.clear();
        self.nodes.insert(ROOT_NODE_ID, Node::new(Role::Window));
        self.node_stack.clear();
        self.node_stack.push(Some(ROOT_NODE_ID));
        self.focused_node = ROOT_NODE_ID;
    }

    /// Sets the children of an existing node.
    ///
    /// Every child must be registered in the current frame, appear only once,
    /// and not already belong to another parent. To reparent a child, first
    /// remove it from its current parent in a separate call.
    pub fn set_children(
        &mut self,
        parent_id: NodeId,
        children: impl IntoIterator<Item = NodeId>,
    ) -> Result<(), SemanticTreeError> {
        if !self.nodes.contains_key(&parent_id) {
            return Err(SemanticTreeError::MissingNode(parent_id));
        }

        let children = children.into_iter().collect::<Vec<_>>();
        if let Some(missing_id) = children
            .iter()
            .find(|child_id| !self.nodes.contains_key(child_id))
        {
            return Err(SemanticTreeError::MissingNode(*missing_id));
        }
        let mut unique_children = HashSet::new();
        if let Some(duplicate_id) = children
            .iter()
            .find(|child_id| !unique_children.insert(**child_id))
        {
            return Err(SemanticTreeError::DuplicateChild(*duplicate_id));
        }
        if let Some(child_id) = children
            .iter()
            .find(|child_id| self.node_reaches(**child_id, parent_id))
        {
            return Err(SemanticTreeError::CycleDetected {
                parent: parent_id,
                child: *child_id,
            });
        }
        for child_id in &children {
            if let Some(existing_parent_id) = self.nodes.iter().find_map(|(candidate_id, node)| {
                (*candidate_id != parent_id && node.children().contains(child_id))
                    .then_some(*candidate_id)
            }) {
                return Err(SemanticTreeError::ChildAlreadyHasParent {
                    child: *child_id,
                    parent: existing_parent_id,
                });
            }
        }

        let Some(parent) = self.nodes.get_mut(&parent_id) else {
            return Err(SemanticTreeError::MissingNode(parent_id));
        };
        parent.set_children(children);
        Ok(())
    }

    fn node_reaches(&self, start_id: NodeId, target_id: NodeId) -> bool {
        let mut pending = vec![start_id];
        let mut visited = HashSet::new();

        while let Some(node_id) = pending.pop() {
            if node_id == target_id {
                return true;
            }
            if !visited.insert(node_id) {
                continue;
            }
            if let Some(node) = self.nodes.get(&node_id) {
                pending.extend(node.children().iter().copied());
            }
        }

        false
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
                tree: Some(TreeInfo::new(ROOT_NODE_ID)),
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
    /// A parent contains the same child more than once.
    DuplicateChild(NodeId),
    /// Adding the child would make the semantic tree cyclic.
    CycleDetected {
        /// The parent receiving the child.
        parent: NodeId,
        /// The child that would close the cycle.
        child: NodeId,
    },
    /// The child is already owned by another parent in this frame.
    ChildAlreadyHasParent {
        /// The child with an existing parent.
        child: NodeId,
        /// The existing parent that owns the child.
        parent: NodeId,
    },
}

/// Routes one AccessKit action to at most one registered handler.
#[derive(Default)]
pub struct SemanticActionRouter {
    handlers: HashMap<(NodeId, Action), SemanticActionListener>,
}

pub(crate) type SemanticActionListener = Box<dyn FnMut(Option<&ActionData>, &mut Window, &mut App)>;

impl SemanticActionRouter {
    /// Registers or replaces a handler for a node/action pair.
    pub fn register(
        &mut self,
        node_id: NodeId,
        action: Action,
        handler: impl FnMut(Option<&ActionData>, &mut Window, &mut App) + 'static,
    ) {
        self.handlers.insert((node_id, action), Box::new(handler));
    }

    pub(crate) fn register_boxed(
        &mut self,
        node_id: NodeId,
        action: Action,
        handler: SemanticActionListener,
    ) {
        self.handlers.insert((node_id, action), handler);
    }

    /// Dispatches an action exactly once and reports whether a handler ran.
    pub fn dispatch(
        &mut self,
        node_id: NodeId,
        action: Action,
        data: Option<&ActionData>,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        if let Some(handler) = self.handlers.get_mut(&(node_id, action)) {
            handler(data, window, cx);
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
    fn set_children_rejects_unknown_nodes_without_mutating_the_parent() {
        let button_id = stable_semantic_node_id("button");
        let missing_id = stable_semantic_node_id("missing");
        let mut builder = SemanticTreeBuilder::new();
        builder.set_node(button_id, Node::new(Role::Button));
        builder
            .set_children(ROOT_NODE_ID, [button_id])
            .expect("known child should be accepted");

        assert_eq!(
            builder.set_children(ROOT_NODE_ID, [missing_id]),
            Err(SemanticTreeError::MissingNode(missing_id))
        );

        let root = builder
            .snapshot()
            .update
            .nodes
            .into_iter()
            .find(|(node_id, _)| *node_id == ROOT_NODE_ID)
            .expect("root should exist");
        assert_eq!(root.1.children(), &[button_id]);
    }

    #[test]
    fn set_children_preserves_tree_structure_invariants() {
        let first_id = stable_semantic_node_id("first");
        let second_id = stable_semantic_node_id("second");
        let mut builder = SemanticTreeBuilder::new();
        builder.set_node(first_id, Node::new(Role::Group));
        builder.set_node(second_id, Node::new(Role::Button));
        builder
            .set_children(ROOT_NODE_ID, [first_id, second_id])
            .expect("known siblings should be accepted");

        assert_eq!(
            builder.set_children(first_id, [second_id, second_id]),
            Err(SemanticTreeError::DuplicateChild(second_id))
        );
        assert_eq!(
            builder.set_children(first_id, [second_id]),
            Err(SemanticTreeError::ChildAlreadyHasParent {
                child: second_id,
                parent: ROOT_NODE_ID,
            })
        );
        assert_eq!(
            builder.set_children(first_id, [ROOT_NODE_ID]),
            Err(SemanticTreeError::CycleDetected {
                parent: first_id,
                child: ROOT_NODE_ID,
            })
        );

        builder
            .set_children(ROOT_NODE_ID, [first_id])
            .expect("removing a child from its old parent should succeed");
        builder
            .set_children(first_id, [second_id])
            .expect("adding a removed child to its new parent should succeed");

        let root = builder
            .snapshot()
            .update
            .nodes
            .into_iter()
            .find(|(node_id, _)| *node_id == ROOT_NODE_ID)
            .expect("root should exist");
        let first = builder
            .snapshot()
            .update
            .nodes
            .into_iter()
            .find(|(node_id, _)| *node_id == first_id)
            .expect("first node should exist");
        assert_eq!(root.1.children(), &[first_id]);
        assert_eq!(first.1.children(), &[second_id]);
    }

    #[test]
    fn action_router_registers_one_handler_per_node_action_pair() {
        let node_id = stable_semantic_node_id("button");
        let mut router = SemanticActionRouter::default();
        router.register(node_id, Action::Click, |_, _, _| {});
        router.register(node_id, Action::Click, |_, _, _| {});
        assert_eq!(router.actions_for(node_id), HashSet::from([Action::Click]));
    }

    #[test]
    fn nested_nodes_are_attached_to_the_current_parent() {
        let group_id = stable_semantic_node_id("group");
        let button_id = stable_semantic_node_id("button");
        let mut builder = SemanticTreeBuilder::new();
        assert_eq!(
            builder.push_node(group_id, Node::new(Role::Group)),
            Some(group_id)
        );
        assert_eq!(
            builder.push_node(button_id, Node::new(Role::Button)),
            Some(button_id)
        );
        builder.pop_node();
        builder.pop_node();

        let snapshot = builder.snapshot();
        let root = snapshot
            .update
            .nodes
            .iter()
            .find(|(node_id, _)| *node_id == ROOT_NODE_ID)
            .expect("root node should exist");
        let group = snapshot
            .update
            .nodes
            .iter()
            .find(|(node_id, _)| *node_id == group_id)
            .expect("group node should exist");
        assert_eq!(root.1.children(), &[group_id]);
        assert_eq!(group.1.children(), &[button_id]);

        builder.clear();
        let snapshot = builder.snapshot();
        assert_eq!(snapshot.update.nodes.len(), 1);
        assert_eq!(snapshot.focused_node, ROOT_NODE_ID);
    }

    #[test]
    fn duplicate_node_suppresses_its_subtree_without_corrupting_siblings() {
        let duplicate_id = stable_semantic_node_id("duplicate");
        let suppressed_child_id = stable_semantic_node_id("suppressed-child");
        let sibling_id = stable_semantic_node_id("sibling");
        let mut builder = SemanticTreeBuilder::new();

        assert_eq!(
            builder.push_node(duplicate_id, Node::new(Role::Group)),
            Some(duplicate_id)
        );
        builder.pop_node();

        assert_eq!(
            builder.push_node(duplicate_id, Node::new(Role::Group)),
            None
        );
        assert_eq!(
            builder.push_node(suppressed_child_id, Node::new(Role::Button)),
            None
        );
        builder.pop_node();
        builder.pop_node();

        assert_eq!(
            builder.push_node(sibling_id, Node::new(Role::Button)),
            Some(sibling_id)
        );
        builder.pop_node();

        let snapshot = builder.snapshot();
        let root = snapshot
            .update
            .nodes
            .iter()
            .find(|(node_id, _)| *node_id == ROOT_NODE_ID)
            .expect("root should exist");
        assert_eq!(root.1.children(), &[duplicate_id, sibling_id]);
        assert!(
            snapshot
                .update
                .nodes
                .iter()
                .all(|(node_id, _)| *node_id != suppressed_child_id)
        );
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
