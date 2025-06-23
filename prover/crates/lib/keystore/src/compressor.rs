use std::{
    fs::File,
    io::{Read, Write},
    sync::{Arc, OnceLock},
    thread,
};

use anyhow::Context;
use circuit_definitions::{
    boojum::{cs::implementations::verifier::VerificationKey, field::goldilocks::GoldilocksField},
    circuit_definitions::{
        aux_layer::compression_modes::{
            CompressionMode1, CompressionMode1ForWrapper, CompressionMode2, CompressionMode3,
            CompressionMode4, CompressionMode5ForWrapper,
        },
        recursion_layer::ZkSyncRecursionLayerStorageType,
    },
};
use proof_compression_gpu::{
    proof_system::{fflonk::FflonkSnarkWrapper, plonk::PlonkSnarkWrapper},
    step::{
        compression::CompressionSetupData, snark_wrapper::SnarkWrapperSetupData, CompressionStep,
    },
    CompressionStepExt, CompressorBlobStorage, CompressorBlobStorageExt, ProofSystemDefinition,
    SnarkWrapperProofSystem, SnarkWrapperStep, SnarkWrapperStepExt,
};
use zksync_prover_fri_types::ProverServiceDataKey;

use crate::keystore::{Keystore, ProverServiceDataType};

const COMPACT_CRS_ENV_VAR: &str = "COMPACT_CRS_FILE";

pub struct PlonkSetupData {
    pub compression_mode1_for_wrapper_setup_data:
        OnceLock<anyhow::Result<CompressionSetupData<CompressionMode1ForWrapper>>>,
    // We can't use cache for Plonk setup data so we load it in-place
}

pub struct FflonkSetupData {
    pub compression_mode1_setup_data:
        OnceLock<anyhow::Result<CompressionSetupData<CompressionMode1>>>,
    pub compression_mode2_setup_data:
        OnceLock<anyhow::Result<CompressionSetupData<CompressionMode2>>>,
    pub compression_mode3_setup_data:
        OnceLock<anyhow::Result<CompressionSetupData<CompressionMode3>>>,
    pub compression_mode4_setup_data:
        OnceLock<anyhow::Result<CompressionSetupData<CompressionMode4>>>,
    pub compression_mode5_for_wrapper_setup_data:
        OnceLock<anyhow::Result<CompressionSetupData<CompressionMode5ForWrapper>>>,
    pub fflonk_snark_wrapper_setup_data:
        OnceLock<anyhow::Result<SnarkWrapperSetupData<FflonkSnarkWrapper>>>,
}

pub struct CompressorSetupData {
    pub fflonk_setup_data: FflonkSetupData,
    pub plonk_setup_data: PlonkSetupData,
}

impl CompressorSetupData {
    pub fn new() -> Self {
        Self {
            fflonk_setup_data: FflonkSetupData {
                compression_mode1_setup_data: OnceLock::new(),
                compression_mode2_setup_data: OnceLock::new(),
                compression_mode3_setup_data: OnceLock::new(),
                compression_mode4_setup_data: OnceLock::new(),
                compression_mode5_for_wrapper_setup_data: OnceLock::new(),
                fflonk_snark_wrapper_setup_data: OnceLock::new(),
            },
            plonk_setup_data: PlonkSetupData {
                compression_mode1_for_wrapper_setup_data: OnceLock::new(),
            },
        }
    }
}

impl Default for CompressorSetupData {
    fn default() -> Self {
        Self::new()
    }
}

impl Keystore {
    fn read_file_for_compression(
        &self,
        key: ProverServiceDataKey,
        service_data_type: ProverServiceDataType,
    ) -> anyhow::Result<Box<dyn Read>> {
        let filepath = self.get_file_path(key, service_data_type);
        Ok(Box::new(File::open(filepath)?))
    }

    fn read_compact_raw_crs(&self) -> anyhow::Result<Box<dyn Read>> {
        let filepath =
            std::env::var(COMPACT_CRS_ENV_VAR).context("No compact CRS file path provided")?;
        Ok(Box::new(File::open(filepath)?))
    }

