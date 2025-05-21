use std::{fs::File, io::Read};

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
    FflonkSetupData, PlonkSetupData, SnarkWrapperProofSystem, SnarkWrapperStep,
};
use zksync_prover_fri_types::ProverServiceDataKey;

use crate::keystore::{Keystore, ProverServiceDataType};

const COMPACT_CRS_ENV_VAR: &str = "COMPACT_CRS_FILE";

impl CompressorBlobStorage for Keystore {
    fn read_file_for_compression(
        &self,
        key: ProverServiceDataKey,
        service_data_type: ProverServiceDataType,
    ) -> Box<dyn Read> {
        let filepath = self.get_file_path(key, service_data_type);
        Box::new(File::open(filepath).unwrap())
    }
}

pub trait CompressorBlobStorage {
    fn read_file_for_compression(
        &self,
        key: ProverServiceDataKey,
        service_data_type: ProverServiceDataType,
    ) -> Box<dyn Read>;

    fn read_compact_raw_crs(&self) -> Box<dyn Read> {
        let filepath =
            std::env::var(COMPACT_CRS_ENV_VAR).expect("No compact CRS file path provided");
        Box::new(File::open(filepath).unwrap())
    }

    fn get_compression_vk<CS: CompressionStep>(&self) -> anyhow::Result<CS::VK> {
        let service_data_type = ProverServiceDataType::VerificationKey;
        let key = if CS::IS_WRAPPER {
            ProverServiceDataKey::new_compression_wrapper(CS::MODE)
        } else {
            ProverServiceDataKey::new_compression(CS::MODE)
        };
        let reader = self.read_file_for_compression(key, service_data_type);
        let vk = CS::load_this_vk(reader);
        Ok(vk)
    }
    fn get_compression_precomputation<CS: CompressionStep>(
        &self,
    ) -> anyhow::Result<CS::Precomputation> {
        let service_data_type = ProverServiceDataType::SetupData;
        let key = if CS::IS_WRAPPER {
            ProverServiceDataKey::new_compression_wrapper(CS::MODE)
        } else {
            ProverServiceDataKey::new_compression(CS::MODE)
        };
        let reader = self.read_file_for_compression(key, service_data_type);
        let precomputation = CS::get_precomputation(reader);
        Ok(precomputation)
    }
    fn get_compression_finalization_hint<CS: CompressionStep>(
        &self,
    ) -> anyhow::Result<CS::FinalizationHint> {
        let service_data_type = ProverServiceDataType::FinalizationHints;
        let key = if CS::IS_WRAPPER {
            ProverServiceDataKey::new_compression_wrapper(CS::MODE)
        } else {
            ProverServiceDataKey::new_compression(CS::MODE)
        };
        let reader = self.read_file_for_compression(key, service_data_type);
        let finalization_hint = CS::load_finalization_hint(reader);
        Ok(finalization_hint)
    }
    fn get_compression_previous_vk<CS: CompressionStep>(
        &self,
    ) -> anyhow::Result<VerificationKey<GoldilocksField, CS::PreviousStepTreeHasher>> {
        let service_data_type = ProverServiceDataType::VerificationKey;
        assert!(CS::MODE >= 1);
        let key = if CS::MODE == 1 {
            ProverServiceDataKey::new_recursive(
                ZkSyncRecursionLayerStorageType::SchedulerCircuit as u8,
            )
        } else {
            ProverServiceDataKey::new_compression(CS::MODE - 1)
        };
        let reader = self.read_file_for_compression(key, service_data_type);
        let previous_vk = CS::load_previous_vk(reader);
        Ok(previous_vk)
    }
    fn get_compression_setup_data<CS: CompressionStep>(
        &self,
    ) -> anyhow::Result<CompressionSetupData<CS>> {
        let vk = self.get_compression_vk::<CS>()?;
        let previous_vk = self.get_compression_previous_vk::<CS>()?;
        let precomputation = self.get_compression_precomputation::<CS>()?;
        let finalization_hint = self.get_compression_finalization_hint::<CS>()?;

        Ok(CompressionSetupData {
            vk,
            previous_vk,
            precomputation,
            finalization_hint,
        })
    }

