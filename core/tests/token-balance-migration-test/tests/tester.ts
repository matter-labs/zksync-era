import * as ethers from 'ethers';
import * as zksync from 'zksync-ethers';
import { getMainWalletPk } from 'highlevel-test-tools/src/wallets';
import { createChainAndStartServer, TestChain, ChainType } from 'highlevel-test-tools/src/create-chain';
import path from 'path';
import { loadConfig } from 'utils/build/file-configs';
import { sleep } from 'zksync-ethers/build/utils';
import { L2_NATIVE_TOKEN_VAULT_ADDRESS } from 'utils/src/constants';
import * as fs from 'fs';

const RICH_WALLET_L1_BALANCE = ethers.parseEther('10.0');
const RICH_WALLET_L2_BALANCE = RICH_WALLET_L1_BALANCE;
const TEST_SUIT_NAME = 'AT_CHAIN_MIGRATION_TEST';
const pathToHome = path.join(__dirname, '../../../..');

function readArtifact(contractName: string, outFolder: string = 'out') {
    return JSON.parse(
        fs
            .readFileSync(
                path.join(pathToHome, `./contracts/l1-contracts/out/${contractName}.sol/${contractName}.json`)
            )
            .toString()
    );
}

const ERC20_EVM_ARTIFACT = readArtifact('TestnetERC20Token');
const ERC20_EVM_BYTECODE = ERC20_EVM_ARTIFACT.bytecode.object;
const ERC20_ABI = ERC20_EVM_ARTIFACT.abi;

const ERC20_ZKEVM_BYTECODE = readArtifact('TestnetERC20Token', 'zkout').bytecode.object;

function randomTokenProps() {
    const name = 'NAME-' + ethers.hexlify(ethers.randomBytes(4));
    const symbol = 'SYM-' + ethers.hexlify(ethers.randomBytes(4));
    const decimals = Math.min(Math.floor(Math.random() * 18) + 1, 18);

    return { name, symbol, decimals };
}

export const DEFAULT_SMALL_AMOUNT = 1n;
export const DEFAULT_LARGE_AMOUNT = ethers.parseEther('0.01');

// TODO: For now, only works with ETH chains
async function generateChainRichWallet(chainName: string): Promise<zksync.Wallet> {
    const generalConfig = loadConfig({
        pathToHome,
        chain: chainName,
        config: 'general.yaml'
    });
    const contractsConfig = loadConfig({
        pathToHome,
        chain: chainName,
        config: 'contracts.yaml'
    });
    const secretsConfig = loadConfig({
        pathToHome,
        chain: chainName,
        config: 'secrets.yaml'
    });
    const ethProviderAddress = secretsConfig.l1.l1_rpc_url;
    const web3JsonRpc = generalConfig.api.web3_json_rpc.http_url;

    const richWallet = new zksync.Wallet(
        getMainWalletPk('gateway'),
        new zksync.Provider(web3JsonRpc),
        new ethers.JsonRpcProvider(ethProviderAddress)
    );

    // We assume that Gateway has "ETH" as the base token.
    // We deposit funds to ensure that the wallet is rich
    await (
        await richWallet.deposit({
            token: zksync.utils.ETH_ADDRESS_IN_CONTRACTS,
            amount: RICH_WALLET_L2_BALANCE
        })
    ).wait();

    return richWallet;
}

function getL2Ntv(l2Wallet: zksync.Wallet) {
    const abi = readArtifact('L2NativeTokenVault').abi;
    return new zksync.Contract(L2_NATIVE_TOKEN_VAULT_ADDRESS, abi, l2Wallet);
}

export class ChainHandler {
    // public name: string;
    public l2RichWallet: zksync.Wallet;

    public inner: TestChain;

    constructor(inner: TestChain, l2RichWallet: zksync.Wallet) {
        this.inner = inner;
        this.l2RichWallet = l2RichWallet;
    }

    ethersWalletToZK(ethersWallet: ethers.Wallet) {
        return new zksync.Wallet(ethersWallet.privateKey, this.l2RichWallet.provider, ethersWallet.provider!);
    }

    async stopServer() {
        await this.inner.mainNode.kill();
    }

    async migrateToGateway() {}

    async migrateFromGateway() {}

    static async createNewChain(chainType: ChainType): Promise<ChainHandler> {
        const testChain = await createChainAndStartServer(chainType, TEST_SUIT_NAME);

        // Need to wait for a bit before the server works fully
        await sleep(2000);

        const handler = new ChainHandler(testChain, await generateChainRichWallet(testChain.chainName));

        return handler;
    }

    async deployNativeToken() {
        return await L2ERC20Handler.deployToken(this.l2RichWallet);
    }
}

