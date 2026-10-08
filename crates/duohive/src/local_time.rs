//! The local time zone's offset from UTC, for the chat's `10:05` times (P2-D8).
//!
//! std has no time zones, and reading one through libc needs `unsafe` (AGENTS.md). So
//! `date +%z` is asked once at start-up; its `+0700` answer is parsed here.

use std::process::{Command, Stdio};

/// The offset, in seconds east of UTC; 0 when `date` can't tell. `tz` is `$TZ`, passed on
/// as given (unset: the system's zone).
pub fn utc_offset_secs(tz: Option<&str>) -> i32 {
    let mut cmd = Command::new("date");
    cmd.arg("+%z").stdin(Stdio::null()).stderr(Stdio::null());
    if let Some(tz) = tz {
        cmd.env("TZ", tz);
    }
    let Ok(out) = cmd.output() else {
        return 0;
    };
    if !out.status.success() {
        return 0;
    }
    String::from_utf8(out.stdout)
        .ok()
        .and_then(|s| parse_offset(&s))
        .unwrap_or(0)
}

/// `+0700` → 25 200, `-0530` → -19 800; `None` for anything else.
pub fn parse_offset(s: &str) -> Option<i32> {
    let s = s.trim();
    let (sign, digits) = match s.split_at_checked(1)? {
        ("+", d) => (1, d),
        ("-", d) => (-1, d),
        _ => return None,
    };
    if digits.len() != 4 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let hours: i32 = digits.get(..2)?.parse().ok()?;
    let minutes: i32 = digits.get(2..)?.parse().ok()?;
    if hours > MAX_HOURS || minutes >= 60 {
        return None;
    }
    Some(sign * (hours * 3_600 + minutes * 60))
}

/// No zone is more than 14 hours from UTC.
const MAX_HOURS: i32 = 14;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_date_z_output() {
        assert_eq!(parse_offset("+0700\n"), Some(25_200));
        assert_eq!(parse_offset("-0530"), Some(-19_800));
        assert_eq!(parse_offset("+0000"), Some(0));
        assert_eq!(parse_offset("+1400"), Some(50_400));
        for bad in [
            "", "0700", "+07", "+07:00", "+0790", "+2500", "UTC", "+07000",
        ] {
            assert_eq!(parse_offset(bad), None, "{bad}");
        }
    }

    #[test]
    fn a_fixed_zone_gives_its_offset() {
        assert_eq!(utc_offset_secs(Some("UTC")), 0);
        assert_eq!(utc_offset_secs(Some("Etc/GMT-7")), 25_200);
    }
}
