#[cfg(any(test, feature = "test-support"))]
use crate::Bounds;
use crate::window::{CursorStyleRequest, ElementStateBox, HitTest, TooltipRequest};
use crate::{
    AccessibilityUpdate, AnyElement, AnyMouseListener, ContentMask, CursorStyle, DispatchNodeId,
    DispatchTree, ElementId, EntityId, FocusId, GlobalElementId, Hitbox, HitboxBehavior, HitboxId,
    LineLayoutIndex, Pixels, Point, Scene, TabStopMap, TextInputOwner, TextStyleRefinement, Window,
    WindowControlArea,
};
use crate::{App, Effect};
use collections::FxHashMap;
use collections::FxHashSet;
use itertools::FoldWhile::{Continue, Done};
use itertools::Itertools;
use smallvec::SmallVec;
use std::{any::TypeId, ops::Range};
use std::{cell::RefCell, mem, rc::Rc};

#[cfg(feature = "frame-diagnostics")]
use crate::profiler::{
    FrameBuildId, FrameDirtyReason, FrameEvent, FrameInputProvenance, FrameInvalidation,
    next_frame_build_id, record_frame_events,
};
#[cfg(feature = "frame-diagnostics")]
use scheduler::Instant;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DrawPhase {
    None,
    Prepaint,
    Paint,
    Focus,
}

struct WindowInvalidatorInner {
    #[cfg(feature = "frame-diagnostics")]
    pub window_id: crate::WindowId,
    pub dirty: bool,
    pub draw_phase: DrawPhase,
    pub dirty_views: FxHashSet<EntityId>,
    pub update_count: usize,
    pub platform_waker: Option<Rc<dyn Fn()>>,
    #[cfg(feature = "frame-diagnostics")]
    pub pending_frame: Option<PendingFrameDiagnostics>,
    #[cfg(feature = "frame-diagnostics")]
    pub active_frame: Option<PendingFrameDiagnostics>,
    #[cfg(feature = "frame-diagnostics")]
    pub current_input: Option<FrameInput>,
    #[cfg(feature = "frame-diagnostics")]
    pub events: SmallVec<[FrameEvent; 16]>,
}

#[cfg(feature = "frame-diagnostics")]
#[derive(Clone, Copy)]
struct FrameInput {
    started_at: Instant,
    provenance: FrameInputProvenance,
}

#[cfg(feature = "frame-diagnostics")]
#[derive(Clone, Copy)]
pub(crate) struct PendingFrameDiagnostics {
    pub(crate) build_id: FrameBuildId,
    pub(crate) dirty_at: Instant,
    pub(crate) invalidations: u64,
    pub(crate) input_started_at: Option<Instant>,
    pub(crate) input: Option<FrameInputProvenance>,
}

#[cfg(feature = "frame-diagnostics")]
pub(crate) struct FrameInputScope {
    invalidator: WindowInvalidator,
}

#[cfg(feature = "frame-diagnostics")]
impl Drop for FrameInputScope {
    fn drop(&mut self) {
        self.invalidator.inner.borrow_mut().current_input = None;
    }
}

#[derive(Clone)]
pub(crate) struct WindowInvalidator {
    inner: Rc<RefCell<WindowInvalidatorInner>>,
}

impl WindowInvalidator {
    #[cfg(not(feature = "frame-diagnostics"))]
    pub fn new() -> Self {
        WindowInvalidator {
            inner: Rc::new(RefCell::new(WindowInvalidatorInner {
                dirty: true,
                draw_phase: DrawPhase::None,
                dirty_views: FxHashSet::default(),
                update_count: 0,
                platform_waker: None,
                #[cfg(feature = "frame-diagnostics")]
                events: SmallVec::new(),
            })),
        }
    }

    #[cfg(feature = "frame-diagnostics")]
    pub fn new(window_id: crate::WindowId) -> Self {
        let initial_frame = PendingFrameDiagnostics {
            build_id: next_frame_build_id(),
            dirty_at: Instant::now(),
            invalidations: 1,
            input_started_at: None,
            input: None,
        };
        WindowInvalidator {
            inner: Rc::new(RefCell::new(WindowInvalidatorInner {
                window_id,
                dirty: true,
                draw_phase: DrawPhase::None,
                dirty_views: FxHashSet::default(),
                update_count: 0,
                platform_waker: None,
                pending_frame: Some(initial_frame),
                active_frame: None,
                current_input: None,
                events: SmallVec::new(),
            })),
        }
        .record_initial_invalidation(window_id)
    }

