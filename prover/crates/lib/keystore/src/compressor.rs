use std::{
    fs::File,
    io::{Read, Write},
};

use circuit_definitions::circuit_definitions::recursion_layer::ZkSyncRecursionLayerStorageType;
use zksync_prover_fri_types::ProverServiceDataKey;

use crate::keystore::{Keystore, ProverServiceDataType};

const COMPACT_CRS_ENV_VAR: &str = "COMPACT_CRS_FILE";

impl proof_compression_gpu::BlobStorage for Keystore {
    fn read_scheduler_vk(&self) -> Box<dyn Read> {
        let filepath = self.get_file_path(
            ProverServiceDataKey::new_recursive(
                ZkSyncRecursionLayerStorageType::SchedulerCircuit as u8,
            ),
            ProverServiceDataType::VerificationKey,
        );
        tracing::info!("Reading filepath {:?}", filepath);
        Box::new(File::open(filepath).unwrap())
    }

    fn read_compression_layer_finalization_hint(&self, circuit_id: u8) -> Box<dyn Read> {
        let filepath = self.get_file_path(
            ProverServiceDataKey::new_compression(circuit_id),
            ProverServiceDataType::FinalizationHints,
        );
        tracing::info!("Reading filepath {:?}", filepath);
        Box::new(File::open(filepath).unwrap())
    }

    fn read_compression_layer_vk(&self, circuit_id: u8) -> Box<dyn Read> {
        let filepath = self.get_file_path(
            ProverServiceDataKey::new_compression(circuit_id),
            ProverServiceDataType::VerificationKey,
        );
        tracing::info!("Reading filepath {:?}", filepath);
        Box::new(File::open(filepath).unwrap())
    }

    fn read_compression_layer_precomputation(&self, circuit_id: u8) -> Box<dyn Read + Send + Sync> {
        let filepath = self.get_file_path(
            ProverServiceDataKey::new_compression(circuit_id),
            ProverServiceDataType::SetupData,
        );
        tracing::info!("Reading filepath {:?}", filepath);
        Box::new(File::open(filepath).unwrap())
    }

    fn read_compression_wrapper_finalization_hint(&self, circuit_id: u8) -> Box<dyn Read> {
        let filepath = self.get_file_path(
            ProverServiceDataKey::new_compression_wrapper(circuit_id),
            ProverServiceDataType::FinalizationHints,
        );
        tracing::info!("Reading filepath {:?}", filepath);
        Box::new(File::open(filepath).unwrap())
    }

    fn read_compression_wrapper_vk(&self, circuit_id: u8) -> Box<dyn Read> {
        let filepath = self.get_file_path(
            ProverServiceDataKey::new_compression_wrapper(circuit_id),
            ProverServiceDataType::VerificationKey,
        );
        tracing::info!("Reading filepath {:?}", filepath);
        Box::new(File::open(filepath).unwrap())
    }

    fn read_compression_wrapper_precomputation(
        &self,
        circuit_id: u8,
    ) -> Box<dyn Read + Send + Sync> {
        let filepath = self.get_file_path(
            ProverServiceDataKey::new_compression_wrapper(circuit_id),
            ProverServiceDataType::SetupData,
        );
        tracing::info!("Reading filepath {:?}", filepath);
        Box::new(File::open(filepath).unwrap())
    }

    fn read_fflonk_vk(&self) -> Box<dyn Read> {
        let filepath = self.get_file_path(
            ProverServiceDataKey::snark(),
            ProverServiceDataType::FflonkSnarkVerificationKey,
        );
        tracing::info!("Reading filepath {:?}", filepath);
        Box::new(File::open(filepath).unwrap())
    }

    fn read_fflonk_precomputation(&self) -> Box<dyn Read + Send + Sync> {
        let filepath = self.get_file_path(
            ProverServiceDataKey::snark(),
            ProverServiceDataType::FflonkSetupData,
        );
        tracing::info!("Reading filepath {:?}", filepath);
        Box::new(File::open(filepath).unwrap())
    }

    fn read_plonk_vk(&self) -> Box<dyn Read> {
        let filepath = self.get_file_path(
            ProverServiceDataKey::snark(),
            ProverServiceDataType::SnarkVerificationKey,
        );
        tracing::info!("Reading filepath {:?}", filepath);
        Box::new(File::open(filepath).unwrap())
    }

    fn read_plonk_precomputation(&self) -> Box<dyn Read + Send + Sync> {
        let filepath = self.get_file_path(
            ProverServiceDataKey::snark(),
            ProverServiceDataType::PlonkSetupData,
        );
        tracing::info!("Reading filepath {:?}", filepath);
        Box::new(File::open(filepath).unwrap())
    }

