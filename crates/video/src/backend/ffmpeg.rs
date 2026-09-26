//! FFmpeg-backed decoder used on Linux and Windows.

use std::path::Path;
use std::time::Duration;

use anyhow::Context as _;
use ffmpeg_the_third as ffmpeg;

use crate::{Rotation, VideoFrame, VideoMetadata, format_label};

/// Decodes a single video stream into packed BGRA frames.
pub struct FfmpegDecoder {
    input: ffmpeg::format::context::Input,
    decoder: ffmpeg::codec::decoder::Video,
    scaler: Option<ffmpeg::software::scaling::Context>,
    scaler_key: Option<(u32, u32, ffmpeg::format::Pixel)>,
    stream_index: usize,
    time_base: ffmpeg::Rational,
    frame_interval: Duration,
    metadata: VideoMetadata,
    rotation: Rotation,
    decoded: ffmpeg::frame::Video,
    scaled: ffmpeg::frame::Video,
    reached_eof: bool,
    skip_until: Option<Duration>,
    position: Duration,
}

// The decoder owns raw FFmpeg contexts, which are not `Sync`. Access is always
// serialized through an `Arc<Mutex<..>>`, so it is safe to move between threads.
unsafe impl Send for FfmpegDecoder {}

impl FfmpegDecoder {
    /// Open `path` and read the video track's metadata.
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        ffmpeg::init().context("initializing ffmpeg")?;
        let file_size = std::fs::metadata(path)
            .map(|metadata| metadata.len())
            .unwrap_or(0);

        let input =
            ffmpeg::format::input(&path).with_context(|| format!("opening {}", path.display()))?;

        let (stream_index, time_base, frame_rate, video_codec_id, rotation, decoder) = {
            let stream = input
                .streams()
                .best(ffmpeg::media::Type::Video)
                .ok_or_else(|| anyhow::anyhow!("no video stream in {}", path.display()))?;
            let frame_rate = non_zero_rational(stream.avg_frame_rate())
                .or_else(|| non_zero_rational(stream.rate()));
            let video_codec_id = stream.parameters().id();
            let rotation = video_stream_rotation(&stream);
            let decoder = ffmpeg::codec::context::Context::from_parameters(stream.parameters())?
                .decoder()
                .video()
                .context("creating video decoder")?;
            (
                stream.index(),
                stream.time_base(),
                frame_rate,
                video_codec_id,
                rotation,
                decoder,
            )
        };
        let coded_width = decoder.width();
        let coded_height = decoder.height();
        let (width, height) = if rotation.swaps_dimensions() {
            (coded_height, coded_width)
        } else {
            (coded_width, coded_height)
        };

        let frame_interval = frame_rate
            .map(|fps| Duration::from_secs_f64(1.0 / fps))
            .filter(|interval| !interval.is_zero())
            .unwrap_or(Duration::from_millis(33));

        let input_duration = input.duration();
        let duration = (input_duration > 0).then(|| Duration::from_micros(input_duration as u64));

        let (has_audio, audio_codec) = match input.streams().best(ffmpeg::media::Type::Audio) {
            Some(stream) => (true, Some(codec_label(stream.parameters().id()))),
            None => (false, None),
        };

        let extension = path.extension().and_then(|extension| extension.to_str());
        let metadata = VideoMetadata {
            duration,
            width,
            height,
            rotation,
            frame_rate,
            video_codec: codec_label(video_codec_id),
            audio_codec,
            has_audio,
            file_size,
            format_label: format_label(extension.unwrap_or("")),
        };

