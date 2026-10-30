use std::{mem, path::PathBuf, sync::Arc, time::Duration};

use audio::PlaybackHandle;
use editor::RevealInFileManager;
use gpui::{
    App, AppContext, Bounds, Context, Entity, EventEmitter, FocusHandle, Focusable, Image, Pixels,
    ScrollDelta, ScrollWheelEvent, SharedString, Subscription, Task, WeakEntity, Window,
};
use i18n::tr;
use project::{Project, ProjectPath};
use settings::Settings;
use util::rel_path::RelPath;

use crate::{
    AudioItem, SeekBackward, SeekForward, SeekToEnd, SeekToStart, Stop, ToggleMute, TogglePlay,
    lyrics::{Lyrics, lyrics_path_for_audio, read_lyrics},
    player::{self, AudioMetadata},
    scrubber::{Scrub, ScrubKind},
    waveform::{self, WAVEFORM_ANALYSIS_LIMIT, WAVEFORM_BUCKETS},
    waveform_cache::{CachedWaveform, WaveformCache},
};

const SEEK_STEP: Duration = Duration::from_secs(5);
const POSITION_TICK: Duration = Duration::from_millis(100);
const POSITION_NOTIFY_QUANTUM: Duration = Duration::from_millis(200);
const VOLUME_STEP: f32 = 0.05;
const VOLUME_PERSIST_DEBOUNCE: Duration = Duration::from_millis(500);
pub(crate) const SPEED_OPTIONS: [f32; 6] = [0.5, 0.75, 1.0, 1.25, 1.5, 2.0];
const MIN_SPEED: f32 = 0.25;
const MAX_SPEED: f32 = 4.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlaybackStatus {
    Stopped,
    Playing,
    Paused,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PlaybackEndAction {
    Restart,
    StopAt(Option<Duration>),
}

pub(crate) enum LoadState {
    Loading,
    Loaded(Box<LoadedAudio>),
    Error(SharedString),
}

pub(crate) struct LoadedAudio {
    generation: u64,
    pub path: PathBuf,
    pub format_hint: String,
    pub metadata: AudioMetadata,
    pub cover: Option<Arc<Image>>,
    pub peaks: Option<Arc<[(f32, f32)]>>,
    pub waveform_duration: Option<Duration>,
    pub analyzing: bool,
    /// Keeps a downloaded remote file alive for as long as it is being played.
    _staged_file: Option<tempfile::TempPath>,
}

pub struct AudioView {
    pub(crate) audio_item: Entity<AudioItem>,
    pub(crate) project: Entity<Project>,
    pub(crate) focus_handle: FocusHandle,
    pub(crate) load_state: LoadState,
    pub(crate) playback: PlaybackStatus,
    pub(crate) position: Duration,
    last_notified_position: Duration,
    pub(crate) volume: f32,
    pub(crate) speed: f32,
    pub(crate) repeat: bool,
    pub(crate) lyrics: Option<Arc<Lyrics>>,
    pub(crate) lyrics_visible: bool,
    pub(crate) muted: bool,
    pub(crate) volume_before_mute: f32,
    pub(crate) handle: Option<PlaybackHandle>,
    pub(crate) playback_error: Option<SharedString>,
    pub(crate) waveform_bounds: Option<Bounds<Pixels>>,
    pub(crate) seek_track_bounds: Option<Bounds<Pixels>>,
    pub(crate) volume_track_bounds: Option<Bounds<Pixels>>,
    pub(crate) scrub: Option<Scrub>,
    load_generation: u64,
    _load_task: Task<()>,
    _waveform_task: Option<Task<()>>,
    _position_task: Option<Task<()>>,
    volume_persist_task: Option<Task<()>>,
    _worktree_subscription: Subscription,
}

pub enum AudioViewEvent {
    TitleChanged,
    MetadataChanged,
}

impl EventEmitter<AudioViewEvent> for AudioView {}
impl EventEmitter<()> for AudioView {}

impl Focusable for AudioView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl AudioView {
    pub fn new(
        audio_item: Entity<AudioItem>,
        project: Entity<Project>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.on_release(|this, _cx| {
            if let Some(handle) = this.handle.take() {
                handle.stop();
            }
        })
        .detach();

        let worktree_subscription = cx.subscribe(&project, Self::on_project_event);
        let volume = audio::AudioSettings::get_global(cx).preview_volume;

        let mut this = Self {
            audio_item,
            project,
            focus_handle: cx.focus_handle(),
            load_state: LoadState::Loading,
            playback: PlaybackStatus::Stopped,
            position: Duration::ZERO,
            last_notified_position: Duration::ZERO,
            volume,
            speed: 1.0,
            repeat: false,
            lyrics: None,
            lyrics_visible: false,
            muted: false,
            volume_before_mute: volume.max(0.01),
            handle: None,
            playback_error: None,
            waveform_bounds: None,
            seek_track_bounds: None,
            volume_track_bounds: None,
            scrub: None,
            load_generation: 0,
            _load_task: Task::ready(()),
            _waveform_task: None,
            _position_task: None,
            volume_persist_task: None,
            _worktree_subscription: worktree_subscription,
        };
        this.start_load(cx);
        this
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
        let item = self.audio_item.read(cx);
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
        self.scrub = None;
        self.stop_playback(cx);
        self.start_load(cx);
    }

    fn start_load(&mut self, cx: &mut Context<Self>) {
        let generation = advance_load_generation(&mut self.load_generation);
        drop(self._waveform_task.take());
        drop(mem::replace(&mut self._load_task, Task::ready(())));

        self._load_task = cx.spawn(async move |this, cx| {
            let prepared = this.update(cx, |view, cx| {
                let item = view.audio_item.read(cx);
                let format_hint = item.path.extension().map(ToOwned::to_owned);
                (
                    format_hint,
                    item.project_path(),
                    view.abs_path(cx),
                    view.project.clone(),
                )
            });

            let (format_hint, project_path, local_path, project) = match prepared {
                Ok(prepared) => prepared,
                Err(error) => {
                    Self::set_error(&this, generation, error.to_string(), cx);
                    return;
                }
            };

            let lyrics_path = local_path
                .as_deref()
                .map(lyrics_path_for_audio);
            let lyrics_task = lyrics_path.map(|path| {
                cx.background_spawn(async move { read_lyrics(&path) })
            });

            let Some(format_hint) = format_hint else {
                let message = cx.update(|cx| {
                    tr(
                        cx,
                        "audio_viewer.error.missing_extension",
                        "Audio file has no extension",
                    )
                });
                Self::set_error(&this, generation, message, cx);
                return;
            };

            let (path, staged_file) = if let Some(local_path) = local_path {
                (local_path, None)
            } else {
                let staged = project
                    .update(cx, |project, cx| {
                        project.stage_file_to_temp(
                            project_path.worktree_id,
                            project_path.path.clone(),
                            cx,
                        )
                    })
                    .await;
                match staged {
                    Ok((path, temp_path)) => (path, Some(temp_path)),
                    Err(error) => {
                        Self::set_error(&this, generation, error.to_string(), cx);
                        return;
                    }
                }
            };

            let info_task = cx.background_spawn({
                let path = path.clone();
                let format_hint = format_hint.clone();
                async move { player::read_audio(&path, &format_hint) }
            });
            let lyrics_result = match lyrics_task {
                Some(task) => task.await,
                None => None,
            };
            let info = info_task.await;

            match info {
                Ok(info) => {
                    let Some(this) = this.upgrade() else {
                        return;
                    };
                    this.update(cx, |view, cx| {
                        if !load_result_is_current(view.load_generation, generation) {
                            log::debug!(
                                "discarding stale audio load result (generation {generation}, current {})",
                                view.load_generation
                            );
                            return;
                        }
                        view.lyrics = lyrics_result;
                        view.load_state = LoadState::Loaded(Box::new(LoadedAudio {
                            generation,
                            path,
                            format_hint,
                            metadata: info.metadata,
                            cover: info.cover,
                            peaks: None,
                            waveform_duration: None,
                            analyzing: true,
                            _staged_file: staged_file,
                        }));
                        view.start_waveform(cx);
                        cx.emit(AudioViewEvent::TitleChanged);
                        cx.emit(AudioViewEvent::MetadataChanged);
                        cx.notify();
                    });
                }
                Err(error) => {
                    let Some(this) = this.upgrade() else {
                        return;
                    };
                    this.update(cx, |view, cx| {
                        if !load_result_is_current(view.load_generation, generation) {
                            log::debug!(
                                "discarding stale audio load error (generation {generation}, current {})",
                                view.load_generation
                            );
                            return;
                        }
                        view.lyrics = None;
                        if matches!(view.load_state, LoadState::Loaded(_)) {
                            log::error!("reloading audio preview: {error:#}");
                        } else {
                            view.load_state = LoadState::Error(error.to_string().into());
                            cx.notify();
                        }
                    });
                }
            }
        });
    }

    fn set_error(
        this: &WeakEntity<Self>,
        generation: u64,
        message: impl Into<SharedString>,
        cx: &mut gpui::AsyncApp,
    ) {
        let message = message.into();
        let Some(this) = this.upgrade() else {
            return;
        };
        this.update(cx, |view, cx| {
            if !load_result_is_current(view.load_generation, generation) {
                log::debug!(
                    "discarding stale audio load error (generation {generation}, current {})",
                    view.load_generation
                );
                return;
            }
            view.lyrics = None;
            view.load_state = LoadState::Error(message);
            cx.notify();
        });
    }

    fn start_waveform(&mut self, cx: &mut Context<Self>) {
        let LoadState::Loaded(loaded) = &self.load_state else {
            return;
        };
        let path = loaded.path.clone();
        let format_hint = loaded.format_hint.clone();
        let generation = loaded.generation;
        let waveform_cache = WaveformCache::default_cache();

        drop(self._waveform_task.take());
        self._waveform_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let cache_key = waveform_cache.key_for_path(
                        &path,
                        WAVEFORM_BUCKETS,
                        WAVEFORM_ANALYSIS_LIMIT,
                    );
                    if let Some(cached) =
                        cache_key.as_ref().and_then(|key| waveform_cache.load(key))
                    {
                        return anyhow::Ok(cached);
                    }

                    let decode_permit = waveform_cache.acquire().await;
                    if let Some(cached) =
                        cache_key.as_ref().and_then(|key| waveform_cache.load(key))
                    {
                        return anyhow::Ok(cached);
                    }

                    let decoder = player::open_decoder(&path, &format_hint)?;
                    let waveform = waveform::extract_waveform_peaks(
                        decoder,
                        WAVEFORM_BUCKETS,
                        WAVEFORM_ANALYSIS_LIMIT,
                    );

                    if let Some(cache_key) = cache_key.as_ref()
                        && let Err(error) = waveform_cache.store(cache_key, &waveform)
                    {
                        log::warn!("could not cache audio waveform: {error:#}");
                    }
                    drop(decode_permit);

                    anyhow::Ok(CachedWaveform::from_peaks(&waveform))
                })
                .await;

            let Some(this) = this.upgrade() else {
                return;
            };
            this.update(cx, |view, cx| {
                if !load_result_is_current(view.load_generation, generation) {
                    log::debug!(
                        "discarding stale audio waveform (generation {generation}, current {})",
                        view.load_generation
                    );
                    return;
                }
                if let LoadState::Loaded(loaded) = &mut view.load_state {
                    loaded.analyzing = false;
                    match result {
                        Ok(waveform) => {
                            log::debug!(
                                "audio waveform ready with {} samples",
                                waveform.sample_count
                            );
                            loaded.peaks = Some(waveform.peaks);
                            loaded.waveform_duration = Some(waveform.duration);
                            if waveform.reached_end && !waveform.duration.is_zero() {
                                loaded.metadata.duration = Some(waveform.duration);
                            }
                        }
                        Err(error) => log::error!("analyzing audio waveform: {error:#}"),
                    }
                }
                cx.emit(AudioViewEvent::MetadataChanged);
                cx.notify();
            });
        }));
    }

    pub(crate) fn loaded(&self) -> Option<&LoadedAudio> {
        match &self.load_state {
            LoadState::Loaded(state) => Some(state),
            _ => None,
        }
    }

    pub(crate) fn duration(&self) -> Option<Duration> {
        self.loaded().and_then(|loaded| loaded.metadata.duration)
    }

    pub(crate) fn can_seek(&self) -> bool {
        seek_is_available(self.duration())
    }

    pub(crate) fn cover(&self) -> Option<Arc<Image>> {
        self.loaded().and_then(|loaded| loaded.cover.clone())
    }

    pub(crate) fn tag_title(&self) -> Option<&str> {
        self.loaded()
            .and_then(|loaded| loaded.metadata.title.as_deref())
    }

    pub(crate) fn tag_artist(&self) -> Option<&str> {
        self.loaded()
            .and_then(|loaded| loaded.metadata.artist.as_deref())
    }

    pub(crate) fn tag_album(&self) -> Option<&str> {
        self.loaded()
            .and_then(|loaded| loaded.metadata.album.as_deref())
    }

    pub(crate) fn is_playing(&self) -> bool {
        self.playback == PlaybackStatus::Playing
    }

    pub(crate) fn effective_volume(&self) -> f32 {
        if self.muted { 0.0 } else { self.volume }
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
        self.audio_item.read(cx).path.file_name().map_or_else(
            || tr(cx, "audio_viewer.file_name.fallback", "Audio"),
            |name| name.to_owned(),
        )
    }

    pub(crate) fn abs_path(&self, cx: &App) -> Option<PathBuf> {
        let item = self.audio_item.read(cx);
        let worktree = self
            .project
            .read(cx)
            .worktree_for_id(item.worktree_id, cx)?;
        let local = worktree.read(cx).as_local()?;
        Some(local.abs_path().join(item.path.as_std_path()))
    }

    pub(crate) fn project_path(&self, cx: &App) -> ProjectPath {
        self.audio_item.read(cx).project_path()
    }

    pub(crate) fn relative_path(&self, cx: &App) -> Arc<RelPath> {
        self.audio_item.read(cx).path.clone()
    }

    pub(crate) fn metadata_tooltip(&self, cx: &App) -> String {
        let path_style = self.project.read(cx).path_style(cx);
        let mut lines = vec![self.relative_path(cx).display(path_style).into_owned()];
        if let Some(title) = self.tag_title() {
            lines.push(title.to_string());
        }
        let artist_album = [self.tag_artist(), self.tag_album()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        if !artist_album.is_empty() {
            lines.push(artist_album.join(" • "));
        }
        if let Some(parts) = self.metadata_parts(cx) {
            lines.push(parts.join(" • "));
        }
        if let Some(loaded) = self.loaded() {
            lines.extend(metadata_detail_lines(&loaded.metadata, cx));
        }
        let output_format = tr(cx, "audio_viewer.output_format", "Output: {} kHz, {}")
            .replacen("{}", &format_sample_rate_value(audio::SAMPLE_RATE.get()), 1)
            .replacen("{}", &format_channels(audio::CHANNEL_COUNT.get(), cx), 1);
        lines.push(output_format);
        lines.join("\n")
    }

    pub(crate) fn metadata_parts(&self, cx: &App) -> Option<Vec<String>> {
        let loaded = self.loaded()?;
        let mut parts = Vec::new();
        parts.push(format_sample_rate(loaded.metadata.sample_rate));
        if let Some(bit_depth) = loaded.metadata.bit_depth {
            parts.push(format_bit_depth(bit_depth));
        }
        parts.push(format_channels(loaded.metadata.channels, cx));
        if let Some(bitrate) = loaded.metadata.bitrate {
            parts.push(format_bitrate(bitrate));
        }
        parts.push(loaded.metadata.format_label.to_string());
        if let Some(duration) = loaded.metadata.duration {
            parts.push(format_timestamp(duration));
        }
        parts.push(util::size::format_file_size(
            loaded.metadata.file_size,
            false,
        ));
        Some(parts)
    }

    pub(crate) fn current_time_label(&self) -> String {
        let current = format_timestamp(self.position);
        match self.duration() {
            Some(duration) => format!("{current} / {}", format_timestamp(duration)),
            None => current,
        }
    }

    fn output_volume(&self) -> f32 {
        gain_for_fader(self.effective_volume())
    }

    fn persist_volume(&mut self, cx: &mut Context<Self>) {
        let volume = self.volume;
        let fs = self.project.read(cx).fs().clone();
        settings::update_settings_file(fs, cx, move |settings, _| {
            settings.audio.get_or_insert_default().preview_volume = Some(volume);
        });
    }

    fn persist_volume_debounced(&mut self, cx: &mut Context<Self>) {
        if let Some(task) = self.volume_persist_task.take() {
            drop(task);
        }

        let volume = self.volume;
        let fs = self.project.read(cx).fs().clone();
        self.volume_persist_task = Some(cx.spawn(async move |_this, cx| {
            cx.background_executor()
                .timer(VOLUME_PERSIST_DEBOUNCE)
                .await;
            cx.update(|cx| {
                settings::update_settings_file(fs, cx, move |settings, _| {
                    settings.audio.get_or_insert_default().preview_volume = Some(volume);
                });
            });
        }));
    }

    fn start_position_tick(&mut self, cx: &mut Context<Self>) {
        self._position_task = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(POSITION_TICK).await;
                let Ok(keep_ticking) = this.update(cx, |view, cx| {
                    if view.sync_playback_position(cx) {
                        cx.notify();
                    }
                    view.playback == PlaybackStatus::Playing
                }) else {
                    break;
                };
                if !keep_ticking {
                    break;
                }
            }
        }));
    }

    fn sync_playback_position(&mut self, cx: &mut Context<Self>) -> bool {
        let previous_playback = self.playback;
        let Some(handle) = &self.handle else {
            self.playback = PlaybackStatus::Stopped;
            return previous_playback != self.playback;
        };
        let finished = handle.is_finished();
        let current_position = handle.position();
        if !finished && self.scrub.is_none() {
            self.position = current_position;
        }

        if finished {
            if let Some(handle) = self.handle.take() {
                handle.stop();
            }
            match playback_end_action(self.repeat, self.duration()) {
                PlaybackEndAction::Restart => {
                    self.playback = PlaybackStatus::Stopped;
                    self.position = Duration::ZERO;
                    self.last_notified_position = Duration::ZERO;
                    self.play_loaded_from(Duration::ZERO, cx);
                }
                PlaybackEndAction::StopAt(position) => {
                    self.playback = PlaybackStatus::Stopped;
                    if self.scrub.is_none()
                        && let Some(position) = position
                    {
                        self.position = position;
                    }
                }
            }
        }

        let playback_changed = self.playback != previous_playback;
        let position_changed = self.scrub.is_none()
            && position_visible_change(self.last_notified_position, self.position);
        let should_notify = finished || playback_changed || position_changed;
        if should_notify {
            self.last_notified_position = self.position;
        }
        should_notify
    }

    pub(crate) fn stop_playback(&mut self, cx: &mut Context<Self>) {
        if let Some(handle) = self.handle.take() {
            handle.stop();
        }
        self.scrub = None;
        self.reset_stopped_state();
        cx.notify();
    }

    fn reset_stopped_state(&mut self) {
        self.playback = PlaybackStatus::Stopped;
        self.playback_error = None;
        self.position = Duration::ZERO;
        self.last_notified_position = Duration::ZERO;
        self._position_task = None;
    }

    fn clamp_position(&self, position: Duration) -> Duration {
        match self.duration() {
            Some(duration) => position.min(duration),
            None => position,
        }
    }

    pub(crate) fn seek_to(&mut self, position: Duration, cx: &mut Context<Self>) {
        let position = self.clamp_position(position);
        self.position = position;
        self.last_notified_position = position;
        if let Some(handle) = &self.handle
            && let Err(error) = handle.try_seek(position)
        {
            log::warn!("seeking audio preview: {error:#}");
        }
        cx.notify();
    }

    fn play_loaded_from(&mut self, start: Duration, cx: &mut Context<Self>) -> bool {
        let Some(loaded) = self.loaded() else {
            return false;
        };
        let path = loaded.path.clone();
        let format_hint = loaded.format_hint.clone();
        let volume = self.output_volume();

        match player::play_path(&path, &format_hint, start, volume, cx) {
            Ok(handle) => {
                handle.set_speed(self.speed);
                self.handle = Some(handle);
                self.playback = PlaybackStatus::Playing;
                self.playback_error = None;
                true
            }
            Err(error) => {
                log::error!("playing audio preview: {error:#}");
                self.playback = PlaybackStatus::Stopped;
                self.playback_error = Some(
                    tr(
                        cx,
                        "audio_viewer.error.playback",
                        "Could not open audio output",
                    )
                    .into(),
                );
                false
            }
        }
    }

    fn start_playback(&mut self, cx: &mut Context<Self>) {
        let Some(loaded) = self.loaded() else {
            return;
        };
        let start = playback_start_position(self.position, loaded.metadata.duration);
        if start != self.position {
            self.position = start;
            self.last_notified_position = start;
        }
        if self.play_loaded_from(start, cx) {
            self.start_position_tick(cx);
        }
        cx.notify();
    }

    pub(crate) fn toggle_play(
        &mut self,
        _: &TogglePlay,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.playback {
            PlaybackStatus::Playing => {
                if let Some(handle) = &self.handle {
                    handle.pause();
                }
                self.playback = PlaybackStatus::Paused;
                self._position_task = None;
                cx.notify();
            }
            PlaybackStatus::Paused => {
                if let Some(handle) = &self.handle
                    && !handle.is_finished()
                {
                    handle.set_volume(self.output_volume());
                    handle.resume();
                    self.playback = PlaybackStatus::Playing;
                    self.start_position_tick(cx);
                    cx.notify();
                    return;
                }
                self.start_playback(cx);
            }
            PlaybackStatus::Stopped => self.start_playback(cx),
        }
    }

    pub(crate) fn stop(&mut self, _: &Stop, _window: &mut Window, cx: &mut Context<Self>) {
        self.stop_playback(cx);
    }

    pub(crate) fn toggle_repeat(&mut self, cx: &mut Context<Self>) {
        self.repeat = !self.repeat;
        cx.notify();
    }

    pub(crate) fn toggle_lyrics(&mut self, cx: &mut Context<Self>) {
        self.lyrics_visible = !self.lyrics_visible;
        cx.notify();
    }

    pub(crate) fn retry_playback(
        &mut self,
        _: &gpui::ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_playback(cx);
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
        if let Some(handle) = &self.handle {
            handle.set_volume(self.output_volume());
        }
        self.persist_volume(cx);
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

    pub(crate) fn begin_scrub(
        &mut self,
        kind: ScrubKind,
        bounds: Bounds<Pixels>,
        position: gpui::Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        if kind == ScrubKind::Seek && !self.can_seek() {
            return;
        }
        let mut scrub = Scrub::new(kind, bounds);
        self.update_scrub_state(&mut scrub, position, kind);
        self.scrub = Some(scrub);
        cx.notify();
    }

    pub(crate) fn update_scrub(&mut self, position: gpui::Point<Pixels>, cx: &mut Context<Self>) {
        let Some(mut scrub) = self.scrub.take() else {
            return;
        };
        let kind = scrub.kind;
        self.update_scrub_state(&mut scrub, position, kind);
        self.scrub = Some(scrub);
        cx.notify();
    }

    fn update_scrub_state(
        &mut self,
        scrub: &mut Scrub,
        position: gpui::Point<Pixels>,
        kind: ScrubKind,
    ) {
        match kind {
            ScrubKind::Seek => {
                let duration = self.duration();
                scrub.update(position, duration);
                if let Some(position) = scrub.pending_position() {
                    self.position = position;
                }
            }
            ScrubKind::Volume => {
                let ratio = crate::scrubber::ratio_from_position(position, scrub.bounds);
                self.set_volume_from_ratio(ratio);
            }
        }
    }

    pub(crate) fn end_scrub(&mut self, cx: &mut Context<Self>) {
        let Some(scrub) = self.scrub.take() else {
            return;
        };
        match scrub.kind {
            ScrubKind::Seek => {
                if let Some(position) = scrub.pending_position() {
                    self.seek_to(position, cx);
                    return;
                }
            }
            ScrubKind::Volume => self.persist_volume(cx),
        }
        cx.notify();
    }

    pub(crate) fn scroll_volume(
        &mut self,
        event: &ScrollWheelEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let delta = match event.delta {
            ScrollDelta::Pixels(pixels) => f32::from(pixels.y),
            ScrollDelta::Lines(lines) => lines.y,
        };
        if delta == 0.0 {
            return;
        }

        let direction = delta.signum();
        let volume = (self.volume + direction * VOLUME_STEP).clamp(0.0, 1.0);
        self.set_volume_from_ratio(volume);
        self.persist_volume_debounced(cx);
        cx.notify();
    }

    fn set_volume_from_ratio(&mut self, ratio: f32) {
        self.volume = ratio.clamp(0.0, 1.0);
        self.muted = false;
        if let Some(handle) = &self.handle {
            handle.set_volume(self.output_volume());
        }
    }

    pub(crate) fn set_speed(&mut self, speed: f32, cx: &mut Context<Self>) {
        self.speed = if speed.is_finite() {
            speed.clamp(MIN_SPEED, MAX_SPEED)
        } else {
            1.0
        };
        if let Some(handle) = &self.handle {
            handle.set_speed(self.speed);
        }
        cx.notify();
    }

    pub(crate) fn speed_label(&self, cx: &App) -> String {
        tr(cx, "audio_viewer.speed.value", "{}×").replacen("{}", &format_speed_value(self.speed), 1)
    }
}

