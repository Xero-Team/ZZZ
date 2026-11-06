#[cfg(any(feature = "inspector", debug_assertions))]
use crate::Inspector;
use crate::{
    Action, AnyDrag, AnyElement, AnyImageCache, AnyTooltip, AnyView, App, AppContext, Arena, Asset,
    AsyncWindowContext, AtlasKey, AtlasTile, AvailableSpace, Background, BorderStyle, Bounds,
    BoxShadow, Capslock, Context, Corners, CursorStyle, Decorations, DeferredDraw, DevicePixels,
    DispatchNodeId, DispatchTree, DisplayId, DrawPhase, Edges, Entity, EntityId, EventEmitter,
    FileDropEvent, FontId, Frame, FrameBuilder, FrameScheduler, Global, GlobalElementId, GlyphId,
    GlyphRasterFormat, GpuSpecs, Hsla, InputHandler, InputModality, InputPreference,
    InteractionOwner, IsZero, KeyBinding, KeyContext, KeyDownEvent, KeyEvent, Keystroke,
    KeystrokeEvent, LayoutId, Modifiers, ModifiersChangedEvent, MonochromeSprite, MouseButton,
    MouseEvent, MouseMoveEvent, MouseUpEvent, PaintIndex, Path, Pixels, PlatformAtlas,
    PlatformCapabilities, PlatformDisplay, PlatformInput, PlatformWindow, Point, PolychromeSprite,
    PrepaintStateIndex, Priority, PromptButton, PromptLevel, Quad, Render, RenderGlyphParams,
    RenderImage, RenderImageParams, RenderSvgParams, ResizeEdge, SMOOTH_SVG_SCALE_FACTOR,
    SUBPIXEL_VARIANTS_X, SUBPIXEL_VARIANTS_Y, ScaledPixels, Shadow, SharedString, Size,
    StrikethroughStyle, Style, SubpixelSprite, SubscriberSet, Subscription, SystemWindowTab,
    SystemWindowTabController, Task, TextInputClient, TextInputOwner, TextRenderingMode, TextStyle,
    TextStyleRefinement, ThermalState, TransformationMatrix, Underline, UnderlineStyle,
    WindowAppearance, WindowBackgroundAppearance, WindowBounds, WindowControls, WindowDecorations,
    WindowInvalidator, WindowOptions, WindowParams, WindowTextSystem, point, prelude::*, px, rems,
    size, transparent_black,
};
use anyhow::{Context as _, Result, anyhow};
#[cfg(any(feature = "inspector", debug_assertions))]
use collections::FxHashMap;
// `DeferredDraw::accessed_entities` exists in every build, so the type it needs
// has to be imported in every build too.
use collections::FxHashSet;
#[cfg(target_os = "macos")]
use core_video::pixel_buffer::CVPixelBuffer;
use derive_more::{Deref, DerefMut};
use futures::channel::oneshot;
#[cfg(feature = "accessibility")]
use futures::{StreamExt as _, channel::mpsc};
use gpui_util::{ResultExt, measure, post_inc};
#[cfg(feature = "input-latency-histogram")]
use hdrhistogram::Histogram;
use parking_lot::{RwLock, RwLockUpgradableReadGuard};
use raw_window_handle::{HandleError, HasDisplayHandle, HasWindowHandle};
use refineable::Refineable;
use scheduler::Instant;
use slotmap::SlotMap;
use smallvec::SmallVec;
use std::{
    any::{Any, TypeId},
    borrow::Cow,
    cell::{Cell, RefCell},
    cmp,
    fmt::{Debug, Display},
    hash::{Hash, Hasher},
    marker::PhantomData,
    mem,
    ops::Range,
    rc::Rc,
    sync::{
        Arc, Weak,
        atomic::{AtomicUsize, Ordering::SeqCst},
    },
    time::Duration,
};
use uuid::Uuid;

mod content_mask;
mod element_id;
mod handles;
mod methods;
mod methods_input;
mod methods_paint;
mod paint_quad;
mod prompts;

use crate::util::{
    atomic_incr_if_not_zero, ceil_to_device_pixel, floor_to_device_pixel, round_half_toward_zero,
    round_half_toward_zero_f64, round_stroke_to_device_pixel, round_to_device_pixel,
};
pub use content_mask::*;
pub use element_id::*;
pub use handles::*;
pub use paint_quad::*;
pub use prompts::*;

#[cfg(feature = "frame-diagnostics")]
use crate::frame::FrameDiagnosticsSnapshot as BuiltFrameDiagnosticsSnapshot;
#[cfg(feature = "frame-diagnostics")]
use crate::profiler::{
    FrameEvent, FrameInputProvenance, FramePhase, FramePhaseTiming, FramePresentationTiming,
    FrameTiming,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct QuantizedGlyphCoordinate {
    integer: i32,
    variant: u8,
}

fn quantize_glyph_coordinate(
    device_coordinate: f32,
    subpixel_variants: u8,
) -> QuantizedGlyphCoordinate {
    debug_assert!(device_coordinate.is_finite());
    debug_assert!(subpixel_variants > 0);

    let subpixel_variants = i32::from(subpixel_variants);
    let tick = round_half_toward_zero(device_coordinate * subpixel_variants as f32) as i32;
    QuantizedGlyphCoordinate {
        integer: tick.div_euclid(subpixel_variants),
        variant: tick.rem_euclid(subpixel_variants) as u8,
    }
}

/// Default window size used when no explicit size is provided.
pub const DEFAULT_WINDOW_SIZE: Size<Pixels> = size(px(1536.), px(1095.));

/// A 6:5 aspect ratio minimum window size to be used for functional,
/// additional-to-main-ZZZ windows, like the settings and rules library windows.
pub const DEFAULT_ADDITIONAL_WINDOW_SIZE: Size<Pixels> = Size {
    width: Pixels(900.),
    height: Pixels(750.),
};

/// Represents the two different phases when dispatching events.
#[derive(Default, Copy, Clone, Debug, Eq, PartialEq)]
pub enum DispatchPhase {
    /// After the capture phase comes the bubble phase, in which mouse event listeners are
    /// invoked front to back and keyboard event listeners are invoked from the focused element
    /// to the root of the element tree. This is the phase you'll most commonly want to use when
    /// registering event listeners.
    #[default]
    Bubble,
    /// During the initial capture phase, mouse event listeners are invoked back to front, and keyboard
    /// listeners are invoked from the root of the tree downward toward the focused element. This phase
    /// is used for special purposes such as clearing the "pressed" state for click events. If
    /// you stop event propagation during this phase, you need to know what you're doing. Handlers
    /// outside of the immediate region may rely on detecting non-local events during this phase.
    Capture,
}

impl DispatchPhase {
    /// Returns true if this represents the "bubble" phase.
    #[inline]
    pub fn bubble(self) -> bool {
        self == DispatchPhase::Bubble
    }

    /// Returns true if this represents the "capture" phase.
    #[inline]
    pub fn capture(self) -> bool {
        self == DispatchPhase::Capture
    }
}

pub(crate) type AnyObserver = Box<dyn FnMut(&mut Window, &mut App) -> bool + 'static>;

pub(crate) type AnyWindowFocusListener =
    Box<dyn FnMut(&WindowFocusEvent, &mut Window, &mut App) -> bool + 'static>;

pub(crate) struct WindowFocusEvent {
    pub(crate) previous_focus_path: SmallVec<[FocusId; 8]>,
    pub(crate) current_focus_path: SmallVec<[FocusId; 8]>,
}

impl WindowFocusEvent {
    pub fn is_focus_in(&self, focus_id: FocusId) -> bool {
        !self.previous_focus_path.contains(&focus_id) && self.current_focus_path.contains(&focus_id)
    }

    pub fn is_focus_out(&self, focus_id: FocusId) -> bool {
        self.previous_focus_path.contains(&focus_id) && !self.current_focus_path.contains(&focus_id)
    }
}

/// This is provided when subscribing for `Context::on_focus_out` events.
pub struct FocusOutEvent {
    /// A weak focus handle representing what was blurred.
    pub blurred: WeakFocusHandle,
}

slotmap::new_key_type! {
    /// A globally unique identifier for a focusable element.
    pub struct FocusId;
}

thread_local! {
    /// Fallback arena used when no app-specific arena is active.
    /// In production, each window draw sets CURRENT_ELEMENT_ARENA to the app's arena.
    pub(crate) static ELEMENT_ARENA: RefCell<Arena> = RefCell::new(Arena::new(1024 * 1024));

    /// Holds the current App's element arena during draw operations.
    /// This allows multiple test Apps to have isolated arenas, preventing
    /// cross-session corruption when the scheduler interleaves their tasks.
    ///
    /// The arena is shared through an `Rc` instead of a raw pointer so that the
    /// scope guard keeps it alive and every access goes through `RefCell`'s
    /// runtime checks. A raw pointer derived from the `App` would be invalidated
    /// by the `&mut` reborrows taken while drawing, which is undefined behavior
    /// under Stacked Borrows.
    static CURRENT_ELEMENT_ARENA: RefCell<Option<Rc<RefCell<Arena>>>> = const { RefCell::new(None) };
}

/// Allocates an element in the current arena. Uses the app-specific arena if one
/// is active (during draw), otherwise falls back to the thread-local ELEMENT_ARENA.
pub(crate) fn with_element_arena<R>(f: impl FnOnce(&mut Arena) -> R) -> R {
    CURRENT_ELEMENT_ARENA.with(|current| match current.borrow().as_ref() {
        Some(arena) => f(&mut arena.borrow_mut()),
        None => ELEMENT_ARENA.with_borrow_mut(f),
    })
}

/// RAII guard that sets CURRENT_ELEMENT_ARENA for the duration of a draw operation.
/// When dropped, restores the previous arena (supporting nested draws).
pub(crate) struct ElementArenaScope {
    previous: Option<Rc<RefCell<Arena>>>,
}

impl ElementArenaScope {
    /// Enter a scope where element allocations use the given arena.
    pub(crate) fn enter(arena: &Rc<RefCell<Arena>>) -> Self {
        let previous = CURRENT_ELEMENT_ARENA.with(|current| {
            let mut current = current.borrow_mut();
            let previous = current.clone();
            *current = Some(arena.clone());
            previous
        });
        Self { previous }
    }
}

impl Drop for ElementArenaScope {
    fn drop(&mut self) {
        CURRENT_ELEMENT_ARENA.with(|current| {
            *current.borrow_mut() = self.previous.take();
        });
    }
}

/// Returned when the element arena has been used and so must be cleared before the next draw.
#[must_use]
pub struct ArenaClearNeeded {
    arena: Rc<RefCell<Arena>>,
}

impl ArenaClearNeeded {
    /// Create a new ArenaClearNeeded that will clear the given arena.
    pub(crate) fn new(arena: &Rc<RefCell<Arena>>) -> Self {
        Self {
            arena: arena.clone(),
        }
    }

    /// Clear the element arena.
    pub fn clear(self) {
        self.arena.borrow_mut().clear();
    }
}

#[derive(Default)]
pub(crate) struct FocusMap {
    pub(crate) handles: SlotMap<FocusId, FocusRef>,
    pub(crate) dropped_handles: Vec<FocusId>,
}

pub(crate) struct FocusRef {
    pub(crate) ref_count: AtomicUsize,
    pub(crate) tab_index: isize,
    pub(crate) tab_stop: bool,
}

impl FocusId {
    /// Obtains whether the element associated with this handle is currently focused.
    pub fn is_focused(&self, window: &Window) -> bool {
        window.interaction.focus == Some(*self)
    }

    /// Obtains whether the element associated with this handle contains the focused
    /// element or is itself focused.
    pub fn contains_focused(&self, window: &Window, cx: &App) -> bool {
        window
            .focused(cx)
            .is_some_and(|focused| self.contains(focused.id, window))
    }

    /// Obtains whether the element associated with this handle is contained within the
    /// focused element or is itself focused.
    pub fn within_focused(&self, window: &Window, cx: &App) -> bool {
        let focused = window.focused(cx);
        focused.is_some_and(|focused| focused.id.contains(*self, window))
    }

    /// Obtains whether this handle contains the given handle in the most recently rendered frame.
    pub(crate) fn contains(&self, other: Self, window: &Window) -> bool {
        window
            .interaction
            .rendered_frame
            .dispatch_tree
            .focus_contains(*self, other)
    }
}

/// A handle which can be used to track and manipulate the focused element in a window.
pub struct FocusHandle {
    pub(crate) id: FocusId,
    handles: Arc<RwLock<FocusMap>>,
    /// The index of this element in the tab order.
    pub tab_index: isize,
    /// Whether this element can be focused by tab navigation.
    pub tab_stop: bool,
}

impl std::fmt::Debug for FocusHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("FocusHandle({:?})", self.id))
    }
}

