//! The production `vm_latest` decommitter must use the same freshness identity as the decommitter
//! used by basic-circuit witness generation.
//!
//! Both engines run the same `zk_evm` opcode implementation for `LOG.Decommit`, which refunds the
//! caller-supplied `extra_cost` iff the prepared query came back with `is_fresh == false`. Only the
//! `DecommittmentProcessor` answering `prepare_to_decommit` differs: [`DecommitterOracle`] in
//! production vs. [`SimpleDecommitter`] in witness generation, whose `history` is keyed by
//! `VersionedHashNormalizedPreimage`. Both must key by the header-stripped 28-byte preimage.

use std::{cell::RefCell, rc::Rc};

use zk_evm_1_5_2::{
    abstractions::DecommittmentProcessor,
    aux_structures::{DecommittmentQuery, MemoryPage, Timestamp},
    reference_impls::decommitter::SimpleDecommitter,
    zkevm_opcode_defs::{
        BlobSha256Format, ContractCodeSha256Format, VersionedHashLen32,
        ERGS_PER_CODE_WORD_DECOMMITTMENT,
    },
};
use zksync_types::{bytecode::BytecodeHash, h256_to_u256, H256, U256};

use crate::{
    interface::storage::InMemoryStorage,
    utils::bytecode::bytes_to_be_words,
    vm_latest::{old_vm::oracles::decommitter::DecommitterOracle, HistoryEnabled, SimpleMemory},
};

/// Word aligned with an odd word count, so the body is simultaneously a valid EraVM bytecode and an
/// EVM deployed bytecode that `pad_evm_bytecode` leaves untouched — hence both hash constructors
/// digest the very same bytes.
const WORDS: usize = 47;

fn alias_pair() -> (Vec<u8>, H256, H256) {
    let body = vec![0x5b_u8; WORDS * 32];
    let native = BytecodeHash::for_bytecode(&body).value();
    let evm = BytecodeHash::for_evm_bytecode(body.len(), &body).value();
    (body, native, evm)
}

fn query(hash: H256, page: u32, ts: u32) -> DecommittmentQuery {
    let (header, normalized_preimage) = match hash.0[0] {
        1 => ContractCodeSha256Format::normalize_for_decommitment(&hash.0),
        2 => BlobSha256Format::normalize_for_decommitment(&hash.0),
        _ => unreachable!("unsupported version"),
    };
    DecommittmentQuery {
        header,
        normalized_preimage,
        timestamp: Timestamp(ts),
        memory_page: MemoryPage(page),
        decommitted_length: 0,
        is_fresh: false,
    }
}

#[test]
fn cross_version_hashes_share_normalized_preimage() {
    let (body, native, evm) = alias_pair();
    assert_eq!(body.len(), 1504);
    assert_ne!(native, evm);
    assert_eq!(native.0[4..], evm.0[4..]);
    assert_eq!(native.0[..4], [1, 0, 0x00, 0x2f]); // version 1, length in *words*
    assert_eq!(evm.0[..4], [2, 0, 0x05, 0xe0]); // version 2, length in *bytes*

    assert!(ContractCodeSha256Format::is_valid(&native.0));
    assert!(BlobSha256Format::is_valid(&evm.0));
    assert_eq!(
        ContractCodeSha256Format::normalize_for_decommitment(&native.0).1,
        BlobSha256Format::normalize_for_decommitment(&evm.0).1,
    );

    // `CodeOracle.yul` charges `4 * padded_len_in_words` for both versions, i.e. 47 words here.
    assert_eq!(ERGS_PER_CODE_WORD_DECOMMITTMENT * WORDS as u32, 188);
}

