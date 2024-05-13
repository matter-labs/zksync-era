use std::time::Duration;

use anyhow::Context;
use google_cloud_gax::grpc::{IntoRequest, Status};
use google_cloud_googleapis::pubsub::v1::PubsubMessage;
use google_cloud_pubsub::{
    client::{Client, ClientConfig},
    subscription::SubscriptionConfig,
    topic::TopicConfig,
};
use google_cloud_storage::http::{
    buckets::get::GetBucketRequest,
    objects::{download::Range, get::GetObjectRequest, list::ListObjectsRequest},
};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{info, level_filters::LevelFilter};
use tracing_log::LogTracer;
use tracing_subscriber::{fmt, prelude::*, EnvFilter, Registry};
use zksync_config::{configs::object_store::ObjectStoreMode, ObjectStoreConfig};
use zksync_object_store::ObjectStoreFactory;
use zksync_tee_verifier::TeeVerifierInput;
use zksync_types::L1BatchNumber;
//use futures_util::StreamExt;

async fn run() {
    let config = ClientConfig::default().with_auth().await.unwrap();
    //let _ = publisher(config).await.unwrap();

    // Create pubsub client.
    let client = Client::new(config).await.unwrap();

    let topics = client.get_topics(None).await.unwrap();
    println!("{:?}", topics);

    println!("1");
    // Create topic.
    let topic = client.topic("projects/intercloud-job-queue/topics/batch_verifier");
    println!("1.1");
    if !topic.exists(None).await.unwrap() {
        println!("2");
        topic.create(None, None).await.unwrap();
    }

    println!("2.1");

    // Start publisher.
    let publisher = topic.new_publisher(None);

    // Publish message.
    let tasks: Vec<JoinHandle<Result<String, Status>>> = (0..10)
        .into_iter()
        .map(|_i| {
            let publisher = publisher.clone();
            tokio::spawn(async move {
                let msg = PubsubMessage {
                    data: "abc".into(),
                    // Set ordering_key if needed (https://cloud.google.com/pubsub/docs/ordering)
                    ordering_key: "order".into(),
                    ..Default::default()
                };
                println!("3");

                // Send a message. There are also `publish_bulk` and `publish_immediately` methods.
                let mut awaiter = publisher.publish(msg).await;

                // The get method blocks until a server-generated ID or an error is returned for the published message.
                let a = awaiter.get().await;
                println!("{:?}", a);
                a
            })
        })
        .collect();

    println!("4");

    // Wait for all publish task finish
    for task in tasks {
        let _message_id = task.await.unwrap();
    }

    println!("5");

    // Wait for publishers in topic finish.
    let mut publisher = publisher;
    publisher.shutdown().await;

    println!("6");

    // Create subscription
    // If subscription name does not contain a "/", then the project is taken from client above. Otherwise, the
    // name will be treated as a fully qualified resource name
    let config = SubscriptionConfig {
        // Enable message ordering if needed (https://cloud.google.com/pubsub/docs/ordering)
        enable_message_ordering: true,
        ..Default::default()
    };

    // Create subscription
    let subscription = client.subscription("batch_verifier-sub");
    if !subscription.exists(None).await.unwrap() {
        subscription
            .create(topic.fully_qualified_name(), config, None)
            .await
            .unwrap();
    }

    // Token for cancel.
    let cancel = CancellationToken::new();
    let cancel2 = cancel.clone();
    tokio::spawn(async move {
        // Cancel after 10 seconds.
        tokio::time::sleep(Duration::from_secs(10)).await;
        cancel2.cancel();
    });

    // Receive blocks until the ctx is cancelled or an error occurs.
    // Or simply use the `subscription.subscribe` method.
    subscription
        .receive(
            |mut message, cancel| async move {
                // Handle data.
                println!("Got Message: {:?}", message.message.data);

                // Ack or Nack message.
                let _ = message.ack().await;
            },
            cancel.clone(),
            None,
        )
        .await
        .unwrap();

    // Delete subscription if needed.
    // subscription.delete(None).await.unwrap();
}

