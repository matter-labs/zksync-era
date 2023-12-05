"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.Web3Provider = exports.Provider = void 0;
const ethers_1 = require("ethers");
var Formatter = ethers_1.providers.Formatter;
const web_1 = require("@ethersproject/web");
const typechain_1 = require("../typechain");
const types_1 = require("./types");
const utils_1 = require("./utils");
const signer_1 = require("./signer");
let defaultFormatter = null;
class Provider extends ethers_1.ethers.providers.JsonRpcProvider {
    constructor(url, network) {
        super(url, network);
        this.pollingInterval = 500;
        const blockTag = this.formatter.blockTag.bind(this.formatter);
        this.formatter.blockTag = (tag) => {
            if (tag == 'committed' || tag == 'finalized') {
                return tag;
            }
            return blockTag(tag);
        };
        this.contractAddresses = {};
        this.formatter.transaction = utils_1.parseTransaction;
    }
    // NOTE: this is almost a complete copy-paste of the parent poll method
    // https://github.com/ethers-io/ethers.js/blob/v5.7.2/packages/providers/src.ts/base-provider.ts#L977
    // The only difference is that we handle transaction receipts with `blockNumber: null` differently here.
    async poll() {
        const pollId = Provider._nextPollId++;
        // Track all running promises, so we can trigger a post-poll once they are complete
        const runners = [];
        let blockNumber = null;
        try {
            blockNumber = await this._getInternalBlockNumber(100 + this.pollingInterval / 2);
        }
        catch (error) {
            this.emit('error', error);
            return;
        }
        this._setFastBlockNumber(blockNumber);
        // Emit a poll event after we have the latest (fast) block number
        this.emit('poll', pollId, blockNumber);
        // If the block has not changed, meh.
        if (blockNumber === this._lastBlockNumber) {
            this.emit('didPoll', pollId);
            return;
        }
        // First polling cycle, trigger a "block" events
        if (this._emitted.block === -2) {
            this._emitted.block = blockNumber - 1;
        }
        if (Math.abs(this._emitted.block - blockNumber) > 1000) {
            console.warn(`network block skew detected; skipping block events (emitted=${this._emitted.block} blockNumber=${blockNumber})`);
            this.emit('error', {
                blockNumber: blockNumber,
                event: 'blockSkew',
                previousBlockNumber: this._emitted.block
            });
            this.emit('block', blockNumber);
        }
        else {
            // Notify all listener for each block that has passed
            for (let i = this._emitted.block + 1; i <= blockNumber; i++) {
                this.emit('block', i);
            }
        }
        // The emitted block was updated, check for obsolete events
        if (this._emitted.block !== blockNumber) {
            this._emitted.block = blockNumber;
            Object.keys(this._emitted).forEach((key) => {
                // The block event does not expire
                if (key === 'block') {
                    return;
                }
                // The block we were at when we emitted this event
                const eventBlockNumber = this._emitted[key];
                // We cannot garbage collect pending transactions or blocks here
                // They should be garbage collected by the Provider when setting
                // "pending" events
                if (eventBlockNumber === 'pending') {
                    return;
                }
                // Evict any transaction hashes or block hashes over 12 blocks
                // old, since they should not return null anyways
                if (blockNumber - eventBlockNumber > 12) {
                    delete this._emitted[key];
                }
            });
        }
        // First polling cycle
        if (this._lastBlockNumber === -2) {
            this._lastBlockNumber = blockNumber - 1;
        }
        // Find all transaction hashes we are waiting on
        this._events.forEach((event) => {
            switch (event.type) {
                case 'tx': {
                    const hash = event.hash;
                    let runner = this.getTransactionReceipt(hash)
                        .then((receipt) => {
                        if (!receipt) {
                            return null;
                        }
                        // NOTE: receipts with blockNumber == null are OK.
                        // this means they were rejected in state-keeper or replaced in mempool.
                        // But we still check that they were actually rejected.
                        if (receipt.blockNumber == null &&
                            !(receipt.status != null && ethers_1.BigNumber.from(receipt.status).isZero())) {
                            return null;
                        }
                        this._emitted['t:' + hash] = receipt.blockNumber;
                        this.emit(hash, receipt);
                        return null;
                    })
                        .catch((error) => {
                        this.emit('error', error);
                    });
                    runners.push(runner);
                    break;
                }
                case 'filter': {
                    // We only allow a single getLogs to be in-flight at a time
                    if (!event._inflight) {
                        event._inflight = true;
                        // This is the first filter for this event, so we want to
                        // restrict events to events that happened no earlier than now
                        if (event._lastBlockNumber === -2) {
                            event._lastBlockNumber = blockNumber - 1;
                        }
                        // Filter from the last *known* event; due to load-balancing
                        // and some nodes returning updated block numbers before
                        // indexing events, a logs result with 0 entries cannot be
                        // trusted and we must retry a range which includes it again
                        const filter = event.filter;
                        filter.fromBlock = event._lastBlockNumber + 1;
                        filter.toBlock = blockNumber;
                        // Prevent fitler ranges from growing too wild, since it is quite
                        // likely there just haven't been any events to move the lastBlockNumber.
                        const minFromBlock = filter.toBlock - this._maxFilterBlockRange;
                        if (minFromBlock > filter.fromBlock) {
                            filter.fromBlock = minFromBlock;
                        }
                        if (filter.fromBlock < 0) {
                            filter.fromBlock = 0;
                        }
                        const runner = this.getLogs(filter)
                            .then((logs) => {
                            // Allow the next getLogs
                            event._inflight = false;
                            if (logs.length === 0) {
                                return;
                            }
                            logs.forEach((log) => {
                                // Only when we get an event for a given block number
                                // can we trust the events are indexed
                                if (log.blockNumber > event._lastBlockNumber) {
                                    event._lastBlockNumber = log.blockNumber;
                                }
                                // Make sure we stall requests to fetch blocks and txs
                                this._emitted['b:' + log.blockHash] = log.blockNumber;
                                this._emitted['t:' + log.transactionHash] = log.blockNumber;
                                this.emit(filter, log);
                            });
                        })
                            .catch((error) => {
                            this.emit('error', error);
                            // Allow another getLogs (the range was not updated)
                            event._inflight = false;
                        });
                        runners.push(runner);
                    }
                    break;
                }
            }
        });
        this._lastBlockNumber = blockNumber;
        // Once all events for this loop have been processed, emit "didPoll"
        Promise.all(runners)
            .then(() => {
            this.emit('didPoll', pollId);
        })
            .catch((error) => {
            this.emit('error', error);
        });
        return;
    }
    async getTransactionReceipt(transactionHash) {
        await this.getNetwork();
        transactionHash = await transactionHash;
        const params = { transactionHash: this.formatter.hash(transactionHash, true) };
        return (0, web_1.poll)(async () => {
            const result = await this.perform('getTransactionReceipt', params);
            if (result == null) {
                if (this._emitted['t:' + transactionHash] === undefined) {
                    return null;
                }
                return undefined;
            }
            if (result.blockNumber == null && result.status != null && ethers_1.BigNumber.from(result.status).isZero()) {
                // transaction is rejected in the state-keeper
                return {
                    ...this.formatter.receipt({
                        ...result,
                        confirmations: 1,
                        blockNumber: 0,
                        blockHash: ethers_1.ethers.constants.HashZero
                    }),
                    blockNumber: null,
                    blockHash: null,
                    l1BatchNumber: null,
                    l1BatchTxIndex: null
                };
            }
            if (result.blockHash == null) {
                // receipt is not ready
                return undefined;
            }
            else {
                const receipt = this.formatter.receipt(result);
                if (receipt.blockNumber == null) {
                    receipt.confirmations = 0;
                }
                else if (receipt.confirmations == null) {
                    const blockNumber = await this._getInternalBlockNumber(100 + 2 * this.pollingInterval);
                    // Add the confirmations using the fast block number (pessimistic)
                    let confirmations = blockNumber - receipt.blockNumber + 1;
                    if (confirmations <= 0) {
                        confirmations = 1;
                    }
                    receipt.confirmations = confirmations;
                }
                return receipt;
            }
        }, { oncePoll: this });
    }
    async getBlock(blockHashOrBlockTag) {
        return this._getBlock(blockHashOrBlockTag, false);
    }
    async getBlockWithTransactions(blockHashOrBlockTag) {
        return this._getBlock(blockHashOrBlockTag, true);
    }
    static getFormatter() {
        if (defaultFormatter == null) {
            defaultFormatter = new Formatter();
            const number = defaultFormatter.number.bind(defaultFormatter);
            const boolean = defaultFormatter.boolean.bind(defaultFormatter);
            const hash = defaultFormatter.hash.bind(defaultFormatter);
            const address = defaultFormatter.address.bind(defaultFormatter);
            defaultFormatter.formats.receiptLog.l1BatchNumber = Formatter.allowNull(number);
            defaultFormatter.formats.l2Tol1Log = {
                blockNumber: number,
                blockHash: hash,
                l1BatchNumber: Formatter.allowNull(number),
                transactionIndex: number,
                shardId: number,
                isService: boolean,
                sender: address,
                key: hash,
                value: hash,
                transactionHash: hash,
                txIndexInL1Batch: Formatter.allowNull(number),
                logIndex: number
            };
            defaultFormatter.formats.receipt.l1BatchNumber = Formatter.allowNull(number);
            defaultFormatter.formats.receipt.l1BatchTxIndex = Formatter.allowNull(number);
            defaultFormatter.formats.receipt.l2ToL1Logs = Formatter.arrayOf((value) => Formatter.check(defaultFormatter.formats.l2Tol1Log, value));
            defaultFormatter.formats.block.l1BatchNumber = Formatter.allowNull(number);
            defaultFormatter.formats.block.l1BatchTimestamp = Formatter.allowNull(number);
            defaultFormatter.formats.blockWithTransactions.l1BatchNumber = Formatter.allowNull(number);
            defaultFormatter.formats.blockWithTransactions.l1BatchTimestamp = Formatter.allowNull(number);
            defaultFormatter.formats.transaction.l1BatchNumber = Formatter.allowNull(number);
            defaultFormatter.formats.transaction.l1BatchTxIndex = Formatter.allowNull(number);
            defaultFormatter.formats.filterLog.l1BatchNumber = Formatter.allowNull(number);
        }
        return defaultFormatter;
    }
    async getBalance(address, blockTag, tokenAddress) {
        const tag = this.formatter.blockTag(blockTag);
        if (tokenAddress == null || (0, utils_1.isETH)(tokenAddress)) {
            // requesting ETH balance
            return await super.getBalance(address, tag);
        }
        else {
            try {
                let token = typechain_1.IERC20MetadataFactory.connect(tokenAddress, this);
                return await token.balanceOf(address, { blockTag: tag });
            }
            catch {
                return ethers_1.BigNumber.from(0);
            }
        }
    }
    async l2TokenAddress(token) {
        if (token == utils_1.ETH_ADDRESS) {
            return utils_1.ETH_ADDRESS;
        }
        const bridgeAddresses = await this.getDefaultBridgeAddresses();
        const l2WethBridge = typechain_1.IL2BridgeFactory.connect(bridgeAddresses.wethL2, this);
        const l2WethToken = await l2WethBridge.l2TokenAddress(token);
        // If the token is Wrapped Ether, return its L2 token address
        if (l2WethToken != ethers_1.ethers.constants.AddressZero) {
            return l2WethToken;
        }
        const l2Erc20Bridge = typechain_1.IL2BridgeFactory.connect(bridgeAddresses.erc20L2, this);
        return await l2Erc20Bridge.l2TokenAddress(token);
    }
    async l1TokenAddress(token) {
        if (token == utils_1.ETH_ADDRESS) {
            return utils_1.ETH_ADDRESS;
        }
        const bridgeAddresses = await this.getDefaultBridgeAddresses();
        const l2WethBridge = typechain_1.IL2BridgeFactory.connect(bridgeAddresses.wethL2, this);
        const l1WethToken = await l2WethBridge.l1TokenAddress(token);
        // If the token is Wrapped Ether, return its L1 token address
        if (l1WethToken != ethers_1.ethers.constants.AddressZero) {
            return l1WethToken;
        }
        const erc20Bridge = typechain_1.IL2BridgeFactory.connect(bridgeAddresses.erc20L2, this);
        return await erc20Bridge.l1TokenAddress(token);
    }
    // This function is used when formatting requests for
    // eth_call and eth_estimateGas. We override it here
    // because we have extra stuff to serialize (customData).
    // This function is for internal use only.
    static hexlifyTransaction(transaction, allowExtra) {
        var _a;
        const result = ethers_1.ethers.providers.JsonRpcProvider.hexlifyTransaction(transaction, {
            ...allowExtra,
            customData: true,
            from: true
        });
        if (transaction.customData == null) {
            return result;
        }
        result.eip712Meta = {
            gasPerPubdata: ethers_1.utils.hexValue((_a = transaction.customData.gasPerPubdata) !== null && _a !== void 0 ? _a : 0)
        };
        transaction.type = utils_1.EIP712_TX_TYPE;
        if (transaction.customData.factoryDeps) {
            // @ts-ignore
            result.eip712Meta.factoryDeps = transaction.customData.factoryDeps.map((dep) => 
            // TODO (SMA-1605): we arraify instead of hexlifying because server expects Vec<u8>.
            //  We should change deserialization there.
            Array.from(ethers_1.utils.arrayify(dep)));
        }
        if (transaction.customData.paymasterParams) {
            // @ts-ignore
            result.eip712Meta.paymasterParams = {
                paymaster: ethers_1.utils.hexlify(transaction.customData.paymasterParams.paymaster),
                paymasterInput: Array.from(ethers_1.utils.arrayify(transaction.customData.paymasterParams.paymasterInput))
            };
        }
        return result;
    }
    async estimateGas(transaction) {
        await this.getNetwork();
        const params = await ethers_1.utils.resolveProperties({
            transaction: this._getTransactionRequest(transaction)
        });
        if (transaction.customData != null) {
            // @ts-ignore
            params.transaction.customData = transaction.customData;
        }
        const result = await this.perform('estimateGas', params);
        try {
            return ethers_1.BigNumber.from(result);
        }
        catch (error) {
            throw new Error(`bad result from backend (estimateGas): ${result}`);
        }
    }
    async estimateGasL1(transaction) {
        await this.getNetwork();
        const params = await ethers_1.utils.resolveProperties({
            transaction: this._getTransactionRequest(transaction)
        });
        if (transaction.customData != null) {
            // @ts-ignore
            params.transaction.customData = transaction.customData;
        }
        const result = await this.send('zks_estimateGasL1ToL2', [
            Provider.hexlifyTransaction(params.transaction, { from: true })
        ]);
        try {
            return ethers_1.BigNumber.from(result);
        }
        catch (error) {
            throw new Error(`bad result from backend (zks_estimateGasL1ToL2): ${result}`);
        }
    }
    async getGasPrice(token) {
        const params = token ? [token] : [];
        const price = await this.send('eth_gasPrice', params);
        return ethers_1.BigNumber.from(price);
    }
    async getMessageProof(blockNumber, sender, messageHash, logIndex) {
        return await this.send('zks_getL2ToL1MsgProof', [
            ethers_1.BigNumber.from(blockNumber).toNumber(),
            sender,
            ethers_1.ethers.utils.hexlify(messageHash),
            logIndex
        ]);
    }
    async getLogProof(txHash, index) {
        return await this.send('zks_getL2ToL1LogProof', [ethers_1.ethers.utils.hexlify(txHash), index]);
    }
    async getL1BatchBlockRange(l1BatchNumber) {
        const range = await this.send('zks_getL1BatchBlockRange', [l1BatchNumber]);
        if (range == null) {
            return null;
        }
        return [parseInt(range[0], 16), parseInt(range[1], 16)];
    }
    async getMainContractAddress() {
        if (!this.contractAddresses.mainContract) {
            this.contractAddresses.mainContract = await this.send('zks_getMainContract', []);
        }
        return this.contractAddresses.mainContract;
    }
    async getTestnetPaymasterAddress() {
        // Unlike contract's addresses, the testnet paymaster is not cached, since it can be trivially changed
        // on the fly by the server and should not be relied to be constant
        return await this.send('zks_getTestnetPaymaster', []);
    }
    async getDefaultBridgeAddresses() {
        if (!this.contractAddresses.erc20BridgeL1) {
            let addresses = await this.send('zks_getBridgeContracts', []);
            this.contractAddresses.erc20BridgeL1 = addresses.l1Erc20DefaultBridge;
            this.contractAddresses.erc20BridgeL2 = addresses.l2Erc20DefaultBridge;
            this.contractAddresses.wethBridgeL1 = addresses.l1WethBridge;
            this.contractAddresses.wethBridgeL2 = addresses.l2WethBridge;
        }
        return {
            erc20L1: this.contractAddresses.erc20BridgeL1,
            erc20L2: this.contractAddresses.erc20BridgeL2,
            wethL1: this.contractAddresses.wethBridgeL1,
            wethL2: this.contractAddresses.wethBridgeL2
        };
    }
    async getConfirmedTokens(start = 0, limit = 255) {
        const tokens = await this.send('zks_getConfirmedTokens', [start, limit]);
        return tokens.map((token) => ({ address: token.l2Address, ...token }));
    }
    async getTokenPrice(token) {
        return await this.send('zks_getTokenPrice', [token]);
    }
    async getAllAccountBalances(address) {
        let balances = await this.send('zks_getAllAccountBalances', [address]);
        for (let token in balances) {
            balances[token] = ethers_1.BigNumber.from(balances[token]);
        }
        return balances;
    }
    async l1ChainId() {
        const res = await this.send('zks_L1ChainId', []);
        return ethers_1.BigNumber.from(res).toNumber();
    }
    async getL1BatchNumber() {
        const number = await this.send('zks_L1BatchNumber', []);
        return ethers_1.BigNumber.from(number).toNumber();
    }
    async getL1BatchDetails(number) {
        return await this.send('zks_getL1BatchDetails', [number]);
    }
    async getBlockDetails(number) {
        return await this.send('zks_getBlockDetails', [number]);
    }
    async getTransactionDetails(txHash) {
        return await this.send('zks_getTransactionDetails', [txHash]);
    }
    async getWithdrawTx(transaction) {
        var _a, _b, _c;
        var _d;
        const { ...tx } = transaction;
        if (tx.to == null && tx.from == null) {
            throw new Error('withdrawal target address is undefined');
        }
        (_a = tx.to) !== null && _a !== void 0 ? _a : (tx.to = tx.from);
        (_b = tx.overrides) !== null && _b !== void 0 ? _b : (tx.overrides = {});
        (_c = (_d = tx.overrides).from) !== null && _c !== void 0 ? _c : (_d.from = tx.from);
        if ((0, utils_1.isETH)(tx.token)) {
            if (!tx.overrides.value) {
                tx.overrides.value = tx.amount;
            }
            const passedValue = ethers_1.BigNumber.from(tx.overrides.value);
            if (!passedValue.eq(tx.amount)) {
                // To avoid users shooting themselves into the foot, we will always use the amount to withdraw
                // as the value
                throw new Error('The tx.value is not equal to the value withdrawn');
            }
            const ethL2Token = typechain_1.IEthTokenFactory.connect(utils_1.L2_ETH_TOKEN_ADDRESS, this);
            return ethL2Token.populateTransaction.withdraw(tx.to, tx.overrides);
        }
        if (tx.bridgeAddress == null) {
            const bridgeAddresses = await this.getDefaultBridgeAddresses();
            const l2WethBridge = typechain_1.IL2BridgeFactory.connect(bridgeAddresses.wethL2, this);
            const l1WethToken = await l2WethBridge.l1TokenAddress(tx.token);
            tx.bridgeAddress =
                l1WethToken != ethers_1.ethers.constants.AddressZero ? bridgeAddresses.wethL2 : bridgeAddresses.erc20L2;
        }
        const bridge = typechain_1.IL2BridgeFactory.connect(tx.bridgeAddress, this);
        return bridge.populateTransaction.withdraw(tx.to, tx.token, tx.amount, tx.overrides);
    }
    async estimateGasWithdraw(transaction) {
        const withdrawTx = await this.getWithdrawTx(transaction);
        return await this.estimateGas(withdrawTx);
    }
    async getTransferTx(transaction) {
        var _a, _b;
        var _c;
        const { ...tx } = transaction;
        (_a = tx.overrides) !== null && _a !== void 0 ? _a : (tx.overrides = {});
        (_b = (_c = tx.overrides).from) !== null && _b !== void 0 ? _b : (_c.from = tx.from);
        if (tx.token == null || tx.token == utils_1.ETH_ADDRESS) {
            return {
                ...(await ethers_1.ethers.utils.resolveProperties(tx.overrides)),
                to: tx.to,
                value: tx.amount
            };
        }
        else {
            const token = typechain_1.IERC20MetadataFactory.connect(tx.token, this);
            return await token.populateTransaction.transfer(tx.to, tx.amount, tx.overrides);
        }
    }
    async estimateGasTransfer(transaction) {
        const transferTx = await this.getTransferTx(transaction);
        return await this.estimateGas(transferTx);
    }
    static getDefaultProvider() {
        // TODO (SMA-1606): Add different urls for different networks.
        return new Provider(process.env.ZKSYNC_WEB3_API_URL || 'http://localhost:3050');
    }
    async newFilter(filter) {
        filter = await filter;
        const id = await this.send('eth_newFilter', [this._prepareFilter(filter)]);
        return ethers_1.BigNumber.from(id);
    }
    async newBlockFilter() {
        const id = await this.send('eth_newBlockFilter', []);
        return ethers_1.BigNumber.from(id);
    }
    async newPendingTransactionsFilter() {
        const id = await this.send('eth_newPendingTransactionFilter', []);
        return ethers_1.BigNumber.from(id);
    }
    async getFilterChanges(idx) {
        const logs = await this.send('eth_getFilterChanges', [idx.toHexString()]);
        return typeof logs[0] === 'string' ? logs : this._parseLogs(logs);
    }
    async getLogs(filter = {}) {
        filter = await filter;
        const logs = await this.send('eth_getLogs', [this._prepareFilter(filter)]);
        return this._parseLogs(logs);
    }
    _parseLogs(logs) {
        return Formatter.arrayOf(this.formatter.filterLog.bind(this.formatter))(logs);
    }
    _prepareFilter(filter) {
        return {
            ...filter,
            fromBlock: filter.fromBlock == null ? null : this.formatter.blockTag(filter.fromBlock),
            toBlock: filter.fromBlock == null ? null : this.formatter.blockTag(filter.toBlock)
        };
    }
    _wrapTransaction(tx, hash) {
        const response = super._wrapTransaction(tx, hash);
        response.waitFinalize = async () => {
            const receipt = await response.wait();
            while (true) {
                const block = await this.getBlock('finalized');
                if (receipt.blockNumber <= block.number) {
                    return await this.getTransactionReceipt(receipt.transactionHash);
                }
                else {
                    await (0, utils_1.sleep)(this.pollingInterval);
                }
            }
        };
        return response;
    }
    // This is inefficient. Status should probably be indicated in the transaction receipt.
    async getTransactionStatus(txHash) {
        const tx = await this.getTransaction(txHash);
        if (tx == null) {
            return types_1.TransactionStatus.NotFound;
        }
        if (tx.blockNumber == null) {
            return types_1.TransactionStatus.Processing;
        }
        const verifiedBlock = await this.getBlock('finalized');
        if (tx.blockNumber <= verifiedBlock.number) {
            return types_1.TransactionStatus.Finalized;
        }
        return types_1.TransactionStatus.Committed;
    }
    async getTransaction(hash) {
        hash = await hash;
        const tx = await super.getTransaction(hash);
        return tx ? this._wrapTransaction(tx, hash) : null;
    }
    async sendTransaction(transaction) {
        return (await super.sendTransaction(transaction));
    }
    async getL2TransactionFromPriorityOp(l1TxResponse) {
        const receipt = await l1TxResponse.wait();
        const l2Hash = (0, utils_1.getL2HashFromPriorityOp)(receipt, await this.getMainContractAddress());
        let status = null;
        do {
            status = await this.getTransactionStatus(l2Hash);
            await (0, utils_1.sleep)(this.pollingInterval);
        } while (status == types_1.TransactionStatus.NotFound);
        return await this.getTransaction(l2Hash);
    }
    async getPriorityOpResponse(l1TxResponse) {
        const l2Response = { ...l1TxResponse };
        l2Response.waitL1Commit = l2Response.wait;
        l2Response.wait = async () => {
            const l2Tx = await this.getL2TransactionFromPriorityOp(l1TxResponse);
            return await l2Tx.wait();
        };
        l2Response.waitFinalize = async () => {
            const l2Tx = await this.getL2TransactionFromPriorityOp(l1TxResponse);
            return await l2Tx.waitFinalize();
        };
        return l2Response;
    }
    async getContractAccountInfo(address) {
        const deployerContract = new ethers_1.Contract(utils_1.CONTRACT_DEPLOYER_ADDRESS, utils_1.CONTRACT_DEPLOYER, this);
        const data = await deployerContract.getAccountInfo(address);
        return {
            supportedAAVersion: data.supportedAAVersion,
            nonceOrdering: data.nonceOrdering
        };
    }
    // TODO (EVM-3): support refundRecipient for fee estimation
    async estimateL1ToL2Execute(transaction) {
        var _a, _b;
        (_a = transaction.gasPerPubdataByte) !== null && _a !== void 0 ? _a : (transaction.gasPerPubdataByte = utils_1.REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_LIMIT);
        // If the `from` address is not provided, we use a random address, because
        // due to storage slot aggregation, the gas estimation will depend on the address
        // and so estimation for the zero address may be smaller than for the sender.
        (_b = transaction.caller) !== null && _b !== void 0 ? _b : (transaction.caller = ethers_1.ethers.Wallet.createRandom().address);
        const customData = {
            gasPerPubdataByte: transaction.gasPerPubdataByte
        };
        if (transaction.factoryDeps) {
            Object.assign(customData, { factoryDeps: transaction.factoryDeps });
        }
        const fee = await this.estimateGasL1({
            from: transaction.caller,
            data: transaction.calldata,
            to: transaction.contractAddress,
            value: transaction.l2Value,
            customData
        });
        return fee;
    }
}
exports.Provider = Provider;
Provider._nextPollId = 1;
class Web3Provider extends Provider {
    constructor(provider, network) {
        if (provider == null) {
            throw new Error('missing provider');
        }
        if (!provider.request) {
            throw new Error('provider must implement eip-1193');
        }
        let path = provider.host || provider.path || (provider.isMetaMask ? 'metamask' : 'eip-1193:');
        super(path, network);
        this.provider = provider;
    }
    async send(method, params) {
        params !== null && params !== void 0 ? params : (params = []);
        // Metamask complains about eth_sign (and on some versions hangs)
        if (method == 'eth_sign' && (this.provider.isMetaMask || this.provider.isStatus)) {
            // https://github.com/ethereum/go-ethereum/wiki/Management-APIs#personal_sign
            method = 'personal_sign';
            params = [params[1], params[0]];
        }
        return await this.provider.request({ method, params });
    }
    getSigner(addressOrIndex) {
        return signer_1.Signer.from(super.getSigner(addressOrIndex));
    }
    async estimateGas(transaction) {
        const gas = await super.estimateGas(transaction);
        const metamaskMinimum = ethers_1.BigNumber.from(21000);
        const isEIP712 = transaction.customData != null || transaction.type == utils_1.EIP712_TX_TYPE;
        return gas.gt(metamaskMinimum) || isEIP712 ? gas : metamaskMinimum;
    }
}
exports.Web3Provider = Web3Provider;