/// The orders in which the two identifiers of one body can be decommitted. Native first is what an
/// `L1 -> L2` transaction's `factoryDeps` plus a `CodeOracle` call produce; EVM first is what
/// `EvmEmulator::fetchBytecode` produces on any call into an EVM contract.
fn alias_orders() -> [(H256, H256, &'static str); 2] {
    let (_, native, evm) = alias_pair();
    [
        (native, evm, "native -> evm"),
        (evm, native, "evm -> native"),
    ]
}

/// Replays "decommit `first`, then request `second`" through the production oracle, returning the
/// query of each request and the hashes reported as used contracts.
fn production_replay(
    first: H256,
    second: H256,
) -> (DecommittmentQuery, DecommittmentQuery, Vec<U256>) {
    let (body, native, evm) = alias_pair();
    let mut storage = InMemoryStorage::default();
    storage.store_factory_dep(native, body.clone());
    storage.store_factory_dep(evm, body);

    let mut oracle =
        DecommitterOracle::<false, _, HistoryEnabled>::new(Rc::new(RefCell::new(storage)));
    let mut memory = SimpleMemory::<HistoryEnabled>::default();

    let first_query = oracle
        .prepare_to_decommit(0, query(first, 1024, 1))
        .unwrap();
    assert!(first_query.is_fresh);
    oracle
        .decommit_into_memory(0, first_query, &mut memory)
        .unwrap();

    let second_query = oracle
        .prepare_to_decommit(1, query(second, 4096, 2))
        .unwrap();
    let used = oracle
        .decommitted_code_hashes
        .inner()
        .keys()
        .copied()
        .collect();
    (first_query, second_query, used)
}

/// The same replay through the decommitter used by basic-circuit witness generation.
fn proving_replay(first: H256, second: H256) -> (DecommittmentQuery, DecommittmentQuery) {
    let (body, native, evm) = alias_pair();
    let words = bytes_to_be_words(&body);

    let mut decommitter = SimpleDecommitter::<false>::new();
    // Both identifiers end up in `used_bytecodes` of the batch witness; `populate` deduplicates
    // them since zksync-protocol#214.
    decommitter.populate(vec![
        (h256_to_u256(native), words.clone()),
        (h256_to_u256(evm), words),
    ]);
    let mut memory = zk_evm_1_5_2::reference_impls::memory::SimpleMemory::new();

    let first_query = decommitter
        .prepare_to_decommit(0, query(first, 1024, 1))
        .unwrap();
    assert!(first_query.is_fresh);
    decommitter
        .decommit_into_memory(0, first_query, &mut memory)
        .unwrap();

    let second_query = decommitter
        .prepare_to_decommit(1, query(second, 4096, 2))
        .unwrap();
    (first_query, second_query)
}

/// The cross-version alias of an already-decommitted body must be a repeat sharing the original
/// memory page, so that `LOG.Decommit` refunds `extra_cost`.
#[test]
fn production_vm_latest_treats_cross_version_alias_as_repeat() {
    for (first, second, label) in alias_orders() {
        let (first_query, alias, used) = production_replay(first, second);
        assert!(
            !alias.is_fresh,
            "{label}: the alias of an already-decommitted body must not be charged for again"
        );
        assert_eq!(
            alias.memory_page, first_query.memory_page,
            "{label}: the alias must resolve to the page of the original decommit; two pages for \
             one normalized hash cannot be represented by the decommittments sorter"
        );
        assert_eq!(
            alias.decommitted_length, first_query.decommitted_length,
            "{label}"
        );
        // Both identifiers must still be reported as used contracts.
        assert!(
            used.contains(&h256_to_u256(first)) && used.contains(&h256_to_u256(second)),
            "{label}: {used:?}"
        );
    }
}

/// The pre-existing same-hash behavior must be untouched.
#[test]
fn production_vm_latest_treats_repeated_hash_as_repeat() {
    let (_, native, evm) = alias_pair();
    for hash in [native, evm] {
        let (first_query, repeat, _) = production_replay(hash, hash);
        assert!(!repeat.is_fresh, "{hash:?} is a repeat of itself");
        assert_eq!(repeat.memory_page, first_query.memory_page, "{hash:?}");
    }
}

/// The same, for the proving decommitter, which keys freshness by the normalized preimage.
#[test]
fn proving_decommitter_treats_cross_version_alias_as_repeat() {
    for (first, second, label) in alias_orders() {
        let (first_query, alias) = proving_replay(first, second);
        assert!(!alias.is_fresh, "{label}");
        assert_eq!(alias.memory_page, first_query.memory_page, "{label}");
    }
}

/// The two conclusions above side by side: identical opcode inputs, identical refunds.
#[test]
fn decommit_refund_matches_between_production_and_proving() {
    let extra_cost = ERGS_PER_CODE_WORD_DECOMMITTMENT * WORDS as u32;

    for (first, second, label) in alias_orders() {
        let (_, production, _) = production_replay(first, second);
        let (_, proving) = proving_replay(first, second);

        // `zk_evm::opcodes::execution::log`: `refund = if !is_fresh { extra_cost } else { 0 }`.
        let production_refund = if production.is_fresh { 0 } else { extra_cost };
        let proving_refund = if proving.is_fresh { 0 } else { extra_cost };
        assert_eq!(
            production_refund, proving_refund,
            "{label}: production and proving replay must refund the same amount"
        );
        assert_eq!(production_refund, extra_cost, "{label}");
    }
}
