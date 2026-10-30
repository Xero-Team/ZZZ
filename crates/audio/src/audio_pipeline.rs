use anyhow::{Context as _, Result};
use collections::HashMap;
use cpal::{
    DeviceDescription, DeviceId, default_host,
    traits::{DeviceTrait, HostTrait},
};
use gpui::{App, AsyncApp, BorrowAppContext, Global};
use parking_lot::Mutex;

use rodio::{
    Decoder, DeviceSinkBuilder, MixerDeviceSink, Player, Sample as RodioSample, Source,
    mixer::Mixer, source::Buffered,
};
use settings::Settings;
use std::{
    io::Cursor,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use util::ResultExt;

mod rodio_ext;
pub use crate::audio_settings::AudioSettings;
pub use rodio_ext::RodioExt;

use crate::audio_settings::LIVE_SETTINGS;

use crate::Sound;

use super::{CHANNEL_COUNT, SAMPLE_RATE};

const DEVICE_ENUMERATION_INTERVAL: Duration = Duration::from_secs(10);
static LAST_ENUMERATION: Mutex<Option<Instant>> = Mutex::new(None);

pub fn init(cx: &mut App) {
    LIVE_SETTINGS.initialize(cx);
}

pub fn ensure_devices_initialized(cx: &mut App) {
    let initialized = cx.has_global::<AvailableAudioDevices>();
    if !initialized {
        cx.default_global::<AvailableAudioDevices>();
    }

    let now = Instant::now();
    let should_enumerate = {
        let mut last_enumeration = LAST_ENUMERATION.lock();
        let due = device_enumeration_due(*last_enumeration, now);
        if !initialized || due {
            *last_enumeration = Some(now);
        }
        !initialized || due
    };
    if !should_enumerate {
        return;
    }

    let enumeration_task = cx
        .background_executor()
        .spawn(async move { get_available_audio_devices() });
    cx.spawn(async move |cx: &mut AsyncApp| {
        let devices = enumeration_task.await;
        cx.update(|cx| cx.set_global(AvailableAudioDevices(devices)));
        cx.refresh();
    })
    .detach();
}

fn device_enumeration_due(last: Option<Instant>, now: Instant) -> bool {
    last.is_none_or(|last| now.saturating_duration_since(last) >= DEVICE_ENUMERATION_INTERVAL)
}

struct OutputSink {
    _handle: MixerDeviceSink,
    mixer: Mixer,
    failed: Arc<AtomicBool>,
}

fn output_needs_rebuild(output: Option<&OutputSink>) -> bool {
    output_failure_needs_rebuild(output.map(|output| output.failed.as_ref()))
}

fn output_failure_needs_rebuild(failed: Option<&AtomicBool>) -> bool {
    failed.is_none_or(|failed| failed.load(Ordering::Acquire))
}

#[derive(Default)]
pub struct Audio {
    output: Option<OutputSink>,
    source_cache: HashMap<Sound, Buffered<Decoder<Cursor<Vec<u8>>>>>,
    preview: Option<PlaybackHandle>,
    player: Option<Arc<Player>>,
}

impl Global for Audio {}

impl Audio {
    fn ensure_output_exists(&mut self, output_audio_device: Option<DeviceId>) -> Result<&Mixer> {
        #[cfg(debug_assertions)]
        log::warn!(
            "Audio does not sound correct without optimizations. Use a release build to debug audio issues"
        );

        if output_needs_rebuild(self.output.as_ref()) {
            self.output.take();
            self.stop_playback();

            let output =
                open_output_stream(output_audio_device).context("Could not open output stream")?;
            self.output = Some(output);
        }

        let Some(output) = self.output.as_ref() else {
            anyhow::bail!("output stream was not retained after opening")
        };
        Ok(&output.mixer)
    }

    pub fn play_sound(sound: Sound, cx: &mut App) {
        let output_audio_device = AudioSettings::get_global(cx).output_audio_device.clone();
        cx.update_default_global(|this: &mut Self, cx| {
            let source = this.sound_source(sound, cx).log_err()?;
            let output_mixer = this
                .ensure_output_exists(output_audio_device)
                .context("Could not get output mixer")
                .log_err()?;

            output_mixer.add(source);
            Some(())
        });
    }

    pub fn end_call(cx: &mut App) {
        cx.update_default_global(|this: &mut Self, _cx| this.invalidate_output_state());
    }

    pub(crate) fn invalidate_output(cx: &mut App) {
        cx.update_default_global(|this: &mut Self, _cx| this.invalidate_output_state());
    }

    pub fn play_source<S>(source: S, cx: &mut App) -> Result<PlaybackHandle>
    where
        S: Source + Send + 'static,
    {
        let output_audio_device = AudioSettings::get_global(cx).output_audio_device.clone();
        cx.update_default_global(|this: &mut Self, _cx| {
            this.start_preview(source, output_audio_device)
        })
    }

    /// Play `source` through a controllable [`Player`].
    ///
    /// Unlike [`Audio::play_source`], the returned player can change its
    /// playback speed and its volume (including amplifying above `1.0`) while
    /// it is running, which the video viewer needs for speed control and for
    /// following the audio clock. Dropping the returned handle does not stop
    /// playback; call [`Player::stop`] on it instead.
    pub fn play_source_player<S>(source: S, cx: &mut App) -> Result<Arc<Player>>
    where
        S: Source<Item = f32> + Send + 'static,
    {
        let output_audio_device = AudioSettings::get_global(cx).output_audio_device.clone();
        cx.update_default_global(|this: &mut Self, _cx| {
            if let Some(previous) = this.preview.take() {
                previous.stop();
            }
            if let Some(previous) = this.player.take() {
                previous.stop();
            }
            let mixer = this.ensure_output_exists(output_audio_device)?.clone();
            let player = Arc::new(Player::connect_new(&mixer));
            player.append(source);
            this.player = Some(player.clone());
            Ok(player)
        })
    }

    pub fn stop_preview(cx: &mut App) {
        cx.update_default_global(|this: &mut Self, _cx| this.stop_playback());
    }

    fn stop_playback(&mut self) {
        if let Some(preview) = self.preview.take() {
            preview.stop();
        }
        if let Some(player) = self.player.take() {
            player.stop();
        }
    }

    fn invalidate_output_state(&mut self) {
        self.stop_playback();
        self.output.take();
    }

    fn start_preview<S>(
        &mut self,
        source: S,
        output_audio_device: Option<DeviceId>,
    ) -> Result<PlaybackHandle>
    where
        S: Source + Send + 'static,
    {
        if let Some(previous) = self.preview.take() {
            previous.stop();
        }
        if let Some(previous) = self.player.take() {
            previous.stop();
        }

        let handle = PlaybackHandle::new();
        let control = handle.control.clone();
        let mixer = self.ensure_output_exists(output_audio_device)?.clone();
        mixer.add(wrap_preview_source(source, control));
        self.preview = Some(handle.clone());
        Ok(handle)
    }

    fn sound_source(&mut self, sound: Sound, cx: &App) -> Result<impl Source + use<>> {
        if let Some(wav) = self.source_cache.get(&sound) {
            return Ok(wav.clone());
        }

        let path = format!("sounds/{}.wav", sound.file());
        let bytes = cx
            .asset_source()
            .load(&path)?
            .map(anyhow::Ok)
            .with_context(|| format!("No asset available for path {path}"))??
            .into_owned();
        let cursor = Cursor::new(bytes);
        let source = Decoder::new(cursor)?.buffered();

        self.source_cache.insert(sound, source.clone());

        Ok(source)
    }
}

struct PlaybackControl {
    paused: AtomicBool,
    stopped: AtomicBool,
    finished: AtomicBool,
    volume: AtomicU32,
    sample_position: AtomicU64,
    seek_to_nanos: Mutex<Option<u64>>,
}

impl PlaybackControl {
    fn new() -> Self {
        Self {
            paused: AtomicBool::new(false),
            stopped: AtomicBool::new(false),
            finished: AtomicBool::new(false),
            volume: AtomicU32::new(1.0f32.to_bits()),
            sample_position: AtomicU64::new(0),
            seek_to_nanos: Mutex::new(None),
        }
    }

    fn volume_factor(&self) -> f32 {
        f32::from_bits(self.volume.load(Ordering::Relaxed))
    }
}

#[derive(Clone)]
pub struct PlaybackHandle {
    control: Arc<PlaybackControl>,
}

impl PlaybackHandle {
    fn new() -> Self {
        Self {
            control: Arc::new(PlaybackControl::new()),
        }
    }

    pub fn pause(&self) {
        self.control.paused.store(true, Ordering::Relaxed);
    }

    pub fn resume(&self) {
        if self.is_finished() {
            return;
        }
        self.control.paused.store(false, Ordering::Relaxed);
    }

    pub fn stop(&self) {
        self.control.stopped.store(true, Ordering::Relaxed);
        self.control.finished.store(true, Ordering::Relaxed);
    }

    pub fn set_volume(&self, volume: f32) {
        self.control
            .volume
            .store(volume.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    pub fn try_seek(&self, position: Duration) -> Result<()> {
        if self.control.stopped.load(Ordering::Relaxed) {
            return Ok(());
        }
        *self.control.seek_to_nanos.lock() = Some(position.as_nanos() as u64);
        Ok(())
    }

    pub fn position(&self) -> Duration {
        let samples = self.control.sample_position.load(Ordering::Relaxed);
        // `SAMPLE_RATE` and `CHANNEL_COUNT` are non-zero by construction, so
        // this divisor can never be zero or negative.
        let samples_per_second = SAMPLE_RATE.get() as f64 * CHANNEL_COUNT.get() as f64;
        Duration::from_secs_f64(samples as f64 / samples_per_second)
    }

    pub fn is_paused(&self) -> bool {
        self.control.paused.load(Ordering::Relaxed)
    }

    pub fn is_finished(&self) -> bool {
        self.control.finished.load(Ordering::Relaxed)
    }
}

struct SampleCountingSource<S> {
    inner: S,
    control: Arc<PlaybackControl>,
}

impl<S: Source> Iterator for SampleCountingSource<S> {
    type Item = RodioSample;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(sample) = self.inner.next() {
            self.control.sample_position.fetch_add(1, Ordering::Relaxed);
            Some(sample)
        } else {
            self.control.finished.store(true, Ordering::Relaxed);
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<S: Source> Source for SampleCountingSource<S> {
    fn current_span_len(&self) -> Option<usize> {
        self.inner.current_span_len()
    }

    fn channels(&self) -> rodio::ChannelCount {
        self.inner.channels()
    }

    fn sample_rate(&self) -> rodio::SampleRate {
        self.inner.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }

    fn try_seek(&mut self, position: Duration) -> Result<(), rodio::source::SeekError> {
        self.inner.try_seek(position)?;
        let sample_count = (position.as_secs_f64()
            * self.inner.sample_rate().get() as f64
            * self.inner.channels().get() as f64)
            .round() as u64;
        self.control
            .sample_position
            .store(sample_count, Ordering::Relaxed);
        Ok(())
    }
}

fn wrap_preview_source<S>(source: S, control: Arc<PlaybackControl>) -> impl Source + Send
where
    S: Source + Send + 'static,
{
    let converted = rodio::source::UniformSourceIterator::new(source, CHANNEL_COUNT, SAMPLE_RATE);
    let counted = SampleCountingSource {
        inner: converted,
        control: control.clone(),
    };
    counted
        .amplify(control.volume_factor())
        .pausable(control.paused.load(Ordering::Relaxed))
        .stoppable()
        .periodic_access(Duration::from_millis(20), move |stoppable| {
            apply_playback_control(stoppable, &control);
        })
}

fn apply_playback_control<S: Source>(
    stoppable: &mut rodio::source::Stoppable<
        rodio::source::Pausable<rodio::source::Amplify<SampleCountingSource<S>>>,
    >,
    control: &PlaybackControl,
) {
    if control.stopped.load(Ordering::Relaxed) {
        stoppable.stop();
        return;
    }

    {
        let pausable = stoppable.inner_mut();
        pausable.set_paused(control.paused.load(Ordering::Relaxed));
        pausable.inner_mut().set_factor(control.volume_factor());
    }

    let Some(seek_nanos) = control.seek_to_nanos.lock().take() else {
        return;
    };
    let position = Duration::from_nanos(seek_nanos);
    if let Err(error) = stoppable.try_seek(position) {
        log::warn!("audio preview seek failed: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rodio::{nz, static_buffer::StaticSamplesBuffer};

    #[test]
    fn sample_counting_source_counts_and_marks_finished() {
        const SAMPLES: [RodioSample; 4] = [0.0, 0.5, -0.5, 1.0];
        let buffer = StaticSamplesBuffer::new(nz!(1), nz!(8), &SAMPLES);
        let control = Arc::new(PlaybackControl::new());
        let counted = SampleCountingSource {
            inner: buffer,
            control: control.clone(),
        };
        let yielded = counted.count();
        assert_eq!(yielded, SAMPLES.len());
        assert_eq!(
            control.sample_position.load(Ordering::Relaxed),
            SAMPLES.len() as u64
        );
        assert!(control.finished.load(Ordering::Relaxed));
    }

    #[test]
    fn output_rebuild_state_covers_missing_healthy_and_failed() {
        assert!(output_failure_needs_rebuild(None));

        let failed = AtomicBool::new(false);
        assert!(!output_failure_needs_rebuild(Some(&failed)));

        failed.store(true, Ordering::Release);
        assert!(output_failure_needs_rebuild(Some(&failed)));
    }

    #[test]
    fn device_enumeration_is_throttled_for_ten_seconds() {
        let now = Instant::now();
        assert!(device_enumeration_due(None, now));
        assert!(!device_enumeration_due(
            Some(now),
            now + Duration::from_millis(9_999)
        ));
        assert!(device_enumeration_due(
            Some(now),
            now + DEVICE_ENUMERATION_INTERVAL
        ));
    }
}

pub fn open_input_stream(
    device_id: Option<DeviceId>,
) -> anyhow::Result<rodio::microphone::Microphone> {
    let builder = rodio::microphone::MicrophoneBuilder::new();
    let builder = if let Some(id) = device_id {
        // TODO(jk): upstream patch
        let mut found = None;
        for input in rodio::microphone::available_inputs()? {
            if input.clone().into_inner().id()? == id {
                found = Some(builder.device(input));
                break;
            }
        }
        found.unwrap_or_else(|| builder.default_device())?
    } else {
        builder.default_device()?
    };
    let stream = builder
        .default_config()?
        .prefer_sample_rates([
            SAMPLE_RATE,
            SAMPLE_RATE.saturating_mul(rodio::nz!(2)),
            SAMPLE_RATE.saturating_mul(rodio::nz!(3)),
            SAMPLE_RATE.saturating_mul(rodio::nz!(4)),
        ])
        .prefer_channel_counts([rodio::nz!(1), rodio::nz!(2), rodio::nz!(3), rodio::nz!(4)])
        .prefer_buffer_sizes(512..)
        .open_stream()?;
    log::info!("Opened microphone: {:?}", stream.config());
    Ok(stream)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioDeviceKind {
    Input,
    Output,
}

pub fn resolve_device(
    device_id: Option<&DeviceId>,
    device_kind: AudioDeviceKind,
) -> anyhow::Result<cpal::Device> {
    if let Some(id) = device_id {
        if let Some(device) = default_host().device_by_id(id) {
            return Ok(device);
        }
        log::warn!("Selected audio device not found, falling back to default");
    }
    match device_kind {
        AudioDeviceKind::Input => default_host()
            .default_input_device()
            .context("no audio input device available"),
        AudioDeviceKind::Output => default_host()
            .default_output_device()
            .context("no audio output device available"),
    }
}

pub fn open_test_output(device_id: Option<DeviceId>) -> anyhow::Result<MixerDeviceSink> {
    let device = resolve_device(device_id.as_ref(), AudioDeviceKind::Output)?;
    let builder = DeviceSinkBuilder::from_device(device)?;
    builder
        .open_sink_or_fallback()
        .context("Could not open output stream")
}

fn open_output_stream(device_id: Option<DeviceId>) -> anyhow::Result<OutputSink> {
    let device = resolve_device(device_id.as_ref(), AudioDeviceKind::Output)?;
    let failed = Arc::new(AtomicBool::new(false));
    let callback_failed = failed.clone();
    let builder = DeviceSinkBuilder::from_device(device)?.with_error_callback(move |error| {
        match error.kind() {
            cpal::ErrorKind::DeviceNotAvailable
            | cpal::ErrorKind::BackendError
            | cpal::ErrorKind::DeviceBusy => {
                callback_failed.store(true, Ordering::Release);
                log::warn!("Audio output stream failed and will be rebuilt: {error}");
            }
            cpal::ErrorKind::DeviceChanged => {
                log::info!("Audio output stream was rerouted to a new device: {error}");
            }
            _ => {
                log::warn!("Audio output stream error: {error}");
            }
        }
    });
    let mut output_handle = builder
        .open_sink_or_fallback()
        .context("Could not open output stream")?;
    output_handle.log_on_drop(false);
    log::info!("Output stream: {:?}", output_handle);

    let (output_mixer, source) = rodio::mixer::mixer(CHANNEL_COUNT, SAMPLE_RATE);
    output_mixer.add(rodio::source::Zero::new(CHANNEL_COUNT, SAMPLE_RATE));
    output_handle.mixer().add(source);

    Ok(OutputSink {
        _handle: output_handle,
        mixer: output_mixer,
        failed,
    })
}

#[derive(Clone, Debug)]
pub struct AudioDeviceInfo {
    pub id: DeviceId,
    pub desc: DeviceDescription,
}

impl AudioDeviceInfo {
    pub fn supports(&self, device_kind: AudioDeviceKind) -> bool {
        match device_kind {
            AudioDeviceKind::Input => self.desc.supports_input(),
            AudioDeviceKind::Output => self.desc.supports_output(),
        }
    }

    pub fn matches(&self, id: &DeviceId, device_kind: AudioDeviceKind) -> bool {
        &self.id == id && self.supports(device_kind)
    }
}

impl std::fmt::Display for AudioDeviceInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.desc.name(), self.id)
    }
}

fn get_available_audio_devices() -> Vec<AudioDeviceInfo> {
    let devices = match default_host().devices() {
        Ok(devices) => devices,
        Err(error) => {
            log::warn!("Failed to enumerate audio devices: {error}");
            return Vec::new();
        }
    };
    devices
        .filter_map(|device| {
            // A device that cannot report its id or description cannot be
            // selected from the settings, so skip it rather than listing an
            // entry that would never round-trip through the settings file.
            let id = device.id().warn_on_err()?;
            let desc = device.description().warn_on_err()?;
            Some(AudioDeviceInfo { id, desc })
        })
        .collect()
}

#[derive(Default, Clone, Debug)]
pub struct AvailableAudioDevices(pub Vec<AudioDeviceInfo>);

impl Global for AvailableAudioDevices {}
