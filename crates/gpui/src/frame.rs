use crate::taffy::TaffyLayoutEngine;
use crate::window::{CursorStyleRequest, ElementStateBox, HitTest, TooltipRequest};
use crate::{
    AccessibilityUpdate, AnyElement, AnyImageCache, AnyMouseListener, Bounds, ContentMask,
    CursorStyle, DispatchNodeId, DispatchTree, ElementId, EntityId, FocusId, GlobalElementId,
    Hitbox, HitboxBehavior, HitboxId, LineLayoutIndex, Pixels, Point, Scene, TabStopMap,
    TextInputOwner, TextStyleRefinement, Window, WindowControlArea,
};
use crate::{App, Effect};
#[cfg(feature = "accessibility")]
use crate::{SemanticActionRouter, SemanticTreeBuilder};
use collections::FxHashMap;
use collections::FxHashSet;
use itertools::FoldWhile::{Continue, Done};
use itertools::Itertools;
use smallvec::SmallVec;
use std::{any::TypeId, ops::Range};
use std::{
    cell::{Cell, RefCell},
    mem,
    rc::Rc,
};

#[cfg(feature = "frame-diagnostics")]
use crate::profiler::{
    FrameBuildId, FrameDiagnosticsSourceId, FrameDirtyReason, FrameEvent, FrameInputProvenance,
    FrameInvalidation, FrameInvalidationPhase, FrameTiming, next_frame_build_id,
    next_frame_diagnostics_source_id, record_frame_events,
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

/// Mutable state used only while constructing a frame.
pub(crate) struct FrameBuilder {
    layout_engine: Option<TaffyLayoutEngine>,
    element_id_stack: SmallVec<[ElementId; 32]>,
    text_style_stack: Vec<TextStyleRefinement>,
    rendered_entity_stack: Vec<EntityId>,
    element_offset_stack: Vec<Point<Pixels>>,
    element_opacity: f32,
    content_mask_stack: Vec<ContentMask<Pixels>>,
    requested_autoscroll: Option<Bounds<Pixels>>,
    image_cache_stack: Vec<AnyImageCache>,
}

impl FrameBuilder {
    pub(crate) fn new() -> Self {
        Self {
            layout_engine: Some(TaffyLayoutEngine::new()),
            element_id_stack: SmallVec::default(),
            text_style_stack: Vec::new(),
            rendered_entity_stack: Vec::new(),
            element_offset_stack: Vec::new(),
            element_opacity: 1.0,
            content_mask_stack: Vec::new(),
            requested_autoscroll: None,
            image_cache_stack: Vec::new(),
        }
    }

    pub(crate) fn debug_assert_idle(&self) {
        debug_assert!(self.layout_engine.is_some());
        debug_assert!(self.element_id_stack.is_empty());
        debug_assert!(self.text_style_stack.is_empty());
        debug_assert!(self.rendered_entity_stack.is_empty());
        debug_assert!(self.element_offset_stack.is_empty());
        debug_assert_eq!(self.element_opacity, 1.0);
        debug_assert!(self.content_mask_stack.is_empty());
        debug_assert!(self.image_cache_stack.is_empty());
    }

    pub(crate) fn layout_engine(&mut self) -> &mut TaffyLayoutEngine {
        self.layout_engine
            .as_mut()
            .expect("layout engine should be present outside layout computation")
    }

    pub(crate) fn take_layout_engine(&mut self) -> TaffyLayoutEngine {
        self.layout_engine
            .take()
            .expect("layout engine should be present outside layout computation")
    }

    pub(crate) fn restore_layout_engine(&mut self, layout_engine: TaffyLayoutEngine) {
        debug_assert!(self.layout_engine.is_none());
        self.layout_engine = Some(layout_engine);
    }

    pub(crate) fn push_element_id(&mut self, element_id: ElementId) {
        self.element_id_stack.push(element_id);
    }

    pub(crate) fn pop_element_id(&mut self) {
        self.element_id_stack.pop();
    }

    pub(crate) fn element_id_path(&self) -> &[ElementId] {
        &self.element_id_stack
    }

    pub(crate) fn element_id_depth(&self) -> usize {
        self.element_id_stack.len()
    }

    pub(crate) fn clone_element_ids(&self) -> SmallVec<[ElementId; 32]> {
        self.element_id_stack.clone()
    }

    pub(crate) fn restore_element_ids(&mut self, element_ids: &SmallVec<[ElementId; 32]>) {
        self.element_id_stack.clone_from(element_ids);
    }

    pub(crate) fn clear_element_ids(&mut self) {
        self.element_id_stack.clear();
    }

    pub(crate) fn text_styles(&self) -> &[TextStyleRefinement] {
        &self.text_style_stack
    }

    pub(crate) fn push_text_style(&mut self, style: TextStyleRefinement) {
        self.text_style_stack.push(style);
    }

    pub(crate) fn pop_text_style(&mut self) {
        self.text_style_stack.pop();
    }

    pub(crate) fn clone_text_styles(&self) -> Vec<TextStyleRefinement> {
        self.text_style_stack.clone()
    }

    pub(crate) fn restore_text_styles(&mut self, styles: &[TextStyleRefinement]) {
        self.text_style_stack.clear();
        self.text_style_stack.extend_from_slice(styles);
    }

    pub(crate) fn clear_text_styles(&mut self) {
        self.text_style_stack.clear();
    }

    pub(crate) fn current_view(&self) -> Option<EntityId> {
        self.rendered_entity_stack.last().copied()
    }

    pub(crate) fn push_rendered_view(&mut self, id: EntityId) {
        self.rendered_entity_stack.push(id);
    }

    pub(crate) fn pop_rendered_view(&mut self) {
        self.rendered_entity_stack.pop();
    }

    pub(crate) fn push_element_offset(&mut self, offset: Point<Pixels>) {
        self.element_offset_stack.push(offset);
    }

    pub(crate) fn pop_element_offset(&mut self) {
        self.element_offset_stack.pop();
    }

    pub(crate) fn element_offset(&self) -> Point<Pixels> {
        self.element_offset_stack
            .last()
            .copied()
            .unwrap_or_default()
    }

    pub(crate) fn element_opacity(&self) -> f32 {
        self.element_opacity
    }

    pub(crate) fn replace_element_opacity(&mut self, opacity: f32) -> f32 {
        mem::replace(&mut self.element_opacity, opacity)
    }

    pub(crate) fn push_content_mask(&mut self, mask: ContentMask<Pixels>) {
        self.content_mask_stack.push(mask);
    }

    pub(crate) fn pop_content_mask(&mut self) {
        self.content_mask_stack.pop();
    }

    pub(crate) fn content_mask(&self) -> Option<ContentMask<Pixels>> {
        self.content_mask_stack.last().cloned()
    }

    pub(crate) fn request_autoscroll(&mut self, bounds: Bounds<Pixels>) {
        self.requested_autoscroll = Some(bounds);
    }

    pub(crate) fn take_autoscroll(&mut self) -> Option<Bounds<Pixels>> {
        self.requested_autoscroll.take()
    }

    pub(crate) fn reset_autoscroll(&mut self) {
        self.requested_autoscroll = None;
    }

    pub(crate) fn push_image_cache(&mut self, image_cache: AnyImageCache) {
        self.image_cache_stack.push(image_cache);
    }

    pub(crate) fn pop_image_cache(&mut self) {
        self.image_cache_stack.pop();
    }

    pub(crate) fn current_image_cache(&self) -> Option<AnyImageCache> {
        self.image_cache_stack.last().cloned()
    }
}

pub(crate) struct FrameScheduler {
    next_frame_callbacks: Rc<RefCell<Vec<Box<dyn FnOnce(&mut Window, &mut App)>>>>,
    dirty_views: FxHashSet<EntityId>,
    needs_present: Rc<Cell<bool>>,
    refreshing: bool,
}

impl FrameScheduler {
    pub(crate) fn new() -> Self {
        Self {
            next_frame_callbacks: Rc::new(RefCell::new(Vec::new())),
            dirty_views: FxHashSet::default(),
            needs_present: Rc::new(Cell::new(false)),
            refreshing: false,
        }
    }

    pub(crate) fn next_frame_callbacks_handle(
        &self,
    ) -> Rc<RefCell<Vec<Box<dyn FnOnce(&mut Window, &mut App)>>>> {
        self.next_frame_callbacks.clone()
    }

    pub(crate) fn needs_present_handle(&self) -> Rc<Cell<bool>> {
        self.needs_present.clone()
    }

    pub(crate) fn queue_next_frame(&self, callback: impl FnOnce(&mut Window, &mut App) + 'static) {
        self.next_frame_callbacks
            .borrow_mut()
            .push(Box::new(callback));
    }

    pub(crate) fn insert_dirty_view(&mut self, entity_id: EntityId) -> bool {
        self.dirty_views.insert(entity_id)
    }

    pub(crate) fn is_view_dirty(&self, entity_id: EntityId) -> bool {
        self.dirty_views.contains(&entity_id)
    }

    pub(crate) fn clear_dirty_views(&mut self) {
        self.dirty_views.clear();
    }

    pub(crate) fn is_refreshing(&self) -> bool {
        self.refreshing
    }

    pub(crate) fn set_refreshing(&mut self, refreshing: bool) {
        self.refreshing = refreshing;
    }

    pub(crate) fn replace_refreshing(&mut self, refreshing: bool) -> bool {
        mem::replace(&mut self.refreshing, refreshing)
    }

    pub(crate) fn mark_present_pending(&self) {
        self.needs_present.set(true);
    }

    pub(crate) fn clear_present_pending(&self) {
        self.needs_present.set(false);
    }
}

struct WindowInvalidatorInner {
    #[cfg(feature = "frame-diagnostics")]
    pub window_id: crate::WindowId,
    #[cfg(feature = "frame-diagnostics")]
    pub source_id: FrameDiagnosticsSourceId,
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
                source_id: next_frame_diagnostics_source_id(),
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
        Self::record_invalidation(
            &mut inner,
            Some(entity),
            FrameDirtyReason::EntityNotify {
                earliest_phase: FrameInvalidationPhase::Layout,
            },
        );
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
            Self::record_invalidation(
                &mut inner,
                None,
                FrameDirtyReason::WindowRefresh {
                    earliest_phase: FrameInvalidationPhase::Layout,
                },
            );
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
        let (source_id, events) = {
            let mut inner = self.inner.borrow_mut();
            (inner.source_id, mem::take(&mut inner.events))
        };
        record_frame_events(source_id, &events);
    }

    #[cfg(feature = "frame-diagnostics")]
    pub(crate) fn frame_diagnostics_source_id(&self) -> FrameDiagnosticsSourceId {
        self.inner.borrow().source_id
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
    pub(crate) accessibility: AccessibilityUpdate,
    #[cfg(feature = "accessibility")]
    pub(crate) accessibility_builder: SemanticTreeBuilder,
    #[cfg(feature = "accessibility")]
    pub(crate) accessibility_actions: SemanticActionRouter,
    pub(crate) diagnostics: FrameDiagnosticsSnapshot,
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

/// Immutable view of the completed frame consumed synchronously by platform/rendering code.
///
/// The scene and interaction collections are borrowed from the rendered frame instead of cloned.
/// Holding this projection therefore prevents the owner from mutating or replaying that frame for
/// the duration of submission. Owned payloads are copied out only where the platform may retain
/// them, such as accessibility updates and diagnostics metadata.
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
    pub(crate) hitboxes: &'a [Hitbox],
    pub(crate) dispatch_tree: &'a DispatchTree,
    pub(crate) mouse_hit_test: &'a HitTest,
    pub(crate) captured_hitbox: Option<HitboxId>,
    pub(crate) focus: Option<FocusId>,
    pub(crate) window_active: bool,
}

#[derive(Clone, Copy, Default)]
#[allow(
    dead_code,
    reason = "completed-frame projections are consumed incrementally by owners"
)]
pub(crate) struct TextInputSnapshot {
    pub(crate) handler_slot_count: usize,
    pub(crate) active_handler_index: Option<usize>,
}

