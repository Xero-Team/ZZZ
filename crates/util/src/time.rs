use std::time::Duration;

/// Formats a media position or duration as `H:MM:SS` or `M:SS`.
///
/// Unlike [`duration_alt_display`], this is a clock-style timestamp that rolls
/// minutes into hours instead of showing `60:00`.
pub fn format_media_duration(duration: Duration) -> String {
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

pub fn duration_alt_display(duration: Duration) -> String {
    let hours = duration.as_secs() / 3600;
    let minutes = (duration.as_secs() % 3600) / 60;
    let seconds = duration.as_secs() % 60;

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
        use format_media_duration as f;
        assert_eq!("0:00", f(Duration::from_secs(0)));
        assert_eq!("0:05", f(Duration::from_secs(5)));
        assert_eq!("1:05", f(Duration::from_secs(65)));
        assert_eq!("59:59", f(Duration::from_secs(3599)));
        assert_eq!("1:00:00", f(Duration::from_secs(3600)));
        assert_eq!("1:01:01", f(Duration::from_secs(3661)));
    }

    #[test]
    fn test_duration_alt_display() {
        use duration_alt_display as f;
        assert_eq!("0s", f(Duration::from_secs(0)));
        assert_eq!("59s", f(Duration::from_secs(59)));
        assert_eq!("1m 0s", f(Duration::from_secs(60)));
        assert_eq!("10m 0s", f(Duration::from_secs(600)));
        assert_eq!("1h 0m 0s", f(Duration::from_secs(3600)));
        assert_eq!("3h 2m 1s", f(Duration::from_secs(3600 * 3 + 60 * 2 + 1)));
        assert_eq!("23h 59m 59s", f(Duration::from_secs(3600 * 24 - 1)));
        assert_eq!("100h 0m 0s", f(Duration::from_hours(100)));
    }
}
