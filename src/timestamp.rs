//! UTC timestamps without a date-time dependency.

use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Timestamp {
    secs: u64,
    micros: u32,
}

impl Timestamp {
    pub fn now() -> Timestamp {
        let since_epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("the clock is after 1970");
        Timestamp {
            secs: since_epoch.as_secs(),
            micros: since_epoch.subsec_micros(),
        }
    }

    /// `2026-10-11T07:04:49.856123Z`
    pub fn rfc3339(&self) -> String {
        let (y, mo, d, h, mi, s) = self.civil();
        format!(
            "{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}.{:06}Z",
            self.micros
        )
    }

    /// `20261011T070449.856123Z`: safe in file names, sorts in time order.
    pub fn compact(&self) -> String {
        let (y, mo, d, h, mi, s) = self.civil();
        format!(
            "{y:04}{mo:02}{d:02}T{h:02}{mi:02}{s:02}.{:06}Z",
            self.micros
        )
    }

    fn civil(&self) -> (i64, u32, u32, u64, u64, u64) {
        let days = (self.secs / 86_400) as i64;
        let rem = self.secs % 86_400;
        let (y, m, d) = civil_from_days(days);
        (y, m, d, rem / 3600, rem % 3600 / 60, rem % 60)
    }
}

/// Days since 1970-01-01 to (year, month, day), proleptic Gregorian.
/// Howard Hinnant's `civil_from_days`.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}
