// SPDX-License-Identifier: GPL-3.0-or-later

//! Human-readable formatting for terminal output.

use time::OffsetDateTime;

/// `1536` → `1.5 KiB`.
pub fn bytes(n: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    let mut value = n as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{n} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// Seeder/leecher counts; unknown shows as `?`.
pub fn count(n: Option<u32>) -> String {
    n.map_or_else(|| "?".into(), |n| n.to_string())
}

/// Age since `published`: `5h`, `3d`, `4mo`, `2y`.
pub fn age(published: OffsetDateTime) -> String {
    let secs = (OffsetDateTime::now_utc() - published)
        .whole_seconds()
        .max(0);
    let (hour, day) = (3600, 86_400);
    match secs {
        s if s < day => format!("{}h", s / hour),
        s if s < 30 * day => format!("{}d", s / day),
        s if s < 365 * day => format!("{}mo", s / (30 * day)),
        s => format!("{}y", s / (365 * day)),
    }
}

/// `1234` → `1.2s`, `250` → `250ms`.
pub fn duration_ms(ms: u64) -> String {
    if ms < 1000 {
        format!("{ms}ms")
    } else {
        format!("{:.1}s", ms as f64 / 1000.0)
    }
}

/// Cuts `s` to at most `max` characters, ending with `…` when cut.
pub fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_owned()
    } else {
        let mut cut: String = s.chars().take(max.saturating_sub(1)).collect();
        cut.push('…');
        cut
    }
}

#[cfg(test)]
mod tests {
    use time::Duration;

    use super::*;

    #[test]
    fn formats_bytes() {
        assert_eq!(bytes(512), "512 B");
        assert_eq!(bytes(1536), "1.5 KiB");
        assert_eq!(bytes(6_114_770_944), "5.7 GiB");
    }

    #[test]
    fn formats_age() {
        let now = OffsetDateTime::now_utc();
        assert_eq!(age(now - Duration::hours(5)), "5h");
        assert_eq!(age(now - Duration::days(3)), "3d");
        assert_eq!(age(now - Duration::days(400)), "1y");
        assert_eq!(
            age(now + Duration::days(1)),
            "0h",
            "future dates clamp to zero"
        );
    }

    #[test]
    fn truncates_on_char_boundaries() {
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(truncate("Ünïcödé title", 5), "Ünïc…");
    }

    #[test]
    fn formats_durations() {
        assert_eq!(duration_ms(250), "250ms");
        assert_eq!(duration_ms(1234), "1.2s");
    }
}
