use crate::genesis::{ensure_genesis_state, GenesisParams};

use zksync_contracts::{BaseSystemContracts, BaseSystemContractsHashes, SystemContractCode};
use zksync_dal::StorageProcessor;
use zksync_types::{L1BatchNumber, L2ChainId, H256};
use zksync_web3_decl::{
    jsonrpsee::{core::error::Error, http_client::HttpClientBuilder},
    namespaces::ZksNamespaceClient,
};

pub async fn perform_genesis_if_needed(
    storage: &mut StorageProcessor<'_>,
    zksync_chain_id: L2ChainId,
    base_system_contracts_hashes: BaseSystemContractsHashes,
    main_node_url: String,
) {
    let mut transaction = storage.start_transaction_blocking();

    let genesis_block_hash = ensure_genesis_state(
        &mut transaction,
        zksync_chain_id,
        GenesisParams::ExternalNode {
            base_system_contracts_hashes,
            main_node_url: main_node_url.clone(),
        },
    )
    .await;

    validate_genesis_state(&main_node_url, genesis_block_hash).await;
    transaction.commit_blocking();
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

pub async fn fetch_system_contract_by_hash(
    main_node_url: &str,
    hash: H256,
) -> Result<SystemContractCode, Error> {
    let client = HttpClientBuilder::default().build(main_node_url).unwrap();
    let bytecode = client
        .get_bytecode_by_hash(hash)
        .await?
        .expect("Failed to get base system contract bytecode");
    assert_eq!(
        hash,
        zksync_utils::bytecode::hash_bytecode(&bytecode),
        "Got invalid base system contract bytecode from main node"
    );
    Ok(SystemContractCode {
        code: zksync_utils::bytes_to_be_words(bytecode),
        hash,
    })
}

pub async fn fetch_base_system_contracts(
    main_node_url: &str,
    hashes: BaseSystemContractsHashes,
) -> Result<BaseSystemContracts, Error> {
    Ok(BaseSystemContracts {
        bootloader: fetch_system_contract_by_hash(main_node_url, hashes.bootloader).await?,
        default_aa: fetch_system_contract_by_hash(main_node_url, hashes.default_aa).await?,
    })
}
