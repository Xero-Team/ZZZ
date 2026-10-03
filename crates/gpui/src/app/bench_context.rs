use std::{future::Future, rc::Rc, sync::Arc};

use anyhow::{Result, anyhow};

#[cfg(feature = "test-support")]
use crate::ThreadedDispatcher;
use crate::{
    AnyView, AnyWindowHandle, App, AppCell, AppContext, BackgroundExecutor, Bounds, Context, Empty,
    Entity, EntityId, Focusable, ForegroundExecutor, Global, Render, Reservation, Task,
    TestDispatcher, TestPlatform, VisualContext, Window, WindowBounds, WindowHandle, WindowOptions,
    app::{GpuiBorrow, GpuiMode},
};

#[derive(Clone)]
enum BenchDispatcher {
    Deterministic(TestDispatcher),
    #[cfg(feature = "test-support")]
    Threaded(Arc<ThreadedDispatcher>),
}

impl BenchDispatcher {
    fn run_until_idle(&self) {
        match self {
            Self::Deterministic(dispatcher) => dispatcher.run_until_parked(),
            #[cfg(feature = "test-support")]
            Self::Threaded(dispatcher) => dispatcher.run_until_idle(),
        }
    }

    fn is_deterministic(&self) -> bool {
        matches!(self, Self::Deterministic(_))
    }
}

/// A GPUI app context for Criterion benchmarks.
///
/// `BenchAppContext` is intentionally separate from `TestAppContext`: it owns a
/// benchmark app instance and exposes only the app/window operations needed by
/// benchmark setup. Criterion remains responsible for the measured loop via its
/// `Bencher` API.
#[derive(Clone)]
pub struct BenchAppContext {
    app: Rc<AppCell>,
    background_executor: BackgroundExecutor,
    foreground_executor: ForegroundExecutor,
    dispatcher: BenchDispatcher,
    benchmark_name: Option<&'static str>,
}

impl BenchAppContext {
    /// Creates a new benchmark app context.
    pub fn new(benchmark_name: Option<&'static str>) -> Self {
        Self::with_seed(benchmark_name, 0)
    }

    /// Creates a new benchmark app context with the provided scheduler seed.
    pub fn with_seed(benchmark_name: Option<&'static str>, seed: u64) -> Self {
        let dispatcher = TestDispatcher::new(seed);
        Self::build(
            Arc::new(dispatcher.clone()),
            BenchDispatcher::Deterministic(dispatcher),
            benchmark_name,
        )
    }

    /// Creates a benchmark context backed by real worker threads and timers.
    #[cfg(feature = "test-support")]
    pub fn threaded(benchmark_name: Option<&'static str>) -> Self {
        let dispatcher = Arc::new(ThreadedDispatcher::new());
        Self::build(
            dispatcher.clone(),
            BenchDispatcher::Threaded(dispatcher),
            benchmark_name,
        )
    }

    fn build(
        platform_dispatcher: Arc<dyn crate::PlatformDispatcher>,
        dispatcher: BenchDispatcher,
        benchmark_name: Option<&'static str>,
    ) -> Self {
        let background_executor = BackgroundExecutor::new(platform_dispatcher.clone());
        let foreground_executor = ForegroundExecutor::new(platform_dispatcher);
        let platform = TestPlatform::new(background_executor.clone(), foreground_executor.clone());
        let asset_source = Arc::new(());
        let http_client = http_client::FakeHttpClient::with_404_response();
        let app = App::new_app(platform, asset_source, http_client);
        app.borrow_mut().mode = GpuiMode::test();

        Self {
            app,
            background_executor,
            foreground_executor,
            dispatcher,
            benchmark_name,
        }
    }

