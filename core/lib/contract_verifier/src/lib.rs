//! Contract verifier able to verify contracts created with `zksolc` or `zkvyper` toolchains.

use std::{
    fmt,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Context as _;
use chrono::Utc;
use ethabi::{Contract, Token};
use tokio::time;
use zksync_dal::{contract_verification_dal::DeployedContractData, ConnectionPool, Core, CoreDal};
use zksync_queued_job_processor::{async_trait, JobProcessor};
use zksync_types::{
    contract_verification_api::{
        CompilationArtifacts, CompilerType, VerificationIncomingRequest, VerificationInfo,
        VerificationRequest,
    },
    Address, CONTRACT_DEPLOYER_ADDRESS,
};
use zksync_utils::bytecode::{prepare_evm_bytecode, BytecodeMarker};

use crate::{
    compilers::{Solc, ZkSolc, ZkVyper},
    error::ContractVerifierError,
    metrics::API_CONTRACT_VERIFIER_METRICS,
    resolver::{CompilerResolver, EnvCompilerResolver},
};

mod compilers;
pub mod error;
mod metrics;
mod resolver;
#[cfg(test)]
mod tests;

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
        // Bytecode should be present because it is checked when accepting request.
        let mut storage = self
            .connection_pool
            .connection_tagged("contract_verifier")
            .await?;
        let deployed_contract = storage
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

        let bytecode_marker = BytecodeMarker::new(deployed_contract.bytecode_hash)
            .context("unknown bytecode kind")?;
        let artifacts = self.compile(request.req.clone(), bytecode_marker).await?;
        let constructor_args = match bytecode_marker {
            BytecodeMarker::EraVm => self
                .decode_era_vm_constructor_args(&deployed_contract, request.req.contract_address)?,
            BytecodeMarker::Evm => Self::decode_evm_constructor_args(
                request.id,
                &deployed_contract,
                &artifacts.bytecode,
            )?,
        };

        let deployed_bytecode = match bytecode_marker {
            BytecodeMarker::EraVm => deployed_contract.bytecode.as_slice(),
            BytecodeMarker::Evm => prepare_evm_bytecode(&deployed_contract.bytecode)
                .context("invalid stored EVM bytecode")?,
        };

        if artifacts.deployed_bytecode() != deployed_bytecode {
            tracing::info!(
                request_id = request.id,
                deployed = hex::encode(deployed_bytecode),
                compiled = hex::encode(artifacts.deployed_bytecode()),
                "Deployed (runtime) bytecode mismatch",
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
            .resolve_zksolc(&req.compiler_versions)
            .await?;
        tracing::debug!(?zksolc, ?req.compiler_versions, "resolved compiler");
        let input = ZkSolc::build_input(req)?;

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
            .resolve_zkvyper(&req.compiler_versions)
            .await?;
        tracing::debug!(?zkvyper, ?req.compiler_versions, "resolved compiler");
        let input = ZkVyper::build_input(req)?;
        time::timeout(self.compilation_timeout, zkvyper.compile(input))
            .await
            .map_err(|_| ContractVerifierError::CompilationTimeout)?
    }

    async fn compile_solc(
        &self,
        req: VerificationIncomingRequest,
    ) -> Result<CompilationArtifacts, ContractVerifierError> {
        let solc = self
            .compiler_resolver
            .resolve_solc(req.compiler_versions.compiler_version())
            .await?;
        tracing::debug!(?solc, ?req.compiler_versions, "resolved compiler");
        let input = Solc::build_input(req)?;

        time::timeout(self.compilation_timeout, solc.compile(input))
            .await
            .map_err(|_| ContractVerifierError::CompilationTimeout)?
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn compile(
        &self,
        req: VerificationIncomingRequest,
        bytecode_marker: BytecodeMarker,
    ) -> Result<CompilationArtifacts, ContractVerifierError> {
        let compiler_type = req.source_code_data.compiler_type();
        let compiler_type_by_versions = req.compiler_versions.compiler_type();
        if compiler_type != compiler_type_by_versions {
            // Should be checked when receiving a request, so here it's more of a sanity check
            let err = anyhow::anyhow!(
                "specified compiler versions {:?} belong to a differing toolchain than source code ({compiler_type:?})",
                req.compiler_versions
            );
            return Err(err.into());
        }

        match (bytecode_marker, compiler_type) {
            (BytecodeMarker::EraVm, CompilerType::Solc) => self.compile_zksolc(req).await,
            (BytecodeMarker::EraVm, CompilerType::Vyper) => self.compile_zkvyper(req).await,
            (BytecodeMarker::Evm, CompilerType::Solc) => self.compile_solc(req).await,
            (BytecodeMarker::Evm, CompilerType::Vyper) => {
                // TODO: add vyper support
                let err = anyhow::anyhow!("vyper toolchain is not yet supported for EVM contracts");
                return Err(err.into());
            }
        }
    }

    /// All returned errors are internal.
    #[tracing::instrument(level = "trace", skip_all, ret, err)]
    fn decode_era_vm_constructor_args(
        &self,
        contract: &DeployedContractData,
        contract_address_to_verify: Address,
    ) -> anyhow::Result<ConstructorArgs> {
        let Some(calldata) = &contract.calldata else {
            return Ok(ConstructorArgs::Ignore);
        };

        if contract.contract_address == Some(CONTRACT_DEPLOYER_ADDRESS) {
            self.decode_contract_deployer_call(calldata, contract_address_to_verify)
        } else {
            Ok(ConstructorArgs::Ignore)
        }
    }

    fn decode_contract_deployer_call(
        &self,
        calldata: &[u8],
        contract_address_to_verify: Address,
    ) -> anyhow::Result<ConstructorArgs> {
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
                ConstructorArgs::Check(
                    tokens[2]
                        .clone()
                        .into_bytes()
                        .context("third parameter of `create/create2` should be of type `bytes`")?,
                )
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
                Self::decode_force_deployment(token_data, force_deploy, contract_address_to_verify)
                    .context("failed decoding force deployment")?
            }
            _ => ConstructorArgs::Ignore,
        })
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

    fn decode_evm_constructor_args(
        request_id: usize,
        contract: &DeployedContractData,
        creation_bytecode: &[u8],
    ) -> Result<ConstructorArgs, ContractVerifierError> {
        let Some(calldata) = &contract.calldata else {
            return Ok(ConstructorArgs::Ignore);
        };
        if contract.contract_address.is_some() {
            // Not an EVM deployment transaction
            return Ok(ConstructorArgs::Ignore);
        }

        let args = calldata.strip_prefix(creation_bytecode).ok_or_else(|| {
            tracing::info!(
                request_id,
                calldata = hex::encode(calldata),
                compiled = hex::encode(creation_bytecode),
                "Creation bytecode mismatch"
            );
            ContractVerifierError::CreationBytecodeMismatch
        })?;
        Ok(ConstructorArgs::Check(args.to_vec()))
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
                    .save_verification_error(request_id, &error_message, &compilation_errors, None)
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
                "Internal error",
                &serde_json::Value::Array(Vec::new()),
                Some(&error),
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