    fn get_snark_wrapper_precomputation<WS: SnarkWrapperStep>(
        &self,
    ) -> anyhow::Result<WS::Precomputation> {
        let key = ProverServiceDataKey::snark();
        let service_data_type = if WS::IS_FFLONK {
            ProverServiceDataType::FflonkSetupData
        } else {
            ProverServiceDataType::PlonkSetupData
        };
        let reader = self.read_file_for_compression(key, service_data_type);
        let precomputation = WS::get_precomputation(reader);
        Ok(precomputation)
    }
    fn get_snark_wrapper_vk<WS: SnarkWrapperStep>(&self) -> anyhow::Result<WS::VK> {
        assert!(WS::IS_FFLONK ^ WS::IS_PLONK);
        let key = ProverServiceDataKey::snark();
        let service_data_type = if WS::IS_FFLONK {
            assert_eq!(WS::IS_PLONK, false);
            ProverServiceDataType::FflonkSnarkVerificationKey
        } else {
            assert_eq!(WS::IS_PLONK, true);
            ProverServiceDataType::SnarkVerificationKey
        };
        let reader = self.read_file_for_compression(key, service_data_type);
        let vk = WS::load_this_vk(reader);
        Ok(vk)
    }
    fn get_snark_wrapper_finalization_hint<WS: SnarkWrapperStep>(
        &self,
    ) -> anyhow::Result<WS::FinalizationHint> {
        let finalization_hint = WS::load_finalization_hint();
        Ok(finalization_hint)
    }
    fn get_snark_wrapper_ctx<WS: SnarkWrapperStep>(&self) -> anyhow::Result<WS::Context> {
        assert!(WS::IS_FFLONK ^ WS::IS_PLONK);
        let reader = self.read_compact_raw_crs();
        let crs = <WS as SnarkWrapperStep>::load_compact_raw_crs(reader);
        let ctx = <WS as SnarkWrapperProofSystem>::init_context(crs);
        Ok(ctx)
    }
    fn get_snark_wrapper_previous_vk<WS: SnarkWrapperStep>(
        &self,
    ) -> anyhow::Result<VerificationKey<GoldilocksField, WS::PreviousStepTreeHasher>> {
        assert!(WS::IS_FFLONK ^ WS::IS_PLONK);
        let previous_compression_mode = WS::PREVIOUS_COMPRESSION_MODE;
        let key = ProverServiceDataKey::new_compression_wrapper(previous_compression_mode);
        let service_data_type = ProverServiceDataType::VerificationKey;
        let reader = self.read_file_for_compression(key, service_data_type);
        let previous_vk = WS::load_previous_vk(reader);
        Ok(previous_vk)
    }
    fn get_snark_wrapper_setup_data<WS: SnarkWrapperStep>(
        &self,
    ) -> anyhow::Result<SnarkWrapperSetupData<WS>> {
        let vk = self.get_snark_wrapper_vk::<WS>()?;
        let previous_vk = self.get_snark_wrapper_previous_vk::<WS>()?;
        let precomputation = self.get_snark_wrapper_precomputation::<WS>()?;
        let ctx = self.get_snark_wrapper_ctx::<WS>()?;
        let finalization_hint = self.get_snark_wrapper_finalization_hint::<WS>()?;

        Ok(SnarkWrapperSetupData {
            vk,
            previous_vk,
            precomputation,
            finalization_hint,
            ctx: Some(ctx),
        })
    }

    fn get_full_fflonk_setup_data(&self) -> anyhow::Result<FflonkSetupData> {
        let compression_mode1_setup_data = self.get_compression_setup_data::<CompressionMode1>()?;
        let compression_mode2_setup_data = self.get_compression_setup_data::<CompressionMode2>()?;
        let compression_mode3_setup_data = self.get_compression_setup_data::<CompressionMode3>()?;
        let compression_mode4_setup_data = self.get_compression_setup_data::<CompressionMode4>()?;
        let compression_mode5_for_wrapper_setup_data =
            self.get_compression_setup_data::<CompressionMode5ForWrapper>()?;
        let fflonk_snark_wrapper_setup_data =
            self.get_snark_wrapper_setup_data::<FflonkSnarkWrapper>()?;

        Ok(FflonkSetupData {
            compression_mode1_setup_data,
            compression_mode2_setup_data,
            compression_mode3_setup_data,
            compression_mode4_setup_data,
            compression_mode5_for_wrapper_setup_data,
            fflonk_snark_wrapper_setup_data,
        })
    }