    fn write_file_for_compression(
        &self,
        key: ProverServiceDataKey,
        service_data_type: ProverServiceDataType,
    ) -> anyhow::Result<Box<dyn Write>> {
        let filepath = self.get_file_path(key, service_data_type);
        Ok(Box::new(File::create(filepath)?))
    }

    pub fn write_compact_raw_crs(&self) -> anyhow::Result<Box<dyn Write>> {
        let filepath =
            std::env::var(COMPACT_CRS_ENV_VAR).context("No compact CRS file path provided")?;
        Ok(Box::new(File::create(filepath)?))
    }
}

impl Keystore {
    fn load_compression_vk<CS: CompressionStep>(&self) -> anyhow::Result<CS::VK> {
        let service_data_type = ProverServiceDataType::VerificationKey;
        let key = if CS::IS_WRAPPER {
            ProverServiceDataKey::new_compression_wrapper(CS::MODE)
        } else {
            ProverServiceDataKey::new_compression(CS::MODE)
        };
        let reader = self.read_file_for_compression(key, service_data_type)?;
        CS::load_this_vk(reader)
    }
    fn load_compression_precomputation<CS: CompressionStep>(
        &self,
    ) -> anyhow::Result<CS::Precomputation> {
        let service_data_type = ProverServiceDataType::SetupData;
        let key = if CS::IS_WRAPPER {
            ProverServiceDataKey::new_compression_wrapper(CS::MODE)
        } else {
            ProverServiceDataKey::new_compression(CS::MODE)
        };
        let reader = self.read_file_for_compression(key, service_data_type)?;
        CS::get_precomputation(reader)
    }
    fn load_compression_finalization_hint<CS: CompressionStep>(
        &self,
    ) -> anyhow::Result<CS::FinalizationHint> {
        let service_data_type = ProverServiceDataType::FinalizationHints;
        let key = if CS::IS_WRAPPER {
            ProverServiceDataKey::new_compression_wrapper(CS::MODE)
        } else {
            ProverServiceDataKey::new_compression(CS::MODE)
        };
        let reader = self.read_file_for_compression(key, service_data_type)?;
        CS::load_finalization_hint(reader)
    }
    fn load_compression_previous_vk<CS: CompressionStep>(
        &self,
    ) -> anyhow::Result<VerificationKey<GoldilocksField, CS::PreviousStepTreeHasher>> {
        let service_data_type = ProverServiceDataType::VerificationKey;
        anyhow::ensure!(CS::MODE >= 1);
        let key = if CS::MODE == 1 {
            ProverServiceDataKey::new_recursive(
                ZkSyncRecursionLayerStorageType::SchedulerCircuit as u8,
            )
        } else {
            ProverServiceDataKey::new_compression(CS::MODE - 1)
        };
        let reader = self.read_file_for_compression(key, service_data_type)?;
        CS::load_previous_vk(reader)
    }

    fn load_snark_wrapper_precomputation<WS: SnarkWrapperStep>(
        &self,
    ) -> anyhow::Result<WS::Precomputation> {
        anyhow::ensure!(WS::IS_FFLONK ^ WS::IS_PLONK);
        let key = ProverServiceDataKey::snark();
        let service_data_type = if WS::IS_FFLONK {
            ProverServiceDataType::FflonkSetupData
        } else {
            ProverServiceDataType::PlonkSetupData
        };
        let reader = self.read_file_for_compression(key, service_data_type)?;
        WS::get_precomputation(reader)
    }
    fn load_snark_wrapper_vk<WS: SnarkWrapperStep>(&self) -> anyhow::Result<WS::VK> {
        anyhow::ensure!(WS::IS_FFLONK ^ WS::IS_PLONK);
        let key = ProverServiceDataKey::snark();
        let service_data_type = if WS::IS_FFLONK {
            anyhow::ensure!(WS::IS_PLONK == false);
            ProverServiceDataType::FflonkSnarkVerificationKey
        } else {
            anyhow::ensure!(WS::IS_PLONK == true);
            ProverServiceDataType::SnarkVerificationKey
        };
        let reader = self.read_file_for_compression(key, service_data_type)?;
        WS::load_this_vk(reader)
    }
    fn load_snark_wrapper_finalization_hint<WS: SnarkWrapperStep>(
        &self,
    ) -> anyhow::Result<WS::FinalizationHint> {
        WS::load_finalization_hint()
    }
    fn load_snark_wrapper_crs<WS: SnarkWrapperStep>(&self) -> anyhow::Result<WS::CRS> {
        anyhow::ensure!(WS::IS_FFLONK ^ WS::IS_PLONK);
        let reader = self.read_compact_raw_crs()?;
        <WS as SnarkWrapperStep>::load_compact_raw_crs(reader)
    }
    fn load_snark_wrapper_previous_vk<WS: SnarkWrapperStep>(
        &self,
    ) -> anyhow::Result<VerificationKey<GoldilocksField, WS::PreviousStepTreeHasher>> {
        anyhow::ensure!(WS::IS_FFLONK ^ WS::IS_PLONK);
        let previous_compression_mode = WS::PREVIOUS_COMPRESSION_MODE;
        let key = ProverServiceDataKey::new_compression_wrapper(previous_compression_mode);
        let service_data_type = ProverServiceDataType::VerificationKey;
        let reader = self.read_file_for_compression(key, service_data_type)?;
        WS::load_previous_vk(reader)
    }

