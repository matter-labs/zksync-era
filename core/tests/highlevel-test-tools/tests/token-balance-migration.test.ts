import { afterAll, beforeAll, describe, it } from 'vitest';
import { TESTED_CHAIN_TYPE } from '../src';

import * as utils from 'utils';
import {
    WithdrawalHandler,
    ChainHandler,
    generateChainRichWallet,
    ERC20Handler,
    expectRevertWithSelector,
    waitUntilBlockExecutedOnGateway,
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

/// Primary goal is to validate the `zkstack chain gateway` migration command flow end-to-end.
/// Token-level checks here are sanity assertions around post-migration accounting and withdrawals.
(shouldSkip ? describe.skip : describe)('Token balance migration tests', function () {
    let chainHandler: ChainHandler;
    let gwBalanceHandler: GatewayBalanceHandler;

    let gwRichWallet: zksync.Wallet;
    let chainRichWallet: zksync.Wallet;

    // Stored token data for cross-test assertions
    const tokens: Record<string, ERC20Handler> = {};
    // Unfinalized withdrawal data for cross-test assertions
    const unfinalizedWithdrawals: Record<string, WithdrawalHandler> = {};
    // Withdrawals
    const withdrawals: Record<string, WithdrawalHandler> = {};

    beforeAll(async () => {
        // Initialize gateway chain
        await initTestWallet(GATEWAY_CHAIN_NAME);
        gwRichWallet = await generateChainRichWallet(GATEWAY_CHAIN_NAME);
        gwBalanceHandler = new GatewayBalanceHandler();
        await gwBalanceHandler.initEcosystemContracts(gwRichWallet);
        await gwBalanceHandler.resetBaseTokenBalance();

        // Initialize tested chain
        console.log(`Creating a new ${TESTED_CHAIN_TYPE} chain...`);
        chainHandler = await ChainHandler.createNewChain(TESTED_CHAIN_TYPE, gwBalanceHandler);
        await chainHandler.initEcosystemContracts(gwRichWallet);
        chainRichWallet = chainHandler.l2RichWallet;

        // DEPLOY TOKENS THAT WILL BE TESTED
        // Token native to L1
        tokens.L1Native = await ERC20Handler.deployTokenOnL1(chainRichWallet);
        // Token native to L1, not deposited to L2 yet
        tokens.L1NativeNotDepositedToL2 = await ERC20Handler.deployTokenOnL1(chainRichWallet);
        // Token native to L2
        tokens.L2Native = await ERC20Handler.deployTokenOnL2(chainHandler);
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
            // Since the base token was deposited after migrating to gateway, its balance is automatically migrated
            const expectedMigrationNumber = assetId === chainHandler.baseTokenAssetId ? 1n : 0n;
            await expect(
                chainHandler.assertAssetTrackersState(assetId, {
                    migrations: {
                        L1AT: expectedMigrationNumber,
                        GWAT: expectedMigrationNumber
                    }
                })
            ).resolves.toBe(true);
        }
    });

    it('Can deposit a token to the chain after migrating to gateway', async () => {
        // Deposit L1 token
        await tokens.L1Native.deposit(chainHandler, gwBalanceHandler);
        const assetId = await tokens.L1Native.assetId(chainHandler);
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

    it('Can initiate token balance migration to Gateway', async () => {
        await chainHandler.initiateTokenBalanceMigration('to-gateway');
    });

    it('Cannot withdraw tokens that have not been migrated', async () => {
        await expectRevertWithSelector(
            tokens.L2Native.withdraw(chainHandler),
            '0x90ed63bb',
            'Withdrawal before finalizing token balance migration to gateway should revert'
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
        // This also tests repeated migrations, as `L1Native` was already effectively migrated
        // This command tries to migrate it again, which will succeed, but later balance check will show it stays the same
        await chainHandler.finalizeTokenBalanceMigration('to-gateway');
        // We need to wait for a bit for L1AT's `_sendConfirmationToChains` to propagate to GW and the tested L2 chain
        await utils.sleep(1);
    });

    it('Can withdraw tokens after migrating token balances to gateway', async () => {
        withdrawals.L2Native = await tokens.L2Native.withdraw(chainHandler);
        const receipt = await chainRichWallet.provider.getTransactionReceipt(withdrawals.L2Native.txHash);
        if (!receipt) {
            throw new Error('Missing receipt for Gateway-settled withdrawal');
        }
        await waitUntilBlockExecutedOnGateway(chainRichWallet, gwRichWallet, receipt.blockNumber);
    });

    it('Correctly assigns chain token balances after migrating token balances to gateway', async () => {
        for (const tokenName of Object.keys(tokens)) {
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
            // Gateway's L1 balance for the base token is global to the shared gateway chain.
            // Other high-level suites mutate it by agreeing to pay settlement fees for their chains,
            // so only non-base assets are deterministic in the full parallel run.
            if (assetId !== chainHandler.baseTokenAssetId) {
                await gwBalanceHandler.assertGWBalance(assetId);
            }
        }
    });

    it('Can migrate the chain from gateway', async () => {
        await chainHandler.migrateFromGateway();
    });

    it('Can withdraw tokens from the chain', async () => {
        unfinalizedWithdrawals.L1Native = await tokens.L1Native.withdraw(chainHandler);
        unfinalizedWithdrawals.baseToken = await tokens.baseToken.withdraw(chainHandler);
    });

    it('Can initiate token balance migration from Gateway', async () => {
        await chainHandler.initiateTokenBalanceMigration('from-gateway');
    });

    it('Can deposit a token to the chain after migrating from gateway', async () => {
        // Deposit L1 token that was not deposited to L2 yet effectively marks it as migrated
        await tokens.L1NativeNotDepositedToL2.deposit(chainHandler, gwBalanceHandler);
        await expect(
            chainHandler.assertAssetTrackersState(await tokens.L1NativeNotDepositedToL2.assetId(chainHandler), {
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
        // This also tests repeated migrations, as `L1NativeNotDepositedToL2` was already effectively migrated
        // This command tries to migrate it again, which will succeed, but later balance check will show it stays the same
        await chainHandler.finalizeTokenBalanceMigration('from-gateway');
        // We need to wait for a bit for L1AT's `_sendConfirmationToChains` to propagate to GW and the tested L2 chain
        await utils.sleep(1);
    });

    it('Correctly assigns chain token balances after migrating token balances to L1', async () => {
        for (const tokenName of Object.keys(tokens)) {
            const assetId = await tokens[tokenName].assetId(chainHandler);
            if (assetId === ethers.ZeroHash) continue;

            // Tokens deposited AFTER migrating from gateway won't have a GWAT migration number set
            const depositedAfterFromGW = tokenName === 'L1NativeNotDepositedToL2';

            await expect(
                chainHandler.assertAssetTrackersState(assetId, {
                    migrations: {
                        L1AT: 2n,
                        GWAT: depositedAfterFromGW ? 0n : 2n
                    }
                })
            ).resolves.toBe(true);
            // Gateway's L1 balance for the base token is shared across all gateway-enabled suites.
            // Exact assertions for it are only valid in isolation, so keep the strict check for
            // all other assets and rely on chain-local trackers for the base token here.
            if (assetId !== chainHandler.baseTokenAssetId) {
                await gwBalanceHandler.assertGWBalance(assetId);
            }
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
        console.log('Complete');
    });
});
