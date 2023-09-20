use tokio::sync::watch;
use vise_exporter::{
    metrics_exporter_prometheus::{Matcher, PrometheusBuilder},
    MetricsExporter,
};

use std::{net::Ipv4Addr, time::Duration};

fn configure_legacy_exporter(builder: PrometheusBuilder) -> PrometheusBuilder {
    // in seconds
    let default_latency_buckets = [0.001, 0.005, 0.025, 0.1, 0.25, 1.0, 5.0, 30.0, 120.0];
    let slow_latency_buckets = [
        0.33, 1.0, 2.0, 5.0, 10.0, 30.0, 60.0, 180.0, 600.0, 1800.0, 3600.0,
    ];
    let prover_buckets = [
        1.0, 10.0, 20.0, 40.0, 60.0, 120.0, 240.0, 360.0, 600.0, 1800.0, 3600.0,
    ];

    let storage_interactions_per_call_buckets = [
        10.0, 100.0, 1000.0, 10000.0, 100000.0, 1000000.0, 10000000.0,
    ];
    let vm_memory_per_call_buckets = [
        1000.0,
        10000.0,
        100000.0,
        500000.0,
        1000000.0,
        5000000.0,
        10000000.0,
        50000000.0,
        100000000.0,
        500000000.0,
        1000000000.0,
    ];
    let percents_buckets = [
        5.0, 10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0, 100.0, 120.0,
    ];
    let zero_to_one_buckets = [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9];

    let around_one_buckets = [
        0.01, 0.03, 0.1, 0.3, 0.5, 0.75, 1., 1.5, 3., 5., 10., 20., 50.,
    ];

    // Buckets for a metric reporting the sizes of the JSON RPC batches sent to the server.
    let batch_size_buckets = [1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0, 128.0, 256.0, 512.0];

    builder
        .set_buckets(&default_latency_buckets)
        .unwrap()
        .set_buckets_for_metric(
            Matcher::Full("runtime_context.storage_interaction.amount".to_owned()),
            &storage_interactions_per_call_buckets,
        )
        .unwrap()
        .set_buckets_for_metric(
            Matcher::Full("runtime_context.storage_interaction.ratio".to_owned()),
            &zero_to_one_buckets,
        )
        .unwrap()
        .set_buckets_for_metric(
            Matcher::Prefix("runtime_context.memory".to_owned()),
            &vm_memory_per_call_buckets,
        )
        .unwrap()
        .set_buckets_for_metric(Matcher::Prefix("server.prover".to_owned()), &prover_buckets)
        .unwrap()
        .set_buckets_for_metric(
            Matcher::Prefix("server.witness_generator".to_owned()),
            &slow_latency_buckets,
        )
        .unwrap()
        .set_buckets_for_metric(Matcher::Prefix("vm.refund".to_owned()), &percents_buckets)
        .unwrap()
        .set_buckets_for_metric(
            Matcher::Full("state_keeper_computational_gas_per_nanosecond".to_owned()),
            &around_one_buckets,
        )
        .unwrap()
        .set_buckets_for_metric(
            Matcher::Full("api.jsonrpc_backend.batch.size".to_owned()),
            &batch_size_buckets,
        )
        .unwrap()
}

#[derive(Debug)]
enum PrometheusTransport {
    Pull {
        port: u16,
    },
    Push {
        gateway_uri: String,
        interval: Duration,
    },
}

/// Configuration of a Prometheus exporter.
#[derive(Debug)]
pub struct PrometheusExporterConfig {
    transport: PrometheusTransport,
    use_new_facade: bool,
}

impl PrometheusExporterConfig {
    /// Creates an exporter that will run an HTTP server on the specified `port`.
    pub const fn pull(port: u16) -> Self {
        Self {
            transport: PrometheusTransport::Pull { port },
            use_new_facade: false,
        }
    }

    /// Creates an exporter that will push metrics to the specified Prometheus gateway endpoint.
    pub const fn push(gateway_uri: String, interval: Duration) -> Self {
        Self {
            transport: PrometheusTransport::Push {
                gateway_uri,
                interval,
            },
            use_new_facade: false,
        }
    }

    /// Enables the new metrics façade (`vise`), which is off by default.
    #[must_use]
    pub fn with_new_facade(self) -> Self {
        Self {
            use_new_facade: true,
            transport: self.transport,
        }
    }

    /// Runs the exporter. This future should be spawned in a separate Tokio task.
    pub async fn run(self, stop_receiver: watch::Receiver<bool>) {
        if self.use_new_facade {
            self.run_with_new_facade(stop_receiver).await;
        } else {
            self.run_without_new_facade().await;
        }
    }

    async fn run_with_new_facade(self, mut stop_receiver: watch::Receiver<bool>) {
        let metrics_exporter = MetricsExporter::default()
            .with_legacy_exporter(configure_legacy_exporter)
            .with_graceful_shutdown(async move {
                stop_receiver.changed().await.ok();
            });

        match self.transport {
            PrometheusTransport::Pull { port } => {
                let prom_bind_address = (Ipv4Addr::UNSPECIFIED, port).into();
                metrics_exporter.start(prom_bind_address).await
            }
            PrometheusTransport::Push {
                gateway_uri,
                interval,
            } => {
                let endpoint = gateway_uri
                    .parse()
                    .expect("Failed parsing Prometheus push gateway endpoint");
                metrics_exporter.push_to_gateway(endpoint, interval).await
            }
        }
    }

    async fn run_without_new_facade(self) {
        let builder = match self.transport {
            PrometheusTransport::Pull { port } => {
                let prom_bind_address = (Ipv4Addr::UNSPECIFIED, port);
                PrometheusBuilder::new().with_http_listener(prom_bind_address)
            }
            PrometheusTransport::Push {
                gateway_uri,
                interval,
            } => PrometheusBuilder::new()
                .with_push_gateway(gateway_uri, interval, None, None)
                .unwrap(),
        };
        let builder = configure_legacy_exporter(builder);
        let (recorder, exporter) = builder.build().unwrap();
        metrics::set_boxed_recorder(Box::new(recorder)).expect("failed to set metrics recorder");
        exporter.await.expect("Prometheus exporter failed");
    }
}