    fn load_compression_setup_data<CS: CompressionStep>(
        &self,
    ) -> anyhow::Result<CompressionSetupData<CS>> {
        let vk = self.load_compression_vk::<CS>()?;
        let previous_vk = self.load_compression_previous_vk::<CS>()?;
        let precomputation = self.load_compression_precomputation::<CS>()?;
        let finalization_hint = self.load_compression_finalization_hint::<CS>()?;
        Ok(CompressionSetupData {
            vk,
            previous_vk,
            precomputation,
            finalization_hint,
        })
    }

    fn load_snark_wrapper_setup_data<WS: SnarkWrapperStep>(
        &self,
    ) -> anyhow::Result<SnarkWrapperSetupData<WS>> {
        let vk = self.load_snark_wrapper_vk::<WS>()?;
        let previous_vk = self.load_snark_wrapper_previous_vk::<WS>()?;
        let precomputation = self.load_snark_wrapper_precomputation::<WS>()?;
        let crs = self.load_snark_wrapper_crs::<WS>()?;
        let finalization_hint = self.load_snark_wrapper_finalization_hint::<WS>()?;
        Ok(SnarkWrapperSetupData {
            vk,
            previous_vk,
            precomputation,
            finalization_hint,
            crs,
        })
    }

    fn store_compression_setup_data<CS: CompressionStepExt>(
        &self,
        precomputation: &<CS as ProofSystemDefinition>::Precomputation,
        vk: &<CS as ProofSystemDefinition>::VK,
        finalization_hint: &<CS as ProofSystemDefinition>::FinalizationHint,
    ) -> anyhow::Result<()> {
        let key = if CS::IS_WRAPPER {
            ProverServiceDataKey::new_compression_wrapper(CS::MODE)
        } else {
            ProverServiceDataKey::new_compression(CS::MODE)
        };
        <CS as CompressionStepExt>::store_precomputation(
            precomputation,
            self.write_file_for_compression(key, ProverServiceDataType::SetupData)?,
        )?;
        <CS as CompressionStepExt>::store_vk(
            vk,
            self.write_file_for_compression(key, ProverServiceDataType::VerificationKey)?,
        )?;
        <CS as CompressionStepExt>::store_finalization_hint(
            finalization_hint,
            self.write_file_for_compression(key, ProverServiceDataType::FinalizationHints)?,
        )?;
        Ok(())
    }

    fn store_snark_wrapper_setup_data<WS: SnarkWrapperStepExt>(
        &self,
        precomputation: &<WS as ProofSystemDefinition>::Precomputation,
        vk: &<WS as ProofSystemDefinition>::VK,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(WS::IS_FFLONK ^ WS::IS_PLONK);
        let key = ProverServiceDataKey::snark();
        let service_data_type_vk = if WS::IS_FFLONK {
            anyhow::ensure!(WS::IS_PLONK == false);
            ProverServiceDataType::FflonkSnarkVerificationKey
        } else {
            anyhow::ensure!(WS::IS_PLONK == true);
            ProverServiceDataType::SnarkVerificationKey
        };
        let service_data_type_precomputation = if WS::IS_FFLONK {
            ProverServiceDataType::FflonkSetupData
        } else {
            ProverServiceDataType::PlonkSetupData
        };
        <WS as SnarkWrapperStepExt>::store_precomputation(
            precomputation,
            self.write_file_for_compression(key, service_data_type_precomputation)?,
        )?;
        <WS as SnarkWrapperStepExt>::store_vk(
            vk,
            self.write_file_for_compression(key, service_data_type_vk)?,
        )?;
        Ok(())
    }
}

