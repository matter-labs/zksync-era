import { afterAll, beforeAll, describe, it } from 'vitest';
import { createChainAndStartServer, TESTED_CHAIN_TYPE, tokenBalanceMigrationTest } from '../src';

import * as utils from 'utils';
import {
    WithdrawalHandler,
    ChainHandler,
    generateChainRichWallet,
    ERC20Handler,
    RICH_WALLET_L1_BALANCE,
    RICH_WALLET_L2_BALANCE
} from './token-balance-migration-tester';
import * as zksync from 'zksync-ethers';
import * as ethers from 'ethers';
import { expect } from 'vitest';
import fs from 'node:fs/promises';
import { existsSync, readFileSync } from 'node:fs';
import { BytesLike } from '@ethersproject/bytes';
import { BigNumberish } from 'ethers';
import { loadConfig, shouldLoadConfigFromFile } from 'utils/build/file-configs';
import path from 'path';
import { CONTRACT_DEPLOYER, CONTRACT_DEPLOYER_ADDRESS, hashBytecode, ZKSYNC_MAIN_ABI } from 'zksync-ethers/build/utils';
import { utils as zksync_utils } from 'zksync-ethers';
import { logsTestPath } from 'utils/build/logs';
import { waitForNewL1Batch } from 'utils';
import { getMainWalletPk } from 'highlevel-test-tools/src/wallets';
import { initTestWallet } from '../src/run-integration-tests';
import { GATEWAY_CHAIN_ID } from 'utils/src/constants';

async function logsPath(name: string): Promise<string> {
    return await logsTestPath(fileConfig.chain, 'logs/upgrade/', name);
}

const L2_BRIDGEHUB_ADDRESS = '0x0000000000000000000000000000000000010002';
const pathToHome = path.join(__dirname, '../../../..');
const fileConfig = shouldLoadConfigFromFile();

// const contracts: Contracts = initContracts(pathToHome, fileConfig.loadFromFile);

const ZK_CHAIN_INTERFACE = JSON.parse(
    readFileSync(pathToHome + '/contracts/l1-contracts/out/IZKChain.sol/IZKChain.json').toString()
).abi;

const depositAmount = ethers.parseEther('0.001');

interface GatewayInfo {
    gatewayChainId: string;
    gatewayProvider: zksync.Provider;
    gatewayCTM: string;
    l2ChainAdmin: string;
    l2DiamondProxyAddress: string;
}

interface Call {
    target: string;
    value: BigNumberish;
    data: BytesLike;
}

