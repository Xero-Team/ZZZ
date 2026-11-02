use crate::{
    AnyElement, App, Bounds, Element, GlobalElementId, InspectorElementId, IntoElement, LayoutId,
    Pixels, Window,
};

/// Builds a `Deferred` element, which delays the layout and paint of its child.
pub fn deferred(child: impl IntoElement) -> Deferred {
    Deferred {
        child: Some(child.into_any_element()),
        priority: 0,
    }
}

/// An element which delays the painting of its child until after all of
/// its ancestors, while keeping its layout as part of the current element tree.
pub struct Deferred {
    child: Option<AnyElement>,
    priority: usize,
}

impl Deferred {
    /// Sets the `priority` value of the `deferred` element, which
    /// determines the drawing order relative to other deferred elements,
    /// with higher values being drawn on top.
    pub fn with_priority(mut self, priority: usize) -> Self {
        self.priority = priority;
        self
    }
}

impl Element for Deferred {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<crate::ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let layout_id = self
            .child
            .as_mut()
            .expect("value should have the expected type")
            .request_layout(window, cx);
        (layout_id, ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        _cx: &mut App,
    ) {
        let child = self.child.take().expect("entry should be present");
        let element_offset = window.element_offset();
        window.defer_draw(child, element_offset, self.priority, None)
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        _window: &mut Window,
        _cx: &mut App,
    ) {
    }
}

impl IntoElement for Deferred {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Deferred {
    /// Sets a priority for the element. A higher priority conceptually means painting the element
    /// on top of deferred draws with a lower priority (i.e. closer to the viewer).
    pub fn priority(mut self, priority: usize) -> Self {
        self.priority = priority;
        self
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, rc::Rc};

    use crate::{
        AnyView, Context, Entity, StyleRefinement, TestAppContext, Window, anchored, deferred, div,
        point, prelude::*, px, size,
    };
    #[cfg(feature = "accessibility")]
    use crate::{StatefulInteractiveElement as _, accesskit};

    struct SubscriptionLeaf {
        value: usize,
        output: Rc<Cell<usize>>,
    }

    struct SubscriptionPanel {
        leaf: Entity<SubscriptionLeaf>,
        deferred_child: bool,
        renders: Rc<Cell<usize>>,
    }

    impl Render for SubscriptionPanel {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            self.renders.set(self.renders.get() + 1);
            let leaf = self.leaf.clone();
            let child = crate::canvas(
                |_, _, _| {},
                move |_, _, _, cx| {
                    let leaf = leaf.read(cx);
                    leaf.output.set(leaf.value);
                },
            )
            .size(px(50.));
            if self.deferred_child {
                deferred(child).into_any_element()
            } else {
                child.into_any_element()
            }
        }
    }

    struct SubscriptionRoot {
        panel: Entity<SubscriptionPanel>,
        deferred_panel: bool,
        renders: Rc<Cell<usize>>,
    }

