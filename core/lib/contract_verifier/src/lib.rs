//! Contract verifier able to verify contracts created with `zksolc` or `zkvyper` toolchains.

use std::{
    fmt,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Context as _;
use chrono::Utc;
use ethabi::{Contract, Token};
use resolver::{GitHubCompilerResolver, ResolverMultiplexer};
use tokio::time;
use zksync_dal::{contract_verification_dal::DeployedContractData, ConnectionPool, Core, CoreDal};
use zksync_queued_job_processor::{async_trait, JobProcessor};
use zksync_types::{
    bytecode::{trim_padded_evm_bytecode, BytecodeHash, BytecodeMarker},
    contract_verification::{
        api::{
            self as api, CompilationArtifacts, VerificationIncomingRequest, VerificationInfo,
            VerificationProblem, VerificationRequest,
        },
        contract_identifier::{ContractIdentifier, DetectedMetadata, Match},
    },
    Address, CONTRACT_DEPLOYER_ADDRESS,
};

use crate::{
    compilers::{Solc, VyperInput, ZkSolc},
    error::ContractVerifierError,
    metrics::API_CONTRACT_VERIFIER_METRICS,
    resolver::{CompilerResolver, EnvCompilerResolver},
};

mod compilers;
pub mod error;
pub mod etherscan;
mod metrics;
mod resolver;
#[cfg(test)]
mod tests;

#[derive(Debug)]
struct ZkCompilerVersions {
    /// Version of the base / non-ZK compiler.
    pub base: String,
    /// Version of the ZK compiler.
    pub zk: String,
}

/// Internal counterpart of `ContractVersions` from API that encompasses all supported compilation modes.
#[derive(Debug)]
enum VersionedCompiler {
    Solc(String),
    Vyper(String),
    ZkSolc(ZkCompilerVersions),
    ZkVyper(ZkCompilerVersions),
}

impl From<api::CompilerVersions> for VersionedCompiler {
    fn from(versions: api::CompilerVersions) -> Self {
        match versions {
            api::CompilerVersions::Solc {
                compiler_solc_version,
                compiler_zksolc_version: None,
            } => Self::Solc(compiler_solc_version),

            api::CompilerVersions::Solc {
                compiler_solc_version,
                compiler_zksolc_version: Some(zk),
            } => Self::ZkSolc(ZkCompilerVersions {
                base: compiler_solc_version,
                zk,
            }),

            api::CompilerVersions::Vyper {
                compiler_vyper_version,
                compiler_zkvyper_version: None,
            } => Self::Vyper(compiler_vyper_version),

            api::CompilerVersions::Vyper {
                compiler_vyper_version,
                compiler_zkvyper_version: Some(zk),
            } => Self::ZkVyper(ZkCompilerVersions {
                base: compiler_vyper_version,
                zk,
            }),
        }
    }
}

impl VersionedCompiler {
    fn expected_bytecode_kind(&self) -> BytecodeMarker {
        match self {
            Self::Solc(_) | Self::Vyper(_) => BytecodeMarker::Evm,
            Self::ZkSolc(_) | Self::ZkVyper(_) => BytecodeMarker::EraVm,
        }
    }
}

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
    etherscan_verifier_enabled: bool,
}

impl ContractVerifier {
    /// Creates a new verifier instance.
    pub async fn new(
        compilation_timeout: Duration,
        connection_pool: ConnectionPool<Core>,
        etherscan_verifier_enabled: bool,
    ) -> anyhow::Result<Self> {
        let env_resolver = Arc::<EnvCompilerResolver>::default();
        let gh_resolver = Arc::new(GitHubCompilerResolver::new().await?);
        let mut resolver = ResolverMultiplexer::new(env_resolver);

        // Killer switch: if anything goes wrong with GH resolver, we can disable it without having to rollback.
        // TODO: Remove once GH resolver is proven to be stable.
        let disable_gh_resolver = std::env::var("DISABLE_GITHUB_RESOLVER").is_ok();
        if !disable_gh_resolver {
            resolver = resolver.with_resolver(gh_resolver);
        } else {
            tracing::warn!("GitHub resolver was disabled via DISABLE_GITHUB_RESOLVER env variable")
        }

        Self::with_resolver(
            compilation_timeout,
            connection_pool,
            Arc::new(resolver),
            etherscan_verifier_enabled,
        )
        .await
    }

