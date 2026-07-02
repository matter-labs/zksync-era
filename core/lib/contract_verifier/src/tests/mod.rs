//! Tests for the contract verifier.

use std::collections::{HashMap, HashSet};

use test_casing::{test_casing, Product};
use tokio::sync::watch;
use zksync_dal::Connection;
use zksync_node_test_utils::{create_l1_batch, create_l2_block};
use zksync_types::{
    address_to_h256,
    bytecode::{pad_evm_bytecode, BytecodeHash},
    contract_verification::api::{
        CompilerVersions, ImmutableReference, SourceCodeData, VerificationIncomingRequest,
    },
    get_code_key, get_known_code_key,
    l2::L2Tx,
    tx::IncludedTxLocation,
    Execute, L1BatchNumber, L2BlockNumber, ProtocolVersion, StorageLog, CONTRACT_DEPLOYER_ADDRESS,
    H256, U256,
};
use zksync_vm_interface::{tracer::ValidationTraces, TransactionExecutionMetrics, VmEvent};

use super::*;
use crate::{
    compilers::{SolcInput, VyperInput, ZkSolcInput},
    resolver::{Compiler, SupportedCompilerVersions},
};

mod real;

const SOLC_VERSION: &str = "0.8.27";
const ZKSOLC_VERSION: &str = "1.5.4";
/// A zksolc version `>= 1.5.13` (CBOR-capable). Used by metadata-disabledness tests that need to
/// exercise the post-1.5.13 regime of [`SourceCodeData::appended_metadata_disabled`].
const ZKSOLC_VERSION_WITH_CBOR: &str = "1.5.14";

const BYTECODE_KINDS: [BytecodeMarker; 2] = [BytecodeMarker::EraVm, BytecodeMarker::Evm];

const COUNTER_CONTRACT: &str = r#"
    contract Counter {
        uint256 value;

        function increment(uint256 x) external {
            value += x;
        }
    }
"#;
const COUNTER_CONTRACT_WITH_CONSTRUCTOR: &str = r#"
    contract Counter {
        uint256 value;

        constructor(uint256 _value) {
            value = _value;
        }

        function increment(uint256 x) external {
            value += x;
        }
    }
"#;
const COUNTER_CONTRACT_WITH_INTERFACE: &str = r#"
    interface ICounter {
        function increment(uint256 x) external;
    }

    contract Counter is ICounter {
        uint256 value;

        function increment(uint256 x) external override {
            value += x;
        }
    }
"#;
const COUNTER_VYPER_CONTRACT: &str = r#"
#pragma version ^0.3.10

value: uint256

@external
def increment(x: uint256):
    self.value += x
"#;
const EMPTY_YUL_CONTRACT: &str = r#"
object "Empty" {
    code {
        mstore(0, 0)
        return(0, 32)
    }
    object "Empty_deployed" {
        code { }
    }
}
"#;
const COUNTER_CONTRACT_WITH_IMMUTABLE: &str = r#"
    contract CounterWithImmutable {
        uint256 public immutable base;
        uint256 public value;

        constructor(uint256 _base, uint256 _value) {
            base = _base;
            value = _value;
        }

        function increment(uint256 x) external {
            value += (base + x);
        }
    }
"#;

#[derive(Debug, Clone, Copy)]
enum TestContract {
    Counter,
    CounterWithConstructor,
}

impl TestContract {
    const ALL: [Self; 2] = [Self::Counter, Self::CounterWithConstructor];

    fn source(self) -> &'static str {
        match self {
            Self::Counter => COUNTER_CONTRACT,
            Self::CounterWithConstructor => COUNTER_CONTRACT_WITH_CONSTRUCTOR,
        }
    }

    fn constructor_args(self) -> &'static [Token] {
        match self {
            Self::Counter => &[],
            Self::CounterWithConstructor => &[Token::Uint(U256([42, 0, 0, 0]))],
        }
    }
}

async fn mock_deployment(
    storage: &mut Connection<'_, Core>,
    address: Address,
    bytecode: Vec<u8>,
    constructor_args: &[Token],
) {
    let bytecode_hash = BytecodeHash::for_bytecode(&bytecode).value();
    let deployment = Execute::for_deploy(H256::zero(), bytecode.clone(), constructor_args);
    mock_deployment_inner(storage, address, bytecode_hash, bytecode, deployment).await;
}

async fn mock_evm_deployment(
    storage: &mut Connection<'_, Core>,
    address: Address,
    creation_bytecode: Vec<u8>,
    deployed_bytecode: &[u8],
    constructor_args: &[Token],
) {
    let stored_bytecode = pad_evm_bytecode(deployed_bytecode);
    mock_evm_deployment_with_stored_bytecode(
        storage,
        address,
        creation_bytecode,
        deployed_bytecode,
        constructor_args,
        stored_bytecode,
    )
    .await;
}