fn load_compression_mode1_setup_data(keystore: &Arc<Keystore>) {
    let keystore = Arc::clone(keystore);
    thread::spawn(move || {
        keystore
            .setup_data_cache_proof_compressor
            .fflonk_setup_data
            .compression_mode1_setup_data
            .get_or_init(|| {
                keystore
                    .load_compression_setup_data::<CompressionMode1>()
                    .context("Failed to load compression mode 1 setup data")
            });
    });
}

fn load_compression_mode2_setup_data(keystore: &Arc<Keystore>) {
    let keystore = Arc::clone(keystore);
    thread::spawn(move || {
        keystore
            .setup_data_cache_proof_compressor
            .fflonk_setup_data
            .compression_mode2_setup_data
            .get_or_init(|| {
                keystore
                    .load_compression_setup_data::<CompressionMode2>()
                    .context("Failed to load compression mode 2 setup data")
            });
    });
}

fn load_compression_mode3_setup_data(keystore: &Arc<Keystore>) {
    let keystore = Arc::clone(keystore);
    thread::spawn(move || {
        keystore
            .setup_data_cache_proof_compressor
            .fflonk_setup_data
            .compression_mode3_setup_data
            .get_or_init(|| {
                keystore
                    .load_compression_setup_data::<CompressionMode3>()
                    .context("Failed to load compression mode 3 setup data")
            });
    });
}

fn load_compression_mode4_setup_data(keystore: &Arc<Keystore>) {
    let keystore = Arc::clone(keystore);
    thread::spawn(move || {
        keystore
            .setup_data_cache_proof_compressor
            .fflonk_setup_data
            .compression_mode4_setup_data
            .get_or_init(|| {
                keystore
                    .load_compression_setup_data::<CompressionMode4>()
                    .context("Failed to load compression mode 4 setup data")
            });
    });
}

fn load_compression_mode5_for_wrapper_setup_data(keystore: &Arc<Keystore>) {
    let keystore = Arc::clone(keystore);
    thread::spawn(move || {
        keystore
            .setup_data_cache_proof_compressor
            .fflonk_setup_data
            .compression_mode5_for_wrapper_setup_data
            .get_or_init(|| {
                keystore
                    .load_compression_setup_data::<CompressionMode5ForWrapper>()
                    .context("Failed to load compression mode 5 for wrapper setup data")
            });
    });
}

fn load_compression_mode1_for_wrapper_setup_data(keystore: &Arc<Keystore>) {
    let keystore = Arc::clone(keystore);
    thread::spawn(move || {
        keystore
            .setup_data_cache_proof_compressor
            .plonk_setup_data
            .compression_mode1_for_wrapper_setup_data
            .get_or_init(|| {
                keystore
                    .load_compression_setup_data::<CompressionMode1ForWrapper>()
                    .context("Failed to load compression mode 1 for wrapper setup data")
            });
    });
}

fn load_fflonk_snark_wrapper_setup_data(keystore: &Arc<Keystore>) {
    let keystore = Arc::clone(keystore);
    thread::spawn(move || {
        keystore
            .setup_data_cache_proof_compressor
            .fflonk_setup_data
            .fflonk_snark_wrapper_setup_data
            .get_or_init(|| {
                keystore
                    .load_snark_wrapper_setup_data::<FflonkSnarkWrapper>()
                    .context("Failed to load Fflonk Snark Wrapper setup data")
            });
    });
}