    fn read_compact_raw_crs(&self) -> Box<dyn Read + Send + Sync> {
        let filepath =
            std::env::var(COMPACT_CRS_ENV_VAR).expect("No compact CRS file path provided");
        tracing::info!("Reading filepath {:?}", filepath);
        Box::new(File::open(filepath).unwrap())
    }
}

impl proof_compression_gpu::BlobStorageExt for Keystore {
    fn write_compression_layer_finalization_hint(&self, circuit_id: u8) -> Box<dyn Write> {
        let filepath = self.get_file_path(
            ProverServiceDataKey::new_compression(circuit_id),
            ProverServiceDataType::FinalizationHints,
        );
        tracing::info!("Writing filepath {:?}", filepath);
        Box::new(File::create(filepath).unwrap())
    }

    fn write_compression_layer_vk(&self, circuit_id: u8) -> Box<dyn Write> {
        let filepath = self.get_file_path(
            ProverServiceDataKey::new_compression(circuit_id),
            ProverServiceDataType::VerificationKey,
        );
        tracing::info!("Writing filepath {:?}", filepath);
        Box::new(File::create(filepath).unwrap())
    }

    fn write_compression_layer_precomputation(&self, circuit_id: u8) -> Box<dyn Write> {
        let filepath = self.get_file_path(
            ProverServiceDataKey::new_compression(circuit_id),
            ProverServiceDataType::SetupData,
        );
        tracing::info!("Writing filepath {:?}", filepath);
        Box::new(File::create(filepath).unwrap())
    }

    fn write_compression_wrapper_finalization_hint(&self, circuit_id: u8) -> Box<dyn Write> {
        let filepath = self.get_file_path(
            ProverServiceDataKey::new_compression_wrapper(circuit_id),
            ProverServiceDataType::FinalizationHints,
        );
        tracing::info!("Writing filepath {:?}", filepath);
        Box::new(File::create(filepath).unwrap())
    }

    fn write_compression_wrapper_vk(&self, circuit_id: u8) -> Box<dyn Write> {
        let filepath = self.get_file_path(
            ProverServiceDataKey::new_compression_wrapper(circuit_id),
            ProverServiceDataType::VerificationKey,
        );
        tracing::info!("Writing filepath {:?}", filepath);
        Box::new(File::create(filepath).unwrap())
    }

    fn write_compression_wrapper_precomputation(&self, circuit_id: u8) -> Box<dyn Write> {
        let filepath = self.get_file_path(
            ProverServiceDataKey::new_compression_wrapper(circuit_id),
            ProverServiceDataType::SetupData,
        );
        tracing::info!("Writing filepath {:?}", filepath);
        Box::new(File::create(filepath).unwrap())
    }

    fn write_fflonk_vk(&self) -> Box<dyn Write> {
        let filepath = self.get_file_path(
            ProverServiceDataKey::snark(),
            ProverServiceDataType::FflonkSnarkVerificationKey,
        );
        tracing::info!("Writing filepath {:?}", filepath);
        Box::new(File::create(filepath).unwrap())
    }

    fn write_fflonk_precomputation(&self) -> Box<dyn Write> {
        let filepath = self.get_file_path(
            ProverServiceDataKey::snark(),
            ProverServiceDataType::FflonkSetupData,
        );
        tracing::info!("Writing filepath {:?}", filepath);
        Box::new(File::create(filepath).unwrap())
    }

    fn write_plonk_vk(&self) -> Box<dyn Write> {
        let filepath = self.get_file_path(
            ProverServiceDataKey::snark(),
            ProverServiceDataType::SnarkVerificationKey,
        );
        tracing::info!("Writing filepath {:?}", filepath);
        Box::new(File::create(filepath).unwrap())
    }

    fn write_plonk_precomputation(&self) -> Box<dyn Write> {
        let filepath = self.get_file_path(
            ProverServiceDataKey::snark(),
            ProverServiceDataType::PlonkSetupData,
        );
        tracing::info!("Writing filepath {:?}", filepath);
        Box::new(File::create(filepath).unwrap())
    }

    fn write_compact_raw_crs(&self) -> Box<dyn Write> {
        let filepath =
            std::env::var(COMPACT_CRS_ENV_VAR).expect("No compact CRS file path provided");
        tracing::info!("Writing filepath {:?}", filepath);
        Box::new(File::create(filepath).unwrap())
    }
}
