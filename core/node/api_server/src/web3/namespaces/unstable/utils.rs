use zksync_dal::{Connection, Core, CoreDal, DalError};
use zksync_system_constants::{
    message_root::{CHAIN_COUNT_KEY, CHAIN_INDEX_TO_ID_KEY, CHAIN_TREE_KEY},
    L2_MESSAGE_ROOT_ADDRESS,
};
use zksync_types::{
    h256_to_u256, l2_to_l1_log::CHAIN_ID_LEAF_PADDING, u256_to_h256, web3::keccak256,
    AccountTreeId, L2BlockNumber, StorageKey, H256, U256,
};
use zksync_web3_decl::error::Web3Error;

pub(super) async fn get_chain_count(
    connection: &mut Connection<'_, Core>,
    block_number: L2BlockNumber,
) -> anyhow::Result<u8> {
    let chain_count_key = CHAIN_COUNT_KEY;
    let chain_count_storage_key =
        message_root_log_key(H256::from_low_u64_be(chain_count_key as u64));
    let chain_count = connection
        .storage_web3_dal()
        .get_historical_value_unchecked(chain_count_storage_key.hashed_key(), block_number)
        .await
        .map_err(DalError::generalize)?;
    if h256_to_u256(chain_count) > u8::MAX.into() {
        anyhow::bail!("Chain count doesn't fit in `u8`");
    }
    Ok(chain_count.0[31])
}

pub(super) async fn get_chain_id_from_index(
    connection: &mut Connection<'_, Core>,
    chain_index: u8,
    block_number: L2BlockNumber,
) -> Result<H256, Web3Error> {
    let key = H256::from_slice(&keccak256(
        &[
            H256::from_low_u64_be(chain_index as u64).0,
            H256::from_low_u64_be(CHAIN_INDEX_TO_ID_KEY as u64).0,
        ]
        .concat(),
    ));
    let storage_key = message_root_log_key(key);
    let chain_id = connection
        .storage_web3_dal()
        .get_historical_value_unchecked(storage_key.hashed_key(), block_number)
        .await
        .map_err(DalError::generalize)?;
    Ok(chain_id)
}

pub(super) async fn get_chain_root_from_id(
    connection: &mut Connection<'_, Core>,
    chain_id: H256,
    block_number: L2BlockNumber,
) -> Result<H256, Web3Error> {
    let chain_tree_key = H256::from_slice(&keccak256(
        &[chain_id.0, H256::from_low_u64_be(CHAIN_TREE_KEY as u64).0].concat(),
    ));
    let chain_sides_len_key =
        u256_to_h256(h256_to_u256(chain_tree_key).overflowing_add(U256::one()).0);
    let chain_sides_len_storage_key = message_root_log_key(chain_sides_len_key);
    let chain_sides_len = connection
        .storage_web3_dal()
        .get_historical_value_unchecked(chain_sides_len_storage_key.hashed_key(), block_number)
        .await
        .map_err(DalError::generalize)?;

    let last_element_pos = {
        let length = h256_to_u256(chain_sides_len);
        assert!(
            length > U256::zero(),
            "_sides.length is zero, chain is not registered"
        );

        length - 1
    };
    let sides_data_start_key = H256(keccak256(chain_sides_len_key.as_bytes()));
    let chain_root_key = h256_to_u256(sides_data_start_key)
        .overflowing_add(last_element_pos)
        .0;
    let chain_root_storage_key = message_root_log_key(u256_to_h256(chain_root_key));
    let chain_root = connection
        .storage_web3_dal()
        .get_historical_value_unchecked(chain_root_storage_key.hashed_key(), block_number)
        .await
        .map_err(DalError::generalize)?;
    println!(
        "getting chain root for chain id {:?} and block number {:?}: {:?}",
        chain_id, block_number, chain_root
    );
    Ok(chain_root)
}

pub(super) fn chain_id_leaf_preimage(chain_root: H256, chain_id: H256) -> [u8; 96] {
    let mut full_preimage = [0u8; 96];

    full_preimage[0..32].copy_from_slice(CHAIN_ID_LEAF_PADDING.as_bytes());
    full_preimage[32..64].copy_from_slice(&chain_root.0);
    full_preimage[64..96].copy_from_slice(&chain_id.0);

    full_preimage
}

fn message_root_log_key(key: H256) -> StorageKey {
    let message_root = AccountTreeId::new(L2_MESSAGE_ROOT_ADDRESS);
    StorageKey::new(message_root, key)
}