// This test requires interop and so it requires Gateway chain.
// This is the name of the chain.
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
    // Stored withdrawal data for cross-test assertions
    const withdrawalsBeforeTBM: Record<string, WithdrawalHandler> = {};
    const withdrawalsAfterTBM: Record<string, WithdrawalHandler> = {};

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

        const withdrawalsToBeFinalized: Record<string, WithdrawalHandler> = {};
        // DEPLOY TOKENS THAT WILL BE TESTED
        // We first deploy all tokens that will need to be withdrawn from L2 to make testing faster
        // Token native to L1, deposited to L2, fully withdrawn from L2
        tokens.L1NativeWithdrawnFromL2 = await ERC20Handler.deployTokenOnL1(chainRichWallet);
        const L1NativeWithdrawnFromL2Amount = await tokens.L1NativeWithdrawnFromL2.deposit(chainHandler);
        withdrawalsToBeFinalized.L1NativeWithdrawnFromL2 = await tokens.L1NativeWithdrawnFromL2.withdraw(
            chainHandler,
            true,
            L1NativeWithdrawnFromL2Amount
        );
        // Token native to L1, deposited to L2, partially withdrawn from L2
        tokens.L1NativePartiallyWithdrawnFromL2 = await ERC20Handler.deployTokenOnL1(chainRichWallet);
        await tokens.L1NativePartiallyWithdrawnFromL2.deposit(chainHandler);
        withdrawalsToBeFinalized.L1NativePartiallyWithdrawnFromL2 =
            await tokens.L1NativePartiallyWithdrawnFromL2.withdraw(chainHandler);
        // Token native to L1, deposited to L2, fully withdrawn from L2 but not finalized yet
        tokens.L1NativeUnfinalizedWithdrawalBeforeTBMToGW = await ERC20Handler.deployTokenOnL1(chainRichWallet);
        await tokens.L1NativeUnfinalizedWithdrawalBeforeTBMToGW.deposit(chainHandler);
        withdrawalsBeforeTBM.L1NativeUnfinalizedWithdrawalBeforeTBMToGW =
            await tokens.L1NativeUnfinalizedWithdrawalBeforeTBMToGW.withdraw(chainHandler, false);
        tokens.L1NativeUnfinalizedWithdrawalAfterTBMToGW = await ERC20Handler.deployTokenOnL1(chainRichWallet);
        await tokens.L1NativeUnfinalizedWithdrawalAfterTBMToGW.deposit(chainHandler);
        withdrawalsAfterTBM.L1NativeUnfinalizedWithdrawalAfterTBMToGW =
            await tokens.L1NativeUnfinalizedWithdrawalAfterTBMToGW.withdraw(chainHandler, false);
        // Token native to L1, deposited to L2
        tokens.L1NativeDepositedToL2 = await ERC20Handler.deployTokenOnL1(chainRichWallet);
        await tokens.L1NativeDepositedToL2.deposit(chainHandler);
        // Token native to L1, not deposited to L2 yet
        tokens.L1NativeNotDepositedToL2 = await ERC20Handler.deployTokenOnL1(chainRichWallet);

        // Token native to L2-A, fully withdrawn to L1
        tokens.L2NativeWithdrawnToL1 = await ERC20Handler.deployTokenOnL2(chainHandler);
        const L2NativeWithdrawnToL1Amount = await tokens.L2NativeWithdrawnToL1.getL2Balance();
        withdrawalsToBeFinalized.L2NativeWithdrawnToL1 = await tokens.L2NativeWithdrawnToL1.withdraw(
            chainHandler,
            L2NativeWithdrawnToL1Amount
        );
        // Token native to L2-A, partially withdrawn to L1
        tokens.L2NativePartiallyWithdrawnToL1 = await ERC20Handler.deployTokenOnL2(chainHandler);
        withdrawalsToBeFinalized.L2NativePartiallyWithdrawnToL1 =
            await tokens.L2NativePartiallyWithdrawnToL1.withdraw(chainHandler);
        // Token native to L2-A, fully withdrawn to L1 but not finalized yet
        tokens.L2NativeUnfinalizedWithdrawalBeforeTBMToGW = await ERC20Handler.deployTokenOnL2(chainHandler);
        withdrawalsBeforeTBM.L2NativeUnfinalizedWithdrawalBeforeTBMToGW =
            await tokens.L2NativeUnfinalizedWithdrawalBeforeTBMToGW.withdraw(chainHandler, false);
        tokens.L2NativeUnfinalizedWithdrawalAfterTBMToGW = await ERC20Handler.deployTokenOnL2(chainHandler);
        withdrawalsAfterTBM.L2NativeUnfinalizedWithdrawalAfterTBMToGW =
            await tokens.L2NativeUnfinalizedWithdrawalAfterTBMToGW.withdraw(chainHandler, false);
        // Token native to L2-A, not withdrawn to L1 yet
        tokens.L2NativeNotWithdrawnToL1 = await ERC20Handler.deployTokenOnL2(chainHandler);

        // Token native to L2-B, withdrawn from L2-B, and deposited to L2-A
        const L2BToken = await ERC20Handler.deployTokenOnL2(customTokenChainHandler, RICH_WALLET_L1_BALANCE);
        const L2BTokenWithdrawal = await L2BToken.withdraw(customTokenChainHandler, true, RICH_WALLET_L1_BALANCE);

        // Finalize all needed withdrawals
        for (const tokenName of Object.keys(withdrawalsToBeFinalized)) {
            await withdrawalsToBeFinalized[tokenName].finalizeWithdrawal(chainRichWallet.ethWallet());
            // We can now define the L1 contracts for the tokens
            await tokens[tokenName].setL1Contract(chainHandler);
        }
        // Get the L1 contract for the L2-B token
        await L2BTokenWithdrawal.finalizeWithdrawal(chainRichWallet.ethWallet());
        const L2BTokenL1Contract = await L2BToken.getL1Contract(customTokenChainHandler);

        // Deposit L2-B token to L2-A
        tokens.L2BToken = await ERC20Handler.fromL2BL1Token(
            L2BTokenL1Contract,
            chainRichWallet,
            customTokenChainRichWallet
        );
        await tokens.L2BToken.deposit(chainHandler);

        // Add the base token to the list
        tokens.baseToken = new ERC20Handler(chainHandler.l2RichWallet, chainHandler.l1BaseTokenContract, undefined);
        const baseTokenAssetId = await tokens.baseToken.assetId(chainHandler);
        // Get the current balance of the base token on the chain for accounting purposes
        chainHandler.chainBalances[baseTokenAssetId] = await chainHandler.l1AssetTracker.chainBalance(
            chainHandler.inner.chainId,
            baseTokenAssetId
        );

        for (const tokenName of Object.keys(tokens)) {
            if (tokenName === 'L1NativeNotDepositedToL2') continue;
            console.log(`Token ${tokenName} Asset ID: ${await tokens[tokenName].assetId(chainHandler)}`);
        }
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

    it('Can finalize pending withdrawals', async () => {
        for (const tokenName of Object.keys(withdrawalsBeforeTBM)) {
            await withdrawalsBeforeTBM[tokenName].finalizeWithdrawal(chainRichWallet.ethWallet());
            const assetId = await tokens[tokenName].assetId(chainHandler);

            if (tokens[tokenName].isL2Token) {
                chainHandler.chainBalances[assetId] = ethers.MaxUint256 - withdrawalsBeforeTBM[tokenName].amount;
            } else {
                chainHandler.chainBalances[assetId] -= withdrawalsBeforeTBM[tokenName].amount;
            }

            delete withdrawalsBeforeTBM[tokenName];
        }
    });

    it('Can deposit a token to the chain after migrating to gateway', async () => {
        tokens.L1NativeDepositedToL2AfterMigrationToGW = await ERC20Handler.deployTokenOnL1(chainRichWallet);
        // Fresh deposit after the chain migrated to gateway marks the asset ID as effectively migrated
        await tokens.L1NativeDepositedToL2AfterMigrationToGW.deposit(chainHandler);
        const assetIdA = await tokens.L1NativeDepositedToL2AfterMigrationToGW.assetId(chainHandler);
        await expect(
            chainHandler.assertAssetTrackersState(assetIdA, {
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
        // Deposit existing tokens has the same result
        await tokens.L1NativeWithdrawnFromL2.deposit(chainHandler);
        const assetIdB = await tokens.L1NativeWithdrawnFromL2.assetId(chainHandler);
        await expect(
            chainHandler.assertAssetTrackersState(assetIdB, {
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
        await tokens.L1NativeNotDepositedToL2.deposit(chainHandler);
        const assetIdC = await tokens.L1NativeNotDepositedToL2.assetId(chainHandler);
        await expect(
            chainHandler.assertAssetTrackersState(assetIdC, {
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

    it('Cannot initiate migration for a false assetId', async () => {
        const bogusAssetId = ethers.randomBytes(32);
        await expect(chainHandler.l2AssetTracker.initiateL1ToGatewayMigrationOnL2(bogusAssetId)).rejects.toThrow();
    });

    it('Can migrate token balances to GW', async () => {
        for (const token of Object.keys(tokens)) {
            console.log(`Token ${token} Asset ID: ${await tokens[token].assetId(chainHandler)}`);
        }
        // Take snapshot right before migration
        // Base token balance increases slighly due to previous token deposits, here we account for that
        const existingBaseTokenL1ATBalanceForGW = await chainHandler.l1AssetTracker.chainBalance(
            GATEWAY_CHAIN_ID,
            chainHandler.baseTokenAssetId
        );
        chainHandler.existingBaseTokenL1ATBalanceForGW = existingBaseTokenL1ATBalanceForGW;
        // Migrate token balances to gateway
        // This also tests repeated migrations, as `L1NativeDepositedToL2AfterMigrationToGW` was already effectively migrated
        // This command tries to migrate it again, which will succeed, but later balance check will show it stays the same
        await chainHandler.migrateTokenBalancesToGateway();
        // We need to wait for a bit for L1AT's `_sendConfirmationToChains` to propagate to GW and the tested L2 chain
        await utils.sleep(5);
    });

    it('Correctly assigns chain token balances after migrating token balances to gateway', async () => {
        // Chain balances are accounted correctly on L1AT
        for (const tokenName of Object.keys(tokens)) {
            const assetId = await tokens[tokenName].assetId(chainHandler);
            if (assetId === ethers.ZeroHash) continue;

            const isL2Token = tokens[tokenName].isL2Token;
            const baseBalance = chainHandler.chainBalances[assetId] ?? (isL2Token ? ethers.MaxUint256 : 0n);
            const pending = withdrawalsAfterTBM[tokenName];
            const pendingAmount = pending?.amount ?? 0n;
            const gwExpected = pendingAmount > 0n ? baseBalance - pendingAmount : baseBalance;

            await expect(
                chainHandler.assertAssetTrackersState(assetId, {
                    balances: {
                        L1AT: pendingAmount,
                        L1AT_GW: gwExpected,
                        GWAT: gwExpected
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

    it('Can finalize pending withdrawals', async () => {
        for (const tokenName of Object.keys(withdrawalsAfterTBM)) {
            await withdrawalsAfterTBM[tokenName].finalizeWithdrawal(chainRichWallet.ethWallet());
            const assetId = await tokens[tokenName].assetId(chainHandler);

            if (tokens[tokenName].isL2Token) {
                chainHandler.chainBalances[assetId] = ethers.MaxUint256 - withdrawalsAfterTBM[tokenName].amount;
            } else {
                chainHandler.chainBalances[assetId] -= withdrawalsAfterTBM[tokenName].amount;
            }

            delete withdrawalsAfterTBM[tokenName];
        }
    });

    it('Can withdraw tokens from the chain', async () => {
        // Can fully withdraw existing tokens
        const withdrawalA = await tokens.L1NativeDepositedToL2.withdraw(chainHandler);
        const withdrawalB = await tokens.L1NativeNotDepositedToL2.withdraw(chainHandler);
        const withdrawalC = await tokens.L2BToken.withdraw(chainHandler);
        await withdrawalA.finalizeWithdrawal(chainRichWallet.ethWallet());
        await withdrawalB.finalizeWithdrawal(chainRichWallet.ethWallet());
        await withdrawalC.finalizeWithdrawal(chainRichWallet.ethWallet());
    });

    it('Can deposit a token to the chain after migrating balances to gateway', async () => {
        tokens.L1NativeDepositedToL2AfterTBMToGW = await ERC20Handler.deployTokenOnL1(chainRichWallet);
        // Fresh deposit after the chain migrated its token balances to gateway marks the asset ID as effectively migrated
        await tokens.L1NativeDepositedToL2AfterTBMToGW.deposit(chainHandler);
        const assetId = await tokens.L1NativeDepositedToL2AfterTBMToGW.assetId(chainHandler);
        await expect(
            chainHandler.assertAssetTrackersState(assetId, {
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