export class L2ERC20Handler {
    public address: string;
    public l2Wallet: zksync.Wallet;
    public contract: zksync.Contract;
    _cachedAssetId: string | null = null;

    constructor(address: string, l2Wallet: zksync.Wallet) {
        this.address = address;
        this.l2Wallet = l2Wallet;
        this.contract = new zksync.Contract(address, ERC20_ABI, l2Wallet);
    }

    async assetId(assertNonNull: boolean = true): Promise<string> {
        if (this._cachedAssetId) {
            return this._cachedAssetId;
        }
        const l2Ntv = getL2Ntv(this.l2Wallet);

        const l2AssetId = await l2Ntv.assetId(this.address);
        if (l2AssetId == ethers.ZeroHash) {
            if (assertNonNull) {
                throw new Error('Expected to be non null');
            } else {
                // We dont cache empty value.
                return l2AssetId;
            }
        }

        this._cachedAssetId = l2AssetId;
        return l2AssetId;
    }

    async registerIfNeeded(): Promise<string> {
        const l2Ntv = getL2Ntv(this.l2Wallet);
        const currentAssetId = await this.assetId(false);
        if (currentAssetId == ethers.ZeroHash) {
            // Registering the token
            await (await l2Ntv.registerToken(this.address)).wait();
        }

        return await this.assetId();
    }

    async withdraw(amount: bigint = DEFAULT_SMALL_AMOUNT): Promise<WithdrawalHandler> {
        await this.registerIfNeeded();

        if ((await this.contract.allowance(this.l2Wallet.address, L2_NATIVE_TOKEN_VAULT_ADDRESS)) < amount) {
            await (await this.contract.approve(L2_NATIVE_TOKEN_VAULT_ADDRESS, 0)).wait();
            await (await this.contract.approve(L2_NATIVE_TOKEN_VAULT_ADDRESS, amount)).wait();
        }

        const withdrawTx = await this.l2Wallet.withdraw({
            token: this.address,
            amount
        });

        return new WithdrawalHandler(withdrawTx.hash, this.l2Wallet.provider);
    }

    async migrateBalanceL2ToGW(): Promise<MigrationHandler> {
        throw new Error('');
    }

    async migrateBalanceGWtoL1(gwRichWallet: zksync.Wallet): Promise<MigrationHandler> {
        throw new Error('');
    }

    static async deployToken(l2Wallet: zksync.Wallet) {
        const factory = new zksync.ContractFactory(ERC20_ABI, ERC20_ZKEVM_BYTECODE, l2Wallet, 'create');

        const props = randomTokenProps();
        const newToken = await factory.deploy(props.name, props.symbol, props.decimals);
        await newToken.waitForDeployment();

        const handler = new L2ERC20Handler(await newToken.getAddress(), l2Wallet);
        await (await handler.contract.mint(l2Wallet.address, RICH_WALLET_L1_BALANCE)).wait();

        return handler;
    }
}

export class L1ERC20Handler {
    public address: string;
    public l1Wallet: ethers.Wallet;
    public contract: zksync.Contract;

    constructor(address: string, l1Wallet: ethers.Wallet) {
        this.address = address;
        this.l1Wallet = l1Wallet;
        this.contract = new ethers.Contract(address, ERC20_ABI, l1Wallet);
    }

    // We always wait for the deposit to be finalized.
    async deposit(chain: ChainHandler, amount: ethers.BigNumberish = DEFAULT_SMALL_AMOUNT) {
        const zksyncWallet = chain.ethersWalletToZK(this.l1Wallet);
        const depositTx = await zksyncWallet.deposit({
            token: this.address,
            amount,
            approveERC20: true,
            approveBaseERC20: true
        });
        await depositTx.wait();
    }

    async atL2SameWallet(chainHandler: ChainHandler) {
        // FIXME: actually obtain it
        const addrL2 = ethers.ZeroAddress;

        return new L2ERC20Handler(addrL2, chainHandler.ethersWalletToZK(this.l1Wallet));
    }

    static async deployToken(l1Wallet: ethers.Wallet) {
        const factory = new ethers.ContractFactory(ERC20_ABI, ERC20_EVM_BYTECODE, l1Wallet);

        const props = randomTokenProps();
        const newToken = await factory.deploy(props.name, props.symbol, props.decimals);
        await newToken.waitForDeployment();

        const handler = new L1ERC20Handler(await newToken.getAddress(), l1Wallet);
        await (await handler.contract.mint(l1Wallet.address, RICH_WALLET_L1_BALANCE)).wait();

        return handler;
    }
}

export class WithdrawalHandler {
    public txHash: string;
    public l2Provider: zksync.Provider;

