use std::{env, error, str::FromStr};

use anyhow::Context;
use zksync_config::configs::PrometheusConfig;

use crate::{envy_load, FromEnv};

impl FromEnv for PrometheusConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("prometheus", "API_PROMETHEUS_")
    }
}

pub fn parse_optional_var<T>(name: &str) -> anyhow::Result<Option<T>>
where
    T: FromStr,
    T::Err: 'static + error::Error + Send + Sync,
{
    env::var(name)
        .ok()
        .map(|val| {
            val.parse()
                .with_context(|| format!("failed to parse env variable {name}"))
        })
        .transpose()
}