async fn mock_raw_evm_deployment(
    storage: &mut Connection<'_, Core>,
    address: Address,
    creation_bytecode: Vec<u8>,
    deployed_bytecode: &[u8],
    constructor_args: &[Token],
) {
    mock_evm_deployment_with_stored_bytecode(
        storage,
        address,
        creation_bytecode,
        deployed_bytecode,
        constructor_args,
        deployed_bytecode.to_vec(),
    )
    .await;
}

async fn mock_evm_deployment_with_stored_bytecode(
    storage: &mut Connection<'_, Core>,
    address: Address,
    creation_bytecode: Vec<u8>,
    deployed_bytecode: &[u8],
    constructor_args: &[Token],
    stored_bytecode: Vec<u8>,
) {
    let mut calldata = creation_bytecode;
    calldata.extend_from_slice(&ethabi::encode(constructor_args));
    let deployment = Execute {
        contract_address: None,
        calldata,
        value: 0.into(),
        factory_deps: vec![],
    };
    let bytecode_hash = BytecodeHash::for_raw_evm_bytecode(deployed_bytecode).value();
    mock_deployment_inner(storage, address, bytecode_hash, stored_bytecode, deployment).await;
}

async fn mock_deployment_inner(
    storage: &mut Connection<'_, Core>,
    address: Address,
    bytecode_hash: H256,
    bytecode: Vec<u8>,
    execute: Execute,
) {
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
        execute,
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

type SharedMockFn<In> = Arc<
    dyn Fn(In, Option<&str>) -> Result<CompilationArtifacts, ContractVerifierError> + Send + Sync,
>;

#[derive(Clone)]
struct MockCompilerResolver {
    zksolc: SharedMockFn<ZkSolcInput>,
    solc: SharedMockFn<SolcInput>,
    zksolc_version: Option<String>,
}

impl fmt::Debug for MockCompilerResolver {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MockCompilerResolver")
            .finish_non_exhaustive()
    }
}

impl MockCompilerResolver {
    fn zksolc(
        zksolc: impl Fn(ZkSolcInput) -> CompilationArtifacts + 'static + Send + Sync,
    ) -> Self {
        Self {
            zksolc: Arc::new(move |input, _version| Ok(zksolc(input))),
            solc: Arc::new(|input, _version| panic!("unexpected solc call: {input:?}")),
            zksolc_version: None,
        }
    }

    fn zksolc_with_version(
        zksolc: impl Fn(ZkSolcInput, &str) -> CompilationArtifacts + 'static + Send + Sync,
    ) -> Self {
        Self {
            zksolc: Arc::new(move |input, version| Ok(zksolc(input, version.unwrap()))),
            solc: Arc::new(|input, _version| panic!("unexpected solc call: {input:?}")),
            zksolc_version: None,
        }
    }

    fn solc(solc: impl Fn(SolcInput) -> CompilationArtifacts + 'static + Send + Sync) -> Self {
        Self {
            solc: Arc::new(move |input, _version| Ok(solc(input))),
            zksolc: Arc::new(|input, _version| panic!("unexpected zksolc call: {input:?}")),
            zksolc_version: None,
        }
    }
}

#[async_trait]
impl Compiler<ZkSolcInput> for MockCompilerResolver {
    async fn compile(
        self: Box<Self>,
        input: ZkSolcInput,
    ) -> Result<CompilationArtifacts, ContractVerifierError> {
        (self.zksolc)(input, self.zksolc_version.as_deref())
    }
}

#[async_trait]
impl Compiler<SolcInput> for MockCompilerResolver {
    async fn compile(
        self: Box<Self>,
        input: SolcInput,
    ) -> Result<CompilationArtifacts, ContractVerifierError> {
        (self.solc)(input, None)
    }
}

#[async_trait]
impl CompilerResolver for MockCompilerResolver {
    async fn supported_versions(&self) -> anyhow::Result<SupportedCompilerVersions> {
        Ok(SupportedCompilerVersions {
            solc: [SOLC_VERSION.to_owned()].into_iter().collect(),
            zksolc: [
                ZKSOLC_VERSION.to_owned(),
                ZKSOLC_VERSION_WITH_CBOR.to_owned(),
            ]
            .into_iter()
            .collect(),
            vyper: HashSet::default(),
            zkvyper: HashSet::default(),
        })
    }

    async fn resolve_solc(
        &self,
        version: &str,
    ) -> Result<Box<dyn Compiler<SolcInput>>, ContractVerifierError> {
        if version != SOLC_VERSION {
            return Err(ContractVerifierError::UnknownCompilerVersion(
                "solc",
                version.to_owned(),
            ));
        }
        Ok(Box::new(self.clone()))
    }

