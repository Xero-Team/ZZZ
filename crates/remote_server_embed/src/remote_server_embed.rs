/// Returns the compressed remote server archive for `os`/`arch`, if this ZZZ
/// binary was built with that platform embedded.
///
/// The second value is the filename extension: `gz` on Unix, `zip` on Windows.
pub fn compressed_archive(os: &str, arch: &str) -> Option<(&'static [u8], &'static str)> {
    #![allow(clippy::match_single_binding)]
    include!(concat!(env!("OUT_DIR"), "/embedded_remote_servers.rs"))
}

#[cfg(test)]
mod tests {
    use super::compressed_archive;

    #[test]
    fn unknown_platform_returns_none() {
        assert!(compressed_archive("plan9", "riscv").is_none());
    }
}
