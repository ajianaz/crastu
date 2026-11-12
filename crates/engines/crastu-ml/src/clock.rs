//! The time source of the registry, injectable so tests need no sleeping.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// A monotonic clock: the time since an arbitrary fixed origin.
pub trait Clock: Send + Sync {
    fn now(&self) -> Duration;
}

/// The real clock.
#[derive(Debug)]
pub struct SystemClock {
    origin: Instant,
}

impl Default for SystemClock {
    fn default() -> Self {
        Self {
            origin: Instant::now(),
        }
    }
}

impl Clock for SystemClock {
    fn now(&self) -> Duration {
        self.origin.elapsed()
    }
}

/// A clock that only moves when told to.
#[derive(Debug, Default)]
pub struct ManualClock {
    nanos: AtomicU64,
}

impl ManualClock {
    pub fn advance(&self, by: Duration) {
        let nanos = u64::try_from(by.as_nanos()).unwrap_or(u64::MAX);
        self.nanos.fetch_add(nanos, Ordering::SeqCst);
    }
}

impl Clock for ManualClock {
    fn now(&self) -> Duration {
        Duration::from_nanos(self.nanos.load(Ordering::SeqCst))
    }
}
