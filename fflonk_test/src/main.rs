use std::{path::PathBuf, sync::Arc};

use proof_compression_gpu::{run_proof_chain, SnarkWrapper};
use zksync_basic_types::L2ChainId;
use zksync_object_store::{FileBackedObjectStore, ObjectStore};
use zksync_prover_fri_types::FriProofWrapper;
use zksync_prover_keystore::{compressor::load_all_resources, keystore::Keystore};


#[tokio::main]
async fn main() {
    let keystore = Arc::new(Keystore::locate().with_setup_path(Some(PathBuf::from("../prover/data/keys"))));
    load_all_resources(&keystore, true);
    let object_store: Arc<dyn ObjectStore> = Arc::new(FileBackedObjectStore::new(PathBuf::from("../prover/artifacts/")).await.expect("failed to create object store"));
    let fri_proof: FriProofWrapper = object_store.get((14515_u32, L2ChainId::new(271).expect("failed to create L2ChainId"))).await.expect("failed to get proof from object store");
    let scheduler_proof = match fri_proof {
        FriProofWrapper::Base(_) => panic!("Must be a scheduler proof not base layer"),
        FriProofWrapper::Recursive(proof) => proof,
    };


    let _proof_wrapper = run_proof_chain(
        SnarkWrapper::Fflonk,
        keystore,
        scheduler_proof.into_inner(),
    ).expect("failed to generate proof");
}