    /// The benchmark function name that created this context.
    pub fn benchmark_name(&self) -> Option<&'static str> {
        self.benchmark_name
    }

    /// Returns the background executor used by this benchmark app.
    pub fn background_executor(&self) -> &BackgroundExecutor {
        &self.background_executor
    }

    /// Returns the foreground executor used by this benchmark app.
    pub fn foreground_executor(&self) -> &ForegroundExecutor {
        &self.foreground_executor
    }

    /// Runs pending scheduled work until the benchmark app is idle.
    pub fn run_until_idle(&self) {
        self.dispatcher.run_until_idle();
    }

    /// Updates the app and flushes synchronous GPUI effects afterward.
    pub fn update<R>(&mut self, update: impl FnOnce(&mut App) -> R) -> R {
        let mut app = self.app.borrow_mut();
        app.update(update)
    }

    /// Reads app state.
    pub fn read<R>(&self, read: impl FnOnce(&App) -> R) -> R {
        let app = self.app.borrow();
        read(&app)
    }

    /// Adds a window with an empty root view for benchmark setup.
    pub fn add_empty_window(&mut self) -> BenchWindowContext {
        let window = {
            let mut app = self.app.borrow_mut();
            let bounds = Bounds::maximized(None, &app);
            let window: AnyWindowHandle = app
                .open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(bounds)),
                        ..Default::default()
                    },
                    |_, cx| cx.new(|_| Empty),
                )
                .expect("failed to open benchmark window")
                .into();
            window
        };

        self.run_until_idle();
        BenchWindowContext {
            cx: self.clone(),
            window,
        }
    }

    /// Runs GPUI benchmark teardown.
    pub fn teardown(mut self) {
        self.run_until_idle();
        let deterministic = self.dispatcher.is_deterministic();
        self.update(move |cx| {
            if deterministic {
                cx.background_executor().forbid_parking();
            }
            cx.quit();
        });
        self.run_until_idle();
    }
}

impl AppContext for BenchAppContext {
    fn new<T: 'static>(&mut self, build_entity: impl FnOnce(&mut Context<T>) -> T) -> Entity<T> {
        let mut app = self.app.borrow_mut();
        app.new(build_entity)
    }

    fn reserve_entity<T: 'static>(&mut self) -> Reservation<T> {
        let mut app = self.app.borrow_mut();
        app.reserve_entity()
    }

    fn insert_entity<T: 'static>(
        &mut self,
        reservation: Reservation<T>,
        build_entity: impl FnOnce(&mut Context<T>) -> T,
    ) -> Entity<T> {
        let mut app = self.app.borrow_mut();
        app.insert_entity(reservation, build_entity)
    }

    fn update_entity<T: 'static, R>(
        &mut self,
        handle: &Entity<T>,
        update: impl FnOnce(&mut T, &mut Context<T>) -> R,
    ) -> R {
        let mut app = self.app.borrow_mut();
        app.update_entity(handle, update)
    }

    fn as_mut<'a, T>(&'a mut self, _: &Entity<T>) -> GpuiBorrow<'a, T>
    where
        T: 'static,
    {
        panic!("Cannot use as_mut with BenchAppContext. Call update() instead.")
    }

    fn read_entity<T, R>(&self, handle: &Entity<T>, read: impl FnOnce(&T, &App) -> R) -> R
    where
        T: 'static,
    {
        let app = self.app.borrow();
        app.read_entity(handle, read)
    }

    fn update_window<T, F>(&mut self, window: AnyWindowHandle, update: F) -> Result<T>
    where
        F: FnOnce(AnyView, &mut Window, &mut App) -> T,
    {
        let mut app = self.app.borrow_mut();
        app.update_window(window, update)
    }

    fn with_window<R>(
        &mut self,
        entity_id: EntityId,
        update: impl FnOnce(&mut Window, &mut App) -> R,
    ) -> Option<R> {
        let mut app = self.app.borrow_mut();
        app.with_window(entity_id, update)
    }

    fn read_window<T, R>(
        &self,
        window: &WindowHandle<T>,
        read: impl FnOnce(Entity<T>, &App) -> R,
    ) -> Result<R>
    where
        T: 'static,
    {
        let app = self.app.borrow();
        app.read_window(window, read)
    }

    fn background_spawn<R>(&self, future: impl Future<Output = R> + Send + 'static) -> Task<R>
    where
        R: Send + 'static,
    {
        self.background_executor.spawn(future)
    }

    fn read_global<G, R>(&self, callback: impl FnOnce(&G, &App) -> R) -> R
    where
        G: Global,
    {
        let app = self.app.borrow();
        app.read_global(callback)
    }
}

