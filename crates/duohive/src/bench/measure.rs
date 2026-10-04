//! What the bench measures about itself: CPU time, peak memory, machine load,
//! and the percentile arithmetic.

use std::mem::MaybeUninit;
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
    fn a_single_sample_is_every_percentile() {
        let p = percentiles(&mut [7.0]);
        assert_eq!((p.p50, p.p95, p.max), (7.0, 7.0, 7.0));
    }
}