    async fn with_resolver(
        compilation_timeout: Duration,
        connection_pool: ConnectionPool<Core>,
        compiler_resolver: Arc<dyn CompilerResolver>,
        etherscan_verifier_enabled: bool,
    ) -> anyhow::Result<Self> {
        Self::sync_compiler_versions(compiler_resolver.as_ref(), &connection_pool).await?;
        Ok(Self {
            compilation_timeout,
            contract_deployer: zksync_contracts::deployer_contract(),
            connection_pool,
            compiler_resolver,
            etherscan_verifier_enabled,
        })
    }

    /// Returns a future that would periodically update the supported compiler versions
    /// in the database.
    pub fn sync_compiler_versions_task(
        &self,
    ) -> impl std::future::Future<Output = anyhow::Result<()>> {
        const UPDATE_INTERVAL: Duration = Duration::from_secs(60 * 60); // 1 hour.

        let resolver = self.compiler_resolver.clone();
        let pool = self.connection_pool.clone();
        async move {
            loop {
                tracing::info!("Updating compiler versions");
                if let Err(err) = Self::sync_compiler_versions(resolver.as_ref(), &pool).await {
                    tracing::error!("Failed to sync compiler versions: {:?}", err);
                }
                tokio::time::sleep(UPDATE_INTERVAL).await;
            }
        }
    }

