//! Server resource metrics for terminal mode.
//!
//! The monitor samples a connected session by writing a single, marker-delimited
//! shell command into a hidden terminal and parsing the captured screen text.
//! Keeping the parsing pure and separate from the terminal plumbing makes it
//! unit testable without a live SSH host.

use collections::HashMap;

/// Marker printed immediately before a metrics sample.
pub const METRICS_BEGIN_MARKER: &str = "__ZZZ_MON_BEGIN__";
/// Marker printed immediately after a metrics sample.
pub const METRICS_END_MARKER: &str = "__ZZZ_MON_END__";

/// Nominal number of seconds the sampling command sleeps between its two reads
/// of `/proc`, used to derive CPU and network rates.
pub const SAMPLE_INTERVAL_SECS: f64 = 0.3;

/// The single-line shell command that prints exactly one metrics sample.
///
/// It is deliberately a one-liner: a multi-line paste would let the shell print
/// a prompt between every statement, interleaving prompt text with the tagged
/// output. `stty -echo` suppresses command echo after the first sample so the
/// captured text is dominated by real output.
pub fn sampling_command() -> String {
    format!(
        "stty -echo 2>/dev/null; \
         printf '{begin}\\n'; \
         printf 'HOST '; hostname 2>/dev/null; \
         printf 'OS '; uname -s 2>/dev/null; \
         printf 'LOAD '; cat /proc/loadavg 2>/dev/null; \
         printf 'UPTIME '; cut -d' ' -f1 /proc/uptime 2>/dev/null; \
         printf 'CPU1 '; awk '/^cpu /{{print; exit}}' /proc/stat 2>/dev/null; \
         printf 'NET1\\n'; cat /proc/net/dev 2>/dev/null; \
         printf 'MEM\\n'; cat /proc/meminfo 2>/dev/null; \
         printf 'DISK\\n'; df -kP 2>/dev/null; \
         sleep {interval}; \
         printf 'CPU2 '; awk '/^cpu /{{print; exit}}' /proc/stat 2>/dev/null; \
         printf 'NET2\\n'; cat /proc/net/dev 2>/dev/null; \
         printf '{end}\\n'",
        begin = METRICS_BEGIN_MARKER,
        end = METRICS_END_MARKER,
        interval = SAMPLE_INTERVAL_SECS,
    )
}

/// One snapshot of a remote host's resource usage.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ServerMetrics {
    pub hostname: Option<String>,
    pub os: Option<String>,
    pub uptime_secs: Option<f64>,
    pub load: Option<LoadAverage>,
    /// Busy CPU fraction in `0.0..=1.0`, derived from two `/proc/stat` samples.
    pub cpu_usage: Option<f64>,
    pub memory: Option<MemoryUsage>,
    pub swap: Option<MemoryUsage>,
    pub disks: Vec<DiskUsage>,
    /// Non-loopback interfaces with rates derived from two `/proc/net/dev`
    /// samples.
    pub network: Vec<NetworkInterface>,
}

/// The three standard Unix load averages.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LoadAverage {
    pub one: f64,
    pub five: f64,
    pub fifteen: f64,
}

/// Memory usage in bytes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MemoryUsage {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
}

/// One mounted filesystem.
#[derive(Clone, Debug, PartialEq)]
pub struct DiskUsage {
    pub filesystem: String,
    pub mount_point: String,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
}

/// One non-loopback network interface and its throughput.
#[derive(Clone, Debug, PartialEq)]
pub struct NetworkInterface {
    pub name: String,
    pub receive_bytes_per_sec: f64,
    pub transmit_bytes_per_sec: f64,
}

