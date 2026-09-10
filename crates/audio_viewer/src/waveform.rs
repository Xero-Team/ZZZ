use std::time::Duration;

use gpui::{Bounds, Hsla, PathBuilder, Pixels, Window, point, px};
use rodio::Source;

pub(crate) const WAVEFORM_BUCKETS: usize = 2048;
pub(crate) const WAVEFORM_ANALYSIS_LIMIT: Duration = Duration::from_secs(600);

#[cfg(test)]
pub(crate) fn downsample_peaks(samples: &[f32], bucket_count: usize) -> Vec<(f32, f32)> {
    if samples.is_empty() || bucket_count == 0 {
        return Vec::new();
    }

    let bucket_count = bucket_count.min(samples.len());
    let samples_per_bucket = samples.len() as f64 / bucket_count as f64;
    (0..bucket_count)
        .map(|bucket| {
            let start = (bucket as f64 * samples_per_bucket).floor() as usize;
            let end = ((bucket + 1) as f64 * samples_per_bucket).ceil() as usize;
            let end = end.min(samples.len()).max(start + 1);
            let mut min = samples[start];
            let mut max = samples[start];
            for &sample in &samples[start + 1..end] {
                min = min.min(sample);
                max = max.max(sample);
            }
            (min, max)
        })
        .collect()
}
pub(crate) struct WaveformPeaks {
    pub peaks: Vec<(f32, f32)>,
    pub sample_count: usize,
    pub duration: Duration,
    pub reached_end: bool,
}

pub(crate) fn extract_waveform_peaks<S: Source>(
    source: S,
    bucket_count: usize,
    max_duration: Duration,
) -> WaveformPeaks {
    if bucket_count == 0 {
        return WaveformPeaks {
            peaks: Vec::new(),
            sample_count: 0,
            duration: Duration::ZERO,
            reached_end: true,
        };
    }

    let channel_count = source.channels().get() as usize;
    let sample_rate = source.sample_rate().get() as u64;
    let max_samples =
        ((max_duration.as_secs_f64() * sample_rate as f64 * channel_count as f64) as usize).max(1);

    let estimated_total = source
        .total_duration()
        .and_then(|duration| {
            let total =
                (duration.as_secs_f64() * sample_rate as f64 * channel_count as f64) as usize;
            (total > 0).then_some(total)
        })
        .unwrap_or(max_samples)
        .min(max_samples)
        .max(1);

    let waveform = stream_peaks(source, bucket_count, estimated_total, max_samples);
    WaveformPeaks {
        duration: Duration::from_secs_f64(
            waveform.sample_count as f64 / sample_rate as f64 / channel_count as f64,
        ),
        ..waveform
    }
}

fn stream_peaks<S: Source>(
    source: S,
    bucket_count: usize,
    bucket_span: usize,
    max_samples: usize,
) -> WaveformPeaks {
    let mut peaks = vec![(0.0f32, 0.0f32); bucket_count];
    let mut bucket_filled = vec![false; bucket_count];
    let samples_per_bucket = (bucket_span as f64 / bucket_count as f64).max(1.0);
    let mut sample_index = 0usize;
    let mut highest_bucket = 0usize;
    let mut reached_end = true;
    for sample in source {
        if sample_index >= max_samples {
            reached_end = false;
            break;
        }
        let bucket = ((sample_index as f64 / samples_per_bucket) as usize).min(bucket_count - 1);
        highest_bucket = highest_bucket.max(bucket);
        if bucket_filled[bucket] {
            let (min, max) = &mut peaks[bucket];
            *min = min.min(sample);
            *max = max.max(sample);
        } else {
            peaks[bucket] = (sample, sample);
            bucket_filled[bucket] = true;
        }
        sample_index += 1;
    }
    if sample_index == 0 {
        peaks.clear();
    } else {
        peaks.truncate(highest_bucket + 1);
    }
    WaveformPeaks {
        peaks,
        sample_count: sample_index,
        duration: Duration::ZERO,
        reached_end,
    }
}

pub(crate) fn waveform_coverage_ratio(analyzed: Duration, total: Duration) -> f32 {
    if total.is_zero() {
        return 1.0;
    }

    (analyzed.as_secs_f64() / total.as_secs_f64()).clamp(0.0, 1.0) as f32
}

fn peak_count_before_playhead(
    playhead_ratio: f32,
    waveform_coverage_ratio: f32,
    peak_count: usize,
) -> usize {
    if waveform_coverage_ratio <= 0.0 {
        return 0;
    }

    ((playhead_ratio.clamp(0.0, 1.0) / waveform_coverage_ratio * peak_count as f32) as usize)
        .min(peak_count)
}

pub(crate) fn paint_waveform(
    window: &mut Window,
    bounds: Bounds<Pixels>,
    peaks: &[(f32, f32)],
    playhead_ratio: f32,
    waveform_coverage_ratio: f32,
    unplayed: Hsla,
    played: Hsla,
) {
    if peaks.is_empty() {
        return;
    }

    let width = f32::from(bounds.size.width);
    let height = f32::from(bounds.size.height);
    if width <= 0.0 || height <= 0.0 {
        return;
    }

    let waveform_width = width * waveform_coverage_ratio.clamp(0.0, 1.0);
    if waveform_width <= 0.0 {
        return;
    }
    let bar_width = waveform_width / peaks.len() as f32;
    let gap = (bar_width * 0.25).min(1.0);
    let fill_width = (bar_width - gap).max(0.5);
    let center_y = bounds.origin.y + bounds.size.height / 2.0;
    let half_height = bounds.size.height / 2.0;
    let playhead_index =
        peak_count_before_playhead(playhead_ratio, waveform_coverage_ratio, peaks.len());

    paint_peak_range(
        window,
        bounds,
        peaks,
        0..playhead_index,
        bar_width,
        fill_width,
        center_y,
        half_height,
        played,
    );
    paint_peak_range(
        window,
        bounds,
        peaks,
        playhead_index..peaks.len(),
        bar_width,
        fill_width,
        center_y,
        half_height,
        unplayed,
    );
}

