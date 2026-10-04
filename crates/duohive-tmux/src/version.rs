//! tmux version detection from `tmux -V` output.

/// A parsed `tmux -V` result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TmuxVersion {
    pub major: u32,
    pub minor: u32,
    /// The trimmed output as printed.
    pub raw: String,
}

/// Oldest supported version.
const MIN_SUPPORTED: (u32, u32) = (3, 2);

/// Parses `tmux 3.2a`, `tmux 3.7c`, `tmux next-3.6`, and a major-only `tmux 4` (minor 0).
///
/// `None` means unknown (`tmux master`, `openbsd-7.4`, junk): the caller warns or refuses.
/// A `next-3.2` build reports as 3.2 and so as supported.
#[must_use]
pub fn parse_version(out: &str) -> Option<TmuxVersion> {
    let raw = out.trim();
    let ver = raw.strip_prefix("tmux ")?.trim();
    let ver = ver.strip_prefix("next-").unwrap_or(ver);
    let (major, rest) = ver.split_once('.').unwrap_or((ver, "0"));
    let minor_len = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    Some(TmuxVersion {
        major: major.parse().ok()?,
        minor: rest[..minor_len].parse().ok()?,
        raw: raw.to_owned(),
    })
}

impl TmuxVersion {
    /// True for tmux 3.2 or newer.
    #[must_use]
    pub fn supported(&self) -> bool {
        (self.major, self.minor) >= MIN_SUPPORTED
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_parse() {
        assert_eq!(
            parse_version("tmux 3.2a").map(|v| (v.major, v.minor)),
            Some((3, 2))
        );
        assert_eq!(
            parse_version("tmux 3.7c\n").map(|v| (v.major, v.minor)),
            Some((3, 7))
        );
        assert!(!parse_version("tmux 3.1c").unwrap().supported());
        assert!(parse_version("tmux 3.2a").unwrap().supported());
        assert!(parse_version("tmux 4.0").unwrap().supported());
        assert!(!parse_version("tmux 2.9").unwrap().supported());
        assert_eq!(parse_version("tmux next-3.6").map(|v| v.minor), Some(6));
        assert_eq!(
            parse_version("tmux 3.10").map(|v| (v.major, v.minor)),
            Some((3, 10))
        );
        assert!(parse_version("tmux 3.10").unwrap().supported());
        assert_eq!(
            parse_version("tmux 4").map(|v| (v.major, v.minor)),
            Some((4, 0))
        );
        assert!(parse_version("tmux 4").unwrap().supported());
        assert_eq!(parse_version("tmux next-3.8").map(|v| v.minor), Some(8));
        assert!(parse_version("tmux next-3.2").unwrap().supported());
        assert_eq!(parse_version("openbsd-7.4"), None);
        assert_eq!(parse_version("tmux master"), None);
        assert_eq!(parse_version("garbage"), None);
        assert_eq!(parse_version("tmux 3.2a").unwrap().raw, "tmux 3.2a");
    }
}
