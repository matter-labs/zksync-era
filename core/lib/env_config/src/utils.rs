use zksync_config::configs::PrometheusConfig;

use crate::{envy_load, FromEnv};

impl FromEnv for PrometheusConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("prometheus", "API_PROMETHEUS_")
    }
}