    impl Render for SubscriptionRoot {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            self.renders.set(self.renders.get() + 1);
            let panel =
                AnyView::from(self.panel.clone()).cached(StyleRefinement::default().size(px(100.)));
            if self.deferred_panel {
                deferred(panel).into_any_element()
            } else {
                panel.into_any_element()
            }
        }
    }

    #[gpui::test]
    fn test_cached_reuse_keeps_entity_subscriptions(cx: &mut TestAppContext) {
        for deferred_child in [true, false] {
            for deferred_panel in [false, true] {
                let output = Rc::new(Cell::new(0));
                let renders = Rc::new(Cell::new(0));
                let root_renders = Rc::new(Cell::new(0));
                let leaf = cx.new(|_| SubscriptionLeaf {
                    value: 1,
                    output: output.clone(),
                });
                let panel = cx.new(|_| SubscriptionPanel {
                    leaf: leaf.clone(),
                    deferred_child,
                    renders: renders.clone(),
                });
                let window = cx.open_window(size(px(800.), px(600.)), |_, _| SubscriptionRoot {
                    panel,
                    deferred_panel,
                    renders: root_renders.clone(),
                });
                cx.run_until_parked();
                assert_eq!(output.get(), 1);
                let initial_renders = renders.get();
                for _ in 0..2 {
                    window
                        .update(cx, |_, _, cx| cx.notify())
                        .expect("notify root");
                    cx.run_until_parked();
                    assert_eq!(
                        renders.get(),
                        initial_renders,
                        "panel should reuse its cache"
                    );
                }

                let renders_before_notify = root_renders.get();
                leaf.update(cx, |leaf, cx| {
                    leaf.value = 2;
                    cx.notify();
                });
                cx.run_until_parked();
                let context =
                    format!("deferred child {deferred_child}, deferred panel {deferred_panel}");
                assert!(
                    root_renders.get() > renders_before_notify,
                    "leaf notification should redraw: {context}"
                );
                // A plain entity read has no dispatch-tree path to invalidate the
                // cached panel, so refresh to run the paint callback again.
                window
                    .update(cx, |_, window, _| window.refresh())
                    .expect("refresh canvas");
                cx.run_until_parked();
                assert_eq!(output.get(), 2, "{context}");
            }
        }
    }

    /// A stand-in for a dock panel hosting a popover (deferred draw) whose
    /// content opens another popover (a deferred draw created while
    /// prepainting the first one's content).
    struct PanelView;

    impl Render for PanelView {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            div().key_context("Panel").size_full().child(
                deferred(
                    anchored().position(point(px(10.), px(10.))).child(
                        div().key_context("Popover").w(px(200.)).h(px(200.)).child(
                            deferred(
                                anchored().position(point(px(30.), px(30.))).child(
                                    div()
                                        .key_context("NestedMenu")
                                        .debug_selector(|| "NESTED_MENU".into())
                                        .w(px(50.))
                                        .h(px(50.)),
                                ),
                            )
                            .with_priority(2),
                        ),
                    ),
                )
                .with_priority(1),
            )
        }
    }

    struct RootView {
        panel: Entity<PanelView>,
    }

    impl Render for RootView {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            div().key_context("Root").size_full().child(
                AnyView::from(self.panel.clone()).cached(StyleRefinement::default().size_full()),
            )
        }
    }

    /// Regression test for a crash with nested deferred draws (e.g. a popover
    /// menu inside a popover hosted by a cached dock panel). Prepaint indices
    /// recorded during the deferred draw rounds must index the same
    /// `deferred_draws` vector that `reuse_prepaint` slices on the next frame;
    /// previously they were measured against a transient per-round vector, so
    /// reusing the panel's subtree grafted the wrong deferred draws and
    /// panicked in the dispatch tree.
    #[gpui::test]
    fn test_nested_deferred_draws_with_reused_views(cx: &mut TestAppContext) {
        let window = cx.open_window(size(px(800.), px(600.)), |_, cx| {
            let panel = cx.new(|_| PanelView);
            RootView { panel }
        });
        cx.run_until_parked();

        let menu_bounds = window
            .update(cx, |_, window, _| {
                window
                    .interaction
                    .rendered_frame
                    .debug_bounds
                    .get("NESTED_MENU")
                    .copied()
            })
            .unwrap()
            .expect("NESTED_MENU debug bounds not found");
        assert_eq!(menu_bounds.size, size(px(50.), px(50.)));

        // Re-render only the root view; the panel is cached, so its subtree -
        // including both deferred draw records - is reused from the previous
        // frame.
        window.update(cx, |_, _, cx| cx.notify()).unwrap();
        cx.run_until_parked();

        // Reuse the subtree a second time, exercising ranges that were
        // themselves recorded during a reused frame.
        window.update(cx, |_, _, cx| cx.notify()).unwrap();
        cx.run_until_parked();

        // Re-render the panel itself again to prove the popovers still draw.
        window
            .update(cx, |root, _, cx| {
                root.panel.update(cx, |_, cx| cx.notify());
            })
            .unwrap();
        cx.run_until_parked();

        window
            .update(cx, |_, window, _| {
                assert_eq!(window.interaction.rendered_frame.deferred_draws.len(), 2);
                assert!(
                    window
                        .interaction
                        .rendered_frame
                        .debug_bounds
                        .contains_key("NESTED_MENU")
                );
            })
            .unwrap();
    }

    #[cfg(feature = "accessibility")]
    struct AccessibilityPanel {
        action_count: Rc<Cell<usize>>,
        show_overlay: bool,
    }

    #[cfg(feature = "accessibility")]
    impl Render for AccessibilityPanel {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            div().size_full().children(self.show_overlay.then(|| {
                deferred(
                    anchored().position(point(px(16.), px(16.))).child(
                        div()
                            .id("cached-deferred-accessibility-action")
                            .role(accesskit::Role::Button)
                            .aria_label("Deferred overlay action")
                            .on_a11y_action(accesskit::Action::Click, {
                                let action_count = self.action_count.clone();
                                move |_, _, _| action_count.set(action_count.get() + 1)
                            })
                            .w(px(120.))
                            .h(px(32.)),
                    ),
                )
                .with_priority(1)
            }))
        }
    }

    #[cfg(feature = "accessibility")]
    struct AccessibilityRoot {
        panel: Entity<AccessibilityPanel>,
    }

    #[cfg(feature = "accessibility")]
    impl Render for AccessibilityRoot {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            div().size_full().child(
                AnyView::from(self.panel.clone()).cached(StyleRefinement::default().size_full()),
            )
        }
    }

    #[cfg(feature = "accessibility")]
    #[gpui::test]
    fn cached_deferred_overlay_preserves_semantics_and_drops_stale_actions(
        cx: &mut TestAppContext,
    ) {
        let action_count = Rc::new(Cell::new(0));
        let window = cx.open_window(size(px(800.), px(600.)), {
            let action_count = action_count.clone();
            move |_, cx| {
                let panel = cx.new(|_| AccessibilityPanel {
                    action_count,
                    show_overlay: true,
                });
                AccessibilityRoot { panel }
            }
        });
        cx.run_until_parked();

        let (node_id, panel) = window
            .update(cx, |root, window, _cx| {
                let snapshot = window
                    .accessibility_snapshot_for_test()
                    .expect("deferred overlay semantic snapshot should exist");
                let node_id = snapshot
                    .update
                    .nodes
                    .iter()
                    .find(|(_, node)| {
                        node.role() == accesskit::Role::Button
                            && node.label() == Some("Deferred overlay action")
                    })
                    .map(|(node_id, _)| *node_id)
                    .expect("deferred overlay button should exist");
                (node_id, root.panel.clone())
            })
            .expect("test window should remain open");

        window
            .update(cx, |_, window, cx| {
                assert!(window.dispatch_accessibility_action(
                    node_id,
                    accesskit::Action::Click,
                    None,
                    cx,
                ));
            })
            .expect("test window should remain open");
        assert_eq!(action_count.get(), 1);

        window
            .update(cx, |_, _, cx| cx.notify())
            .expect("test window should remain open");
        cx.run_until_parked();
        window
            .update(cx, |_, window, _cx| {
                let snapshot = window
                    .accessibility_snapshot_for_test()
                    .expect("cached semantic snapshot should exist");
                assert!(snapshot.update.nodes.iter().any(|(candidate_id, node)| {
                    *candidate_id == node_id
                        && node.role() == accesskit::Role::Button
                        && node.label() == Some("Deferred overlay action")
                }));
            })
            .expect("test window should remain open");

        cx.update(|cx| {
            panel.update(cx, |panel, cx| {
                panel.show_overlay = false;
                cx.notify();
            });
        });
        cx.run_until_parked();

        window
            .update(cx, |_, window, cx| {
                let snapshot = window
                    .accessibility_snapshot_for_test()
                    .expect("overlay removal semantic snapshot should exist");
                assert!(
                    snapshot
                        .update
                        .nodes
                        .iter()
                        .all(|(candidate_id, _)| *candidate_id != node_id)
                );
                assert!(!window.dispatch_accessibility_action(
                    node_id,
                    accesskit::Action::Click,
                    None,
                    cx,
                ));
            })
            .expect("test window should remain open");
    }
}
