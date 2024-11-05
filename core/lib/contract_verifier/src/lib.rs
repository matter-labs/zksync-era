//! Contract verifier able to verify contracts created with `zksolc` or `zkvyper` toolchains.

use std::{
    collections::HashMap,
    fmt,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Context as _;
use chrono::Utc;
use ethabi::{Contract, Token};
use tokio::time;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_queued_job_processor::{async_trait, JobProcessor};
use zksync_types::{
    contract_verification_api::{
        CompilationArtifacts, CompilerType, DeployContractCalldata, SourceCodeData,
        VerificationIncomingRequest, VerificationInfo, VerificationRequest,
    },
    Address,
};

use crate::{
    error::ContractVerifierError,
    metrics::API_CONTRACT_VERIFIER_METRICS,
    resolver::{CompilerResolver, EnvCompilerResolver},
    zksolc_utils::{Optimizer, Settings, Source, StandardJson, ZkSolcInput},
    zkvyper_utils::ZkVyperInput,
};

pub mod error;
mod metrics;
mod resolver;
#[cfg(test)]
mod tests;
mod zksolc_utils;
mod zkvyper_utils;

enum ConstructorArgs {
    Check(Vec<u8>),
    Ignore,
}

impl fmt::Debug for ConstructorArgs {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Check(args) => write!(formatter, "0x{}", hex::encode(args)),
            Self::Ignore => formatter.write_str("(ignored)"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ContractVerifier {
    compilation_timeout: Duration,
    contract_deployer: Contract,
    connection_pool: ConnectionPool<Core>,
    compiler_resolver: Arc<dyn CompilerResolver>,
}

impl ContractVerifier {
    /// Creates a new verifier instance.
    pub async fn new(
        compilation_timeout: Duration,
        connection_pool: ConnectionPool<Core>,
    ) -> anyhow::Result<Self> {
        Self::with_resolver(
            compilation_timeout,
            connection_pool,
            Arc::<EnvCompilerResolver>::default(),
        )
        .await
    }

    async fn with_resolver(
        compilation_timeout: Duration,
        connection_pool: ConnectionPool<Core>,
        compiler_resolver: Arc<dyn CompilerResolver>,
    ) -> anyhow::Result<Self> {
        let this = Self {
            compilation_timeout,
            contract_deployer: zksync_contracts::deployer_contract(),
            connection_pool,
            compiler_resolver,
        };
        this.sync_compiler_versions().await?;
        Ok(this)
    }

    /// Synchronizes compiler versions.
    #[tracing::instrument(level = "debug", skip_all)]
    async fn sync_compiler_versions(&self) -> anyhow::Result<()> {
        let supported_versions = self
            .compiler_resolver
            .supported_versions()
            .await
            .context("cannot get supported compilers")?;
        if supported_versions.lacks_any_compiler() {
            tracing::warn!(
                ?supported_versions,
                "contract verifier lacks support of at least one compiler entirely; it may be incorrectly set up"
            );
        }
        tracing::info!(
            ?supported_versions,
            "persisting supported compiler versions"
        );

        let mut storage = self
            .connection_pool
            .connection_tagged("contract_verifier")
            .await?;
        let mut transaction = storage.start_transaction().await?;
        transaction
            .contract_verification_dal()
            .set_zksolc_versions(supported_versions.zksolc)
            .await?;
        transaction
            .contract_verification_dal()
            .set_solc_versions(supported_versions.solc)
            .await?;
        transaction
            .contract_verification_dal()
            .set_zkvyper_versions(supported_versions.zkvyper)
            .await?;
        transaction
            .contract_verification_dal()
            .set_vyper_versions(supported_versions.vyper)
            .await?;
        transaction.commit().await?;
        Ok(())
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        err,
        fields(id = request.id, addr = ?request.req.contract_address)
    )]
    async fn verify(
        &self,
        mut request: VerificationRequest,
    ) -> Result<VerificationInfo, ContractVerifierError> {
        let artifacts = self.compile(request.req.clone()).await?;

        // Bytecode should be present because it is checked when accepting request.
        let mut storage = self
            .connection_pool
            .connection_tagged("contract_verifier")
            .await?;
        let (deployed_bytecode, creation_tx_calldata) = storage
            .contract_verification_dal()
            .get_contract_info_for_verification(request.req.contract_address)
            .await?
            .with_context(|| {
                format!(
                    "Contract is missing in DB for already accepted verification request. Contract address: {:#?}",
                    request.req.contract_address
                )
            })?;
        drop(storage);

        let constructor_args =
            self.decode_constructor_args(creation_tx_calldata, request.req.contract_address)?;

        if artifacts.bytecode != deployed_bytecode {
            tracing::info!(
                "Bytecode mismatch req {}, deployed: 0x{}, compiled: 0x{}",
                request.id,
                hex::encode(deployed_bytecode),
                hex::encode(artifacts.bytecode)
            );
            return Err(ContractVerifierError::BytecodeMismatch);
        }

        match constructor_args {
            ConstructorArgs::Check(args) => {
                let provided_constructor_args = &request.req.constructor_arguments.0;
                if *provided_constructor_args != args {
                    tracing::trace!(
                        "Constructor args mismatch, deployed: 0x{}, provided in request: 0x{}",
                        hex::encode(&args),
                        hex::encode(provided_constructor_args)
                    );
                    return Err(ContractVerifierError::IncorrectConstructorArguments);
                }
            }
            ConstructorArgs::Ignore => {
                request.req.constructor_arguments = Vec::new().into();
            }
        }

        let verified_at = Utc::now();
        tracing::trace!(%verified_at, "verified request");
        Ok(VerificationInfo {
            request,
            artifacts,
            verified_at,
        })
    }

    async fn compile_zksolc(
        &self,
        req: VerificationIncomingRequest,
    ) -> Result<CompilationArtifacts, ContractVerifierError> {
        let zksolc = self
            .compiler_resolver
            .resolve_solc(&req.compiler_versions)
            .await?;
        tracing::debug!(?zksolc, ?req.compiler_versions, "resolved compiler");
        let input = Self::build_zksolc_input(req)?;

        time::timeout(self.compilation_timeout, zksolc.compile(input))
            .await
            .map_err(|_| ContractVerifierError::CompilationTimeout)?
    }

    async fn compile_zkvyper(
        &self,
        req: VerificationIncomingRequest,
    ) -> Result<CompilationArtifacts, ContractVerifierError> {
        let zkvyper = self
            .compiler_resolver
            .resolve_vyper(&req.compiler_versions)
            .await?;
        tracing::debug!(?zkvyper, ?req.compiler_versions, "resolved compiler");
        let input = Self::build_zkvyper_input(req)?;
        time::timeout(self.compilation_timeout, zkvyper.compile(input))
            .await
            .map_err(|_| ContractVerifierError::CompilationTimeout)?
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn compile(
        &self,
        req: VerificationIncomingRequest,
    ) -> Result<CompilationArtifacts, ContractVerifierError> {
        match req.source_code_data.compiler_type() {
            CompilerType::Solc => self.compile_zksolc(req).await,
            CompilerType::Vyper => self.compile_zkvyper(req).await,
        }
    }

    fn build_zksolc_input(
        req: VerificationIncomingRequest,
    ) -> Result<ZkSolcInput, ContractVerifierError> {
        // Users may provide either just contract name or
        // source file name and contract name joined with ":".
        let (file_name, contract_name) =
            if let Some((file_name, contract_name)) = req.contract_name.rsplit_once(':') {
                (file_name.to_string(), contract_name.to_string())
            } else {
                (
                    format!("{}.sol", req.contract_name),
                    req.contract_name.clone(),
                )
            };
        let default_output_selection = serde_json::json!({
            "*": {
                "*": [ "abi" ],
                 "": [ "abi" ]
            }
        });

        match req.source_code_data {
            SourceCodeData::SolSingleFile(source_code) => {
                let source = Source {
                    content: source_code,
                };
                let sources = HashMap::from([(file_name.clone(), source)]);
                let optimizer = Optimizer {
                    enabled: req.optimization_used,
                    mode: req.optimizer_mode.and_then(|s| s.chars().next()),
                };
                let optimizer_value = serde_json::to_value(optimizer).unwrap();

                let settings = Settings {
                    output_selection: Some(default_output_selection),
                    is_system: req.is_system,
                    force_evmla: req.force_evmla,
                    other: serde_json::Value::Object(
                        vec![("optimizer".to_string(), optimizer_value)]
                            .into_iter()
                            .collect(),
                    ),
                };

                Ok(ZkSolcInput::StandardJson {
                    input: StandardJson {
                        language: "Solidity".to_string(),
                        sources,
                        settings,
                    },
                    contract_name,
                    file_name,
                })
            }
            SourceCodeData::StandardJsonInput(map) => {
                let mut compiler_input: StandardJson =
                    serde_json::from_value(serde_json::Value::Object(map))
                        .map_err(|_| ContractVerifierError::FailedToDeserializeInput)?;
                // Set default output selection even if it is different in request.
                compiler_input.settings.output_selection = Some(default_output_selection);
                Ok(ZkSolcInput::StandardJson {
                    input: compiler_input,
                    contract_name,
                    file_name,
                })
            }
            SourceCodeData::YulSingleFile(source_code) => Ok(ZkSolcInput::YulSingleFile {
                source_code,
                is_system: req.is_system,
            }),
            other => unreachable!("Unexpected `SourceCodeData` variant: {other:?}"),
        }
    }

    fn build_zkvyper_input(
        req: VerificationIncomingRequest,
    ) -> Result<ZkVyperInput, ContractVerifierError> {
        // Users may provide either just contract name or
        // source file name and contract name joined with ":".
        let contract_name = if let Some((_, contract_name)) = req.contract_name.rsplit_once(':') {
            contract_name.to_owned()
        } else {
            req.contract_name.clone()
        };

        let sources = match req.source_code_data {
            SourceCodeData::VyperMultiFile(s) => s,
            other => unreachable!("unexpected `SourceCodeData` variant: {other:?}"),
        };
        Ok(ZkVyperInput {
            contract_name,
            sources,
            optimizer_mode: req.optimizer_mode,
        })
    }

    /// All returned errors are internal.
    #[tracing::instrument(level = "trace", skip_all, ret, err)]
    fn decode_constructor_args(
        &self,
        calldata: DeployContractCalldata,
        contract_address_to_verify: Address,
    ) -> anyhow::Result<ConstructorArgs> {
        match calldata {
            DeployContractCalldata::Deploy(calldata) => {
                anyhow::ensure!(
                    calldata.len() >= 4,
                    "calldata doesn't include Solidity function selector"
                );

                let contract_deployer = &self.contract_deployer;
                let create = contract_deployer
                    .function("create")
                    .context("no `create` in contract deployer ABI")?;
                let create2 = contract_deployer
                    .function("create2")
                    .context("no `create2` in contract deployer ABI")?;
                let create_acc = contract_deployer
                    .function("createAccount")
                    .context("no `createAccount` in contract deployer ABI")?;
                let create2_acc = contract_deployer
                    .function("create2Account")
                    .context("no `create2Account` in contract deployer ABI")?;
                let force_deploy = contract_deployer
                    .function("forceDeployOnAddresses")
                    .context("no `forceDeployOnAddresses` in contract deployer ABI")?;

                let (selector, token_data) = calldata.split_at(4);
                // It's assumed that `create` and `create2` methods have the same parameters
                // and the same for `createAccount` and `create2Account`.
                Ok(match selector {
                    selector
                        if selector == create.short_signature()
                            || selector == create2.short_signature() =>
                    {
                        let tokens = create
                            .decode_input(token_data)
                            .context("failed to decode `create` / `create2` input")?;
                        // Constructor arguments are in the third parameter.
                        ConstructorArgs::Check(tokens[2].clone().into_bytes().context(
                            "third parameter of `create/create2` should be of type `bytes`",
                        )?)
                    }
                    selector
                        if selector == create_acc.short_signature()
                            || selector == create2_acc.short_signature() =>
                    {
                        let tokens = create
                            .decode_input(token_data)
                            .context("failed to decode `createAccount` / `create2Account` input")?;
                        // Constructor arguments are in the third parameter.
                        ConstructorArgs::Check(tokens[2].clone().into_bytes().context(
                            "third parameter of `createAccount/create2Account` should be of type `bytes`",
                        )?)
                    }
                    selector if selector == force_deploy.short_signature() => {
                        Self::decode_force_deployment(
                            token_data,
                            force_deploy,
                            contract_address_to_verify,
                        )
                        .context("failed decoding force deployment")?
                    }
                    _ => ConstructorArgs::Ignore,
                })
            }
            DeployContractCalldata::Ignore => Ok(ConstructorArgs::Ignore),
        }
    }

    fn decode_force_deployment(
        token_data: &[u8],
        force_deploy: &ethabi::Function,
        contract_address_to_verify: Address,
    ) -> anyhow::Result<ConstructorArgs> {
        let tokens = force_deploy
            .decode_input(token_data)
            .context("failed to decode `forceDeployOnAddresses` input")?;
        let deployments = tokens[0]
            .clone()
            .into_array()
            .context("first parameter of `forceDeployOnAddresses` is not an array")?;
        for deployment in deployments {
            match deployment {
                Token::Tuple(tokens) => {
                    let address = tokens[1]
                        .clone()
                        .into_address()
                        .context("unexpected `address`")?;
                    if address == contract_address_to_verify {
                        let call_constructor = tokens[2]
                            .clone()
                            .into_bool()
                            .context("unexpected `call_constructor`")?;
                        return Ok(if call_constructor {
                            let input = tokens[4]
                                .clone()
                                .into_bytes()
                                .context("unexpected constructor input")?;
                            ConstructorArgs::Check(input)
                        } else {
                            ConstructorArgs::Ignore
                        });
                    }
                }
                _ => anyhow::bail!("expected `deployment` to be a tuple"),
            }
        }
        anyhow::bail!("couldn't find force deployment for address {contract_address_to_verify:?}");
    }

    #[tracing::instrument(level = "debug", skip_all, err, fields(id = request_id))]
    async fn process_result(
        &self,
        request_id: usize,
        verification_result: Result<VerificationInfo, ContractVerifierError>,
    ) -> anyhow::Result<()> {
        let mut storage = self
            .connection_pool
            .connection_tagged("contract_verifier")
            .await?;
        match verification_result {
            Ok(info) => {
                storage
                    .contract_verification_dal()
                    .save_verification_info(info)
                    .await?;
                tracing::info!("Successfully processed request with id = {request_id}");
            }
            Err(error) => {
                let error_message = match &error {
                    ContractVerifierError::Internal(err) => {
                        // Do not expose the error externally, but log it.
                        tracing::warn!(request_id, "internal error processing request: {err}");
                        "internal error".to_owned()
                    }
                    _ => error.to_string(),
                };
                let compilation_errors = match error {
                    ContractVerifierError::CompilationError(compilation_errors) => {
                        compilation_errors
                    }
                    _ => serde_json::Value::Array(Vec::new()),
                };
                storage
                    .contract_verification_dal()
                    .save_verification_error(request_id, error_message, compilation_errors, None)
                    .await?;
                tracing::info!("Request with id = {request_id} was failed");
            }
        }
        Ok(())
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
        /// Time overhead for all operations except for compilation.
        const TIME_OVERHEAD: Duration = Duration::from_secs(10);

        let mut connection = self
            .connection_pool
            .connection_tagged("contract_verifier")
            .await?;
        // Considering that jobs that reach compilation timeout will be executed in
        // `compilation_timeout` + `non_compilation_time_overhead` (which is significantly less than `compilation_timeout`),
        // we re-pick up jobs that are being executed for a bit more than `compilation_timeout`.
        let job = connection
            .contract_verification_dal()
            .get_next_queued_verification_request(self.compilation_timeout + TIME_OVERHEAD)
            .await?;
        Ok(job.map(|job| (job.id, job)))
    }

    async fn save_failure(&self, job_id: usize, _started_at: Instant, error: String) {
        let mut connection = self
            .connection_pool
            .connection_tagged("contract_verifier")
            .await
            .unwrap();

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
        _job_id: &Self::JobId,
        job: VerificationRequest,
        started_at: Instant,
    ) -> tokio::task::JoinHandle<anyhow::Result<()>> {
        let this = self.clone();
        tokio::task::spawn(async move {
            tracing::info!("Started to process request with id = {}", job.id);

            let job_id = job.id;
            let verification_result = this.verify(job).await;
            this.process_result(job_id, verification_result).await?;

            API_CONTRACT_VERIFIER_METRICS
                .request_processing_time
                .observe(started_at.elapsed());
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