/// A window-specific context for GPUI benchmarks.
///
/// This is separate from `VisualTestContext`; it provides access to a benchmark
/// window without exposing test-only helpers such as input simulation.
#[derive(Clone)]
pub struct BenchWindowContext {
    cx: BenchAppContext,
    window: AnyWindowHandle,
}

impl BenchWindowContext {
    /// Returns the underlying benchmark app context.
    pub fn app_context(&mut self) -> &mut BenchAppContext {
        &mut self.cx
    }

    /// Returns the window associated with this context.
    pub fn window_handle(&self) -> AnyWindowHandle {
        self.window
    }

    /// Updates the benchmark window.
    pub fn update<R>(&mut self, update: impl FnOnce(&mut Window, &mut App) -> R) -> R {
        self.cx
            .update_window(self.window, |_, window, cx| update(window, cx))
            .expect("benchmark window was unexpectedly closed")
    }

    /// Runs pending scheduled work until the benchmark app is idle.
    pub fn run_until_idle(&self) {
        self.cx.run_until_idle();
    }
}

impl AppContext for BenchWindowContext {
    fn new<T: 'static>(&mut self, build_entity: impl FnOnce(&mut Context<T>) -> T) -> Entity<T> {
        self.window
            .update(&mut self.cx, |_, _, cx| cx.new(build_entity))
            .expect("benchmark window was unexpectedly closed")
    }

    fn reserve_entity<T: 'static>(&mut self) -> Reservation<T> {
        self.cx.reserve_entity()
    }

    fn insert_entity<T: 'static>(
        &mut self,
        reservation: Reservation<T>,
        build_entity: impl FnOnce(&mut Context<T>) -> T,
    ) -> Entity<T> {
        self.window
            .update(&mut self.cx, |_, _, cx| {
                cx.insert_entity(reservation, build_entity)
            })
            .expect("benchmark window was unexpectedly closed")
    }

    fn update_entity<T: 'static, R>(
        &mut self,
        handle: &Entity<T>,
        update: impl FnOnce(&mut T, &mut Context<T>) -> R,
    ) -> R {
        self.cx.update_entity(handle, update)
    }

    fn as_mut<'a, T>(&'a mut self, handle: &Entity<T>) -> GpuiBorrow<'a, T>
    where
        T: 'static,
    {
        self.cx.as_mut(handle)
    }

    fn read_entity<T, R>(&self, handle: &Entity<T>, read: impl FnOnce(&T, &App) -> R) -> R
    where
        T: 'static,
    {
        self.cx.read_entity(handle, read)
    }

    fn update_window<T, F>(&mut self, window: AnyWindowHandle, update: F) -> Result<T>
    where
        F: FnOnce(AnyView, &mut Window, &mut App) -> T,
    {
        self.cx.update_window(window, update)
    }

    fn with_window<R>(
        &mut self,
        entity_id: EntityId,
        update: impl FnOnce(&mut Window, &mut App) -> R,
    ) -> Option<R> {
        self.cx.with_window(entity_id, update)
    }

    fn read_window<T, R>(
        &self,
        window: &WindowHandle<T>,
        read: impl FnOnce(Entity<T>, &App) -> R,
    ) -> Result<R>
    where
        T: 'static,
    {
        self.cx.read_window(window, read)
    }

    fn background_spawn<R>(&self, future: impl Future<Output = R> + Send + 'static) -> Task<R>
    where
        R: Send + 'static,
    {
        self.cx.background_spawn(future)
    }

    fn read_global<G, R>(&self, callback: impl FnOnce(&G, &App) -> R) -> R
    where
        G: Global,
    {
        self.cx.read_global(callback)
    }
}

impl VisualContext for BenchWindowContext {
    type Result<T> = Result<T>;

    fn window_handle(&self) -> AnyWindowHandle {
        self.window
    }

