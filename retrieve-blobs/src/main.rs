use std::{collections::HashMap, convert::Infallible, error::Error, time::Duration};

use hyper::{
    service::{make_service_fn, service_fn},
    Body, Request, Response, Server,
};
use prometheus::{register_counter, register_gauge, Counter, Encoder, Gauge, TextEncoder};
use tokio::time::sleep;
use tokio_postgres::NoTls;

// Define Prometheus metrics
lazy_static::lazy_static! {
    static ref BLOB_RETRIEVALS: Counter = register_counter!(
        "blob_retrievals_total",
        "Total number of blobs successfully retrieved"
    ).unwrap();

    static ref BLOB_AVG_SIZE: Gauge = register_gauge!(
        "blob_avg_size",
        "Average size of blobs in bytes"
    ).unwrap();
}

#[tokio::main]
async fn main() {
    // Start the metrics HTTP server
    tokio::spawn(start_metrics_server());

    let mut blobs = HashMap::new();

    loop {
        // Perform blob retrievals
        match get_blobs(&mut blobs).await {
            Ok(_) => println!("Blob retrieval successful"),
            Err(e) => eprintln!("Blob retrieval error: {}", e),
        };
        sleep(Duration::from_secs(1)).await;
    }
}

async fn get_blobs(blobs: &mut HashMap<String, usize>) -> Result<(), Box<dyn Error>> {
    // Connect to the PostgreSQL server
    let (client, connection) = tokio_postgres::connect(
        "host=postgres user=postgres password=notsecurepassword dbname=zksync_local",
        NoTls,
    )
    .await?;

    // Spawn a background task to handle the connection
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("Connection error: {}", e);
        }
    });

    // Run the SELECT query
    let rows = client
        .query("SELECT blob_id FROM data_availability", &[])
        .await?;

    for row in rows {
        let blob_id: &str = row.get(0);
        let blob_id = blob_id.to_string();

        if !blobs.contains_key(&blob_id) {
            let blob = get(blob_id.clone()).await?;
            blobs.insert(blob_id.clone(), blob.len());

            if !blob.is_empty() {
                BLOB_RETRIEVALS.inc(); // Increment counter if blob retrieval succeeds
            }
        }
        BLOB_AVG_SIZE.set(blobs.values().sum::<usize>() as f64 / blobs.len() as f64);
    }

    Ok(())
}

async fn get(commitment: String) -> Result<Vec<u8>, Box<dyn Error>> {
    let url = format!("http://host.docker.internal:4242/get/0x{commitment}");

    let client = reqwest::Client::new();
    let response = client.get(url).send().await?;

    if response.status().is_success() {
        // Expecting the response body to be binary data
        let body = response.bytes().await?;
        Ok(body.to_vec())
    } else {
        Ok(vec![])
    }
}

// Start the Prometheus metrics server
async fn start_metrics_server() {
    let make_svc =
        make_service_fn(|_conn| async { Ok::<_, Infallible>(service_fn(metrics_handler)) });

    let addr = ([0, 0, 0, 0], 7070).into();
    let server = Server::bind(&addr).serve(make_svc);

    println!("Serving metrics on http://{}", addr);
    server.await.unwrap();
}

// Handle the /metrics endpoint
async fn metrics_handler(_: Request<Body>) -> Result<Response<Body>, Infallible> {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = Vec::new();
    encoder.encode(&metric_families, &mut buffer).unwrap();

    Ok(Response::new(Body::from(buffer)))
}
