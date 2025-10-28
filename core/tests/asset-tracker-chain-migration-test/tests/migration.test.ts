import * as utils from 'utils';
import { L1ERC20Handler, L2ERC20Handler, Tester, WithdrawalHandler } from './tester';
import * as zksync from 'zksync-ethers';
import * as ethers from 'ethers';
import { expect } from 'chai';
import fs from 'node:fs/promises';
import { existsSync, readFileSync } from 'node:fs';
import { BytesLike } from '@ethersproject/bytes';
import { BigNumberish } from 'ethers';
import { loadConfig, shouldLoadConfigFromFile } from 'utils/build/file-configs';
import {
    Contracts,
    initContracts,
    setAggregatedBlockExecuteDeadline,
    setAggregatedBlockProveDeadline,
    setBlockCommitDeadlineMs,
    setEthSenderSenderAggregatedBlockCommitDeadline
} from './utils';
import path from 'path';
import { CONTRACT_DEPLOYER, CONTRACT_DEPLOYER_ADDRESS, hashBytecode, ZKSYNC_MAIN_ABI } from 'zksync-ethers/build/utils';
import { utils as zksync_utils } from 'zksync-ethers';
import { logsTestPath } from 'utils/build/logs';
import { waitForNewL1Batch } from 'utils';
import { getMainWalletPk } from 'highlevel-test-tools/src/wallets';



import {ChainHandler} from "./tester";


async function logsPath(name: string): Promise<string> {
    return await logsTestPath(fileConfig.chain, 'logs/upgrade/', name);
}

const L2_BRIDGEHUB_ADDRESS = '0x0000000000000000000000000000000000010002';
const pathToHome = path.join(__dirname, '../../../..');
const fileConfig = shouldLoadConfigFromFile();

const contracts: Contracts = initContracts(pathToHome, fileConfig.loadFromFile);

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

