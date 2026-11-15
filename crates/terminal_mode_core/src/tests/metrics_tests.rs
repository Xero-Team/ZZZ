use crate::metrics::{
    METRICS_BEGIN_MARKER, METRICS_END_MARKER, ServerMetrics, format_bytes, format_rate,
    format_uptime, parse_metrics,
};

fn sample_lines() -> Vec<String> {
    [
        METRICS_BEGIN_MARKER,
        "HOST web-1",
        "OS Linux",
        "LOAD 0.50 0.40 0.30 1/234 5678",
        "UPTIME 12345.67",
        "CPU1 cpu  100 0 50 1000 20 5 5 0 0 0",
        "NET1",
        "Inter-|   Receive                                                |  Transmit",
        " face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed",
        "    lo: 1000 0 0 0 0 0 0 0 1000 0 0 0 0 0 0 0",
        "  eth0: 2000 0 0 0 0 0 0 0 3000 0 0 0 0 0 0 0",
        "MEM",
        "MemTotal:       16384000 kB",
        "MemFree:         2048000 kB",
        "MemAvailable:    8192000 kB",
        "Buffers:          100000 kB",
        "Cached:          2000000 kB",
        "SwapTotal:       4096000 kB",
        "SwapFree:        2048000 kB",
        "DISK",
        "Filesystem     1024-blocks     Used Available Capacity Mounted on",
        "/dev/sda1        41152736 12345678  26700000      32% /",
        "tmpfs              100000        0    100000       0% /run",
        "CPU2 cpu  150 0 60 1400 30 5 5 0 0 0",
        "NET2",
        "Inter-|   Receive                                                |  Transmit",
        " face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed",
        "    lo: 1500 0 0 0 0 0 0 0 1500 0 0 0 0 0 0 0",
        "  eth0: 2600 0 0 0 0 0 0 0 3300 0 0 0 0 0 0 0",
        METRICS_END_MARKER,
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

#[test]
fn parses_a_complete_sample() {
    let metrics = parse_metrics(&sample_lines()).expect("sample must parse");

    assert_eq!(metrics.hostname.as_deref(), Some("web-1"));
    assert_eq!(metrics.os.as_deref(), Some("Linux"));
    assert_eq!(metrics.uptime_secs, Some(12345.67));

    let load = metrics.load.expect("load");
    assert!((load.one - 0.50).abs() < 1e-9);
    assert!((load.five - 0.40).abs() < 1e-9);
    assert!((load.fifteen - 0.30).abs() < 1e-9);

    let cpu = metrics.cpu_usage.expect("cpu");
    assert!((cpu - 60.0 / 470.0).abs() < 1e-9);

    let memory = metrics.memory.expect("memory");
    assert_eq!(memory.total_bytes, 16_384_000 * 1024);
    assert_eq!(memory.available_bytes, 8_192_000 * 1024);
    assert_eq!(memory.used_bytes, (16_384_000 - 8_192_000) * 1024);

    let swap = metrics.swap.expect("swap");
    assert_eq!(swap.total_bytes, 4_096_000 * 1024);
    assert_eq!(swap.used_bytes, 2_048_000 * 1024);

    assert_eq!(metrics.disks.len(), 2);
    assert_eq!(metrics.disks[0].mount_point, "/");
    assert_eq!(metrics.disks[0].total_bytes, 41_152_736 * 1024);
    assert_eq!(metrics.disks[0].used_bytes, 12_345_678 * 1024);

    assert_eq!(metrics.network.len(), 1);
    assert_eq!(metrics.network[0].name, "eth0");
    assert!((metrics.network[0].receive_bytes_per_sec - 2000.0).abs() < 1e-6);
    assert!((metrics.network[0].transmit_bytes_per_sec - 1000.0).abs() < 1e-6);
}

#[test]
fn ignores_text_outside_the_markers() {
    let mut lines = vec!["user@host:~$ stale output".to_string()];
    lines.extend(sample_lines());
    lines.push("user@host:~$".to_string());

    let metrics = parse_metrics(&lines).expect("sample must parse");
    assert_eq!(metrics.hostname.as_deref(), Some("web-1"));
}

#[test]
fn returns_none_without_markers() {
    let lines = vec!["no metrics here".to_string()];
    assert_eq!(parse_metrics(&lines), None);
}

#[test]
fn missing_proc_files_yield_no_metrics() {
    let lines = vec![
        METRICS_BEGIN_MARKER.to_string(),
        "HOST web-1".to_string(),
        METRICS_END_MARKER.to_string(),
    ];
    let metrics = parse_metrics(&lines).expect("host line still parses");
    assert_eq!(metrics.hostname.as_deref(), Some("web-1"));
    assert_eq!(metrics.cpu_usage, None);
    assert_eq!(metrics.memory, None);
    assert_eq!(
        metrics,
        ServerMetrics {
            hostname: Some("web-1".to_string()),
            ..ServerMetrics::default()
        }
    );
}

#[test]
fn formats_byte_counts_and_rates() {
    assert_eq!(format_bytes(0), "0 B");
    assert_eq!(format_bytes(512), "512 B");
    assert_eq!(format_bytes(1536), "1.5 KiB");
    assert_eq!(format_bytes(5 * 1024 * 1024), "5.0 MiB");
    assert_eq!(format_rate(2048.0), "2.0 KiB/s");
}

#[test]
fn formats_uptime() {
    assert_eq!(format_uptime(90.0), "1m");
    assert_eq!(format_uptime(3_600.0), "1h 0m");
    assert_eq!(format_uptime(90_000.0), "1d 1h");
}
