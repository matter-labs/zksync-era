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
                    TransportError::Http(jsonrpsee::core::http_helpers::HttpError::Stream(e.into()))
                })?
                .to_bytes();

            let encoding = parts
                .headers
                .get(header::CONTENT_ENCODING)
                .and_then(|v| v.to_str().ok());

            let decompressed = match encoding {
                Some("gzip") => {
                    let mut decoder = flate2::read::GzDecoder::new(&compressed_bytes[..]);
                    let mut buf = Vec::new();
                    decoder.read_to_end(&mut buf).map_err(|e| {
                        TransportError::Http(jsonrpsee::core::http_helpers::HttpError::Stream(
                            e.into(),
                        ))
                    })?;
                    Bytes::from(buf)
                }
                Some("zstd") => {
                    let buf = zstd::stream::decode_all(&compressed_bytes[..]).map_err(|e| {
                        TransportError::Http(jsonrpsee::core::http_helpers::HttpError::Stream(
                            e.into(),
                        ))
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

#[cfg(test)]
mod tests {
    use http_body_util::Full;
    use tower::Layer;

    use super::*;

    /// Mock service that returns a response with the given body and headers.
    #[derive(Clone, Debug)]
    struct MockBackend {
        response_body: Bytes,
        content_encoding: Option<&'static str>,
    }

    impl Service<Request<Full<Bytes>>> for MockBackend {
        type Response = Response<Full<Bytes>>;
        type Error = TransportError;
        type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, _req: Request<Full<Bytes>>) -> Self::Future {
            let body = self.response_body.clone();
            let encoding = self.content_encoding;
            Box::pin(async move {
                let mut builder = Response::builder();
                if let Some(enc) = encoding {
                    builder = builder.header(header::CONTENT_ENCODING, enc);
                    builder = builder.header(header::CONTENT_LENGTH, body.len().to_string());
                }
                Ok(builder.body(Full::new(body)).unwrap())
            })
        }
    }

    fn gzip_compress(data: &[u8]) -> Vec<u8> {
        use std::io::Write;

        use flate2::write::GzEncoder;
        let mut encoder = GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(data).unwrap();
        encoder.finish().unwrap()
    }

    fn zstd_compress(data: &[u8]) -> Vec<u8> {
        zstd::stream::encode_all(data, 3).unwrap()
    }

    #[tokio::test]
    async fn passthrough_uncompressed_response() {
        let payload = b"hello world";
        let backend = MockBackend {
            response_body: Bytes::from_static(payload),
            content_encoding: None,
        };
        let mut svc = EagerDecompressionLayer::new().layer(backend);

        let req = Request::builder().body(Full::new(Bytes::new())).unwrap();
        let resp = svc.call(req).await.unwrap();

        assert_eq!(
            resp.into_body().collect().await.unwrap().to_bytes(),
            &payload[..]
        );
    }

    #[tokio::test]
    async fn decompresses_gzip_response() {
        let payload = b"hello gzip world";
        let compressed = gzip_compress(payload);
        let backend = MockBackend {
            response_body: Bytes::from(compressed),
            content_encoding: Some("gzip"),
        };
        let mut svc = EagerDecompressionLayer::new().layer(backend);

        let req = Request::builder().body(Full::new(Bytes::new())).unwrap();
        let resp = svc.call(req).await.unwrap();

        assert_eq!(
            resp.into_body().collect().await.unwrap().to_bytes(),
            &payload[..]
        );
    }

    #[tokio::test]
    async fn decompresses_zstd_response() {
        let payload = b"hello zstd world";
        let compressed = zstd_compress(payload);
        let backend = MockBackend {
            response_body: Bytes::from(compressed),
            content_encoding: Some("zstd"),
        };
        let mut svc = EagerDecompressionLayer::new().layer(backend);

        let req = Request::builder().body(Full::new(Bytes::new())).unwrap();
        let resp = svc.call(req).await.unwrap();

        assert_eq!(
            resp.into_body().collect().await.unwrap().to_bytes(),
            &payload[..]
        );
    }

    #[tokio::test]
    async fn removes_content_encoding_and_content_length_headers() {
        let payload = b"test headers";
        let compressed = gzip_compress(payload);
        let backend = MockBackend {
            response_body: Bytes::from(compressed),
            content_encoding: Some("gzip"),
        };
        let mut svc = EagerDecompressionLayer::new().layer(backend);

        let req = Request::builder().body(Full::new(Bytes::new())).unwrap();
        let resp = svc.call(req).await.unwrap();

        assert!(resp.headers().get(header::CONTENT_ENCODING).is_none());
        assert!(resp.headers().get(header::CONTENT_LENGTH).is_none());
    }

    #[tokio::test]
    async fn adds_accept_encoding_header() {
        /// Mock that captures the request headers.
        #[derive(Clone, Debug)]
        struct HeaderCapture(std::sync::Arc<std::sync::Mutex<Option<HeaderValue>>>);

        impl Service<Request<Full<Bytes>>> for HeaderCapture {
            type Response = Response<Full<Bytes>>;
            type Error = TransportError;
            type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

            fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
                Poll::Ready(Ok(()))
            }

            fn call(&mut self, req: Request<Full<Bytes>>) -> Self::Future {
                let captured = req.headers().get(header::ACCEPT_ENCODING).cloned();
                *self.0.lock().unwrap() = captured;
                Box::pin(async { Ok(Response::new(Full::new(Bytes::new()))) })
            }
        }

        let captured = std::sync::Arc::new(std::sync::Mutex::new(None));
        let backend = HeaderCapture(captured.clone());
        let mut svc = EagerDecompressionLayer::new().layer(backend);

        let req = Request::builder().body(Full::new(Bytes::new())).unwrap();
        let _ = svc.call(req).await.unwrap();

        let header = captured.lock().unwrap().clone().unwrap();
        assert_eq!(header.to_str().unwrap(), "gzip, zstd");
    }

    #[tokio::test]
    async fn does_not_overwrite_existing_accept_encoding() {
        #[derive(Clone, Debug)]
        struct HeaderCapture(std::sync::Arc<std::sync::Mutex<Option<HeaderValue>>>);

        impl Service<Request<Full<Bytes>>> for HeaderCapture {
            type Response = Response<Full<Bytes>>;
            type Error = TransportError;
            type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

            fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
                Poll::Ready(Ok(()))
            }

            fn call(&mut self, req: Request<Full<Bytes>>) -> Self::Future {
                let captured = req.headers().get(header::ACCEPT_ENCODING).cloned();
                *self.0.lock().unwrap() = captured;
                Box::pin(async { Ok(Response::new(Full::new(Bytes::new()))) })
            }
        }

        let captured = std::sync::Arc::new(std::sync::Mutex::new(None));
        let backend = HeaderCapture(captured.clone());
        let mut svc = EagerDecompressionLayer::new().layer(backend);

        let req = Request::builder()
            .header(header::ACCEPT_ENCODING, "br")
            .body(Full::new(Bytes::new()))
            .unwrap();
        let _ = svc.call(req).await.unwrap();

        let header = captured.lock().unwrap().clone().unwrap();
        assert_eq!(header.to_str().unwrap(), "br");
    }

    #[tokio::test]
    async fn decompresses_large_response() {
        // Verify large responses decompress fully (no artificial size limit).
        // jsonrpsee's own max_response_body_size applies downstream.
        let large_data = vec![42u8; 16 * 1024 * 1024]; // 16 MiB
        let compressed = gzip_compress(&large_data);
        let backend = MockBackend {
            response_body: Bytes::from(compressed),
            content_encoding: Some("gzip"),
        };
        let mut svc = EagerDecompressionLayer::new().layer(backend);

        let req = Request::builder().body(Full::new(Bytes::new())).unwrap();
        let resp = svc.call(req).await.unwrap();

        let body = resp.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(body.len(), 16 * 1024 * 1024);
    }

    /// Verifies that the server-side 1KB compression threshold works as configured
    /// in the API server: responses under 1KB are not compressed, responses over 1KB are.
    #[tokio::test]
    async fn server_compression_skips_small_responses() {
        use tower::ServiceExt;
        use tower_http::compression::{
            predicate::{DefaultPredicate, Predicate, SizeAbove},
            CompressionLayer,
        };

        // Same configuration as used in the API server.
        let compression = CompressionLayer::new()
            .no_br()
            .no_deflate()
            .compress_when(DefaultPredicate::new().and(SizeAbove::new(1024)));

        // Small response (< 1KB): should NOT be compressed.
        let small_body = "x".repeat(512);
        let mut svc = tower::ServiceBuilder::new()
            .layer(compression.clone())
            .service_fn(move |_req: Request<Full<Bytes>>| {
                let body = small_body.clone();
                async move {
                    Ok::<_, std::convert::Infallible>(
                        Response::builder()
                            .header(header::CONTENT_TYPE, "application/json")
                            .body(Full::new(Bytes::from(body)))
                            .unwrap(),
                    )
                }
            });
        let req = Request::builder()
            .header(header::ACCEPT_ENCODING, "gzip, zstd")
            .body(Full::new(Bytes::new()))
            .unwrap();
        let resp = svc.ready().await.unwrap().call(req).await.unwrap();
        assert!(
            resp.headers().get(header::CONTENT_ENCODING).is_none(),
            "Small responses (< 1KB) should not be compressed"
        );

        // Large response (> 1KB): should be compressed.
        let large_body = "x".repeat(2048);
        let mut svc = tower::ServiceBuilder::new().layer(compression).service_fn(
            move |_req: Request<Full<Bytes>>| {
                let body = large_body.clone();
                async move {
                    Ok::<_, std::convert::Infallible>(
                        Response::builder()
                            .header(header::CONTENT_TYPE, "application/json")
                            .body(Full::new(Bytes::from(body)))
                            .unwrap(),
                    )
                }
            },
        );
        let req = Request::builder()
            .header(header::ACCEPT_ENCODING, "gzip, zstd")
            .body(Full::new(Bytes::new()))
            .unwrap();
        let resp = svc.ready().await.unwrap().call(req).await.unwrap();
        let encoding = resp
            .headers()
            .get(header::CONTENT_ENCODING)
            .expect("Large responses (> 1KB) should be compressed");
        let enc = encoding.to_str().unwrap();
        assert!(
            enc == "gzip" || enc == "zstd",
            "Expected gzip or zstd encoding, got {enc}"
        );
    }
}
