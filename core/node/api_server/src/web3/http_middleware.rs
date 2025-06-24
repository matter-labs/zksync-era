use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use futures::future::Either;
use http::{Request, Response};
use pin_project_lite::pin_project;
use tower::Service;
use tower_http::cors::{self, Cors, CorsLayer};
use zksync_web3_decl::jsonrpsee::server;

use super::metrics::{ApiTransportLabel, API_METRICS};

/// Middleware applying CORS to HTTP requests and adding the transport label to the request extensions.
#[derive(Debug)]
pub(super) struct TransportLayer {
    pub cors: CorsLayer,
}

impl<Svc> tower::Layer<Svc> for TransportLayer {
    type Service = WithTransport<Svc>;

    fn layer(&self, inner: Svc) -> Self::Service {
        WithTransport {
            cors: self.cors.layer(inner),
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct WithTransport<Svc> {
    cors: Cors<Svc>,
}

impl<Svc, ReqBody, ResBody> Service<Request<ReqBody>> for WithTransport<Svc>
where
    Svc: Service<Request<ReqBody>, Response = Response<ResBody>>,
    ResBody: Default,
{
    type Response = Svc::Response;
    type Error = Svc::Error;
    type Future = Either<WsSession<Svc::Future>, cors::ResponseFuture<Svc::Future>>;

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

        if matches!(transport, ApiTransportLabel::Http) {
            // CORS is not applied to WS requests.
            Either::Right(self.cors.call(req))
        } else {
            Either::Left(WsSession {
                inner: self.cors.get_mut().call(req),
                _guard: API_METRICS.ws_open_sessions.inc_guard(1),
            })
        }
    }
}

pin_project! {
    #[derive(Debug)]
    pub(super) struct WsSession<Fut> {
        #[pin]
        inner: Fut,
        _guard: vise::GaugeGuard,
    }
}

impl<Fut: Future> Future for WsSession<Fut> {
    type Output = Fut::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.project().inner.poll(cx)
    }
}
