use std::{
    collections::hash_map::DefaultHasher,
    ffi::OsStr,
    fs,
    hash::{Hash as _, Hasher as _},
    io,
    path::{Path, PathBuf},
    sync::{Arc, LazyLock},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::Context as _;
use async_lock::{Semaphore, SemaphoreGuardArc};

use crate::waveform::WaveformPeaks;

const CACHE_MAGIC: &[u8; 8] = b"ZZZWAVE\0";
const FORMAT_VERSION: u32 = 1;
const CACHE_FILE_EXTENSION: &str = "waveform";
const DEFAULT_MAX_BYTES: u64 = 64 * 1024 * 1024;
const DEFAULT_MAX_FILES: usize = 512;
const MAX_CACHE_FILE_BYTES: u64 = 1024 * 1024;
const MAX_PEAKS: usize = 1024 * 1024;
const KEY_SALT_PRIMARY: u64 = 0x7a7a_5f77_6176_6531;
const KEY_SALT_SECONDARY: u64 = 0xb5ad_4e5a_6f9c_47e3;
const SEMAPHORE_PERMITS: usize = 2;

static WAVEFORM_SEMAPHORE: LazyLock<Arc<Semaphore>> =
    LazyLock::new(|| Arc::new(Semaphore::new(SEMAPHORE_PERMITS)));

#[derive(Clone, Copy)]
struct CacheLimits {
    max_bytes: u64,
    max_files: usize,
}

impl Default for CacheLimits {
    fn default() -> Self {
        Self {
            max_bytes: DEFAULT_MAX_BYTES,
            max_files: DEFAULT_MAX_FILES,
        }
    }
}

#[derive(Debug)]
pub(crate) struct WaveformCacheKey {
    bytes: Vec<u8>,
    filename: String,
}

pub(crate) struct CachedWaveform {
    pub peaks: Arc<[(f32, f32)]>,
    pub duration: Duration,
    pub sample_count: usize,
    pub reached_end: bool,
}

impl CachedWaveform {
    pub fn from_peaks(waveform: &WaveformPeaks) -> Self {
        Self {
            peaks: Arc::from(waveform.peaks.clone()),
            duration: waveform.duration,
            sample_count: waveform.sample_count,
            reached_end: waveform.reached_end,
        }
    }
}

#[derive(Clone)]
pub(crate) struct WaveformCache {
    root: PathBuf,
    limits: CacheLimits,
    semaphore: Arc<Semaphore>,
}

impl WaveformCache {
    pub fn default_cache() -> Self {
        Self {
            root: paths::data_dir().join("audio-waveforms"),
            limits: CacheLimits::default(),
            semaphore: WAVEFORM_SEMAPHORE.clone(),
        }
    }

    #[cfg(test)]
    fn with_root(root: PathBuf, limits: CacheLimits) -> Self {
        Self {
            root,
            limits,
            semaphore: Arc::new(Semaphore::new(SEMAPHORE_PERMITS)),
        }
    }

    pub fn key_for_path(
        &self,
        path: &Path,
        buckets: usize,
        analysis_limit: Duration,
    ) -> Option<WaveformCacheKey> {
        let metadata = match fs::metadata(path) {
            Ok(metadata) => metadata,
            Err(error) => {
                log::debug!(
                    "could not read audio metadata for waveform cache {}: {error}",
                    path.display()
                );
                return None;
            }
        };
        let modified = metadata.modified().ok();
        Some(WaveformCacheKey::new(
            path,
            metadata.len(),
            modified,
            buckets,
            analysis_limit,
        ))
    }

    pub fn load(&self, key: &WaveformCacheKey) -> Option<CachedWaveform> {
        let path = self.file_path(key);
        match read_cached_waveform(&path, key) {
            Ok(Some(waveform)) => {
                log::debug!("waveform cache hit: {}", path.display());
                Some(waveform)
            }
            Ok(None) => None,
            Err(error) => {
                log::debug!(
                    "ignoring invalid waveform cache {}: {error:#}",
                    path.display()
                );
                None
            }
        }
    }

    pub fn store(&self, key: &WaveformCacheKey, waveform: &WaveformPeaks) -> anyhow::Result<()> {
        fs::create_dir_all(&self.root)
            .with_context(|| format!("creating waveform cache {}", self.root.display()))?;
        let destination = self.file_path(key);
        let mut temporary = tempfile::NamedTempFile::new_in(&self.root)
            .with_context(|| format!("creating waveform cache file in {}", self.root.display()))?;
        write_cached_waveform(temporary.as_file_mut(), key, waveform).with_context(|| {
            format!("writing waveform cache temporary file for {}", key.filename)
        })?;
        temporary
            .as_file_mut()
            .sync_all()
            .context("syncing waveform cache temporary file")?;
        temporary
            .persist(&destination)
            .map_err(|error| error.error)
            .with_context(|| format!("installing waveform cache {}", destination.display()))?;
        self.evict()
    }

    pub async fn acquire(&self) -> SemaphoreGuardArc {
        self.semaphore.acquire_arc().await
    }

    fn file_path(&self, key: &WaveformCacheKey) -> PathBuf {
        self.root
            .join(format!("{}.{CACHE_FILE_EXTENSION}", key.filename))
    }

    fn evict(&self) -> anyhow::Result<()> {
        let entries = fs::read_dir(&self.root)
            .with_context(|| format!("listing waveform cache {}", self.root.display()))?;
        let mut files = Vec::new();
        let mut total_bytes = 0u64;

        for entry in entries {
            let entry = entry.context("reading waveform cache directory entry")?;
            let path = entry.path();
            if path.extension().and_then(OsStr::to_str) != Some(CACHE_FILE_EXTENSION) {
                continue;
            }
            let metadata = match entry.metadata() {
                Ok(metadata) if metadata.is_file() => metadata,
                Ok(_) => continue,
                Err(error) => {
                    log::warn!(
                        "could not inspect waveform cache file {}: {error}",
                        path.display()
                    );
                    continue;
                }
            };
            total_bytes = total_bytes.saturating_add(metadata.len());
            files.push((metadata.modified().ok(), metadata.len(), path));
        }

        files.sort_by(|left, right| {
            left.0
                .cmp(&right.0)
                .then_with(|| left.2.file_name().cmp(&right.2.file_name()))
        });

        let mut file_count = files.len();
        for (_, len, path) in files {
            if file_count <= self.limits.max_files && total_bytes <= self.limits.max_bytes {
                break;
            }
            match fs::remove_file(&path) {
                Ok(()) => {
                    file_count -= 1;
                    total_bytes = total_bytes.saturating_sub(len);
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    file_count -= 1;
                    total_bytes = total_bytes.saturating_sub(len);
                }
                Err(error) => log::warn!(
                    "could not evict waveform cache file {}: {error}",
                    path.display()
                ),
            }
        }
        Ok(())
    }
}

impl WaveformCacheKey {
    fn new(
        path: &Path,
        file_size: u64,
        modified: Option<SystemTime>,
        buckets: usize,
        analysis_limit: Duration,
    ) -> Self {
        let path = path.as_os_str().as_encoded_bytes();
        let mut bytes = Vec::with_capacity(path.len() + 64);
        bytes.extend_from_slice(&(path.len() as u64).to_le_bytes());
        bytes.extend_from_slice(path);
        bytes.extend_from_slice(&file_size.to_le_bytes());
        match modified.and_then(|time| time.duration_since(UNIX_EPOCH).ok()) {
            Some(modified) => {
                bytes.push(1);
                bytes.extend_from_slice(&modified.as_secs().to_le_bytes());
                bytes.extend_from_slice(&modified.subsec_nanos().to_le_bytes());
            }
            None => bytes.push(0),
        }
        bytes.extend_from_slice(&(buckets as u64).to_le_bytes());
        bytes.extend_from_slice(&analysis_limit.as_secs().to_le_bytes());
        bytes.extend_from_slice(&analysis_limit.subsec_nanos().to_le_bytes());
        bytes.extend_from_slice(&FORMAT_VERSION.to_le_bytes());

        let primary = hash_key(KEY_SALT_PRIMARY, &bytes);
        let secondary = hash_key(KEY_SALT_SECONDARY, &bytes);
        Self {
            bytes,
            filename: format!("{primary:016x}{secondary:016x}"),
        }
    }
}

fn hash_key(salt: u64, bytes: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    salt.hash(&mut hasher);
    bytes.hash(&mut hasher);
    hasher.finish()
}

fn write_cached_waveform(
    mut writer: impl io::Write,
    key: &WaveformCacheKey,
    waveform: &WaveformPeaks,
) -> anyhow::Result<()> {
    let key_len = u32::try_from(key.bytes.len()).context("waveform cache key is too large")?;
    let peak_count =
        u32::try_from(waveform.peaks.len()).context("waveform cache has too many peaks")?;
    let sample_count =
        u64::try_from(waveform.sample_count).context("waveform sample count is too large")?;

    writer.write_all(CACHE_MAGIC)?;
    writer.write_all(&FORMAT_VERSION.to_le_bytes())?;
    writer.write_all(&key_len.to_le_bytes())?;
    writer.write_all(&key.bytes)?;
    writer.write_all(&peak_count.to_le_bytes())?;
    writer.write_all(&waveform.duration.as_secs().to_le_bytes())?;
    writer.write_all(&waveform.duration.subsec_nanos().to_le_bytes())?;
    writer.write_all(&sample_count.to_le_bytes())?;
    writer.write_all(&[u8::from(waveform.reached_end)])?;
    for &(min, max) in &waveform.peaks {
        writer.write_all(&quantize_sample(min).to_le_bytes())?;
        writer.write_all(&quantize_sample(max).to_le_bytes())?;
    }
    writer.flush()?;
    Ok(())
}

fn read_cached_waveform(
    path: &Path,
    expected_key: &WaveformCacheKey,
) -> anyhow::Result<Option<CachedWaveform>> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if metadata.len() > MAX_CACHE_FILE_BYTES {
        return Ok(None);
    }
    let bytes = fs::read(path)?;
    let mut cursor = 0;
    let magic = take(&bytes, &mut cursor, CACHE_MAGIC.len())?;
    if magic != CACHE_MAGIC {
        return Ok(None);
    }
    let version = read_u32(&bytes, &mut cursor)?;
    if version != FORMAT_VERSION {
        return Ok(None);
    }
    let key_len = read_u32(&bytes, &mut cursor)? as usize;
    let key = take(&bytes, &mut cursor, key_len)?;
    if key != expected_key.bytes {
        return Ok(None);
    }
    let peak_count = read_u32(&bytes, &mut cursor)? as usize;
    if peak_count > MAX_PEAKS {
        return Ok(None);
    }
    let duration_secs = read_u64(&bytes, &mut cursor)?;
    let duration_nanos = read_u32(&bytes, &mut cursor)?;
    if duration_nanos >= 1_000_000_000 {
        return Ok(None);
    }
    let sample_count = usize::try_from(read_u64(&bytes, &mut cursor)?)?;
    let reached_end = match take(&bytes, &mut cursor, 1)?[0] {
        0 => false,
        1 => true,
        _ => return Ok(None),
    };

    let expected_len = cursor
        .checked_add(peak_count.saturating_mul(4))
        .context("waveform cache length overflow")?;
    if bytes.len() != expected_len {
        return Ok(None);
    }
    let mut peaks = Vec::with_capacity(peak_count);
    for _ in 0..peak_count {
        let min = read_i16(&bytes, &mut cursor)?;
        let max = read_i16(&bytes, &mut cursor)?;
        peaks.push((dequantize_sample(min), dequantize_sample(max)));
    }

    Ok(Some(CachedWaveform {
        peaks: Arc::from(peaks),
        duration: Duration::new(duration_secs, duration_nanos),
        sample_count,
        reached_end,
    }))
}

