use std::collections::HashMap;
use std::env;
use std::path::Path;
use std::time::{Duration, Instant};

use chrono::Utc;
use ethabi::Function;
use lazy_static::lazy_static;
use tokio::time;

use zksync_config::ContractVerifierConfig;
use zksync_dal::{ConnectionPool, StorageProcessor};
use zksync_queued_job_processor::{async_trait, JobProcessor};
use zksync_types::explorer_api::{
    CompilationArtifacts, DeployContractCalldata, SourceCodeData, VerificationInfo,
    VerificationRequest,
};

use crate::error::ContractVerifierError;
use crate::zksolc_utils::{CompilerInput, Optimizer, Settings, Source, ZkSolc};

lazy_static! {
    static ref CREATE_CONTRACT_FUNCTION: Function = zksync_contracts::deployer_contract()
        .function("create")
        .unwrap()
        .clone();
}

#[derive(Debug)]
pub struct ContractVerifier {
    config: ContractVerifierConfig,
}

impl ContractVerifier {
    pub fn new(config: ContractVerifierConfig) -> Self {
        Self { config }
    }

    async fn verify(
        storage: &mut StorageProcessor<'_>,
        mut request: VerificationRequest,
        config: ContractVerifierConfig,
    ) -> Result<VerificationInfo, ContractVerifierError> {
        let artifacts = Self::compile(request.clone(), config).await?;

        // Bytecode should be present because it is checked when accepting request.
        let (deployed_bytecode, creation_tx_calldata) = storage
            .explorer()
            .contract_verification_dal()
            .get_contract_info_for_verification(request.req.contract_address)
            .unwrap()
            .ok_or_else(|| {
                vlog::warn!("Contract is missing in DB for already accepted verification request. Contract address: {:#?}", request.req.contract_address);
                ContractVerifierError::InternalError
            })?;
        let (constructor_arguments, to_ignore) =
            Self::decode_constructor_arguments_from_calldata(creation_tx_calldata);

        if artifacts.bytecode == deployed_bytecode
            && (to_ignore || request.req.constructor_arguments.0 == constructor_arguments)
        {
            if to_ignore {
                request.req.constructor_arguments = Vec::new().into();
            }

            Ok(VerificationInfo {
                request,
                artifacts,
                verified_at: Utc::now(),
            })
        } else if artifacts.bytecode != deployed_bytecode {
            Err(ContractVerifierError::BytecodeMismatch)
        } else {
            Err(ContractVerifierError::IncorrectConstructorArguments)
        }
    }

    async fn compile(
        request: VerificationRequest,
        config: ContractVerifierConfig,
    ) -> Result<CompilationArtifacts, ContractVerifierError> {
        // Users may provide either just contract name or
        // source file name and contract name joined with ":".
        let (file_name, contract_name) =
            if let Some((file_name, contract_name)) = request.req.contract_name.rsplit_once(':') {
                (file_name.to_string(), contract_name.to_string())
            } else {
                (
                    format!("{}.sol", request.req.contract_name),
                    request.req.contract_name.clone(),
                )
            };
        let input = Self::build_compiler_input(request.clone(), file_name.clone())?;

        let zksync_home = env::var("ZKSYNC_HOME").unwrap_or_else(|_| ".".into());
        let zksolc_path = Path::new(&zksync_home)
            .join("etc")
            .join("zksolc-bin")
            .join(request.req.compiler_zksolc_version.as_str())
            .join("zksolc");
        if !zksolc_path.exists() {
            return Err(ContractVerifierError::UnknownZkSolcVersion(
                request.req.compiler_zksolc_version,
            ));
        }

        let solc_path = Path::new(&zksync_home)
            .join("etc")
            .join("solc-bin")
            .join(request.req.compiler_solc_version.as_str())
            .join("solc");
        if !solc_path.exists() {
            return Err(ContractVerifierError::UnknownSolcVersion(
                request.req.compiler_solc_version,
            ));
        }

        let zksolc = ZkSolc::new(zksolc_path, solc_path);

        let output = time::timeout(
            config.compilation_timeout(),
            zksolc.async_compile(&input, request.req.is_system),
        )
        .await
        .map_err(|_| ContractVerifierError::CompilationTimeout)??;

        if let Some(errors) = output.get("errors") {
            let errors = errors.as_array().unwrap().clone();
            if errors
                .iter()
                .any(|err| err["severity"].as_str().unwrap() == "error")
            {
                let error_messages = errors
                    .into_iter()
                    .map(|err| err["formattedMessage"].clone())
                    .collect();
                return Err(ContractVerifierError::CompilationError(
                    serde_json::Value::Array(error_messages),
                ));
            }
        }

        let contracts = output["contracts"]
            .get(file_name.as_str())
            .cloned()
            .ok_or(ContractVerifierError::MissingSource(file_name))?;
        let contract = contracts
            .get(&contract_name)
            .cloned()
            .ok_or(ContractVerifierError::MissingContract(contract_name))?;
        let bytecode_str = contract["evm"]["bytecode"]["object"].as_str().ok_or(
            ContractVerifierError::AbstractContract(request.req.contract_name),
        )?;
        let bytecode = hex::decode(bytecode_str).unwrap();
        let abi = contract["abi"].clone();
        if !abi.is_array() {
            vlog::error!(
                "zksolc returned unexpected value for ABI: {}",
                serde_json::to_string_pretty(&abi).unwrap()
            );
            return Err(ContractVerifierError::InternalError);
        }

        Ok(CompilationArtifacts { bytecode, abi })
    }