    #[cfg(feature = "frame-diagnostics")]
    fn record_initial_invalidation(self, window_id: crate::WindowId) -> Self {
        let pending_frame = self.inner.borrow().pending_frame;
        if let Some(frame) = pending_frame {
            self.inner
                .borrow_mut()
                .events
                .push(FrameEvent::Invalidated(FrameInvalidation {
                    build_id: frame.build_id,
                    window_id,
                    at: frame.dirty_at,
                    entity_id: None,
                    reason: FrameDirtyReason::Initial,
                    input: None,
                    coalesced: false,
                    during_draw: false,
                }));
        }
        self
    }

    pub fn invalidate_view(&self, entity: EntityId, cx: &mut App) -> bool {
        let mut inner = self.inner.borrow_mut();
        inner.update_count += 1;
        inner.dirty_views.insert(entity);
        #[cfg(feature = "frame-diagnostics")]
        Self::record_invalidation(&mut inner, Some(entity), FrameDirtyReason::EntityNotify);
        if inner.draw_phase == DrawPhase::None {
            let became_dirty = !inner.dirty;
            inner.dirty = true;
            let waker = became_dirty.then(|| inner.platform_waker.clone()).flatten();
            drop(inner);
            cx.push_effect(Effect::Notify { emitter: entity });
            if let Some(waker) = waker {
                waker();
            }
            true
        } else {
            false
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.inner.borrow().dirty
    }

    pub fn set_dirty(&self, dirty: bool) {
        let mut inner = self.inner.borrow_mut();
        let became_dirty = dirty && !inner.dirty;
        inner.dirty = dirty;
        if dirty {
            inner.update_count += 1;
            #[cfg(feature = "frame-diagnostics")]
            Self::record_invalidation(&mut inner, None, FrameDirtyReason::WindowRefresh);
        }
        let waker = became_dirty.then(|| inner.platform_waker.clone()).flatten();
        drop(inner);
        if let Some(waker) = waker {
            waker();
        }
    }

    pub fn set_platform_waker(&self, waker: Option<Rc<dyn Fn()>>) {
        let mut inner = self.inner.borrow_mut();
        inner.platform_waker = waker;
        let waker = inner.dirty.then(|| inner.platform_waker.clone()).flatten();
        drop(inner);
        if let Some(waker) = waker {
            waker();
        }
    }

    #[cfg(feature = "frame-diagnostics")]
    fn record_invalidation(
        inner: &mut WindowInvalidatorInner,
        entity_id: Option<EntityId>,
        reason: FrameDirtyReason,
    ) {
        let at = Instant::now();
        let during_draw = inner.draw_phase != DrawPhase::None;
        let current_input = inner.current_input;
        let window_id = inner.window_id;
        let Some((frame, coalesced)) = (if during_draw {
            inner.active_frame.as_mut().map(|frame| (frame, true))
        } else {
            if inner.pending_frame.is_none() {
                inner.pending_frame = Some(PendingFrameDiagnostics {
                    build_id: next_frame_build_id(),
                    dirty_at: at,
                    invalidations: 0,
                    input_started_at: None,
                    input: None,
                });
            }
            inner.pending_frame.as_mut().map(|frame| {
                let coalesced = frame.invalidations > 0;
                (frame, coalesced)
            })
        }) else {
            return;
        };

        frame.invalidations += 1;
        if let Some(input) = current_input
            && frame.input_started_at.is_none()
        {
            frame.input_started_at = Some(input.started_at);
            frame.input = Some(input.provenance);
        }
        let build_id = frame.build_id;
        inner
            .events
            .push(FrameEvent::Invalidated(FrameInvalidation {
                build_id,
                window_id,
                at,
                entity_id,
                reason,
                input: current_input.map(|input| input.provenance),
                coalesced,
                during_draw,
            }));
    }

    #[cfg(feature = "frame-diagnostics")]
    pub(crate) fn begin_frame(&self, draw_start: Instant) -> FrameBuildId {
        let (window_id, frame) = {
            let mut inner = self.inner.borrow_mut();
            let frame = inner
                .pending_frame
                .take()
                .unwrap_or_else(|| PendingFrameDiagnostics {
                    build_id: next_frame_build_id(),
                    dirty_at: draw_start,
                    invalidations: 0,
                    input_started_at: None,
                    input: None,
                });
            inner.active_frame = Some(frame);
            (inner.window_id, frame)
        };
        self.inner
            .borrow_mut()
            .events
            .push(FrameEvent::DrawStarted {
                build_id: frame.build_id,
                window_id,
                at: draw_start,
            });
        frame.build_id
    }

    #[cfg(feature = "frame-diagnostics")]
    pub(crate) fn finish_frame(&self) -> Option<PendingFrameDiagnostics> {
        self.inner.borrow_mut().active_frame.take()
    }

    #[cfg(feature = "frame-diagnostics")]
    pub(crate) fn active_frame(&self) -> Option<PendingFrameDiagnostics> {
        self.inner.borrow().active_frame
    }

    #[cfg(feature = "frame-diagnostics")]
    pub(crate) fn input_scope(
        &self,
        started_at: Instant,
        provenance: FrameInputProvenance,
    ) -> FrameInputScope {
        self.inner.borrow_mut().current_input = Some(FrameInput {
            started_at,
            provenance,
        });
        FrameInputScope {
            invalidator: self.clone(),
        }
    }

    #[cfg(feature = "frame-diagnostics")]
    pub(crate) fn record_event(&self, event: FrameEvent) {
        self.inner.borrow_mut().events.push(event);
    }

    #[cfg(feature = "frame-diagnostics")]
    pub(crate) fn flush_events(&self) {
        let events = mem::take(&mut self.inner.borrow_mut().events);
        record_frame_events(&events);
    }

    pub fn wake_platform(&self) {
        let waker = self.inner.borrow().platform_waker.clone();
        if let Some(waker) = waker {
            waker();
        }
    }

    pub fn set_phase(&self, phase: DrawPhase) {
        self.inner.borrow_mut().draw_phase = phase
    }

    pub fn update_count(&self) -> usize {
        self.inner.borrow().update_count
    }

    pub fn take_views(&self) -> FxHashSet<EntityId> {
        mem::take(&mut self.inner.borrow_mut().dirty_views)
    }

    pub fn replace_views(&self, views: FxHashSet<EntityId>) {
        self.inner.borrow_mut().dirty_views = views;
    }

    pub fn not_drawing(&self) -> bool {
        self.inner.borrow().draw_phase == DrawPhase::None
    }

    #[track_caller]
    pub fn debug_assert_paint(&self) {
        debug_assert!(
            matches!(self.inner.borrow().draw_phase, DrawPhase::Paint),
            "this method can only be called during paint"
        );
    }

    #[track_caller]
    pub fn debug_assert_prepaint(&self) {
        debug_assert!(
            matches!(self.inner.borrow().draw_phase, DrawPhase::Prepaint),
            "this method can only be called during request_layout, or prepaint"
        );
    }

    #[track_caller]
    pub fn debug_assert_paint_or_prepaint(&self) {
        debug_assert!(
            matches!(
                self.inner.borrow().draw_phase,
                DrawPhase::Paint | DrawPhase::Prepaint
            ),
            "this method can only be called during request_layout, prepaint, or paint"
        );
    }
}

pub(crate) struct DeferredDraw {
    pub(crate) current_view: EntityId,
    pub(crate) priority: usize,
    pub(crate) parent_node: DispatchNodeId,
    pub(crate) element_id_stack: SmallVec<[ElementId; 32]>,
    pub(crate) text_style_stack: Vec<TextStyleRefinement>,
    pub(crate) content_mask: Option<ContentMask<Pixels>>,
    pub(crate) rem_size: Pixels,
    pub(crate) element: Option<AnyElement>,
    pub(crate) absolute_offset: Point<Pixels>,
    pub(crate) prepaint_range: Range<PrepaintStateIndex>,
    pub(crate) paint_range: Range<PaintIndex>,
}

pub(crate) struct Frame {
    pub(crate) focus: Option<FocusId>,
    pub(crate) window_active: bool,
    pub(crate) element_states: FxHashMap<(GlobalElementId, TypeId), ElementStateBox>,
    pub(crate) accessed_element_states: Vec<(GlobalElementId, TypeId)>,
    pub(crate) mouse_listeners: Vec<Option<AnyMouseListener>>,
    pub(crate) dispatch_tree: DispatchTree,
    pub(crate) scene: Scene,
    pub(crate) hitboxes: Vec<Hitbox>,
    pub(crate) window_control_hitboxes: Vec<(WindowControlArea, Hitbox)>,
    pub(crate) deferred_draws: Vec<DeferredDraw>,
    pub(crate) tooltip_requests: Vec<Option<TooltipRequest>>,
    pub(crate) cursor_styles: Vec<CursorStyleRequest>,
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) debug_bounds: FxHashMap<String, Bounds<Pixels>>,
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) debug_bounds_records: Vec<(String, Bounds<Pixels>)>,
    #[cfg(any(feature = "inspector", debug_assertions))]
    pub(crate) next_inspector_instance_ids: FxHashMap<Rc<crate::InspectorElementPath>, usize>,
    #[cfg(any(feature = "inspector", debug_assertions))]
    pub(crate) inspector_hitboxes: FxHashMap<HitboxId, crate::InspectorElementId>,
    pub(crate) tab_stops: TabStopMap,
}

