#[cfg(target_os = "macos")]
use regex::Regex;

#[cfg(target_os = "macos")]
static MACOS_VERSION_REGEX: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r"(\s*\(Build [^)]*[0-9]\))").unwrap());

pub fn os_name() -> String {
    #[cfg(target_os = "macos")]
    {
        "macOS".to_owned()
    }
    #[cfg(target_os = "linux")]
    {
        format!("Linux {}", gpui::guess_compositor())
    }
    #[cfg(target_os = "freebsd")]
    {
        format!("FreeBSD {}", gpui::guess_compositor())
    }

    #[cfg(target_os = "windows")]
    {
        "Windows".to_owned()
    }
}

pub fn os_version() -> String {
    #[cfg(target_os = "macos")]
    {
        use objc2_foundation::NSProcessInfo;
        let process_info = NSProcessInfo::processInfo();
        let version_nsstring = process_info.operatingSystemVersionString();
        let version_string = version_nsstring.to_string().replace("Version ", "");
        MACOS_VERSION_REGEX
            .replace_all(&version_string, "")
            .to_string()
    }
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    {
        use std::path::Path;

        let content = if let Ok(file) = std::fs::read_to_string(&Path::new("/etc/os-release")) {
            file
        } else if let Ok(file) = std::fs::read_to_string(&Path::new("/usr/lib/os-release")) {
            file
        } else if let Ok(file) = std::fs::read_to_string(&Path::new("/var/run/os-release")) {
            file
        } else {
            log::error!(
                "Failed to load /etc/os-release, /usr/lib/os-release, or /var/run/os-release"
            );
            "".to_owned()
        };
        let mut name = "unknown";
        let mut version = "unknown";

        for line in content.lines() {
            match line.split_once('=') {
                Some(("ID", val)) => name = val.trim_matches('"'),
                Some(("VERSION_ID", val)) => version = val.trim_matches('"'),
                _ => {}
            }
        }

        format!("{} {}", name, version)
    }

    #[cfg(target_os = "windows")]
    {
        let mut info = unsafe { std::mem::zeroed() };
        let status = unsafe { windows::Wdk::System::SystemServices::RtlGetVersion(&mut info) };
        if status.is_ok() {
            semver::Version::new(
                info.dwMajorVersion as _,
                info.dwMinorVersion as _,
                info.dwBuildNumber as _,
            )
            .to_string()
        } else {
            "unknown".to_owned()
        }
    }
}
