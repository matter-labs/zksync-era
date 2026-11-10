//! Accounting for the factory-dependency preimages a transaction carries.
//!
//! They are metered as one 32-byte hash each, but transported, executed, block-accounted and
//! persisted at full size, so re-attaching an already-published preimage consumes payload capacity
//! nothing charged for. The accounting here is shared by the two boundaries that bound it: the API
//! server at admission, and the state keeper post-execution.

use std::collections::HashSet;

use zksync_basic_types::{bytecode::BytecodeHash, H256};

/// Fraction of an L2 block's payload budget that a single transaction's redundant factory-dependency
/// preimages are allowed to occupy.
const PAYLOAD_BUDGET_FRACTION_PER_TX: usize = 8;

/// Derives the per-transaction cap on redundant factory-dependency preimages from the configured
/// L2-block payload budget (`state_keeper.l2_block_max_payload_size`).
///
/// 125,000 bytes with the shipped budget. Gas metering is unchanged; this only caps how many
/// redundant bytes one transaction may carry, so a large enough redeployment can hit it too — see
/// [`tests::realistic_deployment_bundles_clear_the_bound()`] for the sizes this repo compiles.
pub const fn max_redundant_factory_deps_size(l2_block_max_payload_size: usize) -> usize {
    l2_block_max_payload_size / PAYLOAD_BUDGET_FRACTION_PER_TX
}

/// Size of the preimages this transaction pays no publication work for: already-known bytecode,
/// plus repeats of the same bytecode within the transaction.
///
/// `newly_published` is what it actually publishes — from `MarkedAsKnown` events after execution,
/// or the `KnownCodesStorage` markers before it.
pub fn redundant_factory_deps_size(
    factory_deps: &[Vec<u8>],
    mut newly_published: HashSet<H256>,
) -> usize {
    let mut redundant_bytes = 0;
    for dep in factory_deps {
        // Only the first published occurrence of a hash is backed by publication work; taking it
        // out of the set leaves any further copy counted as pure payload.
        let hash = BytecodeHash::for_bytecode(dep).value();
        if !newly_published.remove(&hash) {
            redundant_bytes += dep.len();
        }
    }
    redundant_bytes
}

#[cfg(test)]
mod tests {
    use std::slice;

    use super::*;

    /// Bytecode of `words` 32-byte words (must be odd, per EraVM). Only its hash and length matter.
    fn bytecode(words: usize) -> Vec<u8> {
        assert!(words % 2 == 1, "bytecode must have an odd number of words");
        let mut code = vec![0_u8; words * 32];
        // Make distinct sizes hash distinctly.
        code[0] = 1;
        code[1..9].copy_from_slice(&(words as u64).to_be_bytes());
        code
    }

    fn hash(bytecode: &[u8]) -> H256 {
        BytecodeHash::for_bytecode(bytecode).value()
    }

    #[test]
    fn bound_is_derived_from_payload_budget() {
        assert_eq!(max_redundant_factory_deps_size(1_000_000), 125_000);
    }

    /// A redeploy re-attaches the bytecode and its transitive deps, all already known. Sizes are the
    /// extremes over the EraVM artifacts this repo's test suite compiles; if the bound drops below
    /// them, ordinary deployments start getting rejected.
    #[test]
    fn realistic_deployment_bundles_clear_the_bound() {
        let bound = max_redundant_factory_deps_size(1_000_000);

        const LARGEST_ARTIFACT: usize = 51_680; // `CustomAccount`
        const LARGEST_BUNDLE: usize = 101_376; // `LegacySharedBridgeTest` + 5 deps

        assert!(LARGEST_ARTIFACT <= bound);
        assert!(LARGEST_BUNDLE <= bound);
    }

    #[test]
    fn empty_factory_deps_are_free() {
        assert_eq!(redundant_factory_deps_size(&[], HashSet::new()), 0);
    }

    /// The lever being closed: a ~1 MB already-known preimage is counted in full.
    #[test]
    fn already_known_dep_is_counted() {
        let dep = bytecode(31_239); // ~1 MB
        let redundant = redundant_factory_deps_size(slice::from_ref(&dep), HashSet::new());

        assert_eq!(redundant, dep.len());
        assert!(redundant > max_redundant_factory_deps_size(1_000_000));
    }

    /// Newly published bytecode pays proportional gas and is exempt.
    #[test]
    fn newly_published_dep_is_free() {
        let dep = bytecode(31_239);
        let published = HashSet::from([hash(&dep)]);

        assert_eq!(redundant_factory_deps_size(&[dep], published), 0);
    }

    /// The first publication is paid for; a second copy in the same transaction is not.
    #[test]
    fn duplicate_of_new_dep_counts_once() {
        let dep = bytecode(4_095); // ~128 KB
        let published = HashSet::from([hash(&dep)]);

        let redundant = redundant_factory_deps_size(&[dep.clone(), dep.clone()], published);
        assert_eq!(redundant, dep.len());
    }

    /// Re-attaching an ordinary known contract stays under the bound (redeploy flow unaffected).
    #[test]
    fn small_already_known_dep_is_under_the_bound() {
        let dep = bytecode(17); // 544 bytes
        let redundant = redundant_factory_deps_size(&[dep], HashSet::new());

        assert!(redundant <= max_redundant_factory_deps_size(1_000_000));
    }

    /// Distinct already-known preimages aggregate, so they cannot be split to dodge the cap.
    #[test]
    fn distinct_already_known_deps_aggregate() {
        let deps: Vec<Vec<u8>> = (0..31).map(|i| bytecode(999 + 2 * i)).collect();
        let total: usize = deps.iter().map(Vec::len).sum();

        let redundant = redundant_factory_deps_size(&deps, HashSet::new());
        assert_eq!(redundant, total);
        assert!(redundant > max_redundant_factory_deps_size(1_000_000));
    }

    /// A mixed transaction is charged only for the entries it does not publish.
    #[test]
    fn only_unpublished_entries_are_charged() {
        let published_dep = bytecode(999);
        let known_dep = bytecode(1_001);
        let published = HashSet::from([hash(&published_dep)]);

        let redundant = redundant_factory_deps_size(&[published_dep, known_dep.clone()], published);
        assert_eq!(redundant, known_dep.len());
    }
}
