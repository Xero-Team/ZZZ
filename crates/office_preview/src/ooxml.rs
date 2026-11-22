//! OOXML 压缩包条目的读取工具，docx 与 pptx 共用。
//!
//! OOXML 文档是 zip 压缩包，正文 XML 以 deflate 压缩存放。压缩比可以达到
//! 约 1000:1，而 worktree 允许打开的文件本身只有数 MB，所以「文件不大」
//! 并不能推出「解压后不大」：一个几 MB 的 docx 就能解出几十 GB 的 XML。

use std::io::Read;

use anyhow::{Result, ensure};

/// 单个 OOXML 条目解压后的字节上限。
///
/// 取值 32 MiB：正常的 `.docx`/`.pptx` 正文 XML 远小于这个量级（最大的
/// 已知样例在 10 MiB 以内），而超过它的输入只可能是异常或恶意构造的文件，
/// 此时报错比继续分配内存安全。
pub(crate) const MAX_OOXML_ENTRY_BYTES: u64 = 32 * 1024 * 1024;

/// 读取一个 OOXML XML 条目，超过 [`MAX_OOXML_ENTRY_BYTES`] 时报错而不是继续分配。
pub(crate) fn read_ooxml_entry(entry: impl Read) -> Result<String> {
    read_ooxml_entry_with_limit(entry, MAX_OOXML_ENTRY_BYTES)
}

fn read_ooxml_entry_with_limit(entry: impl Read, limit: u64) -> Result<String> {
    let mut xml = Vec::new();
    // 多读一个字节：正好等于 limit 的条目应当被接受，只有真正超限的才报错。
    entry.take(limit + 1).read_to_end(&mut xml)?;
    ensure!(
        xml.len() as u64 <= limit,
        "office document entry is larger than {limit} bytes"
    );
    Ok(String::from_utf8(xml)?)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    #[test]
    fn entry_at_the_limit_is_accepted() {
        let xml = "<w:document/>";
        let entry = Cursor::new(xml.as_bytes().to_vec());
        assert_eq!(
            read_ooxml_entry_with_limit(entry, xml.len() as u64).unwrap(),
            xml
        );
    }

    #[test]
    fn entry_over_the_limit_is_rejected() {
        let entry = Cursor::new(b"<w:document/>".to_vec());
        let error = read_ooxml_entry_with_limit(entry, 4).unwrap_err();
        assert!(error.to_string().contains("larger than 4 bytes"), "{error}");
    }
}
