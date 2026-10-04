//! Wall-clock instants as unix milliseconds.

use std::ops::{Add, Sub};
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Unix time in milliseconds.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct Millis(pub u64);

impl Add<Duration> for Millis {
    type Output = Millis;
    fn add(self, rhs: Duration) -> Millis {
        let ms = u64::try_from(rhs.as_millis()).unwrap_or(u64::MAX);
        Millis(self.0.saturating_add(ms))
    }
}

impl Sub<Millis> for Millis {
    type Output = Duration;
    /// Time between two instants; zero when `rhs` is later.
    fn sub(self, rhs: Millis) -> Duration {
        Duration::from_millis(self.0.saturating_sub(rhs.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_and_sub() {
        assert_eq!(Millis(1_000) + Duration::from_secs(2), Millis(3_000));
        assert_eq!(Millis(3_000) - Millis(1_000), Duration::from_secs(2));
    }

    #[test]
    fn sub_saturates_at_zero() {
        assert_eq!(Millis(1) - Millis(5), Duration::ZERO);
    }
}
