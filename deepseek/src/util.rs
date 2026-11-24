//! Small formatting helpers shared by templates and stat cards.

/// Human-readable integer with thin spaces: `12345` → `12 345`.
pub fn thousands(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (idx, ch) in digits.chars().enumerate() {
        if idx > 0 && (digits.len() - idx).is_multiple_of(3) {
            out.push('\u{202f}');
        }
        out.push(ch);
    }
    out
}

/// Compact form for large counters: `12.3k`, `1.2M`.
pub fn compact(value: u64) -> String {
    if value < 1_000 {
        return value.to_string();
    }
    let (scaled, suffix) = if value < 1_000_000 {
        (value as f64 / 1_000.0, "k")
    } else {
        (value as f64 / 1_000_000.0, "M")
    };
    let rounded = (scaled * 10.0).round() / 10.0;
    let text = if rounded.fract() == 0.0 {
        format!("{rounded:.0}")
    } else {
        format!("{rounded:.1}")
    };
    format!("{text}{suffix}")
}

/// Accepts only absolute http(s) URLs; blanks and odd schemes are dropped.
pub fn safe_link(raw: Option<&str>) -> Option<String> {
    let raw = raw?.trim();
    if raw.is_empty() {
        return None;
    }
    let candidate = if raw.starts_with("http://") || raw.starts_with("https://") {
        raw.to_owned()
    } else if raw.contains("://") {
        return None;
    } else {
        format!("https://{raw}")
    };
    let parsed = url::Url::parse(&candidate).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return None;
    }
    Some(parsed.to_string())
}

/// Turns `%%`-free text into a stable anchor id (`C++` → `cpp`).
pub fn slug(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if ch == '#' && !out.is_empty() {
            out.push('s');
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_owned()
}

/// Formats ISO-8601 timestamps as `12 мар 2024` style short dates.
pub fn short_date(iso: Option<&str>) -> String {
    use chrono::{DateTime, Utc};
    let Some(iso) = iso else {
        return "—".to_owned();
    };
    match DateTime::parse_from_rfc3339(iso) {
        Ok(dt) => dt.with_timezone(&Utc).format("%d.%m.%Y").to_string(),
        Err(_) => "—".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thousands_groups_digits() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1_000), "1\u{202f}000");
        assert_eq!(thousands(12_345_678), "12\u{202f}345\u{202f}678");
    }

    #[test]
    fn compact_shortens() {
        assert_eq!(compact(999), "999");
        assert_eq!(compact(1_500), "1.5k");
        assert_eq!(compact(12_000), "12k");
        assert_eq!(compact(2_500_000), "2.5M");
    }

    #[test]
    fn safe_link_filters_schemes() {
        assert_eq!(
            safe_link(Some("octocat.example")).as_deref(),
            Some("https://octocat.example/")
        );
        assert!(safe_link(Some("javascript:alert(1)")).is_none());
        assert!(safe_link(Some("  ")).is_none());
        assert!(safe_link(None).is_none());
        assert!(safe_link(Some("https://github.com/octocat")).is_some());
    }

    #[test]
    fn slug_is_stable() {
        assert_eq!(slug("C++"), "c");
        assert_eq!(slug("Jupyter Notebook"), "jupyter-notebook");
        assert_eq!(slug("C#"), "cs");
    }

    #[test]
    fn short_date_handles_bad_input() {
        assert_eq!(short_date(Some("2024-03-12T10:00:00Z")), "12.03.2024");
        assert_eq!(short_date(Some("nope")), "—");
        assert_eq!(short_date(None), "—");
    }
}
