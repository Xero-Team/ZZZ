use std::time::Duration;

const SECONDS_PER_MINUTE: u64 = 60;
const SECONDS_PER_HOUR: u64 = 60 * SECONDS_PER_MINUTE;

fn split_into_clock_parts(duration: Duration) -> (u64, u64, u64) {
    let total_seconds = duration.as_secs();
    (
        total_seconds / SECONDS_PER_HOUR,
        (total_seconds % SECONDS_PER_HOUR) / SECONDS_PER_MINUTE,
        total_seconds % SECONDS_PER_MINUTE,
    )
}

/// Formats a media position or duration as `H:MM:SS` or `M:SS`.
///
/// Unlike [`duration_alt_display`], this is a clock-style timestamp that rolls
/// minutes into hours instead of showing `60:00`.
pub fn format_media_duration(duration: Duration) -> String {
    let (hours, minutes, seconds) = split_into_clock_parts(duration);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

/// Formats a media position or duration with explicit unit suffixes, as
/// `1h 2m 3s`, `2m 3s`, or `3s`, dropping the leading zero-valued units.
pub fn duration_alt_display(duration: Duration) -> String {
    let (hours, minutes, seconds) = split_into_clock_parts(duration);

    if hours > 0 {
        format!("{hours}h {minutes}m {seconds}s")
    } else if minutes > 0 {
        format!("{minutes}m {seconds}s")
    } else {
        format!("{seconds}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_media_duration() {
        assert_eq!("0:00", format_media_duration(Duration::from_secs(0)));
        assert_eq!("0:05", format_media_duration(Duration::from_secs(5)));
        assert_eq!("1:05", format_media_duration(Duration::from_secs(65)));
        assert_eq!("59:59", format_media_duration(Duration::from_secs(3599)));
        assert_eq!("1:00:00", format_media_duration(Duration::from_secs(3600)));
        assert_eq!("1:01:01", format_media_duration(Duration::from_secs(3661)));
    }

    #[test]
    fn test_duration_alt_display() {
        assert_eq!("0s", duration_alt_display(Duration::from_secs(0)));
        assert_eq!("59s", duration_alt_display(Duration::from_secs(59)));
        assert_eq!("1m 0s", duration_alt_display(Duration::from_secs(60)));
        assert_eq!("10m 0s", duration_alt_display(Duration::from_secs(600)));
        assert_eq!("1h 0m 0s", duration_alt_display(Duration::from_secs(3600)));
        assert_eq!(
            "3h 2m 1s",
            duration_alt_display(Duration::from_secs(
                3 * SECONDS_PER_HOUR + 2 * SECONDS_PER_MINUTE + 1
            ))
        );
        assert_eq!(
            "23h 59m 59s",
            duration_alt_display(Duration::from_secs(24 * SECONDS_PER_HOUR - 1))
        );
        assert_eq!(
            "100h 0m 0s",
            duration_alt_display(Duration::from_hours(100))
        );
    }
}