        Ok(Self {
            input,
            decoder,
            scaler: None,
            scaler_key: None,
            stream_index,
            time_base,
            frame_interval,
            metadata,
            rotation,
            decoded: ffmpeg::frame::Video::empty(),
            scaled: ffmpeg::frame::Video::empty(),
            reached_eof: false,
            skip_until: None,
            position: Duration::ZERO,
        })
    }

    /// Metadata gathered when the file was opened.
    pub fn metadata(&self) -> &VideoMetadata {
        &self.metadata
    }

    /// Presentation timestamp of the most recently returned frame.
    pub fn position(&self) -> Duration {
        self.position
    }

    /// Nominal time between frames, derived from the average frame rate.
    pub fn frame_interval(&self) -> Duration {
        self.frame_interval
    }

    /// Decode and return the next frame, or `None` at the end of the stream.
    pub fn next_frame(&mut self) -> anyhow::Result<Option<VideoFrame>> {
        loop {
            match self.decoder.receive_frame(&mut self.decoded) {
                Ok(()) => {
                    let pts = self.frame_timestamp(self.decoded.pts());
                    if let Some(skip_until) = self.skip_until {
                        // Demuxer seeks land on the keyframe at or before the
                        // target, so discard everything the decoder emits until
                        // the requested position is reached.
                        if pts < skip_until {
                            continue;
                        }
                        self.skip_until = None;
                    }
                    let frame = self.scale_and_extract(pts)?;
                    self.position = pts;
                    return Ok(Some(frame));
                }
                Err(ffmpeg::Error::Eof) => return Ok(None),
                Err(ffmpeg::Error::Other { errno }) if errno == libc::EAGAIN => {
                    if self.reached_eof {
                        return Ok(None);
                    }
                    if !self.send_next_packet()? {
                        self.reached_eof = true;
                        if let Err(error) = self.decoder.send_eof() {
                            log::debug!("flushing video decoder: {error}");
                        }
                    }
                }
                Err(error) => return Err(error.into()),
            }
        }
    }

    /// Seek the demuxer to the keyframe at or before `target`.
    pub fn seek(&mut self, target: Duration) -> anyhow::Result<()> {
        let timestamp = target.as_micros().min(i64::MAX as u128) as i64;
        self.input
            .seek(timestamp, ..=timestamp)
            .with_context(|| format!("seeking to {target:?}"))?;
        self.decoder.flush();
        self.reached_eof = false;
        self.skip_until = Some(target);
        self.position = target;
        Ok(())
    }

    fn send_next_packet(&mut self) -> anyhow::Result<bool> {
        let packet = loop {
            let Some(result) = self.input.packets().next() else {
                return Ok(false);
            };
            let (stream, packet) = result?;
            if stream.index() == self.stream_index {
                break packet;
            }
        };
        self.decoder.send_packet(&packet)?;
        Ok(true)
    }

    fn scale_and_extract(&mut self, pts: Duration) -> anyhow::Result<VideoFrame> {
        let key = (
            self.decoded.width(),
            self.decoded.height(),
            self.decoded.format(),
        );
        if self.scaler_key != Some(key) {
            let scaler = ffmpeg::software::scaling::Context::get(
                key.2,
                key.0,
                key.1,
                ffmpeg::format::Pixel::BGRA,
                key.0,
                key.1,
                ffmpeg::software::scaling::flag::Flags::BILINEAR,
            )
            .context("creating pixel format converter")?;
            self.scaler = Some(scaler);
            self.scaler_key = Some(key);
        }

        if let Some(scaler) = self.scaler.as_mut() {
            scaler
                .run(&self.decoded, &mut self.scaled)
                .context("converting frame to BGRA")?;
        }

        let width = self.scaled.width();
        let height = self.scaled.height();
        let stride = self.scaled.stride(0);
        let data = self.scaled.data(0);
        let row_bytes = width as usize * 4;
        let mut bgra = vec![0u8; row_bytes * height as usize];
        for row in 0..height as usize {
            let source = row * stride;
            bgra[row * row_bytes..(row + 1) * row_bytes]
                .copy_from_slice(&data[source..source + row_bytes]);
        }

        let (width, height, bgra) = self.rotation.apply(width, height, bgra);
        Ok(VideoFrame::new(width, height, bgra, pts))
    }

    fn frame_timestamp(&self, pts: Option<i64>) -> Duration {
        match pts {
            Some(pts) if pts >= 0 => duration_from_timestamp(pts, self.time_base),
            _ => self.position.saturating_add(self.frame_interval),
        }
    }
}

fn duration_from_timestamp(timestamp: i64, time_base: ffmpeg::Rational) -> Duration {
    let numerator = time_base.numerator() as f64;
    let denominator = time_base.denominator() as f64;
    if denominator == 0.0 {
        Duration::ZERO
    } else {
        Duration::from_secs_f64(timestamp as f64 * numerator / denominator)
    }
}

fn non_zero_rational(rational: ffmpeg::Rational) -> Option<f64> {
    let value = rational.numerator() as f64 / rational.denominator() as f64;
    (value.is_finite() && value > 0.0).then_some(value)
}

