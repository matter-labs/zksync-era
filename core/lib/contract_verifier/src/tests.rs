//! Tests for the contract verifier.

use std::{os::unix::fs::PermissionsExt, path::Path};

use tokio::{fs, sync::watch};
use zksync_dal::Connection;
use zksync_node_test_utils::{create_l1_batch, create_l2_block};
use zksync_types::{
    contract_verification_api::{CompilerVersions, VerificationIncomingRequest},
    get_code_key, get_known_code_key,
    l2::L2Tx,
    tx::IncludedTxLocation,
    Execute, L1BatchNumber, L2BlockNumber, ProtocolVersion, StorageLog, CONTRACT_DEPLOYER_ADDRESS,
    H256,
};
use zksync_utils::{
    address_to_h256,
    bytecode::{hash_bytecode, validate_bytecode},
};
use zksync_vm_interface::{TransactionExecutionMetrics, VmEvent};

use super::*;
use crate::resolver::{CompilerPaths, SupportedCompilerVersions};

const SOLC_VERSION: &str = "0.8.27";
const ZKSOLC_VERSION: &str = "1.5.4";

async fn mock_deployment(storage: &mut Connection<'_, Core>, address: Address, bytecode: Vec<u8>) {
    let bytecode_hash = hash_bytecode(&bytecode);
    let logs = [
        StorageLog::new_write_log(get_code_key(&address), bytecode_hash),
        StorageLog::new_write_log(get_known_code_key(&bytecode_hash), H256::from_low_u64_be(1)),
    ];
    storage
        .storage_logs_dal()
        .append_storage_logs(L2BlockNumber(0), &logs)
        .await
        .unwrap();
    storage
        .factory_deps_dal()
        .insert_factory_deps(
            L2BlockNumber(0),
            &HashMap::from([(bytecode_hash, bytecode.clone())]),
        )
        .await
        .unwrap();

    let mut deploy_tx = L2Tx {
        execute: Execute::for_deploy(H256::zero(), bytecode, &[]),
        common_data: Default::default(),
        received_timestamp_ms: 0,
        raw_bytes: Some(vec![0; 128].into()),
    };
    deploy_tx.set_input(vec![0; 128], H256::repeat_byte(0x23));
    storage
        .transactions_dal()
        .insert_transaction_l2(&deploy_tx, TransactionExecutionMetrics::default())
        .await
        .unwrap();

    let deployer_address = Address::repeat_byte(0xff);
    let location = IncludedTxLocation {
        tx_hash: deploy_tx.hash(),
        tx_index_in_l2_block: 0,
        tx_initiator_address: deployer_address,
    };
    let deploy_event = VmEvent {
        location: (L1BatchNumber(0), 0),
        address: CONTRACT_DEPLOYER_ADDRESS,
        indexed_topics: vec![
            VmEvent::DEPLOY_EVENT_SIGNATURE,
            address_to_h256(&deployer_address),
            bytecode_hash,
            address_to_h256(&address),
        ],
        value: vec![],
    };
    storage
        .events_dal()
        .save_events(L2BlockNumber(0), &[(location, vec![&deploy_event])])
        .await
        .unwrap();
}

/// Test compiler resolver.
#[derive(Debug)]
struct TestCompilerResolver(CompilerPaths);

impl TestCompilerResolver {
    async fn install_zksolc(path: &Path) {
        if fs::try_exists(path).await.unwrap() {
            return;
        }

        // We may race from several test processes here; this is OK-ish for tests.
        let version = ZKSOLC_VERSION;
        let compiler_prefix = match svm::platform() {
            svm::Platform::LinuxAmd64 => "zksolc-linux-amd64-musl-",
            svm::Platform::LinuxAarch64 => "zksolc-linux-arm64-musl-",
            svm::Platform::MacOsAmd64 => "zksolc-macosx-amd64-",
            svm::Platform::MacOsAarch64 => "zksolc-macosx-arm64-",
            other => panic!("Unsupported platform: {other:?}"),
        };
        let download_url = format!(
            "https://github.com/matter-labs/zksolc-bin/releases/download/v{version}/{compiler_prefix}v{version}",
        );
        let response = reqwest::Client::new()
            .get(&download_url)
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap();
        let response_bytes = response.bytes().await.unwrap();

        fs::create_dir_all(path.parent().unwrap()).await.unwrap();
        fs::write(path, &response_bytes).await.unwrap();
        // Set the executable flag for the file.
        fs::set_permissions(path, PermissionsExt::from_mode(0o755))
            .await
            .unwrap();
    }

    async fn new() -> Self {
        let solc_version = SOLC_VERSION.parse().unwrap();
        let solc_path = svm::install(&solc_version)
            .await
            .expect("failed installing solc");
        let zksolc_path = svm::data_dir()
            .join(format!("zksolc-{ZKSOLC_VERSION}"))
            .join("zksolc");
        Self::install_zksolc(&zksolc_path).await;

        Self(CompilerPaths {
            base: solc_path,
            zk: zksolc_path,
        })
    }
}