    fn get_full_plonk_setup_data(&self) -> anyhow::Result<PlonkSetupData> {
        let compression_mode1_for_wrapper_setup_data =
            self.get_compression_setup_data::<CompressionMode1ForWrapper>()?;
        let plonk_snark_wrapper_setup_data =
            self.get_snark_wrapper_setup_data::<PlonkSnarkWrapper>()?;

        Ok(PlonkSetupData {
            compression_mode1_for_wrapper_setup_data,
            plonk_snark_wrapper_setup_data,
        })
    }
}

// const COMPACT_CRS_ENV_VAR: &str = "COMPACT_CRS_FILE";

// impl proof_compression_gpu::BlobStorage for Keystore {
//     fn read_scheduler_vk(&self) -> Box<dyn Read> {
//         let filepath = self.get_file_path(
//             ProverServiceDataKey::new_recursive(
//                 ZkSyncRecursionLayerStorageType::SchedulerCircuit as u8,
//             ),
//             ProverServiceDataType::VerificationKey,
//         );

//         Box::new(File::open(filepath).unwrap())
//     }

//     fn read_compression_layer_finalization_hint(&self, circuit_id: u8) -> Box<dyn Read> {
//         let filepath = self.get_file_path(
//             ProverServiceDataKey::new_compression(circuit_id),
//             ProverServiceDataType::FinalizationHints,
//         );

//         Box::new(File::open(filepath).unwrap())
//     }

//     fn read_compression_layer_vk(&self, circuit_id: u8) -> Box<dyn Read> {
//         let filepath = self.get_file_path(
//             ProverServiceDataKey::new_compression(circuit_id),
//             ProverServiceDataType::VerificationKey,
//         );

//         Box::new(File::open(filepath).unwrap())
//     }

//     fn read_compression_layer_precomputation(&self, circuit_id: u8) -> Box<dyn Read + Send + Sync> {
//         let filepath = self.get_file_path(
//             ProverServiceDataKey::new_compression(circuit_id),
//             ProverServiceDataType::SetupData,
//         );

//         Box::new(File::open(filepath).unwrap())
//     }

//     fn read_compression_wrapper_finalization_hint(&self, circuit_id: u8) -> Box<dyn Read> {
//         let filepath = self.get_file_path(
//             ProverServiceDataKey::new_compression_wrapper(circuit_id),
//             ProverServiceDataType::FinalizationHints,
//         );

//         Box::new(File::open(filepath).unwrap())
//     }

//     fn read_compression_wrapper_vk(&self, circuit_id: u8) -> Box<dyn Read> {
//         let filepath = self.get_file_path(
//             ProverServiceDataKey::new_compression_wrapper(circuit_id),
//             ProverServiceDataType::VerificationKey,
//         );

//         Box::new(File::open(filepath).unwrap())
//     }

//     fn read_compression_wrapper_precomputation(
//         &self,
//         circuit_id: u8,
//     ) -> Box<dyn Read + Send + Sync> {
//         let filepath = self.get_file_path(
//             ProverServiceDataKey::new_compression_wrapper(circuit_id),
//             ProverServiceDataType::SetupData,
//         );

//         Box::new(File::open(filepath).unwrap())
//     }

//     fn read_fflonk_vk(&self) -> Box<dyn Read> {
//         let filepath = self.get_file_path(
//             ProverServiceDataKey::snark(),
//             ProverServiceDataType::FflonkSnarkVerificationKey,
//         );

//         Box::new(File::open(filepath).unwrap())
//     }

//     fn read_fflonk_precomputation(&self) -> Box<dyn Read + Send + Sync> {
//         let filepath = self.get_file_path(
//             ProverServiceDataKey::snark(),
//             ProverServiceDataType::FflonkSetupData,
//         );

//         Box::new(File::open(filepath).unwrap())
//     }

//     fn read_plonk_vk(&self) -> Box<dyn Read> {
//         let filepath = self.get_file_path(
//             ProverServiceDataKey::snark(),
//             ProverServiceDataType::SnarkVerificationKey,
//         );