    constructor(txHash: string, provider: zksync.Provider) {
        this.txHash = txHash;
        this.l2Provider = provider;
    }

    async finalizeWithdrawal(l1RichWallet: ethers.Wallet) {
        // Firstly, we've need to wait for the batch to be finalized.

        const l2Wallet = new zksync.Wallet(l1RichWallet.privateKey, this.l2Provider, l1RichWallet.provider!);

        const receipt = await l2Wallet.provider.getTransactionReceipt(this.txHash);
        if (!receipt) {
            throw new Error('Receipt');
        }

        await waitForL2ToL1LogProof(l2Wallet.provider, receipt.blockNumber, this.txHash);

        await (await l2Wallet.finalizeWithdrawal(this.txHash)).wait();
    }
}

export class MigrationHandler {
    public txHash: string;

    constructor(txHash: string, provider: zksync.Provider) {
        this.txHash = txHash;
    }

    async finalizeMigration(l1RichWallet: ethers.Wallet) {}
}

async function newSpawnNewChainZKStack() {}

export class Tester {
    public runningFee: Map<zksync.types.Address, bigint>;
    constructor(
        public ethProvider: ethers.Provider,
        public ethWallet: ethers.Wallet,
        public syncWallet: zksync.Wallet,
        public web3Provider: zksync.Provider
    ) {
        this.runningFee = new Map();
    }

    // prettier-ignore
    static async init(ethProviderAddress: string, web3JsonRpc: string) {
        const ethProvider = new ethers.JsonRpcProvider(ethProviderAddress);

        const chainName = process.env.CHAIN_NAME!!;
        let ethWallet = new ethers.Wallet(getMainWalletPk(chainName));

        ethWallet = ethWallet.connect(ethProvider);
        const web3Provider = new zksync.Provider(web3JsonRpc);
        web3Provider.pollingInterval = 100; // It's OK to keep it low even on stage.
        const syncWallet = new zksync.Wallet(ethWallet.privateKey, web3Provider, ethProvider);


        // Since some tx may be pending on stage, we don't want to get stuck because of it.
        // In order to not get stuck transactions, we manually cancel all the pending txs.
        const latestNonce = await ethWallet.getNonce('latest');
        const pendingNonce = await ethWallet.getNonce('pending');
        const cancellationTxs = [];
        for (let nonce = latestNonce; nonce != pendingNonce; nonce++) {
            // For each transaction to override it, we need to provide greater fee. 
            // We would manually provide a value high enough (for a testnet) to be both valid
            // and higher than the previous one. It's OK as we'll only be charged for the bass fee
            // anyways. We will also set the miner's tip to 5 gwei, which is also much higher than the normal one.
            const maxFeePerGas = ethers.parseEther("0.00000025"); // 250 gwei
            const maxPriorityFeePerGas = ethers.parseEther("0.000000005"); // 5 gwei
            cancellationTxs.push(ethWallet.sendTransaction({ to: ethWallet.address, nonce, maxFeePerGas, maxPriorityFeePerGas }).then((tx) => tx.wait()));
        }
        if (cancellationTxs.length > 0) {
            await Promise.all(cancellationTxs);
            console.log(`Canceled ${cancellationTxs.length} pending transactions`);
        }

        return new Tester(ethProvider, ethWallet, syncWallet, web3Provider);
    }

    emptyWallet() {
        const walletHD = zksync.Wallet.createRandom();
        return new zksync.Wallet(walletHD.privateKey, this.web3Provider, this.ethProvider);
    }
}

export async function waitUntilBlockFinalized(provider: zksync.Provider, blockNumber: number) {
    console.log('Waiting for block to be finalized...', blockNumber);
    let printedBlockNumber = 0;
    while (true) {
        const block = await provider.getBlock('finalized');
        if (blockNumber <= block.number) {
            break;
        } else {
            if (printedBlockNumber < block.number) {
                console.log('Waiting for block to be finalized...', blockNumber, block.number);
                console.log('time', new Date().toISOString());
                printedBlockNumber = block.number;
            }
            await zksync.utils.sleep(provider.pollingInterval);
        }
    }
}

export async function waitForL2ToL1LogProof(provider: zksync.Provider, blockNumber: number, txHash: string) {
    console.log('waiting for block to be finalized');
    // First, we wait for block to be finalized.
    await waitUntilBlockFinalized(provider, blockNumber);

    console.log('waiting for log proof');
    // Second, we wait for the log proof.
    let i = 0;
    while ((await provider.getLogProof(txHash)) == null) {
        console.log(`Waiting for log proof... ${i}`);
        await zksync.utils.sleep(provider.pollingInterval);
        i++;
    }
}