#[derive(Clone, Default)]
pub(crate) struct PrepaintStateIndex {
    pub(crate) hitboxes_index: usize,
    pub(crate) tooltips_index: usize,
    pub(crate) deferred_draws_index: usize,
    pub(crate) dispatch_tree_index: usize,
    pub(crate) accessed_element_states_index: usize,
    pub(crate) line_layout_index: LineLayoutIndex,
}

#[derive(Clone, Default)]
pub(crate) struct PaintIndex {
    pub(crate) scene_index: usize,
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) debug_bounds_index: usize,
    pub(crate) mouse_listeners_index: usize,
    pub(crate) input_handlers_index: usize,
    pub(crate) cursor_styles_index: usize,
    pub(crate) accessed_element_states_index: usize,
    pub(crate) tab_handle_index: usize,
    pub(crate) line_layout_index: LineLayoutIndex,
}

/// Immutable view of the completed frame consumed by platform/rendering code.
#[allow(
    dead_code,
    reason = "completed-frame projections are consumed incrementally by owners"
)]
pub(crate) struct BuiltFrame<'a> {
    pub(crate) scene: &'a Scene,
    pub(crate) interaction: InteractionSnapshot<'a>,
    pub(crate) text_input: TextInputSnapshot,
    pub(crate) accessibility: AccessibilityUpdate,
    pub(crate) diagnostics: FrameDiagnosticsSnapshot,
}

