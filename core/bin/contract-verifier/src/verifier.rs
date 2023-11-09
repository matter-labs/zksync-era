use std::collections::HashMap;
use std::env;
use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::Context as _;
use chrono::Utc;
use ethabi::{Contract, Token};
use lazy_static::lazy_static;
use regex::Regex;
use tokio::time;

use zksync_config::ContractVerifierConfig;
use zksync_dal::{ConnectionPool, StorageProcessor};
use zksync_env_config::FromEnv;
use zksync_queued_job_processor::{async_trait, JobProcessor};
use zksync_types::{
    contract_verification_api::{
        CompilationArtifacts, CompilerType, DeployContractCalldata, SourceCodeData,
        VerificationInfo, VerificationRequest,
    },
    Address,
};

use crate::error::ContractVerifierError;
use crate::zksolc_utils::{
    Optimizer, Settings, Source, StandardJson, ZkSolc, ZkSolcInput, ZkSolcOutput,
};
use crate::zkvyper_utils::{ZkVyper, ZkVyperInput};

lazy_static! {
    static ref DEPLOYER_CONTRACT: Contract = zksync_contracts::deployer_contract();
}

#[derive(Debug)]
enum ConstructorArgs {
    Check(Vec<u8>),
    Ignore,
}

#[derive(Debug)]
pub struct ContractVerifier {
    config: ContractVerifierConfig,
    connection_pool: ConnectionPool,
}

impl ContractVerifier {
    pub fn new(config: ContractVerifierConfig, connection_pool: ConnectionPool) -> Self {
        Self {
            config,
            connection_pool,
        }
    }

    async fn verify(
        storage: &mut StorageProcessor<'_>,
        mut request: VerificationRequest,
        config: ContractVerifierConfig,
    ) -> Result<VerificationInfo, ContractVerifierError> {
        let artifacts = Self::compile(request.clone(), config).await?;

        // Bytecode should be present because it is checked when accepting request.
        let (deployed_bytecode, creation_tx_calldata) = storage
            .contract_verification_dal()
            .get_contract_info_for_verification(request.req.contract_address).await
            .unwrap()
            .ok_or_else(|| {
                tracing::warn!("Contract is missing in DB for already accepted verification request. Contract address: {:#?}", request.req.contract_address);
                ContractVerifierError::InternalError
            })?;
        let constructor_args = Self::decode_constructor_arguments_from_calldata(
            creation_tx_calldata,
            request.req.contract_address,
        );

        if artifacts.bytecode != deployed_bytecode {
            return Err(ContractVerifierError::BytecodeMismatch);
        }

        match constructor_args {
            ConstructorArgs::Check(args) => {
                if request.req.constructor_arguments.0 != args {
                    return Err(ContractVerifierError::IncorrectConstructorArguments);
                }
            }
            ConstructorArgs::Ignore => {
                request.req.constructor_arguments = Vec::new().into();
            }
        }

        Ok(VerificationInfo {
            request,
            artifacts,
            verified_at: Utc::now(),
        })
    }

