use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use futures::future;
use http::{Request, Response};
use pin_project_lite::pin_project;
use tower::{util::Either, Service};
use tower_http::{
    cors::{self, Cors, CorsLayer},
    metrics::{
        in_flight_requests::{self, InFlightRequestsCounter},
        InFlightRequests,
    },
};
use zksync_web3_decl::jsonrpsee::server;

use super::metrics::{ApiTransportLabel, API_METRICS};

/// Middleware applying CORS to HTTP requests and adding the transport label to the request extensions.
#[derive(Debug)]
pub(super) struct TransportLayer {
    cors: CorsLayer,
    http_counter: InFlightRequestsCounter,
    ws_counter: InFlightRequestsCounter,
}

impl TransportLayer {
    const COUNTER_INTERVAL: Duration = Duration::from_millis(100);

    pub fn new(cors: CorsLayer) -> Self {
        let http_counter = InFlightRequestsCounter::default();
        let ws_counter = InFlightRequestsCounter::default();
        tokio::spawn(
            http_counter
                .clone()
                .run_emitter(Self::COUNTER_INTERVAL, |count| {
                    API_METRICS.web3_in_flight_requests[&ApiTransportLabel::Http].observe(count);
                    future::ready(())
                }),
        );
        tokio::spawn(
            ws_counter
                .clone()
                .run_emitter(Self::COUNTER_INTERVAL, |count| {
                    API_METRICS.web3_in_flight_requests[&ApiTransportLabel::Ws].observe(count);
                    future::ready(())
                }),
        );

        Self {
            cors,
            http_counter,
            ws_counter,
        }
    }
}

impl<Svc> tower::Layer<Svc> for TransportLayer {
    type Service = WithTransport<Svc>;

    fn layer(&self, inner: Svc) -> Self::Service {
        WithTransport {
            cors: self.cors.layer(inner),
            http_counter: self.http_counter.clone(),
            ws_counter: self.ws_counter.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct WithTransport<Svc> {
    cors: Cors<Svc>,
    http_counter: InFlightRequestsCounter,
    ws_counter: InFlightRequestsCounter,
}

impl<Svc, ReqBody, ResBody> Service<Request<ReqBody>> for WithTransport<Svc>
where
    Svc: Service<Request<ReqBody>, Response = Response<ResBody>, Error = tower::BoxError>,
    ResBody: Default,
{
    type Response = Response<in_flight_requests::ResponseBody<ResBody>>;
    type Error = tower::BoxError;
    type Future =
        in_flight_requests::ResponseFuture<Either<Svc::Future, cors::ResponseFuture<Svc::Future>>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.cors.poll_ready(cx)
    }

    fn call(&mut self, mut req: Request<ReqBody>) -> Self::Future {
        let transport = if server::ws::is_upgrade_request(&req) {
            ApiTransportLabel::Ws
        } else {
            ApiTransportLabel::Http
        };
        req.extensions_mut().insert(transport);

        // CORS is not applied to WS requests.
        let (inner, counter) = match transport {
            ApiTransportLabel::Http => (Either::B(&mut self.cors), &self.http_counter),
            ApiTransportLabel::Ws => (Either::A(self.cors.get_mut()), &self.ws_counter),
        };
        InFlightRequests::new(inner, counter.clone()).call(req)
    }
}

pin_project! {
    #[derive(Debug)]
    pub(super) struct WsSession<Fut> {
        #[pin]
        inner: Fut,
    }
}

impl<Fut: Future> Future for WsSession<Fut> {
    type Output = Fut::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.project().inner.poll(cx)
    }
}