// Returns a wallet that is both rich on L1 and on GW
async function prepareRichWallet(): Promise<zksync.Wallet> {
    const generalConfig = loadConfig({
        pathToHome,
        chain: GATEWAY_CHAIN_NAME,
        config: 'general.yaml'
    });
    const contractsConfig = loadConfig({
        pathToHome,
        chain: GATEWAY_CHAIN_NAME,
        config: 'contracts.yaml'
    });
    const secretsConfig = loadConfig({
        pathToHome,
        chain: GATEWAY_CHAIN_NAME,
        config: 'secrets.yaml'
    });
    const ethProviderAddress = secretsConfig.l1.l1_rpc_url;
    const web3JsonRpc = generalConfig.api.web3_json_rpc.http_url;

    const richWallet = new zksync.Wallet(getMainWalletPk('gateway'), new zksync.Provider(web3JsonRpc), new ethers.JsonRpcProvider(ethProviderAddress));

    // We assume that Gateway has "ETH" as the base token.
    // We deposit funds to ensure that the wallet is rich
    await (await richWallet.deposit({
        token: zksync.utils.ETH_ADDRESS_IN_CONTRACTS,
        amount: ethers.parseEther('10.0')
    })).wait();

    return richWallet;
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

describe('Asset tracker migration test', function () {
    let ethChainHandler: ChainHandler;
    let erc20ChainHandler: ChainHandler;

    let l1RichWallet: ethers.Wallet;
    let gwRichWallet: zksync.Wallet;

    let ethChainTokenPreBridged: L2ERC20Handler;
    let ethChainTokenNotPreBridged: L2ERC20Handler;
    let ethChainTokenUnfinalizedWithdrawalHandler: WithdrawalHandler;
    let l1NativeToken: L1ERC20Handler;
    let l1NativeTokenPreBridged: L1ERC20Handler;
    let l1NativeToken2: L1ERC20Handler;

    let erc20ChainTokenPreBridged: L2ERC20Handler;
    let erc20ChainTokenNotPreBridged: L2ERC20Handler;

    before('Setup the system', async function () {
        console.log('Initializing rich wallet...');
        gwRichWallet = await prepareRichWallet();
        l1RichWallet = gwRichWallet.ethWallet();

        console.log('Creating a new chain 1...');
        ethChainHandler = await ChainHandler.createNewChain('era');
        // FIXME: not erc20
        console.log('Creating a new chain 2...');
        erc20ChainHandler = await ChainHandler.createNewChain('era');
    });

    test('TMP spawn tokens', async function() {
        // ethChainTokenPreBridged = await L2ERC20Handler.deployToken(ethChainHandler.l2RichWallet);
        // ethChainTokenNotPreBridged = await L2ERC20Handler.deployToken(ethChainHandler.l2RichWallet);
        
        // l1NativeToken = await L1ERC20Handler.deployToken(l1RichWallet);
        // l1NativeTokenPreBridged = await L1ERC20Handler.deployToken(l1RichWallet);
        // l1NativeToken2 = await L1ERC20Handler.deployToken(l1RichWallet);

        // erc20ChainTokenPreBridged = await L2ERC20Handler.deployToken(erc20ChainHandler.l2RichWallet);
        // erc20ChainTokenNotPreBridged = await L2ERC20Handler.deployToken(erc20ChainHandler.l2RichWallet);
    })

    // test('Bridge tokens', async function () {
    //     const withdrawalHandler1 = await ethChainTokenPreBridged.withdraw();

    //     // For now it will be unfinalized, we'll use it later.
    //     ethChainTokenUnfinalizedWithdrawalHandler = await ethChainTokenNotPreBridged.withdraw();
        
    //     await l1NativeTokenPreBridged.deposit(ethChainHandler);
    //     await l1NativeTokenPreBridged.deposit(erc20ChainHandler);

    //     await withdrawalHandler1.finalizeWithdrawal(l1RichWallet);
    // })

    // test('Migration of balances to GW', async function () {
    //     await Promise.all([
    //         ethChainHandler.migrateToGateway(),
    //         erc20ChainHandler.migrateToGateway()
    //     ]);

    //     const l2VersionTokenPreBridged = await l1NativeTokenPreBridged.atL2SameWallet(ethChainHandler);

    //     // const l1Native

    //     // Each of the below should Fail
    //     ethChainTokenPreBridged.withdraw();
    //     ethChainTokenNotPreBridged.withdraw();


    //     // should fail
    //     l2VersionTokenPreBridged.withdraw();
    //     // should also fail
    //     ethChainTokenUnfinalizedWithdrawalHandler.finalizeWithdrawal(l1RichWallet);
        
    //     // We migrate the tokens two times. This is to 
    //     // demonstrate that it is possible to call the migration again 
    //     // and the handlers will still work.
    //     const migrationHandlers1 = [
    //         await ethChainTokenPreBridged.migrateBalanceL2ToGW(),
    //         await ethChainTokenNotPreBridged.migrateBalanceL2ToGW(),
    //         await l2VersionTokenPreBridged.migrateBalanceL2ToGW()
    //     ];
    //     const migrationHandlers2 = [
    //         await ethChainTokenPreBridged.migrateBalanceL2ToGW(),
    //         await ethChainTokenNotPreBridged.migrateBalanceL2ToGW(),
    //         await l2VersionTokenPreBridged.migrateBalanceL2ToGW()
    //     ];

    //     // Sometimes we use migrationHandlers1, sometimes migrationHandlers 2,
    //     // these should be equivalent. 
    //     // TODO: maybe check for actual equivalence of messages.
    //     await migrationHandlers1[0].finalizeMigration(l1RichWallet);
    //     await migrationHandlers2[1].finalizeMigration(l1RichWallet);
    //     await migrationHandlers1[0].finalizeMigration(l1RichWallet);

    //     // Now all the below should succeed:
    //     // TODO: actually check that these withdrawals will finalize fine.
    //     // We should also spawn a withdrawal to be finalized after the chain has moved away from GW.
    //     await ethChainTokenPreBridged.withdraw();
    //     await ethChainTokenNotPreBridged.withdraw();
    //     await l2VersionTokenPreBridged.withdraw();
    //     ethChainTokenUnfinalizedWithdrawalHandler.finalizeWithdrawal(l1RichWallet);
    // });

    // test('Test receiving interop for migrated assets', async function () {
    //     // TODO
    // })

    // test('Test automatic registration', async function () {
    //     await l1NativeToken.deposit(ethChainHandler);
    //     // We dont withdraw it yet, we'll withdraw it after we migrate to L1.
    //     await l1NativeToken2.deposit(ethChainHandler);
    //     const l2Repr = await l1NativeToken.atL2SameWallet(ethChainHandler);

    //     // should succeed
    //     const withdrawHandle = await l2Repr.withdraw();
    //     await withdrawHandle.finalizeWithdrawal(l1RichWallet);

    //     // TODO: dont forget to check asset migrtion number.
    // });

    // test('Migrate back to L1', async function () {
    //     await ethChainHandler.migrateFromGateway();
        
    //     const l2Token = await l1NativeToken2.atL2SameWallet(ethChainHandler);
    //     const withdrawHandler = await l2Token.withdraw();

    //     // should fail, since the chain has not balance.
    //     await withdrawHandler.finalizeWithdrawal(l1RichWallet);

    //     await l2Token.migrateBalanceGWtoL1(gwRichWallet);

    //     // Should succeed 
    //     await withdrawHandler.finalizeWithdrawal(l1RichWallet);

    //     // todo: test the ability to migrate all of the tokens' balances to the chain on L1.

    //     // todo: test that all of the withdrawn tokens can be withdrawn and finalized.
    // })

    after('shutdown', async function () {
        console.log('Tearing down chains...');
        await ethChainHandler.stopServer();
        await erc20ChainHandler.stopServer();
        console.log('Complete');
    });
});
