use axum::{
    extract::Request, middleware::AddExtension, middleware::Next, response::Response, Extension,
};
use axum_server::{accept::Accept, tls_rustls::RustlsAcceptor};
use futures_util::future::BoxFuture;
use rustls::Certificate;
use std::io;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_rustls::server::TlsStream;
use tower::Layer;
use x509_parser::prelude::{FromDer, X509Certificate};

#[derive(Debug, Clone)]
pub struct Auth {
    pub username: String,
}

#[derive(Debug, Clone)]
pub struct TlsData {
    peer_certificates: Option<Vec<Certificate>>,
}

#[derive(Debug, Clone)]
pub struct TLSAcceptor {
    inner: RustlsAcceptor,
}
impl TLSAcceptor {
    pub fn new(inner: RustlsAcceptor) -> Self {
        Self { inner }
    }
}
impl<I, S> Accept<I, S> for TLSAcceptor
where
    I: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    S: Send + 'static,
{
    type Stream = TlsStream<I>;
    type Service = AddExtension<S, TlsData>;
    type Future = BoxFuture<'static, io::Result<(Self::Stream, Self::Service)>>;

    fn accept(&self, stream: I, service: S) -> Self::Future {
        let acceptor = self.inner.clone();

        Box::pin(async move {
            let (stream, service) = acceptor.accept(stream, service).await?;
            let server_conn = stream.get_ref().1;
            let tls_data = TlsData {
                peer_certificates: server_conn.peer_certificates().map(From::from),
            };
            let service = Extension(tls_data).layer(service);

            Ok((stream, service))
        })
    }
}

pub async fn auth_middleware(
    Extension(tls_data): Extension<TlsData>,
    mut request: Request,
    next: Next,
) -> Result<Response, &'static str> {
    if let Some(peer_certificates) = tls_data.peer_certificates {
        // Take first client certificate
        let cert = X509Certificate::from_der(
            &peer_certificates
                .first()
                .ok_or("missing client certificate")?
                .0,
        )
        .map_err(|_| "invalid client certificate")?
        .1;
        // Take first common name from certificate
        let cn = cert
            .subject()
            .iter_common_name()
            .next()
            .ok_or("missing common name in client certificate")?
            .as_str()
            .map_err(|_| "invalid common name in client certificate")?;

        // Pass authentication to routes
        request.extensions_mut().insert(Auth {
            username: cn.to_string(),
        });
    } else {
        return Err("missing client certificate");
    }

    Ok(next.run(request).await)
}
