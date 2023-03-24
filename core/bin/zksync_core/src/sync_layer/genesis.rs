use crate::genesis::ensure_genesis_state;

use zksync_config::ZkSyncConfig;
use zksync_dal::StorageProcessor;
use zksync_types::{L1BatchNumber, H256};
use zksync_web3_decl::{jsonrpsee::http_client::HttpClientBuilder, namespaces::ZksNamespaceClient};

pub async fn perform_genesis_if_needed(storage: &mut StorageProcessor<'_>, config: &ZkSyncConfig) {
    let mut transaction = storage.start_transaction().await;
    let main_node_url = config
        .api
        .web3_json_rpc
        .main_node_url
        .as_ref()
        .expect("main node url is not set");

    let genesis_block_hash = ensure_genesis_state(&mut transaction, config).await;

    validate_genesis_state(main_node_url, genesis_block_hash).await;
    transaction.commit().await;
}

// When running an external node, we want to make sure we have the same
// genesis root hash as the main node.
async fn validate_genesis_state(main_node_url: &str, root_hash: H256) {
    let client = HttpClientBuilder::default().build(main_node_url).unwrap();
    let genesis_block = client
        .get_l1_batch_details(L1BatchNumber(0))
        .await
        .expect("couldn't get genesis block from the main node")
        .expect("main node did not return a genesis block");

    let genesis_block_hash = genesis_block.root_hash.expect("empty genesis block hash");

    if genesis_block_hash != root_hash {
        panic!(
            "Genesis block root hash mismatch with main node: expected {}, got {}",
            root_hash, genesis_block_hash
        );
    }
}
