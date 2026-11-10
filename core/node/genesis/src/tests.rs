use zksync_config::GenesisConfig;
use zksync_dal::{ConnectionPool, Core, CoreDal};

use super::*;

#[tokio::test]
async fn running_genesis() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut conn = pool.connection().await.unwrap();
    conn.blocks_dal().delete_genesis().await.unwrap();

    let params = GenesisParamsInitials::mock();

    let batch = insert_genesis_batch(&mut conn, &params).await.unwrap();

    assert!(!conn.blocks_dal().is_genesis_needed().await.unwrap());
    let metadata = conn
        .blocks_dal()
        .get_l1_batch_metadata(L1BatchNumber(0))
        .await
        .unwrap();
    let metadata = metadata.unwrap().metadata;
    assert_ne!(metadata.root_hash, H256::zero());
    let airbender = metadata.airbender.unwrap();
    assert_eq!(batch.airbender_commitment, Some(airbender.commitment));
    assert_eq!(airbender.commitment, metadata.commitment);
    assert_eq!(airbender.aux_data_hash, metadata.aux_data_hash);

    // Check that `genesis is not needed`
    assert!(!conn.blocks_dal().is_genesis_needed().await.unwrap());
}

#[tokio::test]
async fn running_genesis_with_big_chain_id() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut conn = pool.connection().await.unwrap();
    conn.blocks_dal().delete_genesis().await.unwrap();

    let params = GenesisParams::load_genesis_params(GenesisConfig {
        l2_chain_id: L2ChainId::max(),
        ..mock_genesis_config()
    })
    .unwrap()
    .into();
    insert_genesis_batch(&mut conn, &params).await.unwrap();

    assert!(!conn.blocks_dal().is_genesis_needed().await.unwrap());
    let metadata = conn
        .blocks_dal()
        .get_l1_batch_metadata(L1BatchNumber(0))
        .await;
    let root_hash = metadata.unwrap().unwrap().metadata.root_hash;
    assert_ne!(root_hash, H256::zero());
}

#[tokio::test]
async fn running_genesis_with_non_latest_protocol_version() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut conn = pool.connection().await.unwrap();
    let params = GenesisParams::load_genesis_params(GenesisConfig {
        protocol_version: Some(ProtocolSemanticVersion {
            minor: ProtocolVersionId::Version10,
            patch: 0.into(),
        }),
        ..mock_genesis_config()
    })
    .unwrap()
    .into();

    insert_genesis_batch(&mut conn, &params).await.unwrap();
    assert!(!conn.blocks_dal().is_genesis_needed().await.unwrap());
}

#[tokio::test]
async fn running_genesis_uses_l1_chain_id_for_settlement_layer() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut conn = pool.connection().await.unwrap();
    conn.blocks_dal().delete_genesis().await.unwrap();

    let l1_chain_id = L1ChainId(31337);
    let params = GenesisParams::load_genesis_params(GenesisConfig {
        l1_chain_id,
        ..mock_genesis_config()
    })
    .unwrap()
    .into();

    insert_genesis_batch(&mut conn, &params).await.unwrap();

    let header = conn
        .blocks_dal()
        .get_l1_batch_header(L1BatchNumber(0))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        header.settlement_layer,
        SettlementLayer::L1(l1_chain_id.into())
    );
}

#[tokio::test]
async fn validates_airbender_genesis_commitment() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut conn = pool.connection().await.unwrap();
    let mut params = GenesisParams::mock();
    let mut transaction = conn.start_transaction().await.unwrap();
    let batch = insert_genesis_batch(&mut transaction, &params.clone().into())
        .await
        .unwrap();
    transaction.rollback().await.unwrap();
    params.config.genesis_root_hash = Some(batch.root_hash);
    params.config.genesis_commitment = Some(batch.commitment);
    params.config.rollup_last_leaf_index = Some(batch.rollup_last_leaf_index);
    let invalid = H256::repeat_byte(0xff);
    params.config.genesis_airbender_commitment = Some(invalid);
    let error = ensure_genesis_state(&mut conn, &params, None)
        .await
        .unwrap_err();
    assert!(
        matches!(error, GenesisError::AirbenderCommitment(expected, calculated)
        if expected == invalid && calculated == batch.airbender_commitment)
    );
    assert!(conn.blocks_dal().is_genesis_needed().await.unwrap());
    params.config.genesis_airbender_commitment = batch.airbender_commitment;
    assert_eq!(
        ensure_genesis_state(&mut conn, &params, None)
            .await
            .unwrap(),
        batch.root_hash
    );
}

#[tokio::test]
async fn validates_verifier_key_for_protocol_version() {
    use zksync_eth_client::clients::MockSettlementLayer;
    use zksync_types::ethabi::Token;

    let plonk = H256::repeat_byte(1);
    let fflonk = H256::repeat_byte(2);
    let airbender = H256::repeat_byte(3);
    for minor in [ProtocolVersionId::Version33, ProtocolVersionId::Version34] {
        for primary_key in [plonk, fflonk] {
            let mut params = GenesisParams::mock();
            params.config.protocol_version = Some(ProtocolSemanticVersion {
                minor,
                patch: 0.into(),
            });
            params.config.snark_wrapper_vk_hash = plonk;
            params.config.fflonk_snark_wrapper_vk_hash = Some(fflonk);
            params.config.airbender_snark_wrapper_vk_hash = Some(airbender);
            let client = MockSettlementLayer::<zksync_eth_client::web3_decl::client::L1>::builder()
                .with_call_handler(move |request, _| {
                    let data = &request.data.as_ref().unwrap().0;
                    let chain_abi = hyperchain_contract();
                    if data[..4]
                        == chain_abi
                            .function("getProtocolVersion")
                            .unwrap()
                            .short_signature()
                    {
                        return Token::Uint(minor.into_packed_semver_with_patch(0));
                    }
                    if data[..4] == chain_abi.function("getVerifier").unwrap().short_signature() {
                        return Token::Address(Address::repeat_byte(4));
                    }
                    let verifier_abi = verifier_contract();
                    let function = verifier_abi
                        .functions_by_name("verificationKeyHash")
                        .unwrap()
                        .iter()
                        .find(|function| data[..4] == function.short_signature())
                        .unwrap();
                    let hash = if function.inputs.is_empty() {
                        primary_key
                    } else {
                        match U256::from_big_endian(&data[4..]).as_u32() {
                            0 => fflonk,
                            2 => airbender,
                            _ => panic!("unexpected verification key route"),
                        }
                    };
                    Token::FixedBytes(hash.as_bytes().to_vec())
                })
                .build();
            let expected = if minor >= ProtocolVersionId::Version34 {
                fflonk
            } else {
                plonk
            };
            let result =
                validate_genesis_params(&params, client.as_ref(), Address::repeat_byte(5)).await;
            assert_eq!(
                result.is_ok(),
                primary_key == expected,
                "{minor:?}: {result:?}"
            );
            if minor >= ProtocolVersionId::Version34 {
                params.config.fflonk_snark_wrapper_vk_hash = None;
                let error =
                    validate_genesis_params(&params, client.as_ref(), Address::repeat_byte(5))
                        .await
                        .unwrap_err();
                assert!(error
                    .to_string()
                    .contains("FFLONK verification key hash is required"));
            }
        }
    }
}
