use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LyricLine {
    pub time: Duration,
    pub text: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Lyrics {
    pub lines: Vec<LyricLine>,
}

pub(crate) fn lyrics_path_for_audio(audio_path: &Path) -> PathBuf {
    audio_path.with_extension("lrc")
}

pub(crate) fn read_lyrics(path: &Path) -> Option<Arc<Lyrics>> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == ErrorKind::NotFound => return None,
        Err(error) => {
            log::warn!("reading lyrics at {path:?}: {error:#}");
            return None;
        }
    };

    let contents = match String::from_utf8(bytes) {
        Ok(contents) => contents,
        Err(error) => {
            log::warn!("lyrics at {path:?} are not valid UTF-8: {error}");
            return None;
        }
    };

    let lines = parse_lrc(&contents);
    if lines.is_empty() {
        None
    } else {
        Some(Arc::new(Lyrics { lines }))
    }
}

pub(crate) fn parse_lrc(contents: &str) -> Vec<LyricLine> {
    let contents = contents.strip_prefix('\u{feff}').unwrap_or(contents);
    let mut lines = Vec::new();

    for raw_line in contents.lines() {
        let mut remaining = raw_line.trim_start();
        let mut timestamps = Vec::new();

        while let Some(after_open_bracket) = remaining.strip_prefix('[') {
            let Some((tag, after_tag)) = after_open_bracket.split_once(']') else {
                break;
            };
            remaining = after_tag.trim_start();
            if let Some(time) = parse_timestamp(tag) {
                timestamps.push(time);
            }
        }

        let text = remaining.trim();
        if timestamps.is_empty() || text.is_empty() {
            continue;
        }

        lines.extend(timestamps.into_iter().map(|time| LyricLine {
            time,
            text: text.to_owned(),
        }));
    }

    lines.sort_by(|left, right| {
        left.time
            .cmp(&right.time)
            .then_with(|| left.text.cmp(&right.text))
    });
    lines.dedup_by(|left, right| left.time == right.time && left.text == right.text);
    lines
}

pub(crate) fn current_line_index(position: Duration, lines: &[LyricLine]) -> Option<usize> {
    lines
        .partition_point(|line| line.time <= position)
        .checked_sub(1)
}

fn parse_timestamp(tag: &str) -> Option<Duration> {
    let tag = tag.trim();
    let (minutes, seconds_and_fraction) = tag.split_once(':')?;
    if minutes.is_empty()
        || !minutes.bytes().all(|byte| byte.is_ascii_digit())
        || seconds_and_fraction.is_empty()
    {
        return None;
    }

    let (seconds, fraction) = match seconds_and_fraction.split_once('.') {
        Some((seconds, fraction)) => (seconds, Some(fraction)),
        None => (seconds_and_fraction, None),
    };
    if seconds.len() != 2
        || !seconds.bytes().all(|byte| byte.is_ascii_digit())
        || fraction.is_some_and(|fraction| {
            fraction.is_empty()
                || fraction.len() > 3
                || !fraction.bytes().all(|byte| byte.is_ascii_digit())
        })
    {
        return None;
    }

    let minutes = minutes.parse::<u64>().ok()?;
    let seconds = seconds.parse::<u64>().ok()?;
    if seconds >= 60 {
        return None;
    }

    let milliseconds = match fraction {
        Some(fraction) => {
            let mut milliseconds = fraction.parse::<u64>().ok()?;
            for _ in fraction.len()..3 {
                milliseconds *= 10;
            }
            milliseconds
        }
        None => 0,
    };
    let total_milliseconds = minutes
        .checked_mul(60_000)?
        .checked_add(seconds.checked_mul(1_000)?)?
        .checked_add(milliseconds)?;
    Some(Duration::from_millis(total_milliseconds))
}

#[cfg(test)]
mod tests {
    use super::{LyricLine, current_line_index, lyrics_path_for_audio, parse_lrc};
    use std::{path::Path, time::Duration};

    #[test]
    fn parses_timestamp_fractions_and_multiple_tags() {
        let lines = parse_lrc(
            "[00:01]first\n\
             [00:02.5]second\n\
             [00:03.05][01:04.123]third",
        );

        assert_eq!(
            lines,
            vec![
                LyricLine {
                    time: Duration::from_secs(1),
                    text: "first".to_owned(),
                },
                LyricLine {
                    time: Duration::from_millis(2_500),
                    text: "second".to_owned(),
                },
                LyricLine {
                    time: Duration::from_millis(3_050),
                    text: "third".to_owned(),
                },
                LyricLine {
                    time: Duration::from_millis(64_123),
                    text: "third".to_owned(),
                },
            ]
        );
    }

    #[test]
    fn ignores_metadata_blank_lyrics_and_invalid_timestamps() {
        let lines = parse_lrc(
            "[ar:Artist]\n\
             [ti:Title]\n\
             \n\
             [00:01]\n\
             [bad]\n\
             [00:60]invalid\n\
             [00:02.1234]invalid\n\
             [00:03]valid",
        );

        assert_eq!(
            lines,
            vec![LyricLine {
                time: Duration::from_secs(3),
                text: "valid".to_owned(),
            }]
        );
    }

    #[test]
    fn strips_bom_sorts_and_deduplicates_exact_lines() {
        let lines = parse_lrc(
            "\u{feff}[00:02]second\n\
             [00:01]first\n\
             [00:02]second\n\
             [00:02]another",
        );

        assert_eq!(
            lines,
            vec![
                LyricLine {
                    time: Duration::from_secs(1),
                    text: "first".to_owned(),
                },
                LyricLine {
                    time: Duration::from_secs(2),
                    text: "another".to_owned(),
                },
                LyricLine {
                    time: Duration::from_secs(2),
                    text: "second".to_owned(),
                },
            ]
        );
    }

    #[test]
    fn current_line_respects_timestamp_boundaries() {
        let lines = parse_lrc("[00:05]first\n[00:10]second");

        assert_eq!(current_line_index(Duration::ZERO, &lines), None);
        assert_eq!(
            current_line_index(Duration::from_millis(4_999), &lines),
            None
        );
        assert_eq!(current_line_index(Duration::from_secs(5), &lines), Some(0));
        assert_eq!(current_line_index(Duration::from_secs(9), &lines), Some(0));
        assert_eq!(current_line_index(Duration::from_secs(10), &lines), Some(1));
        assert_eq!(current_line_index(Duration::from_secs(60), &lines), Some(1));
        assert_eq!(current_line_index(Duration::ZERO, &[]), None);
    }

    #[test]
    fn lyrics_path_uses_audio_stem_in_the_same_directory() {
        assert_eq!(
            lyrics_path_for_audio(Path::new("/music/track.flac")),
            Path::new("/music/track.lrc")
        );
    }
}
