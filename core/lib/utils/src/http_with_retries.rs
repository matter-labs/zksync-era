use reqwest::header::HeaderMap;
use reqwest::{Client, Error, Method, Response};
use tokio::time::{sleep, Duration};

#[derive(Debug)]
pub enum HttpError {
    ReqwestError(Error),
    RetryExhausted(String),
}

/// Method to send HTTP request with fixed number of retires with exponential back-offs.
pub async fn send_request_with_retries(
    url: &str,
    max_retries: usize,
    method: Method,
    headers: Option<HeaderMap>,
    body: Option<Vec<u8>>,
) -> Result<Response, HttpError> {
    let mut retries = 0usize;
    let mut delay = Duration::from_secs(1);
    loop {
        let result = send_request(url, method.clone(), headers.clone(), body.clone()).await;
        match result {
            Ok(response) if response.status().is_success() => return Ok(response),
            Ok(response) => {
                tracing::error!("Received non OK http response {:?}", response.status())
            }
            Err(err) => tracing::error!("Error while sending http request {:?}", err),
        }
        if retries >= max_retries {
            return Err(HttpError::RetryExhausted(format!(
                "All {} http retires failed",
                max_retries
            )));
        }
        retries += 1;
        sleep(delay).await;
        delay = delay.checked_mul(2).unwrap_or(Duration::MAX);
    }
}

async fn send_request(
    url: &str,
    method: Method,
    headers: Option<HeaderMap>,
    body: Option<Vec<u8>>,
) -> Result<Response, Error> {
    let client = Client::new();
    let mut request = client.request(method, url);

    if let Some(headers) = headers {
        request = request.headers(headers);
    }

    if let Some(body) = body {
        request = request.body(body);
    }

    let request = request.build()?;
    let response = client.execute(request).await?;
    Ok(response)
}
