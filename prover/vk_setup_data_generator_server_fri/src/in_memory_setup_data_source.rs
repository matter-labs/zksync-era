use std::collections::HashMap;
use std::io::{Error, ErrorKind};
use zkevm_test_harness::data_source::{BlockDataSource, SetupDataSource, SourceResult};
use zksync_prover_fri_types::circuit_definitions::circuit_definitions::base_layer::{
    ZkSyncBaseLayerFinalizationHint, ZkSyncBaseLayerProof, ZkSyncBaseLayerVerificationKey,
};
use zksync_prover_fri_types::circuit_definitions::circuit_definitions::recursion_layer::{
    ZkSyncRecursionLayerFinalizationHint, ZkSyncRecursionLayerProof,
    ZkSyncRecursionLayerVerificationKey,
};

pub struct InMemoryDataSource {
    ///data structures required for holding [`SetupDataSource`] result
    base_layer_vk: HashMap<u8, ZkSyncBaseLayerVerificationKey>,
    base_layer_padding_proof: HashMap<u8, ZkSyncBaseLayerProof>,
    base_layer_finalization_hint: HashMap<u8, ZkSyncBaseLayerFinalizationHint>,
    recursion_layer_vk: HashMap<u8, ZkSyncRecursionLayerVerificationKey>,
    recursion_layer_node_vk: Option<ZkSyncRecursionLayerVerificationKey>,
    recursion_layer_padding_proof: HashMap<u8, ZkSyncRecursionLayerProof>,
    recursion_layer_finalization_hint: HashMap<u8, ZkSyncRecursionLayerFinalizationHint>,
    recursion_layer_leaf_padding_proof: Option<ZkSyncRecursionLayerProof>,
    recursion_layer_node_padding_proof: Option<ZkSyncRecursionLayerProof>,
    recursion_layer_node_finalization_hint: Option<ZkSyncRecursionLayerFinalizationHint>,

    ///data structures required for holding [`BlockDataSource`] result
    base_layer_proofs: HashMap<(u8, usize), ZkSyncBaseLayerProof>,
    leaf_layer_proofs: HashMap<(u8, usize), ZkSyncRecursionLayerProof>,
    node_layer_proofs: HashMap<(u8, usize, usize), ZkSyncRecursionLayerProof>,
    scheduler_proof: Option<ZkSyncRecursionLayerProof>,
}

impl InMemoryDataSource {
    pub fn new() -> Self {
        InMemoryDataSource {
            base_layer_vk: HashMap::new(),
            base_layer_padding_proof: HashMap::new(),
            base_layer_finalization_hint: HashMap::new(),
            recursion_layer_vk: HashMap::new(),
            recursion_layer_node_vk: None,
            recursion_layer_padding_proof: HashMap::new(),
            recursion_layer_finalization_hint: HashMap::new(),
            recursion_layer_leaf_padding_proof: None,
            recursion_layer_node_padding_proof: None,
            recursion_layer_node_finalization_hint: None,
            base_layer_proofs: HashMap::new(),
            leaf_layer_proofs: HashMap::new(),
            node_layer_proofs: HashMap::new(),
            scheduler_proof: None,
        }
    }
}

impl SetupDataSource for InMemoryDataSource {
    fn get_base_layer_vk(&self, circuit_type: u8) -> SourceResult<ZkSyncBaseLayerVerificationKey> {
        self.base_layer_vk
            .get(&circuit_type)
            .cloned()
            .ok_or(Box::new(Error::new(
                ErrorKind::Other,
                format!("no data for circuit type {}", circuit_type),
            )))
    }

    fn get_base_layer_padding_proof(&self, circuit_type: u8) -> SourceResult<ZkSyncBaseLayerProof> {
        self.base_layer_padding_proof
            .get(&circuit_type)
            .cloned()
            .ok_or(Box::new(Error::new(
                ErrorKind::Other,
                format!("no data for circuit type {}", circuit_type),
            )))
    }

