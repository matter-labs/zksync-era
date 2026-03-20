//! Eager HTTP response decompression service for jsonrpsee HTTP client.
//!
//! `tower_http::decompression::DecompressionBody` is `!Unpin`, which violates
//! jsonrpsee's `HttpClientBuilder::build()` bounds. This module provides a custom
//! tower service that eagerly reads and decompresses the response body, returning
//! `Full<Bytes>` (which is `Unpin`). This is zero-overhead since jsonrpsee already
//! buffers the full response for JSON parsing.

use std::{
    future::Future,
    io::Read,
    pin::Pin,
    task::{Context, Poll},
};

/// Maximum decompressed response size (10 MiB).
/// Matches jsonrpsee's default `TEN_MB_SIZE_BYTES` to prevent decompression bombs.
const MAX_DECOMPRESSED_SIZE: u64 = 10 * 1024 * 1024;

use bytes::Bytes;
use http::{header, HeaderValue, Request, Response};
use http_body_util::{BodyExt, Full};
use jsonrpsee::http_client::transport::Error as TransportError;
use tower::Service;

/// Tower layer that adds eager HTTP response decompression.
#[derive(Clone, Copy, Debug, Default)]
pub struct EagerDecompressionLayer;

impl EagerDecompressionLayer {
    pub fn new() -> Self {
        Self
    }
}

impl<S> tower::Layer<S> for EagerDecompressionLayer {
    type Service = EagerDecompression<S>;

    fn layer(&self, inner: S) -> Self::Service {
        EagerDecompression { inner }
    }
}

/// Tower service that decompresses HTTP responses eagerly.
///
/// On the request path, adds `Accept-Encoding: gzip, zstd`.
/// On the response path, reads the full body, decompresses based on
/// `Content-Encoding`, and returns `Response<Full<Bytes>>`.
#[derive(Clone, Debug)]
pub struct EagerDecompression<S> {
    inner: S,
}

impl<S, ReqBody, ResBody> Service<Request<ReqBody>> for EagerDecompression<S>
where
    S: Service<Request<ReqBody>, Response = Response<ResBody>, Error = TransportError>
        + Clone
        + Send
        + 'static,
    S::Future: Send + 'static,
    ReqBody: Send + 'static,
    ResBody: http_body::Body<Data = Bytes> + Send + Unpin + 'static,
    ResBody::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    type Response = Response<Full<Bytes>>;
    type Error = TransportError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: Request<ReqBody>) -> Self::Future {
        // Add Accept-Encoding header if not already set.
        if !req.headers().contains_key(header::ACCEPT_ENCODING) {
            req.headers_mut().insert(
                header::ACCEPT_ENCODING,
                HeaderValue::from_static("gzip, zstd"),
            );
        }

        let mut inner = self.inner.clone();
        // Swap to use the ready clone (tower Service poll_ready convention).
        std::mem::swap(&mut self.inner, &mut inner);

        Box::pin(async move {
            let response = inner.call(req).await?;
            let (parts, body) = response.into_parts();

            let compressed_bytes = body
                .collect()
                .await
                .map_err(|e| {
                    TransportError::Http(jsonrpsee::core::http_helpers::HttpError::Stream(
                        e.into(),
                    ))
                })?
                .to_bytes();

            let encoding = parts
                .headers
                .get(header::CONTENT_ENCODING)
                .and_then(|v| v.to_str().ok());

            let decompressed = match encoding {
                Some("gzip") => {
                    let decoder = flate2::read::GzDecoder::new(&compressed_bytes[..]);
                    let mut buf = Vec::new();
                    decoder.take(MAX_DECOMPRESSED_SIZE).read_to_end(&mut buf).map_err(|e| {
                        TransportError::Http(
                            jsonrpsee::core::http_helpers::HttpError::Stream(e.into()),
                        )
                    })?;
                    Bytes::from(buf)
                }
                Some("zstd") => {
                    let decoder = zstd::stream::Decoder::new(&compressed_bytes[..]).map_err(|e| {
                        TransportError::Http(
                            jsonrpsee::core::http_helpers::HttpError::Stream(e.into()),
                        )
                    })?;
                    let mut buf = Vec::new();
                    decoder.take(MAX_DECOMPRESSED_SIZE).read_to_end(&mut buf).map_err(|e| {
                        TransportError::Http(
                            jsonrpsee::core::http_helpers::HttpError::Stream(e.into()),
                        )
                    })?;
                    Bytes::from(buf)
                }
                _ => compressed_bytes,
            };

            let mut response = Response::from_parts(parts, Full::new(decompressed));
            // Remove Content-Encoding since we've already decompressed.
            response.headers_mut().remove(header::CONTENT_ENCODING);
            // Remove Content-Length since it referred to the compressed size.
            response.headers_mut().remove(header::CONTENT_LENGTH);
            Ok(response)
        })
    }
}
