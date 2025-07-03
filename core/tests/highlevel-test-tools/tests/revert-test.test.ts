// Test of the behaviour of the external node when L1 batches get reverted.
//
// NOTE:
// main_contract.getTotalBatchesCommitted actually checks the number of batches committed.
// main_contract.getTotalBatchesExecuted actually checks the number of batches executed.
import { beforeAll, describe, it } from 'vitest';
import { createChainAndStartServer, TESTED_CHAIN_TYPE, TestChain, getMainWalletPk } from '../src';
import * as utils from 'utils';
import {
    checkRandomTransfer,
    executeDepositAfterRevert,
    executeRevert,
    revertExternalNode,
    waitToCommitBatchesWithoutExecution
} from './revert-utils';
import * as zksync from 'zksync-ethers';
import * as ethers from 'ethers';
import { assert } from 'chai';
import {
    loadConfig,
    replaceL1BatchMinAgeBeforeExecuteSeconds,
    shouldLoadConfigFromFile
} from 'utils/build/file-configs';
import { logsTestPath } from 'utils/build/logs';
import { IZkSyncHyperchain, IZkSyncHyperchain__factory } from 'zksync-ethers/build/typechain';
import * as path from 'node:path';
import step from '../src/step';
import { Tester } from './revert-tester';
import { findHome } from '../src/zksync-home';
//
// describe('Revert Test', () => {
//     it(`for ${TESTED_CHAIN_TYPE} chain`, async () => {
//
//
//         await revertTest(chainName);
//     });
// });

const pathToHome = path.join(__dirname, '../../../..');
let chainName = undefined;

async function logsPath(name: string): Promise<string> {
    return await logsTestPath(chainName, 'logs/revert/en', name);
}

interface GatewayInfo {
    gatewayChainId: string;
    gatewayProvider: zksync.Provider;
    gatewayCTM: string;
    l2ChainAdmin: string;
    l2DiamondProxyAddress: string;
}