    fn get_base_layer_finalization_hint(
        &self,
        circuit_type: u8,
    ) -> SourceResult<ZkSyncBaseLayerFinalizationHint> {
        self.base_layer_finalization_hint
            .get(&circuit_type)
            .cloned()
            .ok_or(Box::new(Error::new(
                ErrorKind::Other,
                format!("no data for circuit type {}", circuit_type),
            )))
    }

    fn get_recursion_layer_vk(
        &self,
        circuit_type: u8,
    ) -> SourceResult<ZkSyncRecursionLayerVerificationKey> {
        self.recursion_layer_vk
            .get(&circuit_type)
            .cloned()
            .ok_or(Box::new(Error::new(
                ErrorKind::Other,
                format!("no data for circuit type {}", circuit_type),
            )))
    }

    fn get_recursion_layer_node_vk(&self) -> SourceResult<ZkSyncRecursionLayerVerificationKey> {
        Ok(self.recursion_layer_node_vk.clone().unwrap())
    }

    fn get_recursion_layer_padding_proof(
        &self,
        circuit_type: u8,
    ) -> SourceResult<ZkSyncRecursionLayerProof> {
        self.recursion_layer_padding_proof
            .get(&circuit_type)
            .cloned()
            .ok_or(Box::new(Error::new(
                ErrorKind::Other,
                format!("no data for circuit type {}", circuit_type),
            )))
    }

    fn get_recursion_layer_finalization_hint(
        &self,
        circuit_type: u8,
    ) -> SourceResult<ZkSyncRecursionLayerFinalizationHint> {
        self.recursion_layer_finalization_hint
            .get(&circuit_type)
            .cloned()
            .ok_or(Box::new(Error::new(
                ErrorKind::Other,
                format!("no data for circuit type {}", circuit_type),
            )))
    }

    fn get_recursion_layer_leaf_padding_proof(&self) -> SourceResult<ZkSyncRecursionLayerProof> {
        Ok(self.recursion_layer_leaf_padding_proof.clone().unwrap())
    }

    fn get_recursion_layer_node_padding_proof(&self) -> SourceResult<ZkSyncRecursionLayerProof> {
        Ok(self.recursion_layer_node_padding_proof.clone().unwrap())
    }

    fn get_recursion_layer_node_finalization_hint(
        &self,
    ) -> SourceResult<ZkSyncRecursionLayerFinalizationHint> {
        Ok(self.recursion_layer_node_finalization_hint.clone().unwrap())
    }

    fn set_base_layer_vk(&mut self, vk: ZkSyncBaseLayerVerificationKey) -> SourceResult<()> {
        self.base_layer_vk.insert(vk.numeric_circuit_type(), vk);
        Ok(())
    }

    fn set_base_layer_padding_proof(&mut self, proof: ZkSyncBaseLayerProof) -> SourceResult<()> {
        self.base_layer_padding_proof
            .insert(proof.numeric_circuit_type(), proof);
        Ok(())
    }

    fn set_base_layer_finalization_hint(
        &mut self,
        hint: ZkSyncBaseLayerFinalizationHint,
    ) -> SourceResult<()> {
        self.base_layer_finalization_hint
            .insert(hint.numeric_circuit_type(), hint);
        Ok(())
    }

    fn set_recursion_layer_vk(
        &mut self,
        vk: ZkSyncRecursionLayerVerificationKey,
    ) -> SourceResult<()> {
        self.recursion_layer_vk
            .insert(vk.numeric_circuit_type(), vk);
        Ok(())
    }

    fn set_recursion_layer_node_vk(
        &mut self,
        vk: ZkSyncRecursionLayerVerificationKey,
    ) -> SourceResult<()> {
        self.recursion_layer_node_vk = Some(vk);
        Ok(())
    }

    fn set_recursion_layer_padding_proof(
        &mut self,
        proof: ZkSyncRecursionLayerProof,
    ) -> SourceResult<()> {
        self.recursion_layer_padding_proof
            .insert(proof.numeric_circuit_type(), proof);
        Ok(())
    }

