use reqwest::{header::HeaderMap, Client, Error, Method, Response, StatusCode};
use tokio::time::{sleep, Duration};

use crate::metrics::AUTOSCALER_METRICS;

#[derive(Clone)]
pub struct HttpClient {
    client: Client,
    max_retries: usize,
}

impl Default for HttpClient {
    fn default() -> Self {
        Self {
            client: Client::new(),
            max_retries: 5,
        }
    }
}

#[derive(Debug)]
pub enum HttpError {
    ReqwestError(Error),
    RetryExhausted(String),
}

impl HttpClient {
    /// Method to send HTTP request with fixed number of retires with exponential back-offs.
    pub async fn send_request_with_retries(
        &self,
        url: &str,
        method: Method,
        headers: Option<HeaderMap>,
        body: Option<Vec<u8>>,
    ) -> Result<Response, HttpError> {
        let mut retries = 0usize;
        let mut delay = Duration::from_secs(1);
        loop {
            let result = self
                .send_request(url, method.clone(), headers.clone(), body.clone())
                .await;
            AUTOSCALER_METRICS.calls[&(
                url.into(),
                match result {
                    Ok(ref response) => response.status().as_u16(),
                    Err(ref err) => err
                        .status()
                        .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
                        .as_u16(),
                },
            )]
                .inc();
            match result {
                Ok(response) if response.status().is_success() => return Ok(response),
                Ok(response) => {
                    tracing::error!("Received non OK http response {:?}", response.status())
                }
                Err(err) => tracing::error!("Error while sending http request {:?}", err),
            }

            if retries >= self.max_retries {
                return Err(HttpError::RetryExhausted(format!(
                    "All {} http retires failed",
                    self.max_retries
                )));
            }
            retries += 1;
            sleep(delay).await;
            delay = delay.checked_mul(2).unwrap_or(Duration::MAX);
        }
    }

    async fn send_request(
        &self,
        url: &str,
        method: Method,
        headers: Option<HeaderMap>,
        body: Option<Vec<u8>>,
    ) -> Result<Response, Error> {
        let mut request = self.client.request(method, url);

        if let Some(headers) = headers {
            request = request.headers(headers);
        }

        if let Some(body) = body {
            request = request.body(body);
        }

        let request = request.build()?;
        let response = self.client.execute(request).await?;
        Ok(response)
    }
}
