use std::{env, fs::create_dir_all, io::Cursor, path::Path, time::Duration};

use zksync_config::configs::FriProofCompressorConfig;
use zksync_prover_fri_types::circuit_definitions::{
    boojum::field::goldilocks::GoldilocksField,
    zkevm_circuits::scheduler::block_header::BlockAuxilaryOutputWitness,
};

pub(crate) fn setup_crs_keys(config: &FriProofCompressorConfig, is_fflonk: bool) {
    if is_fflonk {
        download_initial_setup_keys_if_not_present(
            &config.universal_fflonk_setup_path,
            &config.universal_fflonk_setup_download_url,
        );

        env::set_var("COMPACT_CRS_FILE", &config.universal_fflonk_setup_path);
        return;
    }

    download_initial_setup_keys_if_not_present(
        &config.universal_setup_path,
        &config.universal_setup_download_url,
    );
    env::set_var("CRS_FILE", &config.universal_setup_path);
}

#[tracing::instrument(skip_all)]
fn download_initial_setup_keys_if_not_present(
    initial_setup_key_path: &str,
    key_download_url: &str,
) {
    if Path::new(initial_setup_key_path).exists() {
        tracing::info!(
            "Initial setup already present at {:?}",
            initial_setup_key_path
        );
        return;
    }

    let bytes = download_initial_setup(key_download_url).expect("Failed downloading initial setup");
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

#[tracing::instrument(skip_all)]
fn download_initial_setup(key_download_url: &str) -> reqwest::Result<Vec<u8>> {
    tracing::info!("Downloading initial setup from {:?}", key_download_url);

    const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(600);
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
            Err(e) => {
                tracing::warn!("Failed to download keys: {}", e);
                retry_count += 1
            }
        }

        tracing::warn!("Failed to download keys. Backing off for 5 second");
        std::thread::sleep(Duration::from_secs(5));
    }

    client
        .get(key_download_url)
        .send()
        .and_then(|response| response.bytes().map(|bytes| bytes.to_vec()))
}

pub(crate) fn aux_output_witness_to_array(
    aux_output_witness: BlockAuxilaryOutputWitness<GoldilocksField>,
) -> [[u8; 32]; 4] {
    let mut array: [[u8; 32]; 4] = [[0; 32]; 4];

    for i in 0..32 {
        array[0][i] = aux_output_witness.l1_messages_linear_hash[i];
        array[1][i] = aux_output_witness.rollup_state_diff_for_compression[i];
        array[2][i] = aux_output_witness.bootloader_heap_initial_content[i];
        array[3][i] = aux_output_witness.events_queue_state[i];
    }
    array
}
