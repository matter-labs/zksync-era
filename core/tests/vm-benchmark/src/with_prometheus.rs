use std::time::Duration;

use tokio::sync::watch;
use zksync_vlog::prometheus::PrometheusExporterConfig;

pub fn with_prometheus<F: FnOnce()>(f: F) {
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(with_prometheus_async(f));
}

async fn with_prometheus_async<F: FnOnce()>(f: F) {
    println!("Pushing results to Prometheus");

    let endpoint =
        "http://vmagent.stage.matterlabs.corp/api/v1/import/prometheus/metrics/job/vm-benchmark";
    let (stop_sender, stop_receiver) = watch::channel(false);
    let prometheus_config =
        PrometheusExporterConfig::push(endpoint.to_owned(), Duration::from_millis(100));
    tokio::spawn(prometheus_config.run(stop_receiver));

    f();

    println!("Waiting for push to happen...");
    tokio::time::sleep(Duration::from_secs(1)).await;
    stop_sender.send_replace(true);
}
