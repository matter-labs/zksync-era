/**
 * This suite contains tests checking Interop-A cross-chain message features,
 * those that were included as part of v29 protocol upgrade.
 */

import { TestMaster } from '../src';
import { Token } from '../src/types';
import * as utils from 'utils';
import { loadConfig, shouldLoadConfigFromFile } from 'utils/build/file-configs';

import * as zksync from 'zksync-ethers';
import * as ethers from 'ethers';

import { waitForL2ToL1LogProof } from '../src/helpers';
import { RetryableWallet } from '../src/retry-provider';

import { L2_MESSAGE_VERIFICATION_ADDRESS, ArtifactL2MessageVerification, ArtifactL1BridgeHub } from '../src/constants';
import { FinalizeWithdrawalParams } from 'zksync-ethers/build/types';
import { waitForInteropRootNonZero, getGWBlockNumber } from '../src/helpers';

describe('Interop-A behavior checks', () => {
    let testMaster: TestMaster;
    let alice: RetryableWallet;
    let aliceSecondChain: RetryableWallet;
    let tokenDetails: Token;

    let skipInteropTests = false;

    // L1 Variables
    let l1Provider: ethers.Provider;
    let veryRichWallet: zksync.Wallet;

    // Token details
    let tokenA: Token = {
        name: 'Token A',
        symbol: 'AA',
        decimals: 18n,
        l1Address: '',
        l2Address: '',
        l2AddressSecondChain: ''
    };

    // Interop1 (Main Chain) Variables
    let interop1Provider: zksync.Provider;
    let interop1Wallet: zksync.Wallet;
    let interop1RichWallet: zksync.Wallet;
    // kl todo remove very rich wallet. Useful for local debugging, calldata can be sent directly using cast.
    // let interop1VeryRichWallet: zksync.Wallet;
    let interop1InteropCenter: zksync.Contract;
    let interop2InteropHandler: zksync.Contract;
    let interop1NativeTokenVault: zksync.Contract;
    let interop1TokenA: zksync.Contract;

    // Interop2 (Second Chain) Variables
    let interop2RichWallet: zksync.Wallet;
    let interop2Provider: zksync.Provider;
    let interop2NativeTokenVault: zksync.Contract;
    let dummyInteropRecipient: string;

    // Gateway Variables
    // let gatewayProvider: zksync.Provider;
    // let gatewayWallet: zksync.Wallet;

    let isSameBaseToken: boolean;

    beforeAll(async () => {
        testMaster = TestMaster.getInstance(__filename);
        alice = testMaster.mainAccount();

        tokenDetails = testMaster.environment().erc20Token;

        const testWalletPK = testMaster.newEmptyAccount().privateKey;
        const mainAccount = testMaster.mainAccount();

        // Initialize providers
        l1Provider = mainAccount.providerL1!;
        interop1Provider = mainAccount.provider;
        // Setup wallets for Interop1
        veryRichWallet = new zksync.Wallet(richPk, interop1Provider, l1Provider);

        // Initialize Test Master and create wallets for Interop1
        interop1Wallet = new zksync.Wallet(testWalletPK, interop1Provider, l1Provider);
        interop1RichWallet = new zksync.Wallet(mainAccount.privateKey, interop1Provider, l1Provider);
        // interop1VeryRichWallet = new zksync.Wallet(richPk, interop1Provider, l1Provider);

        // Skip interop tests if the SL is the same as the L1.
        const bridgehub = new ethers.Contract(
            await alice.provider.getBridgehubContractAddress(),
            ArtifactL1BridgeHub.abi,
            alice.providerL1
        );

        if (
            (await bridgehub.settlementLayer((await alice.provider.getNetwork()).chainId)) ==
            (await alice.providerL1!.getNetwork()).chainId
        ) {
            skipInteropTests = true;
        } else {
            // Define the second chain wallet if the SL is different from the L1.
            const maybeAliceSecondChain = testMaster.mainAccountSecondChain();
            if (!maybeAliceSecondChain) {
                throw new Error(
                    'Interop tests cannot be run if the second chain is not set. Use the --second-chain flag to specify a different second chain to run the tests on.'
                );
            }
            aliceSecondChain = maybeAliceSecondChain!;
        }

        // Setup Interop2 Provider and Wallet
        // interop2Provider = new RetryProvider(
        //     { url: await getL2bUrl('validium'), timeout: 1200 * 1000 },
        //     undefined,
        //     testMaster.reporter
        // );
        if (skipInteropTests) {
            return;
        }
        interop2Provider = aliceSecondChain.provider;
        interop2RichWallet = new zksync.Wallet(mainAccount.privateKey, interop2Provider, l1Provider);

        // gatewayProvider = new RetryProvider(
        //     { url: await getL2bUrl('gateway'), timeout: 1200 * 1000 },
        //     undefined,
        //     testMaster.reporter
        // );
        // gatewayWallet = new zksync.Wallet(zksync.Wallet.createRandom().privateKey, gatewayProvider);

        // Initialize Contracts on Interop1
        interop1InteropCenter = new zksync.Contract(
            L2_INTEROP_CENTER_ADDRESS,
            ArtifactInteropCenter.abi,
            interop1Wallet
        );
        interop2InteropHandler = new zksync.Contract(
            L2_INTEROP_HANDLER_ADDRESS,
            ArtifactInteropHandler.abi,
            interop2RichWallet
        );
        interop1NativeTokenVault = new zksync.Contract(
            L2_NATIVE_TOKEN_VAULT_ADDRESS,
            ArtifactNativeTokenVault.abi,
            interop1Wallet
        );

        // Initialize Contracts on Interop2
        interop2NativeTokenVault = new zksync.Contract(
            L2_NATIVE_TOKEN_VAULT_ADDRESS,
            ArtifactNativeTokenVault.abi,
            interop2Provider
        );

        isSameBaseToken =
            testMaster.environment().baseToken.l1Address == testMaster.environment().baseTokenSecondChain!.l1Address;
    });

    let withdrawalHash: string;
    let params: FinalizeWithdrawalParams;
    test('Can check withdrawal hash in L2-A', async () => {
        if (skipInteropTests) {
            return;
        }

        const l2MessageVerification = new zksync.Contract(
            L2_MESSAGE_VERIFICATION_ADDRESS,
            ArtifactL2MessageVerification.abi,
            alice.provider
        );

        // Perform a withdrawal and wait for it to be processed
        const withdrawalPromise = await alice.withdraw({
            token: tokenDetails.l2Address,
            amount: 1
        });
        await expect(withdrawalPromise).toBeAccepted();
        const withdrawalTx = await withdrawalPromise;
        withdrawalHash = withdrawalTx.hash;
        const l2TxReceipt = await alice.provider.getTransactionReceipt(withdrawalHash);
        await waitForL2ToL1LogProof(alice, l2TxReceipt!.blockNumber, withdrawalHash);

        // Proof-based interop on Gateway, meaning the Merkle proof hashes to Gateway's MessageRoot
        params = await alice.getFinalizeWithdrawalParams(withdrawalHash, undefined, 'proof_based_gw');

        // Needed else the L2's view of GW's MessageRoot won't be updated
        await waitForInteropRootNonZero(alice.provider, alice, getGWBlockNumber(params));

        const included = await l2MessageVerification.proveL2MessageInclusionShared(
            Number((await alice.provider.getNetwork()).chainId),
            params.l1BatchNumber,
            params.l2MessageIndex,
            { txNumberInBatch: params.l2TxNumberInBlock, sender: params.sender, data: params.message },
            params.proof
        );
        expect(included).toBe(true);
    });

    test('Can check withdrawal hash from L2-B', async () => {
        if (skipInteropTests) {
            return;
        }

        const l2MessageVerification = new zksync.Contract(
            L2_MESSAGE_VERIFICATION_ADDRESS,
            ArtifactL2MessageVerification.abi,
            aliceSecondChain.provider
        );

        // Needed else the L2's view of GW's MessageRoot won't be updated
        await waitForInteropRootNonZero(aliceSecondChain.provider, aliceSecondChain, getGWBlockNumber(params));

        // We use the same proof that was verified in L2-A
        const included = await l2MessageVerification.proveL2MessageInclusionShared(
            Number((await alice.provider.getNetwork()).chainId),
            params.l1BatchNumber,
            params.l2MessageIndex,
            { txNumberInBatch: params.l2TxNumberInBlock, sender: params.sender, data: params.message },
            params.proof
        );
        expect(included).toBe(true);
    });

    afterAll(async () => {
        await testMaster.deinitialize();
    });
});