    fn set_recursion_layer_finalization_hint(
        &mut self,
        hint: ZkSyncRecursionLayerFinalizationHint,
    ) -> SourceResult<()> {
        self.recursion_layer_finalization_hint
            .insert(hint.numeric_circuit_type(), hint);
        Ok(())
    }

    fn set_recursion_layer_leaf_padding_proof(
        &mut self,
        proof: ZkSyncRecursionLayerProof,
    ) -> SourceResult<()> {
        self.recursion_layer_leaf_padding_proof = Some(proof);
        Ok(())
    }

    fn set_recursion_layer_node_padding_proof(
        &mut self,
        proof: ZkSyncRecursionLayerProof,
    ) -> SourceResult<()> {
        self.recursion_layer_node_padding_proof = Some(proof);
        Ok(())
    }

    fn set_recursion_layer_node_finalization_hint(
        &mut self,
        hint: ZkSyncRecursionLayerFinalizationHint,
    ) -> SourceResult<()> {
        self.recursion_layer_node_finalization_hint = Some(hint);
        Ok(())
    }
}

impl BlockDataSource for InMemoryDataSource {
    fn get_base_layer_proof(
        &self,
        circuit_type: u8,
        index: usize,
    ) -> SourceResult<ZkSyncBaseLayerProof> {
        self.base_layer_proofs
            .get(&(circuit_type, index))
            .cloned()
            .ok_or(Box::new(Error::new(
                ErrorKind::Other,
                format!(
                    "no base layer proof for circuit type {} index {}",
                    circuit_type, index
                ),
            )))
    }

    fn get_leaf_layer_proof(
        &self,
        circuit_type: u8,
        index: usize,
    ) -> SourceResult<ZkSyncRecursionLayerProof> {
        self.leaf_layer_proofs
            .get(&(circuit_type, index))
            .cloned()
            .ok_or(Box::new(Error::new(
                ErrorKind::Other,
                format!(
                    "no leaf layer proof for circuit type {} index {}",
                    circuit_type, index
                ),
            )))
    }

    fn get_node_layer_proof(
        &self,
        circuit_type: u8,
        step: usize,
        index: usize,
    ) -> SourceResult<ZkSyncRecursionLayerProof> {
        self.node_layer_proofs
            .get(&(circuit_type, step, index))
            .cloned()
            .ok_or(Box::new(Error::new(
                ErrorKind::Other,
                format!(
                    "no node layer proof for circuit type {} index {} step {}",
                    circuit_type, index, step
                ),
            )))
    }

    fn get_scheduler_proof(&self) -> SourceResult<ZkSyncRecursionLayerProof> {
        self.scheduler_proof.clone().ok_or(Box::new(Error::new(
            ErrorKind::Other,
            format!("no scheduler proof"),
        )))
    }

    fn set_base_layer_proof(
        &mut self,
        index: usize,
        proof: ZkSyncBaseLayerProof,
    ) -> SourceResult<()> {
        let circuit_type = proof.numeric_circuit_type();
        self.base_layer_proofs.insert((circuit_type, index), proof);
        Ok(())
    }

    fn set_leaf_layer_proof(
        &mut self,
        index: usize,
        proof: ZkSyncRecursionLayerProof,
    ) -> SourceResult<()> {
        let circuit_type = proof.numeric_circuit_type();
        self.leaf_layer_proofs.insert((circuit_type, index), proof);
        Ok(())
    }

    fn set_node_layer_proof(
        &mut self,
        circuit_type: u8,
        step: usize,
        index: usize,
        proof: ZkSyncRecursionLayerProof,
    ) -> SourceResult<()> {
        self.node_layer_proofs
            .insert((circuit_type, step, index), proof);
        Ok(())
    }

    fn set_scheduler_proof(&mut self, proof: ZkSyncRecursionLayerProof) -> SourceResult<()> {
        self.scheduler_proof = Some(proof);
        Ok(())
    }
}
