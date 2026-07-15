use anyhow::Result;

/// Initialize the global tracing subscriber for a binary.
///
/// Log verbosity follows the standard `RUST_LOG` environment variable (via
/// `EnvFilter`), defaulting to `info`. The output format is selected by the
/// `LOG_FORMAT` environment variable:
///
/// * `LOG_FORMAT=json` — newline-delimited structured JSON, suitable for log
///   aggregation pipelines (Loki, Datadog, ...).
/// * anything else or unset — the default human-readable text format.
///
/// When the crate's `sentry` feature is enabled (the server binary turns it on),
/// a Sentry layer is also attached so `tracing` errors become Sentry events and
/// lower-level spans become breadcrumbs. The layer is inert until a Sentry
/// client is initialized, so enabling the feature alone has no effect.
pub fn init_tracing() -> Result<()> {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    use tracing_subscriber::{EnvFilter, Layer};

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let json = std::env::var("LOG_FORMAT")
        .map(|value| value.eq_ignore_ascii_case("json"))
        .unwrap_or(false);

    // Box the format layer so the JSON and text variants share one type.
    let fmt_layer = tracing_subscriber::fmt::layer().with_target(false);
    let fmt_layer = if json {
        fmt_layer.json().boxed()
    } else {
        fmt_layer.boxed()
    };

    let subscriber = tracing_subscriber::registry().with(filter).with(fmt_layer);
    #[cfg(feature = "sentry")]
    let subscriber = subscriber.with(sentry::integrations::tracing::layer());

    // `try_init` returns a `TryInitError` that does not implement
    // `std::error::Error`, so `.context()` is unavailable here.
    subscriber
        .try_init()
        .map_err(|err| anyhow::anyhow!("while attempting to initialize tracing subscriber: {err}"))
}