impl FocusHandle {
    pub(crate) fn new(handles: &Arc<RwLock<FocusMap>>) -> Self {
        let id = handles.write().handles.insert(FocusRef {
            ref_count: AtomicUsize::new(1),
            tab_index: 0,
            tab_stop: false,
        });

        Self {
            id,
            tab_index: 0,
            tab_stop: false,
            handles: handles.clone(),
        }
    }

    pub(crate) fn for_id(id: FocusId, handles: &Arc<RwLock<FocusMap>>) -> Option<Self> {
        let lock = handles.read();
        let focus = lock.handles.get(id)?;
        if atomic_incr_if_not_zero(&focus.ref_count) == 0 {
            return None;
        }
        Some(Self {
            id,
            tab_index: focus.tab_index,
            tab_stop: focus.tab_stop,
            handles: handles.clone(),
        })
    }

    /// Sets the tab index of the element associated with this handle.
    pub fn tab_index(mut self, index: isize) -> Self {
        self.tab_index = index;
        if let Some(focus) = self.handles.write().handles.get_mut(self.id) {
            focus.tab_index = index;
        }
        self
    }

    /// Sets whether the element associated with this handle is a tab stop.
    ///
    /// When `false`, the element will not be included in the tab order.
    pub fn tab_stop(mut self, tab_stop: bool) -> Self {
        self.tab_stop = tab_stop;
        if let Some(focus) = self.handles.write().handles.get_mut(self.id) {
            focus.tab_stop = tab_stop;
        }
        self
    }

    /// Converts this focus handle into a weak variant, which does not prevent it from being released.
    pub fn downgrade(&self) -> WeakFocusHandle {
        WeakFocusHandle {
            id: self.id,
            handles: Arc::downgrade(&self.handles),
        }
    }

    /// Moves the focus to the element associated with this handle.
    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        window.focus(self, cx)
    }

    /// Obtains whether the element associated with this handle is currently focused.
    pub fn is_focused(&self, window: &Window) -> bool {
        self.id.is_focused(window)
    }

    /// Obtains whether the element associated with this handle contains the focused
    /// element or is itself focused.
    pub fn contains_focused(&self, window: &Window, cx: &App) -> bool {
        self.id.contains_focused(window, cx)
    }

    /// Obtains whether the element associated with this handle is contained within the
    /// focused element or is itself focused.
    pub fn within_focused(&self, window: &Window, cx: &mut App) -> bool {
        self.id.within_focused(window, cx)
    }

    /// Obtains whether this handle contains the given handle in the most recently rendered frame.
    pub fn contains(&self, other: &Self, window: &Window) -> bool {
        self.id.contains(other.id, window)
    }

    /// Dispatch an action on the element that rendered this focus handle
    pub fn dispatch_action(&self, action: &dyn Action, window: &mut Window, cx: &mut App) {
        if let Some(node_id) = window
            .interaction
            .rendered_frame
            .dispatch_tree
            .focusable_node_id(self.id)
        {
            window.dispatch_action_on_node(node_id, action, cx)
        }
    }
}

impl Clone for FocusHandle {
    fn clone(&self) -> Self {
        Self::for_id(self.id, &self.handles).expect("for_id should be present")
    }
}

impl PartialEq for FocusHandle {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for FocusHandle {}

impl Drop for FocusHandle {
    fn drop(&mut self) {
        let handles = self.handles.upgradable_read();
        let focus = handles.handles.get(self.id).expect("focus handle missing");
        let previous_count = focus.ref_count.fetch_sub(1, SeqCst);
        assert_ne!(previous_count, 0, "over-released focus handle");
        if previous_count == 1 {
            RwLockUpgradableReadGuard::upgrade(handles)
                .dropped_handles
                .push(self.id);
        }
    }
}

/// A weak reference to a focus handle.
#[derive(Clone, Debug)]
pub struct WeakFocusHandle {
    pub(crate) id: FocusId,
    pub(crate) handles: Weak<RwLock<FocusMap>>,
}

impl WeakFocusHandle {
    /// Attempts to upgrade the [WeakFocusHandle] to a [FocusHandle].
    pub fn upgrade(&self) -> Option<FocusHandle> {
        let handles = self.handles.upgrade()?;
        FocusHandle::for_id(self.id, &handles)
    }
}

impl PartialEq for WeakFocusHandle {
    fn eq(&self, other: &WeakFocusHandle) -> bool {
        self.id == other.id
    }
}

impl Eq for WeakFocusHandle {}

impl PartialEq<FocusHandle> for WeakFocusHandle {
    fn eq(&self, other: &FocusHandle) -> bool {
        self.id == other.id
    }
}

impl PartialEq<WeakFocusHandle> for FocusHandle {
    fn eq(&self, other: &WeakFocusHandle) -> bool {
        self.id == other.id
    }
}

/// Focusable allows users of your view to easily
/// focus it (using window.focus_view(cx, view))
pub trait Focusable: 'static {
    /// Returns the focus handle associated with this view.
    fn focus_handle(&self, cx: &App) -> FocusHandle;
}

impl<V: Focusable> Focusable for Entity<V> {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.read(cx).focus_handle(cx)
    }
}

/// ManagedView is a view (like a Modal, Popover, Menu, etc.)
/// where the lifecycle of the view is handled by another view.
pub trait ManagedView: Focusable + EventEmitter<DismissEvent> + Render {}

impl<M: Focusable + EventEmitter<DismissEvent> + Render> ManagedView for M {}

/// Emitted by implementers of [`ManagedView`] to indicate the view should be dismissed, such as when a view is presented as a modal.
pub struct DismissEvent;

pub(crate) type AnyMouseListener =
    Box<dyn FnMut(&dyn Any, DispatchPhase, &mut Window, &mut App) + 'static>;

#[derive(Clone)]
pub(crate) struct CursorStyleRequest {
    pub(crate) hitbox_id: Option<HitboxId>,
    pub(crate) style: CursorStyle,
}

#[derive(Default, Eq, PartialEq)]
pub(crate) struct HitTest {
    pub(crate) ids: SmallVec<[HitboxId; 8]>,
    pub(crate) hover_hitbox_count: usize,
}

/// A type of window control area that corresponds to the platform window.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowControlArea {
    /// An area that allows dragging of the platform window.
    Drag,
    /// An area that allows closing of the platform window.
    Close,
    /// An area that allows maximizing of the platform window.
    Max,
    /// An area that allows minimizing of the platform window.
    Min,
}

/// An identifier for a [Hitbox] which also includes [HitboxBehavior].
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct HitboxId(u64);

#[cfg(any(test, feature = "test-support"))]
impl HitboxId {
    /// A placeholder HitboxId exclusively for integration testing API's that
    /// need a hitbox but where the value of the hitbox does not matter. The
    /// alternative is to make the Hitbox optional but that complicates the
    /// implementation.
    pub const fn placeholder() -> Self {
        Self(0)
    }
}

impl HitboxId {
    /// Checks if the hitbox with this ID is currently hovered. Returns `false` during keyboard
    /// input modality so that keyboard navigation suppresses hover highlights. Except when handling
    /// `ScrollWheelEvent`, this is typically what you want when determining whether to handle mouse
    /// events or paint hover styles.
    ///
    /// See [`Hitbox::is_hovered`] for details.
    pub fn is_hovered(self, window: &Window) -> bool {
        // If this hitbox has captured the pointer, it's always considered hovered
        if window.interaction.captured_hitbox == Some(self) {
            return true;
        }
        if window.last_input_was_keyboard() {
            return false;
        }
        self.hit_test(window)
    }

    /// Checks if the hitbox with this ID is currently hovered, regardless of the last
    /// input modality used.
    ///
    /// See [`HitboxId::is_hovered`] for more details.
    pub(crate) fn is_hovered_ignoring_last_input(self, window: &Window) -> bool {
        // If this hitbox has captured the pointer, it's always considered hovered
        if window.interaction.captured_hitbox == Some(self) {
            return true;
        }
        self.hit_test(window)
    }

    fn hit_test(self, window: &Window) -> bool {
        let hit_test = &window.interaction.mouse_hit_test;
        for id in hit_test.ids.iter().take(hit_test.hover_hitbox_count) {
            if self == *id {
                return true;
            }
        }
        false
    }

    /// Checks if the hitbox with this ID contains the mouse and should handle scroll events.
    /// Typically this should only be used when handling `ScrollWheelEvent`, and otherwise
    /// `is_hovered` should be used. See the documentation of `Hitbox::is_hovered` for details about
    /// this distinction.
    pub fn should_handle_scroll(self, window: &Window) -> bool {
        window.interaction.mouse_hit_test.ids.contains(&self)
    }

    pub(crate) fn next(self) -> HitboxId {
        HitboxId(self.0.wrapping_add(1))
    }
}

/// A rectangular region that potentially blocks hitboxes inserted prior.
/// See [Window::insert_hitbox] for more details.
#[derive(Clone, Debug, Deref)]
pub struct Hitbox {
    /// A unique identifier for the hitbox.
    pub id: HitboxId,
    /// The bounds of the hitbox.
    #[deref]
    pub bounds: Bounds<Pixels>,
    /// The content mask when the hitbox was inserted.
    pub content_mask: ContentMask<Pixels>,
    /// Flags that specify hitbox behavior.
    pub behavior: HitboxBehavior,
}

impl Hitbox {
    /// Checks if the hitbox is currently hovered. Returns `false` during keyboard input modality
    /// so that keyboard navigation suppresses hover highlights. Except when handling
    /// `ScrollWheelEvent`, this is typically what you want when determining whether to handle mouse
    /// events or paint hover styles.
    ///
    /// This can return `false` even when the hitbox contains the mouse, if a hitbox in front of
    /// this sets `HitboxBehavior::BlockMouse` (`InteractiveElement::occlude`) or
    /// `HitboxBehavior::BlockMouseExceptScroll` (`InteractiveElement::block_mouse_except_scroll`),
    /// or if the current input modality is keyboard (see [`Window::last_input_was_keyboard`]).
    ///
    /// Handling of `ScrollWheelEvent` should typically use `should_handle_scroll` instead.
    /// Concretely, this is due to use-cases like overlays that cause the elements under to be
    /// non-interactive while still allowing scrolling. More abstractly, this is because
    /// `is_hovered` is about element interactions directly under the mouse - mouse moves, clicks,
    /// hover styling, etc. In contrast, scrolling is about finding the current outer scrollable
    /// container.
    pub fn is_hovered(&self, window: &Window) -> bool {
        self.id.is_hovered(window)
    }

    /// Checks if the hitbox contains the mouse and should handle scroll events. Typically this
    /// should only be used when handling `ScrollWheelEvent`, and otherwise `is_hovered` should be
    /// used. See the documentation of `Hitbox::is_hovered` for details about this distinction.
    ///
    /// This can return `false` even when the hitbox contains the mouse, if a hitbox in front of
    /// this sets `HitboxBehavior::BlockMouse` (`InteractiveElement::occlude`).
    pub fn should_handle_scroll(&self, window: &Window) -> bool {
        self.id.should_handle_scroll(window)
    }
}

/// How the hitbox affects mouse behavior.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum HitboxBehavior {
    /// Normal hitbox mouse behavior, doesn't affect mouse handling for other hitboxes.
    #[default]
    Normal,

    /// All hitboxes behind this hitbox will be ignored and so will have `hitbox.is_hovered() ==
    /// false` and `hitbox.should_handle_scroll() == false`. Typically for elements this causes
    /// skipping of all mouse events, hover styles, and tooltips. This flag is set by
    /// [`InteractiveElement::occlude`].
    ///
    /// For mouse handlers that check those hitboxes, this behaves the same as registering a
    /// bubble-phase handler for every mouse event type:
    ///
    /// ```ignore
    /// window.on_mouse_event(move |_: &EveryMouseEventTypeHere, phase, window, cx| {
    ///     if phase == DispatchPhase::Capture && hitbox.is_hovered(window) {
    ///         cx.stop_propagation();
    ///     }
    /// })
    /// ```
    ///
    /// This has effects beyond event handling - any use of hitbox checking, such as hover
    /// styles and tooltips. These other behaviors are the main point of this mechanism. An
    /// alternative might be to not affect mouse event handling - but this would allow
    /// inconsistent UI where clicks and moves interact with elements that are not considered to
    /// be hovered.
    BlockMouse,

    /// All hitboxes behind this hitbox will have `hitbox.is_hovered() == false`, even when
    /// `hitbox.should_handle_scroll() == true`. Typically for elements this causes all mouse
    /// interaction except scroll events to be ignored - see the documentation of
    /// [`Hitbox::is_hovered`] for details. This flag is set by
    /// [`InteractiveElement::block_mouse_except_scroll`].
    ///
    /// For mouse handlers that check those hitboxes, this behaves the same as registering a
    /// bubble-phase handler for every mouse event type **except** `ScrollWheelEvent`:
    ///
    /// ```ignore
    /// window.on_mouse_event(move |_: &EveryMouseEventTypeExceptScroll, phase, window, cx| {
    ///     if phase == DispatchPhase::Bubble && hitbox.should_handle_scroll(window) {
    ///         cx.stop_propagation();
    ///     }
    /// })
    /// ```
    ///
    /// See the documentation of [`Hitbox::is_hovered`] for details of why `ScrollWheelEvent` is
    /// handled differently than other mouse events. If also blocking these scroll events is
    /// desired, then a `cx.stop_propagation()` handler like the one above can be used.
    ///
    /// This has effects beyond event handling - this affects any use of `is_hovered`, such as
    /// hover styles and tooltips. These other behaviors are the main point of this mechanism.
    /// An alternative might be to not affect mouse event handling - but this would allow
    /// inconsistent UI where clicks and moves interact with elements that are not considered to
    /// be hovered.
    BlockMouseExceptScroll,
}

