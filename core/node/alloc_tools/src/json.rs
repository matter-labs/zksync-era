//! Partial JSON model for stats emitted by Jemalloc.

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum JemallocStats {
    Jemalloc {
        stats: GeneralStats,
        #[serde(rename = "stats.arenas")]
        arena_stats: ArenaStatsMap,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct GeneralStats {
    pub allocated: u64,
    pub active: u64,
    pub metadata: u64,
    pub resident: u64,
    pub mapped: u64,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ArenaStatsMap {
    pub merged: ArenaStats,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ArenaStats {
    pub nthreads: u64,
    pub small: OperationStats,
    pub large: OperationStats,
}

#[derive(Debug, Deserialize)]
pub(crate) struct OperationStats {
    pub allocated: u64,
    pub nmalloc: u64,
    pub ndalloc: u64,
    pub nfills: u64,
    pub nflushes: u64,
}
