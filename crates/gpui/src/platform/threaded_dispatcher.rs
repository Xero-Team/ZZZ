use std::{
    collections::BinaryHeap,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use parking_lot::{Condvar, Mutex};

use crate::{
    PlatformDispatcher, Priority, RunnableVariant, ThreadTaskTimings,
    profiler::{self, GLOBAL_THREAD_TIMINGS},
    queue::{PriorityQueueReceiver, PriorityQueueSender},
};

const MIN_THREADS: usize = 2;

/// A multithreaded dispatcher for integration tests and benchmarks.
///
/// Background tasks run on worker threads and timers use real time. Main-thread
/// tasks remain queued until the creating thread calls [`Self::run_until_idle`].
/// Ordinary unit tests should continue to use [`crate::TestDispatcher`].
pub struct ThreadedDispatcher {
    background_sender: PriorityQueueSender<RunnableVariant>,
    main_sender: PriorityQueueSender<RunnableVariant>,
    main_receiver: Mutex<PriorityQueueReceiver<RunnableVariant>>,
    timers: Arc<TimerQueue>,
    idle: Arc<IdleTracker>,
    main_thread_id: thread::ThreadId,
}

#[derive(Default)]
struct IdleTracker {
    inflight: Mutex<usize>,
    condvar: Condvar,
}

impl IdleTracker {
    fn increment(&self) {
        *self.inflight.lock() += 1;
    }

    fn decrement(&self) {
        let mut inflight = self.inflight.lock();
        *inflight = inflight.saturating_sub(1);
        if *inflight == 0 {
            self.condvar.notify_all();
        }
    }

    fn decrement_on_drop(&self) -> impl Drop + '_ {
        gpui_util::defer(|| self.decrement())
    }

    fn notify_under_lock(&self) {
        let _inflight = self.inflight.lock();
        self.condvar.notify_all();
    }
}

struct TimerQueue {
    state: Mutex<TimerQueueState>,
    condvar: Condvar,
}

struct TimerQueueState {
    heap: BinaryHeap<TimerEntry>,
    next_sequence: u64,
    shutdown: bool,
}

struct TimerEntry {
    due: Instant,
    sequence: u64,
    runnable: RunnableVariant,
}

impl PartialEq for TimerEntry {
    fn eq(&self, other: &Self) -> bool {
        self.due == other.due && self.sequence == other.sequence
    }
}

impl Eq for TimerEntry {}

impl PartialOrd for TimerEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TimerEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other
            .due
            .cmp(&self.due)
            .then_with(|| other.sequence.cmp(&self.sequence))
    }
}

impl Default for ThreadedDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl ThreadedDispatcher {
    /// Creates a dispatcher whose main thread is the calling thread.
    pub fn new() -> Self {
        let (background_sender, background_receiver) = PriorityQueueReceiver::new();
        let (main_sender, main_receiver) = PriorityQueueReceiver::new();
        let idle = Arc::new(IdleTracker::default());

        let thread_count = thread::available_parallelism()
            .map_or(MIN_THREADS, |count| count.get().clamp(MIN_THREADS, 8));
        for index in 0..thread_count {
            let mut receiver: PriorityQueueReceiver<RunnableVariant> = background_receiver.clone();
            let idle = idle.clone();
            thread::Builder::new()
                .name(format!("ThreadedDispatcherWorker-{index}"))
                .spawn(move || {
                    while let Ok(runnable) = receiver.pop() {
                        let _decrement = idle.decrement_on_drop();
                        if catch_unwind(AssertUnwindSafe(|| runnable.run())).is_err() {
                            log::error!("threaded dispatcher background task panicked");
                        }
                    }
                })
                .expect("threaded dispatcher worker should spawn");
        }
        drop(background_receiver);

        let timers = Arc::new(TimerQueue {
            state: Mutex::new(TimerQueueState {
                heap: BinaryHeap::new(),
                next_sequence: 0,
                shutdown: false,
            }),
            condvar: Condvar::new(),
        });
        {
            let timers = timers.clone();
            let idle = idle.clone();
            thread::Builder::new()
                .name("ThreadedDispatcherTimer".to_owned())
                .spawn(move || {
                    let mut state = timers.state.lock();
                    loop {
                        if state.shutdown {
                            return;
                        }
                        let Some(entry) = state.heap.peek() else {
                            timers.condvar.wait(&mut state);
                            continue;
                        };
                        let due = entry.due;
                        if due > Instant::now() {
                            timers.condvar.wait_until(&mut state, due);
                            continue;
                        }
                        let Some(entry) = state.heap.pop() else {
                            continue;
                        };
                        idle.increment();
                        drop(state);
                        {
                            let _decrement = idle.decrement_on_drop();
                            if catch_unwind(AssertUnwindSafe(|| entry.runnable.run())).is_err() {
                                log::error!("threaded dispatcher timer task panicked");
                            }
                        }
                        state = timers.state.lock();
                    }
                })
                .expect("threaded dispatcher timer should spawn");
        }

        Self {
            background_sender,
            main_sender,
            main_receiver: Mutex::new(main_receiver),
            timers,
            idle,
            main_thread_id: thread::current().id(),
        }
    }

