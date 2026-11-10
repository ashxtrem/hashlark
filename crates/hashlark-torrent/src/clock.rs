// SPDX-License-Identifier: GPL-3.0-or-later

//! The service's clock. The policies never read the time; the service reads
//! it here and passes it in, so tests can set it.

use std::sync::Arc;

use chrono::{Datelike, Timelike};

use crate::policy::schedule::LocalTime;

/// One reading of the clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Moment {
    /// Epoch milliseconds.
    pub ms: i64,
    /// The local wall-clock hour of the week, for the schedule.
    pub local: LocalTime,
}

impl Moment {
    /// A moment at `ms`, on `weekday` (0 = Monday) at `hour` local time.
    pub fn new(ms: i64, weekday: u8, hour: u8) -> Self {
        Self {
            ms,
            local: LocalTime::new(weekday, hour),
        }
    }
}

/// Reads the current time.
pub type Clock = Arc<dyn Fn() -> Moment + Send + Sync>;

/// The real clock: the system time and the local time zone.
pub fn system_clock() -> Clock {
    Arc::new(|| {
        let now = chrono::Local::now();
        Moment {
            ms: now.timestamp_millis(),
            local: LocalTime::new(
                u8::try_from(now.weekday().num_days_from_monday()).unwrap_or(0),
                u8::try_from(now.hour()).unwrap_or(0),
            ),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_clock_reads_a_plausible_time() {
        let m = system_clock()();
        assert!(m.ms > 1_700_000_000_000);
        assert!(m.local.weekday < 7 && m.local.hour < 24);
    }
}