pub fn load_all_resources(keystore: &Arc<Keystore>, is_fflonk: bool) {
    if is_fflonk {
        load_fflonk_snark_wrapper_setup_data(keystore);
        load_compression_mode1_setup_data(keystore);
        load_compression_mode2_setup_data(keystore);
        load_compression_mode3_setup_data(keystore);
        load_compression_mode4_setup_data(keystore);
        load_compression_mode5_for_wrapper_setup_data(keystore);
    } else {
        load_compression_mode1_for_wrapper_setup_data(keystore);
    }
}

impl CompressorBlobStorage for Keystore {
    fn get_compression_mode1_setup_data(
        &self,
    ) -> anyhow::Result<&CompressionSetupData<CompressionMode1>> {
        self.setup_data_cache_proof_compressor
            .fflonk_setup_data
            .compression_mode1_setup_data
            .get_or_init(|| {
                self.load_compression_setup_data::<CompressionMode1>()
                    .context("Failed to load compression mode 1 setup data")
            })
            .as_ref()
            .map_err(|e| anyhow::anyhow!("Error loading compression mode 1 setup data: {}", e))
    }

    fn get_compression_mode2_setup_data(
        &self,
    ) -> anyhow::Result<&CompressionSetupData<CompressionMode2>> {
        self.setup_data_cache_proof_compressor
            .fflonk_setup_data
            .compression_mode2_setup_data
            .get_or_init(|| {
                self.load_compression_setup_data::<CompressionMode2>()
                    .context("Failed to load compression mode 2 setup data")
            })
            .as_ref()
            .map_err(|e| anyhow::anyhow!("Error loading compression mode 2 setup data: {}", e))
    }

    fn get_compression_mode3_setup_data(
        &self,
    ) -> anyhow::Result<&CompressionSetupData<CompressionMode3>> {
        self.setup_data_cache_proof_compressor
            .fflonk_setup_data
            .compression_mode3_setup_data
            .get_or_init(|| {
                self.load_compression_setup_data::<CompressionMode3>()
                    .context("Failed to load compression mode 3 setup data")
            })
            .as_ref()
            .map_err(|e| anyhow::anyhow!("Error loading compression mode 3 setup data: {}", e))
    }

    fn get_compression_mode4_setup_data(
        &self,
    ) -> anyhow::Result<&CompressionSetupData<CompressionMode4>> {
        self.setup_data_cache_proof_compressor
            .fflonk_setup_data
            .compression_mode4_setup_data
            .get_or_init(|| {
                self.load_compression_setup_data::<CompressionMode4>()
                    .context("Failed to load compression mode 4 setup data")
            })
            .as_ref()
            .map_err(|e| anyhow::anyhow!("Error loading compression mode 4 setup data: {}", e))
    }

    fn get_compression_mode5_for_wrapper_setup_data(
        &self,
    ) -> anyhow::Result<&CompressionSetupData<CompressionMode5ForWrapper>> {
        self.setup_data_cache_proof_compressor
            .fflonk_setup_data
            .compression_mode5_for_wrapper_setup_data
            .get_or_init(|| {
                self.load_compression_setup_data::<CompressionMode5ForWrapper>()
                    .context("Failed to load compression mode 5 for wrapper setup data")
            })
            .as_ref()
            .map_err(|e| {
                anyhow::anyhow!(
                    "Error loading compression mode 5 for wrapper setup data: {}",
                    e
                )
            })
    }

    fn get_compression_mode1_for_wrapper_setup_data(
        &self,
    ) -> anyhow::Result<&CompressionSetupData<CompressionMode1ForWrapper>> {
        self.setup_data_cache_proof_compressor
            .plonk_setup_data
            .compression_mode1_for_wrapper_setup_data
            .get_or_init(|| {
                self.load_compression_setup_data::<CompressionMode1ForWrapper>()
                    .context("Failed to load compression mode 1 for wrapper setup data")
            })
            .as_ref()
            .map_err(|e| {
                anyhow::anyhow!(
                    "Error loading compression mode 1 for wrapper setup data: {}",
                    e
                )
            })
    }

    fn get_plonk_snark_wrapper_setup_data(
        &self,
    ) -> anyhow::Result<SnarkWrapperSetupData<PlonkSnarkWrapper>> {
        self.load_snark_wrapper_setup_data::<PlonkSnarkWrapper>()
            .context("Failed to load Plonk Snark Wrapper setup data")
    }