#[derive(Clone, Copy, Default)]
#[allow(
    dead_code,
    reason = "completed-frame projections are consumed incrementally by owners"
)]
pub(crate) struct FrameDiagnosticsSnapshot {
    pub(crate) build_id: Option<u64>,
    #[cfg(feature = "frame-diagnostics")]
    timing: Option<FrameTiming>,
}

impl FrameDiagnosticsSnapshot {
    #[cfg(feature = "frame-diagnostics")]
    pub(crate) fn from_timing(timing: FrameTiming) -> Self {
        Self {
            build_id: Some(timing.build_id.as_u64()),
            timing: Some(timing),
        }
    }

    #[cfg(feature = "frame-diagnostics")]
    pub(crate) fn timing(self) -> Option<FrameTiming> {
        self.timing
    }
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
                hitboxes: &frame.hitboxes,
                dispatch_tree: &frame.dispatch_tree,
                mouse_hit_test,
                captured_hitbox,
                focus,
                window_active: frame.window_active,
            },
            text_input: TextInputSnapshot {
                handler_slot_count: text_input.rendered_handlers.len(),
                active_handler_index: text_input
                    .rendered_handlers
                    .iter()
                    .rposition(Option::is_some),
            },
            accessibility: frame.accessibility.clone(),
            diagnostics: frame.diagnostics,
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
            accessibility: AccessibilityUpdate::default(),
            #[cfg(feature = "accessibility")]
            accessibility_builder: SemanticTreeBuilder::new(),
            #[cfg(feature = "accessibility")]
            accessibility_actions: SemanticActionRouter::default(),
            diagnostics: FrameDiagnosticsSnapshot::default(),

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
        self.accessibility = AccessibilityUpdate::default();
        #[cfg(feature = "accessibility")]
        {
            self.accessibility_builder.clear();
            self.accessibility_actions = SemanticActionRouter::default();
        }
        self.diagnostics = FrameDiagnosticsSnapshot::default();
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
        #[cfg(feature = "accessibility")]
        {
            self.accessibility =
                AccessibilityUpdate::from_semantic_snapshot(self.accessibility_builder.snapshot());
        }
    }
}
