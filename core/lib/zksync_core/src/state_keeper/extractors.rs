//! Pure functions that convert data as required by the state keeper.

use std::{convert::TryFrom, fmt};

use chrono::{DateTime, TimeZone, Utc};

/// Displays a Unix timestamp (seconds since epoch) in human-readable form. Useful for logging.
pub(super) fn display_timestamp(timestamp: u64) -> impl fmt::Display {
    enum DisplayedTimestamp {
        Parsed(DateTime<Utc>),
        Raw(u64),
    }

    impl fmt::Display for DisplayedTimestamp {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::Parsed(timestamp) => fmt::Display::fmt(timestamp, formatter),
                Self::Raw(raw) => write!(formatter, "(raw: {raw})"),
            }
        }
    }

    let parsed = i64::try_from(timestamp).ok();
    let parsed = parsed.and_then(|ts| Utc.timestamp_opt(ts, 0).single());
    parsed.map_or(
        DisplayedTimestamp::Raw(timestamp),
        DisplayedTimestamp::Parsed,
    )
}