    fn get_fflonk_snark_wrapper_setup_data(
        &self,
    ) -> anyhow::Result<&SnarkWrapperSetupData<FflonkSnarkWrapper>> {
        self.setup_data_cache_proof_compressor
            .fflonk_setup_data
            .fflonk_snark_wrapper_setup_data
            .get_or_init(|| {
                self.load_snark_wrapper_setup_data::<FflonkSnarkWrapper>()
                    .context("Failed to load Fflonk Snark Wrapper setup data")
            })
            .as_ref()
            .map_err(|e| anyhow::anyhow!("Error loading Fflonk Snark Wrapper setup data: {}", e))
    }
}

impl CompressorBlobStorageExt for Keystore {
    fn get_compression_mode1_previous_vk(
        &self,
    ) -> anyhow::Result<
        VerificationKey<
            GoldilocksField,
            <CompressionMode1 as CompressionStep>::PreviousStepTreeHasher,
        >,
    > {
        self.load_compression_previous_vk::<CompressionMode1>()
    }

    fn get_compression_mode2_previous_vk(
        &self,
    ) -> anyhow::Result<
        VerificationKey<
            GoldilocksField,
            <CompressionMode2 as CompressionStep>::PreviousStepTreeHasher,
        >,
    > {
        self.load_compression_previous_vk::<CompressionMode2>()
    }

    fn get_compression_mode3_previous_vk(
        &self,
    ) -> anyhow::Result<
        VerificationKey<
            GoldilocksField,
            <CompressionMode3 as CompressionStep>::PreviousStepTreeHasher,
        >,
    > {
        self.load_compression_previous_vk::<CompressionMode3>()
    }

    fn get_compression_mode4_previous_vk(
        &self,
    ) -> anyhow::Result<
        VerificationKey<
            GoldilocksField,
            <CompressionMode4 as CompressionStep>::PreviousStepTreeHasher,
        >,
    > {
        self.load_compression_previous_vk::<CompressionMode4>()
    }

    fn get_compression_mode5_for_wrapper_previous_vk(
        &self,
    ) -> anyhow::Result<
        VerificationKey<
            GoldilocksField,
            <CompressionMode5ForWrapper as CompressionStep>::PreviousStepTreeHasher,
        >,
    > {
        self.load_compression_previous_vk::<CompressionMode5ForWrapper>()
    }

    fn get_compression_mode1_for_wrapper_previous_vk(
        &self,
    ) -> anyhow::Result<
        VerificationKey<
            GoldilocksField,
            <CompressionMode1ForWrapper as CompressionStep>::PreviousStepTreeHasher,
        >,
    > {
        self.load_compression_previous_vk::<CompressionMode1ForWrapper>()
    }

    fn get_plonk_snark_wrapper_previous_vk_finalization_hint_and_crs(
        &self,
    ) -> anyhow::Result<(
        VerificationKey<
            GoldilocksField,
            <PlonkSnarkWrapper as SnarkWrapperStep>::PreviousStepTreeHasher,
        >,
        <PlonkSnarkWrapper as ProofSystemDefinition>::FinalizationHint,
        <PlonkSnarkWrapper as SnarkWrapperProofSystem>::CRS,
    )> {
        let vk = self.load_snark_wrapper_previous_vk::<PlonkSnarkWrapper>()?;
        let finalization_hint = self.load_snark_wrapper_finalization_hint::<PlonkSnarkWrapper>()?;
        let crs = self.load_snark_wrapper_crs::<PlonkSnarkWrapper>()?;
        Ok((vk, finalization_hint, crs))
    }

    fn get_fflonk_snark_wrapper_previous_vk_finalization_hint_and_crs(
        &self,
    ) -> anyhow::Result<(
        VerificationKey<
            GoldilocksField,
            <FflonkSnarkWrapper as SnarkWrapperStep>::PreviousStepTreeHasher,
        >,
        <FflonkSnarkWrapper as ProofSystemDefinition>::FinalizationHint,
        <FflonkSnarkWrapper as SnarkWrapperProofSystem>::CRS,
    )> {
        let vk = self.load_snark_wrapper_previous_vk::<FflonkSnarkWrapper>()?;
        let finalization_hint =
            self.load_snark_wrapper_finalization_hint::<FflonkSnarkWrapper>()?;
        let crs = self.load_snark_wrapper_crs::<FflonkSnarkWrapper>()?;
        Ok((vk, finalization_hint, crs))
    }