pub(crate) fn format_speed_value(speed: f32) -> String {
    let text = format!("{speed:.2}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text.contains('.') {
        text.to_owned()
    } else {
        format!("{text}.0")
    }
}

fn gain_for_fader(fader: f32) -> f32 {
    fader.clamp(0.0, 1.0).powi(3)
}

fn advance_load_generation(generation: &mut u64) -> u64 {
    *generation = generation.wrapping_add(1);
    *generation
}

fn load_result_is_current(current_generation: u64, result_generation: u64) -> bool {
    current_generation == result_generation
}

fn playback_start_position(position: Duration, duration: Option<Duration>) -> Duration {
    match duration {
        Some(duration) if position >= duration => Duration::ZERO,
        _ => position,
    }
}

fn playback_end_action(repeat: bool, duration: Option<Duration>) -> PlaybackEndAction {
    if repeat {
        PlaybackEndAction::Restart
    } else {
        PlaybackEndAction::StopAt(duration)
    }
}

fn seek_is_available(duration: Option<Duration>) -> bool {
    duration.is_some_and(|duration| !duration.is_zero())
}

fn metadata_detail_lines(metadata: &AudioMetadata, cx: &App) -> Vec<String> {
    let mut lines = Vec::new();
    let year_genre = [
        metadata.year.map(|year| year.to_string()),
        metadata.genre.clone(),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    if !year_genre.is_empty() {
        lines.push(year_genre.join(" • "));
    }
    if let Some((track, total)) = metadata.track {
        lines.push(format_numbered_metadata(
            "audio_viewer.metadata.track",
            "Track {}",
            track,
            total,
            cx,
        ));
    }
    if let Some((disc, total)) = metadata.disc {
        lines.push(format_numbered_metadata(
            "audio_viewer.metadata.disc",
            "Disc {}",
            disc,
            total,
            cx,
        ));
    }
    lines
}

fn format_numbered_metadata(
    key: &str,
    fallback: &str,
    number: u32,
    total: Option<u32>,
    cx: &App,
) -> String {
    let number = match total {
        Some(total) => format!("{number}/{total}"),
        None => number.to_string(),
    };
    tr(cx, key, fallback).replacen("{}", &number, 1)
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

fn position_visible_change(previous: Duration, current: Duration) -> bool {
    current.abs_diff(previous) >= POSITION_NOTIFY_QUANTUM
}

fn format_bitrate(bitrate: u32) -> String {
    format!("{bitrate} kbps")
}

fn format_bit_depth(bit_depth: u8) -> String {
    format!("{bit_depth}-bit")
}

fn format_sample_rate(sample_rate: u32) -> String {
    format!("{} kHz", format_sample_rate_value(sample_rate))
}

fn format_sample_rate_value(sample_rate: u32) -> String {
    if sample_rate.is_multiple_of(1000) {
        (sample_rate / 1000).to_string()
    } else {
        format!("{:.1}", sample_rate as f64 / 1000.0)
    }
}

fn format_channels(channels: u16, cx: &App) -> String {
    match channels {
        1 => tr(cx, "audio_viewer.channels.mono", "Mono"),
        2 => tr(cx, "audio_viewer.channels.stereo", "Stereo"),
        count => tr(cx, "audio_viewer.channels.count", "{} channels").replacen(
            "{}",
            &count.to_string(),
            1,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        PlaybackEndAction, advance_load_generation, format_bit_depth, format_bitrate,
        format_sample_rate, format_speed_value, format_timestamp, gain_for_fader,
        load_result_is_current, playback_end_action, playback_start_position,
        position_visible_change, seek_is_available,
    };
    use std::time::Duration;

    #[test]
    fn format_bitrate_and_bit_depth() {
        assert_eq!(format_bitrate(320), "320 kbps");
        assert_eq!(format_bit_depth(16), "16-bit");
        assert_eq!(format_bit_depth(24), "24-bit");
    }

    #[test]
    fn sample_rate_formatting_separates_value_and_unit() {
        assert_eq!(format_sample_rate(48_000), "48 kHz");
        assert_eq!(format_sample_rate(44_100), "44.1 kHz");
    }

    #[test]
    fn seek_is_only_available_for_positive_known_durations() {
        assert!(!seek_is_available(None));
        assert!(!seek_is_available(Some(Duration::ZERO)));
        assert!(seek_is_available(Some(Duration::from_millis(1))));
    }

    #[test]
    fn speed_label_always_has_one_decimal_place() {
        assert_eq!(format_speed_value(0.5), "0.5");
        assert_eq!(format_speed_value(0.75), "0.75");
        assert_eq!(format_speed_value(1.0), "1.0");
        assert_eq!(format_speed_value(1.25), "1.25");
        assert_eq!(format_speed_value(2.0), "2.0");
    }

    #[test]
    fn format_timestamp_minutes_and_seconds() {
        assert_eq!(format_timestamp(Duration::from_secs(0)), "0:00");
        assert_eq!(format_timestamp(Duration::from_secs(5)), "0:05");
        assert_eq!(format_timestamp(Duration::from_secs(65)), "1:05");
        assert_eq!(format_timestamp(Duration::from_secs(3599)), "59:59");
    }

    #[test]
    fn format_timestamp_hours() {
        assert_eq!(format_timestamp(Duration::from_secs(3600)), "1:00:00");
        assert_eq!(format_timestamp(Duration::from_secs(3661)), "1:01:01");
    }

    #[test]
    fn position_notification_uses_visible_quantum() {
        assert!(!position_visible_change(Duration::ZERO, Duration::ZERO));
        assert!(!position_visible_change(
            Duration::ZERO,
            Duration::from_millis(199)
        ));
        assert!(position_visible_change(
            Duration::ZERO,
            Duration::from_millis(200)
        ));
        assert!(position_visible_change(
            Duration::ZERO,
            Duration::from_millis(201)
        ));
        assert!(position_visible_change(
            Duration::from_millis(201),
            Duration::ZERO
        ));
    }

    #[test]
    fn fader_uses_perceptual_gain_curve_and_clamps() {
        assert_eq!(gain_for_fader(0.0), 0.0);
        assert_eq!(gain_for_fader(0.5), 0.125);
        assert_eq!(gain_for_fader(1.0), 1.0);
        assert_eq!(gain_for_fader(-0.5), 0.0);
        assert_eq!(gain_for_fader(1.5), 1.0);
    }

    #[test]
    fn repeat_restarts_and_non_repeat_stops_at_the_end() {
        let duration = Some(Duration::from_secs(3));
        assert_eq!(
            playback_end_action(true, duration),
            PlaybackEndAction::Restart
        );
        assert_eq!(
            playback_end_action(false, duration),
            PlaybackEndAction::StopAt(duration)
        );
        assert_eq!(
            playback_end_action(false, None),
            PlaybackEndAction::StopAt(None)
        );
    }

    #[test]
    fn playback_retry_preserves_position_until_end() {
        let duration = Some(Duration::from_secs(60));
        assert_eq!(
            playback_start_position(Duration::from_secs(42), duration),
            Duration::from_secs(42)
        );
        assert_eq!(
            playback_start_position(Duration::from_secs(60), duration),
            Duration::ZERO
        );
        assert_eq!(
            playback_start_position(Duration::from_secs(42), None),
            Duration::from_secs(42)
        );
    }

    #[test]
    fn load_generation_rejects_results_from_previous_load() {
        let mut current_generation = 0;
        let first_generation = advance_load_generation(&mut current_generation);
        assert!(load_result_is_current(current_generation, first_generation));

        let second_generation = advance_load_generation(&mut current_generation);
        assert!(!load_result_is_current(
            current_generation,
            first_generation
        ));
        assert!(load_result_is_current(
            current_generation,
            second_generation
        ));
    }
}
