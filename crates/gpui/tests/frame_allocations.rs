use gpui::{
    AnyView, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render,
    StyleRefinement, Styled as _, TestAppContext, Window, div,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Instant;

struct CountingAllocator;

static COUNTING_ENABLED: AtomicBool = AtomicBool::new(false);
static ALLOCATION_COUNT: AtomicUsize = AtomicUsize::new(0);
static ALLOCATED_BYTES: AtomicUsize = AtomicUsize::new(0);

#[global_allocator]
static GLOBAL_ALLOCATOR: CountingAllocator = CountingAllocator;

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: The system allocator receives the layout supplied by Rust's allocation API.
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() && COUNTING_ENABLED.load(Ordering::Relaxed) {
            ALLOCATION_COUNT.fetch_add(1, Ordering::Relaxed);
            ALLOCATED_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: The system allocator receives the layout supplied by Rust's allocation API.
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() && COUNTING_ENABLED.load(Ordering::Relaxed) {
            ALLOCATION_COUNT.fetch_add(1, Ordering::Relaxed);
            ALLOCATED_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: The pointer and layout pair came from this allocator's allocation methods.
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: The pointer and layout pair came from this allocator's allocation methods.
        let pointer = unsafe { System.realloc(pointer, layout, new_size) };
        if !pointer.is_null() && COUNTING_ENABLED.load(Ordering::Relaxed) {
            ALLOCATION_COUNT.fetch_add(1, Ordering::Relaxed);
            ALLOCATED_BYTES.fetch_add(new_size, Ordering::Relaxed);
        }
        pointer
    }
}

struct CachedPanel;

impl Render for CachedPanel {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child("allocation probe")
    }
}

struct AllocationRoot {
    panel: Entity<CachedPanel>,
}

impl Render for AllocationRoot {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .child(AnyView::from(self.panel.clone()).cached(StyleRefinement::default()))
    }
}

fn reset_counting() {
    COUNTING_ENABLED.store(false, Ordering::SeqCst);
    ALLOCATION_COUNT.store(0, Ordering::SeqCst);
    ALLOCATED_BYTES.store(0, Ordering::SeqCst);
    COUNTING_ENABLED.store(true, Ordering::SeqCst);
}

fn finish_counting() -> (usize, usize) {
    COUNTING_ENABLED.store(false, Ordering::SeqCst);
    (
        ALLOCATION_COUNT.load(Ordering::SeqCst),
        ALLOCATED_BYTES.load(Ordering::SeqCst),
    )
}

#[test]
fn frame_allocation_probe() {
    const FRAMES: usize = 100;
    let mut cx = TestAppContext::single();
    let panel = cx.new(|_| CachedPanel);
    let window = cx.add_window(move |_, _| AllocationRoot { panel });
    let handle = window.into();
    cx.run_until_parked();

    for _ in 0..10 {
        cx.update_window(handle, |_, window, cx| window.draw_and_present_for_test(cx))
            .expect("allocation probe window should remain open");
    }
    for _ in 0..FRAMES {
        window
            .update(&mut cx, |_, _, cx| cx.notify())
            .expect("allocation probe window should remain open");
        cx.update_window(handle, |_, window, cx| window.draw_and_present_for_test(cx))
            .expect("allocation probe window should remain open");
    }

    reset_counting();
    let cached_started_at = Instant::now();
    for _ in 0..FRAMES {
        cx.update_window(handle, |_, window, cx| window.draw_and_present_for_test(cx))
            .expect("allocation probe window should remain open");
    }
    let cached_elapsed_ns = cached_started_at.elapsed().as_nanos();
    let (cached_allocations, cached_bytes) = finish_counting();

    reset_counting();
    let dirty_started_at = Instant::now();
    for _ in 0..FRAMES {
        window
            .update(&mut cx, |_, _, cx| cx.notify())
            .expect("allocation probe window should remain open");
        cx.update_window(handle, |_, window, cx| window.draw_and_present_for_test(cx))
            .expect("allocation probe window should remain open");
    }
    let dirty_elapsed_ns = dirty_started_at.elapsed().as_nanos();
    let (dirty_allocations, dirty_bytes) = finish_counting();

    println!(
        "GPUI_FRAME_ALLOCATIONS frames={FRAMES} cached_elapsed_ns={} dirty_elapsed_ns={} cached_allocations={} cached_bytes={} dirty_allocations={} dirty_bytes={} cached_allocations_per_frame={} dirty_allocations_per_frame={} cached_bytes_per_frame={} dirty_bytes_per_frame={}",
        cached_elapsed_ns,
        dirty_elapsed_ns,
        cached_allocations,
        cached_bytes,
        dirty_allocations,
        dirty_bytes,
        cached_allocations / FRAMES,
        dirty_allocations / FRAMES,
        cached_bytes / FRAMES,
        dirty_bytes / FRAMES,
    );

    assert!(cached_allocations > 0 || dirty_allocations > 0);
}