#[async_trait]
impl CompilerResolver for TestCompilerResolver {
    async fn supported_versions(&self) -> anyhow::Result<SupportedCompilerVersions> {
        Ok(SupportedCompilerVersions {
            solc: vec![SOLC_VERSION.to_owned()],
            zksolc: vec![ZKSOLC_VERSION.to_owned()],
            vyper: vec![],
            zkvyper: vec![],
        })
    }

    async fn resolve_solc(
        &self,
        versions: &CompilerVersions,
    ) -> Result<CompilerPaths, ContractVerifierError> {
        if versions.compiler_version() != SOLC_VERSION {
            return Err(ContractVerifierError::UnknownCompilerVersion(
                "solc".to_owned(),
                versions.compiler_version(),
            ));
        }
        if versions.zk_compiler_version() != ZKSOLC_VERSION {
            return Err(ContractVerifierError::UnknownCompilerVersion(
                "zksolc".to_owned(),
                versions.zk_compiler_version(),
            ));
        }
        Ok(self.0.clone())
    }

    async fn resolve_vyper(
        &self,
        _versions: &CompilerVersions,
    ) -> Result<CompilerPaths, ContractVerifierError> {
        unreachable!("not tested")
    }
}

fn test_request(address: Address) -> VerificationIncomingRequest {
    let contract_source = r#"
    contract Counter {
        uint256 value;

        function increment(uint256 x) external {
            value += x;
        }
    }
    "#;
    VerificationIncomingRequest {
        contract_address: address,
        source_code_data: SourceCodeData::SolSingleFile(contract_source.into()),
        contract_name: "Counter".to_owned(),
        compiler_versions: CompilerVersions::Solc {
            compiler_zksolc_version: ZKSOLC_VERSION.to_owned(),
            compiler_solc_version: SOLC_VERSION.to_owned(),
        },
        optimization_used: true,
        optimizer_mode: None,
        constructor_arguments: Default::default(),
        is_system: false,
        force_evmla: false,
    }
}

async fn compile_counter(compiler_paths: CompilerPaths) -> CompilationArtifacts {
    let req = test_request(Address::repeat_byte(1));
    let input = ContractVerifier::build_zksolc_input(req).unwrap();
    ZkSolc::new(compiler_paths, ZKSOLC_VERSION.to_owned())
        .async_compile(input)
        .await
        .unwrap()
}

#[tokio::test]
async fn compiler_works() {
    let compiler_paths = TestCompilerResolver::new().await.0;
    let output = compile_counter(compiler_paths).await;
    validate_bytecode(&output.bytecode).unwrap();
    let items = output.abi.as_array().unwrap();
    assert_eq!(items.len(), 1);
    let increment_function = items[0].as_object().unwrap();
    assert_eq!(increment_function["type"], "function");
    assert_eq!(increment_function["name"], "increment");
}

#[tokio::test]
async fn contract_verifier_basics() {
    let test_resolver = TestCompilerResolver::new().await;
    let output = compile_counter(test_resolver.0.clone()).await;
    let pool = ConnectionPool::test_pool().await;
    let mut storage = pool.connection().await.unwrap();

    // Storage must contain at least 1 block / batch for verifier-related queries to work correctly.
    storage
        .protocol_versions_dal()
        .save_protocol_version_with_tx(&ProtocolVersion::default())
        .await
        .unwrap();
    storage
        .blocks_dal()
        .insert_l2_block(&create_l2_block(0))
        .await
        .unwrap();
    storage
        .blocks_dal()
        .insert_mock_l1_batch(&create_l1_batch(0))
        .await
        .unwrap();

    let address = Address::repeat_byte(1);
    mock_deployment(&mut storage, address, output.bytecode.clone()).await;
    let req = test_request(address);
    let request_id = storage
        .contract_verification_dal()
        .add_contract_verification_request(req)
        .await
        .unwrap();
    let verifier = ContractVerifier::with_resolver(
        Duration::from_secs(60),
        pool.clone(),
        Arc::new(test_resolver),
    )
    .await
    .unwrap();

    // Check that the compiler versions are synced.
    let solc_versions = storage
        .contract_verification_dal()
        .get_solc_versions()
        .await
        .unwrap();
    assert_eq!(solc_versions, [SOLC_VERSION]);
    let zksolc_versions = storage
        .contract_verification_dal()
        .get_zksolc_versions()
        .await
        .unwrap();
    assert_eq!(zksolc_versions, [ZKSOLC_VERSION]);

    let (_stop_sender, stop_receiver) = watch::channel(false);
    verifier.run(stop_receiver, Some(1)).await.unwrap();

    let status = storage
        .contract_verification_dal()
        .get_verification_request_status(request_id)
        .await
        .unwrap()
        .expect("no status");
    assert_eq!(status.error, None);
    assert_eq!(status.compilation_errors, None);
    assert_eq!(status.status, "successful");

    let verification_info = storage
        .contract_verification_dal()
        .get_contract_verification_info(address)
        .await
        .unwrap()
        .expect("no verification info");
    assert_eq!(verification_info.artifacts.bytecode, output.bytecode);
    assert_eq!(verification_info.artifacts.abi, output.abi);
}
