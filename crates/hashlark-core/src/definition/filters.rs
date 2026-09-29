// SPDX-License-Identifier: GPL-3.0-or-later

//! Filters transform extracted text, e.g. `parse_size` or `regex`.

use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use regex::{Regex, RegexBuilder};
use time::OffsetDateTime;
use url::Url;

use super::spec::{FilterArgs, FilterSpec};
use crate::magnet::infohash_from_magnet;

/// Compiled regexes may not exceed this size (guards against pathological
/// patterns in untrusted definitions).
const REGEX_SIZE_LIMIT: usize = 1 << 20;

#[derive(Debug, Clone)]
pub enum Filter {
    Trim,
    Lower,
    Upper,
    /// Digits only: `1,234 seeds` → `1234`.
    ToInt,
    /// `1.4 GiB` → bytes.
    ParseSize,
    /// Parses with a strftime format (or `rfc3339` / `rfc2822`) to RFC 3339.
    Date(String),
    /// `3 days ago`, `yesterday` → RFC 3339.
    RelativeDate,
    /// Unix seconds (or milliseconds) → RFC 3339.
    UnixTs,
    /// Resolves a relative link against the page URL.
    UrlJoin,
    /// Keeps a capture group (default 1).
    Regex {
        re: Regex,
        group: usize,
    },
    Replace {
        from: String,
        to: String,
    },
    ReReplace {
        re: Regex,
        to: String,
    },
    Prepend(String),
    Append(String),
    /// Splits on a separator and keeps one part (negative counts from the end).
    Split {
        sep: String,
        index: i64,
    },
    /// Used when the value is missing or empty.
    Default(String),
    InfohashFromMagnet,
    /// Keeps one query parameter of a URL.
    QueryString(String),
}

pub fn compile_all(specs: &[FilterSpec]) -> Result<Vec<Filter>, String> {
    specs.iter().map(compile).collect()
}

fn one_arg(name: &str, args: &FilterArgs) -> Result<String, String> {
    match args {
        FilterArgs::One(s) => Ok(s.clone()),
        FilterArgs::Number(n) => Ok(n.to_string()),
        FilterArgs::Many(v) if v.len() == 1 => Ok(v[0].clone()),
        FilterArgs::Many(_) => Err(format!("filter `{name}` takes one argument")),
    }
}

fn two_args(name: &str, args: &FilterArgs) -> Result<(String, String), String> {
    match args {
        FilterArgs::Many(v) if v.len() == 2 => Ok((v[0].clone(), v[1].clone())),
        _ => Err(format!("filter `{name}` takes two arguments: [a, b]")),
    }
}

fn regex(pattern: &str) -> Result<Regex, String> {
    RegexBuilder::new(pattern)
        .size_limit(REGEX_SIZE_LIMIT)
        .build()
        .map_err(|e| format!("invalid regex `{pattern}`: {e}"))
}

fn compile(spec: &FilterSpec) -> Result<Filter, String> {
    match spec {
        FilterSpec::Name(name) => match name.as_str() {
            "trim" => Ok(Filter::Trim),
            "lower" => Ok(Filter::Lower),
            "upper" => Ok(Filter::Upper),
            "to_int" => Ok(Filter::ToInt),
            "parse_size" => Ok(Filter::ParseSize),
            "relative_date" => Ok(Filter::RelativeDate),
            "unix_ts" => Ok(Filter::UnixTs),
            "urljoin" => Ok(Filter::UrlJoin),
            "infohash_from_magnet" => Ok(Filter::InfohashFromMagnet),
            "date" => Ok(Filter::Date("auto".into())),
            other => Err(format!(
                "unknown filter `{other}` (or it needs arguments, e.g. `{other}: ...`)"
            )),
        },
        FilterSpec::WithArgs(map) => {
            let mut entries = map.iter();
            let (Some((name, args)), None) = (entries.next(), entries.next()) else {
                return Err("a filter with arguments must have exactly one key".into());
            };
            match name.as_str() {
                "regex" => match args {
                    FilterArgs::Many(v) if v.len() == 2 => Ok(Filter::Regex {
                        re: regex(&v[0])?,
                        group: v[1]
                            .parse()
                            .map_err(|_| "regex group must be a number".to_owned())?,
                    }),
                    _ => Ok(Filter::Regex {
                        re: regex(&one_arg(name, args)?)?,
                        group: 1,
                    }),
                },
                "replace" => {
                    let (from, to) = two_args(name, args)?;
                    if from.is_empty() {
                        return Err("replace: the text to replace must not be empty".into());
                    }
                    Ok(Filter::Replace { from, to })
                }
                "re_replace" => {
                    let (pattern, to) = two_args(name, args)?;
                    Ok(Filter::ReReplace {
                        re: regex(&pattern)?,
                        to,
                    })
                }
                "prepend" => Ok(Filter::Prepend(one_arg(name, args)?)),
                "append" => Ok(Filter::Append(one_arg(name, args)?)),
                "default" => Ok(Filter::Default(one_arg(name, args)?)),
                "date" => Ok(Filter::Date(one_arg(name, args)?)),
                "querystring" => Ok(Filter::QueryString(one_arg(name, args)?)),
                "split" => {
                    let (sep, index) = two_args(name, args)?;
                    Ok(Filter::Split {
                        sep,
                        index: index
                            .parse()
                            .map_err(|_| "split index must be a number".to_owned())?,
                    })
                }
                other => Err(format!("unknown filter `{other}`")),
            }
        }
    }
}

