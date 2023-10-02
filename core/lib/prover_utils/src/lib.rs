#![allow(clippy::upper_case_acronyms, clippy::derive_partial_eq_without_eq)]

extern crate core;

use std::{fs::create_dir_all, io::Cursor, path::Path, time::Duration};

use futures::{channel::mpsc, executor::block_on, SinkExt};

pub mod gcs_proof_fetcher;
pub mod periodic_job;
pub mod region_fetcher;
pub mod vk_commitment_helper;

fn download_bytes(key_download_url: &str) -> reqwest::Result<Vec<u8>> {
    tracing::info!("Downloading initial setup from {:?}", key_download_url);

    const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(120);
    let client = reqwest::blocking::Client::builder()
        .timeout(DOWNLOAD_TIMEOUT)
        .build()
        .unwrap();

    const DOWNLOAD_RETRIES: usize = 5;
    let mut retry_count = 0;

    while retry_count < DOWNLOAD_RETRIES {
        let bytes = client
            .get(key_download_url)
            .send()
            .and_then(|response| response.bytes().map(|bytes| bytes.to_vec()));
        match bytes {
            Ok(bytes) => return Ok(bytes),
            Err(_) => retry_count += 1,
        }

        tracing::warn!("Failed to download keys. Backing off for 5 second");
        std::thread::sleep(Duration::from_secs(5));
    }

    client
        .get(key_download_url)
        .send()
        .and_then(|response| response.bytes().map(|bytes| bytes.to_vec()))
}

pub fn ensure_initial_setup_keys_present(initial_setup_key_path: &str, key_download_url: &str) {
    if Path::new(initial_setup_key_path).exists() {
        tracing::info!(
            "Initial setup already present at {:?}",
            initial_setup_key_path
        );
        return;
    }

    let bytes = download_bytes(key_download_url).expect("Failed downloading initial setup");
    let initial_setup_key_dir = Path::new(initial_setup_key_path).parent().unwrap();
    create_dir_all(initial_setup_key_dir).unwrap_or_else(|_| {
        panic!(
            "Failed creating dirs recursively: {:?}",
            initial_setup_key_dir
        )
    });
    let mut file = std::fs::File::create(initial_setup_key_path)
        .expect("Cannot create file for the initial setup");
    let mut content = Cursor::new(bytes);
    std::io::copy(&mut content, &mut file).expect("Cannot write the downloaded key to the file");
}

pub fn numeric_index_to_circuit_name(circuit_numeric_index: u8) -> Option<&'static str> {
    match circuit_numeric_index {
        0 => Some("Scheduler"),
        1 => Some("Node aggregation"),
        2 => Some("Leaf aggregation"),
        3 => Some("Main VM"),
        4 => Some("Decommitts sorter"),
        5 => Some("Code decommitter"),
        6 => Some("Log demuxer"),
        7 => Some("Keccak"),
        8 => Some("SHA256"),
        9 => Some("ECRecover"),
        10 => Some("RAM permutation"),
        11 => Some("Storage sorter"),
        12 => Some("Storage application"),
        13 => Some("Initial writes pubdata rehasher"),
        14 => Some("Repeated writes pubdata rehasher"),
        15 => Some("Events sorter"),
        16 => Some("L1 messages sorter"),
        17 => Some("L1 messages rehasher"),
        18 => Some("L1 messages merklizer"),
        _ => None,
    }
}

pub fn circuit_name_to_numeric_index(circuit_name: &str) -> Option<u8> {
    match circuit_name {
        "Scheduler" => Some(0),
        "Node aggregation" => Some(1),
        "Leaf aggregation" => Some(2),
        "Main VM" => Some(3),
        "Decommitts sorter" => Some(4),
        "Code decommitter" => Some(5),
        "Log demuxer" => Some(6),
        "Keccak" => Some(7),
        "SHA256" => Some(8),
        "ECRecover" => Some(9),
        "RAM permutation" => Some(10),
        "Storage sorter" => Some(11),
        "Storage application" => Some(12),
        "Initial writes pubdata rehasher" => Some(13),
        "Repeated writes pubdata rehasher" => Some(14),
        "Events sorter" => Some(15),
        "L1 messages sorter" => Some(16),
        "L1 messages rehasher" => Some(17),
        "L1 messages merklizer" => Some(18),
        _ => None,
    }
}

pub fn get_stop_signal_receiver() -> mpsc::Receiver<bool> {
    let (mut stop_signal_sender, stop_signal_receiver) = mpsc::channel(256);
    ctrlc::set_handler(move || {
        block_on(stop_signal_sender.send(true)).expect("Ctrl+C signal send");
    })
    .expect("Error setting Ctrl+C handler");
    stop_signal_receiver
}
