//! Tests for the contract verifier.

use tokio::sync::watch;
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
use zksync_utils::{address_to_h256, bytecode::hash_bytecode};
use zksync_vm_interface::{tracer::ValidationTraces, TransactionExecutionMetrics, VmEvent};

use super::*;
use crate::resolver::{Compiler, SupportedCompilerVersions};

mod real;

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
        .insert_transaction_l2(
            &deploy_tx,
            TransactionExecutionMetrics::default(),
            ValidationTraces::default(),
        )
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

#[derive(Clone)]
struct MockCompilerResolver {
    zksolc: Arc<
        dyn Fn(ZkSolcInput) -> Result<CompilationArtifacts, ContractVerifierError> + Send + Sync,
    >,
}

impl fmt::Debug for MockCompilerResolver {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MockCompilerResolver")
            .finish_non_exhaustive()
    }
}

impl MockCompilerResolver {
    fn new(zksolc: impl Fn(ZkSolcInput) -> CompilationArtifacts + 'static + Send + Sync) -> Self {
        Self {
            zksolc: Arc::new(move |input| Ok(zksolc(input))),
        }
    }
}

#[async_trait]
impl Compiler<ZkSolcInput> for MockCompilerResolver {
    async fn compile(
        self: Box<Self>,
        input: ZkSolcInput,
    ) -> Result<CompilationArtifacts, ContractVerifierError> {
        (self.zksolc)(input)
    }
}

#[async_trait]
impl CompilerResolver for MockCompilerResolver {
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
    ) -> Result<Box<dyn Compiler<ZkSolcInput>>, ContractVerifierError> {
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
        Ok(Box::new(self.clone()))
    }

    async fn resolve_vyper(
        &self,
        _versions: &CompilerVersions,
    ) -> Result<Box<dyn Compiler<ZkVyperInput>>, ContractVerifierError> {
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

fn counter_contract_abi() -> serde_json::Value {
    serde_json::json!([{
        "inputs": [{
            "internalType": "uint256",
            "name": "x",
            "type": "uint256",
        }],
        "name": "increment",
        "outputs": [],
        "stateMutability": "nonpayable",
        "type": "function",
    }])
}

async fn prepare_storage(storage: &mut Connection<'_, Core>) {
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
}

#[tokio::test]
async fn contract_verifier_basics() {
    let pool = ConnectionPool::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    let expected_bytecode = vec![0_u8; 32];

    prepare_storage(&mut storage).await;
    let address = Address::repeat_byte(1);
    mock_deployment(&mut storage, address, expected_bytecode.clone()).await;
    let req = test_request(address);
    let request_id = storage
        .contract_verification_dal()
        .add_contract_verification_request(req)
        .await
        .unwrap();

    let mock_resolver = MockCompilerResolver::new(|input| {
        let ZkSolcInput::StandardJson { input, .. } = &input else {
            panic!("unexpected input");
        };
        assert_eq!(input.language, "Solidity");
        assert_eq!(input.sources.len(), 1);
        let source = input.sources.values().next().unwrap();
        assert!(source.content.contains("contract Counter"), "{source:?}");

        CompilationArtifacts {
            bytecode: vec![0; 32],
            abi: counter_contract_abi(),
        }
    });
    let verifier = ContractVerifier::with_resolver(
        Duration::from_secs(60),
        pool.clone(),
        Arc::new(mock_resolver),
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

    assert_request_success(&mut storage, request_id, address, &expected_bytecode).await;
}

async fn assert_request_success(
    storage: &mut Connection<'_, Core>,
    request_id: usize,
    address: Address,
    expected_bytecode: &[u8],
) {
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
    assert_eq!(verification_info.artifacts.bytecode, *expected_bytecode);
    assert_eq!(verification_info.artifacts.abi, counter_contract_abi());
}

async fn checked_env_resolver() -> Option<(EnvCompilerResolver, SupportedCompilerVersions)> {
    let compiler_resolver = EnvCompilerResolver::default();
    let supported_compilers = compiler_resolver.supported_versions().await.ok()?;
    if supported_compilers.zksolc.is_empty() || supported_compilers.solc.is_empty() {
        return None;
    }
    Some((compiler_resolver, supported_compilers))
}

#[tokio::test]
async fn bytecode_mismatch_error() {
    let pool = ConnectionPool::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    prepare_storage(&mut storage).await;

    let address = Address::repeat_byte(1);
    mock_deployment(&mut storage, address, vec![0xff; 32]).await;
    let req = test_request(address);
    let request_id = storage
        .contract_verification_dal()
        .add_contract_verification_request(req)
        .await
        .unwrap();

    let mock_resolver = MockCompilerResolver::new(|_| CompilationArtifacts {
        bytecode: vec![0; 32],
        abi: counter_contract_abi(),
    });
    let verifier = ContractVerifier::with_resolver(
        Duration::from_secs(60),
        pool.clone(),
        Arc::new(mock_resolver),
    )
    .await
    .unwrap();

    let (_stop_sender, stop_receiver) = watch::channel(false);
    verifier.run(stop_receiver, Some(1)).await.unwrap();

    let status = storage
        .contract_verification_dal()
        .get_verification_request_status(request_id)
        .await
        .unwrap()
        .expect("no status");
    assert_eq!(status.status, "failed");
    assert!(status.compilation_errors.is_none(), "{status:?}");
    let error = status.error.unwrap();
    assert!(error.contains("bytecode"), "{error}");
}

#[tokio::test]
async fn no_compiler_version() {
    let pool = ConnectionPool::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    prepare_storage(&mut storage).await;

    let address = Address::repeat_byte(1);
    mock_deployment(&mut storage, address, vec![0xff; 32]).await;
    let req = VerificationIncomingRequest {
        compiler_versions: CompilerVersions::Solc {
            compiler_zksolc_version: ZKSOLC_VERSION.to_owned(),
            compiler_solc_version: "1.0.0".to_owned(), // a man can dream
        },
        ..test_request(address)
    };
    let request_id = storage
        .contract_verification_dal()
        .add_contract_verification_request(req)
        .await
        .unwrap();

    let mock_resolver =
        MockCompilerResolver::new(|_| unreachable!("should reject unknown solc version"));
    let verifier = ContractVerifier::with_resolver(
        Duration::from_secs(60),
        pool.clone(),
        Arc::new(mock_resolver),
    )
    .await
    .unwrap();

    let (_stop_sender, stop_receiver) = watch::channel(false);
    verifier.run(stop_receiver, Some(1)).await.unwrap();

    let status = storage
        .contract_verification_dal()
        .get_verification_request_status(request_id)
        .await
        .unwrap()
        .expect("no status");
    assert_eq!(status.status, "failed");
    assert!(status.compilation_errors.is_none(), "{status:?}");
    let error = status.error.unwrap();
    assert!(error.contains("solc version"), "{error}");
}
