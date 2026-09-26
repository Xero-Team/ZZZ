use std::{
    collections::VecDeque,
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use anyhow::Context as _;
use audio::Audio;
use editor::RevealInFileManager;
use gpui::{
    App, AppContext, Bounds, Context, Entity, EventEmitter, FocusHandle, Focusable, Pixels,
    RenderImage, SharedString, Subscription, Task, WeakEntity, Window,
};
use i18n::tr;
use project::{Project, ProjectPath};
use rodio::{Decoder, Player};
use util::rel_path::RelPath;
use video::{VideoDecoder, VideoFrame, VideoMetadata};

use crate::{
    DecreaseSpeed, DecreaseVolume, IncreaseSpeed, IncreaseVolume, ResetSpeed, SeekBackward,
    SeekForward, SeekToEnd, SeekToStart, StepBackward, StepForward, Stop, ToggleLoop, ToggleMute,
    TogglePlay, VideoItem,
};

const SEEK_STEP: Duration = Duration::from_secs(5);
const MIN_TICK: Duration = Duration::from_millis(4);
const MAX_TICK: Duration = Duration::from_millis(16);
/// How many decoded frames the pump keeps ready ahead of the playhead.
const PREFETCH_FRAMES: usize = 6;
/// How long the audio track may stop advancing before the clock falls back to
/// the wall clock (for example when the audio track is shorter than the video).
const AUDIO_STALL_TIMEOUT: Duration = Duration::from_millis(500);
/// Quietest and loudest playback rates the speed controls allow.
pub(crate) const MIN_SPEED: f32 = 0.25;
pub(crate) const MAX_SPEED: f32 = 4.0;
const SPEED_STEP: f32 = 0.25;
/// Audio above `1.0` is amplified rather than clipped, so the volume track can
/// boost quiet tracks up to this factor.
pub(crate) const MAX_VOLUME: f32 = 2.0;
const VOLUME_STEP: f32 = 0.05;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlaybackStatus {
    Stopped,
    Playing,
    Paused,
}

pub(crate) enum LoadState {
    Loading,
    Loaded(Box<LoadedVideo>),
    Error(SharedString),
}

pub(crate) struct LoadedVideo {
    pub path: PathBuf,
    pub extension: String,
    pub metadata: VideoMetadata,
    pub frame_interval: Duration,
}

/// Media-time playback clock.
///
/// While an audio track is playing it is the clock master, so video follows the
/// audio device and the two cannot drift apart. Otherwise the clock advances
/// from a wall-clock anchor scaled by the playback speed. If audio stops
/// advancing (for example a track shorter than the video), the clock falls back
/// to the wall clock instead of stalling the video.
struct PlaybackClock {
    anchor: Duration,
    running_since: Option<Instant>,
    speed: f32,
    last_audio_position: Duration,
    last_audio_progress: Option<Instant>,
    audio_stalled: bool,
}

impl PlaybackClock {
    fn new() -> Self {
        Self {
            anchor: Duration::ZERO,
            running_since: None,
            speed: 1.0,
            last_audio_position: Duration::ZERO,
            last_audio_progress: None,
            audio_stalled: false,
        }
    }

    fn reset(&mut self, position: Duration, running: bool) {
        self.anchor = position;
        self.running_since = running.then(Instant::now);
        self.last_audio_position = position;
        self.last_audio_progress = running.then(Instant::now);
        self.audio_stalled = false;
    }

    fn set_speed(&mut self, speed: f32, position: Duration, running: bool) {
        self.speed = speed;
        self.reset(position, running);
    }

    fn position(&mut self, audio: Option<&Player>, duration: Option<Duration>) -> Duration {
        let Some(running_since) = self.running_since else {
            return clamp_to(duration, self.anchor);
        };

        if let Some(player) = audio
            && !player.is_paused()
            && !player.empty()
            && !self.audio_stalled
        {
            let audio_position = player.get_pos();
            if audio_position > self.last_audio_position {
                self.last_audio_position = audio_position;
                self.last_audio_progress = Some(Instant::now());
            } else if self
                .last_audio_progress
                .is_some_and(|last| last.elapsed() > AUDIO_STALL_TIMEOUT)
            {
                self.audio_stalled = true;
            }
            if !self.audio_stalled {
                return clamp_to(duration, audio_position);
            }
        }

        let elapsed = running_since.elapsed().mul_f32(self.speed);
        clamp_to(duration, self.anchor.saturating_add(elapsed))
    }
}

fn clamp_to(duration: Option<Duration>, position: Duration) -> Duration {
    match duration {
        Some(duration) => position.min(duration),
        None => position,
    }
}

pub struct VideoView {
    pub(crate) video_item: Entity<VideoItem>,
    pub(crate) project: Entity<Project>,
    pub(crate) focus_handle: FocusHandle,
    pub(crate) load_state: LoadState,
    pub(crate) playback: PlaybackStatus,
    pub(crate) position: Duration,
    pub(crate) volume: f32,
    pub(crate) muted: bool,
    pub(crate) volume_before_mute: f32,
    pub(crate) speed: f32,
    pub(crate) looping: bool,
    pub(crate) playback_error: Option<SharedString>,
    pub(crate) current_frame: Option<Arc<RenderImage>>,
    pub(crate) seek_track_bounds: Option<Bounds<Pixels>>,
    pub(crate) volume_track_bounds: Option<Bounds<Pixels>>,
    pub(crate) dragging_seek: bool,
    pub(crate) dragging_volume: bool,
    pub(crate) audio: Option<Arc<Player>>,
    clock: PlaybackClock,
    decoder: Option<Arc<Mutex<VideoDecoder>>>,
    generation: u64,
    _load_task: Task<()>,
    _pump_task: Option<Task<()>>,
    _worktree_subscription: Subscription,
}

pub enum VideoViewEvent {
    TitleChanged,
}

impl EventEmitter<VideoViewEvent> for VideoView {}
impl EventEmitter<()> for VideoView {}

impl Focusable for VideoView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl VideoView {
    pub fn new(
        video_item: Entity<VideoItem>,
        project: Entity<Project>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.on_release(|this, _cx| {
            if let Some(player) = this.audio.take() {
                player.stop();
            }
        })
        .detach();

        let load_task = Self::start_load(cx);
        let worktree_subscription = cx.subscribe(&project, Self::on_project_event);

        Self {
            video_item,
            project,
            focus_handle: cx.focus_handle(),
            load_state: LoadState::Loading,
            playback: PlaybackStatus::Stopped,
            position: Duration::ZERO,
            volume: 1.0,
            muted: false,
            volume_before_mute: 1.0,
            speed: 1.0,
            looping: false,
            playback_error: None,
            current_frame: None,
            seek_track_bounds: None,
            volume_track_bounds: None,
            dragging_seek: false,
            dragging_volume: false,
            audio: None,
            clock: PlaybackClock::new(),
            decoder: None,
            generation: 0,
            _load_task: load_task,
            _pump_task: None,
            _worktree_subscription: worktree_subscription,
        }
    }

    fn on_project_event(
        &mut self,
        _: Entity<Project>,
        event: &project::Event,
        cx: &mut Context<Self>,
    ) {
        let project::Event::WorktreeUpdatedEntries(worktree_id, changes) = event else {
            return;
        };
        let item = self.video_item.read(cx);
        if item.worktree_id != *worktree_id {
            return;
        }
        let path = item.path.clone();
        if changes
            .iter()
            .any(|(changed_path, _, _)| changed_path.as_ref() == path.as_ref())
        {
            self.reload(cx);
        }
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        self.stop_playback(cx);
        self.decoder = None;
        self.current_frame = None;
        self.playback_error = None;
        self.load_state = LoadState::Loading;
        self._load_task = Self::start_load(cx);
        cx.notify();
    }

    /// Decode and display the first frame so an opened video has a poster.
    fn show_first_frame(&mut self, cx: &mut Context<Self>) {
        let Some(decoder) = self.decoder.clone() else {
            return;
        };
        let generation = self.generation;
        self._pump_task = Some(cx.spawn(async move |this, cx| {
            let decoded = cx
                .background_spawn(async move { next_decoder_frame(&decoder) })
                .await;
            match decoded {
                Ok(Some(frame)) => {
                    this.update(cx, |view, cx| {
                        if view.generation == generation && view.playback == PlaybackStatus::Stopped
                        {
                            view.present_frame(frame, cx);
                        }
                    })
                    .ok();
                }
                Ok(None) => {}
                Err(error) => log::warn!("decoding video poster frame: {error:#}"),
            }
        }));
    }

    fn start_load(cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            let prepared = this.update(cx, |view, cx| {
                let extension = view
                    .video_item
                    .read(cx)
                    .path
                    .extension()
                    .map(ToOwned::to_owned);
                (view.abs_path(cx), extension)
            });

            let (path, extension) = match prepared {
                Ok((Some(path), Some(extension))) => (path, extension),
                Ok((None, _)) => {
                    this.update(cx, |view, cx| {
                        view.load_state = LoadState::Error(
                            tr(
                                cx,
                                "video_viewer.error.not_local",
                                "Video file is not available locally",
                            )
                            .into(),
                        );
                        cx.notify();
                    })
                    .ok();
                    return;
                }
                Ok((_, None)) => {
                    this.update(cx, |view, cx| {
                        view.load_state = LoadState::Error(
                            tr(
                                cx,
                                "video_viewer.error.missing_extension",
                                "Video file has no extension",
                            )
                            .into(),
                        );
                        cx.notify();
                    })
                    .ok();
                    return;
                }
                Err(error) => {
                    Self::set_error(&this, error.to_string(), cx);
                    return;
                }
            };

            let opened = cx
                .background_spawn({
                    let path = path.clone();
                    async move { VideoDecoder::open(&path) }
                })
                .await;

            match opened {
                Ok(decoder) => {
                    this.update(cx, |view, cx| {
                        let metadata = decoder.metadata().clone();
                        let frame_interval = decoder.frame_interval();
                        view.decoder = Some(Arc::new(Mutex::new(decoder)));
                        view.load_state = LoadState::Loaded(Box::new(LoadedVideo {
                            path,
                            extension,
                            metadata,
                            frame_interval,
                        }));
                        view.playback = PlaybackStatus::Stopped;
                        view.position = Duration::ZERO;
                        view.playback_error = None;
                        view.clock.reset(Duration::ZERO, false);
                        view.show_first_frame(cx);
                        cx.emit(VideoViewEvent::TitleChanged);
                        cx.notify();
                    })
                    .ok();
                }
                Err(error) => {
                    Self::set_error(&this, error.to_string(), cx);
                }
            }
        })
    }

    fn set_error(
        this: &WeakEntity<Self>,
        message: impl Into<SharedString>,
        cx: &mut gpui::AsyncApp,
    ) {
        let message = message.into();
        this.update(cx, |view, cx| {
            view.load_state = LoadState::Error(message);
            cx.notify();
        })
        .ok();
    }

    pub(crate) fn loaded(&self) -> Option<&LoadedVideo> {
        match &self.load_state {
            LoadState::Loaded(state) => Some(state),
            _ => None,
        }
    }

    pub(crate) fn duration(&self) -> Option<Duration> {
        self.loaded().and_then(|loaded| loaded.metadata.duration)
    }

    pub(crate) fn is_playing(&self) -> bool {
        self.playback == PlaybackStatus::Playing
    }

    pub(crate) fn is_looping(&self) -> bool {
        self.looping
    }

    pub(crate) fn effective_volume(&self) -> f32 {
        if self.muted { 0.0 } else { self.volume }
    }

    pub(crate) fn speed_label(&self) -> String {
        format_speed(self.speed)
    }

    pub(crate) fn playhead_ratio(&self) -> f32 {
        let Some(duration) = self.duration() else {
            return 0.0;
        };
        let total = duration.as_secs_f32();
        if total <= 0.0 {
            0.0
        } else {
            (self.position.as_secs_f32() / total).clamp(0.0, 1.0)
        }
    }

    pub(crate) fn file_name(&self, cx: &App) -> String {
        self.video_item
            .read(cx)
            .path
            .file_name()
            .map(|name| name.to_owned())
            .unwrap_or_else(|| tr(cx, "video_viewer.file_name.fallback", "Video"))
    }

    pub(crate) fn abs_path(&self, cx: &App) -> Option<PathBuf> {
        let item = self.video_item.read(cx);
        let worktree = self
            .project
            .read(cx)
            .worktree_for_id(item.worktree_id, cx)?;
        let local = worktree.read(cx).as_local()?;
        Some(local.abs_path().join(item.path.as_std_path()))
    }

    pub(crate) fn project_path(&self, cx: &App) -> ProjectPath {
        self.video_item.read(cx).project_path()
    }

    pub(crate) fn relative_path(&self, cx: &App) -> Arc<RelPath> {
        self.video_item.read(cx).path.clone()
    }

    pub(crate) fn metadata_tooltip(&self, cx: &App) -> String {
        let path_style = self.project.read(cx).path_style(cx);
        let mut lines = vec![self.relative_path(cx).display(path_style).into_owned()];
        if let Some(parts) = self.metadata_parts(cx) {
            lines.push(parts.join(" • "));
        }
        lines.join("\n")
    }

    pub(crate) fn metadata_parts(&self, cx: &App) -> Option<Vec<String>> {
        let loaded = self.loaded()?;
        let metadata = &loaded.metadata;
        let mut parts = vec![metadata.resolution_label(), metadata.video_codec.clone()];
        if let Some(frame_rate) = metadata.frame_rate {
            parts.push(format!("{frame_rate:.0} fps"));
        }
        if metadata.has_audio {
            parts.push(
                metadata
                    .audio_codec
                    .clone()
                    .unwrap_or_else(|| tr(cx, "video_viewer.audio", "Audio")),
            );
        }
        parts.push(metadata.format_label.to_string());
        if let Some(duration) = metadata.duration {
            parts.push(format_timestamp(duration));
        }
        parts.push(util::size::format_file_size(metadata.file_size, false));
        Some(parts)
    }

    pub(crate) fn current_time_label(&self) -> String {
        let current = format_timestamp(self.position);
        match self.duration() {
            Some(duration) => format!("{current} / {}", format_timestamp(duration)),
            None => current,
        }
    }

    fn clamp_position(&self, position: Duration) -> Duration {
        clamp_to(self.duration(), position)
    }

    fn clock_position(&mut self) -> Duration {
        let duration = self.duration();
        self.clock.position(self.audio.as_deref(), duration)
    }

    pub(crate) fn stop_playback(&mut self, cx: &mut Context<Self>) {
        self.generation = self.generation.wrapping_add(1);
        if let Some(player) = self.audio.take() {
            player.stop();
        }
        self.clock.reset(Duration::ZERO, false);
        self.playback = PlaybackStatus::Stopped;
        self.position = Duration::ZERO;
        cx.notify();
    }

    fn start_playback(&mut self, cx: &mut Context<Self>) {
        if self.loaded().is_none() {
            return;
        }
        if let Some(duration) = self.duration()
            && self.position >= duration
        {
            self.position = Duration::ZERO;
        }

        let start = self.position;
        self.playback_error = None;
        self.start_audio(start, cx);
        self.clock.reset(start, true);
        self.playback = PlaybackStatus::Playing;
        self.start_pump(cx, Some(start));
        cx.notify();
    }

    /// Build a controllable audio player positioned at `start`, if the file has
    /// a decodable audio track.
    fn start_audio(&mut self, start: Duration, cx: &mut Context<Self>) {
        if let Some(previous) = self.audio.take() {
            previous.stop();
        }

        let (path, extension, has_audio) = match self.loaded() {
            Some(loaded) => (
                loaded.path.clone(),
                loaded.extension.clone(),
                loaded.metadata.has_audio,
            ),
            None => return,
        };

        match start_audio(
            &path,
            &extension,
            start,
            self.effective_volume(),
            self.speed,
            cx,
        ) {
            Ok(player) => self.audio = Some(player),
            Err(error) => {
                log::debug!("video preview has no playable audio track: {error:#}");
                if has_audio {
                    self.playback_error = Some(
                        tr(
                            cx,
                            "video_viewer.error.audio",
                            "The audio track could not be played",
                        )
                        .into(),
                    );
                }
            }
        }
    }

    fn start_pump(&mut self, cx: &mut Context<Self>, seek: Option<Duration>) {
        let Some(decoder) = self.decoder.clone() else {
            return;
        };
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        let frame_interval = self
            .loaded()
            .map(|loaded| loaded.frame_interval)
            .unwrap_or(Duration::from_millis(33));
        let tick = frame_interval.clamp(MIN_TICK, MAX_TICK);

        self._pump_task = Some(cx.spawn(async move |this, cx| {
            if let Some(target) = seek {
                let result = cx
                    .background_spawn({
                        let decoder = decoder.clone();
                        async move { seek_decoder(&decoder, target) }
                    })
                    .await;
                if let Err(error) = result {
                    log::warn!("seeking video decoder: {error:#}");
                }
            }

            let mut buffered: VecDeque<VideoFrame> = VecDeque::new();
            loop {
                let Ok((playback, current_generation)) =
                    this.update(cx, |view, _| (view.playback, view.generation))
                else {
                    break;
                };
                if playback != PlaybackStatus::Playing || current_generation != generation {
                    break;
                }

                let Ok(target) = this.update(cx, |view, _| {
                    let position = view.clock_position();
                    view.position = position;
                    position
                }) else {
                    break;
                };

                if !refill_frames(cx, &decoder, &mut buffered, generation, &this).await {
                    break;
                }

                if buffered.is_empty() {
                    let looping = this.update(cx, |view, _| view.looping).unwrap_or(false);
                    if looping {
                        this.update(cx, |view, cx| view.restart_loop(cx)).ok();
                    } else {
                        this.update(cx, |view, cx| view.finish_playback(cx)).ok();
                    }
                    break;
                }

                // Drop frames the playhead has already passed so slow
                // decoders catch up instead of falling further behind.
                while buffered.len() > 1 {
                    let Some(front) = buffered.front() else {
                        break;
                    };
                    if front.pts().saturating_add(frame_interval) <= target {
                        buffered.pop_front();
                    } else {
                        break;
                    }
                }

                let Some(front) = buffered.front() else {
                    continue;
                };
                if front.pts() <= target {
                    let frame = buffered.pop_front().expect("buffer is not empty");
                    this.update(cx, |view, cx| view.present_frame(frame, cx))
                        .ok();
                    cx.background_executor().timer(tick).await;
                } else {
                    let wait = front.pts().saturating_sub(target).clamp(MIN_TICK, tick);
                    cx.background_executor().timer(wait).await;
                }
            }
        }));
    }

    fn restart_loop(&mut self, cx: &mut Context<Self>) {
        self.position = Duration::ZERO;
        self.start_audio(Duration::ZERO, cx);
        self.clock.reset(Duration::ZERO, true);
        self.playback = PlaybackStatus::Playing;
        self.start_pump(cx, Some(Duration::ZERO));
        cx.notify();
    }

    fn finish_playback(&mut self, cx: &mut Context<Self>) {
        self.playback = PlaybackStatus::Stopped;
        self.clock.reset(self.position, false);
        if let Some(duration) = self.duration() {
            self.position = duration;
        }
        if let Some(player) = self.audio.take() {
            player.stop();
        }
        cx.notify();
    }

    fn fail_playback(&mut self, message: String, cx: &mut Context<Self>) {
        log::error!("video playback: {message}");
        self.playback = PlaybackStatus::Stopped;
        self.clock.reset(self.position, false);
        self.playback_error = Some(message.into());
        if let Some(player) = self.audio.take() {
            player.stop();
        }
        cx.notify();
    }

    fn present_frame(&mut self, frame: VideoFrame, cx: &mut Context<Self>) {
        self.current_frame = Some(Arc::new(render_image_from_frame(frame)));
        cx.notify();
    }

    pub(crate) fn toggle_play(
        &mut self,
        _: &TogglePlay,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.playback {
            PlaybackStatus::Playing => self.pause_playback(cx),
            PlaybackStatus::Paused => {
                if let Some(player) = &self.audio
                    && !player.empty()
                {
                    player.set_volume(self.effective_volume());
                    player.set_speed(self.speed);
                    player.play();
                    self.clock.reset(self.position, true);
                    self.playback = PlaybackStatus::Playing;
                    // The decoder runs ahead of the playhead, so re-seek to the
                    // paused position instead of continuing from its cursor.
                    self.start_pump(cx, Some(self.position));
                    cx.notify();
                    return;
                }
                self.start_playback(cx);
            }
            PlaybackStatus::Stopped => self.start_playback(cx),
        }
    }

    fn pause_playback(&mut self, cx: &mut Context<Self>) {
        self.position = self.clock_position();
        self.clock.reset(self.position, false);
        if let Some(player) = &self.audio {
            player.pause();
        }
        self.playback = PlaybackStatus::Paused;
        self.generation = self.generation.wrapping_add(1);
        cx.notify();
    }

    pub(crate) fn stop(&mut self, _: &Stop, _window: &mut Window, cx: &mut Context<Self>) {
        self.stop_playback(cx);
    }

    pub(crate) fn seek_forward(
        &mut self,
        _: &SeekForward,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.seek_to(self.position.saturating_add(SEEK_STEP), cx);
    }

    pub(crate) fn seek_backward(
        &mut self,
        _: &SeekBackward,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.seek_to(self.position.saturating_sub(SEEK_STEP), cx);
    }

    pub(crate) fn seek_to_start(
        &mut self,
        _: &SeekToStart,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.seek_to(Duration::ZERO, cx);
    }

    pub(crate) fn seek_to_end(
        &mut self,
        _: &SeekToEnd,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(duration) = self.duration() {
            self.seek_to(duration, cx);
        }
    }

    /// Commit a seek to the current playhead, used when a scrub gesture ends.
    pub(crate) fn seek_to_position(&mut self, cx: &mut Context<Self>) {
        let position = self.position;
        self.seek_to(position, cx);
    }

    fn seek_to(&mut self, position: Duration, cx: &mut Context<Self>) {
        let position = self.clamp_position(position);
        self.position = position;
        self.clock
            .reset(position, self.playback == PlaybackStatus::Playing);
        if let Some(player) = &self.audio
            && let Err(error) = player.try_seek(position)
        {
            log::debug!("seeking video audio: {error}");
        }
        if self.playback == PlaybackStatus::Playing {
            self.start_pump(cx, Some(position));
        } else {
            self.preview_seek_frame(position, cx);
        }
        cx.notify();
    }

    fn preview_seek_frame(&mut self, target: Duration, cx: &mut Context<Self>) {
        let Some(decoder) = self.decoder.clone() else {
            return;
        };
        self._pump_task = Some(cx.spawn(async move |this, cx| {
            let decoded = cx
                .background_spawn(async move {
                    seek_decoder(&decoder, target)?;
                    next_decoder_frame(&decoder)
                })
                .await;
            match decoded {
                Ok(Some(frame)) => {
                    this.update(cx, |view, cx| view.present_frame(frame, cx))
                        .ok();
                }
                Ok(None) => {}
                Err(error) => log::warn!("previewing video frame: {error:#}"),
            }
        }));
    }

    pub(crate) fn step_forward(
        &mut self,
        _: &StepForward,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.step_frame(1, cx);
    }

    pub(crate) fn step_backward(
        &mut self,
        _: &StepBackward,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.step_frame(-1, cx);
    }

    fn step_frame(&mut self, direction: i64, cx: &mut Context<Self>) {
        if self.playback == PlaybackStatus::Playing {
            self.pause_playback(cx);
        }
        let interval = self
            .loaded()
            .map(|loaded| loaded.frame_interval)
            .unwrap_or(Duration::from_millis(33));
        let target = if direction >= 0 {
            self.position.saturating_add(interval)
        } else {
            self.position.saturating_sub(interval)
        };
        self.seek_to(target, cx);
    }

    pub(crate) fn toggle_mute(
        &mut self,
        _: &ToggleMute,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.muted {
            self.muted = false;
            self.volume = self.volume_before_mute;
        } else {
            self.volume_before_mute = self.volume.max(0.01);
            self.muted = true;
        }
        if let Some(player) = &self.audio {
            player.set_volume(self.effective_volume());
        }
        cx.notify();
    }

    pub(crate) fn increase_volume(
        &mut self,
        _: &IncreaseVolume,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_volume(self.volume + VOLUME_STEP, cx);
    }

    pub(crate) fn decrease_volume(
        &mut self,
        _: &DecreaseVolume,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_volume(self.volume - VOLUME_STEP, cx);
    }

    fn set_volume(&mut self, volume: f32, cx: &mut Context<Self>) {
        self.volume = volume.clamp(0.0, MAX_VOLUME);
        self.muted = self.volume <= 0.0;
        if let Some(player) = &self.audio {
            player.set_volume(self.effective_volume());
        }
        cx.notify();
    }

    pub(crate) fn increase_speed(
        &mut self,
        _: &IncreaseSpeed,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_speed(self.speed + SPEED_STEP, cx);
    }

    pub(crate) fn decrease_speed(
        &mut self,
        _: &DecreaseSpeed,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_speed(self.speed - SPEED_STEP, cx);
    }

    pub(crate) fn reset_speed(
        &mut self,
        _: &ResetSpeed,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_speed(1.0, cx);
    }

    fn set_speed(&mut self, speed: f32, cx: &mut Context<Self>) {
        let speed = speed.clamp(MIN_SPEED, MAX_SPEED);
        if (speed - self.speed).abs() < f32::EPSILON {
            return;
        }
        let position = self.clock_position();
        self.position = position;
        self.speed = speed;
        self.clock
            .set_speed(speed, position, self.playback == PlaybackStatus::Playing);
        if let Some(player) = &self.audio {
            player.set_speed(speed);
        }
        cx.notify();
    }

    pub(crate) fn toggle_loop(
        &mut self,
        _: &ToggleLoop,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.looping = !self.looping;
        cx.notify();
    }

    pub(crate) fn reveal_in_file_manager(
        &mut self,
        _: &RevealInFileManager,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(path) = self.abs_path(cx) {
            self.project
                .update(cx, |project, cx| project.reveal_path(&path, cx));
        }
    }

    pub(crate) fn seek_from_ratio(&mut self, ratio: f32, cx: &mut Context<Self>) {
        let Some(duration) = self.duration() else {
            return;
        };
        self.scrub_to(position_from_ratio(duration, ratio), cx);
    }

    /// Update the playhead while dragging without paying for a decoder seek.
    fn scrub_to(&mut self, position: Duration, cx: &mut Context<Self>) {
        let position = self.clamp_position(position);
        self.position = position;
        self.clock
            .reset(position, self.playback == PlaybackStatus::Playing);
        if let Some(player) = &self.audio
            && let Err(error) = player.try_seek(position)
        {
            log::debug!("scrubbing video audio: {error}");
        }
        cx.notify();
    }

    pub(crate) fn set_volume_from_ratio(&mut self, ratio: f32, cx: &mut Context<Self>) {
        self.set_volume(ratio.clamp(0.0, 1.0) * MAX_VOLUME, cx);
    }

    pub(crate) fn ratio_from_position(
        position: gpui::Point<Pixels>,
        bounds: Bounds<Pixels>,
    ) -> f32 {
        let x = f32::from(position.x - bounds.origin.x);
        let width = f32::from(bounds.size.width).max(1.0);
        (x / width).clamp(0.0, 1.0)
    }
}

