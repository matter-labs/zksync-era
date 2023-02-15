import { EIP712Signer } from './signer';
import { Provider } from './provider';
import { serialize, EIP712_TX_TYPE } from './utils';
import { ethers, utils } from 'ethers';
import { BlockTag, TransactionResponse, TransactionRequest } from './types';
import { ProgressCallback } from '@ethersproject/json-wallets';
import { AdapterL1, AdapterL2 } from './adapters';

export class Wallet extends AdapterL2(AdapterL1(ethers.Wallet)) {
    override readonly provider: Provider;
    providerL1?: ethers.providers.Provider;
    public eip712: EIP712Signer;

    override _providerL1() {
        if (this.providerL1 == null) {
            throw new Error('L1 provider missing: use `connectToL1` to specify');
        }
        return this.providerL1;
    }

    override _providerL2() {
        return this.provider;
    }

    override _signerL1() {
        return this.ethWallet();
    }

    override _signerL2() {
        return this;
    }

    ethWallet() {
        return new ethers.Wallet(this._signingKey(), this._providerL1());
    }

    // an alias with a better name
    async getNonce(blockTag?: BlockTag) {
        return await this.getTransactionCount(blockTag);
    }

    override connect(provider: Provider) {
        return new Wallet(this._signingKey(), provider, this.providerL1);
    }

    connectToL1(provider: ethers.providers.Provider) {
        return new Wallet(this._signingKey(), this.provider, provider);
    }

    static override fromMnemonic(mnemonic: string, path?: string, wordlist?: ethers.Wordlist) {
        const wallet = super.fromMnemonic(mnemonic, path, wordlist);
        return new Wallet(wallet._signingKey());
    }

    static override async fromEncryptedJson(
        json: string,
        password?: string | ethers.Bytes,
        callback?: ProgressCallback
    ) {
        const wallet = await super.fromEncryptedJson(json, password, callback);
        return new Wallet(wallet._signingKey());
    }

    static override fromEncryptedJsonSync(json: string, password?: string | ethers.Bytes) {
        const wallet = super.fromEncryptedJsonSync(json, password);
        return new Wallet(wallet._signingKey());
    }

    static override createRandom(options?: any) {
        const wallet = super.createRandom(options);
        return new Wallet(wallet._signingKey());
    }

    constructor(
        privateKey: ethers.BytesLike | utils.SigningKey,
        providerL2?: Provider,
        providerL1?: ethers.providers.Provider
    ) {
        super(privateKey, providerL2);
        if (this.provider != null) {
            const chainId = this.getChainId();
            // @ts-ignore
            this.eip712 = new EIP712Signer(this, chainId);
        }
        this.providerL1 = providerL1;
    }

    override async populateTransaction(transaction: TransactionRequest): Promise<TransactionRequest> {
        if (transaction.type == null && transaction.customData == null) {
            // use legacy txs by default
            transaction.type = 0;
        }
        transaction = await super.populateTransaction(transaction);
        if (transaction.customData == null && transaction.type != EIP712_TX_TYPE) {
            return transaction;
        }

        transaction.type = EIP712_TX_TYPE;
        transaction.value ??= 0;
        transaction.data ??= '0x';
        transaction.customData = this._fillCustomData(transaction.customData);
        transaction.gasPrice = await this.provider.getGasPrice();
        return transaction;
    }

    override async signTransaction(transaction: TransactionRequest): Promise<string> {
        if (transaction.customData == null && transaction.type != EIP712_TX_TYPE) {
            if (transaction.type == 2 && transaction.maxFeePerGas == null) {
                transaction.maxFeePerGas = await this.provider.getGasPrice();
            }
            return await super.signTransaction(transaction);
        } else {
            transaction.from ??= this.address;
            if (transaction.from.toLowerCase() != this.address.toLowerCase()) {
                throw new Error('Transaction `from` address mismatch');
            }
            transaction.customData.customSignature = await this.eip712.sign(transaction);

            return serialize(transaction);
        }
    }

    override async sendTransaction(transaction: ethers.providers.TransactionRequest): Promise<TransactionResponse> {
        // Typescript isn't smart enough to recognise that wallet.sendTransaction
        // calls provider.sendTransaction which returns our extended type and not ethers' one.
        return (await super.sendTransaction(transaction)) as TransactionResponse;
    }
}
