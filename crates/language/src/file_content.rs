use std::io::{self, Read};

use chardetng::{EncodingDetector, Iso2022JpDetection, Utf8Detection};
use encoding_rs::{CoderResult, Decoder, Encoding};
use futures_lite::future::yield_now;

/// Number of leading bytes that [`analyze_byte_content`] inspects.
pub const FILE_ANALYSIS_BYTES: usize = 1024;

/// Number of bytes [`DecodingReader`] transcodes per read of its source.
const DECODING_BLOCK_BYTES: usize = 64 * 1024;

/// Reads up to [`FILE_ANALYSIS_BYTES`] from `reader`.
///
/// Returns the bytes actually read and whether the source ended before the full
/// prefix was available, so a caller can decide from the prefix and still hand
/// the same source on to a decoder.
pub fn read_byte_header(reader: &mut dyn Read) -> io::Result<(Vec<u8>, bool)> {
    let mut header = vec![0; FILE_ANALYSIS_BYTES];
    let mut length = 0;
    while length < header.len() {
        let read = match reader.read(&mut header[length..]) {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            result => result?,
        };
        if read == 0 {
            header.truncate(length);
            return Ok((header, true));
        }
        length += read;
    }
    Ok((header, false))
}

/// A [`Read`] adapter that transcodes its source into UTF-8 block by block, so no
/// more than one block of the source is held in memory at a time.
///
/// [`Encoding::new_decoder`] removes a byte order mark, so a source that starts
/// with one yields text without it.
pub struct DecodingReader<R> {
    inner: R,
    decoder: Decoder,
    input: Vec<u8>,
    output: Vec<u8>,
    output_position: usize,
    finished: bool,
}

impl<R: Read> DecodingReader<R> {
    /// Wraps `inner`, decoding it from `encoding` into UTF-8.
    pub fn new(inner: R, encoding: &'static Encoding) -> Self {
        Self {
            inner,
            decoder: encoding.new_decoder(),
            input: vec![0; DECODING_BLOCK_BYTES],
            output: Vec::new(),
            output_position: 0,
            finished: false,
        }
    }

    /// The encoding the source is decoded from.
    pub fn encoding(&self) -> &'static Encoding {
        self.decoder.encoding()
    }

    fn fill_output(&mut self) -> io::Result<()> {
        self.output.clear();
        self.output_position = 0;
        let read = self.inner.read(&mut self.input)?;
        self.finished = read == 0;
        decode_block(
            &mut self.decoder,
            &self.input[..read],
            self.finished,
            &mut self.output,
        )
    }
}

impl<R: Read> Read for DecodingReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        while self.output_position == self.output.len() {
            if self.finished {
                return Ok(0);
            }
            match self.fill_output() {
                // A signal interrupting the read is not an error; retry.
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                result => result?,
            }
        }

        let available = &self.output[self.output_position..];
        let count = available.len().min(buffer.len());
        buffer[..count].copy_from_slice(&available[..count]);
        self.output_position += count;
        Ok(count)
    }
}

fn decode_block(
    decoder: &mut Decoder,
    block: &[u8],
    last: bool,
    output: &mut Vec<u8>,
) -> io::Result<()> {
    let capacity = decoder
        .max_utf8_buffer_length(block.len())
        .ok_or_else(|| io::Error::new(io::ErrorKind::OutOfMemory, "decoded block is too large"))?;
    let start = output.len();
    output.resize(start + capacity, 0);
    let (result, consumed, written, _) = decoder.decode_to_utf8(block, &mut output[start..], last);
    output.truncate(start + written);
    if result != CoderResult::InputEmpty || consumed != block.len() {
        return Err(io::Error::other("decoder did not consume the whole block"));
    }
    Ok(())
}

