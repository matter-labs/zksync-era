import { expect } from 'chai';
import * as ethers from 'ethers';
import * as zkweb3 from 'zksync-ethers';
import * as fs from 'fs';
import * as path from 'path';

export class Tester {
    public runningFee: Map<zkweb3.types.Address, ethers.BigNumber>;
    constructor(
        public ethProvider: ethers.providers.Provider,
        public ethWallet: ethers.Wallet,
        public syncWallet: zkweb3.Wallet,
        public web3Provider: zkweb3.Provider,
        public hyperchainAdmin: ethers.Wallet
    ) {
        this.runningFee = new Map();
    }

    // prettier-ignore
    static async init(l1_rpc_addr: string, l2_rpc_addr: string) : Promise<Tester> {
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
        const web3Provider = new zkweb3.Provider(l2_rpc_addr);
        web3Provider.pollingInterval = 100; // It's OK to keep it low even on stage.
        const syncWallet = new zkweb3.Wallet(ethWallet.privateKey, web3Provider, ethProvider);


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

        return new Tester(ethProvider, ethWallet, syncWallet, web3Provider, hyperchainAdmin);
    }

    async fundedWallet(
        ethAmount: ethers.BigNumberish,
        l1Token: zkweb3.types.Address,
        tokenAmount: ethers.BigNumberish
    ) {
        const newWallet = zkweb3.Wallet.createRandom().connect(this.web3Provider).connectToL1(this.ethProvider);

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
        return zkweb3.Wallet.createRandom().connect(this.web3Provider).connectToL1(this.ethProvider);
    }
}
