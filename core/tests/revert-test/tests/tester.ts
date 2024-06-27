import { expect } from 'chai';
import * as ethers from 'ethers';
import * as zksync from 'zksync-ethers';
import * as fs from 'fs';
import * as path from 'path';

const BASE_ERC20_TO_MINT = ethers.utils.parseEther('100');

export class Tester {
    public runningFee: Map<zksync.types.Address, ethers.BigNumber>;
    constructor(
        public ethProvider: ethers.providers.Provider,
        public ethWallet: ethers.Wallet,
        public syncWallet: zksync.Wallet,
        public web3Provider: zksync.Provider,
        public hyperchainAdmin: ethers.Wallet, // We need to add validator to ValidatorTimelock with admin rights
        public isETHBasedChain: boolean,
        public baseTokenAddress: string
    ) {
        this.runningFee = new Map();
    }

    // prettier-ignore
    static async init(l1_rpc_addr: string, l2_rpc_addr: string, baseTokenAddress: string) : Promise<Tester> {
        const ethProvider = new ethers.providers.JsonRpcProvider(l1_rpc_addr);
        ethProvider.pollingInterval = 100;

        const testConfigPath = path.join(process.env.ZKSYNC_HOME!, `etc/test_config/constant`);
        const ethTestConfig = JSON.parse(fs.readFileSync(`${testConfigPath}/eth.json`, { encoding: 'utf-8' }));
        let ethWallet = ethers.Wallet.fromMnemonic(
            ethTestConfig.test_mnemonic as string,
            "m/44'/60'/0'/0/0"
        ).connect(ethProvider);
        let hyperchainAdmin = ethers.Wallet.fromMnemonic(
            ethTestConfig.mnemonic as string,
            "m/44'/60'/0'/0/1"
        ).connect(ethProvider);
        const web3Provider = new zksync.Provider(l2_rpc_addr);
        web3Provider.pollingInterval = 100; // It's OK to keep it low even on stage.
        const syncWallet = new zksync.Wallet(ethWallet.privateKey, web3Provider, ethProvider);


        // Since some tx may be pending on stage, we don't want to get stuck because of it.
        // In order to not get stuck transactions, we manually cancel all the pending txs.
        const latestNonce = await ethWallet.getTransactionCount('latest');
        const pendingNonce = await ethWallet.getTransactionCount('pending');
        const cancellationTxs = [];
        for (let nonce = latestNonce; nonce != pendingNonce; nonce++) {
            // For each transaction to override it, we need to provide greater fee.
            // We would manually provide a value high enough (for a testnet) to be both valid
            // and higher than the previous one. It's OK as we'll only be charged for the bass fee
            // anyways. We will also set the miner's tip to 5 gwei, which is also much higher than the normal one.
            const maxFeePerGas = ethers.utils.parseEther("0.00000025"); // 250 gwei
            const maxPriorityFeePerGas = ethers.utils.parseEther("0.000000005"); // 5 gwei
            cancellationTxs.push(ethWallet.sendTransaction({ to: ethWallet.address, nonce, maxFeePerGas, maxPriorityFeePerGas }).then((tx) => tx.wait()));
        }
        if (cancellationTxs.length > 0) {
            await Promise.all(cancellationTxs);
            console.log(`Canceled ${cancellationTxs.length} pending transactions`);
        }

        const isETHBasedChain = baseTokenAddress == zksync.utils.ETH_ADDRESS_IN_CONTRACTS;

        return new Tester(ethProvider, ethWallet, syncWallet, web3Provider, hyperchainAdmin, isETHBasedChain, baseTokenAddress);
    }

    /// Ensures that the main wallet has enough base token.
    /// This can not be done inside the `init` function because `init` function can be called before the
    /// L2 RPC is active, but we need the L2 RPC to get the base token address.
    async fundSyncWallet() {
        const baseTokenAddress = await this.syncWallet.provider.getBaseTokenContractAddress();
        if (!(baseTokenAddress === zksync.utils.ETH_ADDRESS_IN_CONTRACTS)) {
            const l1Erc20ABI = ['function mint(address to, uint256 amount)'];
            const l1Erc20Contract = new ethers.Contract(baseTokenAddress, l1Erc20ABI, this.ethWallet);
            await (await l1Erc20Contract.mint(this.ethWallet.address, BASE_ERC20_TO_MINT)).wait();
        }
    }

    async fundedWallet(
        ethAmount: ethers.BigNumberish,
        l1Token: zksync.types.Address,
        tokenAmount: ethers.BigNumberish
    ) {
        const newWallet = zksync.Wallet.createRandom().connect(this.web3Provider).connectToL1(this.ethProvider);

        let ethBalance = await this.syncWallet.getBalanceL1();
        expect(ethBalance.gt(ethAmount), 'Insufficient eth balance to create funded wallet').to.be.true;

        // To make the wallet capable of requesting priority operations,
        // send ETH to L1.

        const tx1 = await this.syncWallet.ethWallet().sendTransaction({
            to: newWallet.address,
            value: ethAmount
        });
        await tx1.wait();

        // Funds the wallet with L1 token.

        let tokenBalance = await this.syncWallet.getBalanceL1(l1Token);
        expect(tokenBalance.gt(tokenAmount), 'Insufficient token balance to create funded wallet').to.be.true;

        const erc20ABI = ['function transfer(address to, uint256 amount)'];
        const erc20Contract = new ethers.Contract(l1Token, erc20ABI, this.ethWallet);

        const tx2 = await erc20Contract.transfer(newWallet.address, tokenAmount);
        await tx2.wait();

        return newWallet;
    }

    emptyWallet() {
        return zksync.Wallet.createRandom().connect(this.web3Provider).connectToL1(this.ethProvider);
    }
}
