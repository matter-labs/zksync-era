import * as utils from 'utils';
import { Tester } from './tester';
import * as zksync from 'zksync-ethers';
import * as ethers from 'ethers';
import { expect } from 'chai';
import fs from 'node:fs/promises';
import { readFileSync } from 'node:fs';
import { ZeroAddress } from 'ethers';
import { loadConfig, shouldLoadConfigFromFile } from 'utils/build/file-configs';
import path from 'path';
import { logsTestPath } from 'utils/build/logs';
import { getEcosystemContracts } from 'utils/build/tokens';
import { getMainWalletPk } from 'highlevel-test-tools/src/wallets';

async function logsPath(name: string): Promise<string> {
    return await logsTestPath(fileConfig.chain, 'logs/migration/', name);
}

const pathToHome = path.join(__dirname, '../../../..');
const fileConfig = shouldLoadConfigFromFile();

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

describe('Migration from gateway test', function () {
    // Utility wallets for facilitating testing
    let tester: Tester;
    let alice: zksync.Wallet;

    // The diamond proxy contract on the settlement layer.
    let l1MainContract: ethers.Contract;
    let gwMainContract: ethers.Contract;
    let chainGatewayContract: ethers.Contract;

    let gatewayChain: string;
    let logs: fs.FileHandle;
    let direction: string;
    let ethProviderAddress: string | undefined;
    let web3JsonRpc: string | undefined;

    let mainNodeSpawner: utils.NodeSpawner;

    before('Create test wallet', async () => {
        direction = process.env.DIRECTION || 'TO';
        logs = await fs.open(await logsPath(`migration_${direction}.log`), 'a');
        console.log(`Start Migration ${direction} gateway`);
        gatewayChain = process.env.GATEWAY_CHAIN || 'gateway';

        if (!fileConfig.loadFromFile) {
            throw new Error('Non file based not supported');
        }

        const generalConfig = loadConfig({
            pathToHome,
            chain: fileConfig.chain,
            config: 'general.yaml'
        });
        const contractsConfig = loadConfig({
            pathToHome,
            chain: fileConfig.chain,
            config: 'contracts.yaml'
        });
        const chainGatewayConfig = loadConfig({
            pathToHome,
            chain: fileConfig.chain,
            config: 'gateway_chain.yaml'
        });
        const secretsConfig = loadConfig({
            pathToHome,
            chain: fileConfig.chain,
            config: 'secrets.yaml'
        });
        ethProviderAddress = secretsConfig.l1.l1_rpc_url;
        web3JsonRpc = generalConfig.api.web3_json_rpc.http_url;

        mainNodeSpawner = new utils.NodeSpawner(pathToHome, logs, fileConfig, {
            enableConsensus: false,
            ethClientWeb3Url: ethProviderAddress!,
            apiWeb3JsonRpcHttpUrl: web3JsonRpc!,
            baseTokenAddress: contractsConfig.l1.base_token_addr
        });

        await mainNodeSpawner.killAndSpawnMainNode();

        const gatewayRpcUrl = secretsConfig.l1.gateway_rpc_url;
        tester = await Tester.init(ethProviderAddress!, web3JsonRpc!, gatewayRpcUrl!);
        alice = tester.emptyWallet();

        l1MainContract = new ethers.Contract(
            contractsConfig.l1.diamond_proxy_addr,
            ZK_CHAIN_INTERFACE,
            tester.syncWallet.providerL1
        );

        const gatewayContractsConfig = loadConfig({
            pathToHome,
            chain: gatewayChain,
            config: 'contracts.yaml'
        });
        const gwMainContractsAddress = gatewayContractsConfig!.l1.diamond_proxy_addr;
        gwMainContract = new ethers.Contract(gwMainContractsAddress, ZK_CHAIN_INTERFACE, tester.syncWallet.providerL1);

        chainGatewayContract = new ethers.Contract(
            chainGatewayConfig.diamond_proxy_addr,
            ZK_CHAIN_INTERFACE,
            tester.gwProvider
        );
    });

    step('Run server and execute some transactions', async () => {
        let blocksCommitted = await l1MainContract.getTotalBatchesCommitted();

        const initialL1BatchNumber = await tester.web3Provider.getL1BatchNumber();

        const baseToken = await tester.syncWallet.provider.getBaseTokenContractAddress();

        if (!zksync.utils.isAddressEq(baseToken, zksync.utils.ETH_ADDRESS_IN_CONTRACTS)) {
            await (await tester.syncWallet.approveERC20(baseToken, ethers.MaxUint256)).wait();
            await mintToAddress(baseToken, tester.ethWallet, tester.syncWallet.address, depositAmount * 10n);
        }

        const firstDepositHandle = await tester.syncWallet.deposit({
            token: baseToken,
            amount: depositAmount,
            to: alice.address
        });
        await firstDepositHandle.wait();
        while ((await tester.web3Provider.getL1BatchNumber()) <= initialL1BatchNumber) {
            await utils.sleep(1);
        }

        const secondDepositHandle = await tester.syncWallet.deposit({
            token: baseToken,
            amount: depositAmount,
            to: alice.address
        });
        await secondDepositHandle.wait();
        while ((await tester.web3Provider.getL1BatchNumber()) <= initialL1BatchNumber + 1) {
            await utils.sleep(1);
        }

        const balance = await alice.getBalance();
        expect(balance === depositAmount * 2n, 'Incorrect balance after deposits').to.be.true;

        const tokenDetails = tester.token;
        const l1Erc20ABI = ['function mint(address to, uint256 amount)'];
        const l1Erc20Contract = new ethers.Contract(tokenDetails.address, l1Erc20ABI, tester.ethWallet);
        await (await l1Erc20Contract.mint(tester.syncWallet.address, depositAmount)).wait();

        const thirdDepositHandle = await tester.syncWallet.deposit({
            token: tokenDetails.address,
            amount: depositAmount,
            approveERC20: true,
            approveBaseERC20: true,
            to: alice.address
        });
        await thirdDepositHandle.wait();
        while ((await tester.web3Provider.getL1BatchNumber()) <= initialL1BatchNumber) {
            await utils.sleep(1);
        }

        // kl todo add an L2 token and withdrawal here, to check token balance migration properly.

        // Wait for at least one new committed block
        let newBlocksCommitted = await l1MainContract.getTotalBatchesCommitted();
        let tryCount = 0;
        while (blocksCommitted === newBlocksCommitted && tryCount < 30) {
            newBlocksCommitted = await l1MainContract.getTotalBatchesCommitted();
            tryCount += 1;
            await utils.sleep(1);
        }
    });

    step('Pause deposits before initiating migration', async () => {
        await utils.spawn(`zkstack chain pause-deposits --chain ${fileConfig.chain}`);

        // Wait until the priority queue is empty
        let tryCount = 0;
        while ((await getPriorityQueueSize()) > 0 && tryCount < 200) {
            tryCount += 1;
            await utils.sleep(1);
        }
        console.error('tryCount', tryCount);
    });

    step('Migrate to/from gateway', async () => {
        if (direction == 'TO') {
            await utils.spawn(`zkstack chain gateway notify-about-to-gateway-update --chain ${fileConfig.chain}`);
        } else {
            await utils.spawn(`zkstack chain gateway notify-about-from-gateway-update --chain ${fileConfig.chain}`);
        }
        // Trying to send a transaction from the same address again
        await checkedRandomTransfer(alice, 1n);

        // Theoretically, if L1 is really slow at this part, it could lead to a situation
        // where there is an inflight transaction before the migration is complete.
        // If you encounter an error, such as a failed transaction, after the migration,
        // this area might be worth revisiting to wait for unconfirmed transactions on the server.
        await utils.sleep(30);

        // Wait for all batches to be executed
        await waitForAllBatchesToBeExecuted();

        if (direction == 'TO') {
            await utils.spawn(
                `zkstack chain gateway migrate-to-gateway --chain ${fileConfig.chain} --gateway-chain-name ${gatewayChain}`
            );
        } else {
            await utils.spawn(
                `zkstack chain gateway migrate-from-gateway --chain ${fileConfig.chain} --gateway-chain-name ${gatewayChain}`
            );
        }
        await mainNodeSpawner.mainNode?.waitForShutdown();
        // Node is already killed, so we simply start the new server
        await mainNodeSpawner.killAndSpawnMainNode();
    });

    step('Check the settlement layer on l1 and gateway', async () => {
        let gatewayInfo = getGatewayInfo(pathToHome, fileConfig.chain!);
        let slMainContract = new ethers.Contract(
            gatewayInfo?.l2DiamondProxyAddress!,
            ZK_CHAIN_INTERFACE,
            gatewayInfo?.gatewayProvider
        );

        let slAddressl1 = await l1MainContract.getSettlementLayer();
        let slAddressGW = await slMainContract.getSettlementLayer();
        if (direction == 'TO') {
            expect(slAddressl1 != ZeroAddress);
            expect(slAddressGW == ZeroAddress);
        } else {
            expect(slAddressl1 == ZeroAddress);
            expect(slAddressGW != ZeroAddress);
        }
    });

    step('Wait for block finalization', async () => {
        await utils.spawn(`zkstack server wait --ignore-prerequisites --verbose --chain ${fileConfig.chain}`);
        // Execute an L2 transaction
        const txHandle = await checkedRandomTransfer(alice, 1n);
        await txHandle.waitFinalize();
    });

    step('Check token settlement layers', async () => {
        const tokenDetails = tester.token;
        const ecosystemContracts = await getEcosystemContracts(tester.syncWallet);
        let assetId = await ecosystemContracts.nativeTokenVault.assetId(tokenDetails.address);
        const chainId = (await tester.syncWallet.provider!.getNetwork()).chainId;
        const migrationNumberL1 = await ecosystemContracts.assetTracker.assetMigrationNumber(chainId, assetId);

        await utils.spawn(`zkstack dev init-test-wallet --chain gateway`);

        const gatewayInfo = getGatewayInfo(pathToHome, fileConfig.chain!);
        const gatewayEcosystemContracts = await getEcosystemContracts(
            new zksync.Wallet(getMainWalletPk('gateway'), gatewayInfo?.gatewayProvider!, tester.syncWallet.providerL1)
        );
        const migrationNumberGateway = await gatewayEcosystemContracts.assetTracker.assetMigrationNumber(
            chainId,
            assetId
        );

        // let expectedL1AssetSettlementLayer = (await tester.ethWallet.provider!.getNetwork()).chainId;
        // let expectedGatewayAssetSettlementLayer = 0n;
        if (direction == 'TO') {
            // expectedL1AssetSettlementLayer = BigInt(gatewayInfo?.gatewayChainId!);
            // expectedGatewayAssetSettlementLayer = BigInt(fileConfig.chain!);
        } else {
            return; // kl todo add migrate back from gateway
        }
        // expect(l1AssetSettlementLayer === fileConfig.chain).to.be.true;
        // expect(gatewayAssetSettlementLayer === gatewayChain).to.be.true;
        expect(migrationNumberL1 === migrationNumberGateway).to.be.true;
        console.log('migrationNumberL1', migrationNumberL1);
    });

    step('Execute transactions after simple restart', async () => {
        // Stop server.
        await mainNodeSpawner.killAndSpawnMainNode();

        // Trying to send a transaction from the same address again
        await checkedRandomTransfer(alice, 1n);
    });

    /// Verify that the precommits are enabled on the gateway. This check is enough for making sure
    // precommits are working correctly. The rest of the checks are done by contract.
    step('Verify precommits', async () => {
        let gwCommittedBatches = await gwMainContract.getTotalBatchesCommitted();
        while (gwCommittedBatches === 1) {
            console.log(`Waiting for at least one batch committed batch on gateway... ${gwCommittedBatches}`);
            await utils.sleep(1);
        }

        // Now we sure that we have at least one batch was committed from the gateway

        const currentBlock = await tester.ethProvider.getBlockNumber();
        const fromBlock = Math.max(0, currentBlock - 50000);
        const filter = gwMainContract.filters.BatchPrecommitmentSet();
        const events = await gwMainContract.queryFilter(filter, fromBlock, 'latest');
        expect(events.length > 0, 'No precommitment events found on the gateway').to.be.true;
    });

    // Migrating back to the gateway is temporarily unsupported in v31.
    // This test verifies that the operation fails as expected.
    // TODO: When support is restored in future versions, remove this negative test.
    step('Migrating back to gateway fails', async () => {
        // Pause deposits before trying migration back to gateway
        await utils.spawn(`zkstack chain pause-deposits --chain ${fileConfig.chain}`);

        try {
            // We use utils.exec instead of utils.spawn to capture stdout/stderr for assertion
            await utils.exec(
                `zkstack chain gateway migrate-to-gateway --chain ${fileConfig.chain} --gateway-chain-name ${gatewayChain}`
            );
            expect.fail('Migrating back to gateway should have failed');
        } catch (e: any) {
            const output = (e.stdout || '') + (e.stderr || '');
            // 0x47d42b1b corresponds to IteratedMigrationsNotSupported() error
            expect(output).to.include('execution reverted, data: Some(String("0x47d42b1b"))');
        }
    });

    after('Try killing server', async () => {
        try {
            mainNodeSpawner.mainNode?.terminate();
        } catch (_) {}
    });

    async function getPriorityQueueSize() {
        if (direction == 'TO') {
            return await l1MainContract.getPriorityQueueSize();
        } else {
            return await chainGatewayContract.getPriorityQueueSize();
        }
    }

    async function waitForAllBatchesToBeExecuted() {
        let tryCount = 0;
        let totalBatchesCommitted = await getTotalBatchesCommitted();
        let totalBatchesExecuted = await getTotalBatchesExecuted();
        while (totalBatchesCommitted !== totalBatchesExecuted && tryCount < 100) {
            tryCount += 1;
            await utils.sleep(1);
            totalBatchesCommitted = await getTotalBatchesCommitted();
            totalBatchesExecuted = await getTotalBatchesExecuted();
        }
    }

    async function getTotalBatchesCommitted() {
        if (direction == 'TO') {
            return await l1MainContract.getTotalBatchesCommitted();
        } else {
            return await chainGatewayContract.getTotalBatchesCommitted();
        }
    }

    async function getTotalBatchesExecuted() {
        if (direction == 'TO') {
            return await l1MainContract.getTotalBatchesExecuted();
        } else {
            return await chainGatewayContract.getTotalBatchesExecuted();
        }
    }
});

