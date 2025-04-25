mod builder;

use std::{path::PathBuf, str::FromStr};

use anyhow::Context;
use zksync_config::configs::ObservabilityConfig;
use zksync_core_leftovers::temp_config_store::read_yaml_repr;
use zksync_types::url::SensitiveUrl;

fn read_ecosystem(path_buf: PathBuf) -> Vec<SensitiveUrl> {
    let paths = std::fs::read_dir(path_buf.join("chains")).unwrap();
    let mut urls = vec![];

    for path in paths {
        let path = path.unwrap().path();
        if path.is_dir() {
            let path = path.join("configs").join("general.yaml");
            let config =
                read_yaml_repr::<zksync_protobuf_config::proto::general::GeneralConfig>(&path)
                    .context("failed decoding general YAML config")
                    .unwrap();
            urls.push(
                SensitiveUrl::from_str(&config.api_config.unwrap().web3_json_rpc.http_url).unwrap(),
            );
        }
    }
    urls
}

fn main() -> anyhow::Result<()> {
    let node = builder::InteropSwitchBuilder::new(read_ecosystem("../../../".into()))?;

    let observability_config = ObservabilityConfig {
        sentry_url: None,
        sentry_environment: None,
        opentelemetry: None,
        log_format: "plain".to_string(),
        log_directives: None,
    };
    let observability_guard = {
        // Observability initialization should be performed within tokio context.
        let _context_guard = node.runtime_handle().enter();
        observability_config.install()?
    };

    let node = node.build()?;
    node.run(observability_guard)?;
    Ok(())
}
