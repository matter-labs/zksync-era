
#[test]
fn test_consensus_registry_abi() {
    let c = crate::consensus_l2_contracts::ConsensusRegistry::load();
    c.get_attester_committee().test().unwrap();
    c.add().test().unwrap();
}