describe('Block reverting test', function () {
    let operatorAddress: string;
    let depositAmount: bigint;
    let externalNodeTester: Tester;
    let mainNodeTester: Tester;

    let testChain: TestChain;

    let gatewayInfo: GatewayInfo | null;
    let settlementLayerMainContract: IZkSyncHyperchain;
    let alice: zksync.Wallet;
    let batchesCommittedBeforeRevert: bigint;
    let targetBatchForRevert: bigint;
    let balanceBeforeRevert: bigint;

    function createTester(l1_rpc_addr: string, l2_rpc_addr: string) {
        const ethProvider = new ethers.JsonRpcProvider(l1_rpc_addr);
        ethProvider.pollingInterval = 100;

        const ethWallet = new ethers.Wallet(getMainWalletPk(chainName), ethProvider);

        const web3Provider = new zksync.Provider(l2_rpc_addr);
        web3Provider.pollingInterval = 100;
        const syncWallet = new zksync.Wallet(ethWallet.privateKey, web3Provider, ethProvider);

        return new Tester(
            ethProvider,
            ethWallet,
            syncWallet,
            web3Provider,
            !testChain.isCustomToken,
            testChain.baseTokenAddress
        );
    }

    beforeAll(async () => {
        testChain = await createChainAndStartServer(TESTED_CHAIN_TYPE, 'Revert Test');

        chainName = testChain.chainName;

        await testChain.generateRealisticLoad();

        await testChain.waitForAllBatchesToBeExecuted();

        await testChain.initExternalNode();

        await testChain.runExternalNode();

        console.log(`😴 Sleeping for 60 seconds before killing external node to wait for it to sync..`);
        await new Promise((resolve) => setTimeout(resolve, 60000));

        let ethClientWeb3Url: string;
        let apiWeb3JsonRpcHttpUrl: string;
        let enEthClientUrl: string;

        const secretsConfig = loadConfig({ pathToHome, chain: chainName, config: 'secrets.yaml' });
        const generalConfig = loadConfig({ pathToHome, chain: chainName, config: 'general.yaml' });
        const contractsConfig = loadConfig({ pathToHome, chain: chainName, config: 'contracts.yaml' });
        const externalNodeGeneralConfig = loadConfig({
            pathToHome,
            configsFolderSuffix: 'external_node',
            chain: chainName,
            config: 'general.yaml'
        });
        const walletsConfig = loadConfig({ pathToHome, chain: chainName, config: 'wallets.yaml' });

        ethClientWeb3Url = secretsConfig.l1.l1_rpc_url;
        apiWeb3JsonRpcHttpUrl = generalConfig.api.web3_json_rpc.http_url;
        enEthClientUrl = externalNodeGeneralConfig.api.web3_json_rpc.http_url;
        operatorAddress = walletsConfig.operator.address;

        mainNodeTester = createTester(ethClientWeb3Url, apiWeb3JsonRpcHttpUrl);
        externalNodeTester = createTester(ethClientWeb3Url, enEthClientUrl);

        depositAmount = ethers.parseEther('0.001');
        gatewayInfo = getGatewayInfo(pathToHome, chainName);

        const l1Provider = new ethers.JsonRpcProvider(ethClientWeb3Url);

        settlementLayerMainContract = gatewayInfo
            ? IZkSyncHyperchain__factory.connect(gatewayInfo.l2DiamondProxyAddress, gatewayInfo.gatewayProvider)
            : IZkSyncHyperchain__factory.connect(contractsConfig.l1.diamond_proxy_addr, l1Provider);
    });

    it('EN revert test', async () => {
        console.log('Funding wallets');
        await mainNodeTester.fundSyncWallet();
        await externalNodeTester.fundSyncWallet();
        alice = externalNodeTester.emptyWallet();

        console.log('Sealing L1 batch and waiting for it to be executed');
        await externalNodeTester.createBatchWithDeposit(alice.address, depositAmount);
        await testChain.waitForAllBatchesToBeExecuted();

        console.log('Restarting the node with different L1BatchMinAgeBeforeExecute');
        await testChain.mainNode.kill();
        replaceL1BatchMinAgeBeforeExecuteSeconds(findHome(), chainName, 10000);
        await testChain.runMainNode();

        console.log('Sealing another L1 batch');
        await externalNodeTester.createBatchWithDeposit(alice.address, depositAmount);

        balanceBeforeRevert = await alice.getBalance();
        utils.log(`Balance before revert: ${balanceBeforeRevert}`);
        assert(balanceBeforeRevert === depositAmount * 2n, 'Incorrect balance after deposits');

        batchesCommittedBeforeRevert = await waitToCommitBatchesWithoutExecution(settlementLayerMainContract);
        await testChain.mainNode.kill();

        testChain.externalNode.markAsExpectingErrorCode(1);
        targetBatchForRevert = await executeRevert(
            pathToHome,
            chainName,
            operatorAddress,
            batchesCommittedBeforeRevert,
            settlementLayerMainContract
        );

        replaceL1BatchMinAgeBeforeExecuteSeconds(findHome(), chainName, 0);
        await testChain.runMainNode();
        await testChain.externalNode.waitForExitWithErrorCode();

        for (let stepIndex = 0; stepIndex < 2; stepIndex++) {
            await step('Restart EN', async () => {
                await testChain.runExternalNode();
            });

            await step('wait until last deposit is re-executed', async () => {
                let balance;
                let tryCount = 0;
                while ((balance = await alice.getBalance()) !== balanceBeforeRevert && tryCount < 30) {
                    utils.log(`Balance after revert: ${balance} (expecting: ${balanceBeforeRevert})`);
                    tryCount++;
                    await utils.sleep(1);
                }
                assert(balance === balanceBeforeRevert, 'Incorrect balance after revert');
            });

            await step('execute transaction after revert', async () => {
                await executeDepositAfterRevert(externalNodeTester, alice, depositAmount);
                const balanceAfter = await alice.getBalance();
                utils.log(`Balance after another deposit: ${balanceAfter}`);
                assert(balanceAfter === balanceBeforeRevert + depositAmount, 'Incorrect balance after another deposit');
            });

            await step('check random transfer', async () => {
                balanceBeforeRevert = await checkRandomTransfer(alice, 1n);
                utils.log('Balance after transfer', balanceBeforeRevert);
            });

            if (stepIndex === 0) {
                await step('Terminate external node', async () => {
                    await testChain.externalNode.kill();
                });

                await step('revert external node via command', async () => {
                    await revertExternalNode(chainName, targetBatchForRevert);
                });
            }
        }
    });
});

function getGatewayInfo(pathToHome: string, chain: string): GatewayInfo | null {
    const gatewayChainConfig = loadConfig({
        pathToHome,
        chain,
        config: 'gateway_chain.yaml'
    });

    if (!gatewayChainConfig) {
        return null;
    }

    if (gatewayChainConfig.gateway_chain_id) {
        const secretsConfig = loadConfig({
            pathToHome,
            chain,
            config: 'secrets.yaml'
        });

        return {
            gatewayChainId: gatewayChainConfig.gateway_chain_id,
            gatewayProvider: new zksync.Provider(secretsConfig.l1.gateway_rpc_url),
            gatewayCTM: gatewayChainConfig.state_transition_proxy_addr,
            l2ChainAdmin: gatewayChainConfig.chain_admin_addr,
            l2DiamondProxyAddress: gatewayChainConfig.diamond_proxy_addr
        };
    }

    return null;
}