/// An identifier for a tooltip.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct TooltipId(usize);

impl TooltipId {
    /// Checks if the tooltip is currently hovered.
    pub fn is_hovered(&self, window: &Window) -> bool {
        window
            .interaction
            .tooltip_bounds
            .as_ref()
            .is_some_and(|tooltip_bounds| {
                tooltip_bounds.id == *self
                    && tooltip_bounds.bounds.contains(&window.mouse_position())
            })
    }
}

pub(crate) struct TooltipBounds {
    id: TooltipId,
    bounds: Bounds<Pixels>,
}

#[derive(Clone)]
pub(crate) struct TooltipRequest {
    id: TooltipId,
    tooltip: AnyTooltip,
}

/// Holds the state for a specific window.
pub struct Window {
    pub(crate) handle: AnyWindowHandle,
    pub(crate) invalidator: WindowInvalidator,
    pub(crate) removed: bool,
    pub(crate) platform_window: Box<dyn PlatformWindow>,
    display_id: Option<DisplayId>,
    is_resizable: bool,
    is_minimizable: bool,
    sprite_atlas: Arc<dyn PlatformAtlas>,
    text_system: Rc<WindowTextSystem>,
    text_rendering_mode: Rc<Cell<TextRenderingMode>>,
    rem_size: Pixels,
    /// The stack of override values for the window's rem size.
    ///
    /// This is used by `with_rem_size` to allow rendering an element tree with
    /// a given rem size.
    rem_size_override_stack: SmallVec<[Pixels; 8]>,
    pub(crate) viewport_size: Size<Pixels>,
    pub(crate) root: Option<AnyView>,
    frame_builder: FrameBuilder,
    pub(crate) interaction: InteractionOwner,
    pub(crate) text_input: TextInputOwner,
    pub(crate) frame_scheduler: FrameScheduler,
    scale_factor: f32,
    pub(crate) bounds_observers: SubscriberSet<(), AnyObserver>,
    appearance: WindowAppearance,
    pub(crate) appearance_observers: SubscriberSet<(), AnyObserver>,
    pub(crate) button_layout_observers: SubscriberSet<(), AnyObserver>,
    active: Rc<Cell<bool>>,
    hovered: Rc<Cell<bool>>,
    /// Tracks recent input event timestamps to determine if input is arriving at a high rate.
    /// Used to selectively enable VRR optimization only when input rate exceeds 60fps.
    pub(crate) input_rate_tracker: Rc<RefCell<InputRateTracker>>,
    #[cfg(feature = "input-latency-histogram")]
    input_latency_tracker: InputLatencyTracker,
    pub(crate) activation_observers: SubscriberSet<(), AnyObserver>,
    prompt: Option<RenderablePromptHandle>,
    pub(crate) client_inset: Option<Pixels>,
    #[cfg(any(feature = "inspector", debug_assertions))]
    inspector: Option<Entity<Inspector>>,
}

/// Tracks input event timestamps to determine if input is arriving at a high rate.
/// Used for selective VRR (Variable Refresh Rate) optimization.
#[derive(Clone, Debug)]
pub(crate) struct InputRateTracker {
    timestamps: Vec<Instant>,
    window: Duration,
    inputs_per_second: u32,
    sustain_until: Instant,
    sustain_duration: Duration,
}

const FRAME_INTERVAL_TOLERANCE: Duration = Duration::from_millis(1);

/// Keeps throttled frames on a stable cadence despite callback delivery jitter.
#[derive(Default)]
struct FramePacer {
    last_frame: Option<Instant>,
}

impl FramePacer {
    fn should_defer(&mut self, now: Instant, minimum_interval: Option<Duration>) -> bool {
        if let Some(interval) = minimum_interval
            && let Some(last_frame) = self.last_frame
        {
            let deadline = last_frame + interval;
            if now + FRAME_INTERVAL_TOLERANCE < deadline {
                return true;
            }
            // Correct small dispatch delays without shifting subsequent deadlines,
            // but bound the correction to a quarter interval. Larger delays restart
            // the cadence so recovery cannot produce closely spaced catch-up frames.
            self.last_frame = Some(if now.saturating_duration_since(deadline) <= interval / 4 {
                deadline
            } else {
                now
            });
        } else {
            self.last_frame = Some(now);
        }
        false
    }
}

fn minimum_frame_interval(
    should_throttle: bool,
    inactive_without_high_rate_input: bool,
    inactive_frame_interval: Option<Duration>,
    low_power_frame_interval: Option<Duration>,
    thermal_frame_interval: Option<Duration>,
    thermal_state: ThermalState,
    power_mode: crate::PowerMode,
) -> Option<Duration> {
    if !should_throttle {
        return None;
    }

    [
        inactive_frame_interval.filter(|_| inactive_without_high_rate_input),
        low_power_frame_interval.filter(|_| power_mode == crate::PowerMode::LowPower),
        thermal_frame_interval.filter(|_| {
            matches!(
                thermal_state,
                ThermalState::Critical | ThermalState::Serious
            )
        }),
    ]
    .into_iter()
    .flatten()
    .max()
}

impl Default for InputRateTracker {
    fn default() -> Self {
        Self {
            timestamps: Vec::new(),
            window: Duration::from_millis(100),
            inputs_per_second: 60,
            sustain_until: Instant::now(),
            sustain_duration: Duration::from_secs(1),
        }
    }
}

impl InputRateTracker {
    pub fn record_input(&mut self) {
        let now = Instant::now();
        self.timestamps.push(now);
        self.prune_old_timestamps(now);

        let min_events = self.inputs_per_second as u128 * self.window.as_millis() / 1000;
        if self.timestamps.len() as u128 >= min_events {
            self.sustain_until = now + self.sustain_duration;
        }
    }

    pub fn is_high_rate(&self) -> bool {
        Instant::now() < self.sustain_until
    }

    fn prune_old_timestamps(&mut self, now: Instant) {
        self.timestamps
            .retain(|&t| now.duration_since(t) <= self.window);
    }
}

/// A point-in-time snapshot of the input-latency histograms for a window,
/// suitable for external formatting.
#[cfg(feature = "input-latency-histogram")]
pub struct InputLatencySnapshot {
    /// Histogram of input-to-frame latency samples, in nanoseconds.
    pub latency_histogram: Histogram<u64>,
    /// Histogram of input events coalesced per rendered frame.
    pub events_per_frame_histogram: Histogram<u64>,
    /// Count of input events that arrived mid-draw and were excluded from
    /// latency recording.
    pub mid_draw_events_dropped: u64,
}

/// Records the time between when the first input event in a frame is dispatched
/// and when the resulting frame is presented, capturing worst-case latency when
/// multiple events are coalesced into a single frame.
#[cfg(feature = "input-latency-histogram")]
struct InputLatencyTracker {
    /// Timestamp of the first unrendered input event in the current frame;
    /// cleared when a frame is presented.
    first_input_at: Option<Instant>,
    /// Count of input events received since the last frame was presented.
    pending_input_count: u64,
    /// Histogram of input-to-frame latency samples, in nanoseconds.
    latency_histogram: Histogram<u64>,
    /// Histogram of input events coalesced per rendered frame.
    events_per_frame_histogram: Histogram<u64>,
    /// Count of input events that arrived mid-draw and were excluded from
    /// latency recording because their effects won't appear until the next frame.
    mid_draw_events_dropped: u64,
}

#[cfg(feature = "input-latency-histogram")]
impl InputLatencyTracker {
    fn new() -> Result<Self> {
        Ok(Self {
            first_input_at: None,
            pending_input_count: 0,
            latency_histogram: Histogram::new(3)
                .map_err(|e| anyhow!("Failed to create input latency histogram: {e}"))?,
            events_per_frame_histogram: Histogram::new(3)
                .map_err(|e| anyhow!("Failed to create events per frame histogram: {e}"))?,
            mid_draw_events_dropped: 0,
        })
    }

    /// Record that an input event was dispatched at the given time.
    /// Only the first event's timestamp per frame is retained (worst-case latency).
    fn record_input(&mut self, dispatch_time: Instant) {
        self.first_input_at.get_or_insert(dispatch_time);
        self.pending_input_count += 1;
    }

    /// Record that an input event arrived during a draw phase and was excluded
    /// from latency tracking.
    fn record_mid_draw_input(&mut self) {
        self.mid_draw_events_dropped += 1;
    }

    /// Record that a frame was presented, flushing pending latency and coalescing samples.
    fn record_frame_presented(&mut self) {
        if let Some(first_input_at) = self.first_input_at.take() {
            let latency_nanos = first_input_at.elapsed().as_nanos() as u64;
            self.latency_histogram.record(latency_nanos).ok();
        }
        if self.pending_input_count > 0 {
            self.events_per_frame_histogram
                .record(self.pending_input_count)
                .ok();
            self.pending_input_count = 0;
        }
    }

    fn snapshot(&self) -> InputLatencySnapshot {
        InputLatencySnapshot {
            latency_histogram: self.latency_histogram.clone(),
            events_per_frame_histogram: self.events_per_frame_histogram.clone(),
            mid_draw_events_dropped: self.mid_draw_events_dropped,
        }
    }
}

pub(crate) struct ElementStateBox {
    pub(crate) inner: Box<dyn Any>,
    #[cfg(debug_assertions)]
    pub(crate) type_name: &'static str,
}

fn active_window_bounds(cx: &mut App) -> Option<WindowBounds> {
    let window_update_stack = cx.window_update_stack.clone();
    let window_stack = cx.window_stack().unwrap_or_default();

    let active_window = window_stack
        .into_iter()
        .filter(|window| !window_update_stack.contains(&window.window_id()))
        .find_map(|window| {
            window
                .update(cx, |_, window, _| window.window_bounds())
                .ok()
        });

    active_window.or_else(|| {
        cx.active_window()
            .filter(|window| !window_update_stack.contains(&window.window_id()))
            .and_then(|window| {
                window
                    .update(cx, |_, window, _| window.window_bounds())
                    .ok()
            })
    })
}