//         Box::new(File::open(filepath).unwrap())
//     }

//     fn read_plonk_precomputation(&self) -> Box<dyn Read + Send + Sync> {
//         let filepath = self.get_file_path(
//             ProverServiceDataKey::snark(),
//             ProverServiceDataType::PlonkSetupData,
//         );

//         Box::new(File::open(filepath).unwrap())
//     }

//     fn read_compact_raw_crs(&self) -> Box<dyn Read + Send + Sync> {
//         let filepath =
//             std::env::var(COMPACT_CRS_ENV_VAR).expect("No compact CRS file path provided");
//         Box::new(File::open(filepath).unwrap())
//     }
// }

// impl proof_compression_gpu::BlobStorageExt for Keystore {
//     fn write_compression_layer_finalization_hint(&self, circuit_id: u8) -> Box<dyn Write> {
//         let filepath = self.get_file_path(
//             ProverServiceDataKey::new_compression(circuit_id),
//             ProverServiceDataType::FinalizationHints,
//         );

//         Box::new(File::create(filepath).unwrap())
//     }

//     fn write_compression_layer_vk(&self, circuit_id: u8) -> Box<dyn Write> {
//         let filepath = self.get_file_path(
//             ProverServiceDataKey::new_compression(circuit_id),
//             ProverServiceDataType::VerificationKey,
//         );

//         Box::new(File::create(filepath).unwrap())
//     }

//     fn write_compression_layer_precomputation(&self, circuit_id: u8) -> Box<dyn Write> {
//         let filepath = self.get_file_path(
//             ProverServiceDataKey::new_compression(circuit_id),
//             ProverServiceDataType::SetupData,
//         );

//         Box::new(File::create(filepath).unwrap())
//     }

//     fn write_compression_wrapper_finalization_hint(&self, circuit_id: u8) -> Box<dyn Write> {
//         let filepath = self.get_file_path(
//             ProverServiceDataKey::new_compression_wrapper(circuit_id),
//             ProverServiceDataType::FinalizationHints,
//         );

//         Box::new(File::create(filepath).unwrap())
//     }

//     fn write_compression_wrapper_vk(&self, circuit_id: u8) -> Box<dyn Write> {
//         let filepath = self.get_file_path(
//             ProverServiceDataKey::new_compression_wrapper(circuit_id),
//             ProverServiceDataType::VerificationKey,
//         );

//         Box::new(File::create(filepath).unwrap())
//     }

//     fn write_compression_wrapper_precomputation(&self, circuit_id: u8) -> Box<dyn Write> {
//         let filepath = self.get_file_path(
//             ProverServiceDataKey::new_compression_wrapper(circuit_id),
//             ProverServiceDataType::SetupData,
//         );

//         Box::new(File::create(filepath).unwrap())
//     }

//     fn write_fflonk_vk(&self) -> Box<dyn Write> {
//         let filepath = self.get_file_path(
//             ProverServiceDataKey::snark(),
//             ProverServiceDataType::FflonkSnarkVerificationKey,
//         );

//         Box::new(File::create(filepath).unwrap())
//     }

//     fn write_fflonk_precomputation(&self) -> Box<dyn Write> {
//         let filepath = self.get_file_path(
//             ProverServiceDataKey::snark(),
//             ProverServiceDataType::FflonkSetupData,
//         );

//         Box::new(File::create(filepath).unwrap())
//     }

//     fn write_plonk_vk(&self) -> Box<dyn Write> {
//         let filepath = self.get_file_path(
//             ProverServiceDataKey::snark(),
//             ProverServiceDataType::SnarkVerificationKey,
//         );

//         Box::new(File::create(filepath).unwrap())
//     }

//     fn write_plonk_precomputation(&self) -> Box<dyn Write> {
//         let filepath = self.get_file_path(
//             ProverServiceDataKey::snark(),
//             ProverServiceDataType::PlonkSetupData,
//         );

//         Box::new(File::create(filepath).unwrap())
//     }

//     fn write_compact_raw_crs(&self) -> Box<dyn Write> {
//         let filepath =
//             std::env::var(COMPACT_CRS_ENV_VAR).expect("No compact CRS file path provided");
//         Box::new(File::create(filepath).unwrap())
//     }
// }
