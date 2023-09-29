#[cfg(test)]
mod tests {
    use circuit_definitions::circuit_definitions::recursion_layer::ZkSyncRecursionLayerStorageType;
    use proptest::prelude::*;
    use zksync_types::proofs::AggregationRound;
    use zksync_vk_setup_data_server_fri::{
        get_base_layer_vk_for_circuit_type, get_base_path, get_base_vk_path, get_file_path,
        get_recursive_layer_vk_for_circuit_type, get_round_for_recursive_circuit_type,
        ProverServiceDataKey, ProverServiceDataType,
    };

    proptest! {
        #[test]
        fn test_get_base_layer_vk_for_circuit_type(circuit_id in 1u8..13) {
            let vk = get_base_layer_vk_for_circuit_type(circuit_id);
            assert_eq!(circuit_id, vk.numeric_circuit_type());
        }

        #[test]
        fn test_get_recursive_layer_vk_for_circuit_type(circuit_id in 1u8..15) {
            let vk = get_recursive_layer_vk_for_circuit_type(circuit_id);
            assert_eq!(circuit_id, vk.numeric_circuit_type());
        }
    }

    // Test get_base_path method
    #[test]
    fn test_get_base_path() {
        let base_path = get_base_path();
        assert!(!base_path.is_empty(), "Base path should not be empty");
    }

    // Test get_base_vk_path method
    #[test]
    fn test_get_base_vk_path() {
        let vk_path = get_base_vk_path();
        assert!(!vk_path.is_empty(), "Base VK path should not be empty");
    }

    // Test get_file_path method
    #[test]
    fn test_get_file_path() {
        let key = ProverServiceDataKey::new(1, AggregationRound::BasicCircuits);
        let file_path = get_file_path(key, ProverServiceDataType::VerificationKey);
        assert!(!file_path.is_empty(), "File path should not be empty");
    }

    // Test ProverServiceDataKey::new method
    #[test]
    fn test_proverservicedatakey_new() {
        let key = ProverServiceDataKey::new(1, AggregationRound::BasicCircuits);
        assert_eq!(
            key.circuit_id, 1,
            "Circuit id should be equal to the given value"
        );
        assert_eq!(
            key.round,
            AggregationRound::BasicCircuits,
            "Round should be equal to the given value"
        );
    }

    // Test get_round_for_recursive_circuit_type method
    #[test]
    fn test_get_round_for_recursive_circuit_type() {
        let round = get_round_for_recursive_circuit_type(
            ZkSyncRecursionLayerStorageType::SchedulerCircuit as u8,
        );
        assert_eq!(
            round,
            AggregationRound::Scheduler,
            "Round should be scheduler"
        );
    }
}