async fn refill_frames(
    cx: &mut gpui::AsyncApp,
    decoder: &Arc<Mutex<VideoDecoder>>,
    buffered: &mut VecDeque<VideoFrame>,
    generation: u64,
    view: &WeakEntity<VideoView>,
) -> bool {
    while buffered.len() < PREFETCH_FRAMES {
        let decoded = cx
            .background_spawn({
                let decoder = decoder.clone();
                async move { next_decoder_frame(&decoder) }
            })
            .await;
        match decoded {
            Ok(Some(frame)) => buffered.push_back(frame),
            Ok(None) => break,
            Err(error) => {
                let message = error.to_string();
                view.update(cx, |view, cx| {
                    if view.generation == generation {
                        view.fail_playback(message, cx);
                    }
                })
                .ok();
                return false;
            }
        }
    }
    true
}

fn next_decoder_frame(decoder: &Arc<Mutex<VideoDecoder>>) -> anyhow::Result<Option<VideoFrame>> {
    let mut decoder = decoder
        .lock()
        .map_err(|_| anyhow::anyhow!("video decoder lock poisoned"))?;
    decoder.next_frame()
}

fn seek_decoder(decoder: &Arc<Mutex<VideoDecoder>>, target: Duration) -> anyhow::Result<()> {
    let mut decoder = decoder
        .lock()
        .map_err(|_| anyhow::anyhow!("video decoder lock poisoned"))?;
    decoder.seek(target)
}

