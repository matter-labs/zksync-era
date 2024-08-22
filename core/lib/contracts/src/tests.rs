use crate::consensus_l2_contracts::{ConsensusRegistry};

#[test]
fn test_consensus_registry_abi() {
    let c = ConsensusRegistry::at(Default::default());
    c.get_attester_committee().test().unwrap();
    c.add(
        Default::default(),
        Default::default(),
        Default::default(),
        Default::default(),
        Default::default(),
        Default::default(),
    ).test().unwrap();
    c.initialize(
        Default::default(),
    ).test().unwrap();
}