    /// Runs main-thread work until background work and due timers become idle.
    pub fn run_until_idle(&self) {
        assert!(
            self.is_main_thread(),
            "run_until_idle must run on the dispatcher's main thread"
        );
        loop {
            if self.drain_main_queue() {
                continue;
            }
            if self.has_due_timer() {
                let mut inflight = self.idle.inflight.lock();
                self.idle
                    .condvar
                    .wait_for(&mut inflight, Duration::from_millis(1));
                continue;
            }
            let mut inflight = self.idle.inflight.lock();
            if self.main_queue_has_work() {
                continue;
            }
            if *inflight == 0 {
                return;
            }
            self.idle.condvar.wait(&mut inflight);
        }
    }

    /// Runs the main-thread tasks that were ready when this method began.
    pub fn run_ready_main_tasks(&self) -> bool {
        assert!(
            self.is_main_thread(),
            "main tasks must run on the dispatcher's main thread"
        );
        let pending = self.main_receiver.lock().len();
        let mut ran_any = false;
        for _ in 0..pending {
            if !self.run_one_main_task() {
                break;
            }
            ran_any = true;
        }
        ran_any
    }

    /// Cancels all timers that have not fired.
    pub fn cancel_pending_timers(&self) -> usize {
        let timers = {
            let mut state = self.timers.state.lock();
            let timers: Vec<_> = state.heap.drain().collect();
            self.timers.condvar.notify_all();
            timers
        };
        let canceled = timers.len();
        drop(timers);
        canceled
    }

    /// Returns whether the dispatcher has no queued or in-flight work.
    pub fn is_idle(&self) -> bool {
        !self.main_queue_has_work() && !self.has_due_timer() && *self.idle.inflight.lock() == 0
    }

    /// Describes the current queues for failure diagnostics.
    pub fn debug_state(&self) -> String {
        let inflight = *self.idle.inflight.lock();
        let timers = self.timers.state.lock().heap.len();
        let main_queue_has_work = self.main_queue_has_work();
        format!(
            "ThreadedDispatcher {{ inflight: {inflight}, pending_timers: {timers}, main_queue_has_work: {main_queue_has_work} }}"
        )
    }

    fn run_one_main_task(&self) -> bool {
        let runnable = self.main_receiver.lock().try_pop();
        match runnable {
            Ok(Some(runnable)) => {
                runnable.run();
                true
            }
            Ok(None) | Err(_) => false,
        }
    }

    fn has_due_timer(&self) -> bool {
        let state = self.timers.state.lock();
        state
            .heap
            .peek()
            .is_some_and(|entry| entry.due <= Instant::now())
    }

    fn main_queue_has_work(&self) -> bool {
        !self.main_receiver.lock().is_empty()
    }

    fn drain_main_queue(&self) -> bool {
        let mut ran_any = false;
        while self.run_one_main_task() {
            ran_any = true;
        }
        ran_any
    }
}

impl Drop for ThreadedDispatcher {
    fn drop(&mut self) {
        let mut state = self.timers.state.lock();
        state.shutdown = true;
        state.heap.clear();
        self.timers.condvar.notify_all();
    }
}

impl PlatformDispatcher for ThreadedDispatcher {
    fn get_all_timings(&self) -> Vec<ThreadTaskTimings> {
        ThreadTaskTimings::convert(&GLOBAL_THREAD_TIMINGS.lock())
    }

    fn get_current_thread_timings(&self) -> ThreadTaskTimings {
        profiler::get_current_thread_task_timings()
    }

    fn is_main_thread(&self) -> bool {
        thread::current().id() == self.main_thread_id
    }

    fn dispatch(&self, runnable: RunnableVariant, priority: Priority) {
        self.idle.increment();
        if self.background_sender.send(priority, runnable).is_err() {
            self.idle.decrement();
            panic!("threaded dispatcher workers are no longer running");
        }
    }