    fn set_compression_mode1_setup_data(
        &self,
        precomputation: &<CompressionMode1 as ProofSystemDefinition>::Precomputation,
        vk: &<CompressionMode1 as ProofSystemDefinition>::VK,
        finalization_hint: &<CompressionMode1 as ProofSystemDefinition>::FinalizationHint,
    ) -> anyhow::Result<()> {
        self.store_compression_setup_data::<CompressionMode1>(precomputation, vk, finalization_hint)
    }

    fn set_compression_mode2_setup_data(
        &self,
        precomputation: &<CompressionMode2 as ProofSystemDefinition>::Precomputation,
        vk: &<CompressionMode2 as ProofSystemDefinition>::VK,
        finalization_hint: &<CompressionMode2 as ProofSystemDefinition>::FinalizationHint,
    ) -> anyhow::Result<()> {
        self.store_compression_setup_data::<CompressionMode2>(precomputation, vk, finalization_hint)
    }

    fn set_compression_mode3_setup_data(
        &self,
        precomputation: &<CompressionMode3 as ProofSystemDefinition>::Precomputation,
        vk: &<CompressionMode3 as ProofSystemDefinition>::VK,
        finalization_hint: &<CompressionMode3 as ProofSystemDefinition>::FinalizationHint,
    ) -> anyhow::Result<()> {
        self.store_compression_setup_data::<CompressionMode3>(precomputation, vk, finalization_hint)
    }

    fn set_compression_mode4_setup_data(
        &self,
        precomputation: &<CompressionMode4 as ProofSystemDefinition>::Precomputation,
        vk: &<CompressionMode4 as ProofSystemDefinition>::VK,
        finalization_hint: &<CompressionMode4 as ProofSystemDefinition>::FinalizationHint,
    ) -> anyhow::Result<()> {
        self.store_compression_setup_data::<CompressionMode4>(precomputation, vk, finalization_hint)
    }

    fn set_compression_mode5_for_wrapper_setup_data(
        &self,
        precomputation: &<CompressionMode5ForWrapper as ProofSystemDefinition>::Precomputation,
        vk: &<CompressionMode5ForWrapper as ProofSystemDefinition>::VK,
        finalization_hint: &<CompressionMode5ForWrapper as ProofSystemDefinition>::FinalizationHint,
    ) -> anyhow::Result<()> {
        self.store_compression_setup_data::<CompressionMode5ForWrapper>(
            precomputation,
            vk,
            finalization_hint,
        )
    }

    fn set_compression_mode1_for_wrapper_setup_data(
        &self,
        precomputation: &<CompressionMode1ForWrapper as ProofSystemDefinition>::Precomputation,
        vk: &<CompressionMode1ForWrapper as ProofSystemDefinition>::VK,
        finalization_hint: &<CompressionMode1ForWrapper as ProofSystemDefinition>::FinalizationHint,
    ) -> anyhow::Result<()> {
        self.store_compression_setup_data::<CompressionMode1ForWrapper>(
            precomputation,
            vk,
            finalization_hint,
        )
    }

    fn set_plonk_snark_wrapper_setup_data(
        &self,
        precomputation: &<PlonkSnarkWrapper as ProofSystemDefinition>::Precomputation,
        vk: &<PlonkSnarkWrapper as ProofSystemDefinition>::VK,
    ) -> anyhow::Result<()> {
        self.store_snark_wrapper_setup_data::<PlonkSnarkWrapper>(precomputation, vk)
    }

    fn set_fflonk_snark_wrapper_setup_data(
        &self,
        precomputation: &<FflonkSnarkWrapper as ProofSystemDefinition>::Precomputation,
        vk: &<FflonkSnarkWrapper as ProofSystemDefinition>::VK,
    ) -> anyhow::Result<()> {
        self.store_snark_wrapper_setup_data::<FflonkSnarkWrapper>(precomputation, vk)
    }
}