    async fn resolve_zksolc(
        &self,
        version: &ZkCompilerVersions,
    ) -> Result<Box<dyn Compiler<ZkSolcInput>>, ContractVerifierError> {
        if version.base != SOLC_VERSION {
            return Err(ContractVerifierError::UnknownCompilerVersion(
                "solc",
                version.base.clone(),
            ));
        }
        let zk_version = version.zk.strip_prefix('v').unwrap_or(&version.zk);
        if zk_version != ZKSOLC_VERSION && zk_version != ZKSOLC_VERSION_WITH_CBOR {
            return Err(ContractVerifierError::UnknownCompilerVersion(
                "zksolc",
                version.zk.clone(),
            ));
        }
        Ok(Box::new(Self {
            zksolc_version: Some(version.zk.clone()),
            ..self.clone()
        }))
    }

    async fn resolve_vyper(
        &self,
        _version: &str,
    ) -> Result<Box<dyn Compiler<VyperInput>>, ContractVerifierError> {
        unreachable!("not tested")
    }

    async fn resolve_zkvyper(
        &self,
        _version: &ZkCompilerVersions,
    ) -> Result<Box<dyn Compiler<VyperInput>>, ContractVerifierError> {
        unreachable!("not tested")
    }
}

fn test_request(address: Address, source: &str) -> VerificationIncomingRequest {
    VerificationIncomingRequest {
        contract_address: address,
        source_code_data: SourceCodeData::SolSingleFile(source.into()),
        contract_name: "Counter".to_owned(),
        compiler_versions: CompilerVersions::Solc {
            compiler_zksolc_version: Some(ZKSOLC_VERSION.to_owned()),
            compiler_solc_version: SOLC_VERSION.to_owned(),
        },
        optimization_used: true,
        optimizer_mode: None,
        constructor_arguments: Default::default(),
        is_system: false,
        force_evmla: false,
        evm_specific: Default::default(),
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

#[test_casing(2, TestContract::ALL)]
#[tokio::test]
async fn contract_verifier_basics(contract: TestContract) {
    let pool = ConnectionPool::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    let expected_bytecode = vec![0_u8; 32];

    prepare_storage(&mut storage).await;
    let address = Address::repeat_byte(1);
    mock_deployment(
        &mut storage,
        address,
        expected_bytecode.clone(),
        contract.constructor_args(),
    )
    .await;
    let mut req = test_request(address, contract.source());
    req.constructor_arguments = ethabi::encode(contract.constructor_args()).into();
    let request_id = storage
        .contract_verification_dal()
        .add_contract_verification_request(&req)
        .await
        .unwrap();

    let mock_resolver = MockCompilerResolver::zksolc(|input| {
        let ZkSolcInput::StandardJson { input, .. } = &input else {
            panic!("unexpected input");
        };
        assert_eq!(input.language, "Solidity");
        assert_eq!(input.sources.len(), 1);
        let source = input.sources.values().next().unwrap();
        assert!(source.content.contains("contract Counter"), "{source:?}");

        CompilationArtifacts {
            bytecode: vec![0; 32],
            deployed_bytecode: None,
            abi: counter_contract_abi(),
            immutable_refs: Default::default(),
            factory_dependency_refs: Default::default(),
        }
    });
    let verifier = ContractVerifier::with_resolver(
        Duration::from_secs(60),
        pool.clone(),
        Arc::new(mock_resolver),
        false,
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
    // Ordered by version text, so `1.5.14` sorts before `1.5.4`.
    assert_eq!(zksolc_versions, [ZKSOLC_VERSION_WITH_CBOR, ZKSOLC_VERSION]);

    let (_stop_sender, stop_receiver) = watch::channel(false);
    verifier.run(stop_receiver, Some(1)).await.unwrap();

    assert_request_success(&mut storage, request_id, address, &expected_bytecode, &[]).await;
}

async fn assert_request_success(
    storage: &mut Connection<'_, Core>,
    request_id: usize,
    address: Address,
    expected_bytecode: &[u8],
    verification_problems: &[VerificationProblem],
) -> VerificationInfo {
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
    assert_eq!(
        without_internal_types(verification_info.artifacts.abi.clone()),
        without_internal_types(counter_contract_abi())
    );
    assert_eq!(
        &verification_info.verification_problems,
        verification_problems
    );

    verification_info
}

fn without_internal_types(mut abi: serde_json::Value) -> serde_json::Value {
    let items = abi.as_array_mut().unwrap();
    for item in items {
        if let Some(inputs) = item.get_mut("inputs") {
            let inputs = inputs.as_array_mut().unwrap();
            for input in inputs {
                input.as_object_mut().unwrap().remove("internalType");
            }
        }
        if let Some(outputs) = item.get_mut("outputs") {
            let outputs = outputs.as_array_mut().unwrap();
            for output in outputs {
                output.as_object_mut().unwrap().remove("internalType");
            }
        }
    }
    abi
}

#[test_casing(2, TestContract::ALL)]
#[tokio::test]
async fn verifying_evm_bytecode(contract: TestContract) {
    let pool = ConnectionPool::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    let creation_bytecode = vec![3_u8; 20];
    let deployed_bytecode = vec![5_u8; 10];

    prepare_storage(&mut storage).await;
    let address = Address::repeat_byte(1);
    mock_evm_deployment(
        &mut storage,
        address,
        creation_bytecode.clone(),
        &deployed_bytecode,
        contract.constructor_args(),
    )
    .await;
    let mut req = test_request(address, contract.source());
    req.compiler_versions = CompilerVersions::Solc {
        compiler_solc_version: SOLC_VERSION.to_owned(),
        compiler_zksolc_version: None,
    };
    req.constructor_arguments = ethabi::encode(contract.constructor_args()).into();
    let request_id = storage
        .contract_verification_dal()
        .add_contract_verification_request(&req)
        .await
        .unwrap();

    let artifacts = CompilationArtifacts {
        bytecode: creation_bytecode.clone(),
        deployed_bytecode: Some(deployed_bytecode),
        abi: counter_contract_abi(),
        immutable_refs: Default::default(),
        factory_dependency_refs: Default::default(),
    };
    let mock_resolver = MockCompilerResolver::solc(move |input| {
        assert_eq!(input.standard_json.language, "Solidity");
        assert_eq!(input.standard_json.sources.len(), 1);
        let source = input.standard_json.sources.values().next().unwrap();
        assert!(source.content.contains("contract Counter"), "{source:?}");

        artifacts.clone()
    });
    let verifier = ContractVerifier::with_resolver(
        Duration::from_secs(60),
        pool.clone(),
        Arc::new(mock_resolver),
        false,
    )
    .await
    .unwrap();

    let (_stop_sender, stop_receiver) = watch::channel(false);
    verifier.run(stop_receiver, Some(1)).await.unwrap();

    assert_request_success(&mut storage, request_id, address, &creation_bytecode, &[]).await;
}

#[tokio::test]
async fn verifying_evm_with_raw_stored_bytecode() {
    let pool = ConnectionPool::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    let creation_bytecode = vec![3_u8; 20];
    let deployed_bytecode = vec![5_u8; 10];

    prepare_storage(&mut storage).await;
    let address = Address::repeat_byte(1);
    mock_raw_evm_deployment(
        &mut storage,
        address,
        creation_bytecode.clone(),
        &deployed_bytecode,
        &[],
    )
    .await;
    let mut req = test_request(address, COUNTER_CONTRACT);
    req.compiler_versions = CompilerVersions::Solc {
        compiler_solc_version: SOLC_VERSION.to_owned(),
        compiler_zksolc_version: None,
    };
    let request_id = storage
        .contract_verification_dal()
        .add_contract_verification_request(&req)
        .await
        .unwrap();

    let artifacts = CompilationArtifacts {
        bytecode: creation_bytecode.clone(),
        deployed_bytecode: Some(deployed_bytecode),
        abi: counter_contract_abi(),
        immutable_refs: Default::default(),
        factory_dependency_refs: Default::default(),
    };
    let mock_resolver = MockCompilerResolver::solc(move |_| artifacts.clone());
    let verifier = ContractVerifier::with_resolver(
        Duration::from_secs(60),
        pool.clone(),
        Arc::new(mock_resolver),
        false,
    )
    .await
    .unwrap();

    let (_stop_sender, stop_receiver) = watch::channel(false);
    verifier.run(stop_receiver, Some(1)).await.unwrap();

    assert_request_success(&mut storage, request_id, address, &creation_bytecode, &[]).await;
}

#[tokio::test]
async fn bytecode_mismatch_error() {
    let pool = ConnectionPool::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    prepare_storage(&mut storage).await;

    let address = Address::repeat_byte(1);
    mock_deployment(&mut storage, address, vec![0xff; 32], &[]).await;
    let req = test_request(address, COUNTER_CONTRACT);
    let request_id = storage
        .contract_verification_dal()
        .add_contract_verification_request(&req)
        .await
        .unwrap();

    let mock_resolver = MockCompilerResolver::zksolc(|_| CompilationArtifacts {
        bytecode: vec![0; 32],
        deployed_bytecode: None,
        abi: counter_contract_abi(),
        immutable_refs: Default::default(),
        factory_dependency_refs: Default::default(),
    });
    let verifier = ContractVerifier::with_resolver(
        Duration::from_secs(60),
        pool.clone(),
        Arc::new(mock_resolver),
        false,
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
    let err = status.error.unwrap();
    assert_eq!(err, ContractVerifierError::BytecodeMismatch.to_string());
}

/// EraVM bytecode of a `value() -> 1` contract compiled with `appendCBOR = false`, i.e. with no
/// metadata. Its trailing word is functional code (zeroing it changes the return value), not a hash.
const NO_METADATA_BYTECODE: &str = "0000008003000039000000400030043f0000000100200190000000110000c13d0000000900100198000000190000613d000000000101043b0000000a011001970000000b0010009c000000190000c13d0000000001000416000000000001004b000000190000c13d0000000101000039000000800010043f0000000c010000410000001c0001042e0000000001000416000000000001004b000000190000c13d00000020010000390000010000100443000001200000044300000008010000410000001c0001042e00000000010000190000001d000104300000001b000004320000001c0001042e0000001d0001043000000000000000000000000000000000000000020000000000000000000000000000004000000100000000000000000000000000000000000000000000000000fffffffc000000000000000000000000ffffffff000000000000000000000000000000000000000000000000000000003fa4f245000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000020000000800000000000000000";

fn no_metadata_standard_json_request(
    address: Address,
    metadata: serde_json::Value,
    zksolc_version: &str,
) -> VerificationIncomingRequest {
    let standard_json = serde_json::json!({
        "language": "Solidity",
        "sources": {
            "Counter.sol": {
                "content": "// SPDX-License-Identifier: UNLICENSED\npragma solidity ^0.8.30;\ncontract Counter { function value() external pure returns (uint256) { return 1; } }\n"
            }
        },
        "settings": {
            "metadata": metadata,
            "optimizer": { "enabled": true, "mode": "3" }
        }
    });
    VerificationIncomingRequest {
        contract_address: address,
        source_code_data: SourceCodeData::StandardJsonInput(
            standard_json.as_object().unwrap().clone(),
        ),
        contract_name: "Counter".to_owned(),
        compiler_versions: CompilerVersions::Solc {
            compiler_zksolc_version: Some(zksolc_version.to_owned()),
            compiler_solc_version: SOLC_VERSION.to_owned(),
        },
        optimization_used: true,
        optimizer_mode: None,
        constructor_arguments: Default::default(),
        is_system: false,
        force_evmla: false,
        evm_specific: Default::default(),
    }
}

/// A deployed contract differing from the verified source only in the final word must NOT pass as a
/// partial match when metadata is disabled (that word is functional code). Cased over `(metadata
/// settings, zksolc version)` pairs that yield metadata-less bytecode, which is version-dependent:
/// `>= 1.5.13` needs `appendCBOR:false` + non-`keccak256` hash; `< 1.5.13` needs `bytecodeHash:none`.
#[test_casing(4, [
    (serde_json::json!({ "bytecodeHash": "none", "appendCBOR": false }), "1.5.14"),
    (serde_json::json!({ "appendCBOR": false }), "1.5.14"),
    (serde_json::json!({ "bytecodeHash": "ipfs", "appendCBOR": false }), "1.5.14"),
    (serde_json::json!({ "bytecodeHash": "none" }), "1.5.4"),
])]
#[tokio::test]
async fn no_metadata_final_word_mismatch_is_rejected(
    metadata: serde_json::Value,
    zksolc_version: &str,
) {
    let pool = ConnectionPool::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    prepare_storage(&mut storage).await;

    let original = hex::decode(NO_METADATA_BYTECODE).unwrap();
    // The attacker deploys bytecode that differs from the verified source only in the final word.
    let mut deployed = original.clone();
    let len = deployed.len();
    deployed[len - 32..].fill(0);
    assert_ne!(original, deployed);

    let address = Address::repeat_byte(1);
    mock_deployment(&mut storage, address, deployed, &[]).await;
    let req = no_metadata_standard_json_request(address, metadata, zksolc_version);
    let request_id = storage
        .contract_verification_dal()
        .add_contract_verification_request(&req)
        .await
        .unwrap();

    // The verifier honestly recompiles the source and reproduces the ORIGINAL bytecode.
    let mock_resolver = MockCompilerResolver::zksolc(move |_| CompilationArtifacts {
        bytecode: original.clone(),
        deployed_bytecode: None,
        abi: counter_contract_abi(),
        immutable_refs: Default::default(),
        factory_dependency_refs: Default::default(),
    });
    let verifier = ContractVerifier::with_resolver(
        Duration::from_secs(60),
        pool.clone(),
        Arc::new(mock_resolver),
        false,
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
    assert_eq!(status.status, "failed", "{status:?}");
    assert!(status.compilation_errors.is_none(), "{status:?}");
    let err = status.error.unwrap();
    assert_eq!(err, ContractVerifierError::BytecodeMismatch.to_string());
}

/// Counterpart to [`no_metadata_final_word_mismatch_is_rejected`]: when metadata is NOT disabled, a
/// trailing-word difference must still be accepted as a partial match (no over-rejection).
#[tokio::test]
async fn metadata_final_word_mismatch_is_partial_match() {
    let pool = ConnectionPool::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    prepare_storage(&mut storage).await;

    let original = hex::decode(NO_METADATA_BYTECODE).unwrap();
    let mut deployed = original.clone();
    let len = deployed.len();
    deployed[len - 32..].fill(0);

    let address = Address::repeat_byte(1);
    mock_deployment(&mut storage, address, deployed, &[]).await;
    // `SolSingleFile` does not disable metadata, so the trailing-word difference is treated as a
    // (benign) metadata mismatch, i.e. a partial match.
    let req = test_request(address, COUNTER_CONTRACT);
    let request_id = storage
        .contract_verification_dal()
        .add_contract_verification_request(&req)
        .await
        .unwrap();

    let original_for_assert = original.clone();
    let mock_resolver = MockCompilerResolver::zksolc(move |_| CompilationArtifacts {
        bytecode: original.clone(),
        deployed_bytecode: None,
        abi: counter_contract_abi(),
        immutable_refs: Default::default(),
        factory_dependency_refs: Default::default(),
    });
    let verifier = ContractVerifier::with_resolver(
        Duration::from_secs(60),
        pool.clone(),
        Arc::new(mock_resolver),
        false,
    )
    .await
    .unwrap();

    let (_stop_sender, stop_receiver) = watch::channel(false);
    verifier.run(stop_receiver, Some(1)).await.unwrap();

    // The stored artifacts are the recompiled (original) bytecode.
    assert_request_success(
        &mut storage,
        request_id,
        address,
        &original_for_assert,
        &[VerificationProblem::IncorrectMetadata],
    )
    .await;
}

#[test_casing(4, Product((TestContract::ALL, BYTECODE_KINDS)))]
#[tokio::test]
async fn args_mismatch_error(contract: TestContract, bytecode_kind: BytecodeMarker) {
    let pool = ConnectionPool::test_pool().await;
    let mut storage = pool.connection().await.unwrap();

    prepare_storage(&mut storage).await;
    let address = Address::repeat_byte(1);
    let bytecode = vec![0_u8; 32];
    match bytecode_kind {
        BytecodeMarker::EraVm => {
            mock_deployment(
                &mut storage,
                address,
                bytecode.clone(),
                contract.constructor_args(),
            )
            .await;
        }
        BytecodeMarker::Evm => {
            let creation_bytecode = vec![3_u8; 48];
            mock_evm_deployment(
                &mut storage,
                address,
                creation_bytecode,
                &bytecode,
                contract.constructor_args(),
            )
            .await;
        }
    }

    let mut req = test_request(address, contract.source());
    if matches!(bytecode_kind, BytecodeMarker::Evm) {
        req.compiler_versions = CompilerVersions::Solc {
            compiler_zksolc_version: None,
            compiler_solc_version: SOLC_VERSION.to_owned(),
        };
    }

    // Intentionally encode incorrect constructor args
    req.constructor_arguments = match contract {
        TestContract::Counter => ethabi::encode(&[Token::Bool(true)]).into(),
        TestContract::CounterWithConstructor => ethabi::encode(&[]).into(),
    };
    let request_id = storage
        .contract_verification_dal()
        .add_contract_verification_request(&req)
        .await
        .unwrap();

    let mock_resolver = match bytecode_kind {
        BytecodeMarker::EraVm => MockCompilerResolver::zksolc(move |_| CompilationArtifacts {
            bytecode: bytecode.clone(),
            deployed_bytecode: None,
            abi: counter_contract_abi(),
            immutable_refs: Default::default(),
            factory_dependency_refs: Default::default(),
        }),
        BytecodeMarker::Evm => MockCompilerResolver::solc(move |_| CompilationArtifacts {
            bytecode: vec![3_u8; 48],
            deployed_bytecode: Some(bytecode.clone()),
            abi: counter_contract_abi(),
            immutable_refs: Default::default(),
            factory_dependency_refs: Default::default(),
        }),
    };
    let verifier = ContractVerifier::with_resolver(
        Duration::from_secs(60),
        pool.clone(),
        Arc::new(mock_resolver),
        false,
    )
    .await
    .unwrap();

    let (_stop_sender, stop_receiver) = watch::channel(false);
    verifier.run(stop_receiver, Some(1)).await.unwrap();

    assert_constructor_args_mismatch(&mut storage, request_id).await;
}

async fn assert_constructor_args_mismatch(storage: &mut Connection<'_, Core>, request_id: usize) {
    let status = storage
        .contract_verification_dal()
        .get_verification_request_status(request_id)
        .await
        .unwrap()
        .expect("no status");
    assert_eq!(status.status, "failed");
    assert_eq!(status.compilation_errors, None);
    let err = status.error.unwrap();
    assert_eq!(
        err,
        ContractVerifierError::IncorrectConstructorArguments.to_string()
    );
}

#[tokio::test]
async fn creation_bytecode_mismatch() {
    let pool = ConnectionPool::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    prepare_storage(&mut storage).await;

    let address = Address::repeat_byte(1);
    let creation_bytecode = vec![3; 20];
    let deployed_bytecode = vec![5; 10];
    mock_evm_deployment(
        &mut storage,
        address,
        creation_bytecode,
        &deployed_bytecode,
        &[],
    )
    .await;
    let mut req = test_request(address, COUNTER_CONTRACT);
    req.compiler_versions = CompilerVersions::Solc {
        compiler_zksolc_version: None,
        compiler_solc_version: SOLC_VERSION.to_owned(),
    };
    let request_id = storage
        .contract_verification_dal()
        .add_contract_verification_request(&req)
        .await
        .unwrap();

    let mock_resolver = MockCompilerResolver::solc(move |_| {
        CompilationArtifacts {
            bytecode: vec![4; 20], // differs from `creation_bytecode`
            deployed_bytecode: Some(deployed_bytecode.clone()),
            abi: counter_contract_abi(),
            immutable_refs: Default::default(),
            factory_dependency_refs: Default::default(),
        }
    });
    let verifier = ContractVerifier::with_resolver(
        Duration::from_secs(60),
        pool.clone(),
        Arc::new(mock_resolver),
        false,
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
    let err = status.error.unwrap();
    assert_eq!(
        err,
        ContractVerifierError::CreationBytecodeMismatch.to_string()
    );
}

#[tokio::test]
async fn no_compiler_version() {
    let pool = ConnectionPool::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    prepare_storage(&mut storage).await;

    let address = Address::repeat_byte(1);
    mock_deployment(&mut storage, address, vec![0xff; 32], &[]).await;
    let req = VerificationIncomingRequest {
        compiler_versions: CompilerVersions::Solc {
            compiler_zksolc_version: Some(ZKSOLC_VERSION.to_owned()),
            compiler_solc_version: "1.0.0".to_owned(), // a man can dream
        },
        ..test_request(address, COUNTER_CONTRACT)
    };
    let request_id = storage
        .contract_verification_dal()
        .add_contract_verification_request(&req)
        .await
        .unwrap();

    let mock_resolver =
        MockCompilerResolver::zksolc(|_| unreachable!("should reject unknown solc version"));
    let verifier = ContractVerifier::with_resolver(
        Duration::from_secs(60),
        pool.clone(),
        Arc::new(mock_resolver),
        false,
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

#[tokio::test]
async fn verifying_evm_with_immutables() {
    let pool = ConnectionPool::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    prepare_storage(&mut storage).await;

    let creation_bytecode = vec![0x03; 10];
    let mut deployed_bytecode = vec![0x05; 10];
    // Place the immutables deployed_bytecode code at offsets 4..6:
    deployed_bytecode[4..6].copy_from_slice(&[0xAA, 0xBB]);

    let address = Address::repeat_byte(1);
    mock_evm_deployment(
        &mut storage,
        address,
        creation_bytecode.clone(),
        &deployed_bytecode,
        &[],
    )
    .await;

    let mut req = test_request(address, COUNTER_CONTRACT_WITH_IMMUTABLE);
    req.compiler_versions = CompilerVersions::Solc {
        compiler_solc_version: SOLC_VERSION.to_owned(),
        compiler_zksolc_version: None,
    };
    let request_id = storage
        .contract_verification_dal()
        .add_contract_verification_request(&req)
        .await
        .unwrap();

    let mut deployed_bytecode_compiled = vec![0x05; 10];
    deployed_bytecode_compiled[4..6].copy_from_slice(&[0x00, 0x00]);

    let mut imm_map = HashMap::new();
    imm_map.insert(
        "somePlaceholder".to_string(),
        vec![ImmutableReference {
            start: 4,
            length: 2,
        }],
    );

    let artifacts = CompilationArtifacts {
        bytecode: creation_bytecode.clone(),
        deployed_bytecode: Some(deployed_bytecode_compiled),
        abi: counter_contract_abi(),
        immutable_refs: imm_map,
        factory_dependency_refs: Default::default(),
    };

    let mock_resolver = MockCompilerResolver::solc(move |_| artifacts.clone());

    let verifier = ContractVerifier::with_resolver(
        Duration::from_secs(60),
        pool.clone(),
        Arc::new(mock_resolver),
        false,
    )
    .await
    .unwrap();

    let (_stop_sender, stop_receiver) = watch::channel(false);
    verifier.run(stop_receiver, Some(1)).await.unwrap();

    assert_request_success(&mut storage, request_id, address, &creation_bytecode, &[]).await;
}

#[tokio::test]
async fn verifying_era_vm_with_factory_dependency_hash_ref() {
    let pool = ConnectionPool::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    prepare_storage(&mut storage).await;

    let mut compiled_bytecode = vec![0x11; 96];
    compiled_bytecode[32..64].copy_from_slice(&[0xaa; 32]);
    let mut deployed_bytecode = compiled_bytecode.clone();
    deployed_bytecode[32..64].copy_from_slice(&[0xbb; 32]);

    let address = Address::repeat_byte(1);
    mock_deployment(&mut storage, address, deployed_bytecode, &[]).await;
    let req = test_request(address, COUNTER_CONTRACT);
    let request_id = storage
        .contract_verification_dal()
        .add_contract_verification_request(&req)
        .await
        .unwrap();

    let artifacts = CompilationArtifacts {
        bytecode: compiled_bytecode.clone(),
        deployed_bytecode: None,
        abi: counter_contract_abi(),
        immutable_refs: Default::default(),
        factory_dependency_refs: vec![ImmutableReference {
            start: 32,
            length: 32,
        }],
    };

    let mock_resolver = MockCompilerResolver::zksolc(move |_| artifacts.clone());
    let verifier = ContractVerifier::with_resolver(
        Duration::from_secs(60),
        pool.clone(),
        Arc::new(mock_resolver),
        false,
    )
    .await
    .unwrap();

    let (_stop_sender, stop_receiver) = watch::channel(false);
    verifier.run(stop_receiver, Some(1)).await.unwrap();

    assert_request_success(&mut storage, request_id, address, &compiled_bytecode, &[]).await;
}

#[tokio::test]
async fn metadata_version_fallback_patches_factory_dependency_hash_refs() {
    let pool = ConnectionPool::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    prepare_storage(&mut storage).await;

    let cbor_metadata = eravm_cbor_metadata_suffix_for_zksolc(ZKSOLC_VERSION_WITH_CBOR);
    let mut matching_bytecode = vec![0x11; 96];
    matching_bytecode[32..64].copy_from_slice(&[0xaa; 32]);
    matching_bytecode.extend_from_slice(&[0; 32]);
    matching_bytecode.extend_from_slice(&cbor_metadata);
    assert_eq!(matching_bytecode.len() / 32 % 2, 1);

    let mut deployed_bytecode = matching_bytecode.clone();
    deployed_bytecode[32..64].copy_from_slice(&[0xbb; 32]);

    let mut wrong_version_bytecode = matching_bytecode.clone();
    wrong_version_bytecode[0..32].copy_from_slice(&[0xcc; 32]);

    let address = Address::repeat_byte(1);
    mock_deployment(&mut storage, address, deployed_bytecode, &[]).await;
    let req = test_request(address, COUNTER_CONTRACT);
    let request_id = storage
        .contract_verification_dal()
        .add_contract_verification_request(&req)
        .await
        .unwrap();

    let expected_bytecode = matching_bytecode.clone();
    let metadata_zksolc_version = format!("v{ZKSOLC_VERSION_WITH_CBOR}");
    let mock_resolver = MockCompilerResolver::zksolc_with_version(move |_input, version| {
        let bytecode = if version == metadata_zksolc_version {
            matching_bytecode.clone()
        } else {
            wrong_version_bytecode.clone()
        };

        CompilationArtifacts {
            bytecode,
            deployed_bytecode: None,
            abi: counter_contract_abi(),
            immutable_refs: Default::default(),
            factory_dependency_refs: vec![ImmutableReference {
                start: 32,
                length: 32,
            }],
        }
    });
    let verifier = ContractVerifier::with_resolver(
        Duration::from_secs(60),
        pool.clone(),
        Arc::new(mock_resolver),
        false,
    )
    .await
    .unwrap();

    let (_stop_sender, stop_receiver) = watch::channel(false);
    verifier.run(stop_receiver, Some(1)).await.unwrap();

    assert_request_success(&mut storage, request_id, address, &expected_bytecode, &[]).await;
}

fn eravm_cbor_metadata_suffix_for_zksolc(version: &str) -> Vec<u8> {
    let compiler = format!("zksolc:{version}");
    let compiler = compiler.as_bytes();
    assert!(compiler.len() < 24);

    let mut metadata = vec![0xa1, 0x64];
    metadata.extend_from_slice(b"solc");
    metadata.push(0x60 + compiler.len() as u8);
    metadata.extend_from_slice(compiler);

    let aligned_metadata_len = metadata.len().div_ceil(32) * 32;
    assert!(metadata.len() + 2 <= aligned_metadata_len);

    let mut suffix = vec![0; aligned_metadata_len - metadata.len() - 2];
    suffix.extend_from_slice(&metadata);
    suffix.extend_from_slice(&(metadata.len() as u16).to_be_bytes());
    suffix
}
