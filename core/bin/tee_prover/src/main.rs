extern crate core;

use anyhow::Context;
use k256::{
    ecdsa::{signature::Signer, Signature, SigningKey, VerifyingKey},
    pkcs8::DecodePrivateKey,
};
use zksync_config::configs::ObservabilityConfig;
use zksync_env_config::FromEnv;
use zksync_tee_verifier::TeeVerifierInput;
use zksync_types::L1BatchNumber;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
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

    // Report whether sentry is running after the logging subsystem was initialized.
    if let Some(sentry_url) = observability_config.sentry_url {
        tracing::info!("Sentry configured with URL: {sentry_url}");
    } else {
        tracing::info!("No sentry URL was provided");
    }

    let signing_key_pem = std::env::var("TEE_SIGNING_KEY").unwrap_or(
        r#"-----BEGIN PRIVATE KEY-----
MIGEAgEAMBAGByqGSM49AgEGBSuBBAAKBG0wawIBAQQgtQs4yNOWyIco/AMuzlWO
valpB6CxqTQCiXFe73vyneuhRANCAARXX55cR5OHwxskdsKKBBalUBgCAU+oKIl7
K678gpQwmzwbZqqiuNpWxeEu+51Nh3tmPPJGTW9Y0BCNC0if7cxA
-----END PRIVATE KEY-----
"#
        .into(),
    );
    let signing_key: SigningKey = SigningKey::from_pkcs8_pem(&signing_key_pem).unwrap();
    let _verifying_key_bytes = signing_key.verifying_key().to_sec1_bytes();

    // TEST TEST
    {
        use k256::ecdsa::signature::Verifier;
        let vkey: VerifyingKey = VerifyingKey::try_from(_verifying_key_bytes.as_ref()).unwrap();
        let signature: Signature = signing_key.try_sign(&[0, 0, 0, 0]).unwrap();
        let sig_bytes = signature.to_vec();
        let signature: Signature = Signature::try_from(sig_bytes.as_ref()).unwrap();
        let _ = vkey.verify(&[0, 0, 0, 0], &signature).unwrap();
    }
    // END TEST

    let attestation_quote_file = std::env::var("TEE_SIGNING_KEY").unwrap_or_default();
    // read attestation quote bytes from attestation_quote_file
    let _attestation_quote_bytes = std::fs::read(&attestation_quote_file).unwrap_or_default();

    let tst_tvi_json = r#"
            {
                "V1": {
                    "prepare_basic_circuits_job": {
                        "merkle_paths": [
                            {
                                "root_hash": [
                                    199, 231, 138, 237, 215, 168, 130, 194, 198, 6, 187, 237, 77, 26, 152, 210,
                                    88, 244, 103, 217, 198, 89, 54, 183, 3, 48, 12, 198, 157, 109, 17, 108
                                ],
                                "is_write": false,
                                "first_write": false,
                                "merkle_paths": [
                                    [
                                        14, 61, 115, 101, 43, 176, 68, 16, 107, 44, 117, 212, 243, 107, 174, 139,
                                        221, 199, 237, 48, 120, 145, 101, 195, 53, 184, 23, 176, 118, 216, 58, 141
                                    ]
                                ],
                                "leaf_hashed_key": "0xa792adc37510103905c79c23e63fc13938000f8acb1120dd8cc76d6f13a11577",
                                "leaf_enumeration_index": 2,
                                "value_written": [
                                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
                                ],
                                "value_read": [
                                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
                                ]
                            }
                        ],
                        "next_enumeration_index": 23577
                    },
                    "l2_blocks_execution_data": [
                        {
                            "number": 91,
                            "timestamp": 1715693104,
                            "prev_block_hash": "0x1ef787bfb1908c8a55d972375af69b74038bfb999cc4f46d5be582d945a3b2be",
                            "virtual_blocks": 1,
                            "txs": [
                                {
                                    "common_data": {
                                        "L1": {
                                            "sender": "0x62b13dd4f940b691a015d1b1e29cecd3cfec5d77",
                                            "serialId": 208,
                                            "deadlineBlock": 0,
                                            "layer2TipFee": "0x0",
                                            "fullFee": "0x0",
                                            "maxFeePerGas": "0x10642ac0",
                                            "gasLimit": "0x4c4b40",
                                            "gasPerPubdataLimit": "0x320",
                                            "opProcessingType": "Common",
                                            "priorityQueueType": "Deque",
                                            "ethHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
                                            "ethBlock": 1727,
                                            "canonicalTxHash": "0x8f53c4d042e7931cbe43889f7992a86cd9d24db76ac0ff2dc4ceb55250059a92",
                                            "toMint": "0x4e28e2290f000",
                                            "refundRecipient": "0x62b13dd4f940b691a015d1b1e29cecd3cfec5d77"
                                        }
                                    },
                                    "execute": {
                                        "contractAddress": "0x350822d8850e1ce8894e4bb86ed7243baaa747fc",
                                        "calldata": "0xd542b16c000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000000a00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000001",
                                        "value": "0x0",
                                        "factoryDeps": [ [ 0 ] ]
                                    },
                                    "received_timestamp_ms": 1715693103904,
                                    "raw_bytes": null
                                }
                            ]
                        },
                        {
                            "number": 95,
                            "timestamp": 1715693108,
                            "prev_block_hash": "0x3ca40be0a54a377be77bd6ad87e376ae7ddb25090d2d32382e3c94759d1fc76d",
                            "virtual_blocks": 1,
                            "txs": []
                        }
                    ],
                    "l1_batch_env": {
                        "previous_batch_hash": "0xc7e78aedd7a882c2c606bbed4d1a98d258f467d9c65936b703300cc69d6d116c",
                        "number": 20,
                        "timestamp": 1715693104,
                        "fee_input": {
                            "PubdataIndependent": {
                                "fair_l2_gas_price": 100000000,
                                "fair_pubdata_price": 13600000000,
                                "l1_gas_price": 800000000
                            }
                        },
                        "fee_account": "0xde03a0b5963f75f1c8485b355ff6d30f3093bde7",
                        "enforced_base_fee": null,
                        "first_l2_block": {
                            "number": 91,
                            "timestamp": 1715693104,
                            "prev_block_hash": "0x1ef787bfb1908c8a55d972375af69b74038bfb999cc4f46d5be582d945a3b2be",
                            "max_virtual_blocks_to_create": 1
                        }
                    },
                    "system_env": {
                        "zk_porter_available": false,
                        "version": "Version24",
                        "base_system_smart_contracts": {
                            "bootloader": {
                                "code": [
                                    "0x2000000000002001c00000000000200000000030100190000006003300270"
                                ],
                                "hash": "0x010008e742608b21bf7eb23c1a9d0602047e3618b464c9b59c0fba3b3d7ab66e"
                            },
                            "default_aa": {
                                "code": [
                                    "0x4000000610355000500000061035500060000006103550007000000610355"
                                ],
                                "hash": "0x01000563374c277a2c1e34659a2a1e87371bb6d852ce142022d497bfb50b9e32"
                            }
                        },
                        "bootloader_gas_limit": 4294967295,
                        "execution_mode": "VerifyExecute",
                        "default_validation_computational_gas_limit": 4294967295,
                        "chain_id": 270
                    },
                    "used_contracts": [
                        [
                            "0x010001211b0c33353cdf7a320f768e3dc40bce1326d639fcac099bba9ecd8e34",
                            [ 0, 4, 0, 0 ]
                        ]
                    ]
                }
            }
        "#;

    let tvi: TeeVerifierInput = serde_json::from_str(&tst_tvi_json).unwrap();

    // TODO: report error
    let batch_no = tvi.l1_batch_no().unwrap_or(L1BatchNumber(0));

    tracing::info!("Verifying L1 batch #{batch_no}");

    // TODO: catch panic?
    match tvi.verify() {
        Err(e) => {
            tracing::warn!("L1 batch #{batch_no} verification failed: {e}")
        }
        Ok(root_hash) => {
            let root_hash_bytes = root_hash.as_bytes();
            let signature: Signature = signing_key.try_sign(root_hash_bytes).unwrap();
            let _sig_bytes = signature.to_vec();
            // TODO: use _attestation_quote_bytes _verifying_key_bytes _sig_bytes
        }
    }

    Ok(())
}
