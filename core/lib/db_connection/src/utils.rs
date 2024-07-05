use std::time::Duration;

use sqlx::{postgres::types::PgInterval, types::chrono::NaiveTime};

use crate::connection::DbMarker;

#[derive(Debug, Clone)]
pub(crate) struct InternalMarker;

impl DbMarker for InternalMarker {}

const MICROSECONDS_IN_A_SECOND: i64 = 1_000_000;
const MICROSECONDS_IN_A_MINUTE: i64 = MICROSECONDS_IN_A_SECOND * 60;
const MICROSECONDS_IN_AN_HOUR: i64 = MICROSECONDS_IN_A_MINUTE * 60;

fn duration_to_naive_time_common(duration: Duration, use_ms: bool) -> NaiveTime {
    let total_ms = duration.as_millis();
    let total_seconds = (total_ms / 1_000) as u32;
    let hours = total_seconds / 3_600;
    let minutes = (total_seconds / 60) % 60;
    let seconds = total_seconds % 60;
    let ms = (total_ms % 1_000) as u32;

    if use_ms {
        NaiveTime::from_hms_milli_opt(hours, minutes, seconds, ms).unwrap()
    } else {
        NaiveTime::from_hms_opt(hours, minutes, seconds).unwrap()
    }
}
pub fn duration_to_naive_time(duration: Duration) -> NaiveTime {
    duration_to_naive_time_common(duration, false)
}

pub fn duration_to_naive_time_ms(duration: Duration) -> NaiveTime {
    duration_to_naive_time_common(duration, true)
}

pub const fn pg_interval_from_duration(processing_timeout: Duration) -> PgInterval {
    PgInterval {
        months: 0,
        days: 0,
        microseconds: processing_timeout.as_micros() as i64,
    }
}

// Note: this conversion purposefully ignores `.days` and `.months` fields of PgInterval.
// The PgIntervals expected are below 24h (represented by `.microseconds`). If that's not the case,
// the function will trim days and months. Use at your own risk.
pub fn naive_time_from_pg_interval(pg_interval: PgInterval) -> NaiveTime {
    NaiveTime::from_hms_micro_opt(
        (pg_interval.microseconds / MICROSECONDS_IN_AN_HOUR) as u32,
        ((pg_interval.microseconds / MICROSECONDS_IN_A_MINUTE) % 60) as u32,
        ((pg_interval.microseconds / MICROSECONDS_IN_A_SECOND) % 60) as u32,
        (pg_interval.microseconds as u32) % 1_000_000,
    )
    .expect("failed to convert PgInterval to NaiveTime")
}