    /// Synchronizes compiler versions.
    #[tracing::instrument(level = "debug", skip_all)]
    async fn sync_compiler_versions(
        resolver: &dyn CompilerResolver,
        pool: &ConnectionPool<Core>,
    ) -> anyhow::Result<()> {
        let supported_versions = resolver
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

        let mut storage = pool.connection_tagged("contract_verifier").await?;
        let mut transaction = storage.start_transaction().await?;
        transaction
            .contract_verification_dal()
            .set_zksolc_versions(&supported_versions.zksolc.into_iter().collect::<Vec<_>>())
            .await?;
        transaction
            .contract_verification_dal()
            .set_solc_versions(&supported_versions.solc.into_iter().collect::<Vec<_>>())
            .await?;
        transaction
            .contract_verification_dal()
            .set_zkvyper_versions(&supported_versions.zkvyper.into_iter().collect::<Vec<_>>())
            .await?;
        transaction
            .contract_verification_dal()
            .set_vyper_versions(&supported_versions.vyper.into_iter().collect::<Vec<_>>())
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
    ) -> Result<(VerificationInfo, ContractIdentifier), ContractVerifierError> {
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
        let deployed_bytecode = match bytecode_marker {
            BytecodeMarker::EraVm => deployed_contract.bytecode.as_slice(),
            BytecodeMarker::Evm => trim_padded_evm_bytecode(
                BytecodeHash::try_from(deployed_contract.bytecode_hash)
                    .context("Invalid bytecode hash")?,
                &deployed_contract.bytecode,
            )
            .context("invalid stored EVM bytecode")?,
        };
        let mut deployed_code = deployed_bytecode.to_vec();
        let deployed_identifier =
            ContractIdentifier::from_bytecode(bytecode_marker, &deployed_code);

        let artifacts = self
            .get_compilation_artifacts(
                &mut request,
                deployed_identifier.detected_metadata.as_ref(),
                &bytecode_marker,
                &deployed_code,
            )
            .await?;

        let mut compiled_code = artifacts.deployed_bytecode().to_vec();

        // If contract contains immutable references (e.g. places to be filled during constructor execution),
        // rewrite them with zeroes, as we can't know the values just yet.
        // We're checking the constructor arguments as well, so assuming tha constructor arguments
        // are the same, the immutable values should also be the same.
        artifacts.patch_immutable_bytecodes(&mut compiled_code, &mut deployed_code);

        let compiled_identifier =
            ContractIdentifier::from_bytecode(bytecode_marker, &compiled_code);

        // regenerate the deployed identifier after patching the immutable bytecode
        let deployed_identifier =
            ContractIdentifier::from_bytecode(bytecode_marker, &deployed_code);

        let constructor_args = match bytecode_marker {
            BytecodeMarker::EraVm => self
                .decode_era_vm_constructor_args(&deployed_contract, request.req.contract_address)?,
            BytecodeMarker::Evm => Self::decode_evm_constructor_args(
                request.id,
                &deployed_contract,
                &artifacts.bytecode,
                &compiled_identifier,
                &deployed_identifier,
            )?,
        };

        let mut verification_problems = Vec::new();

        match compiled_identifier.matches(&deployed_identifier) {
            Match::Full => {}
            Match::Partial => {
                tracing::info!(
                    request_id = request.id,
                    deployed = hex::encode(deployed_bytecode),
                    compiled = hex::encode(artifacts.deployed_bytecode()),
                    "Partial bytecode match",
                );
                verification_problems.push(VerificationProblem::IncorrectMetadata);
            }
            Match::None => {
                tracing::info!(
                    request_id = request.id,
                    deployed = hex::encode(deployed_bytecode),
                    compiled = hex::encode(artifacts.deployed_bytecode()),
                    "Deployed (runtime) bytecode mismatch",
                );
                return Err(ContractVerifierError::BytecodeMismatch);
            }
        }

        match constructor_args {
            ConstructorArgs::Check(args) => {
                let provided_constructor_args = &request.req.constructor_arguments.0;
                if *provided_constructor_args != args {
                    tracing::info!(
                        "Constructor args mismatch, deployed: 0x{}, provided in request: 0x{}",
                        hex::encode(&args),
                        hex::encode(provided_constructor_args)
                    );
                    // We could, in theory, accept this contract and mark it as partially verified,
                    // but in during verification it is always possible to reconstruct the
                    // constructor arguments, so there is no reason for that.
                    // Mismatching constructor arguments are only needed for "similar bytecodes"
                    // (e.g. displayed contract as verified without a direct verification request).
                    return Err(ContractVerifierError::IncorrectConstructorArguments);
                }
            }
            ConstructorArgs::Ignore => {
                request.req.constructor_arguments = Vec::new().into();
            }
        }

        let verified_at = Utc::now();
        tracing::trace!(%verified_at, "verified request");
        let info = VerificationInfo {
            request,
            artifacts,
            verified_at,
            verification_problems,
        };
        Ok((info, compiled_identifier))
    }

    /// Returns compilation artifacts for the given request.
    /// Tries to use the compiler versions from the metadata if they are different from the ones in the request.
    /// If the metadata is not present, or the versions are the same, uses the ones from the request.
    async fn get_compilation_artifacts(
        &self,
        request: &mut VerificationRequest,
        detected_metadata: Option<&DetectedMetadata>,
        bytecode_marker: &BytecodeMarker,
        deployed_code: &[u8],
    ) -> Result<CompilationArtifacts, ContractVerifierError> {
        match detected_metadata {
            Some(DetectedMetadata::Cbor {
                metadata: cbor_metadata,
                ..
            }) if !request.req.compiler_versions_match(cbor_metadata) => {
                let updated_request = request
                    .req
                    .clone()
                    .with_updated_compiler_versions(cbor_metadata);

                match self
                    .compile(updated_request.clone(), *bytecode_marker)
                    .await
                {
                    Ok(artifacts) if artifacts.match_bytecode(bytecode_marker, deployed_code) => {
                        tracing::warn!(
                            request_id = request.id,
                            request_compiler = request.req.compiler_versions.compiler_version(),
                            metadata_compiler =
                                updated_request.compiler_versions.compiler_version(),
                            request_zk_compiler =
                                request.req.compiler_versions.zk_compiler_version(),
                            metadata_zk_compiler =
                                updated_request.compiler_versions.zk_compiler_version(),
                            "Updating request compiler versions in the DB."
                        );

                        // Since we know the version that should be used for the contract, we override one provided in
                        // the request, so that if this request is interpreted by anyone else, they can get correct
                        // compilation results
                        self.update_request_compiler_versions(
                            request.id,
                            updated_request.compiler_versions.compiler_version(),
                            updated_request.compiler_versions.zk_compiler_version(),
                        )
                        .await?;

                        // Update the request for downstream use
                        request.req = updated_request;

                        Ok(artifacts)
                    }
                    _ => {
                        tracing::warn!(
                            request_id = request.id,
                            "Failed to compile with the compiler versions from the metadata or the compiled bytecode doesn't match. Falling back to the original request."
                        );
                        // Fallback to the original request
                        self.compile(request.req.clone(), *bytecode_marker).await
                    }
                }
            }
            _ => {
                // No CBOR metadata — use original request
                self.compile(request.req.clone(), *bytecode_marker).await
            }
        }
    }

