use zksync_config::configs::{object_store::ObjectStoreMode, FriProverConfig, ObjectStoreConfig};
use zksync_dal::fri_proof_compressor_dal::FriProofCompressorDal;
use zksync_object_store::{bincode, ObjectStoreFactory};
use zksync_proof_fri_compressor::compressor::ProofCompressor;
use zksync_prover_fri_types::FriProofWrapper;


async fn prover_and_assert_compressor_layer(
    expected_proof_id: u32
) -> anyhow::Result<()> {
    let compression_mode: u8 = 1;
    let verify_wrapper_proof: bool = true;

    let object_store_config = ObjectStoreConfig {
        mode: ObjectStoreMode::FileBacked {
            file_backed_base_path: "./tests/data/".to_owned(),
        },
        max_retries: 5,
    };
    let object_store = ObjectStoreFactory::new(object_store_config)
        .create_store()
        .await;
    let wrapper_proof: FriProofWrapper = object_store
        .get(expected_proof_id)
        .await
        .expect("missing expected proof");
    let schedule_proof = match wrapper_proof {
        FriProofWrapper::Base(_) => anyhow::bail!("Must be a scheduler proof not base layer"),
        FriProofWrapper::Recursive(proof) => proof,
        _ => {}
    };

    let snark_proof = ProofCompressor::compress_proof(schedule_proof, compression_mode, verify_wrapper_proof);

    assert!(snark_proof.is_ok());
    Ok(())
}


#[tokio::test]
async fn test_base_layer_sha256_proof_gen() {
    let key = "FRI_PROVER_SETUP_DATA_PATH";
    if std::env::var(key).is_err() {
        std::env::set_var(key, "../vk_setup_data_generator_server_fri/data");
    }

    prover_and_assert_compressor_layer(107)
        .await
        .unwrap();
}
