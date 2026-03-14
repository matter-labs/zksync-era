import { afterAll, beforeAll, describe, it } from 'vitest';
import { TESTED_CHAIN_TYPE } from '../src';

import * as utils from 'utils';
import {
    WithdrawalHandler,
    ChainHandler,
    generateChainRichWallet,
    ERC20Handler,
    expectRevertWithSelector,
    TOKEN_MINT_AMOUNT,
    INTEROP_TEST_AMOUNT,
    sendInteropBundle,
    awaitInteropBundle,
    readAndBroadcastInteropBundle,
    readAndUnbundleInteropBundle,
    waitUntilBlockExecutedOnGateway,
    attemptExecuteBundle,
    attemptUnbundleBundle,
    GatewayBalanceHandler
} from './token-balance-migration-tester';
import * as zksync from 'zksync-ethers';
import * as ethers from 'ethers';
import { expect } from 'vitest';
import { initTestWallet } from '../src/run-integration-tests';

// This test requires the Gateway chain to be present.
const GATEWAY_CHAIN_NAME = 'gateway';
const useGatewayChain = process.env.USE_GATEWAY_CHAIN;
const shouldSkip = useGatewayChain !== 'WITH_GATEWAY';

if (shouldSkip) {
    console.log(
        `⏭️ Skipping asset migration test for ${TESTED_CHAIN_TYPE} chain (USE_GATEWAY_CHAIN=${useGatewayChain})`
    );
}