    // Updates request compiler versions in the DB.
    async fn update_request_compiler_versions(
        &self,
        request_id: usize,
        compiler_version: &str,
        zk_compiler_version: Option<&str>,
    ) -> Result<(), ContractVerifierError> {
        let mut storage = self
            .connection_pool
            .connection_tagged("contract_verifier")
            .await?;

        storage
            .contract_verification_dal()
            .update_verification_request_compiler_versions(
                request_id,
                compiler_version,
                zk_compiler_version,
            )
            .await?;

        Ok(())
    }

    async fn compile_zksolc(
        &self,
        version: &ZkCompilerVersions,
        req: VerificationIncomingRequest,
    ) -> Result<CompilationArtifacts, ContractVerifierError> {
        let zksolc = self.compiler_resolver.resolve_zksolc(version).await?;
        tracing::debug!(?zksolc, ?version, "resolved compiler");
        let input = ZkSolc::build_input(req)?;

        time::timeout(self.compilation_timeout, zksolc.compile(input))
            .await
            .map_err(|_| ContractVerifierError::CompilationTimeout)?
    }

    async fn compile_zkvyper(
        &self,
        version: &ZkCompilerVersions,
        req: VerificationIncomingRequest,
    ) -> Result<CompilationArtifacts, ContractVerifierError> {
        let zkvyper = self.compiler_resolver.resolve_zkvyper(version).await?;
        tracing::debug!(?zkvyper, ?version, "resolved compiler");
        let input = VyperInput::new(req)?;
        time::timeout(self.compilation_timeout, zkvyper.compile(input))
            .await
            .map_err(|_| ContractVerifierError::CompilationTimeout)?
    }

    async fn compile_solc(
        &self,
        version: &str,
        req: VerificationIncomingRequest,
    ) -> Result<CompilationArtifacts, ContractVerifierError> {
        let solc = self.compiler_resolver.resolve_solc(version).await?;
        tracing::debug!(?solc, ?req.compiler_versions, "resolved compiler");
        let input = Solc::build_input(req)?;

        time::timeout(self.compilation_timeout, solc.compile(input))
            .await
            .map_err(|_| ContractVerifierError::CompilationTimeout)?
    }