fn take<'a>(bytes: &'a [u8], cursor: &mut usize, len: usize) -> anyhow::Result<&'a [u8]> {
    let end = cursor
        .checked_add(len)
        .context("waveform cache offset overflow")?;
    let slice = bytes
        .get(*cursor..end)
        .context("truncated waveform cache file")?;
    *cursor = end;
    Ok(slice)
}

fn read_u32(bytes: &[u8], cursor: &mut usize) -> anyhow::Result<u32> {
    let value: [u8; 4] = take(bytes, cursor, 4)?
        .try_into()
        .context("invalid u32 in waveform cache")?;
    Ok(u32::from_le_bytes(value))
}

fn read_u64(bytes: &[u8], cursor: &mut usize) -> anyhow::Result<u64> {
    let value: [u8; 8] = take(bytes, cursor, 8)?
        .try_into()
        .context("invalid u64 in waveform cache")?;
    Ok(u64::from_le_bytes(value))
}

fn read_i16(bytes: &[u8], cursor: &mut usize) -> anyhow::Result<i16> {
    let value: [u8; 2] = take(bytes, cursor, 2)?
        .try_into()
        .context("invalid i16 in waveform cache")?;
    Ok(i16::from_le_bytes(value))
}

fn quantize_sample(sample: f32) -> i16 {
    (sample.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16
}

fn dequantize_sample(sample: i16) -> f32 {
    sample as f32 / i16::MAX as f32
}

#[cfg(test)]
mod tests {
    use std::{
        path::PathBuf,
        time::{Duration, UNIX_EPOCH},
    };

    use tempfile::tempdir;

    use super::{
        CACHE_MAGIC, CacheLimits, FORMAT_VERSION, WaveformCache, WaveformCacheKey,
        read_cached_waveform,
    };
    use crate::waveform::WaveformPeaks;

    fn test_cache(max_bytes: u64, max_files: usize) -> (tempfile::TempDir, WaveformCache) {
        let directory = tempdir().expect("temporary directory should be created");
        let cache = WaveformCache::with_root(
            directory.path().to_path_buf(),
            CacheLimits {
                max_bytes,
                max_files,
            },
        );
        (directory, cache)
    }

    fn key(index: u64) -> WaveformCacheKey {
        WaveformCacheKey::new(
            &PathBuf::from(format!("/audio/{index}.wav")),
            1024 + index,
            Some(UNIX_EPOCH + Duration::from_secs(100 + index)),
            2048,
            Duration::from_secs(600),
        )
    }

    fn waveform() -> WaveformPeaks {
        WaveformPeaks {
            peaks: vec![(-0.75, 0.5), (-0.125, 0.875), (0.0, 1.0)],
            sample_count: 3072,
            duration: Duration::new(12, 345_000_000),
            reached_end: true,
        }
    }

    #[test]
    fn cache_round_trip_preserves_quantized_waveform() {
        let (_directory, cache) = test_cache(1024 * 1024, 2);
        let cache_key = key(0);
        let waveform = waveform();

        cache
            .store(&cache_key, &waveform)
            .expect("waveform should be cached");
        let cached = cache.load(&cache_key).expect("waveform should be found");

        assert_eq!(cached.duration, waveform.duration);
        assert_eq!(cached.sample_count, waveform.sample_count);
        assert_eq!(cached.reached_end, waveform.reached_end);
        assert_eq!(cached.peaks.len(), waveform.peaks.len());
        for (cached, original) in cached.peaks.iter().zip(&waveform.peaks) {
            assert!((cached.0 - original.0).abs() <= 1.0 / i16::MAX as f32);
            assert!((cached.1 - original.1).abs() <= 1.0 / i16::MAX as f32);
        }
    }

    #[test]
    fn cache_key_changes_with_file_metadata_and_analysis_parameters() {
        let directory = tempdir().expect("temporary directory should be created");
        let path = directory.path().join("audio.wav");
        std::fs::write(&path, [1, 2, 3]).expect("audio file should be written");
        let cache =
            WaveformCache::with_root(directory.path().join("cache"), CacheLimits::default());

        let initial = cache
            .key_for_path(&path, 2048, Duration::from_secs(600))
            .expect("initial key should be created");
        std::fs::write(&path, [1, 2, 3, 4]).expect("audio file should be changed");
        let changed_size = cache
            .key_for_path(&path, 2048, Duration::from_secs(600))
            .expect("changed-size key should be created");
        let changed_buckets = cache
            .key_for_path(&path, 4096, Duration::from_secs(600))
            .expect("changed-buckets key should be created");
        let changed_limit = cache
            .key_for_path(&path, 2048, Duration::from_secs(300))
            .expect("changed-limit key should be created");

        assert_ne!(initial.filename, changed_size.filename);
        assert_ne!(changed_size.filename, changed_buckets.filename);
        assert_ne!(changed_size.filename, changed_limit.filename);
    }

    #[test]
    fn cache_rejects_wrong_format_version() {
        let (_directory, cache) = test_cache(1024 * 1024, 2);
        let key = key(0);
        cache
            .store(&key, &waveform())
            .expect("waveform should be cached");
        let path = cache.file_path(&key);
        let mut bytes = std::fs::read(&path).expect("cache file should be readable");
        let version_offset = CACHE_MAGIC.len();
        bytes[version_offset..version_offset + 4]
            .copy_from_slice(&(FORMAT_VERSION + 1).to_le_bytes());
        std::fs::write(&path, bytes).expect("cache file should be writable");

        assert!(cache.load(&key).is_none());
    }

    #[test]
    fn cache_rejects_truncated_file() {
        let (_directory, cache) = test_cache(1024 * 1024, 2);
        let key = key(0);
        cache
            .store(&key, &waveform())
            .expect("waveform should be cached");
        let path = cache.file_path(&key);
        let bytes = std::fs::read(&path).expect("cache file should be readable");
        std::fs::write(&path, &bytes[..bytes.len() - 1]).expect("truncated file should be written");

        assert!(cache.load(&key).is_none());
    }

    #[test]
    fn cache_rejects_file_whose_embedded_key_does_not_match() {
        let (_directory, cache) = test_cache(1024 * 1024, 2);
        let first = key(0);
        let second = key(1);
        cache
            .store(&first, &waveform())
            .expect("waveform should be cached");
        std::fs::copy(cache.file_path(&first), cache.file_path(&second))
            .expect("cache file should be copied");

        assert!(cache.load(&second).is_none());
    }

    #[test]
    fn cache_evicts_oldest_files_to_respect_file_limit() {
        let (_directory, cache) = test_cache(1024 * 1024, 2);
        let keys = [key(0), key(1), key(2)];
        for key in &keys {
            cache
                .store(key, &waveform())
                .expect("waveform should be cached");
            std::thread::sleep(Duration::from_millis(5));
        }

        assert!(!cache.file_path(&keys[0]).exists());
        assert!(cache.file_path(&keys[1]).exists());
        assert!(cache.file_path(&keys[2]).exists());
    }

    #[test]
    fn cache_evicts_files_to_respect_byte_limit() {
        let (_directory, cache) = test_cache(1, 10);
        let key = key(0);
        cache
            .store(&key, &waveform())
            .expect("waveform should be cached");

        assert!(!cache.file_path(&key).exists());
    }

    #[test]
    fn cache_semaphore_allows_two_concurrent_decodes() {
        let (_directory, cache) = test_cache(1024 * 1024, 2);
        let first = cache
            .semaphore
            .try_acquire()
            .expect("first permit should be available");
        let second = cache
            .semaphore
            .try_acquire()
            .expect("second permit should be available");
        assert!(cache.semaphore.try_acquire().is_none());
        drop(second);
        assert!(cache.semaphore.try_acquire().is_some());
        drop(first);
    }

    #[test]
    fn cache_reader_short_circuits_without_matching_key() {
        let (_directory, cache) = test_cache(1024 * 1024, 2);
        let cache_key = key(0);
        cache
            .store(&cache_key, &waveform())
            .expect("waveform should be cached");
        let wrong_key = key(1);

        assert!(
            read_cached_waveform(&cache.file_path(&cache_key), &wrong_key)
                .expect("cache read should not fail")
                .is_none()
        );
    }
}