    fn dispatch_on_main_thread(&self, runnable: RunnableVariant, priority: Priority) {
        if let Err(error) = self.main_sender.send(priority, runnable) {
            drop(error);
            return;
        }
        self.idle.notify_under_lock();
    }

    fn dispatch_after(&self, duration: Duration, runnable: RunnableVariant) {
        let mut state = self.timers.state.lock();
        let sequence = state.next_sequence;
        state.next_sequence += 1;
        state.heap.push(TimerEntry {
            due: Instant::now() + duration,
            sequence,
            runnable,
        });
        self.timers.condvar.notify_one();
    }

    fn spawn_realtime(&self, callback: Box<dyn FnOnce() + Send>) {
        thread::Builder::new()
            .name("ThreadedDispatcherRealtime".to_owned())
            .spawn(callback)
            .expect("threaded dispatcher realtime thread should spawn");
    }

    fn as_threaded(&self) -> Option<&ThreadedDispatcher> {
        Some(self)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use super::*;
    use crate::{BackgroundExecutor, ForegroundExecutor};
    use futures::channel::oneshot;

    #[test]
    fn completes_background_to_main_handoffs() {
        let dispatcher = Arc::new(ThreadedDispatcher::new());
        let background = BackgroundExecutor::new(dispatcher.clone());
        let foreground = ForegroundExecutor::new(dispatcher.clone());
        let completed = Arc::new(AtomicBool::new(false));
        let (sender, receiver) = oneshot::channel();

        foreground
            .spawn({
                let completed = completed.clone();
                async move {
                    receiver.await.expect("background handoff should complete");
                    completed.store(true, Ordering::SeqCst);
                }
            })
            .detach();

        background
            .spawn(async move { sender.send(()).expect("foreground receiver should live") })
            .detach();

        dispatcher.run_until_idle();
        assert!(completed.load(Ordering::SeqCst));
        assert!(dispatcher.is_idle(), "{}", dispatcher.debug_state());
    }

    #[test]
    fn timers_fire_and_pending_timers_cancel() {
        let dispatcher = Arc::new(ThreadedDispatcher::new());
        let background = BackgroundExecutor::new(dispatcher.clone());
        let fired = Arc::new(AtomicBool::new(false));
        let timer = background.timer(Duration::from_millis(5));
        background
            .spawn({
                let fired = fired.clone();
                async move {
                    timer.await;
                    fired.store(true, Ordering::SeqCst);
                }
            })
            .detach();

        let deadline = Instant::now() + Duration::from_secs(1);
        while !fired.load(Ordering::SeqCst) && Instant::now() < deadline {
            dispatcher.run_until_idle();
            thread::yield_now();
        }
        assert!(fired.load(Ordering::SeqCst));

        let pending_timer = background.timer(Duration::from_secs(10));
        background.spawn(pending_timer).detach();
        dispatcher.run_until_idle();
        assert_eq!(dispatcher.cancel_pending_timers(), 1);
        dispatcher.run_until_idle();
        assert_eq!(dispatcher.cancel_pending_timers(), 0);
    }

    #[test]
    fn repeated_dispatchers_complete_work_and_teardown() {
        for _ in 0..100 {
            let dispatcher = Arc::new(ThreadedDispatcher::new());
            let background = BackgroundExecutor::new(dispatcher.clone());
            let completed = Arc::new(AtomicUsize::new(0));
            for _ in 0..4 {
                background
                    .spawn({
                        let completed = completed.clone();
                        async move {
                            completed.fetch_add(1, Ordering::SeqCst);
                        }
                    })
                    .detach();
            }
            dispatcher.run_until_idle();
            assert_eq!(completed.load(Ordering::SeqCst), 4);
        }
    }

    #[test]
    fn panic_and_cancellation_leave_dispatcher_idle() {
        let dispatcher = Arc::new(ThreadedDispatcher::new());
        let background = BackgroundExecutor::new(dispatcher.clone());

        background
            .spawn(async move {
                panic!("intentional threaded dispatcher test panic");
            })
            .detach();
        dispatcher.run_until_idle();
        assert!(dispatcher.is_idle(), "{}", dispatcher.debug_state());

        let started = Arc::new(AtomicBool::new(false));
        let task = background.spawn({
            let started = started.clone();
            async move {
                started.store(true, Ordering::SeqCst);
                std::future::pending::<()>().await;
            }
        });
        while !started.load(Ordering::SeqCst) {
            thread::yield_now();
        }
        drop(task);
        dispatcher.run_until_idle();
        assert!(dispatcher.is_idle(), "{}", dispatcher.debug_state());
    }
}