    async fn compile_vyper(
        &self,
        version: &str,
        req: VerificationIncomingRequest,
    ) -> Result<CompilationArtifacts, ContractVerifierError> {
        let vyper = self.compiler_resolver.resolve_vyper(version).await?;
        tracing::debug!(?vyper, ?req.compiler_versions, "resolved compiler");
        let input = VyperInput::new(req)?;

        time::timeout(self.compilation_timeout, vyper.compile(input))
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

        let compiler = VersionedCompiler::from(req.compiler_versions.clone());
        if compiler.expected_bytecode_kind() != bytecode_marker {
            let err = anyhow::anyhow!(
                "bytecode kind expected by compiler {compiler:?} differs from the actual bytecode kind \
                 of the verified contract ({bytecode_marker:?})",
            );
            return Err(err.into());
        }

        match &compiler {
            VersionedCompiler::Solc(version) => self.compile_solc(version, req).await,
            VersionedCompiler::Vyper(version) => self.compile_vyper(version, req).await,
            VersionedCompiler::ZkSolc(version) => self.compile_zksolc(version, req).await,
            VersionedCompiler::ZkVyper(version) => self.compile_zkvyper(version, req).await,
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
                let tokens = create_acc
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
        compiled_identifier: &ContractIdentifier,
        deployed_identifier: &ContractIdentifier,
    ) -> Result<ConstructorArgs, ContractVerifierError> {
        fn extract_arguments<'a>(
            calldata: &'a [u8],
            creation_bytecode: &'a [u8],
            compiled_identifier: &ContractIdentifier,
            deployed_identifier: &ContractIdentifier,
        ) -> Result<&'a [u8], &'static str> {
            if creation_bytecode.len() < compiled_identifier.metadata_length() {
                // This shouldn't normally happen, since we calculated contract identifier based on this code.
                return Err("Creation bytecode doesn't fit metadata");
            }
            let creation_bytecode_without_metadata = &creation_bytecode
                [..creation_bytecode.len() - compiled_identifier.metadata_length()];

            // Ensure equivalence of the creation bytecode (which can be different from the deployed bytecode).
            // Note that metadata hash may still be different; this is checked by other part of the code.
            let constructor_args_with_metadata = calldata
                .strip_prefix(creation_bytecode_without_metadata)
                .ok_or("Creation bytecode is different")?;

            // Skip metadata to get to the constructor arguments.
            // Note that deployed contract may have different metadata, so we use another
            // identifier here.
            if constructor_args_with_metadata.len() < deployed_identifier.metadata_length() {
                return Err("Calldata doesn't fit metadata");
            }
            Ok(&constructor_args_with_metadata[deployed_identifier.metadata_length()..])
        }

        let Some(calldata) = &contract.calldata else {
            return Ok(ConstructorArgs::Ignore);
        };
        if contract.contract_address.is_some() {
            // Not an EVM deployment transaction
            return Ok(ConstructorArgs::Ignore);
        }

        match extract_arguments(
            calldata,
            creation_bytecode,
            compiled_identifier,
            deployed_identifier,
        ) {
            Ok(args) => Ok(ConstructorArgs::Check(args.to_vec())),
            Err(err) => {
                tracing::info!(
                    request_id,
                    calldata = hex::encode(calldata),
                    compiled = hex::encode(creation_bytecode),
                    "Creation bytecode mismatch: {err}"
                );
                Err(ContractVerifierError::CreationBytecodeMismatch)
            }
        }
    }

    #[tracing::instrument(level = "debug", skip_all, err, fields(id = request_id))]
    async fn process_result(
        &self,
        request_id: usize,
        verification_result: Result<(VerificationInfo, ContractIdentifier), ContractVerifierError>,
    ) -> anyhow::Result<()> {
        let mut storage = self
            .connection_pool
            .connection_tagged("contract_verifier")
            .await?;
        match verification_result {
            Ok((info, identifier)) => {
                let mut transaction = storage.start_transaction().await?;
                if self.etherscan_verifier_enabled
                    && etherscan::is_supported_verification_request(&info.request)
                {
                    tracing::debug!(
                        "Created etherscan verification request with id = {request_id}"
                    );
                    transaction
                        .etherscan_verification_dal()
                        .add_verification_request(request_id)
                        .await?;
                }

                transaction
                    .contract_verification_dal()
                    .save_verification_info(
                        info,
                        identifier.bytecode_keccak256,
                        identifier.bytecode_without_metadata_keccak256,
                    )
                    .await?;

                transaction.commit().await?;
                tracing::info!("Successfully processed request with id = {request_id}");

                API_CONTRACT_VERIFIER_METRICS.successful_verifications[&Self::SERVICE_NAME].inc();
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

                API_CONTRACT_VERIFIER_METRICS.failed_verifications[&Self::SERVICE_NAME].inc();
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