#[derive(Clone, Copy)]
#[allow(
    dead_code,
    reason = "completed-frame projections are consumed incrementally by owners"
)]
pub(crate) struct InteractionSnapshot<'a> {
    pub(crate) frame: &'a Frame,
    pub(crate) mouse_hit_test: &'a HitTest,
    pub(crate) captured_hitbox: Option<HitboxId>,
    pub(crate) focus: Option<FocusId>,
}

#[derive(Clone, Copy, Default)]
#[allow(
    dead_code,
    reason = "completed-frame projections are consumed incrementally by owners"
)]
pub(crate) struct TextInputSnapshot {
    pub(crate) rendered_handler_count: usize,
    pub(crate) next_handler_count: usize,
}

#[derive(Clone, Copy, Default)]
#[allow(
    dead_code,
    reason = "completed-frame projections are consumed incrementally by owners"
)]
pub(crate) struct FrameDiagnosticsSnapshot {
    pub(crate) build_id: Option<u64>,
}

impl<'a> BuiltFrame<'a> {
    pub(crate) fn new(
        frame: &'a Frame,
        mouse_hit_test: &'a HitTest,
        captured_hitbox: Option<HitboxId>,
        focus: Option<FocusId>,
        text_input: &'a TextInputOwner,
    ) -> Self {
        Self {
            scene: &frame.scene,
            interaction: InteractionSnapshot {
                frame,
                mouse_hit_test,
                captured_hitbox,
                focus,
            },
            text_input: TextInputSnapshot {
                rendered_handler_count: text_input.rendered_handlers.len(),
                next_handler_count: text_input.next_handlers.len(),
            },
            accessibility: AccessibilityUpdate::default(),
            diagnostics: FrameDiagnosticsSnapshot::default(),
        }
    }
}

