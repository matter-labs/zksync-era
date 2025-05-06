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

async function logsPath(name: string): Promise<string> {
    return await logsTestPath(fileConfig.chain, 'logs/upgrade/', name);
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

describe('Migration From/To gateway test', function () {
    // Utility wallets for facilitating testing
    let tester: Tester;
    let alice: zksync.Wallet;

    // The diamond proxy contract on the settlement layer.
    let l1MainContract: ethers.Contract;

    let gatewayChain: string;
    let logs: fs.FileHandle;
    let direction: string;
    let ethProviderAddress: string | undefined;
    let web3JsonRpc: string | undefined;

    let mainNodeSpawner: utils.NodeSpawner;

    before('Create test wallet', async () => {
        logs = await fs.open(await logsPath('migration.log'), 'a');
        direction = process.env.DIRECTION || 'TO';
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

        tester = await Tester.init(ethProviderAddress!, web3JsonRpc!);
        alice = tester.emptyWallet();

        l1MainContract = new ethers.Contract(
            contractsConfig.l1.diamond_proxy_addr,
            ZK_CHAIN_INTERFACE,
            tester.syncWallet.providerL1
        );
    });

    step('Run server and execute some transactions', async () => {
        await mainNodeSpawner.killAndSpawnMainNode();

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

        // Wait for at least one new committed block
        let newBlocksCommitted = await l1MainContract.getTotalBatchesCommitted();
        let tryCount = 0;
        while (blocksCommitted === newBlocksCommitted && tryCount < 30) {
            newBlocksCommitted = await l1MainContract.getTotalBatchesCommitted();
            tryCount += 1;
            await utils.sleep(1);
        }
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
            let slAddressl1 = await l1MainContract.getSettlementLayer();
            expect(slAddressl1 == ZeroAddress);
            expect(slAddressGW != ZeroAddress);
        }
    });

    step('Wait for block finalization', async () => {
        // Execute an L2 transaction
        const txHandle = await checkedRandomTransfer(alice, 1n);
        await txHandle.waitFinalize();
    });

    step('Execute transactions after simple restart', async () => {
        // Stop server.
        await mainNodeSpawner.killAndSpawnMainNode();

        // Trying to send a transaction from the same address again
        await checkedRandomTransfer(alice, 1n);
    });

    after('Try killing server', async () => {
        try {
            mainNodeSpawner.mainNode?.terminate();
        } catch (_) {}
    });
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
