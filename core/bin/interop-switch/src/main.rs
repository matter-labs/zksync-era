mod builder;
use zksync_types::url::SensitiveUrl;

use std::str::FromStr;
use zksync_config::configs::ObservabilityConfig;

fn main() -> anyhow::Result<()> {
    let node = builder::InteropSwitchBuilder::new(vec![
        SensitiveUrl::from_str("http://localhost:8545").unwrap(),
        // SensitiveUrl::from_str("http://localhost:8546").unwrap(),
    ])?;

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
