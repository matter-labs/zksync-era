#[macro_export]
macro_rules! metrics_middleware {
    ($methods:ident, $middleware:ident, $prefix:literal) => {
        #[derive(Debug, vise::Metrics)]
        #[metrics(prefix = $prefix)]
        pub(crate) struct ApiMethodMetrics {
            #[metrics(labels = ["method", "status"], buckets = vise::Buckets::LATENCIES)]
            pub call_latency: vise::LabeledFamily<($methods, u16), vise::Histogram<std::time::Duration>, 2>,
        }

        #[vise::register]
        pub(crate) static METRICS: vise::Global<ApiMethodMetrics> = vise::Global::new();

        #[derive(Debug)]
        pub(crate) struct $middleware {
            method: $methods,
            started_at: tokio::time::Instant,
        }

        impl $middleware {
            pub fn new(method: $methods) -> Self {
                MetricsMiddleware {
                    method,
                    started_at: tokio::time::Instant::now(),
                }
            }

            pub fn observe(&self, status_code: axum::http::StatusCode) {
                METRICS.call_latency[&(self.method, status_code.as_u16())]
                    .observe(self.started_at.elapsed());
            }
        }
    };
}



#[macro_export]
macro_rules! create_middleware_factory {
    ($methods:ident, $middleware:ident) => {
        |method: $methods| {
            axum::middleware::from_fn(move |req: axum::extract::Request, next: axum::middleware::Next| async move {
                let middleware = $middleware::new(method);
                let response = next.run(req).await;
                middleware.observe(response.status());
                response
            })
        };
    };
}