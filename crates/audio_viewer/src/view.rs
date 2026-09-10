use std::{path::PathBuf, sync::Arc, time::Duration};

use audio::PlaybackHandle;
use editor::RevealInFileManager;
use gpui::{
    App, AppContext, Bounds, Context, Entity, EventEmitter, FocusHandle, Focusable, Image, Pixels,
    SharedString, Subscription, Task, WeakEntity, Window,
};
use i18n::tr;
use project::{Project, ProjectPath};
use util::rel_path::RelPath;

use crate::{
    AudioItem, SeekBackward, SeekForward, SeekToEnd, SeekToStart, Stop, ToggleMute, TogglePlay,
    player::{self, AudioMetadata},
    waveform::{self, WAVEFORM_ANALYSIS_LIMIT, WAVEFORM_BUCKETS},
};

const SEEK_STEP: Duration = Duration::from_secs(5);
const POSITION_TICK: Duration = Duration::from_millis(50);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlaybackStatus {
    Stopped,
    Playing,
    Paused,
}

pub(crate) enum LoadState {
    Loading,
    Loaded(Box<LoadedAudio>),
    Error(SharedString),
}

pub(crate) struct LoadedAudio {
    pub path: PathBuf,
    pub format_hint: String,
    pub metadata: AudioMetadata,
    pub cover: Option<Arc<Image>>,
    pub peaks: Option<Arc<[(f32, f32)]>>,
    pub waveform_duration: Option<Duration>,
    pub analyzing: bool,
}

pub struct AudioView {
    pub(crate) audio_item: Entity<AudioItem>,
    pub(crate) project: Entity<Project>,
    pub(crate) focus_handle: FocusHandle,
    pub(crate) load_state: LoadState,
    pub(crate) playback: PlaybackStatus,
    pub(crate) position: Duration,
    pub(crate) volume: f32,
    pub(crate) muted: bool,
    pub(crate) volume_before_mute: f32,
    pub(crate) handle: Option<PlaybackHandle>,
    pub(crate) playback_error: Option<SharedString>,
    pub(crate) waveform_bounds: Option<Bounds<Pixels>>,
    pub(crate) seek_track_bounds: Option<Bounds<Pixels>>,
    pub(crate) volume_track_bounds: Option<Bounds<Pixels>>,
    pub(crate) dragging_seek: bool,
    pub(crate) dragging_volume: bool,
    _load_task: Task<()>,
    _waveform_task: Option<Task<()>>,
    _position_task: Option<Task<()>>,
    _worktree_subscription: Subscription,
}

pub enum AudioViewEvent {
    TitleChanged,
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

        let load_task = Self::start_load(cx);
        let worktree_subscription = cx.subscribe(&project, Self::on_project_event);

        Self {
            audio_item,
            project,
            focus_handle: cx.focus_handle(),
            load_state: LoadState::Loading,
            playback: PlaybackStatus::Stopped,
            position: Duration::ZERO,
            volume: 1.0,
            muted: false,
            volume_before_mute: 1.0,
            handle: None,
            playback_error: None,
            waveform_bounds: None,
            seek_track_bounds: None,
            volume_track_bounds: None,
            dragging_seek: false,
            dragging_volume: false,
            _load_task: load_task,
            _waveform_task: None,
            _position_task: None,
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
        self.stop_playback(cx);
        self._load_task = Self::start_load(cx);
    }

