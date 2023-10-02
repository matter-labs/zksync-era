use metrics_exporter_prometheus::PrometheusBuilder;
use std::time::Duration;

pub fn with_prometheus<F: FnOnce()>(f: F) {
    println!("Pushing results to Prometheus");

    let endpoint =
        "http://vmagent.stage.matterlabs.corp/api/v1/import/prometheus/metrics/job/vm-benchmark";

    tokio::runtime::Runtime::new().unwrap().block_on(async {
        PrometheusBuilder::new()
            .with_push_gateway(endpoint, Duration::from_millis(100), None, None)
            .unwrap()
            .install()
            .unwrap();

        f();

        println!("Waiting for push to happen...");
        tokio::time::sleep(Duration::from_secs(1)).await;
    });
}