    fn build_compiler_input(
        request: VerificationRequest,
        file_name: String,
    ) -> Result<CompilerInput, ContractVerifierError> {
        let default_output_selection = serde_json::json!(
            {
                "*": {
                    "*": [ "abi" ],
                     "": [ "abi" ]
                }
            }
        );

        match request.req.source_code_data {
            SourceCodeData::SingleFile(source_code) => {
                let source = Source {
                    content: source_code,
                };
                let sources: HashMap<String, Source> =
                    vec![(file_name, source)].into_iter().collect();
                let optimizer = Optimizer::new(request.req.optimization_used);

                let settings = Settings {
                    libraries: None,
                    output_selection: Some(default_output_selection),
                    optimizer,
                };

                Ok(CompilerInput {
                    language: "Solidity".to_string(),
                    sources,
                    settings,
                })
            }
            SourceCodeData::StandardJsonInput(map) => {
                let mut compiler_input: CompilerInput =
                    serde_json::from_value(serde_json::Value::Object(map))
                        .map_err(|_| ContractVerifierError::FailedToDeserializeInput)?;
                // Set default output selection even if it is different in request.
                compiler_input.settings.output_selection = Some(default_output_selection);
                Ok(compiler_input)
            }
        }
    }

    fn decode_constructor_arguments_from_calldata(
        calldata: DeployContractCalldata,
    ) -> (Vec<u8>, bool) {
        match calldata {
            DeployContractCalldata::Deploy(calldata) => {
                // `calldata` is abi encoded call of `function create(bytes32 _salt, bytes32 _bytecodeHash, bytes _input)`.
                // Constructor arguments are in the third parameter.
                let tokens = CREATE_CONTRACT_FUNCTION
                    .decode_input(&calldata[4..])
                    .expect("Failed to decode constructor arguments");
                (
                    tokens[2]
                        .clone()
                        .into_bytes()
                        .expect("The third parameter of `create` should be of type `bytes`"),
                    false,
                )
            }
            DeployContractCalldata::Ignore => (Vec::new(), true),
        }
    }

    fn process_result(
        storage: &mut StorageProcessor<'_>,
        request_id: usize,
        verification_result: Result<VerificationInfo, ContractVerifierError>,
    ) {
        match verification_result {
            Ok(info) => {
                storage
                    .explorer()
                    .contract_verification_dal()
                    .save_verification_info(info)
                    .unwrap();
                vlog::info!("Successfully processed request with id = {}", request_id);
            }
            Err(error) => {
                let error_message = error.to_string();
                let compilation_errors = match error {
                    ContractVerifierError::CompilationError(compilation_errors) => {
                        compilation_errors
                    }
                    _ => serde_json::Value::Array(Vec::new()),
                };
                storage
                    .explorer()
                    .contract_verification_dal()
                    .save_verification_error(request_id, error_message, compilation_errors, None)
                    .unwrap();
                vlog::info!("Request with id = {} was failed", request_id);
            }
        }
    }
}

#[async_trait]
impl JobProcessor for ContractVerifier {
    type Job = VerificationRequest;
    type JobId = usize;
    type JobArtifacts = ();

    const SERVICE_NAME: &'static str = "contract_verifier";

    async fn get_next_job(
        &self,
        connection_pool: ConnectionPool,
    ) -> Option<(Self::JobId, Self::Job)> {
        let mut connection = connection_pool.access_storage().await;

        // Time overhead for all operations except for compilation.
        const TIME_OVERHEAD: Duration = Duration::from_secs(10);

        // Considering that jobs that reach compilation timeout will be executed in
        // `compilation_timeout` + `non_compilation_time_overhead` (which is significantly less than `compilation_timeout`),
        // we re-pick up jobs that are being executed for a bit more than `compilation_timeout`.
        let job = connection
            .explorer()
            .contract_verification_dal()
            .get_next_queued_verification_request(self.config.compilation_timeout() + TIME_OVERHEAD)
            .unwrap();

        job.map(|job| (job.id, job))
    }

    async fn save_failure(
        connection_pool: ConnectionPool,
        job_id: usize,
        _started_at: Instant,
        error: String,
    ) -> () {
        let mut connection = connection_pool.access_storage().await;

        connection
            .explorer()
            .contract_verification_dal()
            .save_verification_error(
                job_id,
                "Internal error".to_string(),
                serde_json::Value::Array(Vec::new()),
                Some(error),
            )
            .unwrap();
    }

    #[allow(clippy::async_yields_async)]
    async fn process_job(
        connection_pool: ConnectionPool,
        job: VerificationRequest,
        started_at: Instant,
    ) -> tokio::task::JoinHandle<()> {
        tokio::task::spawn(async move {
            vlog::info!("Started to process request with id = {}", job.id);

            let config: ContractVerifierConfig = ContractVerifierConfig::from_env();
            let mut connection = connection_pool.access_storage_blocking();

            let job_id = job.id;
            let verification_result = Self::verify(&mut connection, job, config).await;
            Self::process_result(&mut connection, job_id, verification_result);

            metrics::histogram!(
                "api.contract_verifier.request_processing_time",
                started_at.elapsed()
            );
        })
    }

    async fn save_result(_: ConnectionPool, _: Self::JobId, _: Instant, _: Self::JobArtifacts) {}
}
