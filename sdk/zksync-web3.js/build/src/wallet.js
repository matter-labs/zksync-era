"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.Wallet = void 0;
const signer_1 = require("./signer");
const utils_1 = require("./utils");
const ethers_1 = require("ethers");
const adapters_1 = require("./adapters");
class Wallet extends (0, adapters_1.AdapterL2)((0, adapters_1.AdapterL1)(ethers_1.ethers.Wallet)) {
    constructor(privateKey, providerL2, providerL1) {
        super(privateKey, providerL2);
        if (this.provider != null) {
            const chainId = this.getChainId();
            // @ts-ignore
            this.eip712 = new signer_1.EIP712Signer(this, chainId);
        }
        this.providerL1 = providerL1;
    }
    _providerL1() {
        if (this.providerL1 == null) {
            throw new Error('L1 provider missing: use `connectToL1` to specify');
        }
        return this.providerL1;
    }
    _providerL2() {
        return this.provider;
    }
    _signerL1() {
        return this.ethWallet();
    }
    _signerL2() {
        return this;
    }
    ethWallet() {
        return new ethers_1.ethers.Wallet(this._signingKey(), this._providerL1());
    }
    // an alias with a better name
    async getNonce(blockTag) {
        return await this.getTransactionCount(blockTag);
    }
    connect(provider) {
        return new Wallet(this._signingKey(), provider, this.providerL1);
    }
    connectToL1(provider) {
        return new Wallet(this._signingKey(), this.provider, provider);
    }
    static fromMnemonic(mnemonic, path, wordlist) {
        const wallet = super.fromMnemonic(mnemonic, path, wordlist);
        return new Wallet(wallet._signingKey());
    }
    static async fromEncryptedJson(json, password, callback) {
        const wallet = await super.fromEncryptedJson(json, password, callback);
        return new Wallet(wallet._signingKey());
    }
    static fromEncryptedJsonSync(json, password) {
        const wallet = super.fromEncryptedJsonSync(json, password);
        return new Wallet(wallet._signingKey());
    }
    static createRandom(options) {
        const wallet = super.createRandom(options);
        return new Wallet(wallet._signingKey());
    }
    async populateTransaction(transaction) {
        var _a, _b;
        if (transaction.type == null && transaction.customData == null) {
            // use legacy txs by default
            transaction.type = 0;
        }
        transaction = await super.populateTransaction(transaction);
        if (transaction.customData == null && transaction.type != utils_1.EIP712_TX_TYPE) {
            return transaction;
        }
        transaction.type = utils_1.EIP712_TX_TYPE;
        (_a = transaction.value) !== null && _a !== void 0 ? _a : (transaction.value = 0);
        (_b = transaction.data) !== null && _b !== void 0 ? _b : (transaction.data = '0x');
        transaction.customData = this._fillCustomData(transaction.customData);
        transaction.gasPrice = await this.provider.getGasPrice();
        return transaction;
    }
    async signTransaction(transaction) {
        var _a;
        if (transaction.customData == null && transaction.type != utils_1.EIP712_TX_TYPE) {
            if (transaction.type == 2 && transaction.maxFeePerGas == null) {
                transaction.maxFeePerGas = await this.provider.getGasPrice();
            }
            return await super.signTransaction(transaction);
        }
        else {
            (_a = transaction.from) !== null && _a !== void 0 ? _a : (transaction.from = this.address);
            if (transaction.from.toLowerCase() != this.address.toLowerCase()) {
                throw new Error('Transaction `from` address mismatch');
            }
            transaction.customData.customSignature = await this.eip712.sign(transaction);
            return (0, utils_1.serialize)(transaction);
        }
    }
    async sendTransaction(transaction) {
        // Typescript isn't smart enough to recognise that wallet.sendTransaction
        // calls provider.sendTransaction which returns our extended type and not ethers' one.
        return (await super.sendTransaction(transaction));
    }
}
exports.Wallet = Wallet;