fn default_bounds(display_id: Option<DisplayId>, cx: &mut App) -> WindowBounds {
    let active_window_bounds = active_window_bounds(cx);

    const CASCADE_OFFSET: f32 = 25.0;

    let display = display_id.map_or_else(|| cx.primary_display(), |id| cx.find_display(id));

    let default_placement = || Bounds::new(point(px(0.), px(0.)), DEFAULT_WINDOW_SIZE);

    // Use visible_bounds to exclude taskbar/dock areas
    let display_bounds = display
        .as_ref()
        .map_or_else(default_placement, |d| d.visible_bounds());

    let (
        Bounds {
            origin: base_origin,
            size: base_size,
        },
        window_bounds_ctor,
    ): (_, fn(Bounds<Pixels>) -> WindowBounds) = match active_window_bounds {
        // New windows should inherit the restore bounds of the active window,
        // but they should always open as regular windowed windows.
        Some(bounds) => (bounds.get_bounds(), WindowBounds::Windowed),
        None => (
            display
                .as_ref()
                .map_or_else(default_placement, |d| d.default_bounds()),
            WindowBounds::Windowed,
        ),
    };

    let cascade_offset = point(px(CASCADE_OFFSET), px(CASCADE_OFFSET));
    let proposed_origin = base_origin + cascade_offset;
    let proposed_bounds = Bounds::new(proposed_origin, base_size);

    let display_right = display_bounds.origin.x + display_bounds.size.width;
    let display_bottom = display_bounds.origin.y + display_bounds.size.height;
    let window_right = proposed_bounds.origin.x + proposed_bounds.size.width;
    let window_bottom = proposed_bounds.origin.y + proposed_bounds.size.height;

    let fits_horizontally = window_right <= display_right;
    let fits_vertically = window_bottom <= display_bottom;

    let final_origin = match (fits_horizontally, fits_vertically) {
        (true, true) => proposed_origin,
        (false, true) => point(display_bounds.origin.x, base_origin.y),
        (true, false) => point(base_origin.x, display_bounds.origin.y),
        (false, false) => display_bounds.origin,
    };
    window_bounds_ctor(Bounds::new(final_origin, base_size))
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "accessibility")]
    use crate::AccessibilityUpdate;
    #[cfg(feature = "frame-diagnostics")]
    use crate::RenderImage;
    #[cfg(feature = "accessibility")]
    use crate::StatefulInteractiveElement as _;
    #[cfg(feature = "accessibility")]
    use crate::util::FluentBuilder as _;
    #[cfg(feature = "frame-diagnostics")]
    use crate::{
        AnyView, App, Entity, FrameDirtyReason, FrameEvent, FrameInputProvenance,
        FrameInvalidationPhase, FramePhase, FrameTimingCollector, InputHandler, KeyDownEvent,
        Keystroke, PlatformInput, Point, StyleRefinement, TextInputClient, UTF16Selection,
    };
    use crate::{
        AnyWindowHandle, AppContext as _, Bounds, Context, FocusHandle, InteractiveElement as _,
        IntoElement, ParentElement as _, Pixels, Render, RequestFrameOptions, Styled as _,
        TestAppContext, Window, WindowAppearance, WindowBounds, WindowOptions, canvas, div, px,
        size,
    };
    #[cfg(feature = "frame-diagnostics")]
    use scheduler::Instant;
    #[cfg(feature = "frame-diagnostics")]
    use std::ops::Range;
    #[cfg(feature = "frame-diagnostics")]
    use std::sync::Arc;
    use std::{cell::Cell, rc::Rc};

    struct RootView {
        explicit_size: bool,
        child_bounds: Rc<Cell<Bounds<Pixels>>>,
    }

    struct EmptyView;

    struct CountingView {
        render_count: Rc<Cell<usize>>,
    }

    impl Render for CountingView {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            self.render_count.set(self.render_count.get() + 1);
            div()
        }
    }

    #[gpui::test]
    fn test_maximized_window_opens_when_backend_cannot_zoom(cx: &mut TestAppContext) {
        // The test platform inherits WindowHost's "zoom unsupported" default.
        // A maximized window must still open: its bounds are already applied
        // through WindowParams, so requesting the maximized state is only a
        // best-effort decoration, not a prerequisite for creating the window.
        let result = cx.update(|cx| {
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Maximized(Bounds::maximized(None, cx))),
                    ..Default::default()
                },
                |_, cx| cx.new(|_| EmptyView),
            )
        });
        if let Err(error) = result {
            panic!("a maximized window must open even when the backend cannot zoom: {error:#}");
        }
    }

    #[gpui::test]
    async fn test_focus_handle_release(cx: &mut TestAppContext) {
        let (handle, survivor) = cx.update(|cx| (cx.focus_handle(), cx.focus_handle()));
        let handle_id = handle.id;
        let weak_handle = handle.downgrade();
        let retained = handle.clone();
        drop(handle);

        cx.update(|_| {});
        cx.read(|cx| {
            let handles = cx.focus_handles.read();
            assert_eq!(handles.handles.len(), 2);
            assert!(handles.dropped_handles.is_empty());
        });
        assert_eq!(weak_handle.upgrade().as_ref(), Some(&retained));

        cx.background_spawn(async move { drop(retained) }).await;
        assert_eq!(weak_handle.upgrade(), None);
        cx.read(|cx| {
            assert_eq!(cx.focus_handles.read().dropped_handles, [handle_id]);
        });
        cx.update(|_| {});
        cx.read(|cx| {
            let handles = cx.focus_handles.read();
            assert_eq!(handles.handles.keys().collect::<Vec<_>>(), [survivor.id]);
            assert!(handles.dropped_handles.is_empty());
        });

        let replacements = cx.update(|cx| [cx.focus_handle(), cx.focus_handle()]);
        assert_eq!(weak_handle.upgrade(), None);
        cx.update(|cx| {
            drop(replacements);
            cx.defer(move |_| drop(survivor));
        });
        cx.read(|cx| {
            let handles = cx.focus_handles.read();
            assert_eq!(handles.handles.len(), 0);
            assert!(handles.dropped_handles.is_empty());
        });
    }

    #[gpui::test]
    fn test_dropped_focus_handle_blurs_only_its_windows(cx: &mut TestAppContext) {
        let windows = [
            cx.add_window(|_, _| EmptyView),
            cx.add_window(|_, _| EmptyView),
            cx.add_window(|_, _| EmptyView),
            cx.add_window(|_, _| EmptyView),
            cx.add_window(|_, _| EmptyView),
        ];
        let (handle, other_handle, survivor) =
            cx.update(|cx| (cx.focus_handle(), cx.focus_handle(), cx.focus_handle()));
        for window in &windows[..2] {
            window
                .update(cx, |_, window, cx| window.focus(&handle, cx))
                .expect("window exists");
        }
        windows[2]
            .update(cx, |_, window, cx| window.focus(&other_handle, cx))
            .expect("window exists");
        windows[3]
            .update(cx, |_, window, cx| window.focus(&survivor, cx))
            .expect("window exists");

        cx.update(|cx| {
            for _ in 0..1_000 {
                drop(cx.focus_handle());
            }
            drop((handle, other_handle));
        });
        for window in &windows[..3] {
            window
                .update(cx, |_, window, _| {
                    assert_eq!(window.interaction.focus, None)
                })
                .expect("window exists");
        }
        windows[3]
            .update(cx, |_, window, cx| {
                assert_eq!(window.focused(cx).as_ref(), Some(&survivor));
            })
            .expect("window exists");
        windows[4]
            .update(cx, |_, window, _| {
                assert_eq!(window.interaction.focus, None);
            })
            .expect("window exists");
    }

    fn reconstructed_coordinate(
        coordinate: super::QuantizedGlyphCoordinate,
        subpixel_variants: u8,
    ) -> f32 {
        coordinate.integer as f32 + f32::from(coordinate.variant) / f32::from(subpixel_variants)
    }

    #[test]
    fn glyph_coordinate_quantization_uses_euclidean_subpixel_ticks() {
        for tick in -8..=8 {
            let device_coordinate = tick as f32 / f32::from(crate::SUBPIXEL_VARIANTS_X);
            let quantized =
                super::quantize_glyph_coordinate(device_coordinate, crate::SUBPIXEL_VARIANTS_X);
            assert_eq!(
                reconstructed_coordinate(quantized, crate::SUBPIXEL_VARIANTS_X),
                device_coordinate
            );
            assert!(quantized.variant < crate::SUBPIXEL_VARIANTS_X);
        }

        assert_eq!(
            super::quantize_glyph_coordinate(-0.25, crate::SUBPIXEL_VARIANTS_X),
            super::QuantizedGlyphCoordinate {
                integer: -1,
                variant: 3,
            }
        );
        assert_eq!(
            super::quantize_glyph_coordinate(0.75, crate::SUBPIXEL_VARIANTS_X),
            super::QuantizedGlyphCoordinate {
                integer: 0,
                variant: 3,
            }
        );
    }

    #[test]
    fn glyph_coordinate_quantization_preserves_ties_scale_and_integer_alignment() {
        for (device_coordinate, expected) in [
            (
                -0.1251,
                super::QuantizedGlyphCoordinate {
                    integer: -1,
                    variant: 3,
                },
            ),
            (
                -0.125,
                super::QuantizedGlyphCoordinate {
                    integer: 0,
                    variant: 0,
                },
            ),
            (
                -0.1249,
                super::QuantizedGlyphCoordinate {
                    integer: 0,
                    variant: 0,
                },
            ),
            (
                0.1249,
                super::QuantizedGlyphCoordinate {
                    integer: 0,
                    variant: 0,
                },
            ),
            (
                0.125,
                super::QuantizedGlyphCoordinate {
                    integer: 0,
                    variant: 0,
                },
            ),
            (
                0.1251,
                super::QuantizedGlyphCoordinate {
                    integer: 0,
                    variant: 1,
                },
            ),
        ] {
            assert_eq!(
                super::quantize_glyph_coordinate(device_coordinate, crate::SUBPIXEL_VARIANTS_X,),
                expected
            );
        }

        for integer in -16..=16 {
            assert_eq!(
                super::quantize_glyph_coordinate(integer as f32, crate::SUBPIXEL_VARIANTS_X),
                super::QuantizedGlyphCoordinate {
                    integer,
                    variant: 0,
                }
            );
            assert_eq!(
                super::quantize_glyph_coordinate(integer as f32, crate::SUBPIXEL_VARIANTS_Y),
                super::QuantizedGlyphCoordinate {
                    integer,
                    variant: 0,
                }
            );
        }

        for scale_factor in [1.0, 1.25, 2.0] {
            for logical_coordinate in [-2.0, -0.2, -0.125, 0.0, 0.125, 0.2, 2.0] {
                let device_coordinate = logical_coordinate * scale_factor;
                let quantized =
                    super::quantize_glyph_coordinate(device_coordinate, crate::SUBPIXEL_VARIANTS_X);
                let expected = crate::util::round_half_toward_zero(
                    device_coordinate * f32::from(crate::SUBPIXEL_VARIANTS_X),
                ) / f32::from(crate::SUBPIXEL_VARIANTS_X);
                assert_eq!(
                    reconstructed_coordinate(quantized, crate::SUBPIXEL_VARIANTS_X),
                    expected
                );
            }
        }
    }

    impl Render for EmptyView {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
        }
    }

    #[cfg(feature = "accessibility")]
    struct AccessibilityView {
        action_count: Rc<Cell<usize>>,
    }

    #[cfg(feature = "accessibility")]
    impl Render for AccessibilityView {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let action_count = self.action_count.clone();
            div()
                .id("accessibility-group")
                .role(accesskit::Role::Group)
                .child(
                    div()
                        .id("accessibility-button")
                        .role(accesskit::Role::Button)
                        .aria_label("Activate")
                        .aria_description("Runs the configured action")
                        .aria_disabled(true)
                        .aria_selected(true)
                        .aria_expanded(false)
                        .aria_toggled(accesskit::Toggled::True)
                        .aria_value("ready")
                        .on_a11y_action(accesskit::Action::Click, move |_, _, _| {
                            action_count.set(action_count.get() + 1);
                        }),
                )
        }
    }

    #[cfg(feature = "accessibility")]
    struct DuplicateAccessibilityIdsView;

    #[cfg(feature = "accessibility")]
    impl Render for DuplicateAccessibilityIdsView {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .child(
                    div()
                        .id("duplicate-accessibility-id")
                        .role(accesskit::Role::Group)
                        .aria_label("First group")
                        .child(
                            div()
                                .id("first-child")
                                .role(accesskit::Role::Button)
                                .aria_label("First child"),
                        ),
                )
                .child(
                    div()
                        .id("duplicate-accessibility-id")
                        .role(accesskit::Role::Group)
                        .aria_label("Duplicate group")
                        .child(
                            div()
                                .id("suppressed-child")
                                .role(accesskit::Role::Button)
                                .aria_label("Suppressed child"),
                        ),
                )
                .child(
                    div()
                        .id("sibling")
                        .role(accesskit::Role::Button)
                        .aria_label("Sibling"),
                )
        }
    }

    #[cfg(feature = "accessibility")]
    struct RemovableAccessibilityView {
        action_count: Rc<Cell<usize>>,
        show_button: bool,
    }

    #[cfg(feature = "accessibility")]
    impl Render for RemovableAccessibilityView {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().when(self.show_button, |this| {
                let action_count = self.action_count.clone();
                this.child(
                    div()
                        .id("removable-accessibility-button")
                        .role(accesskit::Role::Button)
                        .aria_label("Removable action")
                        .on_a11y_action(accesskit::Action::Click, move |_, _, _| {
                            action_count.set(action_count.get() + 1);
                        }),
                )
            })
        }
    }

    #[cfg(feature = "frame-diagnostics")]
    struct DiagnosticsPanel;

    #[cfg(feature = "frame-diagnostics")]
    struct AtlasCachedPanel {
        image: Arc<RenderImage>,
        render_count: Rc<Cell<usize>>,
    }

    #[cfg(feature = "frame-diagnostics")]
    struct AtlasCachedRoot {
        panel: Entity<AtlasCachedPanel>,
    }

    #[cfg(feature = "frame-diagnostics")]
    struct DropImageDuringPaintView {
        image: Arc<RenderImage>,
        render_count: Rc<Cell<usize>>,
        drop_on_paint: Rc<Cell<bool>>,
    }

    #[cfg(feature = "frame-diagnostics")]
    impl Render for DiagnosticsPanel {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full().child("cached panel")
        }
    }

    #[cfg(feature = "frame-diagnostics")]
    impl Render for AtlasCachedPanel {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            self.render_count.set(self.render_count.get() + 1);
            let image = self.image.clone();
            canvas(
                |bounds, _, _| bounds,
                move |bounds, image_bounds, window, _| {
                    window
                        .paint_image(bounds, image_bounds, Default::default(), image, 0, false)
                        .expect("test image should paint");
                },
            )
            .size_full()
        }
    }

    #[cfg(feature = "frame-diagnostics")]
    impl Render for AtlasCachedRoot {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .child(AnyView::from(self.panel.clone()).cached(StyleRefinement::default()))
        }
    }

    #[cfg(feature = "frame-diagnostics")]
    impl Render for DropImageDuringPaintView {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            self.render_count.set(self.render_count.get() + 1);
            let image = self.image.clone();
            let drop_on_paint = self.drop_on_paint.clone();
            canvas(
                |bounds, _, _| bounds,
                move |bounds, image_bounds, window, _| {
                    window
                        .paint_image(
                            bounds,
                            image_bounds,
                            Default::default(),
                            image.clone(),
                            0,
                            false,
                        )
                        .expect("test image should paint");
                    if drop_on_paint.replace(false) {
                        window
                            .drop_image(image)
                            .expect("image removal should queue during paint");
                    }
                },
            )
            .size_full()
        }
    }

    #[cfg(feature = "frame-diagnostics")]
    struct DiagnosticsInputView {
        focus: FocusHandle,
        panel: Entity<DiagnosticsPanel>,
    }

    #[cfg(feature = "frame-diagnostics")]
    impl Render for DiagnosticsInputView {
        fn render(&mut self, _: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .track_focus(&self.focus)
                .on_key_down(|_, window, _| window.refresh())
                .child(AnyView::from(self.panel.clone()).cached(StyleRefinement::default()))
                .into_any_element()
        }
    }

    #[cfg(feature = "frame-diagnostics")]
    struct RefreshingTextInput;

    #[cfg(feature = "frame-diagnostics")]
    impl InputHandler for RefreshingTextInput {
        fn selected_text_range(
            &mut self,
            _: bool,
            _: &mut Window,
            _: &mut App,
        ) -> Option<UTF16Selection> {
            None
        }

        fn marked_text_range(&mut self, _: &mut Window, _: &mut App) -> Option<Range<usize>> {
            None
        }

        fn text_for_range(
            &mut self,
            _: Range<usize>,
            _: &mut Option<Range<usize>>,
            _: &mut Window,
            _: &mut App,
        ) -> Option<String> {
            None
        }

        fn replace_text_in_range(
            &mut self,
            _: Option<Range<usize>>,
            _: &str,
            window: &mut Window,
            _: &mut App,
        ) {
            window.refresh();
        }

        fn replace_and_mark_text_in_range(
            &mut self,
            _: Option<Range<usize>>,
            _: &str,
            _: Option<Range<usize>>,
            window: &mut Window,
            _: &mut App,
        ) {
            window.refresh();
        }

        fn unmark_text(&mut self, _: &mut Window, _: &mut App) {}

        fn bounds_for_range(
            &mut self,
            _: Range<usize>,
            _: &mut Window,
            _: &mut App,
        ) -> Option<Bounds<Pixels>> {
            None
        }

        fn character_index_for_point(
            &mut self,
            _: Point<Pixels>,
            _: &mut Window,
            _: &mut App,
        ) -> Option<usize> {
            None
        }
    }

    #[test]
    fn frame_throttle_tracks_display_refreshes() {
        let start = super::Instant::now();
        for (elapsed, interval) in [
            (
                std::time::Duration::from_micros(16_666),
                std::time::Duration::from_micros(16_667),
            ),
            (
                std::time::Duration::from_micros(33_332),
                std::time::Duration::from_micros(33_333),
            ),
        ] {
            let mut pacer = super::FramePacer::default();
            assert!(!pacer.should_defer(start, Some(interval)));
            assert!(pacer.should_defer(start + elapsed / 2, Some(interval)));
            assert!(!pacer.should_defer(start + elapsed, Some(interval)));
        }
    }

    #[test]
    fn frame_throttle_preserves_cadence_without_catching_up_after_stalls() {
        let start = super::Instant::now();
        let interval = std::time::Duration::from_micros(33_333);
        let mut pacer = super::FramePacer::default();
        assert!(!pacer.should_defer(start, Some(interval)));
        let delayed_frame = start + interval + std::time::Duration::from_millis(5);
        assert!(!pacer.should_defer(delayed_frame, Some(interval)));
        assert!(pacer.should_defer(start + interval + interval / 2, Some(interval)));
        assert!(!pacer.should_defer(start + interval * 2, Some(interval)));

        // Even a stall just short of another full interval must not cause a burst.
        let after_stall =
            start + interval * 3 + interval.saturating_sub(std::time::Duration::from_millis(1));
        assert!(!pacer.should_defer(after_stall, Some(interval)));
        assert!(pacer.should_defer(after_stall + interval / 2, Some(interval)));
        assert!(!pacer.should_defer(after_stall + interval, Some(interval)));
        // Mandatory presentation bypasses the limit and establishes a fresh cadence.
        let mandatory_frame = after_stall + interval + std::time::Duration::from_millis(2);
        assert!(!pacer.should_defer(mandatory_frame, None));
        assert!(pacer.should_defer(mandatory_frame + interval / 2, Some(interval)));
    }

    #[test]
    fn power_modes_preserve_frame_limits() {
        let options = crate::WindowOptions::default();
        for (power_mode, active_interval, thermal_interval) in [
            (
                crate::PowerMode::LowPower,
                Some(std::time::Duration::from_micros(33_333)),
                std::time::Duration::from_micros(33_333),
            ),
            (
                crate::PowerMode::Unknown,
                None,
                std::time::Duration::from_micros(16_667),
            ),
            (
                crate::PowerMode::Balanced,
                None,
                std::time::Duration::from_micros(16_667),
            ),
            (
                crate::PowerMode::HighPower,
                None,
                std::time::Duration::from_micros(16_667),
            ),
        ] {
            assert_eq!(
                super::minimum_frame_interval(
                    true,
                    false,
                    options.inactive_frame_interval,
                    options.low_power_frame_interval,
                    options.thermal_frame_interval,
                    crate::ThermalState::Nominal,
                    power_mode,
                ),
                active_interval,
            );
            assert_eq!(
                super::minimum_frame_interval(
                    true,
                    false,
                    options.inactive_frame_interval,
                    options.low_power_frame_interval,
                    options.thermal_frame_interval,
                    crate::ThermalState::Serious,
                    power_mode,
                ),
                Some(thermal_interval),
            );
        }
    }

    #[test]
    fn frame_limits_combine_enabled_intervals() {
        let short = Some(std::time::Duration::from_millis(10));
        let long = Some(std::time::Duration::from_millis(100));
        for (inactive, low_power, thermal, expected) in [
            (long, short, short, long),
            (short, long, short, long),
            (short, short, long, long),
            (long, None, None, long),
            (None, long, None, long),
            (None, None, long, long),
            (None, None, None, None),
        ] {
            assert_eq!(
                super::minimum_frame_interval(
                    true,
                    true,
                    inactive,
                    low_power,
                    thermal,
                    crate::ThermalState::Critical,
                    crate::PowerMode::LowPower,
                ),
                expected,
            );
        }
        assert_eq!(
            super::minimum_frame_interval(
                false,
                true,
                long,
                long,
                long,
                crate::ThermalState::Critical,
                crate::PowerMode::LowPower,
            ),
            None,
        );
    }

    #[test]
    fn test_scale_factor_change_preserves_bounds_and_survives_resize() {
        let mut cx = TestAppContext::single();
        let window = cx.add_window(|_, _| EmptyView);
        let handle: AnyWindowHandle = window.into();
        let window_state = |cx: &mut TestAppContext| {
            cx.update_window(handle, |_, window, _| {
                (
                    window.scale_factor(),
                    window.bounds(),
                    window.viewport_size(),
                )
            })
            .unwrap()
        };

        let (scale_factor, mut expected_bounds, _) = window_state(&mut cx);
        assert_eq!(scale_factor, 2.0);

        for (scale_factor, resized_size) in [
            (1.0, size(px(800.), px(600.))),
            (1.25, size(px(640.), px(480.))),
            (2.0, size(px(1024.), px(768.))),
        ] {
            cx.simulate_window_scale_factor_change(handle, scale_factor);
            assert_eq!(
                window_state(&mut cx),
                (scale_factor, expected_bounds, expected_bounds.size)
            );

            cx.simulate_window_resize(handle, resized_size);
            expected_bounds.size = resized_size;
            assert_eq!(
                window_state(&mut cx),
                (scale_factor, expected_bounds, resized_size)
            );
        }
    }

    #[gpui::test]
    fn test_native_close_defers_app_access(cx: &mut TestAppContext) {
        let window = cx.add_window(|_, _| EmptyView);
        let platform_window = cx.test_window(window.into());
        let app = cx.app.borrow_mut();
        platform_window.simulate_close();
        assert!(app.windows.contains_key(window.window_id()));
        drop(app);
        cx.run_until_parked();
        assert!(!cx.app.borrow().windows.contains_key(window.window_id()));
    }

    #[gpui::test]
    fn test_frame_waker_fires_on_frame_demand(cx: &mut TestAppContext) {
        let window = cx.add_window(|_, _| EmptyView);
        let test_window = cx.test_window(window.into());

        // This test exercises idle wakeups, not the inactive-window frame limit.
        test_window.simulate_active_status_change(true);

        assert!(
            test_window.frame_wake_count() >= 1,
            "opening a window must wake the frame source for the initial frame"
        );

        test_window.simulate_frame_request(RequestFrameOptions::default());

        let baseline = test_window.frame_wake_count();
        test_window.simulate_frame_request(RequestFrameOptions::default());
        window.update(cx, |_, _, _| {}).unwrap();
        assert_eq!(
            test_window.frame_wake_count(),
            baseline,
            "clean frames and non-notifying updates must not wake the frame source"
        );

        window.update(cx, |_, _, cx| cx.notify()).unwrap();
        assert!(
            test_window.frame_wake_count() > baseline,
            "notifying a view in an idle window must wake the frame source"
        );

        test_window.simulate_frame_request(RequestFrameOptions::default());
        let baseline = test_window.frame_wake_count();
        test_window.simulate_frame_request(RequestFrameOptions::default());
        assert_eq!(
            test_window.frame_wake_count(),
            baseline,
            "serving demand must return the window to idle"
        );

        window
            .update(cx, |_, window, _| window.on_next_frame(|_, _| {}))
            .unwrap();
        assert!(
            test_window.frame_wake_count() > baseline,
            "scheduling a next-frame callback in an idle window must wake the frame source"
        );
    }

    #[gpui::test]
    fn test_platform_capability_matrix(cx: &mut TestAppContext) {
        let window = cx.add_window(|_, _| EmptyView);
        window
            .update(cx, |_, window, _| {
                let capabilities = window.platform_capabilities();
                assert!(capabilities.text_input);
                assert!(!capabilities.ime_candidate_position);
                assert!(capabilities.frame_callbacks);
                assert!(capabilities.native_prompt);
                assert_eq!(capabilities.accessibility, cfg!(feature = "accessibility"));
                assert_eq!(
                    capabilities.clipboard,
                    crate::ClipboardCapabilities::READ_WRITE
                );
                assert!(capabilities.window_controls.fullscreen);
                assert!(capabilities.window_controls.move_window);
                assert!(!capabilities.window_controls.resize_window);
                assert!(!capabilities.window_controls.maximize);
                assert!(!capabilities.window_controls.minimize);
                assert!(!capabilities.window_controls.window_menu);
                assert_eq!(
                    capabilities.window_services,
                    crate::WindowServiceCapabilities {
                        document_metadata: true,
                        ..Default::default()
                    }
                );
            })
            .expect("test window should remain open");
    }

    #[gpui::test]
    fn test_throttled_forced_render_is_not_lost(cx: &mut TestAppContext) {
        let render_count = Rc::new(Cell::new(0));
        let window = cx.add_window({
            let render_count = render_count.clone();
            move |_, _| CountingView { render_count }
        });
        let test_window = cx.test_window(window.into());

        // Serve a frame so the inactive-window throttle has a reference time.
        test_window.simulate_frame_request(RequestFrameOptions::default());
        assert_eq!(render_count.get(), 1);

        // Test windows report themselves inactive, so a forced render arriving
        // inside the configured inactive interval is deferred instead of drawn.
        test_window.simulate_frame_request(RequestFrameOptions {
            require_presentation: false,
            force_render: true,
        });
        assert_eq!(
            render_count.get(),
            1,
            "a throttled forced render must not draw before the interval elapses"
        );

        // Wait out the interval, then send an ordinary request. It carries no
        // force flag of its own, so only the deferred forced render can make it draw.
        std::thread::sleep(std::time::Duration::from_millis(50));
        test_window.simulate_frame_request(RequestFrameOptions::default());
        assert_eq!(
            render_count.get(),
            2,
            "the deferred forced render must be served on the next frame"
        );
    }

    #[gpui::test]
    fn test_pending_next_frame_callbacks_are_not_stranded(cx: &mut TestAppContext) {
        let window = cx.add_window(|_, _| EmptyView);
        let test_window = cx.test_window(window.into());
        test_window.simulate_frame_request(RequestFrameOptions::default());

        let callback_ran = Rc::new(Cell::new(false));
        window
            .update(cx, {
                let callback_ran = callback_ran.clone();
                move |_, window, _| {
                    window.on_next_frame(move |_, _| callback_ran.set(true));
                }
            })
            .unwrap();

        let baseline = test_window.frame_wake_count();
        test_window.simulate_frame_request(RequestFrameOptions::default());
        assert!(
            test_window.frame_wake_count() > baseline || callback_ran.get(),
            "a frame request with pending next-frame callbacks must either run them or re-arm the frame source"
        );
    }

    #[cfg(feature = "frame-diagnostics")]
    #[gpui::test]
    fn printable_text_input_is_linked_to_presented_frame(cx: &mut TestAppContext) {
        let window = cx.add_window(|_, _| EmptyView);
        let handle: AnyWindowHandle = window.into();
        cx.update_window(handle, |_, window, cx| {
            window.draw_and_present_for_test(cx);
            let input = TextInputClient::new(window.to_async(cx), Box::new(RefreshingTextInput));
            window.platform_window.set_input_handler(input);
        })
        .expect("diagnostics window should remain open");

        let mut collector = cx
            .update_window(handle, |_, window, _| {
                FrameTimingCollector::for_window(window)
            })
            .expect("diagnostics window should remain open");
        cx.dispatch_keystroke(handle, Keystroke::parse("a").expect("valid keystroke"));
        cx.update_window(handle, |_, window, _| window.present_for_test())
            .expect("diagnostics window should remain open");

        assert!(collector.snapshot().events.iter().any(|event| matches!(
            event,
            FrameEvent::Presented(timing)
                if timing.input == Some(FrameInputProvenance::Keyboard)
                    && timing.input_to_present.is_some()
        )));
    }

    #[gpui::test]
    fn built_frame_projects_only_completed_read_only_state(cx: &mut TestAppContext) {
        let window = cx.add_window(|_, _| EmptyView);
        let handle: AnyWindowHandle = window.into();

        cx.update_window(handle, |_, window, cx| {
            window.refresh();
            window.draw(cx).clear();

            let expected_scene = &window.interaction.rendered_frame.scene;
            let expected_atlas_epoch = window.interaction.rendered_frame.atlas_epoch;
            let expected_hitbox_count = window.interaction.rendered_frame.hitboxes.len();
            let expected_dispatch_node_count =
                window.interaction.rendered_frame.dispatch_tree.len();
            let expected_mouse_hitbox_count = window.interaction.mouse_hit_test.ids.len();
            let expected_captured_hitbox = window.interaction.captured_hitbox;
            let expected_focus = window.interaction.focus;
            let expected_window_active = window.interaction.rendered_frame.window_active;
            let expected_handler_count = window.text_input.rendered_handlers.len();
            let expected_active_handler = window
                .text_input
                .rendered_handlers
                .iter()
                .rposition(Option::is_some);

            let built_frame = window.interaction.built_frame(&window.text_input);
            assert!(std::ptr::eq(built_frame.scene, expected_scene));
            assert_eq!(built_frame.atlas_epoch, expected_atlas_epoch);
            assert_eq!(
                built_frame.interaction.hitboxes.len(),
                expected_hitbox_count
            );
            assert_eq!(
                built_frame.interaction.dispatch_tree.len(),
                expected_dispatch_node_count
            );
            assert_eq!(
                built_frame.interaction.mouse_hit_test.ids.len(),
                expected_mouse_hitbox_count
            );
            assert_eq!(
                built_frame.interaction.captured_hitbox,
                expected_captured_hitbox
            );
            assert_eq!(built_frame.interaction.focus, expected_focus);
            assert_eq!(
                built_frame.interaction.window_active,
                expected_window_active
            );
            assert_eq!(
                built_frame.text_input.handler_slot_count,
                expected_handler_count
            );
            assert_eq!(
                built_frame.text_input.active_handler_index,
                expected_active_handler
            );
        })
        .expect("built-frame window should remain open");
    }

    #[cfg(feature = "accessibility")]
    #[gpui::test]
    fn built_frame_carries_accessibility_update(cx: &mut TestAppContext) {
        let action_count = Rc::new(Cell::new(0));
        let window = cx.add_window({
            let action_count = action_count.clone();
            move |_, _| AccessibilityView { action_count }
        });
        let handle: AnyWindowHandle = window.into();
        cx.update_window(handle, |_, window, cx| {
            window.refresh();
            window.draw(cx).clear();
            assert_eq!(
                window
                    .interaction
                    .rendered_frame
                    .accessibility_builder
                    .node_count(),
                3
            );
            let built_frame = window.interaction.built_frame(&window.text_input);
            let snapshot = built_frame
                .accessibility
                .semantic_snapshot()
                .expect("built frame should contain a semantic snapshot");
            let (group_id, group) = snapshot
                .update
                .nodes
                .iter()
                .find(|(_, node)| node.role() == accesskit::Role::Group)
                .expect("semantic group should be present");
            let (button_id, button) = snapshot
                .update
                .nodes
                .iter()
                .find(|(_, node)| node.role() == accesskit::Role::Button)
                .expect("semantic button should be present");
            assert_eq!(group.children(), &[*button_id]);
            assert_eq!(button.label(), Some("Activate"));
            assert_eq!(button.description(), Some("Runs the configured action"));
            assert!(button.is_disabled());
            assert_eq!(button.is_selected(), Some(true));
            assert_eq!(button.is_expanded(), Some(false));
            assert_eq!(button.toggled(), Some(accesskit::Toggled::True));
            assert_eq!(button.value(), Some("ready"));
            assert!(button.supports_action(accesskit::Action::Click));
            assert!(snapshot.update.nodes[0].1.children().contains(group_id));
            let button_id = *button_id;

            assert!(window.dispatch_accessibility_action(
                button_id,
                accesskit::Action::Click,
                None,
                cx,
            ));
            assert_eq!(action_count.get(), 1);
            assert!(!window.dispatch_accessibility_action(
                button_id,
                accesskit::Action::Focus,
                None,
                cx,
            ));
            assert_eq!(action_count.get(), 1);

            window.draw(cx).clear();
            let cached_frame = window.interaction.built_frame(&window.text_input);
            let cached_snapshot = cached_frame
                .accessibility
                .semantic_snapshot()
                .expect("accessibility frames should rebuild complete semantic trees");
            assert_eq!(cached_snapshot.update.nodes.len(), 3);
        })
        .expect("accessibility window should remain open");
    }

    #[cfg(feature = "accessibility")]
    #[gpui::test]
    fn duplicate_semantic_element_ids_do_not_attach_descendants_to_prior_siblings(
        cx: &mut TestAppContext,
    ) {
        let window = cx.add_window(|_, _| DuplicateAccessibilityIdsView);
        let handle: AnyWindowHandle = window.into();
        let snapshot = cx
            .update_window(handle, |_, window, cx| {
                window.refresh();
                window.draw(cx).clear();
                window
                    .accessibility_snapshot_for_test()
                    .expect("completed frame should contain a semantic snapshot")
            })
            .expect("semantic test window should remain open");

        let mut labels = snapshot
            .update
            .nodes
            .iter()
            .filter_map(|(_, node)| node.label())
            .collect::<Vec<_>>();
        labels.sort_unstable();
        assert_eq!(labels, ["First child", "First group", "Sibling"]);

        let (first_group_id, first_group) = snapshot
            .update
            .nodes
            .iter()
            .find(|(_, node)| node.label() == Some("First group"))
            .expect("first group should be present");
        let (first_child_id, _) = snapshot
            .update
            .nodes
            .iter()
            .find(|(_, node)| node.label() == Some("First child"))
            .expect("first child should be present");
        let (sibling_id, _) = snapshot
            .update
            .nodes
            .iter()
            .find(|(_, node)| node.label() == Some("Sibling"))
            .expect("sibling should be present");
        let root = snapshot
            .update
            .nodes
            .iter()
            .find(|(node_id, _)| *node_id == crate::ROOT_NODE_ID)
            .expect("root should be present");

        assert_eq!(first_group.children(), &[*first_child_id]);
        assert_eq!(root.1.children(), &[*first_group_id, *sibling_id]);
    }

    #[cfg(feature = "accessibility")]
    #[gpui::test]
    fn removed_semantic_nodes_drop_stale_actions(cx: &mut TestAppContext) {
        let action_count = Rc::new(Cell::new(0));
        let window = cx.add_window({
            let action_count = action_count.clone();
            move |_, _| RemovableAccessibilityView {
                action_count,
                show_button: true,
            }
        });
        let handle: AnyWindowHandle = window.into();
        let button_id = cx
            .update_window(handle, |_, window, cx| {
                window.refresh();
                window.draw(cx).clear();
                window
                    .accessibility_snapshot_for_test()
                    .expect("completed frame should contain a semantic snapshot")
                    .update
                    .nodes
                    .iter()
                    .find(|(_, node)| node.label() == Some("Removable action"))
                    .map(|(node_id, _)| *node_id)
                    .expect("removable semantic button should be present")
            })
            .expect("semantic test window should remain open");

        window
            .update(cx, |view, _, cx| {
                view.show_button = false;
                cx.notify();
            })
            .expect("semantic test window should remain open");
        cx.update_window(handle, |_, window, cx| {
            window.draw(cx).clear();
            let snapshot = window
                .accessibility_snapshot_for_test()
                .expect("completed frame should contain a semantic snapshot");
            assert!(
                snapshot
                    .update
                    .nodes
                    .iter()
                    .all(|(_, node)| node.label() != Some("Removable action"))
            );
            assert!(!window.dispatch_accessibility_action(
                button_id,
                accesskit::Action::Click,
                None,
                cx,
            ));
        })
        .expect("semantic test window should remain open");
        assert_eq!(action_count.get(), 0);
    }

    #[cfg(feature = "accessibility")]
    #[gpui::test]
    fn platform_accessibility_callbacks_route_one_action(cx: &mut TestAppContext) {
        let action_count = Rc::new(Cell::new(0));
        let window = cx.add_window({
            let action_count = action_count.clone();
            move |_, _| AccessibilityView { action_count }
        });
        let handle: AnyWindowHandle = window.into();
        let test_window = cx.test_window(handle);

        let initial_tree = test_window
            .simulate_accessibility_activation()
            .expect("test adapter should provide an initial tree");
        assert_eq!(initial_tree.nodes.len(), 1);
        cx.run_until_parked();
        test_window.simulate_frame_request(RequestFrameOptions::default());

        let update = test_window
            .accessibility_update()
            .and_then(AccessibilityUpdate::into_semantic_snapshot)
            .expect("completed frame should reach the platform bridge");
        let button_id = update
            .update
            .nodes
            .iter()
            .find_map(|(node_id, node)| {
                (node.role() == accesskit::Role::Button).then_some(*node_id)
            })
            .expect("semantic button should be present");

        test_window.simulate_accessibility_action(accesskit::ActionRequest {
            action: accesskit::Action::Click,
            target_tree: accesskit::TreeId::ROOT,
            target_node: button_id,
            data: None,
        });
        cx.run_until_parked();
        assert_eq!(action_count.get(), 1);
    }

    #[cfg(feature = "frame-diagnostics")]
    #[gpui::test]
    fn test_frame_diagnostics_follow_build_through_present(cx: &mut TestAppContext) {
        let window = cx.add_window(|_, _| EmptyView);
        let handle: AnyWindowHandle = window.into();
        let window_id = handle.window_id();
        let test_window = cx.test_window(handle);
        let mut collector = cx
            .update_window(handle, |_, window, _| {
                FrameTimingCollector::for_window(window)
            })
            .expect("diagnostics window should remain open");

        test_window.simulate_frame_request(RequestFrameOptions::default());
        let initial = collector.snapshot();
        assert_eq!(initial.dropped_events, 0);
        let completed_build_id = cx
            .update_window(handle, |_, window, _| {
                window
                    .interaction
                    .built_frame(&window.text_input)
                    .diagnostics
                    .build_id
            })
            .expect("diagnostics window should remain open")
            .expect("completed frame should carry a build ID");
        let initial_build_id = initial
            .events
            .iter()
            .find_map(|event| match event {
                FrameEvent::Presented(timing)
                    if timing.window_id == window_id
                        && timing.build_id.as_u64() == completed_build_id =>
                {
                    Some(timing.build_id)
                }
                _ => None,
            })
            .expect("initial frame should be presented");
        assert!(initial.events.iter().any(|event| matches!(
            event,
            FrameEvent::Invalidated(invalidation)
                if invalidation.window_id == window_id
                    && invalidation.reason.earliest_phase() == FrameInvalidationPhase::Layout
        )));
        assert!(initial.events.iter().any(|event| matches!(
            event,
            FrameEvent::Invalidated(invalidation)
                if invalidation.window_id == window_id
                    && invalidation.build_id == initial_build_id
        )));
        assert!(initial.events.iter().any(|event| matches!(
            event,
            FrameEvent::DrawStarted { build_id, window_id: event_window_id, .. }
                if *event_window_id == window_id && *build_id == initial_build_id
        )));
        assert!(initial.events.iter().any(|event| matches!(
            event,
            FrameEvent::DrawFinished(timing)
                if timing.window_id == window_id && timing.build_id == initial_build_id
        )));
        for phase in [
            FramePhase::RequestLayout,
            FramePhase::Prepaint,
            FramePhase::Paint,
        ] {
            assert!(initial.events.iter().any(|event| matches!(
                event,
                FrameEvent::Phase(timing)
                    if timing.window_id == window_id
                        && timing.build_id == initial_build_id
                        && timing.phase == phase
            )));
        }

        cx.update_window(handle, |_, window, _| {
            window.refresh();
            window.refresh();
        })
        .expect("window should remain open");
        test_window.simulate_frame_request(RequestFrameOptions::default());
        let coalesced = collector.snapshot();
        assert!(coalesced.events.iter().any(|event| matches!(
            event,
            FrameEvent::Invalidated(invalidation)
                if invalidation.window_id == window_id && invalidation.coalesced
        )));
        assert!(coalesced.events.iter().any(|event| matches!(
            event,
            FrameEvent::Invalidated(invalidation)
                if matches!(
                    invalidation.reason,
                    FrameDirtyReason::WindowRefresh {
                        earliest_phase: FrameInvalidationPhase::Layout
                    }
                )
        )));

        cx.update_window(handle, |_, window, cx| {
            window.dispatch_event(
                PlatformInput::KeyDown(KeyDownEvent {
                    keystroke: Keystroke::parse("a").expect("valid keystroke"),
                    is_held: false,
                    prefer_character_input: false,
                }),
                cx,
            );
        })
        .expect("window should remain open");
        test_window.simulate_frame_request(RequestFrameOptions::default());
        let input = collector.snapshot();
        assert!(input.events.iter().any(|event| matches!(
            event,
            FrameEvent::Invalidated(invalidation)
                if invalidation.window_id == window_id
                    && invalidation.input == Some(FrameInputProvenance::Keyboard)
        )));
        assert!(input.events.iter().any(|event| matches!(
            event,
            FrameEvent::Presented(timing)
                if timing.window_id == window_id
                    && timing.input == Some(FrameInputProvenance::Keyboard)
                    && timing.input_to_present.is_some()
        )));
    }

    #[cfg(feature = "frame-diagnostics")]
    #[gpui::test]
    fn atlas_epoch_change_bypasses_cached_paint_replay(cx: &mut TestAppContext) {
        let frame = image::Frame::new(image::ImageBuffer::from_pixel(
            1,
            1,
            image::Rgba([0, 0, 255, 255]),
        ));
        let image = Arc::new(RenderImage::new(smallvec::SmallVec::from_elem(frame, 1)));
        let render_count = Rc::new(Cell::new(0));
        let window = cx.add_window({
            let image = image.clone();
            let render_count = render_count.clone();
            move |_, cx| {
                let panel = cx.new(|_| AtlasCachedPanel {
                    image,
                    render_count,
                });
                AtlasCachedRoot { panel }
            }
        });
        let handle: AnyWindowHandle = window.into();
        cx.update_window(handle, |_, window, cx| {
            window.draw_and_present_for_test(cx);
        })
        .expect("atlas cache window should remain open");
        assert_eq!(render_count.get(), 1);
        let test_window = cx.test_window(handle);

        let mut collector = cx
            .update_window(handle, |_, window, _| {
                FrameTimingCollector::for_window(window)
            })
            .expect("atlas cache window should remain open");
        collector.snapshot();

        window
            .update(cx, |_, _, cx| cx.notify())
            .expect("atlas cache root should remain alive");
        test_window.simulate_frame_request(RequestFrameOptions::default());
        assert_eq!(render_count.get(), 1);
        assert!(collector.snapshot().events.iter().any(|event| matches!(
            event,
            FrameEvent::Phase(timing) if timing.phase == FramePhase::PaintCacheReplay
        )));

        cx.update_window(handle, move |_, window, _| window.drop_image(image))
            .expect("atlas cache window should remain open")
            .expect("image removal should succeed");
        collector.snapshot();
        test_window.simulate_frame_request(RequestFrameOptions::default());
        cx.update_window(handle, |_, window, _| {
            assert_eq!(
                window.interaction.rendered_frame.atlas_epoch,
                window.sprite_atlas.current_epoch()
            );
        })
        .expect("atlas cache window should remain open");

        assert_eq!(render_count.get(), 2);
        let final_snapshot = collector.snapshot();
        assert!(!final_snapshot.events.iter().any(|event| matches!(
            event,
            FrameEvent::Phase(timing) if timing.phase == FramePhase::PaintCacheReplay
        )));
        cx.update_window(handle, |_, window, _| {
            let epoch = window.interaction.rendered_frame.atlas_epoch;
            window.present_for_test();
            assert_eq!(window.interaction.rendered_frame.atlas_epoch, epoch);
        })
        .expect("atlas cache window should remain open");
        assert_eq!(render_count.get(), 2);
    }

    #[cfg(feature = "frame-diagnostics")]
    #[gpui::test]
    fn dropping_an_image_refreshes_each_window_at_its_own_epoch(cx: &mut TestAppContext) {
        let frame = image::Frame::new(image::ImageBuffer::from_pixel(
            1,
            1,
            image::Rgba([0, 255, 0, 255]),
        ));
        let image = Arc::new(RenderImage::new(smallvec::SmallVec::from_elem(frame, 1)));
        let first_render_count = Rc::new(Cell::new(0));
        let second_render_count = Rc::new(Cell::new(0));

        let first_window = cx.add_window({
            let image = image.clone();
            let render_count = first_render_count.clone();
            move |_, cx| {
                let panel = cx.new(|_| AtlasCachedPanel {
                    image,
                    render_count,
                });
                AtlasCachedRoot { panel }
            }
        });
        let second_window = cx.add_window({
            let image = image.clone();
            let render_count = second_render_count.clone();
            move |_, cx| {
                let panel = cx.new(|_| AtlasCachedPanel {
                    image,
                    render_count,
                });
                AtlasCachedRoot { panel }
            }
        });
        let first_handle: AnyWindowHandle = first_window.into();
        let second_handle: AnyWindowHandle = second_window.into();
        for handle in [first_handle, second_handle] {
            cx.update_window(handle, |_, window, cx| {
                window.draw_and_present_for_test(cx);
            })
            .expect("atlas cache window should remain open");
        }
        assert_eq!(first_render_count.get(), 1);
        assert_eq!(second_render_count.get(), 1);
        for handle in [first_handle, second_handle] {
            cx.update_window(handle, |_, window, _| {
                let key = crate::RenderImageParams {
                    image_id: image.id,
                    frame_index: 0,
                };
                window
                    .sprite_atlas
                    .get_or_insert_with(key.into(), &mut || {
                        Ok(Some((
                            crate::size(crate::DevicePixels(1), crate::DevicePixels(1)),
                            std::borrow::Cow::Borrowed(&[0, 255, 0, 255]),
                        )))
                    })
                    .expect("test image should enter each atlas")
                    .expect("test image builder should return a tile");
                assert_eq!(window.sprite_atlas.snapshot().entry_count, 1);
            })
            .expect("atlas cache window should remain open");
        }

        let first_test_window = cx.test_window(first_handle);
        let second_test_window = cx.test_window(second_handle);
        let first_wake_count = first_test_window.frame_wake_count();
        let second_wake_count = second_test_window.frame_wake_count();
        cx.update(|cx| cx.drop_image(image, None));
        assert!(first_test_window.frame_wake_count() > first_wake_count);
        assert!(second_test_window.frame_wake_count() > second_wake_count);

        first_test_window.simulate_frame_request(RequestFrameOptions::default());
        second_test_window.simulate_frame_request(RequestFrameOptions::default());
        assert_eq!(first_render_count.get(), 2);
        assert_eq!(second_render_count.get(), 2);
        for handle in [first_handle, second_handle] {
            cx.update_window(handle, |_, window, _| {
                assert_ne!(
                    window.interaction.rendered_frame.atlas_epoch,
                    crate::AtlasEpoch::default()
                );
                assert_eq!(
                    window.interaction.rendered_frame.atlas_epoch,
                    window.sprite_atlas.current_epoch()
                );
            })
            .expect("atlas cache window should remain open");
        }
    }

    #[cfg(feature = "frame-diagnostics")]
    #[gpui::test]
    fn image_removal_during_paint_schedules_the_retirement_frame(cx: &mut TestAppContext) {
        let frame = image::Frame::new(image::ImageBuffer::from_pixel(
            1,
            1,
            image::Rgba([255, 0, 0, 255]),
        ));
        let image = Arc::new(RenderImage::new(smallvec::SmallVec::from_elem(frame, 1)));
        let render_count = Rc::new(Cell::new(0));
        let drop_on_paint = Rc::new(Cell::new(true));
        let window = cx.add_window({
            let render_count = render_count.clone();
            let drop_on_paint = drop_on_paint.clone();
            move |_, _| DropImageDuringPaintView {
                image,
                render_count,
                drop_on_paint,
            }
        });
        let handle: AnyWindowHandle = window.into();
        cx.update_window(handle, |_, window, cx| {
            window.draw_and_present_for_test(cx);
        })
        .expect("atlas retirement window should remain open");
        assert!(!drop_on_paint.get());
        assert!(render_count.get() >= 2);
        cx.update_window(handle, |_, window, _| {
            assert_ne!(
                window.interaction.rendered_frame.atlas_epoch,
                crate::AtlasEpoch::default()
            );
            assert_eq!(window.sprite_atlas.snapshot().entry_count, 1);
        })
        .expect("atlas retirement window should remain open");
    }

    #[cfg(feature = "frame-diagnostics")]
    #[gpui::test]
    fn frame_diagnostics_record_skipped_submission(cx: &mut TestAppContext) {
        let window = cx.add_window(|_, _| EmptyView);
        let handle: AnyWindowHandle = window.into();
        let window_id = handle.window_id();
        let test_window = cx.test_window(handle);
        let mut collector = cx
            .update_window(handle, |_, window, _| {
                FrameTimingCollector::for_window(window)
            })
            .expect("diagnostics window should remain open");
        collector.snapshot();

        test_window.set_draw_result(false);
        window
            .update(cx, |_, window, _| window.refresh())
            .expect("diagnostics window should remain open");
        test_window.simulate_frame_request(RequestFrameOptions::default());

        let snapshot = collector.snapshot();
        let skipped_build_id = snapshot.events.iter().find_map(|event| match event {
            FrameEvent::SubmissionSkipped {
                build_id,
                window_id: event_window_id,
                ..
            } if *event_window_id == window_id => Some(*build_id),
            _ => None,
        });
        assert!(skipped_build_id.is_some());
        assert!(!snapshot.events.iter().any(|event| matches!(
            event,
            FrameEvent::Presented(timing)
                if timing.window_id == window_id
                    && Some(timing.build_id) == skipped_build_id
        )));
    }

    #[cfg(feature = "frame-diagnostics")]
    #[gpui::test]
    fn frame_diagnostics_runner(cx: &mut TestAppContext) {
        const ITERATIONS: usize = 100;
        let panel = cx.new(|_| DiagnosticsPanel);
        let window = cx.add_window({
            let panel = panel.clone();
            move |_, cx| DiagnosticsInputView {
                focus: cx.focus_handle(),
                panel,
            }
        });
        let handle: AnyWindowHandle = window.into();
        let test_window = cx.test_window(handle);
        let mut collector = cx
            .update_window(handle, |_, window, _| {
                FrameTimingCollector::for_window(window)
            })
            .expect("diagnostics window should remain open");

        window
            .update(cx, |view, window, cx| view.focus.focus(window, cx))
            .expect("diagnostics window should remain open");
        test_window.simulate_frame_request(RequestFrameOptions::default());
        collector.snapshot();

        let dirty_started_at = Instant::now();
        for _ in 0..ITERATIONS {
            window
                .update(cx, |_, _, cx| cx.notify())
                .expect("diagnostics window should remain open");
            test_window.simulate_frame_request(RequestFrameOptions::default());
        }
        for _ in 0..ITERATIONS {
            panel.update(cx, |_, cx| cx.notify());
            test_window.simulate_frame_request(RequestFrameOptions::default());
        }
        for _ in 0..ITERATIONS {
            cx.simulate_keystrokes(handle, "a");
            test_window.simulate_frame_request(RequestFrameOptions::default());
        }
        let elapsed = dirty_started_at.elapsed();
        let snapshot = collector.snapshot();

        let mut draw_durations = Vec::new();
        let mut input_latencies = Vec::new();
        let mut phase_durations = [0u128; 5];
        let mut prepaint_replays = 0usize;
        let mut paint_replays = 0usize;
        let mut presented = 0usize;
        let mut submissions_skipped = 0usize;
        for event in snapshot.events {
            match event {
                FrameEvent::DrawFinished(timing) => {
                    draw_durations.push(timing.draw_duration().as_nanos());
                }
                FrameEvent::Presented(timing) => {
                    presented += 1;
                    if let Some(latency) = timing.input_to_present {
                        input_latencies.push(latency.as_nanos());
                    }
                }
                FrameEvent::Phase(timing) => {
                    let index = match timing.phase {
                        FramePhase::RequestLayout => 0,
                        FramePhase::Prepaint => 1,
                        FramePhase::Paint => 2,
                        FramePhase::PrepaintCacheReplay => 3,
                        FramePhase::PaintCacheReplay => 4,
                    };
                    phase_durations[index] += timing.duration.as_nanos();
                    if timing.phase == FramePhase::PrepaintCacheReplay {
                        prepaint_replays += 1;
                    }
                    if timing.phase == FramePhase::PaintCacheReplay {
                        paint_replays += 1;
                    }
                }
                FrameEvent::SubmissionSkipped { .. } => submissions_skipped += 1,
                FrameEvent::Invalidated(_) | FrameEvent::DrawStarted { .. } => {}
            }
        }

        fn percentile(samples: &mut [u128], numerator: usize, denominator: usize) -> u128 {
            samples.sort_unstable();
            let index = samples
                .len()
                .saturating_mul(numerator)
                .div_ceil(denominator)
                .saturating_sub(1);
            samples.get(index).copied().unwrap_or_default()
        }

        assert!(presented >= ITERATIONS * 2);
        assert_eq!(submissions_skipped, 0);
        assert!(!draw_durations.is_empty());
        assert!(!input_latencies.is_empty());
        println!(
            "GPUI_FRAME_DIAGNOSTICS iterations={ITERATIONS} elapsed_ns={} dropped_events={} draws={} draw_p50_ns={} draw_p95_ns={} draw_p99_ns={} input_p50_ns={} input_p95_ns={} input_p99_ns={} prepaint_replays={} paint_replays={} phase_request_layout_ns={} phase_prepaint_ns={} phase_paint_ns={} phase_prepaint_replay_ns={} phase_paint_replay_ns={}",
            elapsed.as_nanos(),
            snapshot.dropped_events,
            draw_durations.len(),
            percentile(&mut draw_durations, 50, 100),
            percentile(&mut draw_durations, 95, 100),
            percentile(&mut draw_durations, 99, 100),
            percentile(&mut input_latencies, 50, 100),
            percentile(&mut input_latencies, 95, 100),
            percentile(&mut input_latencies, 99, 100),
            prepaint_replays,
            paint_replays,
            phase_durations[0],
            phase_durations[1],
            phase_durations[2],
            phase_durations[3],
            phase_durations[4],
        );
    }

    #[gpui::test]
    fn test_window_reports_no_raw_handle_instead_of_panicking(cx: &mut TestAppContext) {
        use raw_window_handle::{HandleError, HasDisplayHandle as _, HasWindowHandle as _};

        let window = cx.add_window(|_, _| EmptyView);
        window
            .update(cx, |_, window, _| {
                assert!(matches!(
                    window.window_handle(),
                    Err(HandleError::NotSupported)
                ));
                assert!(matches!(
                    window.display_handle(),
                    Err(HandleError::NotSupported)
                ));
            })
            .unwrap();
    }

    #[gpui::test]
    fn test_appearance_change_runs_after_app_update(cx: &mut TestAppContext) {
        let window = cx.add_window(|_, _| EmptyView);
        let observed_appearance = Rc::new(Cell::new(None));
        let _subscription = window
            .update(cx, {
                let observed_appearance = observed_appearance.clone();
                move |_, window, _| {
                    window.observe_window_appearance(move |window, _| {
                        observed_appearance.set(Some(window.appearance()));
                    })
                }
            })
            .unwrap();
        let test_window = cx.test_window(window.into());

        cx.update(|_| {
            test_window.simulate_appearance_change(WindowAppearance::Dark);
            assert_eq!(observed_appearance.get(), None);
        });
        cx.run_until_parked();

        assert_eq!(observed_appearance.get(), Some(WindowAppearance::Dark));
    }

    impl Render for RootView {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let child_bounds = self.child_bounds.clone();
            let root = div().flex().flex_col().child(
                canvas(
                    move |bounds, _, _| child_bounds.set(bounds),
                    |_, _, _, _| {},
                )
                .size_full(),
            );
            if self.explicit_size {
                root.w(px(300.)).h(px(200.))
            } else {
                root
            }
        }
    }

    #[test]
    fn auto_sized_window_root_fills_the_window() {
        let mut cx = TestAppContext::single();
        let child_bounds = Rc::new(Cell::new(Bounds::default()));
        let window = cx.add_window({
            let child_bounds = child_bounds.clone();
            move |_, _| RootView {
                explicit_size: false,
                child_bounds,
            }
        });

        let viewport_size = cx
            .update_window(window.into(), |_, window, cx| {
                window.draw(cx).clear();
                window.viewport_size()
            })
            .expect("window should remain available while drawing the root");

        assert_eq!(child_bounds.get().size, viewport_size);
    }

    #[test]
    fn explicitly_sized_window_root_keeps_its_size() {
        let mut cx = TestAppContext::single();
        let child_bounds = Rc::new(Cell::new(Bounds::default()));
        let window = cx.add_window({
            let child_bounds = child_bounds.clone();
            move |_, _| RootView {
                explicit_size: true,
                child_bounds,
            }
        });

        cx.update_window(window.into(), |_, window, cx| {
            window.draw(cx).clear();
        })
        .expect("window should remain available while drawing the root");

        assert_eq!(child_bounds.get().size, size(px(300.), px(200.)));
    }

    struct FocusForwarder {
        a: FocusHandle,
        b: FocusHandle,
    }

    impl Render for FocusForwarder {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .child(div().w(px(50.)).h(px(50.)).track_focus(&self.a))
                .child(div().w(px(50.)).h(px(50.)).track_focus(&self.b))
        }
    }

    #[gpui::test]
    fn test_focus_moved_by_focus_listener_is_dispatched(cx: &mut TestAppContext) {
        let b_focus_count = Rc::new(Cell::new(0));
        let window = cx.add_window({
            let b_focus_count = b_focus_count.clone();
            move |window, cx| {
                let a = cx.focus_handle();
                let b = cx.focus_handle();
                cx.on_focus(&a, window, |this: &mut FocusForwarder, window, cx| {
                    let b = this.b.clone();
                    window.focus(&b, cx);
                })
                .detach();
                cx.on_focus(&b, window, move |_, _, _| {
                    b_focus_count.set(b_focus_count.get() + 1);
                })
                .detach();
                FocusForwarder { a, b }
            }
        });

        window
            .update(cx, |_, window, _| window.activate_window())
            .unwrap();
        cx.executor().run_until_parked();

        window
            .update(cx, |this, window, cx| {
                let a = this.a.clone();
                window.focus(&a, cx);
            })
            .unwrap();
        cx.executor().run_until_parked();

        window
            .update(cx, |this, window, _| {
                assert!(this.b.is_focused(window));
            })
            .unwrap();
        assert_eq!(b_focus_count.get(), 1);
    }

    #[gpui::test]
    fn cached_view_follows_inherited_opacity(cx: &mut TestAppContext) {
        struct Child {
            renders: usize,
        }

        impl Render for Child {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                self.renders += 1;
                div().size_full().bg(crate::rgb(0xff0000))
            }
        }

        struct Root {
            opacity: f32,
            width: Pixels,
            child: crate::Entity<Child>,
        }

        impl Render for Root {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                div()
                    .id("opacity-root")
                    .w(self.width)
                    .h(px(200.))
                    .opacity(self.opacity)
                    .hover(|style| style.opacity(1.))
                    .child(
                        crate::AnyView::from(self.child.clone())
                            .cached(crate::StyleRefinement::default().size(px(100.))),
                    )
            }
        }

        /// Updates the root, draws a few frames, and returns the child's final painted
        /// alpha and how many times it has rendered.
        fn update_and_draw(
            window: crate::WindowHandle<Root>,
            child: &crate::Entity<Child>,
            cx: &mut TestAppContext,
            update: impl FnOnce(&mut Root, &mut Window, &mut Context<Root>),
        ) -> (f32, usize) {
            window
                .update(cx, |root, window, cx| {
                    update(root, window, cx);
                    cx.notify();
                })
                .expect("update root");
            let mut result = (0., 0);
            // Cached output replayed under a different opacity only redraws on the next
            // frame, so draw enough frames for the view to settle.
            for _ in 0..3 {
                result = cx
                    .update_window(window.into(), |_, window, cx| {
                        window.draw(cx).clear();
                        let quads = &window.interaction.rendered_frame.scene.quads;
                        assert_eq!(quads.len(), 1);
                        let alpha = quads.first().expect("child quad").background.solid.a;
                        (alpha, child.read(cx).renders)
                    })
                    .expect("draw window");
            }
            result
        }

        let child = cx.new(|_| Child { renders: 0 });
        let window = cx.add_window(|_, _| Root {
            opacity: 1.,
            width: px(200.),
            child: child.clone(),
        });
        let outside = crate::point(px(500.), px(500.));
        let inside = crate::point(px(50.), px(50.));
        let beside = crate::point(px(250.), px(50.));

        // Changing the wrapper's opacity redraws the child; unchanged opacity reuses it.
        let move_outside = |_: &mut Root, window: &mut Window, cx: &mut Context<Root>| {
            window.simulate_mouse_move(outside, cx)
        };
        // Caching is disabled when the `accessibility` feature is enabled, so the
        // child is re-rendered on every frame in that configuration. The opacity
        // assertions still apply to both configurations.
        fn assert_draw_state(actual: (f32, usize), expected_alpha: f32, expected_renders: usize) {
            assert_eq!(actual.0, expected_alpha);
            if !cfg!(feature = "accessibility") {
                assert_eq!(actual.1, expected_renders);
            }
        }
        assert_draw_state(update_and_draw(window, &child, cx, move_outside), 1., 1);
        assert_draw_state(update_and_draw(window, &child, cx, |_, _, _| {}), 1., 1);
        let fade = |root: &mut Root, _: &mut Window, _: &mut Context<Root>| root.opacity = 0.5;
        assert_draw_state(update_and_draw(window, &child, cx, fade), 0.5, 2);
        assert_draw_state(update_and_draw(window, &child, cx, |_, _, _| {}), 0.5, 2);

        // Hover opacity applies when the mouse enters and leaves.
        let move_inside = |_: &mut Root, window: &mut Window, cx: &mut Context<Root>| {
            window.simulate_mouse_move(inside, cx)
        };
        assert_eq!(update_and_draw(window, &child, cx, move_inside).0, 1.);
        let move_beside = |_: &mut Root, window: &mut Window, cx: &mut Context<Root>| {
            window.simulate_mouse_move(beside, cx)
        };
        assert_eq!(update_and_draw(window, &child, cx, move_beside).0, 0.5);
    }

    #[gpui::test]
    fn test_drag_cursor_applies_before_next_paint(cx: &mut TestAppContext) {
        use crate::{
            CursorStyle, Empty, InputEvent as _, MouseButton, MouseDownEvent, MouseMoveEvent,
            MouseUpEvent, StatefulInteractiveElement as _, point,
        };

        struct ResizeHandle;

        impl Render for ResizeHandle {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                div().size_full().cursor_text().child(
                    div()
                        .id("resize-handle")
                        .w(px(4.))
                        .h_full()
                        .cursor_col_resize()
                        .on_drag((), |_, _, _, cx| cx.new(|_| Empty)),
                )
            }
        }

        let window = cx.add_window(|_, _| ResizeHandle);
        let handle_position = point(px(1.), px(10.));
        let drag_position = point(px(20.), px(10.));
        let cursor_observer = cx.clone();

        let update_result = cx.update_window(window.into(), |_, window, cx| {
            window.hovered.set(true);
            window.active.set(true);
            window.draw(cx).clear();
            window.dispatch_event(
                MouseMoveEvent {
                    position: handle_position,
                    ..Default::default()
                }
                .to_platform_input(),
                cx,
            );
            window.dispatch_event(
                MouseDownEvent {
                    position: handle_position,
                    button: MouseButton::Left,
                    ..Default::default()
                }
                .to_platform_input(),
                cx,
            );
            window.dispatch_event(
                MouseMoveEvent {
                    position: drag_position,
                    pressed_button: Some(MouseButton::Left),
                    ..Default::default()
                }
                .to_platform_input(),
                cx,
            );
            assert!(cx.has_active_drag());
            // Asserted here because leaving this update flushes effects, which paints a frame.
            assert_eq!(
                cursor_observer.test_platform().cursor_style(),
                CursorStyle::ResizeColumn
            );

            window.draw(cx).clear();
            window.dispatch_event(
                MouseUpEvent {
                    position: drag_position,
                    button: MouseButton::Left,
                    ..Default::default()
                }
                .to_platform_input(),
                cx,
            );
            window.draw(cx).clear();
        });
        assert!(
            update_result.is_ok(),
            "failed to run drag: {update_result:?}"
        );
        assert_eq!(
            cursor_observer.test_platform().cursor_style(),
            CursorStyle::IBeam
        );
    }
}
