//! AVFoundation-backed decoder used on macOS.
//!
//! Frames are read through an [`AVAssetReaderTrackOutput`] configured for
//! 32-bit BGRA pixel buffers, which are copied into the cross-platform
//! [`VideoFrame`]. Seeking recreates the reader and skips decoded samples
//! until the requested presentation time.
//!
//! This backend is compiled only on macOS.

#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

use std::{ffi::c_void, path::Path, ptr, time::Duration};

use anyhow::{Context as _, anyhow};
use objc::{class, msg_send, runtime::Object, sel, sel_impl};

use crate::{Rotation, VideoFrame, VideoMetadata, format_label};

type Id = *mut Object;

const K_CV_PIXEL_FORMAT_TYPE_32_BGRA: u32 = 0x4247_5241;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct CMTime {
    value: i64,
    timescale: i32,
    flags: u32,
    epoch: i64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct CGSize {
    width: f64,
    height: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
#[allow(dead_code)]
struct CGAffineTransform {
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    tx: f64,
    ty: f64,
}

unsafe impl objc::Encode for CMTime {
    fn encode() -> objc::Encoding {
        unsafe { objc::Encoding::from_str("{CMTime=qiIq}") }
    }
}

unsafe impl objc::Encode for CGSize {
    fn encode() -> objc::Encoding {
        unsafe { objc::Encoding::from_str("{CGSize=dd}") }
    }
}

unsafe impl objc::Encode for CGAffineTransform {
    fn encode() -> objc::Encoding {
        unsafe { objc::Encoding::from_str("{CGAffineTransform=dddddd}") }
    }
}

#[link(name = "AVFoundation", kind = "framework")]
unsafe extern "C" {
    static AVMediaTypeVideo: Id;
    static AVMediaTypeAudio: Id;
}

#[link(name = "CoreMedia", kind = "framework")]
unsafe extern "C" {
    fn CMTimeGetSeconds(time: CMTime) -> f64;
    fn CMSampleBufferGetImageBuffer(sample_buffer: Id) -> Id;
    fn CMSampleBufferGetPresentationTimeStamp(sample_buffer: Id) -> CMTime;
    fn CMFormatDescriptionGetMediaSubType(description: Id) -> u32;
}

#[link(name = "CoreVideo", kind = "framework")]
unsafe extern "C" {
    fn CVPixelBufferLockBaseAddress(pixel_buffer: Id, flags: u64) -> i32;
    fn CVPixelBufferUnlockBaseAddress(pixel_buffer: Id, flags: u64) -> i32;
    fn CVPixelBufferGetBaseAddress(pixel_buffer: Id) -> *const u8;
    fn CVPixelBufferGetBytesPerRow(pixel_buffer: Id) -> usize;
    fn CVPixelBufferGetWidth(pixel_buffer: Id) -> usize;
    fn CVPixelBufferGetHeight(pixel_buffer: Id) -> usize;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(cf: *const c_void);
}

/// Decodes a single video track into packed BGRA frames using AVFoundation.
pub struct AvFoundationDecoder {
    asset: Id,
    video_track: Id,
    reader: Id,
    output: Id,
    frame_interval: Duration,
    metadata: VideoMetadata,
    rotation: Rotation,
    skip_until: Option<Duration>,
    position: Duration,
}

// AVFoundation objects are confined to this struct and only used while holding
// the owning `Mutex`, so moving the decoder between threads is safe.
unsafe impl Send for AvFoundationDecoder {}

impl AvFoundationDecoder {
    /// Open `path` and read the video track's metadata.
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let c_path = std::ffi::CString::new(path.as_os_str().as_encoded_bytes())
            .context("path contains a NUL byte")?;

        unsafe {
            let ns_path: Id = msg_send![class!(NSString), stringWithUTF8String: c_path.as_ptr()];
            if ns_path.is_null() {
                return Err(anyhow!("could not create NSString for {}", path.display()));
            }
            let url: Id = msg_send![class!(NSURL), fileURLWithPath: ns_path];
            let asset: Id = msg_send![class!(AVURLAsset), alloc];
            let asset: Id = msg_send![asset, initWithURL: url options: ptr::null_mut::<Object>()];
            if asset.is_null() {
                return Err(anyhow!("could not open {}", path.display()));
            }

            let tracks: Id = msg_send![asset, tracksWithMediaType: AVMediaTypeVideo];
            let track_count: usize = msg_send![tracks, count];
            if track_count == 0 {
                let _: () = msg_send![asset, release];
                return Err(anyhow!("no video track in {}", path.display()));
            }
            let video_track: Id = msg_send![tracks, objectAtIndex: 0usize];
            let _: Id = msg_send![video_track, retain];

            let size: CGSize = msg_send![video_track, naturalSize];
            let nominal_frame_rate: f32 = msg_send![video_track, nominalFrameRate];
            let transform: CGAffineTransform = msg_send![video_track, preferredTransform];
            // `preferredTransform` maps the track's native pixels into display
            // orientation; its rotation is the angle between the basis vectors.
            let rotation =
                Rotation::from_degrees(transform.b.atan2(transform.a).to_degrees().round() as i32);
            let encoded_width = size.width.max(0.0) as u32;
            let encoded_height = size.height.max(0.0) as u32;
            let (width, height) = if rotation.swaps_dimensions() {
                (encoded_height, encoded_width)
            } else {
                (encoded_width, encoded_height)
            };

            let duration: CMTime = msg_send![asset, duration];
            let duration = cm_time_to_duration(duration).filter(|duration| !duration.is_zero());

            let video_codec = track_codec_label(video_track);
            let audio_tracks: Id = msg_send![asset, tracksWithMediaType: AVMediaTypeAudio];
            let audio_count: usize = msg_send![audio_tracks, count];

            let frame_rate = (nominal_frame_rate as f64 > 0.0).then_some(nominal_frame_rate as f64);
            let frame_interval = frame_rate
                .map(|fps| Duration::from_secs_f64(1.0 / fps))
                .unwrap_or(Duration::from_millis(33));

            let (reader, output) = create_reader(asset, video_track)?;

            let extension = path.extension().and_then(|extension| extension.to_str());
            let metadata = VideoMetadata {
                duration,
                width,
                height,
                rotation,
                frame_rate,
                video_codec,
                audio_codec: (audio_count > 0).then(|| "Audio".to_string()),
                has_audio: audio_count > 0,
                file_size: std::fs::metadata(path).map(|m| m.len()).unwrap_or(0),
                format_label: format_label(extension.unwrap_or("")),
            };

            Ok(Self {
                asset,
                video_track,
                reader,
                output,
                frame_interval,
                metadata,
                rotation,
                skip_until: None,
                position: Duration::ZERO,
            })
        }
    }

    /// Metadata gathered when the file was opened.
    pub fn metadata(&self) -> &VideoMetadata {
        &self.metadata
    }

    /// Presentation timestamp of the most recently returned frame.
    pub fn position(&self) -> Duration {
        self.position
    }

    /// Nominal time between frames, derived from the track's frame rate.
    pub fn frame_interval(&self) -> Duration {
        self.frame_interval
    }

    /// Decode and return the next frame, or `None` at the end of the stream.
    pub fn next_frame(&mut self) -> anyhow::Result<Option<VideoFrame>> {
        unsafe {
            loop {
                let sample_buffer: Id = msg_send![self.output, copyNextSampleBuffer];
                if sample_buffer.is_null() {
                    return Ok(None);
                }

                let image_buffer = CMSampleBufferGetImageBuffer(sample_buffer);
                let timestamp = CMSampleBufferGetPresentationTimeStamp(sample_buffer);
                let pts = cm_time_to_duration(timestamp).unwrap_or(self.position);

                if image_buffer.is_null() {
                    CFRelease(sample_buffer as *const c_void);
                    continue;
                }

                if let Some(skip_until) = self.skip_until {
                    if pts < skip_until {
                        CFRelease(sample_buffer as *const c_void);
                        continue;
                    }
                    self.skip_until = None;
                }

                let frame = self.copy_pixel_buffer(image_buffer, pts);
                CFRelease(sample_buffer as *const c_void);

                let frame = frame?;
                self.position = pts;
                return Ok(Some(frame));
            }
        }
    }

    /// Seek to `target` by rebuilding the reader and skipping earlier samples.
    pub fn seek(&mut self, target: Duration) -> anyhow::Result<()> {
        unsafe {
            let _: () = msg_send![self.reader, cancelReading];
            let _: () = msg_send![self.reader, release];
            let _: () = msg_send![self.output, release];

            let (reader, output) = create_reader(self.asset, self.video_track)?;
            self.reader = reader;
            self.output = output;
            self.skip_until = Some(target);
            self.position = target;
        }
        Ok(())
    }

    unsafe fn copy_pixel_buffer(
        &self,
        image_buffer: Id,
        pts: Duration,
    ) -> anyhow::Result<VideoFrame> {
        let lock_status = unsafe { CVPixelBufferLockBaseAddress(image_buffer, 0) };
        anyhow::ensure!(
            lock_status == 0,
            "locking pixel buffer failed: {lock_status}"
        );

        let width = unsafe { CVPixelBufferGetWidth(image_buffer) };
        let height = unsafe { CVPixelBufferGetHeight(image_buffer) };
        let stride = unsafe { CVPixelBufferGetBytesPerRow(image_buffer) };
        let base = unsafe { CVPixelBufferGetBaseAddress(image_buffer) };

        let row_bytes = width * 4;
        let mut bgra = vec![0u8; row_bytes * height];
        if !base.is_null() {
            for row in 0..height {
                unsafe {
                    ptr::copy_nonoverlapping(
                        base.add(row * stride),
                        bgra.as_mut_ptr().add(row * row_bytes),
                        row_bytes,
                    );
                }
            }
        }

        unsafe { CVPixelBufferUnlockBaseAddress(image_buffer, 0) };

        let (width, height, bgra) = self.rotation.apply(width as u32, height as u32, bgra);
        Ok(VideoFrame::new(width, height, bgra, pts))
    }
}

impl Drop for AvFoundationDecoder {
    fn drop(&mut self) {
        unsafe {
            let _: () = msg_send![self.reader, cancelReading];
            let _: () = msg_send![self.output, release];
            let _: () = msg_send![self.reader, release];
            let _: () = msg_send![self.video_track, release];
            let _: () = msg_send![self.asset, release];
        }
    }
}

unsafe fn create_reader(asset: Id, video_track: Id) -> anyhow::Result<(Id, Id)> {
    unsafe {
        let reader: Id = msg_send![class!(AVAssetReader), alloc];
        let mut error: Id = ptr::null_mut();
        let reader: Id = msg_send![reader, initWithAsset: asset error: &mut error];
        if reader.is_null() {
            return Err(anyhow!(
                "could not create AVAssetReader: {}",
                describe_error(error)
            ));
        }

        let key: Id = msg_send![class!(NSString), stringWithUTF8String: b"PixelFormatType\0".as_ptr() as *const i8];
        let value: Id = msg_send![
            class!(NSNumber),
            numberWithUnsignedInt: K_CV_PIXEL_FORMAT_TYPE_32_BGRA
        ];
        let settings: Id = msg_send![class!(NSDictionary), dictionaryWithObject: value forKey: key];

        let output: Id = msg_send![class!(AVAssetReaderTrackOutput), alloc];
        let output: Id = msg_send![output, initWithTrack: video_track outputSettings: settings];
        let _: () = msg_send![reader, addOutput: output];

        let started: bool = msg_send![reader, startReading];
        if !started {
            let _: () = msg_send![output, release];
            let _: () = msg_send![reader, release];
            return Err(anyhow!("AVAssetReader failed to start reading"));
        }

        Ok((reader, output))
    }
}

unsafe fn track_codec_label(video_track: Id) -> String {
    unsafe {
        let descriptions: Id = msg_send![video_track, formatDescriptions];
        let count: usize = msg_send![descriptions, count];
        if count == 0 {
            return "Video".to_string();
        }
        let description: Id = msg_send![descriptions, objectAtIndex: 0usize];
        codec_label(CMFormatDescriptionGetMediaSubType(description))
    }
}

unsafe fn describe_error(error: Id) -> String {
    if error.is_null() {
        return "unknown error".to_string();
    }
    unsafe {
        let description: Id = msg_send![error, localizedDescription];
        let utf8: *const i8 = msg_send![description, UTF8String];
        if utf8.is_null() {
            "unknown error".to_string()
        } else {
            std::ffi::CStr::from_ptr(utf8)
                .to_string_lossy()
                .into_owned()
        }
    }
}

fn cm_time_to_duration(time: CMTime) -> Option<Duration> {
    let seconds = unsafe { CMTimeGetSeconds(time) };
    (seconds.is_finite() && seconds >= 0.0).then(|| Duration::from_secs_f64(seconds))
}

fn codec_label(code: u32) -> String {
    match &code.to_be_bytes() {
        b"avc1" | b"avc3" => "H.264".to_string(),
        b"hvc1" | b"hev1" => "H.265".to_string(),
        b"av01" => "AV1".to_string(),
        b"vp09" => "VP9".to_string(),
        b"vp08" => "VP8".to_string(),
        other => String::from_utf8_lossy(other)
            .trim_end_matches('\0')
            .to_string(),
    }
}
