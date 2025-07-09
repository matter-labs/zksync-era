/**
 * This suite contains tests checking interop functionality.
 */

import { TestMaster } from '../src';
import { Token } from '../src/types';

import * as zksync from 'zksync-ethers';
import * as ethers from 'ethers';
import { waitForL2ToL1LogProof } from '../src/helpers';
import { RetryableWallet } from '../src/retry-provider';

import {
    L2_MESSAGE_VERIFICATION_ADDRESS,
    L2_INTEROP_ROOT_STORAGE_ADDRESS,
    ArtifactL2MessageVerification,
    ArtifactL2InteropRootStorage,
    ArtifactBridgeHub,
    GATEWAY_CHAIN_ID
} from '../src/constants';
import { FinalizeWithdrawalParams } from 'zksync-ethers/build/types';
import { ETH_ADDRESS } from 'zksync-ethers/build/utils';

describe('Interop behavior checks', () => {
    let testMaster: TestMaster;
    let alice: RetryableWallet;
    let aliceSecondChain: RetryableWallet;
    let tokenDetails: Token;

    let skipInteropTest = false;

    beforeAll(async () => {
        testMaster = TestMaster.getInstance(__filename);
        alice = testMaster.mainAccount();

        tokenDetails = testMaster.environment().erc20Token;

        // Skip interop tests if the SL is the same as the L1.
        const bridgehub = new ethers.Contract(
            await alice.provider.getBridgehubContractAddress(),
            ArtifactBridgeHub.abi,
            alice.providerL1
        );

        if (
            (await bridgehub.settlementLayer((await alice.provider.getNetwork()).chainId)) ==
            (await alice.providerL1!.getNetwork()).chainId
        ) {
            skipInteropTest = true;
        } else {
            // Define the second chain wallet if the SL is different from the L1.
            const maybeAliceSecondChain = testMaster.mainAccountSecondChain();
            if (!!maybeAliceSecondChain) {
                throw new Error('Interop tests cannot be run if the second chain is not set');
            }
            aliceSecondChain = maybeAliceSecondChain!;
        }
    });

    beforeEach(() => {
        if (skipInteropTest) {
            pending('Skipping interop test because settlement layer is the same as L1');
        }
    });

    let withdrawalHash: string;
    let params: FinalizeWithdrawalParams;
    test('Can check withdrawal hash in L2-A', async () => {
        const l2MessageVerification = new zksync.Contract(
            L2_MESSAGE_VERIFICATION_ADDRESS,
            ArtifactL2MessageVerification.abi,
            alice.provider
        );

        // Perform a withdrawal and wait for it to be processed
        const withdrawalPromise = await alice.withdraw({
            token: tokenDetails.l2Address,
            amount: 1n
        });
        await expect(withdrawalPromise).toBeAccepted();
        const withdrawalTx = await withdrawalPromise;
        withdrawalHash = withdrawalTx.hash;
        const l2TxReceipt = await alice.provider.getTransactionReceipt(withdrawalHash);
        await waitForL2ToL1LogProof(alice, l2TxReceipt!.blockNumber, withdrawalHash);

        // Proof-based interop on Gateway, meaning the Merkle proof hashes to Gateway's MessageRoot
        params = await alice.getFinalizeWithdrawalParams(withdrawalHash, undefined, 'proof_based_gw');

        // Needed else the L2's view of GW's MessageRoot won't be updated
        await waitForInteropRootNonZero(alice.provider, alice, getGWBlockNumber(params), tokenDetails.l2Address);

        const included = await l2MessageVerification.proveL2MessageInclusionShared(
            (await alice.provider.getNetwork()).chainId,
            params.l1BatchNumber,
            params.l2MessageIndex,
            { txNumberInBatch: params.l2TxNumberInBlock, sender: params.sender, data: params.message },
            params.proof
        );
        expect(included).toBe(true);
    });

    test('Can check withdrawal hash from L2-B', async () => {
        const l2MessageVerification = new zksync.Contract(
            L2_MESSAGE_VERIFICATION_ADDRESS,
            ArtifactL2MessageVerification.abi,
            aliceSecondChain.provider
        );

        // Needed else the L2's view of GW's MessageRoot won't be updated
        await waitForInteropRootNonZero(aliceSecondChain.provider, aliceSecondChain, getGWBlockNumber(params));

        // We use the same proof that was verified in L2-A
        const included = await l2MessageVerification.proveL2MessageInclusionShared(
            (await alice.provider.getNetwork()).chainId,
            params.l1BatchNumber,
            params.l2MessageIndex,
            { txNumberInBatch: params.l2TxNumberInBlock, sender: params.sender, data: params.message },
            params.proof
        );
        expect(included).toBe(true);
    });

    function getGWBlockNumber(params: FinalizeWithdrawalParams): number {
        /// see hashProof in MessageHashing.sol for this logic.
        let gwProofIndex =
            1 + parseInt(params.proof[0].slice(4, 6), 16) + 1 + parseInt(params.proof[0].slice(6, 8), 16);
        return parseInt(params.proof[gwProofIndex].slice(2, 34), 16);
    }

    async function waitForInteropRootNonZero(
        provider: zksync.Provider,
        alice: zksync.Wallet,
        l1BatchNumber: number,
        tokenToSend: string = ETH_ADDRESS
    ) {
        const l2InteropRootStorage = new zksync.Contract(
            L2_INTEROP_ROOT_STORAGE_ADDRESS,
            ArtifactL2InteropRootStorage.abi,
            provider
        );
        let currentRoot = ethers.ZeroHash;

        while (currentRoot === ethers.ZeroHash) {
            // We make repeated transactions to force the L2 to update the interop root.
            const tx = await alice.transfer({
                to: alice.address,
                amount: 1,
                token: tokenToSend
            });
            await tx.wait();

            currentRoot = await l2InteropRootStorage.interopRoots(GATEWAY_CHAIN_ID, l1BatchNumber);
            await zksync.utils.sleep(alice.provider.pollingInterval);
        }
    }

    afterAll(async () => {
        await testMaster.deinitialize();
    });
});
