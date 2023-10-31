use sqlx::postgres::types::PgInterval;
use sqlx::types::chrono::NaiveTime;
use std::time::Duration;

pub fn duration_to_naive_time(duration: Duration) -> NaiveTime {
    let total_seconds = duration.as_secs() as u32;
    NaiveTime::from_hms_opt(
        total_seconds / 3600,
        (total_seconds / 60) % 60,
        total_seconds % 60,
    )
    .unwrap()
}

pub const fn pg_interval_from_duration(processing_timeout: Duration) -> PgInterval {
    PgInterval {
        months: 0,
        days: 0,
        microseconds: processing_timeout.as_micros() as i64,
    }
}