async function checkedRandomTransfer(sender: zksync.Wallet, amount: bigint): Promise<zksync.types.TransactionResponse> {
    const senderBalanceBefore = await sender.getBalance();
    const receiverHD = zksync.Wallet.createRandom();
    const receiver = new zksync.Wallet(receiverHD.privateKey, sender.provider);
    const transferHandle = await sender.sendTransaction({
        to: receiver.address,
        value: amount,
        type: 0
    });
    const txReceipt = await transferHandle.wait();

    const senderBalanceAfter = await sender.getBalance();
    const receiverBalanceAfter = await receiver.getBalance();

    expect(receiverBalanceAfter === amount, 'Failed updated the balance of the receiver').to.be.true;

    const spentAmount = txReceipt.gasUsed * transferHandle.gasPrice! + amount;
    expect(senderBalanceAfter + spentAmount >= senderBalanceBefore, 'Failed to update the balance of the sender').to.be
        .true;

    if (process.env.CHECK_EN_URL) {
        console.log('Checking EN after transfer');
        await utils.sleep(2);
        const enProvider = new ethers.JsonRpcProvider(process.env.CHECK_EN_URL);
        const enSenderBalance = await enProvider.getBalance(sender.address);
        expect(enSenderBalance === senderBalanceAfter, 'Failed to update the balance of the sender on EN').to.be.true;
    }

    return transferHandle;
}

async function mintToAddress(
    baseTokenAddress: zksync.types.Address,
    ethersWallet: ethers.Wallet,
    addressToMintTo: string,
    amountToMint: bigint
) {
    const l1Erc20ABI = ['function mint(address to, uint256 amount)'];
    const l1Erc20Contract = new ethers.Contract(baseTokenAddress, l1Erc20ABI, ethersWallet);
    await (await l1Erc20Contract.mint(addressToMintTo, amountToMint)).wait();
}

export function getGatewayInfo(pathToHome: string, chain: string): GatewayInfo | null {
    const gatewayChainConfig = loadConfig({
        pathToHome,
        chain,
        config: 'gateway_chain.yaml'
    });
    if (!gatewayChainConfig) {
        return null;
    }

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
