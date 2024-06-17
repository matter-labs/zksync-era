extern crate core;

use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
};

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use rand::rngs::OsRng;
use secp256k1::{KeyPair, PublicKey};
use zksync_config::configs::ObservabilityConfig;
use zksync_env_config::FromEnv;
use zksync_tee_verifier::TeeVerifierInput;
use zksync_types::L1BatchNumber;

use crate::api_data_fetcher::{PeriodicApiStruct, PROOF_GENERATION_DATA_PATH, SUBMIT_PROOF_PATH};

mod api_data_fetcher;

// Gramine-specific device file from which the attestation quote can be read
pub(crate) const GRAMINE_ATTESTATION_QUOTE_DEVICE_FILE: &str = "/dev/attestation/quote";

// Gramine-specific device file from which the attestation type can be read
pub(crate) const GRAMINE_ATTESTATION_TYPE_DEVICE_FILE: &str = "/dev/attestation/attestation_type";

// Gramine-specific device file. User data should be written to this file before obtaining the
// attestation quote
pub(crate) const GRAMINE_ATTESTATION_USER_REPORT_DATA_DEVICE_FILE: &str =
    "/dev/attestation/user_report_data";

// TODO see:
// - prover/prover_fri_gateway/src/api_data_fetcher.rs
// - prover/prover_fri_gateway/src/main.rs
// for some inspiration

#[derive(Debug, Parser)]
#[command(author = "Matter Labs", version)]
struct Cli {
    /// In simulation mode, a simulated/mocked attestation is used, read from the
    /// ATTESTATION_QUOTE_FILE environment variable file. To fetch a real attestation, the
    /// application must be run inside an enclave.
    #[arg(long)]
    simulation_mode: bool,

    /// The URL of the TEE prover interface API.
    #[arg(long)]
    endpoint_url: String,
}

fn save_attestation_user_report_data(pubkey: PublicKey) -> Result<()> {
    let mut user_report_data_file = OpenOptions::new()
        .write(true)
        .open(GRAMINE_ATTESTATION_USER_REPORT_DATA_DEVICE_FILE)?;
    user_report_data_file
        .write_all(&pubkey.serialize())
        .map_err(|err| anyhow!("Failed to save user report data: {err}"))
}

fn get_attestation_quote() -> Result<Vec<u8>> {
    let mut quote_file = File::open(GRAMINE_ATTESTATION_QUOTE_DEVICE_FILE)?;
    let mut quote = Vec::new();
    quote_file.read_to_end(&mut quote)?;
    Ok(quote)
}