/// Runs `filters` over `value`. `base` resolves relative links.
pub fn apply(filters: &[Filter], mut value: Option<String>, base: &Url) -> Option<String> {
    for filter in filters {
        value = apply_one(filter, value, base);
    }
    value.filter(|v| !v.is_empty())
}

fn apply_one(filter: &Filter, value: Option<String>, base: &Url) -> Option<String> {
    if let Filter::Default(default) = filter {
        return match value {
            Some(v) if !v.trim().is_empty() => Some(v),
            _ => Some(default.clone()),
        };
    }
    let v = value?;
    match filter {
        Filter::Trim => Some(v.trim().to_owned()),
        Filter::Lower => Some(v.to_lowercase()),
        Filter::Upper => Some(v.to_uppercase()),
        Filter::ToInt => to_int(&v).map(|n| n.to_string()),
        Filter::ParseSize => parse_size(&v).map(|n| n.to_string()),
        Filter::Date(format) => parse_date_with(&v, format).map(rfc3339),
        Filter::RelativeDate => parse_relative_date(&v, OffsetDateTime::now_utc()).map(rfc3339),
        Filter::UnixTs => unix_ts(&v).map(rfc3339),
        Filter::UrlJoin => base.join(v.trim()).ok().map(String::from),
        Filter::Regex { re, group } => re
            .captures(&v)
            .and_then(|c| c.get(*group))
            .map(|m| m.as_str().to_owned()),
        Filter::Replace { from, to } => Some(v.replace(from.as_str(), to)),
        Filter::ReReplace { re, to } => Some(re.replace_all(&v, to.as_str()).into_owned()),
        Filter::Prepend(prefix) => Some(format!("{prefix}{v}")),
        Filter::Append(suffix) => Some(format!("{v}{suffix}")),
        Filter::Split { sep, index } => {
            let parts: Vec<&str> = v.split(sep.as_str()).collect();
            let i = if *index < 0 {
                parts.len().checked_sub(index.unsigned_abs() as usize)?
            } else {
                usize::try_from(*index).ok()?
            };
            parts.get(i).map(|p| p.trim().to_owned())
        }
        Filter::InfohashFromMagnet => infohash_from_magnet(&v).map(|h| h.to_hex()),
        Filter::QueryString(param) => base
            .join(v.trim())
            .ok()?
            .query_pairs()
            .find(|(k, _)| k == param)
            .map(|(_, val)| val.into_owned()),
        Filter::Default(_) => unreachable!("handled above"),
    }
}

/// Digits of `s` as a number: `1,234` → 1234. `None` if there are none.
pub fn to_int(s: &str) -> Option<u64> {
    let digits: String = s.chars().filter(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// `1.4 GiB`, `700MB`, `1,234.5 KB`, `1,5 GB`, `123456` → bytes. Decimal
/// and binary units are both treated as powers of 1024, as torrent sites do.
pub fn parse_size(s: &str) -> Option<u64> {
    let s = s.trim();
    let split = s
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == ',' || c == ' '))
        .unwrap_or(s.len());
    let (number, unit) = s.split_at(split);
    let number = number.replace(' ', "");
    let number = match (number.contains('.'), number.rfind(',')) {
        // "1,5" → decimal comma; "1,234" or "1,234.5" → thousands separators.
        (false, Some(i)) if number.len() - i - 1 != 3 => number.replace(',', "."),
        _ => number.replace(',', ""),
    };
    let value: f64 = number.parse().ok()?;
    let multiplier: f64 = match unit.trim().to_ascii_lowercase().trim_end_matches('s') {
        "" | "b" | "byte" => 1.0,
        "k" | "kb" | "kib" => 1024.0,
        "m" | "mb" | "mib" => 1024f64.powi(2),
        "g" | "gb" | "gib" => 1024f64.powi(3),
        "t" | "tb" | "tib" => 1024f64.powi(4),
        "p" | "pb" | "pib" => 1024f64.powi(5),
        _ => return None,
    };
    let bytes = value * multiplier;
    (bytes.is_finite() && bytes >= 0.0).then(|| bytes.round() as u64)
}