    fn start_load(cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            let prepared = this.update(cx, |view, cx| {
                let format_hint = view
                    .audio_item
                    .read(cx)
                    .path
                    .extension()
                    .map(ToOwned::to_owned);
                (view.abs_path(cx), format_hint)
            });

            let (path, format_hint) = match prepared {
                Ok((Some(path), Some(format_hint))) => (path, format_hint),
                Ok((None, _)) => {
                    this.update(cx, |view, cx| {
                        view.load_state = LoadState::Error(
                            tr(
                                cx,
                                "audio_viewer.error.not_local",
                                "Audio file is not available locally",
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
                                "audio_viewer.error.missing_extension",
                                "Audio file has no extension",
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

            let info = cx
                .background_spawn({
                    let path = path.clone();
                    let format_hint = format_hint.clone();
                    async move { player::read_audio(&path, &format_hint) }
                })
                .await;

            match info {
                Ok(info) => {
                    this.update(cx, |view, cx| {
                        view.load_state = LoadState::Loaded(Box::new(LoadedAudio {
                            path,
                            format_hint,
                            metadata: info.metadata,
                            cover: info.cover,
                            peaks: None,
                            waveform_duration: None,
                            analyzing: true,
                        }));
                        view.start_waveform(cx);
                        cx.emit(AudioViewEvent::TitleChanged);
                        cx.notify();
                    })
                    .ok();
                }
                Err(error) => {
                    let keep_existing = this
                        .update(cx, |view, _cx| {
                            matches!(view.load_state, LoadState::Loaded(_))
                        })
                        .unwrap_or(false);
                    if keep_existing {
                        log::error!("reloading audio preview: {error:#}");
                    } else {
                        Self::set_error(&this, error.to_string(), cx);
                    }
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

    fn start_waveform(&mut self, cx: &mut Context<Self>) {
        let LoadState::Loaded(loaded) = &self.load_state else {
            return;
        };
        let path = loaded.path.clone();
        let format_hint = loaded.format_hint.clone();

        self._waveform_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let decoder = player::open_decoder(&path, &format_hint)?;
                    anyhow::Ok(waveform::extract_waveform_peaks(
                        decoder,
                        WAVEFORM_BUCKETS,
                        WAVEFORM_ANALYSIS_LIMIT,
                    ))
                })
                .await;

            this.update(cx, |view, cx| {
                if let LoadState::Loaded(loaded) = &mut view.load_state {
                    loaded.analyzing = false;
                    match result {
                        Ok(waveform) => {
                            loaded.peaks = Some(Arc::from(waveform.peaks));
                            loaded.waveform_duration = Some(waveform.duration);
                            if waveform.reached_end && !waveform.duration.is_zero() {
                                loaded.metadata.duration = Some(waveform.duration);
                            }
                        }
                        Err(error) => log::error!("analyzing audio waveform: {error:#}"),
                    }
                }
                cx.notify();
            })
            .ok();
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
        self.audio_item
            .read(cx)
            .path
            .file_name()
            .map(|name| name.to_owned())
            .unwrap_or_else(|| tr(cx, "audio_viewer.file_name.fallback", "Audio"))
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
        self.effective_volume()
    }

    fn start_position_tick(&mut self, cx: &mut Context<Self>) {
        self._position_task = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(POSITION_TICK).await;
                if this
                    .update(cx, |view, cx| {
                        view.sync_playback_position(cx);
                    })
                    .is_err()
                {
                    break;
                }
            }
        }));
    }

    fn sync_playback_position(&mut self, cx: &mut Context<Self>) {
        let Some(handle) = &self.handle else {
            return;
        };
        self.position = handle.position();
        if handle.is_finished() {
            self.handle = None;
            self.playback = PlaybackStatus::Stopped;
            if let Some(duration) = self.duration() {
                self.position = duration;
            }
            self._position_task = None;
        }
        cx.notify();
    }

    pub(crate) fn stop_playback(&mut self, cx: &mut Context<Self>) {
        if let Some(handle) = self.handle.take() {
            handle.stop();
        }
        self.playback = PlaybackStatus::Stopped;
        self.position = Duration::ZERO;
        self._position_task = None;
        cx.notify();
    }

    fn clamp_position(&self, position: Duration) -> Duration {
        match self.duration() {
            Some(duration) => position.min(duration),
            None => position,
        }
    }

    fn seek_to(&mut self, position: Duration, cx: &mut Context<Self>) {
        let position = self.clamp_position(position);
        self.position = position;
        if let Some(handle) = &self.handle
            && let Err(error) = handle.try_seek(position)
        {
            log::warn!("seeking audio preview: {error:#}");
        }
        cx.notify();
    }

    fn start_playback(&mut self, cx: &mut Context<Self>) {
        let Some(loaded) = self.loaded() else {
            return;
        };
        let path = loaded.path.clone();
        let format_hint = loaded.format_hint.clone();
        let duration = loaded.metadata.duration;
        if let Some(duration) = duration
            && self.position >= duration
        {
            self.position = Duration::ZERO;
        }

        let start = self.position;
        let volume = self.output_volume();

        match player::play_path(&path, &format_hint, start, volume, cx) {
            Ok(handle) => {
                self.handle = Some(handle);
                self.playback = PlaybackStatus::Playing;
                self.playback_error = None;
                self.start_position_tick(cx);
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
            }
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
        self.seek_to(position_from_ratio(duration, ratio), cx);
    }

    pub(crate) fn set_volume_from_ratio(&mut self, ratio: f32, cx: &mut Context<Self>) {
        self.volume = ratio.clamp(0.0, 1.0);
        self.muted = false;
        if let Some(handle) = &self.handle {
            handle.set_volume(self.output_volume());
        }
        cx.notify();
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

fn position_from_ratio(duration: Duration, ratio: f32) -> Duration {
    Duration::from_secs_f64(duration.as_secs_f64() * ratio.clamp(0.0, 1.0) as f64)
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

fn format_bitrate(bitrate: u32) -> String {
    format!("{bitrate} kbps")
}

fn format_bit_depth(bit_depth: u8) -> String {
    format!("{bit_depth}-bit")
}

fn format_sample_rate(sample_rate: u32) -> String {
    if sample_rate.is_multiple_of(1000) {
        format!("{} kHz", sample_rate / 1000)
    } else {
        format!("{:.1} kHz", sample_rate as f64 / 1000.0)
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
    use super::{format_bit_depth, format_bitrate, format_timestamp, position_from_ratio};
    use std::time::Duration;

    #[test]
    fn format_bitrate_and_bit_depth() {
        assert_eq!(format_bitrate(320), "320 kbps");
        assert_eq!(format_bit_depth(16), "16-bit");
        assert_eq!(format_bit_depth(24), "24-bit");
    }

    #[test]
    fn position_from_ratio_maps_seconds() {
        let duration = Duration::from_secs(200);
        assert_eq!(position_from_ratio(duration, 0.0), Duration::ZERO);
        assert_eq!(position_from_ratio(duration, 0.5), Duration::from_secs(100));
        assert_eq!(position_from_ratio(duration, 1.0), duration);
        assert_eq!(position_from_ratio(duration, -0.5), Duration::ZERO);
        assert_eq!(position_from_ratio(duration, 2.0), duration);
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
}