fn render_image_from_frame(frame: VideoFrame) -> RenderImage {
    let width = frame.width();
    let height = frame.height();
    let bgra = frame.into_bgra();
    let buffer = image::RgbaImage::from_raw(width, height, bgra).unwrap_or_else(|| {
        log::warn!("video frame buffer did not match {width}x{height}");
        image::RgbaImage::new(width, height)
    });
    RenderImage::new(vec![image::Frame::new(buffer)])
}

fn start_audio(
    path: &Path,
    extension: &str,
    start: Duration,
    volume: f32,
    speed: f32,
    cx: &mut App,
) -> anyhow::Result<Arc<Player>> {
    let file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let byte_len = file.metadata()?.len();
    let decoder = Decoder::builder()
        .with_data(BufReader::new(file))
        .with_byte_len(byte_len)
        .with_seekable(true)
        .with_hint(extension)
        .build()
        .map_err(|error| anyhow::anyhow!(error))
        .context("decoding audio")?;

    let player = Audio::play_source_player(decoder, cx)?;
    player.set_volume(volume);
    player.set_speed(speed);
    if start > Duration::ZERO
        && let Err(error) = player.try_seek(start)
    {
        log::debug!("seeking video audio: {error}");
    }
    Ok(player)
}

fn position_from_ratio(duration: Duration, ratio: f32) -> Duration {
    Duration::from_secs_f64(duration.as_secs_f64() * ratio.clamp(0.0, 1.0) as f64)
}