/// Detects the encoding of `reader` without holding the whole source in memory,
/// yielding between blocks so a large file does not stall the executor.
///
/// Detection needs the whole stream, so a caller that wants to decode afterwards
/// must be able to replay it — for example by re-opening the file.
pub async fn detect_encoding(reader: &mut (dyn Read + Send)) -> io::Result<&'static Encoding> {
    let mut detector = EncodingDetector::new(Iso2022JpDetection::Allow);
    let mut block = vec![0; DECODING_BLOCK_BYTES];
    let mut length = 0;
    loop {
        let read = match reader.read(&mut block[length..]) {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            result => result?,
        };
        if read == 0 {
            detector.feed(&block[..length], true);
            return Ok(detector.guess(None, Utf8Detection::Allow));
        }
        length += read;
        if length == block.len() {
            detector.feed(&block, false);
            yield_now().await;
            length = 0;
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum ByteContent {
    Utf16Le,
    Utf16Be,
    Binary,
    Unknown,
}

impl ByteContent {
    /// The encoding this classification pins down, if any.
    pub fn encoding(&self) -> Option<&'static Encoding> {
        match self {
            Self::Utf16Le => Some(encoding_rs::UTF_16LE),
            Self::Utf16Be => Some(encoding_rs::UTF_16BE),
            Self::Binary | Self::Unknown => None,
        }
    }
}

// Heuristic check using null byte distribution plus a generic text-likeness
// heuristic. This prefers UTF-16 when many bytes are NUL and otherwise
// distinguishes between text-like and binary-like content.
pub fn analyze_byte_content(bytes: &[u8]) -> ByteContent {
    if bytes.len() < 2 {
        return ByteContent::Unknown;
    }

    if is_known_binary_header(bytes) {
        return ByteContent::Binary;
    }

    let limit = bytes.len().min(FILE_ANALYSIS_BYTES);
    let mut even_null_count = 0usize;
    let mut odd_null_count = 0usize;
    let mut non_text_like_count = 0usize;

    for (i, &byte) in bytes[..limit].iter().enumerate() {
        if byte == 0 {
            if i % 2 == 0 {
                even_null_count += 1;
            } else {
                odd_null_count += 1;
            }
            non_text_like_count += 1;
            continue;
        }

        let is_text_like = match byte {
            b'\t' | b'\n' | b'\r' | 0x0C => true,
            0x20..=0x7E => true,
            // Treat bytes that are likely part of UTF-8 or single-byte encodings as text-like.
            0x80..=0xBF | 0xC2..=0xF4 => true,
            _ => false,
        };

        if !is_text_like {
            non_text_like_count += 1;
        }
    }

    let total_null_count = even_null_count + odd_null_count;

    // If there are no NUL bytes at all, this is overwhelmingly likely to be text.
    if total_null_count == 0 {
        return ByteContent::Unknown;
    }

    let has_significant_nulls = total_null_count >= limit / 16;
    let nulls_skew_to_even = even_null_count > odd_null_count * 4;
    let nulls_skew_to_odd = odd_null_count > even_null_count * 4;

    if has_significant_nulls {
        let sample = &bytes[..limit];

        // UTF-16BE ASCII: [0x00, char] — nulls at even positions (high byte first)
        // UTF-16LE ASCII: [char, 0x00] — nulls at odd positions (low byte first)

        if nulls_skew_to_even && is_plausible_utf16_text(sample, false) {
            return ByteContent::Utf16Be;
        }

        if nulls_skew_to_odd && is_plausible_utf16_text(sample, true) {
            return ByteContent::Utf16Le;
        }

        return ByteContent::Binary;
    }

    if non_text_like_count * 100 < limit * 8 {
        ByteContent::Unknown
    } else {
        ByteContent::Binary
    }
}

fn is_known_binary_header(bytes: &[u8]) -> bool {
    bytes.starts_with(b"%PDF-") // PDF
        || bytes.starts_with(b"PK\x03\x04") // ZIP local header
        || bytes.starts_with(b"PK\x05\x06") // ZIP end of central directory
        || bytes.starts_with(b"PK\x07\x08") // ZIP spanning/splitting
        || bytes.starts_with(b"\x89PNG\r\n\x1a\n") // PNG
        || bytes.starts_with(b"\xFF\xD8\xFF") // JPEG
        || bytes.starts_with(b"GIF87a") // GIF87a
        || bytes.starts_with(b"GIF89a") // GIF89a
        || bytes.starts_with(b"IWAD") // Doom IWAD archive
        || bytes.starts_with(b"PWAD") // Doom PWAD archive
        || bytes.starts_with(b"RIFF") // WAV, AVI, WebP
        || bytes.starts_with(b"OggS") // OGG (Vorbis, Opus, FLAC)
        || bytes.starts_with(b"fLaC") // FLAC
        || bytes.starts_with(b"ID3") // MP3 with ID3v2 tag
        || bytes.starts_with(b"\xFF\xFB") // MP3 frame sync (MPEG1 Layer3)
        || bytes.starts_with(b"\xFF\xFA") // MP3 frame sync (MPEG1 Layer3)
        || bytes.starts_with(b"\xFF\xF3") // MP3 frame sync (MPEG2 Layer3)
        || bytes.starts_with(b"\xFF\xF2") // MP3 frame sync (MPEG2 Layer3)
}

// Null byte skew alone is not enough to identify UTF-16 -- binary formats with
// small 16-bit values (like PCM audio) produce the same pattern. Decode the
// bytes as UTF-16 and reject if too many code units land in control character
// ranges or form unpaired surrogates, which real text almost never contains.
fn is_plausible_utf16_text(bytes: &[u8], little_endian: bool) -> bool {
    let mut suspicious_count = 0usize;
    let mut word_like_count = 0usize;
    let mut total = 0usize;

    let mut i = 0;
    while let Some(code_unit) = read_u16(bytes, i, little_endian) {
        total += 1;

        match code_unit {
            0x0009 | 0x000A | 0x000C | 0x000D => {}
            0x0020 | 0x0030..=0x0039 | 0x0041..=0x005A | 0x0061..=0x007A => {
                word_like_count += 1;
            }
            // C0/C1 control characters and non-characters
            0x0000..=0x001F | 0x007F..=0x009F | 0xFFFE | 0xFFFF => suspicious_count += 1,
            0xD800..=0xDBFF => {
                let next_offset = i + 2;
                let has_low_surrogate = read_u16(bytes, next_offset, little_endian)
                    .is_some_and(|next| (0xDC00..=0xDFFF).contains(&next));
                if has_low_surrogate {
                    total += 1;
                    word_like_count += 2;
                    i += 2;
                } else {
                    suspicious_count += 1;
                }
            }
            // Lone low surrogate without a preceding high surrogate
            0xDC00..=0xDFFF => suspicious_count += 1,
            0x0100.. => word_like_count += 1,
            _ => {}
        }

        i += 2;
    }

    if total == 0 {
        return false;
    }

    // Real UTF-16 text has near-zero control characters; binary data with
    // small 16-bit values typically exceeds 5%. 2% provides a safe margin.
    let low_control_ratio = suspicious_count * 100 < total * 2;

    // Binary formats that interleave short ASCII fragments with small
    // length/type fields (e.g. game asset formats) can dodge the control
    // character check above while barely containing any real words: their
    // code units land on ASCII punctuation and Latin-1 symbol values rather
    // than letters, digits, or spaces. Real text is overwhelmingly made of
    // word characters, so require a minimum share of them. Code units above
    // the Latin-1 range (and surrogate pairs) also count as word-like so
    // that scripts such as Cyrillic or Greek, whose letters are non-ASCII,
    // are still recognized -- tag bytes paired with a zero byte can never
    // land there.
    let enough_word_chars = word_like_count * 100 >= total * 30;

    low_control_ratio && enough_word_chars
}

fn read_u16(bytes: &[u8], offset: usize, little_endian: bool) -> Option<u16> {
    let pair = [*bytes.get(offset)?, *bytes.get(offset + 1)?];
    if little_endian {
        return Some(u16::from_le_bytes(pair));
    }
    Some(u16::from_be_bytes(pair))
}

#[cfg(test)]
mod tests {
    use super::{DecodingReader, read_byte_header};
    use encoding_rs::{UTF_8, UTF_16LE};
    use std::io::{Cursor, Read as _};

    #[test]
    fn decoding_reader_transcodes_across_blocks() {
        // More than one 64 KiB block, so the transcoder has to stitch a
        // multi-byte sequence that spans the read boundary together correctly.
        let text = "Здравствуй, мир! ".repeat(8_000);
        let mut encoded = Vec::new();
        encoded.extend_from_slice(&[0xFF, 0xFE]);
        encoded.extend(text.encode_utf16().flat_map(u16::to_le_bytes));

        let mut reader = DecodingReader::new(Cursor::new(encoded), UTF_16LE);
        let mut decoded = String::new();
        reader
            .read_to_string(&mut decoded)
            .expect("in-memory source cannot fail");

        assert_eq!(decoded, text);
        assert_eq!(reader.encoding(), UTF_16LE);
    }

    #[test]
    fn decoding_reader_passes_utf8_through() {
        let mut reader = DecodingReader::new(Cursor::new(b"plain ascii".to_vec()), UTF_8);
        let mut decoded = String::new();
        reader
            .read_to_string(&mut decoded)
            .expect("in-memory source cannot fail");
        assert_eq!(decoded, "plain ascii");
    }

    #[test]
    fn read_byte_header_reports_short_sources() {
        let mut short = Cursor::new(b"short".to_vec());
        let (header, reached_eof) =
            read_byte_header(&mut short).expect("in-memory read cannot fail");
        assert_eq!(header, b"short");
        assert!(reached_eof);

        let mut long = Cursor::new(vec![b'x'; super::FILE_ANALYSIS_BYTES + 1]);
        let (header, reached_eof) =
            read_byte_header(&mut long).expect("in-memory read cannot fail");
        assert_eq!(header.len(), super::FILE_ANALYSIS_BYTES);
        assert!(!reached_eof);
    }
}