fn rfc3339(t: OffsetDateTime) -> String {
    t.format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

fn from_chrono(t: DateTime<Utc>) -> Option<OffsetDateTime> {
    OffsetDateTime::from_unix_timestamp(t.timestamp()).ok()
}

fn unix_ts(s: &str) -> Option<OffsetDateTime> {
    let n: i64 = s.trim().parse().ok()?;
    // Values this large are milliseconds.
    let secs = if n.abs() > 100_000_000_000 {
        n / 1000
    } else {
        n
    };
    OffsetDateTime::from_unix_timestamp(secs).ok()
}

fn parse_date_with(s: &str, format: &str) -> Option<OffsetDateTime> {
    let s = s.trim();
    match format {
        "auto" => parse_date_any(s),
        "rfc3339" => DateTime::parse_from_rfc3339(s)
            .ok()
            .and_then(|d| from_chrono(d.to_utc())),
        "rfc2822" => DateTime::parse_from_rfc2822(s)
            .ok()
            .and_then(|d| from_chrono(d.to_utc())),
        fmt => DateTime::parse_from_str(s, fmt)
            .map(|d| d.to_utc())
            .or_else(|_| NaiveDateTime::parse_from_str(s, fmt).map(|d| d.and_utc()))
            .or_else(|_| {
                NaiveDate::parse_from_str(s, fmt)
                    .map(|d| d.and_hms_opt(0, 0, 0).expect("midnight").and_utc())
            })
            .ok()
            .and_then(from_chrono),
    }
}

/// Tries every common date format, then relative dates.
pub fn parse_date_any(s: &str) -> Option<OffsetDateTime> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if s.chars().all(|c| c.is_ascii_digit()) {
        return unix_ts(s);
    }
    for fmt in ["rfc3339", "rfc2822"] {
        if let Some(t) = parse_date_with(s, fmt) {
            return Some(t);
        }
    }
    const FORMATS: [&str; 10] = [
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d",
        "%Y/%m/%d",
        "%d.%m.%Y",
        "%b %d, %Y",
        "%B %d, %Y",
        "%d %b %Y",
        "%d %B %Y",
    ];
    FORMATS
        .iter()
        .find_map(|fmt| parse_date_with(s, fmt))
        .or_else(|| parse_relative_date(s, OffsetDateTime::now_utc()))
}