fn format_speed(speed: f32) -> String {
    if speed.fract().abs() < f32::EPSILON {
        format!("{}x", speed as u32)
    } else {
        let text = format!("{speed:.2}");
        format!("{}x", text.trim_end_matches('0').trim_end_matches('.'))
    }
}

pub(crate) fn format_timestamp(duration: Duration) -> String {
    let total_seconds = duration.as_secs();
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::{PlaybackClock, format_speed, format_timestamp, position_from_ratio};
    use std::time::Duration;

    #[test]
    fn format_timestamp_minutes_and_hours() {
        assert_eq!(format_timestamp(Duration::from_secs(0)), "0:00");
        assert_eq!(format_timestamp(Duration::from_secs(65)), "1:05");
        assert_eq!(format_timestamp(Duration::from_secs(3661)), "1:01:01");
    }

    #[test]
    fn position_from_ratio_maps_seconds() {
        let duration = Duration::from_secs(200);
        assert_eq!(position_from_ratio(duration, 0.5), Duration::from_secs(100));
        assert_eq!(position_from_ratio(duration, 2.0), duration);
    }

    #[test]
    fn clock_is_paused_until_reset_running() {
        let mut clock = PlaybackClock::new();
        clock.reset(Duration::from_secs(10), false);
        assert_eq!(clock.position(None, None), Duration::from_secs(10));
    }

    #[test]
    fn clock_clamps_to_duration() {
        let mut clock = PlaybackClock::new();
        clock.reset(Duration::from_secs(10), false);
        assert_eq!(
            clock.position(None, Some(Duration::from_secs(5))),
            Duration::from_secs(5)
        );
    }

    #[test]
    fn format_speed_trims_trailing_zeros() {
        assert_eq!(format_speed(1.0), "1x");
        assert_eq!(format_speed(2.0), "2x");
        assert_eq!(format_speed(1.5), "1.5x");
        assert_eq!(format_speed(0.25), "0.25x");
    }
}
