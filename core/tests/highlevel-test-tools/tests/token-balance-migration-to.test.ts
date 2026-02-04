import { afterAll, beforeAll, describe, it } from 'vitest';
import { TESTED_CHAIN_TYPE } from '../src';

import * as utils from 'utils';
import {
    WithdrawalHandler,
    ChainHandler,
    generateChainRichWallet,
    ERC20Handler,
    expectRevertWithSelector,
    TOKEN_MINT_AMOUNT
} from './token-balance-migration-tester';
import * as zksync from 'zksync-ethers';
import * as ethers from 'ethers';
import { expect } from 'vitest';
import { initTestWallet } from '../src/run-integration-tests';
import { GATEWAY_CHAIN_ID } from 'utils/src/constants';

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
(shouldSkip ? describe.skip : describe)('Token balance migration TO GW tests', function () {
    let chainHandler: ChainHandler;
    let customTokenChainHandler: ChainHandler;

    let l1RichWallet: ethers.Wallet;
    let gwRichWallet: zksync.Wallet;
    let chainRichWallet: zksync.Wallet;
    let customTokenChainRichWallet: zksync.Wallet;

    // Stored token data for cross-test assertions
    const tokens: Record<string, ERC20Handler> = {};
    const tokensSecondChain: Record<string, ERC20Handler> = {};
    // Unfinalized withdrawal data for cross-test assertions
    const unfinalizedWithdrawals: Record<string, WithdrawalHandler> = {};
    const unfinalizedWithdrawalsSecondChain: Record<string, WithdrawalHandler> = {};

    beforeAll(async () => {
        // Initialize gateway chain
        console.log('Initializing rich wallet for gateway chain...');
        await initTestWallet(GATEWAY_CHAIN_NAME);
        gwRichWallet = await generateChainRichWallet(GATEWAY_CHAIN_NAME);
        l1RichWallet = gwRichWallet.ethWallet();
        console.log('Gateway rich wallet private key:', gwRichWallet.privateKey);

        // Initialize tested chain
        console.log(`Creating a new ${TESTED_CHAIN_TYPE} chain...`);
        chainHandler = await ChainHandler.createNewChain(TESTED_CHAIN_TYPE);
        await chainHandler.initEcosystemContracts(gwRichWallet);
        chainRichWallet = chainHandler.l2RichWallet;
        console.log('Chain rich wallet private key:', chainRichWallet.privateKey);
        // Initialize auxiliary chain
        console.log('Creating a secondary chain...');
        customTokenChainHandler = await ChainHandler.createNewChain('era');
        customTokenChainRichWallet = customTokenChainHandler.l2RichWallet;

        // DEPLOY TOKENS THAT WILL BE TESTED
        // Token native to L2-B, withdrawn from L2-B, and deposited to L2-A
        tokensSecondChain.L2BToken = await ERC20Handler.deployTokenOnL2(customTokenChainHandler);
        unfinalizedWithdrawalsSecondChain.L2BToken = await tokensSecondChain.L2BToken.withdraw(
            customTokenChainHandler,
            TOKEN_MINT_AMOUNT
        );
        // Token native to L2-B, withdrawn from L2-B, not yet deposited to L2-A
        tokensSecondChain.L2BTokenNotDepositedToL2A = await ERC20Handler.deployTokenOnL2(customTokenChainHandler);
        unfinalizedWithdrawalsSecondChain.L2BTokenNotDepositedToL2A =
            await tokensSecondChain.L2BTokenNotDepositedToL2A.withdraw(customTokenChainHandler, TOKEN_MINT_AMOUNT);

        // Token native to L1, deposited to L2
        tokens.L1NativeDepositedToL2 = await ERC20Handler.deployTokenOnL1(chainRichWallet);
        await tokens.L1NativeDepositedToL2.deposit(chainHandler);
        unfinalizedWithdrawals.L1NativeDepositedToL2 = await tokens.L1NativeDepositedToL2.withdraw(chainHandler);
        // Token native to L1, not deposited to L2 yet
        tokens.L1NativeNotDepositedToL2 = await ERC20Handler.deployTokenOnL1(chainRichWallet);

        // Token native to L2-A, withdrawn to L1
        tokens.L2NativeWithdrawnToL1 = await ERC20Handler.deployTokenOnL2(chainHandler);
        unfinalizedWithdrawals.L2NativeWithdrawnToL1 = await tokens.L2NativeWithdrawnToL1.withdraw(chainHandler);
        // Token native to L2-A, not withdrawn to L1 yet
        tokens.L2NativeNotWithdrawnToL1 = await ERC20Handler.deployTokenOnL2(chainHandler);

        // Add the base token to the list
        tokens.baseToken = new ERC20Handler(chainHandler.l2RichWallet, chainHandler.l1BaseTokenContract, undefined);
        const baseTokenAssetId = await tokens.baseToken.assetId(chainHandler);
        // Get the current balance of the base token on the chain for accounting purposes
        chainHandler.chainBalances[baseTokenAssetId] = await chainHandler.l1AssetTracker.chainBalance(
            chainHandler.inner.chainId,
            baseTokenAssetId
        );
    });

    it('Correctly assigns chain token balances', async () => {
        // Chain balances are accounted correctly on L1AT
        for (const token of Object.keys(tokens)) {
            const assetId = await tokens[token].assetId(chainHandler);
            if (assetId === ethers.ZeroHash) continue;
            await expect(
                chainHandler.assertAssetTrackersState(assetId, {
                    balances: {
                        L1AT_GW: 0n,
                        GWAT: 0n
                    },
                    migrations: {
                        L1AT: 0n,
                        L1AT_GW: 0n,
                        GWAT: 0n
                    }
                })
            ).resolves.toBe(true);
        }
    });

    it('Can migrate the chain to Gateway', async () => {
        await chainHandler.migrateToGateway();
    });

    it('Can deposit a token to the chain after migrating to gateway', async () => {
        // Deposit L1 token that was not deposited to L2 yet
        await tokens.L1NativeNotDepositedToL2.deposit(chainHandler);
        await expect(
            chainHandler.assertAssetTrackersState(await tokens.L1NativeNotDepositedToL2.assetId(chainHandler), {
                balances: {
                    L1AT: 0n
                },
                migrations: {
                    L1AT: 1n,
                    L1AT_GW: 0n,
                    GWAT: 1n
                }
            })
        ).resolves.toBe(true);

        // Finalize withdrawal of L2-B token
        await unfinalizedWithdrawalsSecondChain.L2BToken.finalizeWithdrawal(chainRichWallet.ethWallet());
        delete unfinalizedWithdrawalsSecondChain.L2BToken;
        // Define the L2-B token for L2-A use
        const L2BTokenL1Contract = await tokensSecondChain.L2BToken.getL1Contract(customTokenChainHandler);
        tokens.L2BToken = await ERC20Handler.fromL2BL1Token(
            L2BTokenL1Contract,
            chainRichWallet,
            customTokenChainRichWallet
        );
        // Deposit L2-B token to L2-A
        await tokens.L2BToken.deposit(chainHandler);
        await expect(
            chainHandler.assertAssetTrackersState(await tokens.L1NativeNotDepositedToL2.assetId(chainHandler), {
                balances: {
                    L1AT: 0n
                },
                migrations: {
                    L1AT: 1n,
                    L1AT_GW: 0n,
                    GWAT: 1n
                }
            })
        ).resolves.toBe(true);
    });

    it('Can initiate token balance migration to Gateway', async () => {
        await chainHandler.initiateTokenBalanceMigration('to-gateway');
    });

    it('Cannot yet withdraw tokens from the chain before finalizing token balance migration', async () => {
        await expectRevertWithSelector(
            tokens.L1NativeDepositedToL2.withdraw(chainHandler),
            '0x90ed63bb',
            'Withdrawal before finalizing token balance migration to gateway should revert'
        );
    });

    it('Can finalize pending withdrawals', async () => {
        // Finalize all pending withdrawals for L2-B
        for (const tokenName of Object.keys(unfinalizedWithdrawalsSecondChain)) {
            await unfinalizedWithdrawalsSecondChain[tokenName].finalizeWithdrawal(chainRichWallet.ethWallet());
            delete unfinalizedWithdrawalsSecondChain[tokenName];
        }

        // Finalize all pending withdrawals for L2-A
        for (const tokenName of Object.keys(unfinalizedWithdrawals)) {
            await unfinalizedWithdrawals[tokenName].finalizeWithdrawal(chainRichWallet.ethWallet());

            // Ensure accounting is correct
            const assetId = await tokens[tokenName].assetId(chainHandler);
            if (tokens[tokenName].isL2Token) chainHandler.chainBalances[assetId] = ethers.MaxUint256;
            chainHandler.chainBalances[assetId] -= unfinalizedWithdrawals[tokenName].amount;

            // We can now define the L1 contracts for the tokens
            await tokens[tokenName]?.setL1Contract(chainHandler);

            delete unfinalizedWithdrawals[tokenName];
        }

        // Define the L2-B token for L2-A use
        const L2BTokenNotDepositedToL2AL1Contract =
            await tokensSecondChain.L2BTokenNotDepositedToL2A.getL1Contract(customTokenChainHandler);
        tokens.L2BTokenNotDepositedToL2A = await ERC20Handler.fromL2BL1Token(
            L2BTokenNotDepositedToL2AL1Contract,
            chainRichWallet,
            customTokenChainRichWallet
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

    it('Can migrate token balances to GW', async () => {
        // Take snapshot right before migration
        // Base token balance increases slighly due to previous token deposits, here we account for that
        const existingBaseTokenL1ATBalanceForGW = await chainHandler.l1AssetTracker.chainBalance(
            GATEWAY_CHAIN_ID,
            chainHandler.baseTokenAssetId
        );
        chainHandler.existingBaseTokenL1ATBalanceForGW = existingBaseTokenL1ATBalanceForGW;
        // Finalize migrating token balances to Gateway
        // This also tests repeated migrations, as `L1NativeNotDepositedToL2` was already effectively migrated
        // This command tries to migrate it again, which will succeed, but later balance check will show it stays the same
        await chainHandler.finalizeTokenBalanceMigration('to-gateway');
        // We need to wait for a bit for L1AT's `_sendConfirmationToChains` to propagate to GW and the tested L2 chain
        await utils.sleep(1);
    });

    it('Correctly assigns chain token balances after migrating token balances to gateway', async () => {
        for (const tokenName of Object.keys(tokens)) {
            if (tokenName === 'L2BTokenNotDepositedToL2A') continue;
            const assetId = await tokens[tokenName].assetId(chainHandler);
            if (assetId === ethers.ZeroHash) continue;

            const isL2Token = tokens[tokenName].isL2Token;
            const baseBalance = chainHandler.chainBalances[assetId] ?? (isL2Token ? ethers.MaxUint256 : 0n);

            await expect(
                chainHandler.assertAssetTrackersState(assetId, {
                    balances: {
                        L1AT: 0n,
                        L1AT_GW: baseBalance,
                        GWAT: baseBalance
                    },
                    migrations: {
                        L1AT: 1n,
                        L1AT_GW: 0n,
                        GWAT: 1n
                    }
                })
            ).resolves.toBe(true);
        }
    });

    it('Can deposit tokens to the chain after migrating token balances to gateway', async () => {
        // Deposit L2-B token that was not deposited to L2-A yet effectively marks it as migrated
        await tokens.L2BTokenNotDepositedToL2A.deposit(chainHandler);
        await expect(
            chainHandler.assertAssetTrackersState(await tokens.L2BTokenNotDepositedToL2A.assetId(chainHandler), {
                balances: {
                    L1AT: 0n
                },
                migrations: {
                    L1AT: 1n,
                    L1AT_GW: 0n,
                    GWAT: 1n
                }
            })
        ).resolves.toBe(true);
    });

    it('Can withdraw tokens from the chain', async () => {
        await tokens.L1NativeDepositedToL2.withdraw(chainHandler);
    });

    afterAll(async () => {
        console.log('Tearing down chains...');
        if (chainHandler) {
            await chainHandler.stopServer();
        }
        if (customTokenChainHandler) {
            await customTokenChainHandler.stopServer();
        }
        console.log('Complete');
    });
});