/// Read the stream's display matrix and resolve it into a display rotation.
///
/// The angle convention matches FFmpeg's own autorotation: the reported angle
/// is negated before being snapped to a quarter turn, so a positive value means
/// "rotate the decoded image clockwise by this much".
///
/// FFmpeg 7 moved stream side data onto `AVCodecParameters::coded_side_data`,
/// which the high-level API does not expose yet, so the bindings are read
/// directly here.
fn video_stream_rotation(stream: &ffmpeg::format::stream::Stream<'_>) -> Rotation {
    let parameters = stream.parameters().as_ptr();
    if parameters.is_null() {
        return Rotation::None;
    }

    unsafe {
        let count = (*parameters).nb_coded_side_data;
        if count <= 0 || (*parameters).coded_side_data.is_null() {
            return Rotation::None;
        }
        for index in 0..count as usize {
            let side_data = &*(*parameters).coded_side_data.add(index);
            if side_data.type_ != ffmpeg::ffi::AVPacketSideDataType::DISPLAYMATRIX {
                continue;
            }
            let data = std::slice::from_raw_parts(side_data.data, side_data.size);
            if let Some(rotation) = rotation_from_display_matrix(data) {
                return rotation;
            }
        }
    }

    Rotation::None
}

fn rotation_from_display_matrix(data: &[u8]) -> Option<Rotation> {
    const FIXED_POINT_ONE: f64 = 65536.0;
    const MATRIX_BYTES: usize = 9 * 4;

    if data.len() < MATRIX_BYTES {
        return None;
    }

    let mut matrix = [0i32; 9];
    for (index, value) in matrix.iter_mut().enumerate() {
        let start = index * 4;
        *value = i32::from_ne_bytes(data[start..start + 4].try_into().ok()?);
    }

    let scale_x = (matrix[0] as f64 / FIXED_POINT_ONE).hypot(matrix[3] as f64 / FIXED_POINT_ONE);
    let scale_y = (matrix[1] as f64 / FIXED_POINT_ONE).hypot(matrix[4] as f64 / FIXED_POINT_ONE);
    if scale_x == 0.0 || scale_y == 0.0 {
        return None;
    }

    let a = matrix[0] as f64 / FIXED_POINT_ONE / scale_x;
    let b = matrix[1] as f64 / FIXED_POINT_ONE / scale_y;
    let degrees = b.atan2(a).to_degrees().round() as i32;
    Some(Rotation::from_degrees(degrees))
}

fn codec_label(id: ffmpeg::codec::Id) -> String {
    use ffmpeg::codec::Id;
    match id {
        Id::H264 => "H.264".to_string(),
        Id::HEVC => "H.265".to_string(),
        Id::AV1 => "AV1".to_string(),
        Id::VP9 => "VP9".to_string(),
        Id::VP8 => "VP8".to_string(),
        Id::MPEG2VIDEO => "MPEG-2".to_string(),
        Id::AAC => "AAC".to_string(),
        Id::MP3 => "MP3".to_string(),
        Id::OPUS => "Opus".to_string(),
        Id::VORBIS => "Vorbis".to_string(),
        Id::FLAC => "FLAC".to_string(),
        Id::AC3 => "AC-3".to_string(),
        other => format!("{other:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn display_matrix(matrix: [i32; 9]) -> Vec<u8> {
        matrix
            .into_iter()
            .flat_map(|value| value.to_ne_bytes())
            .collect()
    }

    #[test]
    fn parses_display_matrix_rotations() {
        assert_eq!(
            rotation_from_display_matrix(&display_matrix([
                0,
                -65536,
                0,
                65536,
                0,
                0,
                0,
                0,
                1 << 30,
            ])),
            Some(Rotation::Clockwise270)
        );
        assert_eq!(
            rotation_from_display_matrix(&display_matrix([
                -65536,
                0,
                0,
                0,
                -65536,
                0,
                0,
                0,
                1 << 30,
            ])),
            Some(Rotation::Clockwise180)
        );
        assert_eq!(
            rotation_from_display_matrix(&display_matrix([
                0,
                65536,
                0,
                -65536,
                0,
                0,
                0,
                0,
                1 << 30,
            ])),
            Some(Rotation::Clockwise90)
        );
    }

    #[test]
    fn ignores_unrotated_or_short_matrices() {
        assert_eq!(
            rotation_from_display_matrix(&display_matrix([
                65536,
                0,
                0,
                0,
                65536,
                0,
                0,
                0,
                1 << 30,
            ])),
            Some(Rotation::None)
        );
        assert_eq!(rotation_from_display_matrix(&[0u8; 8]), None);
    }
}