async fn publisher(config: ClientConfig) -> Result<(), Status> {
    // Create pubsub client.
    let client = Client::new(config).await.unwrap();

    println!("1");
    // Create topic.
    let topic = client.topic("projects/intercloud-job-queue/topics/batch_verifier");
    println!("1.1");
    if !topic.exists(None).await? {
        println!("2");
        topic.create(None, None).await?;
    }

    println!("2.1");

    // Start publisher.
    let publisher = topic.new_publisher(None);

    // Publish message.
    let tasks: Vec<JoinHandle<Result<String, Status>>> = (0..10)
        .into_iter()
        .map(|_i| {
            let publisher = publisher.clone();
            tokio::spawn(async move {
                let msg = PubsubMessage {
                    data: "abc".into(),
                    // Set ordering_key if needed (https://cloud.google.com/pubsub/docs/ordering)
                    ordering_key: "order".into(),
                    ..Default::default()
                };
                println!("3");

                // Send a message. There are also `publish_bulk` and `publish_immediately` methods.
                let mut awaiter = publisher.publish(msg).await;

                // The get method blocks until a server-generated ID or an error is returned for the published message.
                awaiter.get().await
            })
        })
        .collect();

    println!("4");

    // Wait for all publish task finish
    for task in tasks {
        let _message_id = task.await.unwrap()?;
    }

    println!("5");

    // Wait for publishers in topic finish.
    let mut publisher = publisher;
    publisher.shutdown().await;

    println!("6");

    Ok(())
}

async fn subscriber(config: ClientConfig) -> Result<(), Status> {
    // Create pubsub client.
    let client = Client::new(config).await.unwrap();

    // Get the topic to subscribe to.
    let topic = client.topic("batch_verifier");

    // Create subscription
    // If subscription name does not contain a "/", then the project is taken from client above. Otherwise, the
    // name will be treated as a fully qualified resource name
    let config = SubscriptionConfig {
        // Enable message ordering if needed (https://cloud.google.com/pubsub/docs/ordering)
        enable_message_ordering: true,
        ..Default::default()
    };

    // Create subscription
    let subscription = client.subscription("batch_verifier-sub");
    if !subscription.exists(None).await? {
        subscription
            .create(topic.fully_qualified_name(), config, None)
            .await?;
    }

    // Token for cancel.
    let cancel = CancellationToken::new();
    let cancel2 = cancel.clone();
    tokio::spawn(async move {
        // Cancel after 10 seconds.
        tokio::time::sleep(Duration::from_secs(10)).await;
        cancel2.cancel();
    });

    // Receive blocks until the ctx is cancelled or an error occurs.
    // Or simply use the `subscription.subscribe` method.
    subscription
        .receive(
            |mut message, cancel| async move {
                // Handle data.
                println!("Got Message: {:?}", message.message.data);

                // Ack or Nack message.
                let _ = message.ack().await;
            },
            cancel.clone(),
            None,
        )
        .await?;

    // Delete subscription if needed.
    subscription.delete(None).await?;

    Ok(())
}

async fn run_storage() -> Result<(), Status> {
    let config = google_cloud_storage::client::ClientConfig::default()
        .with_auth()
        .await
        .unwrap();
    let client = google_cloud_storage::client::Client::new(config);

    let result = client
        .get_bucket(&GetBucketRequest {
            bucket: "zksync-tee-prover-inputs".to_string(),
            ..Default::default()
        })
        .await;

    println!("{:?}", result);

    let result = client
        .list_objects(&ListObjectsRequest {
            bucket: "zksync-tee-prover-inputs".to_string(),
            ..Default::default()
        })
        .await;

    println!("{:?}", result);

    let result = client
        .download_object(
            &GetObjectRequest {
                bucket: "zksync-tee-prover-inputs".to_string(),
                object: "gcs.txt".to_string(),
                ..Default::default()
            },
            &Range(Some(0), None),
        )
        .await;

    println!("{}", String::from_utf8(result.unwrap()).unwrap());

    Ok(())
}
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    LogTracer::init().context("Failed to set logger")?;

    // run().await;
    run_storage().await;

    let subscriber = Registry::default()
        .with(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .with(fmt::layer().with_writer(std::io::stderr));
    tracing::subscriber::set_global_default(subscriber).context("Failed to set logger")?;

    let object_store = ObjectStoreFactory::new(ObjectStoreConfig {
        mode: ObjectStoreMode::FileBacked {
            file_backed_base_path: "artifacts".to_string(),
        },
        max_retries: 0,
    })
    .create_store()
    .await;

    for i in 1..u32::MAX {
        object_store
            .get::<TeeVerifierInput>(L1BatchNumber(i))
            .await
            .context(format!("failed to get batch verifier inputs for batch {i}"))?
            .verify()?;
        info!("Successfully validated batch {i}");
    }
    Ok(())
}
