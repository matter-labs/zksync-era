//! Tests for the contract verifier.

use std::{os::unix::fs::PermissionsExt, path::Path};

use tokio::{fs, sync::watch};
use zksync_dal::Connection;
use zksync_types::{
    contract_verification_api::{CompilerVersions, VerificationIncomingRequest},
    get_code_key, get_known_code_key,
    tx::IncludedTxLocation,
    L1BatchNumber, L2BlockNumber, StorageLog, CONTRACT_DEPLOYER_ADDRESS, H256,
};
use zksync_utils::{
    address_to_h256,
    bytecode::{hash_bytecode, validate_bytecode},
};
use zksync_vm_interface::VmEvent;

use super::*;

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
            &HashMap::from([(bytecode_hash, bytecode)]),
        )
        .await
        .unwrap();

    let deployer_address = Address::repeat_byte(0xff);
    let location = IncludedTxLocation {
        tx_hash: H256::zero(),
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

async fn prepare_compilers() -> CompilerPaths {
    let solc_version = SOLC_VERSION.parse().unwrap();
    let solc_path = svm::install(&solc_version)
        .await
        .expect("failed installing solc");
    let zksolc_path = svm::data_dir()
        .join(format!("zksolc-{ZKSOLC_VERSION}"))
        .join("zksolc");
    install_zksolc(&zksolc_path).await;

    CompilerPaths {
        base: solc_path,
        zk: zksolc_path,
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

async fn compile_counter() -> CompilationArtifacts {
    let compilers_path = prepare_compilers().await;
    let req = test_request(Address::repeat_byte(1));
    let input = ContractVerifier::build_zksolc_input(VerificationRequest { id: 0, req }).unwrap();
    ZkSolc::new(compilers_path, ZKSOLC_VERSION.to_owned())
        .async_compile(input)
        .await
        .unwrap()
}

#[tokio::test]
async fn compiler_works() {
    let output = compile_counter().await;
    validate_bytecode(&output.bytecode).unwrap();
    let items = output.abi.as_array().unwrap();
    assert_eq!(items.len(), 1);
    let increment_function = items[0].as_object().unwrap();
    assert_eq!(increment_function["type"], "function");
    assert_eq!(increment_function["name"], "increment");
}

// FIXME: needs customizing compiler resolution to work
#[tokio::test]
async fn contract_verifier_basics() {
    let output = compile_counter().await;
    let pool = ConnectionPool::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    let address = Address::repeat_byte(1);
    mock_deployment(&mut storage, address, output.bytecode.clone()).await;
    let req = test_request(address);
    storage
        .contract_verification_dal()
        .add_contract_verification_request(req)
        .await
        .unwrap();

    let config = ContractVerifierConfig {
        compilation_timeout: 86_400, // sec
        polling_interval: Some(50),  // ms

        // Fields below are unused
        prometheus_port: 0,
        threads_per_server: None,
        port: 0,
        url: "".to_string(),
    };
    let verifier = ContractVerifier::new(config, pool.clone());
    let (_stop_sender, stop_receiver) = watch::channel(false);
    verifier.run(stop_receiver, Some(1)).await.unwrap();

    let verification_info = storage
        .contract_verification_dal()
        .get_contract_verification_info(address)
        .await
        .unwrap()
        .expect("no verification info");
    assert_eq!(verification_info.artifacts.bytecode, output.bytecode);
    assert_eq!(verification_info.artifacts.abi, output.abi);
}