/// There are the following kinds of tokens' states that we test:
/// At the moment of migration the token can be:
/// - Native to chain, already present on L1/other L2s.
/// - Native to chain, not present on L1 at all (can have unfinalized withdrawal).
/// - Native to L1, never been on the chain.
/// - Native to L1, already present on the chain.
/// - Native to another L2, never present on the chain.
/// - Native to another L2, already present on the chain.
/// After the chain migrates to GW, we can classify the states of the tokens the following way:
/// - Migrated the balance to GW. May be done after the token already received some deposits.
/// - Never migrated the balance to GW (but the token is known to the chain). May be done after the token.
/// - Never migrated the balance to GW (but the token is bridged for the first time). No migration should be needed at all.
/// After the chain migrates from GW, we need to test that all the tokens can be withdrawn in sufficient amounts to move
/// the entire balance to L1. It should not be possible to finalize all old interops.
(shouldSkip ? describe.skip : describe)('Token balance migration tests', function () {
    let chainHandler: ChainHandler;
    let secondChainHandler: ChainHandler;
    let gwBalanceHandler: GatewayBalanceHandler;

    let gwRichWallet: zksync.Wallet;
    let chainRichWallet: zksync.Wallet;
    let secondChainRichWallet: zksync.Wallet;

    // Stored token data for cross-test assertions
    const tokens: Record<string, ERC20Handler> = {};
    const tokensSecondChain: Record<string, ERC20Handler> = {};
    // Unfinalized withdrawal data for cross-test assertions
    const unfinalizedWithdrawals: Record<string, WithdrawalHandler> = {};
    const unfinalizedWithdrawalsSecondChain: Record<string, WithdrawalHandler> = {};
    // Withdrawals initiated while the chain is on Gateway
    const gatewayEraWithdrawals: Record<string, WithdrawalHandler> = {};
    // Bundles sent from chain to second chain, to be executed while second chain settles on Gateway
    const bundlesExecutedOnGateway: Record<string, ethers.TransactionReceipt> = {};
    // Bundles sent from chain to second chain, to be unbundled while second chain settles on Gateway
    const bundlesUnbundledOnGateway: Record<string, ethers.TransactionReceipt> = {};

    beforeAll(async () => {
        // Initialize gateway chain
        console.log('Initializing rich wallet for gateway chain...');
        await initTestWallet(GATEWAY_CHAIN_NAME);
        gwRichWallet = await generateChainRichWallet(GATEWAY_CHAIN_NAME);
        console.log('Gateway rich wallet private key:', gwRichWallet.privateKey);
        gwBalanceHandler = new GatewayBalanceHandler();
        await gwBalanceHandler.initEcosystemContracts(gwRichWallet);
        await gwBalanceHandler.resetBaseTokenBalance();

        // Initialize tested chain
        console.log(`Creating a new ${TESTED_CHAIN_TYPE} chain...`);
        chainHandler = await ChainHandler.createNewChain(TESTED_CHAIN_TYPE, gwBalanceHandler);
        await chainHandler.initEcosystemContracts(gwRichWallet);
        chainRichWallet = chainHandler.l2RichWallet;
        console.log('Chain rich wallet private key:', chainRichWallet.privateKey);
        // Initialize auxiliary chain
        console.log('Creating a secondary chain...');
        secondChainHandler = await ChainHandler.createNewChain('era', gwBalanceHandler);
        await secondChainHandler.initEcosystemContracts(gwRichWallet);
        secondChainRichWallet = secondChainHandler.l2RichWallet;

        // DEPLOY TOKENS THAT WILL BE TESTED
        // Token native to L1, deposited to L2
        tokens.L1NativeDepositedToL2 = await ERC20Handler.deployTokenOnL1(chainRichWallet);
        await tokens.L1NativeDepositedToL2.deposit(chainHandler, gwBalanceHandler);
        unfinalizedWithdrawals.L1NativeDepositedToL2 = await tokens.L1NativeDepositedToL2.withdraw(chainHandler);
        // Token native to L2-A, withdrawn to L1
        tokens.L2NativeWithdrawnToL1 = await ERC20Handler.deployTokenOnL2(chainHandler);
        unfinalizedWithdrawals.L2NativeWithdrawnToL1 = await tokens.L2NativeWithdrawnToL1.withdraw(chainHandler);

        // Token native to L2-B, withdrawn from L2-B, and deposited to L2-A
        tokensSecondChain.L2BToken = await ERC20Handler.deployTokenOnL2(secondChainHandler);
        unfinalizedWithdrawalsSecondChain.L2BToken = await tokensSecondChain.L2BToken.withdraw(
            secondChainHandler,
            TOKEN_MINT_AMOUNT
        );
        // Token native to L2-B, withdrawn from L2-B, not yet deposited to L2-A
        tokensSecondChain.L2BTokenNotDepositedToL2A = await ERC20Handler.deployTokenOnL2(secondChainHandler);
        unfinalizedWithdrawalsSecondChain.L2BTokenNotDepositedToL2A =
            await tokensSecondChain.L2BTokenNotDepositedToL2A.withdraw(secondChainHandler, TOKEN_MINT_AMOUNT);

        // Token native to L1, not deposited to L2 yet
        tokens.L1NativeNotDepositedToL2 = await ERC20Handler.deployTokenOnL1(chainRichWallet);
        // Token native to L2-A, not withdrawn to L1 yet
        tokens.L2NativeNotWithdrawnToL1 = await ERC20Handler.deployTokenOnL2(chainHandler);

        // Add the base token to the list
        tokens.baseToken = new ERC20Handler(
            chainHandler.l2RichWallet,
            chainHandler.l1BaseTokenContract,
            undefined,
            true
        );
    });

    it('Correctly assigns chain token balances', async () => {
        // Chain balances are accounted correctly on L1AT
        for (const token of Object.keys(tokens)) {
            const assetId = await tokens[token].assetId(chainHandler);
            if (assetId === ethers.ZeroHash) continue;
            await expect(
                chainHandler.assertAssetTrackersState(assetId, {
                    migrations: {
                        L1AT: 0n,
                        GWAT: 0n
                    }
                })
            ).resolves.toBe(true);
        }
    });

    it('Cannot initiate interop before migrating to gateway', async () => {
        await expectRevertWithSelector(
            sendInteropBundle(
                chainRichWallet,
                secondChainHandler.inner.chainId,
                await tokens.L1NativeDepositedToL2.l2Contract?.getAddress()
            ),
            '"0x472477e2', // NotInGatewayMode
            'Initiate interop before migrating to gateway should revert'
        );
    });

    it('Can migrate both chains to Gateway', async () => {
        await chainHandler.migrateToGateway();
        await secondChainHandler.migrateToGateway();
    });

    it('Can deposit a token to the chain after migrating to gateway', async () => {
        // Deposit L1 token that was not deposited to L2 yet
        await tokens.L1NativeNotDepositedToL2.deposit(chainHandler, gwBalanceHandler);
        const token1AssetId = await tokens.L1NativeNotDepositedToL2.assetId(chainHandler);
        await expect(
            chainHandler.assertAssetTrackersState(token1AssetId, {
                migrations: {
                    L1AT: 1n,
                    GWAT: 1n
                }
            })
        ).resolves.toBe(true);
        await gwBalanceHandler.assertGWBalance(token1AssetId);

        // Finalize withdrawal of L2-B token
        await unfinalizedWithdrawalsSecondChain.L2BToken.finalizeWithdrawal(chainRichWallet.ethWallet());
        delete unfinalizedWithdrawalsSecondChain.L2BToken;
        // Define the L2-B token for L2-A use
        const L2BTokenL1Contract = await tokensSecondChain.L2BToken.getL1Contract(secondChainHandler);
        tokens.L2BToken = await ERC20Handler.fromL2BL1Token(L2BTokenL1Contract, chainRichWallet, secondChainRichWallet);
        // Deposit L2-B token to L2-A
        await tokens.L2BToken.deposit(chainHandler, gwBalanceHandler);
        const assetId = await tokens.L2BToken.assetId(chainHandler);
        await expect(
            chainHandler.assertAssetTrackersState(assetId, {
                migrations: {
                    L1AT: 1n,
                    GWAT: 1n
                }
            })
        ).resolves.toBe(true);
        await gwBalanceHandler.assertGWBalance(assetId);
    });

    it('Cannot initiate interop to non registered chains', async () => {
        await expectRevertWithSelector(
            sendInteropBundle(
                chainRichWallet,
                1337, // Unregistered chain ID
                await tokens.L2BToken.l2Contract?.getAddress()
            ),
            '0x2d159f39', // DestinationChainNotRegistered
            'Initiate to non registered chains should revert'
        );
    });

    it('Can initiate token balance migration to Gateway', async () => {
        await chainHandler.initiateTokenBalanceMigration('to-gateway');
    });

    it('Cannot initiate interop for non migrated tokens', async () => {
        // We already registered the destination chain on the sending chain before migrating any of them to gateway
        // Trying this registration now would revert with `ChainsSettlementLayerMismatch`
        await expectRevertWithSelector(
            sendInteropBundle(
                chainRichWallet,
                secondChainHandler.inner.chainId,
                await tokens.L1NativeDepositedToL2.l2Contract?.getAddress()
            ),
            '0x90ed63bb', // TokenBalanceNotMigratedToGateway
            'Initiate interop for non migrated tokens should revert'
        );
    });

    it('Cannot withdraw tokens that have not been migrated', async () => {
        await expectRevertWithSelector(
            tokens.L1NativeDepositedToL2.withdraw(chainHandler),
            '0x90ed63bb',
            'Withdrawal before finalizing token balance migration to gateway should revert'
        );
    });

    it('Can finalize pending withdrawals after migrating to gateway', async () => {
        // Finalize all pending withdrawals for L2-B
        for (const tokenName of Object.keys(unfinalizedWithdrawalsSecondChain)) {
            await unfinalizedWithdrawalsSecondChain[tokenName].finalizeWithdrawal(chainRichWallet.ethWallet());
            delete unfinalizedWithdrawalsSecondChain[tokenName];
        }

        // Finalize all pending withdrawals for L2-A
        for (const tokenName of Object.keys(unfinalizedWithdrawals)) {
            await unfinalizedWithdrawals[tokenName].finalizeWithdrawal(chainRichWallet.ethWallet());

            // We can now define the L1 contracts for the tokens
            await tokens[tokenName]?.setL1Contract(chainHandler);

            delete unfinalizedWithdrawals[tokenName];
        }

        // Define the L2-B token for L2-A use
        const L2BTokenNotDepositedToL2AL1Contract =
            await tokensSecondChain.L2BTokenNotDepositedToL2A.getL1Contract(secondChainHandler);
        tokens.L2BTokenNotDepositedToL2A = await ERC20Handler.fromL2BL1Token(
            L2BTokenNotDepositedToL2AL1Contract,
            chainRichWallet,
            secondChainRichWallet
        );
    });

    it('Cannot initiate migration for a false assetId', async () => {
        const bogusAssetId = ethers.randomBytes(32);
        await expectRevertWithSelector(
            chainHandler.l2AssetTracker.initiateL1ToGatewayMigrationOnL2(bogusAssetId),
            '0xda72d995',
            'Initiate migration for false assetId should revert'
        );
    });

    it('Can migrate token balances to gateway', async () => {
        // Finalize migrating token balances to Gateway
        // This also tests repeated migrations, as `L1NativeNotDepositedToL2` was already effectively migrated
        // This command tries to migrate it again, which will succeed, but later balance check will show it stays the same
        await chainHandler.finalizeTokenBalanceMigration('to-gateway');
        // We need to wait for a bit for L1AT's `_sendConfirmationToChains` to propagate to GW and the tested L2 chain
        await utils.sleep(1);
    });

    it('Can withdraw tokens after migrating token balances to gateway', async () => {
        gatewayEraWithdrawals.L1NativeDepositedToL2 = await tokens.L1NativeDepositedToL2.withdraw(chainHandler);
        const receipt = await chainRichWallet.provider.getTransactionReceipt(
            gatewayEraWithdrawals.L1NativeDepositedToL2.txHash
        );
        if (!receipt) {
            throw new Error('Missing receipt for Gateway-settled withdrawal');
        }
        await waitUntilBlockExecutedOnGateway(chainRichWallet, gwRichWallet, receipt.blockNumber);
    });

    it('Correctly assigns chain token balances after migrating token balances to gateway', async () => {
        for (const tokenName of Object.keys(tokens)) {
            if (tokenName === 'L2BTokenNotDepositedToL2A') continue;
            const assetId = await tokens[tokenName].assetId(chainHandler);
            if (assetId === ethers.ZeroHash) continue;

            await expect(
                chainHandler.assertAssetTrackersState(assetId, {
                    migrations: {
                        L1AT: 1n,
                        GWAT: 1n
                    }
                })
            ).resolves.toBe(true);
            await gwBalanceHandler.assertGWBalance(assetId);
        }
    });

    const tokenNames = ['L1NativeDepositedToL2', 'L2NativeNotWithdrawnToL1', 'L2BToken'];
    it('Can initiate interop of migrated tokens', async () => {
        // Claiming interop requires the destination chain to settle on Gateway (not L1).
        // We send two sets of bundles: one to be executed, one to be unbundled on Gateway.
        // The destination pending balance is only updated after the source batches are executed on Gateway.
        let lastSendBlockNumber = 0;
        for (const tokenName of tokenNames) {
            bundlesExecutedOnGateway[tokenName] = await sendInteropBundle(
                chainRichWallet,
                secondChainHandler.inner.chainId,
                await tokens[tokenName].l2Contract?.getAddress()
            );
            lastSendBlockNumber = Math.max(lastSendBlockNumber, bundlesExecutedOnGateway[tokenName].blockNumber);

            bundlesUnbundledOnGateway[tokenName] = await sendInteropBundle(
                chainRichWallet,
                secondChainHandler.inner.chainId,
                await tokens[tokenName].l2Contract?.getAddress(),
                secondChainRichWallet.address // unbundler address
            );
            lastSendBlockNumber = Math.max(lastSendBlockNumber, bundlesUnbundledOnGateway[tokenName].blockNumber);
        }

        if (lastSendBlockNumber > 0) {
            await waitUntilBlockExecutedOnGateway(chainRichWallet, gwRichWallet, lastSendBlockNumber);
        }
        for (const tokenName of tokenNames) {
            // Two accounting: one for the bundle that will be executed and the one for the one that wont be
            await chainHandler.accountForSentInterop(tokens[tokenName], secondChainHandler);
            await chainHandler.accountForSentInterop(tokens[tokenName], secondChainHandler);
        }

        // After the source batches settle on Gateway, the destination chain's chainBalance has NOT increased yet.
        // Both sets of bundles now sit in pendingInteropBalance, awaiting execution confirmation.
        for (const tokenName of tokenNames) {
            const assetId = await tokens[tokenName].assetId(chainHandler);
            await expect(secondChainHandler.assertAssetTrackersState(assetId)).resolves.toBe(true);
        }
    });

    it('Can finalize interop of migrated tokens', async () => {
        // We wait for the last of these bundles to be executable on the destination chain.
        // By then, all of the bundles should be executable.
        await awaitInteropBundle(
            chainRichWallet,
            gwRichWallet,
            secondChainRichWallet,
            bundlesExecutedOnGateway[tokenNames[tokenNames.length - 1]].hash
        );
        let lastExecutionBlockNumber = 0;
        for (const bundleName of Object.keys(bundlesExecutedOnGateway)) {
            const receipt = await readAndBroadcastInteropBundle(
                secondChainRichWallet,
                chainRichWallet.provider,
                bundlesExecutedOnGateway[bundleName].hash
            );
            if (receipt) lastExecutionBlockNumber = Math.max(lastExecutionBlockNumber, receipt.blockNumber);
        }
        // Wait for secondChain to settle the batch containing the executeBundle txs on Gateway.
        // Only then does GWAssetTracker process the InteropHandler confirmation messages and move
        // balances from pendingInteropBalance to chainBalance.
        if (lastExecutionBlockNumber > 0) {
            await waitUntilBlockExecutedOnGateway(secondChainRichWallet, gwRichWallet, lastExecutionBlockNumber);
        }
        // The executed bundles are now confirmed: chainBalance increased, pendingInteropBalance decreased.
        // The unbundled bundles are not yet processed, so pendingInteropBalance still holds INTEROP_TEST_AMOUNT.
        for (const tokenName of tokenNames) {
            await secondChainHandler.confirmReceivedInterop(tokens[tokenName]);
            const assetId = await tokens[tokenName].assetId(chainHandler);
            if (assetId === ethers.ZeroHash) continue;
            await expect(secondChainHandler.assertAssetTrackersState(assetId)).resolves.toBe(true);
        }
    });

    it('Can unbundle interop of migrated tokens on Gateway', async () => {
        // unbundleBundle with CallStatus.Executed calls _executeCalls which calls _sendCallExecutedMessage
        // for each executed call. GWAssetTracker will therefore receive confirmations and move balances
        // from pendingInteropBalance to chainBalance during the next settlement.
        let lastUnbundleBlockNumber = 0;
        for (const bundleName of Object.keys(bundlesUnbundledOnGateway)) {
            const receipt = await readAndUnbundleInteropBundle(
                secondChainRichWallet,
                chainRichWallet.provider,
                bundlesUnbundledOnGateway[bundleName].hash
            );
            if (receipt) lastUnbundleBlockNumber = Math.max(lastUnbundleBlockNumber, receipt.blockNumber);
        }
        // Wait for secondChain to settle the batch containing the unbundleBundle txs on Gateway.
        // Only then does GWAssetTracker process the confirmation messages and move balances.
        if (lastUnbundleBlockNumber > 0) {
            await waitUntilBlockExecutedOnGateway(secondChainRichWallet, gwRichWallet, lastUnbundleBlockNumber);
        }
        // Both executed and unbundled bundles are now confirmed: full chainBalance, zero pendingInteropBalance.
        for (const tokenName of tokenNames) {
            await secondChainHandler.confirmReceivedInterop(tokens[tokenName]);
            const assetId = await tokens[tokenName].assetId(chainHandler);
            if (assetId === ethers.ZeroHash) continue;
            await expect(secondChainHandler.assertAssetTrackersState(assetId)).resolves.toBe(true);
        }
    });

    it('Can migrate the second chain from gateway', async () => {
        await secondChainHandler.migrateFromGateway();
    });

    it('Can initiate interop to chains that are registered on this chain, but migrated from gateway', async () => {
        // The destination chain has migrated from Gateway and now settles on L1.
        // The tokens leave chainHandler's balance and go to secondChain's pendingInteropBalance on GWAT.
        // They only appear as pending after the source batch is executed on Gateway, and remain pending
        // since secondChain is no longer on Gateway.
        const receipt = await sendInteropBundle(
            chainRichWallet,
            secondChainHandler.inner.chainId,
            await tokens.L1NativeDepositedToL2.l2Contract?.getAddress()
        );
        await waitUntilBlockExecutedOnGateway(chainRichWallet, gwRichWallet, receipt.blockNumber);
        await chainHandler.accountForSentInterop(tokens.L1NativeDepositedToL2, secondChainHandler);

        const assetId = await tokens.L1NativeDepositedToL2.assetId(chainHandler);
        await expect(secondChainHandler.assertAssetTrackersState(assetId)).resolves.toBe(true);
    });

    it('Can migrate the chain from gateway', async () => {
        await chainHandler.migrateFromGateway();
    });

    it('Cannot execute interop bundle when settling on L1', async () => {
        await expect(attemptExecuteBundle(chainRichWallet)).rejects.toThrow();
    });

    it('Cannot unbundle interop bundle when settling on L1', async () => {
        await expect(attemptUnbundleBundle(chainRichWallet)).rejects.toThrow();
    });

    it('Can withdraw tokens from the chain', async () => {
        unfinalizedWithdrawals.L1NativeDepositedToL2 = await tokens.L1NativeDepositedToL2.withdraw(chainHandler);
        unfinalizedWithdrawals.baseToken = await tokens.baseToken.withdraw(chainHandler);
    });

    it('Can initiate token balance migration from Gateway', async () => {
        await chainHandler.initiateTokenBalanceMigration('from-gateway');
    });

    it('Can deposit a token to the chain after migrating from gateway', async () => {
        // Deposit L2-B token that was not deposited to L2-A yet effectively marks it as migrated
        await tokens.L2BTokenNotDepositedToL2A.deposit(chainHandler, gwBalanceHandler);
        await expect(
            chainHandler.assertAssetTrackersState(await tokens.L2BTokenNotDepositedToL2A.assetId(chainHandler), {
                migrations: {
                    L1AT: 2n,
                    GWAT: 0n
                }
            })
        ).resolves.toBe(true);
    });

    it('Cannot finalize pending withdrawals before finalizing token balance migration to L1', async () => {
        for (const tokenName of Object.keys(unfinalizedWithdrawals)) {
            await expectRevertWithSelector(
                unfinalizedWithdrawals[tokenName].finalizeWithdrawal(chainRichWallet.ethWallet()),
                '0x07859b3b', // InsufficientChainBalance
                'Withdrawal before finalizing token balance migration to L1 should revert'
            );
        }
    });

    it('Can migrate token balances to L1', async () => {
        // Migrate token balances from gateway
        // This also tests repeated migrations, as `L2BTokenNotDepositedToL2A` was already effectively migrated
        // This command tries to migrate it again, which will succeed, but later balance check will show it stays the same
        await chainHandler.finalizeTokenBalanceMigration('from-gateway');
        // We need to wait for a bit for L1AT's `_sendConfirmationToChains` to propagate to GW and the tested L2 chain
        await utils.sleep(5);
    });

    it('Correctly assigns chain token balances after migrating token balances to L1', async () => {
        for (const tokenName of Object.keys(tokens)) {
            const assetId = await tokens[tokenName].assetId(chainHandler);
            if (assetId === ethers.ZeroHash) continue;

            // Tokens deposited AFTER migrating from gateway won't have a GWAT migration number set
            const depositedAfterFromGW = tokenName === 'L2BTokenNotDepositedToL2A';

            await expect(
                chainHandler.assertAssetTrackersState(assetId, {
                    migrations: {
                        L1AT: 2n,
                        GWAT: depositedAfterFromGW ? 0n : 2n
                    }
                })
            ).resolves.toBe(true);
            await gwBalanceHandler.assertGWBalance(assetId);
        }
    });

    it('Can finalize pending withdrawals after migrating token balances from gateway', async () => {
        for (const tokenName of Object.keys(unfinalizedWithdrawals)) {
            await unfinalizedWithdrawals[tokenName].finalizeWithdrawal(chainRichWallet.ethWallet());
            delete unfinalizedWithdrawals[tokenName];
        }
    });

    afterAll(async () => {
        console.log('Tearing down chains...');
        if (chainHandler) {
            await chainHandler.stopServer();
        }
        if (secondChainHandler) {
            await secondChainHandler.stopServer();
        }
        console.log('Complete');
    });
});