/// This application is a TEE verifier (a.k.a. a prover, or worker) that interacts with three
/// endpoints of the TEE prover interface API:
/// 1. `/tee/proof_inputs` - Fetches input data about a batch for the TEE verifier to process.
/// 2. `/tee/submit_proofs/<l1_batch_number>` - Submits the TEE proof, which is a signature of the
///    root hash.
/// 3. `/tee/register_attestation` - Registers the TEE attestation that binds a key used to sign a
///    root hash to the enclave where the signing process occurred. This effectively proves that the
///    signature was produced in a trusted execution environment.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opt = Cli::parse();

    let observability_config =
        ObservabilityConfig::from_env().context("ObservabilityConfig::from_env()")?;
    let log_format: vlog::LogFormat = observability_config
        .log_format
        .parse()
        .context("Invalid log format")?;
    let mut builder = vlog::ObservabilityBuilder::new().with_log_format(log_format);
    if let Some(sentry_url) = &observability_config.sentry_url {
        builder = builder
            .with_sentry_url(sentry_url)
            .expect("Invalid Sentry URL")
            .with_sentry_environment(observability_config.sentry_environment);
    }
    let _guard = builder.build();

    if let Some(sentry_url) = observability_config.sentry_url {
        tracing::info!("Sentry configured with URL: {sentry_url}");
    } else {
        tracing::info!("No sentry URL was provided");
    }

    let key_pair = KeyPair::new_global(&mut OsRng);
    let attestation_quote_bytes = if opt.simulation_mode {
        let attestation_quote_file = std::env::var("ATTESTATION_QUOTE_FILE").unwrap_or_default();
        std::fs::read(&attestation_quote_file).unwrap_or_default()
    } else {
        save_attestation_user_report_data(key_pair.public_key())?;
        get_attestation_quote()?
    };

    let proof_gen_data_fetcher = PeriodicApiStruct {
        api_url: format!("{}{PROOF_GENERATION_DATA_PATH}", config.api_url),
        poll_duration: config.api_poll_duration(),
        client: Client::new(),
    };
    let proof_submitter = PeriodicApiStruct {
        api_url: format!("{}{SUBMIT_PROOF_PATH}", config.api_url),
        poll_duration: config.api_poll_duration(),
        client: Client::new(),
    };

    let (stop_sender, stop_receiver) = watch::channel(false);

    let (stop_signal_sender, stop_signal_receiver) = oneshot::channel();
    let mut stop_signal_sender = Some(stop_signal_sender);
    ctrlc::set_handler(move || {
        if let Some(stop_signal_sender) = stop_signal_sender.take() {
            stop_signal_sender.send(()).ok();
        }
    })
    .context("Error setting Ctrl+C handler")?;

    tracing::info!("Starting Fri Prover Gateway");

    let tasks = vec![
        tokio::spawn(
            PrometheusExporterConfig::pull(config.prometheus_listener_port)
                .run(stop_receiver.clone()),
        ),
        tokio::spawn(
            proof_gen_data_fetcher.run::<ProofGenerationDataRequest>(stop_receiver.clone()),
        ),
        tokio::spawn(proof_submitter.run::<SubmitProofRequest>(stop_receiver)),
    ];

    let mut tasks = ManagedTasks::new(tasks);
    tokio::select! {
        _ = tasks.wait_single() => {},
        _ = stop_signal_receiver => {
            tracing::info!("Stop signal received, shutting down");
        }
    }
    stop_sender.send(true).ok();
    tasks.complete(Duration::from_secs(5)).await;

    // 3. Register attestation (input needed: endpoint URL)
    // 4. In a loop until the process is interrupted:
    //    4a. fetch input
    //    4b. process input, generating proof
    //    4c. submit proof

    // TODO: introduce clap parser that allows to run the prover in different modes:
    // - attestation registration with a file as an input
    // - continuous proof inputs fetching and proof submission
    // - proof inputs fetching and proof submission for N batches
    // extra obligatory input parameters: endpoint's URL

    // use reqwest::Error;
    // use tokio::time::{sleep, Duration};

    // #[tokio::main]
    // async fn main() {
    //     // Infinite loop to continuously poll the API
    //     loop {
    //         // Spawn a new task for each polling iteration
    //         tokio::spawn(async {
    //             match fetch_data().await {
    //                 Ok(data) => {
    //                     // Spawn a separate task for processing data
    //                     tokio::spawn(async move {
    //                         process_data(data).await;
    //                     });
    //                 }
    //                 Err(e) => eprintln!("Error fetching data: {}", e),
    //             }
    //         });

    //         // Sleep for some duration before the next poll
    //         sleep(Duration::from_secs(10)).await;
    //     }
    // }

    // // Function to fetch data from an API endpoint
    // async fn fetch_data() -> Result<String, Error> {
    //     let response = reqwest::get("https://api.example.com/data")
    //         .await?
    //         .text()
    //         .await?;
    //     Ok(response)
    // }

    // // Function to process the fetched data
    // async fn process_data(data: String) {
    //     // Simulate some data processing
    //     println!("Processing data: {}", data);

    //     // Simulate a delay in processing
    //     sleep(Duration::from_secs(5)).await;

    //     println!("Finished processing data: {}", data);
    // }

    //////////////////////////////////////////

    // use reqwest::Error;
    // use tokio::signal;
    // use tokio::task;

    // #[tokio::main]
    // async fn main() -> Result<(), Box<dyn std::error::Error>> {
    //     let client = reqwest::Client::new();

    //     let mut tasks = vec![];
    //     let mut number = 1;

    //     let mut termination_signal = signal::ctrl_c();

    //     loop {
    //         tokio::select! {
    //             _ = termination_signal => {
    //                 break;
    //             }
    //             else => {
    //                 let client = client.clone();
    //                 let task = task::spawn(async move {
    //                     let url = format!("http://localhost/endpoint/{}", number);
    //                     let response = client.get(&url).send().await?;
    //                     let process_task = task::spawn(process_data(response));
    //                     process_task.await??;
    //                     Ok(())
    //                 });
    //                 tasks.push(task);
    //                 number += 1;
    //             }
    //         }
    //     }

    //     for task in tasks {
    //         let _ = task.await??;
    //     }

    //     Ok(())
    // }

    // async fn process_data(response: reqwest::Response) -> Result<(), Error> {
    //     // Here you can process the data from the response
    //     // This is just a placeholder
    //     println!("Status: {}", response.status());
    //     Ok(())
    // }

    //////
    //     let signing_key_pem = std::env::var("ATTESTATION_QUOTE_FILE").unwrap_or(
    //         r#"-----BEGIN PRIVATE KEY-----
    // MIGEAgEAMBAGByqGSM49AgEGBSuBBAAKBG0wawIBAQQgtQs4yNOWyIco/AMuzlWO
    // valpB6CxqTQCiXFe73vyneuhRANCAARXX55cR5OHwxskdsKKBBalUBgCAU+oKIl7
    // K678gpQwmzwbZqqiuNpWxeEu+51Nh3tmPPJGTW9Y0BCNC0if7cxA
    // -----END PRIVATE KEY-----
    // "#
    //         .into(),
    //     );
    //     let signing_key: SigningKey = SigningKey::from_pkcs8_pem(&signing_key_pem).unwrap();
    //     let _verifying_key_bytes = signing_key.verifying_key().to_sec1_bytes();

    // TEST TEST
    // {
    //     use k256::ecdsa::signature::Verifier;
    //     let vkey: VerifyingKey = VerifyingKey::try_from(_verifying_key_bytes.as_ref()).unwrap();
    //     let signature: Signature = signing_key.try_sign(&[0, 0, 0, 0]).unwrap();
    //     let sig_bytes = signature.to_vec();
    //     let signature: Signature = Signature::try_from(sig_bytes.as_ref()).unwrap();
    //     let _ = vkey.verify(&[0, 0, 0, 0], &signature).unwrap();
    // }
    // END TEST

    // let tst_tvi_json = r#"
    //         {
    //             "V1": {
    //                 "prepare_basic_circuits_job": {
    //                     "merkle_paths": [
    //                         {
    //                             "root_hash": [
    //                                 199, 231, 138, 237, 215, 168, 130, 194, 198, 6, 187, 237, 77, 26, 152, 210,
    //                                 88, 244, 103, 217, 198, 89, 54, 183, 3, 48, 12, 198, 157, 109, 17, 108
    //                             ],
    //                             "is_write": false,
    //                             "first_write": false,
    //                             "merkle_paths": [
    //                                 [
    //                                     14, 61, 115, 101, 43, 176, 68, 16, 107, 44, 117, 212, 243, 107, 174, 139,
    //                                     221, 199, 237, 48, 120, 145, 101, 195, 53, 184, 23, 176, 118, 216, 58, 141
    //                                 ]
    //                             ],
    //                             "leaf_hashed_key": "0xa792adc37510103905c79c23e63fc13938000f8acb1120dd8cc76d6f13a11577",
    //                             "leaf_enumeration_index": 2,
    //                             "value_written": [
    //                                 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    //                                 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
    //                             ],
    //                             "value_read": [
    //                                 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    //                                 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
    //                             ]
    //                         }
    //                     ],
    //                     "next_enumeration_index": 23577
    //                 },
    //                 "l2_blocks_execution_data": [
    //                     {
    //                         "number": 91,
    //                         "timestamp": 1715693104,
    //                         "prev_block_hash": "0x1ef787bfb1908c8a55d972375af69b74038bfb999cc4f46d5be582d945a3b2be",
    //                         "virtual_blocks": 1,
    //                         "txs": [
    //                             {
    //                                 "common_data": {
    //                                     "L1": {
    //                                         "sender": "0x62b13dd4f940b691a015d1b1e29cecd3cfec5d77",
    //                                         "serialId": 208,
    //                                         "deadlineBlock": 0,
    //                                         "layer2TipFee": "0x0",
    //                                         "fullFee": "0x0",
    //                                         "maxFeePerGas": "0x10642ac0",
    //                                         "gasLimit": "0x4c4b40",
    //                                         "gasPerPubdataLimit": "0x320",
    //                                         "opProcessingType": "Common",
    //                                         "priorityQueueType": "Deque",
    //                                         "ethHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
    //                                         "ethBlock": 1727,
    //                                         "canonicalTxHash": "0x8f53c4d042e7931cbe43889f7992a86cd9d24db76ac0ff2dc4ceb55250059a92",
    //                                         "toMint": "0x4e28e2290f000",
    //                                         "refundRecipient": "0x62b13dd4f940b691a015d1b1e29cecd3cfec5d77"
    //                                     }
    //                                 },
    //                                 "execute": {
    //                                     "contractAddress": "0x350822d8850e1ce8894e4bb86ed7243baaa747fc",
    //                                     "calldata": "0xd542b16c000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000000a00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000001",
    //                                     "value": "0x0",
    //                                     "factoryDeps": [ [ 0 ] ]
    //                                 },
    //                                 "received_timestamp_ms": 1715693103904,
    //                                 "raw_bytes": null
    //                             }
    //                         ]
    //                     },
    //                     {
    //                         "number": 95,
    //                         "timestamp": 1715693108,
    //                         "prev_block_hash": "0x3ca40be0a54a377be77bd6ad87e376ae7ddb25090d2d32382e3c94759d1fc76d",
    //                         "virtual_blocks": 1,
    //                         "txs": []
    //                     }
    //                 ],
    //                 "l1_batch_env": {
    //                     "previous_batch_hash": "0xc7e78aedd7a882c2c606bbed4d1a98d258f467d9c65936b703300cc69d6d116c",
    //                     "number": 20,
    //                     "timestamp": 1715693104,
    //                     "fee_input": {
    //                         "PubdataIndependent": {
    //                             "fair_l2_gas_price": 100000000,
    //                             "fair_pubdata_price": 13600000000,
    //                             "l1_gas_price": 800000000
    //                         }
    //                     },
    //                     "fee_account": "0xde03a0b5963f75f1c8485b355ff6d30f3093bde7",
    //                     "enforced_base_fee": null,
    //                     "first_l2_block": {
    //                         "number": 91,
    //                         "timestamp": 1715693104,
    //                         "prev_block_hash": "0x1ef787bfb1908c8a55d972375af69b74038bfb999cc4f46d5be582d945a3b2be",
    //                         "max_virtual_blocks_to_create": 1
    //                     }
    //                 },
    //                 "system_env": {
    //                     "zk_porter_available": false,
    //                     "version": "Version24",
    //                     "base_system_smart_contracts": {
    //                         "bootloader": {
    //                             "code": [
    //                                 "0x2000000000002001c00000000000200000000030100190000006003300270"
    //                             ],
    //                             "hash": "0x010008e742608b21bf7eb23c1a9d0602047e3618b464c9b59c0fba3b3d7ab66e"
    //                         },
    //                         "default_aa": {
    //                             "code": [
    //                                 "0x4000000610355000500000061035500060000006103550007000000610355"
    //                             ],
    //                             "hash": "0x01000563374c277a2c1e34659a2a1e87371bb6d852ce142022d497bfb50b9e32"
    //                         }
    //                     },
    //                     "bootloader_gas_limit": 4294967295,
    //                     "execution_mode": "VerifyExecute",
    //                     "default_validation_computational_gas_limit": 4294967295,
    //                     "chain_id": 270
    //                 },
    //                 "used_contracts": [
    //                     [
    //                         "0x010001211b0c33353cdf7a320f768e3dc40bce1326d639fcac099bba9ecd8e34",
    //                         [ 0, 4, 0, 0 ]
    //                     ]
    //                 ]
    //             }
    //         }
    //     "#;

    // let tvi: TeeVerifierInput = serde_json::from_str(&tst_tvi_json).unwrap();

    // // TODO: report error
    // let batch_no = tvi.l1_batch_no().unwrap_or(L1BatchNumber(0));

    // tracing::info!("Verifying L1 batch #{batch_no}");

    // // TODO: catch panic?
    // match tvi.verify() {
    //     Err(e) => {
    //         tracing::warn!("L1 batch #{batch_no} verification failed: {e}")
    //     }
    //     Ok(root_hash) => {
    //         let root_hash_bytes = root_hash.as_bytes();
    //         let signature: Signature = signing_key.try_sign(root_hash_bytes).unwrap();
    //         let _sig_bytes = signature.to_vec();
    //         // TODO: use _attestation_quote_bytes _verifying_key_bytes _sig_bytes
    //     }
    // }

    Ok(())
}
