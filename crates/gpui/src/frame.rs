use crate::{App, Effect, EntityId};
use collections::FxHashSet;
use std::{cell::RefCell, mem, rc::Rc};

#[cfg(feature = "frame-diagnostics")]
use crate::profiler::{
    FrameBuildId, FrameDirtyReason, FrameEvent, FrameInputProvenance, FrameInvalidation,
    next_frame_build_id, record_frame_events,
};
#[cfg(feature = "frame-diagnostics")]
use scheduler::Instant;
#[cfg(feature = "frame-diagnostics")]
use smallvec::SmallVec;

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