/// Parses the tagged output produced by [`sampling_command`] into metrics.
///
/// `lines` are the terminal's logical lines in reading order. Only the region
/// between the last begin marker and the following end marker is considered, so
/// earlier samples or shell prompts cannot leak into the result.
pub fn parse_metrics(lines: &[String]) -> Option<ServerMetrics> {
    let begin = lines
        .iter()
        .rposition(|line| line.trim() == METRICS_BEGIN_MARKER)?;
    let end = begin
        + lines[begin..]
            .iter()
            .position(|line| line.trim() == METRICS_END_MARKER)?;
    let body = lines.get(begin + 1..end)?;

    let mut metrics = ServerMetrics::default();
    let mut cpu_before = None;
    let mut cpu_after = None;
    let mut network_before: HashMap<String, (u64, u64)> = HashMap::default();
    let mut network_after: HashMap<String, (u64, u64)> = HashMap::default();
    let mut memory_info: HashMap<String, u64> = HashMap::default();
    let mut section = Section::None;

    for line in body {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("HOST ") {
            metrics.hostname = non_empty(rest);
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("OS ") {
            metrics.os = non_empty(rest);
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("LOAD ") {
            metrics.load = parse_load_average(rest);
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("UPTIME ") {
            metrics.uptime_secs = rest
                .split_whitespace()
                .next()
                .and_then(|value| value.parse().ok());
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("CPU1 ") {
            cpu_before = parse_cpu_times(rest);
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("CPU2 ") {
            cpu_after = parse_cpu_times(rest);
            continue;
        }

        match trimmed {
            "MEM" => {
                section = Section::Memory;
                continue;
            }
            "DISK" => {
                section = Section::Disk;
                continue;
            }
            "NET1" => {
                section = Section::NetworkBefore;
                continue;
            }
            "NET2" => {
                section = Section::NetworkAfter;
                continue;
            }
            _ => {}
        }

        match section {
            Section::None => {}
            Section::Memory => parse_meminfo_line(trimmed, &mut memory_info),
            Section::Disk => {
                if let Some(disk) = parse_disk_line(trimmed) {
                    metrics.disks.push(disk);
                }
            }
            Section::NetworkBefore => parse_network_line(trimmed, &mut network_before),
            Section::NetworkAfter => parse_network_line(trimmed, &mut network_after),
        }
    }

    metrics.cpu_usage = cpu_usage(cpu_before.as_deref(), cpu_after.as_deref());
    metrics.memory = memory_usage(&memory_info);
    metrics.swap = swap_usage(&memory_info);
    metrics
        .disks
        .sort_by_key(|disk| std::cmp::Reverse(disk.total_bytes));

    for (name, (received_after, transmitted_after)) in &network_after {
        if name == "lo" {
            continue;
        }
        let Some((received_before, transmitted_before)) = network_before.get(name) else {
            continue;
        };
        metrics.network.push(NetworkInterface {
            name: name.clone(),
            receive_bytes_per_sec: throughput(*received_after, *received_before),
            transmit_bytes_per_sec: throughput(*transmitted_after, *transmitted_before),
        });
    }
    metrics
        .network
        .sort_by_key(|interface| interface.name.clone());

    (metrics != ServerMetrics::default()).then_some(metrics)
}

/// Which tagged block the parser is currently inside.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Section {
    None,
    Memory,
    Disk,
    NetworkBefore,
    NetworkAfter,
}

fn non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn parse_load_average(value: &str) -> Option<LoadAverage> {
    let mut fields = value.split_whitespace();
    let one = fields.next()?.parse().ok()?;
    let five = fields.next()?.parse().ok()?;
    let fifteen = fields.next()?.parse().ok()?;
    Some(LoadAverage { one, five, fifteen })
}

fn parse_cpu_times(value: &str) -> Option<Vec<u64>> {
    let mut fields = value.split_whitespace();
    if fields.next()? != "cpu" {
        return None;
    }
    let times: Vec<u64> = fields.filter_map(|field| field.parse().ok()).collect();
    (!times.is_empty()).then_some(times)
}

fn cpu_usage(before: Option<&[u64]>, after: Option<&[u64]>) -> Option<f64> {
    let before = before?;
    let after = after?;
    let total_before: u64 = before.iter().sum();
    let total_after: u64 = after.iter().sum();
    let idle_before = before.get(3).copied().unwrap_or(0) + before.get(4).copied().unwrap_or(0);
    let idle_after = after.get(3).copied().unwrap_or(0) + after.get(4).copied().unwrap_or(0);
    let total_delta = total_after.checked_sub(total_before)?;
    if total_delta == 0 {
        return None;
    }
    let idle_delta = idle_after.saturating_sub(idle_before);
    let busy = total_delta.saturating_sub(idle_delta);
    Some((busy as f64 / total_delta as f64).clamp(0.0, 1.0))
}

fn parse_meminfo_line(line: &str, info: &mut HashMap<String, u64>) {
    let Some((key, value)) = line.split_once(':') else {
        return;
    };
    let Some(value) = value
        .split_whitespace()
        .next()
        .and_then(|value| value.parse().ok())
    else {
        return;
    };
    info.insert(key.trim().to_string(), value);
}

fn memory_usage(info: &HashMap<String, u64>) -> Option<MemoryUsage> {
    let total = info.get("MemTotal").copied()?;
    let available = info.get("MemAvailable").copied().or_else(|| {
        let free = info.get("MemFree").copied()?;
        let buffers = info.get("Buffers").copied().unwrap_or(0);
        let cached = info.get("Cached").copied().unwrap_or(0);
        Some(free.saturating_add(buffers).saturating_add(cached))
    })?;
    let available = available.min(total);
    Some(MemoryUsage {
        total_bytes: kib_to_bytes(total),
        used_bytes: kib_to_bytes(total - available),
        available_bytes: kib_to_bytes(available),
    })
}

fn swap_usage(info: &HashMap<String, u64>) -> Option<MemoryUsage> {
    let total = info.get("SwapTotal").copied()?;
    let free = info.get("SwapFree").copied()?;
    let free = free.min(total);
    Some(MemoryUsage {
        total_bytes: kib_to_bytes(total),
        used_bytes: kib_to_bytes(total - free),
        available_bytes: kib_to_bytes(free),
    })
}

fn parse_disk_line(line: &str) -> Option<DiskUsage> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    let (filesystem, blocks, used, available, mount) = (
        *fields.first()?,
        fields.get(1)?,
        fields.get(2)?,
        fields.get(3)?,
        fields.get(5..)?,
    );
    if filesystem == "Filesystem" || mount.is_empty() {
        return None;
    }
    let blocks: u64 = blocks.parse().ok()?;
    let used: u64 = used.parse().ok()?;
    let available: u64 = available.parse().ok()?;
    Some(DiskUsage {
        filesystem: filesystem.to_string(),
        mount_point: mount.join(" "),
        total_bytes: kib_to_bytes(blocks),
        used_bytes: kib_to_bytes(used),
        available_bytes: kib_to_bytes(available),
    })
}

fn parse_network_line(line: &str, interfaces: &mut HashMap<String, (u64, u64)>) {
    let Some((name, counters)) = line.split_once(':') else {
        return;
    };
    let name = name.trim();
    if name.is_empty() {
        return;
    }
    let counters: Vec<u64> = counters
        .split_whitespace()
        .filter_map(|field| field.parse().ok())
        .collect();
    let (Some(received), Some(transmitted)) = (counters.first(), counters.get(8)) else {
        return;
    };
    interfaces.insert(name.to_string(), (*received, *transmitted));
}

fn throughput(after: u64, before: u64) -> f64 {
    after.saturating_sub(before) as f64 / SAMPLE_INTERVAL_SECS
}

fn kib_to_bytes(kib: u64) -> u64 {
    kib.saturating_mul(1024)
}

/// Formats a byte count with binary units, for example `1.5 GiB`.
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// Formats a throughput value as `X/s`, using [`format_bytes`] for the amount.
pub fn format_rate(bytes_per_sec: f64) -> String {
    format!("{}/s", format_bytes(bytes_per_sec.max(0.0) as u64))
}

/// Formats an uptime in seconds as a compact `3d 4h`, `4h 5m`, or `5m` string.
pub fn format_uptime(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    let days = total / 86_400;
    let hours = (total % 86_400) / 3_600;
    let minutes = (total % 3_600) / 60;
    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    }
}
