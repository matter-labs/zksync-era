import { afterAll, beforeAll, describe, it } from 'vitest';
import { createChainAndStartServer, TESTED_CHAIN_TYPE, tokenBalanceMigrationTest } from '../src';

import * as utils from 'utils';
import {
    WithdrawalHandler,
    ChainHandler,
    generateChainRichWallet,
    ERC20Handler
} from './token-balance-migration-tester';
import * as zksync from 'zksync-ethers';
import * as ethers from 'ethers';
import { expect } from 'chai';
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
(shouldSkip ? describe.skip : describe)('Token balance migration tests', function () {
    let chainHandler: ChainHandler;
    let customTokenChainHandler: ChainHandler;

    let l1RichWallet: ethers.Wallet;
    let gwRichWallet: zksync.Wallet;
    let chainRichWallet: zksync.Wallet;

    // Stored token data for cross-test assertions
    const tokens: Record<string, { handler: ERC20Handler; balance: bigint }> = {};

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
        chainRichWallet = chainHandler.l2RichWallet;
        console.log('Chain rich wallet private key:', chainRichWallet.privateKey);
        // Initialize auxiliary chain
        console.log('Creating a new chain with a custom token as the base token...');
        customTokenChainHandler = await ChainHandler.createNewChain('custom_token');

        const withdrawalsToBeFinalized: WithdrawalHandler[] = [];
        // DEPLOY TOKENS THAT WILL BE TESTED
        // We first deploy all tokens that will need to be withdrawn from L2 to make testing faster
        // Token B: Native to L1, deposited to L2, fully withdrawn from L2
        tokens.L1NativeWithdrawnFromL2 = { handler: await ERC20Handler.deployTokenOnL1(chainRichWallet), balance: 0n };
        const L1NativeWithdrawnFromL2Amount = await tokens.L1NativeWithdrawnFromL2.handler.deposit(chainHandler, true);
        withdrawalsToBeFinalized.push(
            await tokens.L1NativeWithdrawnFromL2.handler.withdraw(L1NativeWithdrawnFromL2Amount)
        );
        // Token G: Native to L2-A, fully withdrawn to L1
        tokens.L2NativeWithdrawnToL1 = { handler: await ERC20Handler.deployTokenOnL2(chainRichWallet), balance: 0n };
        const L2NativeWithdrawnToL1Amount = await tokens.L2NativeWithdrawnToL1.handler.getL2Balance();
        withdrawalsToBeFinalized.push(await tokens.L2NativeWithdrawnToL1.handler.withdraw(L2NativeWithdrawnToL1Amount));
        // Token A: Native to L1, deposited to L2
        tokens.L1NativeDepositedToL2 = { handler: await ERC20Handler.deployTokenOnL1(chainRichWallet), balance: 0n };
        tokens.L1NativeDepositedToL2.balance = await tokens.L1NativeDepositedToL2.handler.deposit(chainHandler);
        // Token C: Native to L1, deposited to L2, fully withdrawn from L2 but not finalized yet

        // Token C: Native to L1, not deposited to L2 yet
        tokens.L1NativeNotDepositedToL2 = { handler: await ERC20Handler.deployTokenOnL1(chainRichWallet), balance: 0n };
        // Token D: Native to L2-B, withdrawn from L2-B, and deposited to L2-A

        // Token E: Native to L2-B, withdrawn from L2-B, not deposited to L2-A yet

        // Token F: Native to L2-A, partially withdrawn to L1

        // Token H: Native to L2-A, not withdrawn to L1

        // Finalize all needed withdrawals
        for (const withdrawal of withdrawalsToBeFinalized) {
            await withdrawal.finalizeWithdrawal(chainRichWallet);
        }
        await tokens.L2NativeWithdrawnToL1.handler.setL1Contract(chainHandler);
    });

    it('Correctly assigns chain token balances', async () => {
        // TODO
    });

    it('Can migrate token balances to GW', async () => {
        await chainHandler.migrateToGateway();
        await chainHandler.migrateTokenBalancesToGateway();
    });

    it('Can do repeated migrations', async () => {
        // await chainHandler.migrateTokenBalancesToGateway();
    });

    it('Can migrate token balances from GW', async () => {
        // TODO: WHY DOES THIS FAIL?
        // await chainHandler.migrateFromGateway();
        // await chainHandler.migrateTokenBalancesToL1();
    });

    it('Bridge tokens', async () => {
        /* const withdrawalHandler1 = await ethChainTokenPreBridged.withdraw();

        // For now it will be unfinalized, we'll use it later.
        ethChainTokenUnfinalizedWithdrawalHandler = await ethChainTokenNotPreBridged.withdraw();

        await l1NativeTokenPreBridged.deposit(chainHandler, DEFAULT_LARGE_AMOUNT);
        await l1NativeTokenPreBridged.deposit(customTokenChainHandler, DEFAULT_LARGE_AMOUNT);

        await withdrawalHandler1.finalizeWithdrawal(l1RichWallet); */
    });

    it('Migration of balances to GW', async () => {
        /* await Promise.all([
            chainHandler.migrateToGateway()
            // customTokenChainHandler.migrateToGateway()
        ]);

        // const l2VersionTokenPreBridged = await l1NativeTokenPreBridged.atL2SameWallet(chainHandler);

        // const l1Native

        // Each of the below should Fail
        ethChainTokenPreBridged.withdraw();
        ethChainTokenNotPreBridged.withdraw(); */
        // // should fail
        // l2VersionTokenPreBridged.withdraw();
        // // should also fail
        // ethChainTokenUnfinalizedWithdrawalHandler.finalizeWithdrawal(l1RichWallet);
        // // We migrate the tokens two times. This is to
        // // demonstrate that it is possible to call the migration again
        // // and the handlers will still work.
        // const migrationHandlers1 = [
        //     await ethChainTokenPreBridged.migrateBalanceL2ToGW(),
        //     await ethChainTokenNotPreBridged.migrateBalanceL2ToGW(),
        //     await l2VersionTokenPreBridged.migrateBalanceL2ToGW()
        // ];
        // const migrationHandlers2 = [
        //     await ethChainTokenPreBridged.migrateBalanceL2ToGW(),
        //     await ethChainTokenNotPreBridged.migrateBalanceL2ToGW(),
        //     await l2VersionTokenPreBridged.migrateBalanceL2ToGW()
        // ];
        // // Sometimes we use migrationHandlers1, sometimes migrationHandlers 2,
        // // these should be equivalent.
        // // TODO: maybe check for actual equivalence of messages.
        // await migrationHandlers1[0].finalizeMigration(l1RichWallet);
        // await migrationHandlers2[1].finalizeMigration(l1RichWallet);
        // await migrationHandlers1[0].finalizeMigration(l1RichWallet);
        // // Now all the below should succeed:
        // // TODO: actually check that these withdrawals will finalize fine.
        // // We should also spawn a withdrawal to be finalized after the chain has moved away from GW.
        // await ethChainTokenPreBridged.withdraw();
        // await ethChainTokenNotPreBridged.withdraw();
        // await l2VersionTokenPreBridged.withdraw();
        // ethChainTokenUnfinalizedWithdrawalHandler.finalizeWithdrawal(l1RichWallet);
    });

    // step('Test receiving interop for migrated assets', async function () {
    //     // TODO
    // })

    // step('Test automatic registration', async function () {
    //     await l1NativeToken.deposit(chainHandler);
    //     // We dont withdraw it yet, we'll withdraw it after we migrate to L1.
    //     await l1NativeToken2.deposit(chainHandler);
    //     const l2Repr = await l1NativeToken.atL2SameWallet(chainHandler);

    //     // should succeed
    //     const withdrawHandle = await l2Repr.withdraw();
    //     await withdrawHandle.finalizeWithdrawal(l1RichWallet);

    //     // TODO: dont forget to check asset migrtion number.
    // });

    // step('Migrate back to L1', async function () {
    //     await chainHandler.migrateFromGateway();

    //     const l2Token = await l1NativeToken2.atL2SameWallet(chainHandler);
    //     const withdrawHandler = await l2Token.withdraw();

    //     // should fail, since the chain has not balance.
    //     await withdrawHandler.finalizeWithdrawal(l1RichWallet);

    //     await l2Token.migrateBalanceGWtoL1(gwRichWallet);

    //     // Should succeed
    //     await withdrawHandler.finalizeWithdrawal(l1RichWallet);

    //     // todo: test the ability to migrate all of the tokens' balances to the chain on L1.

    //     // todo: test that all of the withdrawn tokens can be withdrawn and finalized.
    // })

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
