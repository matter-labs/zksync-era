// Test of the behaviour of the external node when L1 batches get reverted.
//
// NOTE:
// main_contract.getTotalBatchesCommitted actually checks the number of batches committed.
// main_contract.getTotalBatchesExecuted actually checks the number of batches executed.
import * as utils from 'utils';
import {
    checkRandomTransfer,
    executeDepositAfterRevert,
    executeRevert,
    Node,
    NodeSpawner,
    NodeType,
    revertExternalNode,
    waitToCommitBatchesWithoutExecution,
    waitToExecuteBatch
} from './utils';
import * as zksync from 'zksync-ethers';
import * as ethers from 'ethers';
import { assert } from 'chai';
import fs from 'node:fs/promises';
import {
    loadConfig,
    replaceL1BatchMinAgeBeforeExecuteSeconds,
    shouldLoadConfigFromFile
} from 'utils/build/file-configs';
import path from 'path';
import { logsTestPath } from 'utils/build/logs';
import { IZkSyncHyperchain, IZkSyncHyperchain__factory } from 'zksync-ethers/build/typechain';

const pathToHome = path.join(__dirname, '../../../..');
const chainName = shouldLoadConfigFromFile().chain;

if (!chainName) {
    throw new Error('`CHAIN_NAME` env variable is required');
}

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
    let mainNodeSpawner: NodeSpawner;
    let mainNode: Node<NodeType.MAIN>;
    let extNodeSpawner: NodeSpawner;
    let extNode: Node<NodeType.EXT>;
    let gatewayInfo: GatewayInfo | null;
    let settlementLayerMainContract: IZkSyncHyperchain;
    let alice: zksync.Wallet;
    let depositL1BatchNumber: number;
    let batchesCommittedBeforeRevert: bigint;
    let targetBatchForRevert: bigint;

    const autoKill: boolean = !process.env.NO_KILL;

    before('initialize test', async () => {
        let ethClientWeb3Url: string;
        let apiWeb3JsonRpcHttpUrl: string;
        let baseTokenAddress: string;
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
        baseTokenAddress = contractsConfig.l1.base_token_addr;
        enEthClientUrl = externalNodeGeneralConfig.api.web3_json_rpc.http_url;
        operatorAddress = walletsConfig.operator.address;

        const pathToMainLogs = await logsPath('server.log');
        const mainLogs = await fs.open(pathToMainLogs, 'a');
        console.log(`Writing main node logs to ${pathToMainLogs}`);

        const pathToEnLogs = await logsPath('external_node.log');
        const extLogs = await fs.open(pathToEnLogs, 'a');
        console.log(`Writing EN logs to ${pathToEnLogs}`);

        const enableConsensus = process.env.ENABLE_CONSENSUS === 'true';
        console.log(`enableConsensus = ${enableConsensus}`);
        depositAmount = ethers.parseEther('0.001');

        const mainNodeSpawnOptions = {
            enableConsensus,
            ethClientWeb3Url,
            apiWeb3JsonRpcHttpUrl,
            baseTokenAddress
        };
        mainNodeSpawner = new NodeSpawner(pathToHome, mainLogs, chainName, mainNodeSpawnOptions);
        const extNodeSpawnOptions = {
            enableConsensus,
            ethClientWeb3Url,
            apiWeb3JsonRpcHttpUrl: enEthClientUrl,
            baseTokenAddress
        };
        extNodeSpawner = new NodeSpawner(pathToHome, extLogs, chainName, extNodeSpawnOptions);

        gatewayInfo = getGatewayInfo(pathToHome, chainName);

        const l1Provider = new ethers.JsonRpcProvider(ethClientWeb3Url);

        settlementLayerMainContract = gatewayInfo
            ? IZkSyncHyperchain__factory.connect(gatewayInfo.l2DiamondProxyAddress, gatewayInfo.gatewayProvider)
            : IZkSyncHyperchain__factory.connect(contractsConfig.l1.diamond_proxy_addr, l1Provider);
    });

    step('Make sure that nodes are not running', async () => {
        if (autoKill) {
            await Node.killAll(NodeType.MAIN);
            await Node.killAll(NodeType.EXT);
        }
    });

    step('Start main node', async () => {
        mainNode = await mainNodeSpawner.spawnMainNode(true);
    });

    step('Start external node', async () => {
        extNode = await extNodeSpawner.spawnExtNode();
    });

    step('Fund wallets', async () => {
        await mainNode.tester.fundSyncWallet();
        await extNode.tester.fundSyncWallet();
        alice = extNode.tester.emptyWallet();
    });

    step('Seal L1 batch', async () => {
        depositL1BatchNumber = await extNode.createBatchWithDeposit(alice.address, depositAmount);
    });

    step('wait for L1 batch to get executed', async () => {
        await waitToExecuteBatch(settlementLayerMainContract, depositL1BatchNumber);
    });

    step('Restart main node with batch execution turned off', async () => {
        await mainNode.killAndWaitForShutdown();
        mainNode = await mainNodeSpawner.spawnMainNode(false);
    });

    step('seal another L1 batch', async () => {
        await extNode.createBatchWithDeposit(alice.address, depositAmount);
    });

    step('check wallet balance', async () => {
        const balance = await alice.getBalance();
        console.log(`Balance before revert: ${balance}`);
        assert(balance === depositAmount * 2n, 'Incorrect balance after deposits');
    });

    step('wait for the new batch to be committed', async () => {
        batchesCommittedBeforeRevert = await waitToCommitBatchesWithoutExecution(settlementLayerMainContract);
    });

    step('stop server', async () => {
        await mainNode.killAndWaitForShutdown();
    });

    step('revert batches', async () => {
        targetBatchForRevert = await executeRevert(
            pathToHome,
            chainName,
            operatorAddress,
            batchesCommittedBeforeRevert,
            settlementLayerMainContract
        );
    });

    step('restart server', async () => {
        mainNode = await mainNodeSpawner.spawnMainNode(true);
    });

    step('Wait for EN to detect reorg and terminate', async () => {
        await extNode.waitForExit();
    });

    for (let stepIndex = 0; stepIndex < 2; stepIndex++) {
        step('Restart EN', async () => {
            extNode = await extNodeSpawner.spawnExtNode();
        });

        step('wait until last deposit is re-executed', async () => {
            let balanceBefore;
            let tryCount = 0;
            while ((balanceBefore = await alice.getBalance()) !== 2n * depositAmount && tryCount < 30) {
                console.log(`Balance after revert: ${balanceBefore}`);
                tryCount++;
                await utils.sleep(1);
            }
            assert(balanceBefore === 2n * depositAmount, 'Incorrect balance after revert');
        });

        step('execute transaction after revert', async () => {
            await executeDepositAfterRevert(extNode.tester, alice, depositAmount);
            const balanceAfter = await alice.getBalance();
            console.log(`Balance after another deposit: ${balanceAfter}`);
            assert(balanceAfter === depositAmount * 3n, 'Incorrect balance after another deposit');
        });

        step('check random transfer', async () => {
            await checkRandomTransfer(alice, 1n);
        });

        if (stepIndex === 0) {
            step('Terminate external node', async () => {
                await extNode.terminate();
            });

            step('revert external node via command', async () => {
                await revertExternalNode(chainName, targetBatchForRevert);
            });
        }
    }

    after('terminate nodes', async () => {
        await mainNode.terminate();
        await extNode.terminate();
        replaceL1BatchMinAgeBeforeExecuteSeconds(pathToHome, chainName, 0);
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