impl Frame {
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn record_debug_bounds(&mut self, selector: String, bounds: Bounds<Pixels>) {
        self.debug_bounds.insert(selector.clone(), bounds);
        self.debug_bounds_records.push((selector, bounds));
    }

    pub(crate) fn new(dispatch_tree: DispatchTree) -> Self {
        Frame {
            focus: None,
            window_active: false,
            element_states: FxHashMap::default(),
            accessed_element_states: Vec::new(),
            mouse_listeners: Vec::new(),
            dispatch_tree,
            scene: Scene::default(),
            hitboxes: Vec::new(),
            window_control_hitboxes: Vec::new(),
            deferred_draws: Vec::new(),
            tooltip_requests: Vec::new(),
            cursor_styles: Vec::new(),

            #[cfg(any(test, feature = "test-support"))]
            debug_bounds: FxHashMap::default(),
            #[cfg(any(test, feature = "test-support"))]
            debug_bounds_records: Vec::new(),

            #[cfg(any(feature = "inspector", debug_assertions))]
            next_inspector_instance_ids: FxHashMap::default(),

            #[cfg(any(feature = "inspector", debug_assertions))]
            inspector_hitboxes: FxHashMap::default(),
            tab_stops: TabStopMap::default(),
        }
    }

    pub(crate) fn clear(&mut self) {
        self.element_states.clear();
        self.accessed_element_states.clear();
        self.mouse_listeners.clear();
        self.dispatch_tree.clear();
        self.scene.clear();
        self.tooltip_requests.clear();
        self.cursor_styles.clear();
        self.hitboxes.clear();
        self.window_control_hitboxes.clear();
        self.deferred_draws.clear();
        self.tab_stops.clear();
        self.focus = None;

        #[cfg(any(test, feature = "test-support"))]
        {
            self.debug_bounds.clear();
            self.debug_bounds_records.clear();
        }

        #[cfg(any(feature = "inspector", debug_assertions))]
        {
            self.next_inspector_instance_ids.clear();
            self.inspector_hitboxes.clear();
        }
    }

    pub(crate) fn cursor_style(&self, window: &Window) -> Option<CursorStyle> {
        self.cursor_styles
            .iter()
            .rev()
            .fold_while(None, |style, request| match request.hitbox_id {
                None => Done(Some(request.style)),
                Some(hitbox_id) => Continue(style.or_else(|| {
                    hitbox_id
                        .is_hovered_ignoring_last_input(window)
                        .then_some(request.style)
                })),
            })
            .into_inner()
    }

    pub(crate) fn hit_test(&self, position: Point<Pixels>) -> HitTest {
        let mut set_hover_hitbox_count = false;
        let mut hit_test = HitTest::default();
        for hitbox in self.hitboxes.iter().rev() {
            let bounds = hitbox.bounds.intersect(&hitbox.content_mask.bounds);
            if bounds.contains(&position) {
                hit_test.ids.push(hitbox.id);
                if !set_hover_hitbox_count
                    && hitbox.behavior == HitboxBehavior::BlockMouseExceptScroll
                {
                    hit_test.hover_hitbox_count = hit_test.ids.len();
                    set_hover_hitbox_count = true;
                }
                if hitbox.behavior == HitboxBehavior::BlockMouse {
                    break;
                }
            }
        }
        if !set_hover_hitbox_count {
            hit_test.hover_hitbox_count = hit_test.ids.len();
        }
        hit_test
    }

    pub(crate) fn focus_path(&self) -> SmallVec<[FocusId; 8]> {
        self.focus
            .map(|focus_id| self.dispatch_tree.focus_path(focus_id))
            .unwrap_or_default()
    }

    pub(crate) fn finish(&mut self, prev_frame: &mut Self) {
        for element_state_key in &self.accessed_element_states {
            if let Some((element_state_key, element_state)) =
                prev_frame.element_states.remove_entry(element_state_key)
            {
                self.element_states.insert(element_state_key, element_state);
            }
        }

        self.scene.finish();
    }
}