    fn update_window_entity<T: 'static, R>(
        &mut self,
        entity: &Entity<T>,
        update: impl FnOnce(&mut T, &mut Window, &mut Context<T>) -> R,
    ) -> Result<R> {
        let entity = entity.clone();
        self.cx
            .app
            .borrow_mut()
            .with_window(entity.entity_id(), |window, app| {
                entity.update(app, |entity, cx| update(entity, window, cx))
            })
            .ok_or_else(|| {
                anyhow!("entity has no current window; use `update` instead of `update_in`")
            })
    }

    fn new_window_entity<T: 'static>(
        &mut self,
        build_entity: impl FnOnce(&mut Window, &mut Context<T>) -> T,
    ) -> Result<Entity<T>> {
        self.window.update(&mut self.cx, |_, window, cx| {
            cx.new(|cx| build_entity(window, cx))
        })
    }

    fn replace_root_view<V>(
        &mut self,
        build_view: impl FnOnce(&mut Window, &mut Context<V>) -> V,
    ) -> Result<Entity<V>>
    where
        V: 'static + Render,
    {
        self.window.update(&mut self.cx, |_, window, cx| {
            window.replace_root(cx, build_view)
        })
    }

    fn focus<V>(&mut self, entity: &Entity<V>) -> Result<()>
    where
        V: Focusable,
    {
        self.window.update(&mut self.cx, |_, window, cx| {
            entity.read(cx).focus_handle(cx).focus(window, cx)
        })
    }
}

#[cfg(all(test, feature = "test-support"))]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    use futures::channel::oneshot;
    use std::time::Instant;

    use super::*;

    #[test]
    fn threaded_context_completes_background_to_main_handoff() {
        let cx = BenchAppContext::threaded(Some("threaded_context_handoff"));
        let background = cx.background_executor().clone();
        let foreground = cx.foreground_executor().clone();
        let completed = Arc::new(AtomicBool::new(false));
        let (sender, receiver) = oneshot::channel();

        foreground
            .spawn({
                let completed = completed.clone();
                async move {
                    receiver.await.expect("background sender should complete");
                    completed.store(true, Ordering::SeqCst);
                }
            })
            .detach();
        background
            .spawn(async move { sender.send(()).expect("foreground receiver should live") })
            .detach();

        cx.run_until_idle();
        assert!(completed.load(Ordering::SeqCst));
        cx.teardown();
    }

    #[test]
    fn threaded_context_runs_exp_008_seed_matrix() {
        const SEEDS: usize = 100;
        let mut cx = BenchAppContext::threaded(Some("exp_008_seed_matrix"));
        let background = cx.background_executor().clone();
        let foreground = cx.foreground_executor().clone();
        let mut elapsed_samples = Vec::with_capacity(SEEDS);

        for seed in 0..SEEDS {
            let mut window = cx.add_empty_window();
            let started_at = Instant::now();
            let task_count = seed % 4 + 1;
            let completed_tasks = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            for _ in 0..task_count {
                let timer = background.timer(std::time::Duration::ZERO);
                background
                    .spawn({
                        let completed_tasks = completed_tasks.clone();
                        async move {
                            std::thread::sleep(std::time::Duration::from_millis(10));
                            timer.await;
                            completed_tasks.fetch_add(1, Ordering::SeqCst);
                        }
                    })
                    .detach();
            }

            let handoff_completed = Arc::new(AtomicBool::new(false));
            let (sender, receiver) = oneshot::channel();
            foreground
                .spawn({
                    let handoff_completed = handoff_completed.clone();
                    async move {
                        receiver.await.expect("background sender should complete");
                        handoff_completed.store(true, Ordering::SeqCst);
                    }
                })
                .detach();
            background
                .spawn(async move { sender.send(()).expect("foreground receiver should live") })
                .detach();

            let canceled_task = background.spawn(std::future::pending::<()>());
            drop(canceled_task);

            cx.run_until_idle();
            assert_eq!(completed_tasks.load(Ordering::SeqCst), task_count);
            assert!(handoff_completed.load(Ordering::SeqCst));
            window.update(|window, _| window.remove_window());
            window.run_until_idle();
            elapsed_samples.push(started_at.elapsed().as_secs_f64());
        }

        let measured = &elapsed_samples[10..];
        let mean = measured.iter().sum::<f64>() / measured.len() as f64;
        let variance = measured
            .iter()
            .map(|sample| (sample - mean).powi(2))
            .sum::<f64>()
            / measured.len() as f64;
        let coefficient_of_variation = variance.sqrt() / mean;
        println!(
            "GPUI_EXP_008 seeds={SEEDS} mean_seconds={mean:.9} cv={coefficient_of_variation:.6}"
        );
        cx.teardown();
    }
}