fn paint_peak_range(
    window: &mut Window,
    bounds: Bounds<Pixels>,
    peaks: &[(f32, f32)],
    range: std::ops::Range<usize>,
    bar_width: f32,
    fill_width: f32,
    center_y: Pixels,
    half_height: Pixels,
    color: Hsla,
) {
    if range.start >= range.end {
        return;
    }

    let mut builder = PathBuilder::fill();
    let mut added = false;
    for index in range {
        let Some(&(min, max)) = peaks.get(index) else {
            continue;
        };
        let x = bounds.origin.x + px(bar_width * index as f32 + (bar_width - fill_width) / 2.0);
        let y_top = center_y - half_height * max.clamp(-1.0, 1.0);
        let y_bottom = center_y - half_height * min.clamp(-1.0, 1.0);
        let y_top = y_top.min(y_bottom);
        let y_bottom = y_bottom.max(y_top + px(1.0));
        builder.add_polygon(
            &[
                point(x, y_top),
                point(x + px(fill_width), y_top),
                point(x + px(fill_width), y_bottom),
                point(x, y_bottom),
            ],
            true,
        );
        added = true;
    }

    if added && let Ok(path) = builder.build() {
        window.paint_path(path, color);
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{
        downsample_peaks, extract_waveform_peaks, peak_count_before_playhead,
        waveform_coverage_ratio,
    };

    #[test]
    fn downsample_peaks_of_short_synthetic_buffer() {
        let samples = [0.0, 1.0, -0.5, 0.25];
        let peaks = downsample_peaks(&samples, 2);
        assert_eq!(peaks, vec![(0.0, 1.0), (-0.5, 0.25)]);
    }

    #[test]
    fn downsample_peaks_empty_input() {
        assert!(downsample_peaks(&[], 8).is_empty());
        assert!(downsample_peaks(&[0.1, -0.1], 0).is_empty());
    }

    #[test]
    fn downsample_peaks_clamps_bucket_count_to_sample_len() {
        let samples = [0.5, -0.5];
        let peaks = downsample_peaks(&samples, 16);
        assert_eq!(peaks.len(), 2);
    }

    #[test]
    fn stream_peaks_uses_full_width_when_duration_is_overestimated() {
        use super::stream_peaks;
        use rodio::{nz, static_buffer::StaticSamplesBuffer};

        const SAMPLES: [f32; 100] = [0.25; 100];
        let source = StaticSamplesBuffer::new(nz!(1), nz!(100), &SAMPLES);
        let waveform = stream_peaks(source, 10, 1000, 10_000);
        assert!(waveform.reached_end);
        assert_eq!(waveform.sample_count, 100);
        assert_eq!(waveform.peaks.len(), 1);
        assert_eq!(waveform.peaks[0], (0.25, 0.25));
    }

    #[test]
    fn stream_peaks_fills_all_buckets_when_span_matches_samples() {
        use super::stream_peaks;
        use rodio::{nz, static_buffer::StaticSamplesBuffer};

        const SAMPLES: [f32; 100] = {
            let mut samples = [1.0f32; 100];
            let mut index = 50;
            while index < 100 {
                samples[index] = -1.0;
                index += 1;
            }
            samples
        };
        let source = StaticSamplesBuffer::new(nz!(1), nz!(100), &SAMPLES);
        let waveform = stream_peaks(source, 10, 100, 10_000);
        assert!(waveform.reached_end);
        assert_eq!(waveform.sample_count, 100);
        assert_eq!(waveform.peaks.len(), 10);
        assert!(
            waveform
                .peaks
                .iter()
                .take(5)
                .all(|&(min, max)| max > 0.0 && min >= 0.0)
        );
        assert!(
            waveform
                .peaks
                .iter()
                .skip(5)
                .all(|&(min, max)| min < 0.0 && max <= 0.0)
        );
    }

    #[test]
    fn waveform_coverage_keeps_limited_analysis_on_the_full_timeline() {
        assert_eq!(
            waveform_coverage_ratio(Duration::from_secs(600), Duration::from_secs(1200)),
            0.5
        );
        assert_eq!(peak_count_before_playhead(0.25, 0.5, 100), 50);
        assert_eq!(peak_count_before_playhead(0.75, 0.5, 100), 100);
    }

    #[test]
    fn extract_waveform_peaks_reports_the_limited_duration() {
        use rodio::{nz, static_buffer::StaticSamplesBuffer};

        const SAMPLES: [f32; 1_200] = [0.25; 1_200];
        let source = StaticSamplesBuffer::new(nz!(1), nz!(1), &SAMPLES);
        let waveform = extract_waveform_peaks(source, 10, Duration::from_secs(600));

        assert!(!waveform.reached_end);
        assert_eq!(waveform.sample_count, 600);
        assert_eq!(waveform.duration, Duration::from_secs(600));
        assert_eq!(waveform.peaks.len(), 10);
    }
}