    async fn compile_zksolc(
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
        let input = Self::build_zksolc_input(request.clone(), file_name.clone())?;

        let zksync_home = env::var("ZKSYNC_HOME").unwrap_or_else(|_| ".".into());
        let zksolc_path = Path::new(&zksync_home)
            .join("etc")
            .join("zksolc-bin")
            .join(request.req.compiler_versions.zk_compiler_version())
            .join("zksolc");
        if !zksolc_path.exists() {
            return Err(ContractVerifierError::UnknownCompilerVersion(
                "zksolc".to_string(),
                request.req.compiler_versions.zk_compiler_version(),
            ));
        }

        let solc_path = Path::new(&zksync_home)
            .join("etc")
            .join("solc-bin")
            .join(request.req.compiler_versions.compiler_version())
            .join("solc");
        if !solc_path.exists() {
            return Err(ContractVerifierError::UnknownCompilerVersion(
                "solc".to_string(),
                request.req.compiler_versions.compiler_version(),
            ));
        }

        let zksolc = ZkSolc::new(zksolc_path, solc_path);

        let output = time::timeout(config.compilation_timeout(), zksolc.async_compile(input))
            .await
            .map_err(|_| ContractVerifierError::CompilationTimeout)??;

        match output {
            ZkSolcOutput::StandardJson(output) => {
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
                    tracing::error!(
                        "zksolc returned unexpected value for ABI: {}",
                        serde_json::to_string_pretty(&abi).unwrap()
                    );
                    return Err(ContractVerifierError::InternalError);
                }

                Ok(CompilationArtifacts { bytecode, abi })
            }
            ZkSolcOutput::YulSingleFile(output) => {
                let re = Regex::new(r"Contract `.*` bytecode: 0x([\da-f]+)").unwrap();
                let cap = re.captures(&output).unwrap();
                let bytecode_str = cap.get(1).unwrap().as_str();
                let bytecode = hex::decode(bytecode_str).unwrap();
                Ok(CompilationArtifacts {
                    bytecode,
                    abi: serde_json::Value::Array(Vec::new()),
                })
            }
        }
    }

    async fn compile_zkvyper(
        request: VerificationRequest,
        config: ContractVerifierConfig,
    ) -> Result<CompilationArtifacts, ContractVerifierError> {
        // Users may provide either just contract name or
        // source file name and contract name joined with ":".
        let contract_name =
            if let Some((_file_name, contract_name)) = request.req.contract_name.rsplit_once(':') {
                contract_name.to_string()
            } else {
                request.req.contract_name.clone()
            };
        let input = Self::build_zkvyper_input(request.clone())?;

        let zksync_home = env::var("ZKSYNC_HOME").unwrap_or_else(|_| ".".into());
        let zkvyper_path = Path::new(&zksync_home)
            .join("etc")
            .join("zkvyper-bin")
            .join(request.req.compiler_versions.zk_compiler_version())
            .join("zkvyper");
        if !zkvyper_path.exists() {
            return Err(ContractVerifierError::UnknownCompilerVersion(
                "zkvyper".to_string(),
                request.req.compiler_versions.zk_compiler_version(),
            ));
        }

        let vyper_path = Path::new(&zksync_home)
            .join("etc")
            .join("vyper-bin")
            .join(request.req.compiler_versions.compiler_version())
            .join("vyper");
        if !vyper_path.exists() {
            return Err(ContractVerifierError::UnknownCompilerVersion(
                "vyper".to_string(),
                request.req.compiler_versions.compiler_version(),
            ));
        }

        let zkvyper = ZkVyper::new(zkvyper_path, vyper_path);

        let output = time::timeout(config.compilation_timeout(), zkvyper.async_compile(input))
            .await
            .map_err(|_| ContractVerifierError::CompilationTimeout)??;

        let file_name = format!("{contract_name}.vy");
        let object = output
            .as_object()
            .cloned()
            .ok_or(ContractVerifierError::InternalError)?;
        for (path, artifact) in object {
            let path = Path::new(&path);
            if path.file_name().unwrap().to_str().unwrap() == file_name {
                let bytecode_str = artifact["bytecode"]
                    .as_str()
                    .ok_or(ContractVerifierError::InternalError)?;
                let bytecode = hex::decode(bytecode_str).unwrap();
                return Ok(CompilationArtifacts {
                    abi: artifact["abi"].clone(),
                    bytecode,
                });
            }
        }

        Err(ContractVerifierError::MissingContract(contract_name))
    }

    async fn compile(
        request: VerificationRequest,
        config: ContractVerifierConfig,
    ) -> Result<CompilationArtifacts, ContractVerifierError> {
        match request.req.source_code_data.compiler_type() {
            CompilerType::Solc => Self::compile_zksolc(request, config).await,
            CompilerType::Vyper => Self::compile_zkvyper(request, config).await,
        }
    }

    fn build_zksolc_input(
        request: VerificationRequest,
        file_name: String,
    ) -> Result<ZkSolcInput, ContractVerifierError> {
        let default_output_selection = serde_json::json!(
            {
                "*": {
                    "*": [ "abi" ],
                     "": [ "abi" ]
                }
            }
        );

        match request.req.source_code_data {
            SourceCodeData::SolSingleFile(source_code) => {
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
                    is_system: request.req.is_system,
                    metadata: None,
                };

                Ok(ZkSolcInput::StandardJson(StandardJson {
                    language: "Solidity".to_string(),
                    sources,
                    settings,
                }))
            }
            SourceCodeData::StandardJsonInput(map) => {
                let mut compiler_input: StandardJson =
                    serde_json::from_value(serde_json::Value::Object(map))
                        .map_err(|_| ContractVerifierError::FailedToDeserializeInput)?;
                // Set default output selection even if it is different in request.
                compiler_input.settings.output_selection = Some(default_output_selection);
                Ok(ZkSolcInput::StandardJson(compiler_input))
            }
            SourceCodeData::YulSingleFile(source_code) => {
                Ok(ZkSolcInput::YulSingleFile(source_code))
            }
            _ => panic!("Unexpected SourceCode variant"),
        }
    }

    fn build_zkvyper_input(
        request: VerificationRequest,
    ) -> Result<ZkVyperInput, ContractVerifierError> {
        let sources = match request.req.source_code_data {
            SourceCodeData::VyperMultiFile(s) => s,
            _ => panic!("Unexpected SourceCode variant"),
        };
        Ok(ZkVyperInput {
            sources,
            optimizer_mode: request.req.optimizer_mode,
        })
    }

    fn decode_constructor_arguments_from_calldata(
        calldata: DeployContractCalldata,
        contract_address_to_verify: Address,
    ) -> ConstructorArgs {
        match calldata {
            DeployContractCalldata::Deploy(calldata) => {
                let create = DEPLOYER_CONTRACT.function("create").unwrap();
                let create2 = DEPLOYER_CONTRACT.function("create2").unwrap();

                let create_acc = DEPLOYER_CONTRACT.function("createAccount").unwrap();
                let create2_acc = DEPLOYER_CONTRACT.function("create2Account").unwrap();

                let force_deploy = DEPLOYER_CONTRACT
                    .function("forceDeployOnAddresses")
                    .unwrap();
                // It's assumed that `create` and `create2` methods have the same parameters
                // and the same for `createAccount` and `create2Account`.
                match &calldata[0..4] {
                    selector
                        if selector == create.short_signature()
                            || selector == create2.short_signature() =>
                    {
                        let tokens = create
                            .decode_input(&calldata[4..])
                            .expect("Failed to decode input");
                        // Constructor arguments are in the third parameter.
                        ConstructorArgs::Check(tokens[2].clone().into_bytes().expect(
                            "The third parameter of `create/create2` should be of type `bytes`",
                        ))
                    }
                    selector
                        if selector == create_acc.short_signature()
                            || selector == create2_acc.short_signature() =>
                    {
                        let tokens = create
                            .decode_input(&calldata[4..])
                            .expect("Failed to decode input");
                        // Constructor arguments are in the third parameter.
                        ConstructorArgs::Check(
                            tokens[2].clone().into_bytes().expect(
                                "The third parameter of `createAccount/create2Account` should be of type `bytes`",
                            ),
                        )
                    }
                    selector if selector == force_deploy.short_signature() => {
                        let tokens = force_deploy
                            .decode_input(&calldata[4..])
                            .expect("Failed to decode input");
                        let deployments = tokens[0].clone().into_array().unwrap();
                        for deployment in deployments {
                            match deployment {
                                Token::Tuple(tokens) => {
                                    let address = tokens[1].clone().into_address().unwrap();
                                    if address == contract_address_to_verify {
                                        let call_constructor =
                                            tokens[2].clone().into_bool().unwrap();
                                        return if call_constructor {
                                            let input = tokens[4].clone().into_bytes().unwrap();
                                            ConstructorArgs::Check(input)
                                        } else {
                                            ConstructorArgs::Ignore
                                        };
                                    }
                                }
                                _ => panic!("Expected `deployment` to be a tuple"),
                            }
                        }
                        panic!("Couldn't find force deployment for given address");
                    }
                    _ => ConstructorArgs::Ignore,
                }
            }
            DeployContractCalldata::Ignore => ConstructorArgs::Ignore,
        }
    }

    async fn process_result(
        storage: &mut StorageProcessor<'_>,
        request_id: usize,
        verification_result: Result<VerificationInfo, ContractVerifierError>,
    ) {
        match verification_result {
            Ok(info) => {
                storage
                    .contract_verification_dal()
                    .save_verification_info(info)
                    .await
                    .unwrap();
                tracing::info!("Successfully processed request with id = {}", request_id);
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
                    .contract_verification_dal()
                    .save_verification_error(request_id, error_message, compilation_errors, None)
                    .await
                    .unwrap();
                tracing::info!("Request with id = {} was failed", request_id);
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
    const BACKOFF_MULTIPLIER: u64 = 1;

    async fn get_next_job(&self) -> anyhow::Result<Option<(Self::JobId, Self::Job)>> {
        let mut connection = self.connection_pool.access_storage().await.unwrap();

        // Time overhead for all operations except for compilation.
        const TIME_OVERHEAD: Duration = Duration::from_secs(10);

        // Considering that jobs that reach compilation timeout will be executed in
        // `compilation_timeout` + `non_compilation_time_overhead` (which is significantly less than `compilation_timeout`),
        // we re-pick up jobs that are being executed for a bit more than `compilation_timeout`.
        let job = connection
            .contract_verification_dal()
            .get_next_queued_verification_request(self.config.compilation_timeout() + TIME_OVERHEAD)
            .await
            .context("get_next_queued_verification_request()")?;

        Ok(job.map(|job| (job.id, job)))
    }

    async fn save_failure(&self, job_id: usize, _started_at: Instant, error: String) {
        let mut connection = self.connection_pool.access_storage().await.unwrap();

        connection
            .contract_verification_dal()
            .save_verification_error(
                job_id,
                "Internal error".to_string(),
                serde_json::Value::Array(Vec::new()),
                Some(error),
            )
            .await
            .unwrap();
    }

    #[allow(clippy::async_yields_async)]
    async fn process_job(
        &self,
        job: VerificationRequest,
        started_at: Instant,
    ) -> tokio::task::JoinHandle<anyhow::Result<()>> {
        let connection_pool = self.connection_pool.clone();
        tokio::task::spawn(async move {
            tracing::info!("Started to process request with id = {}", job.id);

            let config: ContractVerifierConfig =
                ContractVerifierConfig::from_env().context("ContractVerifierConfig")?;
            let mut connection = connection_pool.access_storage().await.unwrap();

            let job_id = job.id;
            let verification_result = Self::verify(&mut connection, job, config).await;
            Self::process_result(&mut connection, job_id, verification_result).await;

            metrics::histogram!(
                "api.contract_verifier.request_processing_time",
                started_at.elapsed()
            );
            Ok(())
        })
    }

    async fn save_result(
        &self,
        _: Self::JobId,
        _: Instant,
        _: Self::JobArtifacts,
    ) -> anyhow::Result<()> {
        // Do nothing
        Ok(())
    }

    fn max_attempts(&self) -> u32 {
        u32::MAX
    }

    async fn get_job_attempts(&self, _job_id: &Self::JobId) -> anyhow::Result<u32> {
        Ok(1)
    }
}
