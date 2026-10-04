//! What the bench measures about itself: CPU time, peak memory, machine load,
//! and the percentile arithmetic.

use std::mem::MaybeUninit;
use std::process::Command;
use std::time::Duration;

use super::report::Percentiles;

/// `getrusage(RUSAGE_SELF)`, or `None` when the call fails.
#[expect(unsafe_code, reason = "getrusage has no safe wrapper in std")]
fn rusage() -> Option<libc::rusage> {
    let mut usage = MaybeUninit::<libc::rusage>::zeroed();
    // SAFETY: `usage` is a valid, writable `rusage`; `getrusage` only fills it in.
    let rc = unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) };
    // SAFETY: zero-initialised above (all-zero is a valid `rusage`), and filled on success.
    (rc == 0).then(|| unsafe { usage.assume_init() })
}

fn timeval(tv: libc::timeval) -> Duration {
    let secs = u64::try_from(tv.tv_sec).unwrap_or(0);
    let micros = u64::try_from(tv.tv_usec).unwrap_or(0);
    Duration::from_secs(secs) + Duration::from_micros(micros)
}

/// This process's user plus system CPU time so far.
pub fn cpu_time() -> Option<Duration> {
    let usage = rusage()?;
    Some(timeval(usage.ru_utime) + timeval(usage.ru_stime))
}

/// CPU used over `wall`, as a percentage of one core.
pub fn cpu_percent(used: Duration, wall: Duration) -> f64 {
    if wall.is_zero() {
        return 0.0;
    }
    used.as_secs_f64() / wall.as_secs_f64() * 100.0
}

/// This process's peak resident set size in MB (`ru_maxrss`: bytes on macOS, KiB elsewhere).
pub fn peak_rss_mb() -> f64 {
    let Some(usage) = rusage() else { return 0.0 };
    let raw = u64::try_from(usage.ru_maxrss).unwrap_or(0) as f64;
    let bytes = if cfg!(target_os = "macos") {
        raw
    } else {
        raw * 1024.0
    };
    bytes / (1024.0 * 1024.0)
}

/// The 1, 5 and 15 minute load averages, when the system reports them.
#[expect(unsafe_code, reason = "getloadavg has no safe wrapper in std")]
pub fn load_average() -> Option<[f64; 3]> {
    let mut loads = [0.0_f64; 3];
    // SAFETY: `loads` has room for the 3 samples asked for.
    let got = unsafe { libc::getloadavg(loads.as_mut_ptr(), 3) };
    (got == 3).then_some(loads)
}

/// Another process's CPU time so far and current resident memory.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProcStats {
    /// User plus system CPU time (`ps` `time`: centiseconds on macOS, whole seconds on Linux).
    pub cpu: Duration,
    /// Resident set size now, in MB.
    pub rss_mb: f64,
}

/// `[[dd-]hh:]mm:ss[.ss]`, as `ps` prints CPU time.
pub fn parse_ps_time(text: &str) -> Option<Duration> {
    let text = text.trim();
    let (days, clock) = match text.split_once('-') {
        Some((d, rest)) => (d.parse::<u64>().ok()?, rest),
        None => (0, text),
    };
    let mut secs = 0.0_f64;
    for part in clock.split(':') {
        secs = secs * 60.0 + part.parse::<f64>().ok().filter(|v| *v >= 0.0)?;
    }
    Some(Duration::from_secs_f64(secs + days as f64 * 86_400.0))
}

/// What `ps` says about `pid`, or `None` when it is gone or `ps` fails.
pub fn process_stats(pid: u32) -> Option<ProcStats> {
    let out = Command::new("ps")
        .args(["-o", "rss=,time=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let (rss, time) = text.trim().split_once(char::is_whitespace)?;
    Some(ProcStats {
        cpu: parse_ps_time(time)?,
        rss_mb: rss.trim().parse::<f64>().ok()? / 1024.0,
    })
}

/// The pids of this process's children whose command line mentions `needle`.
pub fn child_pids_matching(needle: &str) -> Vec<u32> {
    let me = std::process::id();
    let Ok(out) = Command::new("ps")
        .args(["-axo", "pid=,ppid=,command="])
        .output()
    else {
        return Vec::new();
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| {
            let mut it = line.split_whitespace();
            let pid: u32 = it.next()?.parse().ok()?;
            let ppid: u32 = it.next()?.parse().ok()?;
            (ppid == me && line.contains(needle)).then_some(pid)
        })
        .collect()
}

/// The `p`-quantile (0..=1) of sorted samples, by linear interpolation.
fn quantile(sorted: &[f64], p: f64) -> f64 {
    let Some(last) = sorted.len().checked_sub(1) else {
        return 0.0;
    };
    let rank = p * last as f64;
    let low = rank.floor();
    let frac = rank - low;
    let at = |i: f64| sorted.get(i as usize).copied().unwrap_or(0.0);
    at(low) + (at(low + 1.0).max(at(low)) - at(low)) * frac
}

/// p50, p95 and max of `samples` (sorted in place); all zero when there are none.
pub fn percentiles(samples: &mut [f64]) -> Percentiles {
    samples.sort_by(f64::total_cmp);
    Percentiles {
        p50: quantile(samples, 0.50),
        p95: quantile(samples, 0.95),
        max: samples.last().copied().unwrap_or(0.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_process_reports_cpu_and_memory() {
        assert!(cpu_time().is_some());
        assert!(peak_rss_mb() > 0.0);
        assert!(
            (cpu_percent(Duration::from_millis(50), Duration::from_secs(10)) - 0.5).abs() < 1e-9
        );
        assert_eq!(cpu_percent(Duration::from_secs(1), Duration::ZERO), 0.0);
    }

    #[test]
    fn ps_times_parse() {
        assert_eq!(parse_ps_time("0:00.12"), Some(Duration::from_millis(120)));
        assert_eq!(
            parse_ps_time(" 1:02.50 "),
            Some(Duration::from_millis(62_500))
        );
        assert_eq!(parse_ps_time("01:02:03"), Some(Duration::from_secs(3723)));
        assert_eq!(
            parse_ps_time("2-01:00:00"),
            Some(Duration::from_secs(176_400))
        );
        assert_eq!(parse_ps_time("junk"), None);
    }

    #[test]
    fn this_process_is_seen_by_ps() {
        let own = process_stats(std::process::id()).unwrap();
        assert!(own.rss_mb > 0.0);
        assert!(process_stats(u32::MAX - 1).is_none());
    }

    #[test]
    fn a_single_sample_is_every_percentile() {
        let p = percentiles(&mut [7.0]);
        assert_eq!((p.p50, p.p95, p.max), (7.0, 7.0, 7.0));
    }
}