/// `3 days ago`, `an hour ago`, `1 day 2 hours ago`, `yesterday`, `today`.
pub fn parse_relative_date(s: &str, now: OffsetDateTime) -> Option<OffsetDateTime> {
    use std::sync::LazyLock;
    static UNIT: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)(\d+(?:\.\d+)?|an?|one)\s*(second|sec|minute|min|hour|hr|day|week|wk|month|mo|year|yr)s?\b")
            .expect("valid regex")
    });
    let lower = s.trim().to_lowercase();
    match lower.as_str() {
        "now" | "just now" | "today" => return Some(now),
        "yesterday" | "y-day" => return Some(now - time::Duration::days(1)),
        _ => {}
    }
    let mut total = 0.0f64;
    let mut found = false;
    for cap in UNIT.captures_iter(&lower) {
        let amount = match &cap[1] {
            "a" | "an" | "one" => 1.0,
            n => n.parse().ok()?,
        };
        let secs = match &cap[2] {
            "second" | "sec" => 1.0,
            "minute" | "min" => 60.0,
            "hour" | "hr" => 3600.0,
            "day" => 86_400.0,
            "week" | "wk" => 7.0 * 86_400.0,
            "month" | "mo" => 30.0 * 86_400.0,
            _ => 365.0 * 86_400.0,
        };
        total += amount * secs;
        found = true;
    }
    found.then(|| now - time::Duration::seconds_f64(total))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(yaml_filters: &str, input: &str) -> Option<String> {
        let specs: Vec<FilterSpec> = serde_saphyr::from_str(yaml_filters).unwrap();
        let filters = compile_all(&specs).unwrap();
        apply(
            &filters,
            Some(input.to_owned()),
            &"https://site.example/a/b".parse().unwrap(),
        )
    }

    #[test]
    fn sizes() {
        assert_eq!(parse_size("1.5 GiB"), Some(1_610_612_736));
        assert_eq!(parse_size("700MB"), Some(734_003_200));
        assert_eq!(parse_size("1,234.5 KB"), Some(1_264_128));
        assert_eq!(parse_size("1,5 GB"), Some(1_610_612_736));
        assert_eq!(parse_size("123456"), Some(123_456));
        assert_eq!(parse_size("704.0 MB"), Some(738_197_504));
        assert_eq!(parse_size("12 bytes"), Some(12));
        assert_eq!(parse_size("big"), None);
        assert_eq!(parse_size("3 parsecs"), None);
    }

    #[test]
    fn dates() {
        let t = parse_date_any("Wed, 11 Mar 2026 22:11:37 -0300").unwrap();
        assert_eq!(rfc3339(t), "2026-03-12T01:11:37Z");
        assert_eq!(
            rfc3339(parse_date_any("Feb 15, 2026").unwrap()),
            "2026-02-15T00:00:00Z"
        );
        assert_eq!(
            rfc3339(parse_date_any("2024-09-14 17:49:52").unwrap()),
            "2024-09-14T17:49:52Z"
        );
        assert_eq!(
            rfc3339(parse_date_any("1700000000").unwrap()),
            "2023-11-14T22:13:20Z"
        );
        assert_eq!(
            rfc3339(parse_date_any("1700000000000").unwrap()),
            "2023-11-14T22:13:20Z"
        );
        assert!(parse_date_any("not a date").is_none());

        let now = OffsetDateTime::from_unix_timestamp(1_000_000_000).unwrap();
        let ago = |s| parse_relative_date(s, now).map(|t| (now - t).whole_seconds());
        assert_eq!(ago("3 days ago"), Some(3 * 86_400));
        assert_eq!(ago("an hour ago"), Some(3600));
        assert_eq!(ago("1 day 2 hours ago"), Some(86_400 + 7200));
        assert_eq!(ago("Yesterday"), Some(86_400));
        assert_eq!(ago("no idea"), None);
    }

    #[test]
    fn filter_pipeline() {
        assert_eq!(run("[trim, lower]", "  Hello "), Some("hello".into()));
        assert_eq!(run("[to_int]", "1,234 seeds"), Some("1234".into()));
        assert_eq!(run("[parse_size]", "1 KiB"), Some("1024".into()));
        assert_eq!(
            run(
                r#"[{regex: "/torrents/([0-9a-f]{40})/"}]"#,
                "/torrents/fc399543377017c7a7379994fd1188cd3a575775/"
            ),
            Some("fc399543377017c7a7379994fd1188cd3a575775".into())
        );
        assert_eq!(run(r#"[{regex: ["(a)(b)", "2"]}]"#, "ab"), Some("b".into()));
        assert_eq!(
            run(r#"[{replace: [",", ""]}]"#, "1,2,3"),
            Some("123".into())
        );
        assert_eq!(
            run(r#"[{re_replace: ["\\s+", " "]}]"#, "a   b"),
            Some("a b".into())
        );
        assert_eq!(
            run(r#"[{prepend: "x"}, {append: "z"}]"#, "y"),
            Some("xyz".into())
        );
        assert_eq!(
            run(r#"[{split: ["|", "-1"]}]"#, "a | b | c"),
            Some("c".into())
        );
        assert_eq!(run(r#"[{split: ["|", "5"]}]"#, "a|b"), None);
        assert_eq!(
            run("[urljoin]", "../x.torrent"),
            Some("https://site.example/x.torrent".into())
        );
        assert_eq!(
            run(r#"[{querystring: "id"}]"#, "/dl.php?id=42&x=1"),
            Some("42".into())
        );
        assert_eq!(
            run(r#"[{date: "%d.%m.%Y"}]"#, "15.02.2026"),
            Some("2026-02-15T00:00:00Z".into())
        );
        assert_eq!(
            run(
                "[infohash_from_magnet]",
                "magnet:?xt=urn:btih:FC399543377017C7A7379994FD1188CD3A575775"
            ),
            Some("fc399543377017c7a7379994fd1188cd3a575775".into())
        );
    }

    #[test]
    fn default_fills_missing_values() {
        let filters = compile_all(&[FilterSpec::WithArgs(
            [("default".to_owned(), FilterArgs::One("0".into()))].into(),
        )])
        .unwrap();
        let base: Url = "https://x.example".parse().unwrap();
        assert_eq!(apply(&filters, None, &base), Some("0".into()));
        assert_eq!(apply(&filters, Some("  ".into()), &base), Some("0".into()));
        assert_eq!(apply(&filters, Some("5".into()), &base), Some("5".into()));
    }

    #[test]
    fn bad_filters_are_rejected() {
        let bad = |yaml: &str| {
            let specs: Vec<FilterSpec> = serde_saphyr::from_str(yaml).unwrap();
            compile_all(&specs).unwrap_err()
        };
        assert!(bad("[nope]").contains("unknown filter"));
        assert!(bad("[regex]").contains("needs arguments"));
        assert!(bad(r#"[{regex: "("}]"#).contains("invalid regex"));
        assert!(bad(r#"[{replace: "x"}]"#).contains("two arguments"));
    }
}
