use super::*;

cfg_if! {
    if #[cfg(all(target_arch = "wasm32", target_os = "unknown"))] {
        use js_sys::Date;

        /// Current wall-clock time as microseconds since the Unix epoch.
        #[must_use]
        pub fn get_raw_timestamp() -> u64 {
            if is_browser() {
                (Date::now() * 1000.0f64) as u64
            } else {
                panic!("WASM requires browser environment");
            }
        }

        /// Format a microsecond Unix timestamp as a UTC string, omitting date fields that match the current time.
        #[must_use]
        pub fn human_timestamp(ts: u64) -> String {
            if is_browser() {
                let now = Date::new_0();
                now.set_time(Date::now());
                let date = Date::new_0();
                date.set_time((ts / 1000u64) as f64);

                let show_year = now.get_utc_full_year() != date.get_utc_full_year();
                let show_month = show_year || now.get_utc_month() != date.get_utc_month();
                let show_date = show_month || now.get_utc_date() != date.get_utc_date();

                let s_year = if show_year {
                    format!("{:04}/",date.get_utc_full_year())
                } else {
                    "".to_owned()
                };
                let s_month = if show_month {
                    format!("{:02}/",date.get_utc_month())
                } else {
                    "".to_owned()
                };
                let s_date = if show_date {
                    format!("{:02}-",date.get_utc_date())
                } else {
                    "".to_owned()
                };
                let s_time = format!("{:02}:{:02}:{:02}.{:04}",
                    date.get_utc_hours(),
                    date.get_utc_minutes(),
                    date.get_utc_seconds(),
                    date.get_utc_milliseconds()
                );

                format!("{}{}{}{}",
                    s_year,
                    s_month,
                    s_date,
                    s_time
                )
            } else {
                panic!("WASM requires browser environment");
            }
        }
    } else {
        use std::time::{SystemTime, UNIX_EPOCH};

        /// Current wall-clock time as microseconds since the Unix epoch.
        #[must_use]
        pub fn get_raw_timestamp() -> u64 {
            match SystemTime::now().duration_since(UNIX_EPOCH) {
                Ok(n) => n.as_micros() as u64,
                Err(_) => panic!("SystemTime before UNIX_EPOCH!"),
            }
        }

        // Days since 1970-01-01 to (year, month, day); Howard Hinnant's civil_from_days
        fn civil_from_days(days: i64) -> (i32, u32, u32) {
            let z = days + 719468;
            let era = if z >= 0 { z } else { z - 146096 } / 146097;
            let doe = (z - era * 146097) as u64;
            let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
            let y = yoe as i64 + era * 400;
            let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
            let mp = (5 * doy + 2) / 153;
            let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
            let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
            ((y + if m <= 2 { 1 } else { 0 }) as i32, m, d)
        }

        // (year, month, day, hour, minute, second, millisecond) in UTC
        fn utc_fields(micros: u64) -> (i32, u32, u32, u32, u32, u32, u32) {
            let secs = micros / 1_000_000;
            let (year, month, day) = civil_from_days((secs / 86400) as i64);
            let tod = secs % 86400;
            (
                year,
                month,
                day,
                (tod / 3600) as u32,
                (tod % 3600 / 60) as u32,
                (tod % 60) as u32,
                ((micros % 1_000_000) / 1000) as u32,
            )
        }

        /// Format a microsecond Unix timestamp as a UTC string, omitting date fields that match the current time.
        #[must_use]
        pub fn human_timestamp(ts: u64) -> String {
            let (n_year, n_month, n_day, ..) = utc_fields(get_raw_timestamp());
            let (year, month, day, hour, minute, second, millis) = utc_fields(ts);

            let show_year = n_year != year;
            let show_month = show_year || n_month != month;
            let show_date = show_month || n_day != day;

            let s_year = if show_year {
                format!("{:04}/", year)
            } else {
                "".to_owned()
            };
            let s_month = if show_month {
                format!("{:02}/", month)
            } else {
                "".to_owned()
            };
            let s_date = if show_date {
                format!("{:02}-", day)
            } else {
                "".to_owned()
            };
            let s_time = format!("{:02}:{:02}:{:02}.{:04}",
                hour,
                minute,
                second,
                millis
            );
            format!("{}{}{}{}",
                s_year,
                s_month,
                s_date,
                s_time)
        }
    }
}

const DAY: u64 = 1_000_000u64 * 60 * 60 * 24;
const HOUR: u64 = 1_000_000u64 * 60 * 60;
const MIN: u64 = 1_000_000u64 * 60;
const SEC: u64 = 1_000_000u64;
const MSEC: u64 = 1_000u64;

/// Format a microsecond duration as a human-readable string, e.g. `1d2h3m4.005s` or `1.005ms`.
#[must_use]
pub fn human_duration(dur: u64) -> String {
    let days = dur / DAY;
    let dur = dur % DAY;
    let hours = dur / HOUR;
    let dur = dur % HOUR;
    let mins = dur / MIN;
    let dur = dur % MIN;
    let secs = dur / SEC;
    let dur = dur % SEC;
    let msecs = dur / MSEC;
    let dur = dur % MSEC;

    // microseconds format
    if days == 0 && hours == 0 && mins == 0 && secs == 0 {
        format!("{}.{:03}ms", msecs, dur)
    } else {
        format!(
            "{}{}{}{}.{:03}s",
            if days != 0 {
                format!("{}d", days)
            } else {
                "".to_owned()
            },
            if hours != 0 {
                format!("{}h", hours)
            } else {
                "".to_owned()
            },
            if mins != 0 {
                format!("{}m", mins)
            } else {
                "".to_owned()
            },
            secs,
            msecs
        )
    }
}

/// Parse a duration string with `h`/`m`/`s` suffixes into microseconds; returns `None` on any unexpected character.
#[must_use]
pub fn parse_duration(s: &str) -> Option<u64> {
    let mut dur_total: u64 = 0;
    let mut dur: u64 = 0;
    for c in s.as_bytes() {
        match c {
            b'0'..=b'9' => {
                dur *= 10;
                dur += (c - b'0') as u64;
            }
            b'h' => {
                dur *= 3_600_000u64;
                dur_total += dur;
                dur = 0;
            }
            b'm' => {
                dur *= 60_000u64;
                dur_total += dur;
                dur = 0;
            }
            b's' => {
                dur *= 1_000u64;
                dur_total += dur;
                dur = 0;
            }
            _ => return None,
        }
    }
    dur_total += dur;
    Some(dur_total * 1_000u64)
}
