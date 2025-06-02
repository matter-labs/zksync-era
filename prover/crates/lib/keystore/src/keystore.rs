use std::{
    collections::HashMap,
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::Context as _;
#[cfg(any(feature = "gpu", feature = "gpu-light"))]
use circuit_definitions::circuit_definitions::aux_layer::{
    CompressionProofsTreeHasher, CompressionProofsTreeHasherForWrapper,
};
use circuit_definitions::{
    boojum::cs::implementations::setup::FinalizationHintsForProver,
    circuit_definitions::{
        aux_layer::{
            ZkSyncCompressionForWrapperVerificationKey, ZkSyncCompressionLayerVerificationKey,
            ZkSyncSnarkWrapperVK,
        },
        base_layer::ZkSyncBaseLayerVerificationKey,
        recursion_layer::{ZkSyncRecursionLayerStorageType, ZkSyncRecursionLayerVerificationKey},
    },
    zkevm_circuits::scheduler::aux::BaseLayerCircuitType,
};
#[cfg(feature = "gpu")]
use fflonk_gpu::{FflonkSnarkVerifierCircuitDeviceSetup, FflonkSnarkVerifierCircuitVK};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
#[cfg(any(feature = "gpu", feature = "gpu-light"))]
use shivini::boojum::field::goldilocks::GoldilocksField;
use zkevm_test_harness::data_source::{in_memory_data_source::InMemoryDataSource, SetupDataSource};
use zksync_prover_fri_types::{ProverServiceDataKey, ProvingStage, MAX_COMPRESSION_CIRCUITS};
use zksync_utils::env::Workspace;

#[cfg(feature = "gpu")]
use crate::compressor::CompressorSetupData;
#[cfg(any(feature = "gpu", feature = "gpu-light"))]
use crate::{GoldilocksGpuProverSetupData, GpuProverSetupData};
use crate::{GoldilocksProverSetupData, VkCommitments};

#[derive(Debug, Clone, Copy)]
pub enum ProverServiceDataType {
    VerificationKey,
    SetupData,
    FinalizationHints,
    PlonkSetupData,
    FflonkSetupData,
    SnarkVerificationKey,
    FflonkSnarkVerificationKey,
}

/// Key store manages all the prover keys.
/// There are 2 types:
/// - small verification, finalization keys (used only during verification)
/// - large setup keys, used during proving.
#[derive(Clone)]
pub struct Keystore {
    /// Directory to store all the small keys.
    basedir: PathBuf,
    /// Directory to store large setup keys.
    setup_data_path: PathBuf,
    /// Setup data cache for proof compressor
    #[cfg(feature = "gpu")]
    pub setup_data_cache_proof_compressor: Arc<CompressorSetupData>,
}

impl Keystore {
    /// Base-dir is the location of smaller keys (like verification keys and finalization hints).
    /// Setup data path is used for the large setup keys.
    pub fn new(basedir: PathBuf) -> Self {
        Keystore {
            basedir: basedir.clone(),
            setup_data_path: basedir,
            #[cfg(feature = "gpu")]
            setup_data_cache_proof_compressor: Arc::new(CompressorSetupData::new()),
        }
    }

    /// Uses automatic detection of the base path, and assumes that setup keys
    /// are stored in the same directory.
    ///
    /// The "base" path is considered to be equivalent to the `prover/data/keys`
    /// directory in the repository.
    pub fn locate() -> Self {
        // There might be several cases:
        // - We're running from the prover workspace.
        // - We're running from the core workspace.
        // - We're running the binary from the docker.
        let data_dir_path = match Workspace::locate() {
            Workspace::Root => {
                // We're running a binary, likely in a docker.
                // Keys can be in one of a few paths.
                // We want to be very conservative here, and checking
                // more locations than we likely need to not accidentally
                // break something.
                let paths = ["./prover/data", "./data", "/prover/data", "/data"];
                paths.iter().map(PathBuf::from).find(|path| path.exists()).unwrap_or_else(|| {
                    panic!("Failed to locate the prover data directory. Locations checked: {paths:?}")
                })
            }
            ws => {
                // If we're running in the Cargo workspace, the data *must* be in `prover/data`.
                ws.prover().join("data")
            }
        };
        let base_path = data_dir_path.join("keys");

        #[cfg(feature = "gpu")]
        let setup_data_cache_proof_compressor = Arc::new(CompressorSetupData::new());

        Self {
            basedir: base_path.clone(),
            setup_data_path: base_path,
            #[cfg(feature = "gpu")]
            setup_data_cache_proof_compressor,
        }
    }

    /// Will override the setup path, if present.
    pub fn with_setup_path(mut self, setup_data_path: Option<PathBuf>) -> Self {
        if let Some(setup_data_path) = setup_data_path {
            self.setup_data_path = setup_data_path;
        }
        self
    }

    pub fn get_base_path(&self) -> &PathBuf {
        &self.basedir
    }

    pub(crate) fn get_file_path(
        &self,
        key: ProverServiceDataKey,
        service_data_type: ProverServiceDataType,
    ) -> PathBuf {
        let name = key.name();
        match service_data_type {
            ProverServiceDataType::VerificationKey => {
                self.basedir.join(format!("verification_{}_key.json", name))
            }
            ProverServiceDataType::SetupData => self
                .setup_data_path
                .join(format!("setup_{}_data.bin", name)),
            ProverServiceDataType::FinalizationHints => self
                .basedir
                .join(format!("finalization_hints_{}.bin", name)),
            ProverServiceDataType::PlonkSetupData => self
                .setup_data_path
                .join(format!("plonk_setup_{}_data.bin", name)),
            ProverServiceDataType::FflonkSetupData => self
                .setup_data_path
                .join(format!("fflonk_setup_{}_data.bin", name)),
            ProverServiceDataType::SnarkVerificationKey => {
                self.basedir.join(format!("verification_{}_key.json", name))
            }
            ProverServiceDataType::FflonkSnarkVerificationKey => self
                .basedir
                .join(format!("fflonk_verification_{}_key.json", name)),
        }
    }

    fn load_json_from_file<T: for<'a> Deserialize<'a>>(
        filepath: impl AsRef<Path> + std::fmt::Debug,
    ) -> anyhow::Result<T> {
        let text = std::fs::read_to_string(&filepath)
            .with_context(|| format!("Failed reading verification key from path: {filepath:?}"))?;
        serde_json::from_str::<T>(&text).with_context(|| {
            format!("Failed deserializing verification key from path: {filepath:?}")
        })
    }
    fn save_json_pretty<T: Serialize>(
        filepath: impl AsRef<Path> + std::fmt::Debug,
        data: &T,
    ) -> anyhow::Result<()> {
        std::fs::write(&filepath, serde_json::to_string_pretty(data).unwrap())
            .with_context(|| format!("writing to '{filepath:?}' failed"))
    }

    fn load_bincode_from_file<T: for<'a> Deserialize<'a>>(
        filepath: impl AsRef<Path> + std::fmt::Debug,
    ) -> anyhow::Result<T> {
        let mut file = File::open(&filepath)
            .with_context(|| format!("Failed reading setup-data from path: {filepath:?}"))?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).with_context(|| {
            format!("Failed reading setup-data to buffer from path: {filepath:?}")
        })?;
        bincode::deserialize::<T>(&buffer)
            .with_context(|| format!("Failed deserializing setup-data at path: {filepath:?}"))
    }

    ///
    ///   Verification keys
    ///

    pub fn load_base_layer_verification_key(
        &self,
        circuit_type: u8,
    ) -> anyhow::Result<ZkSyncBaseLayerVerificationKey> {
        Self::load_json_from_file(self.get_file_path(
            ProverServiceDataKey::new(circuit_type, ProvingStage::BasicCircuits),
            ProverServiceDataType::VerificationKey,
        ))
    }

    pub fn load_recursive_layer_verification_key(
        &self,
        circuit_type: u8,
    ) -> anyhow::Result<ZkSyncRecursionLayerVerificationKey> {
        if circuit_type == ZkSyncRecursionLayerStorageType::SchedulerCircuit as u8 {
            let vk = Self::load_json_from_file(self.get_file_path(
                ProverServiceDataKey::new_recursive(circuit_type),
                ProverServiceDataType::VerificationKey,
            ))?;
            return Ok(ZkSyncRecursionLayerVerificationKey::SchedulerCircuit(vk));
        }

        Self::load_json_from_file(self.get_file_path(
            ProverServiceDataKey::new_recursive(circuit_type),
            ProverServiceDataType::VerificationKey,
        ))
    }

    pub fn load_compression_verification_key(
        &self,
        circuit_type: u8,
    ) -> anyhow::Result<ZkSyncCompressionLayerVerificationKey> {
        let key = Self::load_json_from_file(self.get_file_path(
            ProverServiceDataKey::new_compression(circuit_type),
            ProverServiceDataType::VerificationKey,
        ))?;

        match circuit_type {
            1 => Ok(ZkSyncCompressionLayerVerificationKey::CompressionMode1Circuit(key)),
            2 => Ok(ZkSyncCompressionLayerVerificationKey::CompressionMode2Circuit(key)),
            3 => Ok(ZkSyncCompressionLayerVerificationKey::CompressionMode3Circuit(key)),
            4 => Ok(ZkSyncCompressionLayerVerificationKey::CompressionMode4Circuit(key)),
            _ => Err(anyhow::anyhow!(
                "Invalid compression circuit type: {}",
                circuit_type
            )),
        }
    }

    pub fn load_compression_for_wrapper_vk(
        &self,
        circuit_type: u8,
    ) -> anyhow::Result<ZkSyncCompressionForWrapperVerificationKey> {
        let key = Self::load_json_from_file(self.get_file_path(
            ProverServiceDataKey::new_compression_wrapper(circuit_type),
            ProverServiceDataType::VerificationKey,
        ))?;

        match circuit_type {
            1 => Ok(ZkSyncCompressionForWrapperVerificationKey::CompressionMode1Circuit(key)),
            2 => Ok(ZkSyncCompressionForWrapperVerificationKey::CompressionMode2Circuit(key)),
            3 => Ok(ZkSyncCompressionForWrapperVerificationKey::CompressionMode3Circuit(key)),
            4 => Ok(ZkSyncCompressionForWrapperVerificationKey::CompressionMode4Circuit(key)),
            5 => Ok(ZkSyncCompressionForWrapperVerificationKey::CompressionMode5Circuit(key)),
            _ => Err(anyhow::anyhow!(
                "Invalid compression circuit type: {}",
                circuit_type
            )),
        }
    }

    pub fn save_base_layer_verification_key(
        &self,
        vk: ZkSyncBaseLayerVerificationKey,
    ) -> anyhow::Result<()> {
        let filepath = self.get_file_path(
            ProverServiceDataKey::new(vk.numeric_circuit_type(), ProvingStage::BasicCircuits),
            ProverServiceDataType::VerificationKey,
        );
        tracing::info!("saving basic verification key to: {:?}", filepath);
        Self::save_json_pretty(filepath, &vk)
    }

    pub fn save_recursive_layer_verification_key(
        &self,
        vk: ZkSyncRecursionLayerVerificationKey,
    ) -> anyhow::Result<()> {
        let filepath = self.get_file_path(
            ProverServiceDataKey::new_recursive(vk.numeric_circuit_type()),
            ProverServiceDataType::VerificationKey,
        );

        if let ZkSyncRecursionLayerVerificationKey::SchedulerCircuit(key) = vk {
            tracing::info!("saving recursive layer verification key to: {:?}", filepath);
            return Self::save_json_pretty(filepath, &key);
        }

        tracing::info!("saving recursive layer verification key to: {:?}", filepath);
        Self::save_json_pretty(filepath, &vk)
    }

    pub fn save_compression_vk(
        &self,
        vk: ZkSyncCompressionLayerVerificationKey,
    ) -> anyhow::Result<()> {
        let filepath = self.get_file_path(
            ProverServiceDataKey::new_compression(vk.numeric_circuit_type()),
            ProverServiceDataType::VerificationKey,
        );
        tracing::info!(
            "saving compression layer verification key to: {:?}",
            filepath
        );
        Self::save_json_pretty(filepath, &vk.into_inner())
    }

    pub fn save_compression_for_wrapper_vk(
        &self,
        vk: ZkSyncCompressionForWrapperVerificationKey,
    ) -> anyhow::Result<()> {
        let filepath = self.get_file_path(
            ProverServiceDataKey::new_compression_wrapper(vk.numeric_circuit_type()),
            ProverServiceDataType::VerificationKey,
        );
        tracing::info!(
            "saving compression wrapper verification key to: {:?}",
            filepath
        );
        Self::save_json_pretty(filepath, &vk.into_inner())
    }

    ///
    /// Finalization hints
    ///

    pub fn save_finalization_hints(
        &self,
        key: ProverServiceDataKey,
        hint: &FinalizationHintsForProver,
    ) -> anyhow::Result<()> {
        let filepath = self.get_file_path(key, ProverServiceDataType::FinalizationHints);

        tracing::info!("saving finalization hints for {:?} to: {:?}", key, filepath);
        let serialized =
            bincode::serialize(&hint).context("Failed to serialize finalization hints")?;
        fs::write(filepath, serialized).context("Failed to write finalization hints to file")
    }

    pub fn load_finalization_hints(
        &self,
        key: ProverServiceDataKey,
    ) -> anyhow::Result<FinalizationHintsForProver> {
        let mut key = key;
        // For `NodeAggregation` round we have only 1 finalization hints for all circuit type.
        // TODO: is this needed??
        if key.stage == ProvingStage::NodeAggregation {
            key.circuit_id = ZkSyncRecursionLayerStorageType::NodeLayerCircuit as u8;
        }
        Self::load_bincode_from_file(
            self.get_file_path(key, ProverServiceDataType::FinalizationHints),
        )
    }

    ///
    ///   Snark wrapper
    ///

    /// Loads snark verification key
    // For snark wrapper - we're actually returning a raw serialized string, and the parsing happens
    // on the reader's side (in proof compressor).
    // This way, we avoid including the old 1.3.3 test harness to our main library.
    pub fn load_snark_verification_key(&self) -> anyhow::Result<String> {
        let filepath = self.get_file_path(
            ProverServiceDataKey::snark(),
            ProverServiceDataType::SnarkVerificationKey,
        );
        std::fs::read_to_string(&filepath).with_context(|| {
            format!("Failed reading Snark verification key from path: {filepath:?}")
        })
    }

    pub fn load_fflonk_snark_verification_key(&self) -> anyhow::Result<String> {
        let filepath = self.get_file_path(
            ProverServiceDataKey::snark(),
            ProverServiceDataType::FflonkSnarkVerificationKey,
        );
        std::fs::read_to_string(&filepath).with_context(|| {
            format!("Failed reading FFLONK Snark verification key from path: {filepath:?}")
        })
    }

    pub fn save_snark_verification_key(&self, vk: ZkSyncSnarkWrapperVK) -> anyhow::Result<()> {
        let filepath = self.get_file_path(
            ProverServiceDataKey::snark(),
            ProverServiceDataType::SnarkVerificationKey,
        );
        tracing::info!("saving snark verification key to: {:?}", filepath);
        Self::save_json_pretty(filepath, &vk.into_inner())
    }

    #[cfg(feature = "gpu")]
    pub fn save_fflonk_snark_verification_key(
        &self,
        vk: FflonkSnarkVerifierCircuitVK,
    ) -> anyhow::Result<()> {
        let filepath = self.get_file_path(
            ProverServiceDataKey::snark(),
            ProverServiceDataType::FflonkSnarkVerificationKey,
        );
        tracing::info!("saving snark verification key to: {:?}", filepath);
        Self::save_json_pretty(filepath, &vk)
    }

    ///
    /// Setup keys
    ///

    pub fn load_cpu_setup_data_for_circuit_type(
        &self,
        key: ProverServiceDataKey,
    ) -> anyhow::Result<GoldilocksProverSetupData> {
        let filepath = self.get_file_path(key, ProverServiceDataType::SetupData);

        let mut file = File::open(filepath.clone())
            .with_context(|| format!("Failed reading setup-data from path: {filepath:?}"))?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).with_context(|| {
            format!("Failed reading setup-data to buffer from path: {filepath:?}")
        })?;
        tracing::info!("loading {:?} setup data from path: {:?}", key, filepath);
        bincode::deserialize::<GoldilocksProverSetupData>(&buffer).with_context(|| {
            format!("Failed deserializing setup-data at path: {filepath:?} for circuit: {key:?}")
        })
    }

    #[cfg(any(feature = "gpu", feature = "gpu-light"))]
    pub fn load_gpu_setup_data_for_circuit_type(
        &self,
        key: ProverServiceDataKey,
    ) -> anyhow::Result<GoldilocksGpuProverSetupData> {
        let filepath = self.get_file_path(key, ProverServiceDataType::SetupData);

        let mut file = File::open(filepath.clone())
            .with_context(|| format!("Failed reading setup-data from path: {filepath:?}"))?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).with_context(|| {
            format!("Failed reading setup-data to buffer from path: {filepath:?}")
        })?;
        tracing::info!("loading {:?} setup data from path: {:?}", key, filepath);
        bincode::deserialize::<GoldilocksGpuProverSetupData>(&buffer).with_context(|| {
            format!("Failed deserializing setup-data at path: {filepath:?} for circuit: {key:?}")
        })
    }

    #[cfg(any(feature = "gpu", feature = "gpu-light"))]
    pub fn load_compression_layer_setup_data(
        &self,
        circuit: u8,
    ) -> anyhow::Result<GpuProverSetupData<GoldilocksField, CompressionProofsTreeHasher>> {
        let filepath = self.get_file_path(
            ProverServiceDataKey::new_compression(circuit),
            ProverServiceDataType::SetupData,
        );

        let mut file = File::open(filepath.clone())
            .with_context(|| format!("Failed reading setup-data from path: {filepath:?}"))?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).with_context(|| {
            format!("Failed reading setup-data to buffer from path: {filepath:?}")
        })?;
        tracing::info!(
            "loading compression layer setup data from path: {:?}",
            filepath
        );
        bincode::deserialize::<GpuProverSetupData<GoldilocksField, CompressionProofsTreeHasher>>(
            &buffer,
        )
        .with_context(|| {
            format!("Failed deserializing compression layer setup data at path: {filepath:?}")
        })
    }

    #[cfg(any(feature = "gpu", feature = "gpu-light"))]
    pub fn load_compression_wrapper_setup_data(
        &self,
        circuit: u8,
    ) -> anyhow::Result<GpuProverSetupData<GoldilocksField, CompressionProofsTreeHasherForWrapper>>
    {
        let filepath = self.get_file_path(
            ProverServiceDataKey::new_compression_wrapper(circuit),
            ProverServiceDataType::SetupData,
        );

        let mut file = File::open(filepath.clone())
            .with_context(|| format!("Failed reading setup-data from path: {filepath:?}"))?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).with_context(|| {
            format!("Failed reading setup-data to buffer from path: {filepath:?}")
        })?;
        tracing::info!(
            "loading compression wrapper setup data from path: {:?}",
            filepath
        );
        bincode::deserialize::<
            GpuProverSetupData<GoldilocksField, CompressionProofsTreeHasherForWrapper>,
        >(&buffer)
        .with_context(|| {
            format!("Failed deserializing compression wrapper setup data at path: {filepath:?}")
        })
    }

    #[cfg(feature = "gpu")]
    pub fn load_fflonk_snark_verifier_setup_data(
        &self,
    ) -> anyhow::Result<FflonkSnarkVerifierCircuitDeviceSetup> {
        let filepath = self.get_file_path(
            ProverServiceDataKey::snark(),
            ProverServiceDataType::SetupData,
        );

        let file = File::open(filepath.clone())
            .with_context(|| format!("Failed reading setup-data from path: {filepath:?}"))?;
        FflonkSnarkVerifierCircuitDeviceSetup::read(file)
            .context("Failed reading FFLONK SNARK setup data from a file")
    }

    pub fn is_setup_data_present(&self, key: &ProverServiceDataKey) -> bool {
        Path::new(&self.get_file_path(*key, ProverServiceDataType::SetupData)).exists()
    }

    pub fn save_setup_data_for_circuit_type(
        &self,
        key: ProverServiceDataKey,
        serialized_setup_data: &Vec<u8>,
    ) -> anyhow::Result<()> {
        let filepath = self.get_file_path(key, ProverServiceDataType::SetupData);
        tracing::info!("saving {:?} setup data to: {:?}", key, filepath);
        std::fs::write(filepath.clone(), serialized_setup_data)
            .with_context(|| format!("Failed saving setup-data at path: {filepath:?}"))
    }

    #[cfg(feature = "gpu")]
    pub fn save_fflonk_snark_setup_data(
        &self,
        setup_data: FflonkSnarkVerifierCircuitDeviceSetup,
    ) -> anyhow::Result<()> {
        let filepath = self.get_file_path(
            ProverServiceDataKey::snark(),
            ProverServiceDataType::SetupData,
        );

        let file = File::create(filepath.clone())
            .with_context(|| format!("Failed creating setup-data file at path: {filepath:?}"))?;

        setup_data
            .write(file)
            .context("Failed writing FFLONK SNARK setup data to a file")
    }

    /// Loads all the verification keys into the Data Source.
    /// Keys are loaded from the default 'base path' files.
    pub fn load_keys_to_data_source(&self) -> anyhow::Result<InMemoryDataSource> {
        let mut data_source = InMemoryDataSource::new();
        for base_circuit_type in BaseLayerCircuitType::as_iter_u8() {
            data_source
                .set_base_layer_vk(self.load_base_layer_verification_key(base_circuit_type)?)
                .unwrap();
        }

        for circuit_type in ZkSyncRecursionLayerStorageType::as_iter_u8() {
            data_source
                .set_recursion_layer_vk(self.load_recursive_layer_verification_key(circuit_type)?)
                .unwrap();
        }
        data_source
            .set_recursion_tip_vk(self.load_recursive_layer_verification_key(
                ZkSyncRecursionLayerStorageType::RecursionTipCircuit as u8,
            )?)
            .unwrap();

        data_source
            .set_recursion_layer_node_vk(self.load_recursive_layer_verification_key(
                ZkSyncRecursionLayerStorageType::NodeLayerCircuit as u8,
            )?)
            .unwrap();

        for circuit in 1..MAX_COMPRESSION_CIRCUITS {
            data_source
                .set_compression_vk(self.load_compression_verification_key(circuit)?)
                .unwrap();
        }

        data_source
            .set_compression_for_wrapper_vk(self.load_compression_for_wrapper_vk(5)?)
            .unwrap();

        Ok(data_source)
    }

    pub fn save_keys_from_data_source(&self, source: &dyn SetupDataSource) -> anyhow::Result<()> {
        // Base circuits
        for base_circuit_type in BaseLayerCircuitType::as_iter_u8() {
            let vk = source.get_base_layer_vk(base_circuit_type).map_err(|err| {
                anyhow::anyhow!("No vk exist for circuit type: {base_circuit_type}: {err}")
            })?;
            self.save_base_layer_verification_key(vk)
                .context("save_base_layer_vk()")?;

            let hint = source
                .get_base_layer_finalization_hint(base_circuit_type)
                .map_err(|err| {
                    anyhow::anyhow!(
                        "No finalization_hint exist for circuit type: {base_circuit_type}: {err}"
                    )
                })?
                .into_inner();
            let key = ProverServiceDataKey::new(base_circuit_type, ProvingStage::BasicCircuits);
            self.save_finalization_hints(key, &hint)
                .context("save_finalization_hints()")?;
        }
        // Leaf circuits
        for leaf_circuit_type in ZkSyncRecursionLayerStorageType::leafs_as_iter_u8() {
            let vk = source
                .get_recursion_layer_vk(leaf_circuit_type)
                .map_err(|err| {
                    anyhow::anyhow!("No vk exist for circuit type: {leaf_circuit_type}: {err}")
                })?;
            self.save_recursive_layer_verification_key(vk)
                .context("save_recursive_layer_vk()")?;

            let hint = source
                .get_recursion_layer_finalization_hint(leaf_circuit_type)
                .map_err(|err| {
                    anyhow::anyhow!(
                        "No finalization hint exist for circuit type: {leaf_circuit_type}: {err}"
                    )
                })?
                .into_inner();
            let key = ProverServiceDataKey::new_recursive(leaf_circuit_type);
            self.save_finalization_hints(key, &hint)
                .context("save_finalization_hints()")?;
        }
        // Node
        self.save_recursive_layer_verification_key(
            source
                .get_recursion_layer_node_vk()
                .map_err(|err| anyhow::anyhow!("No vk exist for node layer circuit: {err}"))?,
        )
        .context("save_recursive_layer_vk")?;

        let node_hint = source
            .get_recursion_layer_node_finalization_hint()
            .map_err(|err| {
                anyhow::anyhow!("No finalization hint exist for node layer circuit: {err}")
            })?
            .into_inner();
        self.save_finalization_hints(
            ProverServiceDataKey::new_recursive(
                ZkSyncRecursionLayerStorageType::NodeLayerCircuit as u8,
            ),
            &node_hint,
        )
        .context("save_finalization_hints()")?;

        // Recursion tip
        self.save_recursive_layer_verification_key(source.get_recursion_tip_vk().map_err(
            |err| anyhow::anyhow!("No vk exist for recursion tip layer circuit: {err}"),
        )?)
        .context("save_recursion_tip_vk")?;

        let recursion_tip_hint = source
            .get_recursion_tip_finalization_hint()
            .map_err(|err| {
                anyhow::anyhow!("No finalization hint exist for recursion tip layer circuit: {err}")
            })?
            .into_inner();
        self.save_finalization_hints(
            ProverServiceDataKey::new_recursive(
                ZkSyncRecursionLayerStorageType::RecursionTipCircuit as u8,
            ),
            &recursion_tip_hint,
        )
        .context("save_finalization_hints()")?;

        // Scheduler
        self.save_recursive_layer_verification_key(
            source
                .get_recursion_layer_vk(ZkSyncRecursionLayerStorageType::SchedulerCircuit as u8)
                .map_err(|err| anyhow::anyhow!("No vk exist for scheduler circuit: {err}"))?,
        )
        .context("save_recursive_layer_vk")?;

        let scheduler_hint = source
            .get_recursion_layer_finalization_hint(
                ZkSyncRecursionLayerStorageType::SchedulerCircuit as u8,
            )
            .map_err(|err| {
                anyhow::anyhow!("No finalization hint exist for scheduler layer circuit: {err}")
            })?
            .into_inner();

        self.save_finalization_hints(
            ProverServiceDataKey::new_recursive(
                ZkSyncRecursionLayerStorageType::SchedulerCircuit as u8,
            ),
            &scheduler_hint,
        )
        .context("save_finalization_hints()")?;

        Ok(())
    }

    pub fn load_commitments(&self) -> anyhow::Result<VkCommitments> {
        Self::load_json_from_file(self.get_base_path().join("commitments.json"))
    }

    pub fn save_commitments(&self, commitments: &VkCommitments) -> anyhow::Result<()> {
        Self::save_json_pretty(self.get_base_path().join("commitments.json"), &commitments)
    }

    /// Async loads mapping of all circuits to setup key, if successful
    #[cfg(any(feature = "gpu", feature = "gpu-light"))]
    pub async fn load_all_setup_key_mapping(
        &self,
    ) -> anyhow::Result<HashMap<ProverServiceDataKey, Arc<GoldilocksGpuProverSetupData>>> {
        self.load_key_mapping(ProverServiceDataType::SetupData)
            .await
    }

    /// Async loads mapping of all circuits to finalization hints, if successful
    pub async fn load_all_finalization_hints_mapping(
        &self,
    ) -> anyhow::Result<HashMap<ProverServiceDataKey, Arc<FinalizationHintsForProver>>> {
        self.load_key_mapping(ProverServiceDataType::FinalizationHints)
            .await
    }

    /// Async function that loads mapping from disk.
    /// Whilst IO is not parallelizable, ser/de is.
    async fn load_key_mapping<T: DeserializeOwned + Send + Sync + 'static>(
        &self,
        data_type: ProverServiceDataType,
    ) -> anyhow::Result<HashMap<ProverServiceDataKey, Arc<T>>> {
        let mut mapping: HashMap<ProverServiceDataKey, Arc<T>> = HashMap::new();

        // Load each file in parallel. Note that FS access is not necessarily parallel, but
        // deserialization is. For larger files, it makes a big difference.
        // Note: `collect` is important, because iterators are lazy, and otherwise we won't actually
        // spawn threads.
        let handles: Vec<_> = ProverServiceDataKey::all_boojum()
            .into_iter()
            .map(|key| {
                let filepath = self.get_file_path(key, data_type);
                tokio::task::spawn_blocking(move || {
                    let data = Self::load_bincode_from_file(filepath)?;
                    anyhow::Ok((key, Arc::new(data)))
                })
            })
            .collect();
        for handle in futures::future::join_all(handles).await {
            let (key, setup_data) = handle.context("future loading key panicked")??;
            mapping.insert(key, setup_data);
        }
        Ok(mapping)
    }

    /// Async function that loads specified mapping from disk.
    pub async fn load_single_key_mapping<T: DeserializeOwned + Send + Sync + 'static>(
        &self,
        key: ProverServiceDataKey,
        data_type: ProverServiceDataType,
    ) -> anyhow::Result<Arc<T>> {
        let filepath = self.get_file_path(key, data_type);
        let data = Self::load_bincode_from_file(filepath)?;
        Ok(data)
    }
}
