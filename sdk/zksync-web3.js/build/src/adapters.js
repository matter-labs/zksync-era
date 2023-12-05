"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.AdapterL2 = exports.AdapterL1 = void 0;
const ethers_1 = require("ethers");
const typechain_1 = require("../typechain");
const utils_1 = require("./utils");
function AdapterL1(Base) {
    return class Adapter extends Base {
        _providerL2() {
            throw new Error('Must be implemented by the derived class!');
        }
        _providerL1() {
            throw new Error('Must be implemented by the derived class!');
        }
        _signerL1() {
            throw new Error('Must be implemented by the derived class!');
        }
        async getMainContract() {
            const address = await this._providerL2().getMainContractAddress();
            return typechain_1.IZkSyncFactory.connect(address, this._signerL1());
        }
        async getL1BridgeContracts() {
            const addresses = await this._providerL2().getDefaultBridgeAddresses();
            return {
                erc20: typechain_1.IL1BridgeFactory.connect(addresses.erc20L1, this._signerL1()),
                weth: typechain_1.IL1BridgeFactory.connect(addresses.wethL1, this._signerL1())
            };
        }
        async getBalanceL1(token, blockTag) {
            token !== null && token !== void 0 ? token : (token = utils_1.ETH_ADDRESS);
            if ((0, utils_1.isETH)(token)) {
                return await this._providerL1().getBalance(await this.getAddress(), blockTag);
            }
            else {
                const erc20contract = typechain_1.IERC20MetadataFactory.connect(token, this._providerL1());
                return await erc20contract.balanceOf(await this.getAddress());
            }
        }
        async getAllowanceL1(token, bridgeAddress, blockTag) {
            if (!bridgeAddress) {
                const bridgeContracts = await this.getL1BridgeContracts();
                const l2WethToken = await bridgeContracts.weth.l2TokenAddress(token);
                // If the token is Wrapped Ether, return allowance to its own bridge, otherwise to the default ERC20 bridge.
                bridgeAddress =
                    l2WethToken != ethers_1.ethers.constants.AddressZero
                        ? bridgeContracts.weth.address
                        : bridgeContracts.erc20.address;
            }
            const erc20contract = typechain_1.IERC20MetadataFactory.connect(token, this._providerL1());
            return await erc20contract.allowance(await this.getAddress(), bridgeAddress, { blockTag });
        }
        async l2TokenAddress(token) {
            if (token == utils_1.ETH_ADDRESS) {
                return utils_1.ETH_ADDRESS;
            }
            const bridgeContracts = await this.getL1BridgeContracts();
            const l2WethToken = await bridgeContracts.weth.l2TokenAddress(token);
            // If the token is Wrapped Ether, return its L2 token address.
            if (l2WethToken != ethers_1.ethers.constants.AddressZero) {
                return l2WethToken;
            }
            return await bridgeContracts.erc20.l2TokenAddress(token);
        }
        async approveERC20(token, amount, overrides) {
            if ((0, utils_1.isETH)(token)) {
                throw new Error("ETH token can't be approved. The address of the token does not exist on L1.");
            }
            let bridgeAddress = overrides === null || overrides === void 0 ? void 0 : overrides.bridgeAddress;
            const erc20contract = typechain_1.IERC20MetadataFactory.connect(token, this._signerL1());
            if (bridgeAddress == null) {
                const bridgeContracts = await this.getL1BridgeContracts();
                const l2WethToken = await bridgeContracts.weth.l2TokenAddress(token);
                // If the token is Wrapped Ether, return corresponding bridge, otherwise return default ERC20 bridge
                bridgeAddress =
                    l2WethToken != ethers_1.ethers.constants.AddressZero
                        ? bridgeContracts.weth.address
                        : bridgeContracts.erc20.address;
            }
            else {
                delete overrides.bridgeAddress;
            }
            overrides !== null && overrides !== void 0 ? overrides : (overrides = {});
            return await erc20contract.approve(bridgeAddress, amount, overrides);
        }
        async getBaseCost(params) {
            var _a, _b;
            const zksyncContract = await this.getMainContract();
            const parameters = { ...(0, utils_1.layer1TxDefaults)(), ...params };
            (_a = parameters.gasPrice) !== null && _a !== void 0 ? _a : (parameters.gasPrice = await this._providerL1().getGasPrice());
            (_b = parameters.gasPerPubdataByte) !== null && _b !== void 0 ? _b : (parameters.gasPerPubdataByte = utils_1.REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_LIMIT);
            return ethers_1.BigNumber.from(await zksyncContract.l2TransactionBaseCost(parameters.gasPrice, parameters.gasLimit, parameters.gasPerPubdataByte));
        }
        async deposit(transaction) {
            var _a, _b, _c;
            var _d;
            const depositTx = await this.getDepositTx(transaction);
            if (transaction.token == utils_1.ETH_ADDRESS) {
                const baseGasLimit = await this.estimateGasRequestExecute(depositTx);
                const gasLimit = (0, utils_1.scaleGasLimit)(baseGasLimit);
                (_a = depositTx.overrides) !== null && _a !== void 0 ? _a : (depositTx.overrides = {});
                (_b = (_d = depositTx.overrides).gasLimit) !== null && _b !== void 0 ? _b : (_d.gasLimit = gasLimit);
                return this.requestExecute(depositTx);
            }
            else {
                const bridgeContracts = await this.getL1BridgeContracts();
                if (transaction.approveERC20) {
                    const l2WethToken = await bridgeContracts.weth.l2TokenAddress(transaction.token);
                    // If the token is Wrapped Ether, use its bridge.
                    const proposedBridge = l2WethToken != ethers_1.ethers.constants.AddressZero
                        ? bridgeContracts.weth.address
                        : bridgeContracts.erc20.address;
                    const bridgeAddress = transaction.bridgeAddress ? transaction.bridgeAddress : proposedBridge;
                    // We only request the allowance if the current one is not enough.
                    const allowance = await this.getAllowanceL1(transaction.token, bridgeAddress);
                    if (allowance.lt(transaction.amount)) {
                        const approveTx = await this.approveERC20(transaction.token, transaction.amount, {
                            bridgeAddress,
                            ...transaction.approveOverrides
                        });
                        await approveTx.wait();
                    }
                }
                const baseGasLimit = await this._providerL1().estimateGas(depositTx);
                const gasLimit = (0, utils_1.scaleGasLimit)(baseGasLimit);
                (_c = depositTx.gasLimit) !== null && _c !== void 0 ? _c : (depositTx.gasLimit = gasLimit);
                return await this._providerL2().getPriorityOpResponse(await this._signerL1().sendTransaction(depositTx));
            }
        }
        async estimateGasDeposit(transaction) {
            const depositTx = await this.getDepositTx(transaction);
            let baseGasLimit;
            if (transaction.token == utils_1.ETH_ADDRESS) {
                baseGasLimit = await this.estimateGasRequestExecute(depositTx);
            }
            else {
                baseGasLimit = await this._providerL1().estimateGas(depositTx);
            }
            return (0, utils_1.scaleGasLimit)(baseGasLimit);
        }
        async getDepositTx(transaction) {
            var _a, _b, _c, _d, _e, _f, _g, _h, _j, _k;
            const bridgeContracts = await this.getL1BridgeContracts();
            if (transaction.bridgeAddress != null) {
                bridgeContracts.erc20 = bridgeContracts.erc20.attach(transaction.bridgeAddress);
            }
            const { ...tx } = transaction;
            (_a = tx.to) !== null && _a !== void 0 ? _a : (tx.to = await this.getAddress());
            (_b = tx.operatorTip) !== null && _b !== void 0 ? _b : (tx.operatorTip = ethers_1.BigNumber.from(0));
            (_c = tx.overrides) !== null && _c !== void 0 ? _c : (tx.overrides = {});
            (_d = tx.gasPerPubdataByte) !== null && _d !== void 0 ? _d : (tx.gasPerPubdataByte = utils_1.REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_LIMIT);
            if (tx.bridgeAddress != null) {
                const customBridgeData = ((_e = tx.customBridgeData) !== null && _e !== void 0 ? _e : bridgeContracts.weth.address == tx.bridgeAddress)
                    ? '0x'
                    : await (0, utils_1.getERC20DefaultBridgeData)(tx.token, this._providerL1());
                const bridge = typechain_1.IL1BridgeFactory.connect(tx.bridgeAddress, this._signerL1());
                const l2Address = await bridge.l2Bridge();
                (_f = tx.l2GasLimit) !== null && _f !== void 0 ? _f : (tx.l2GasLimit = await (0, utils_1.estimateCustomBridgeDepositL2Gas)(this._providerL2(), tx.bridgeAddress, l2Address, tx.token, tx.amount, tx.to, customBridgeData, await this.getAddress(), tx.gasPerPubdataByte));
            }
            else {
                (_g = tx.l2GasLimit) !== null && _g !== void 0 ? _g : (tx.l2GasLimit = await (0, utils_1.estimateDefaultBridgeDepositL2Gas)(this._providerL1(), this._providerL2(), tx.token, tx.amount, tx.to, await this.getAddress(), tx.gasPerPubdataByte));
            }
            const { to, token, amount, operatorTip, overrides } = tx;
            await insertGasPrice(this._providerL1(), overrides);
            const gasPriceForEstimation = overrides.maxFeePerGas || overrides.gasPrice;
            const zksyncContract = await this.getMainContract();
            const baseCost = await zksyncContract.l2TransactionBaseCost(await gasPriceForEstimation, tx.l2GasLimit, tx.gasPerPubdataByte);
            if (token == utils_1.ETH_ADDRESS) {
                (_h = overrides.value) !== null && _h !== void 0 ? _h : (overrides.value = baseCost.add(operatorTip).add(amount));
                return {
                    contractAddress: to,
                    calldata: '0x',
                    l2Value: amount,
                    // For some reason typescript can not deduce that we've already set the
                    // tx.l2GasLimit
                    l2GasLimit: tx.l2GasLimit,
                    ...tx
                };
            }
            else {
                let refundRecipient = (_j = tx.refundRecipient) !== null && _j !== void 0 ? _j : ethers_1.ethers.constants.AddressZero;
                const args = [
                    to,
                    token,
                    amount,
                    tx.l2GasLimit,
                    tx.gasPerPubdataByte,
                    refundRecipient
                ];
                (_k = overrides.value) !== null && _k !== void 0 ? _k : (overrides.value = baseCost.add(operatorTip));
                await (0, utils_1.checkBaseCost)(baseCost, overrides.value);
                const l2WethToken = await bridgeContracts.weth.l2TokenAddress(tx.token);
                const bridge = l2WethToken != ethers_1.ethers.constants.AddressZero ? bridgeContracts.weth : bridgeContracts.erc20;
                return await bridge.populateTransaction.deposit(...args, overrides);
            }
        }
        // Retrieves the full needed ETH fee for the deposit.
        // Returns the L1 fee and the L2 fee.
        async getFullRequiredDepositFee(transaction) {
            var _a, _b, _c, _d;
            // It is assumed that the L2 fee for the transaction does not depend on its value.
            const dummyAmount = '1';
            const { ...tx } = transaction;
            const zksyncContract = await this.getMainContract();
            (_a = tx.overrides) !== null && _a !== void 0 ? _a : (tx.overrides = {});
            await insertGasPrice(this._providerL1(), tx.overrides);
            const gasPriceForMessages = (await tx.overrides.maxFeePerGas) || (await tx.overrides.gasPrice);
            (_b = tx.to) !== null && _b !== void 0 ? _b : (tx.to = await this.getAddress());
            (_c = tx.gasPerPubdataByte) !== null && _c !== void 0 ? _c : (tx.gasPerPubdataByte = utils_1.REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_LIMIT);
            let l2GasLimit;
            if (tx.bridgeAddress != null) {
                const bridgeContracts = await this.getL1BridgeContracts();
                const customBridgeData = ((_d = tx.customBridgeData) !== null && _d !== void 0 ? _d : bridgeContracts.weth.address == tx.bridgeAddress)
                    ? '0x'
                    : await (0, utils_1.getERC20DefaultBridgeData)(tx.token, this._providerL1());
                let bridge = typechain_1.IL1BridgeFactory.connect(tx.bridgeAddress, this._signerL1());
                let l2Address = await bridge.l2Bridge();
                l2GasLimit !== null && l2GasLimit !== void 0 ? l2GasLimit : (l2GasLimit = await (0, utils_1.estimateCustomBridgeDepositL2Gas)(this._providerL2(), tx.bridgeAddress, l2Address, tx.token, dummyAmount, tx.to, customBridgeData, await this.getAddress(), tx.gasPerPubdataByte));
            }
            else {
                l2GasLimit !== null && l2GasLimit !== void 0 ? l2GasLimit : (l2GasLimit = await (0, utils_1.estimateDefaultBridgeDepositL2Gas)(this._providerL1(), this._providerL2(), tx.token, dummyAmount, tx.to, await this.getAddress(), tx.gasPerPubdataByte));
            }
            const baseCost = await zksyncContract.l2TransactionBaseCost(gasPriceForMessages, l2GasLimit, tx.gasPerPubdataByte);
            const selfBalanceETH = await this.getBalanceL1();
            // We could 0 in, because the final fee will anyway be bigger than
            if (baseCost.gte(selfBalanceETH.add(dummyAmount))) {
                const recommendedETHBalance = ethers_1.BigNumber.from(tx.token == utils_1.ETH_ADDRESS
                    ? utils_1.L1_RECOMMENDED_MIN_ETH_DEPOSIT_GAS_LIMIT
                    : utils_1.L1_RECOMMENDED_MIN_ERC20_DEPOSIT_GAS_LIMIT)
                    .mul(gasPriceForMessages)
                    .add(baseCost);
                const formattedRecommendedBalance = ethers_1.ethers.utils.formatEther(recommendedETHBalance);
                throw new Error(`Not enough balance for deposit. Under the provided gas price, the recommended balance to perform a deposit is ${formattedRecommendedBalance} ETH`);
            }
            // For ETH token the value that the user passes to the estimation is the one which has the
            // value for the L2 comission substracted.
            let amountForEstimate;
            if ((0, utils_1.isETH)(tx.token)) {
                amountForEstimate = ethers_1.BigNumber.from(dummyAmount);
            }
            else {
                amountForEstimate = ethers_1.BigNumber.from(dummyAmount);
                if ((await this.getAllowanceL1(tx.token)) < amountForEstimate) {
                    throw new Error('Not enough allowance to cover the deposit');
                }
            }
            // Deleting the explicit gas limits in the fee estimation
            // in order to prevent the situation where the transaction
            // fails because the user does not have enough balance
            const estimationOverrides = { ...tx.overrides };
            delete estimationOverrides.gasPrice;
            delete estimationOverrides.maxFeePerGas;
            delete estimationOverrides.maxPriorityFeePerGas;
            const l1GasLimit = await this.estimateGasDeposit({
                ...tx,
                amount: amountForEstimate,
                overrides: estimationOverrides,
                l2GasLimit
            });
            const fullCost = {
                baseCost,
                l1GasLimit,
                l2GasLimit
            };
            if (tx.overrides.gasPrice) {
                fullCost.gasPrice = ethers_1.BigNumber.from(await tx.overrides.gasPrice);
            }
            else {
                fullCost.maxFeePerGas = ethers_1.BigNumber.from(await tx.overrides.maxFeePerGas);
                fullCost.maxPriorityFeePerGas = ethers_1.BigNumber.from(await tx.overrides.maxPriorityFeePerGas);
            }
            return fullCost;
        }
        async _getWithdrawalLog(withdrawalHash, index = 0) {
            const hash = ethers_1.ethers.utils.hexlify(withdrawalHash);
            const receipt = await this._providerL2().getTransactionReceipt(hash);
            const log = receipt.logs.filter((log) => log.address == utils_1.L1_MESSENGER_ADDRESS &&
                log.topics[0] == ethers_1.ethers.utils.id('L1MessageSent(address,bytes32,bytes)'))[index];
            return {
                log,
                l1BatchTxId: receipt.l1BatchTxIndex
            };
        }
        async _getWithdrawalL2ToL1Log(withdrawalHash, index = 0) {
            const hash = ethers_1.ethers.utils.hexlify(withdrawalHash);
            const receipt = await this._providerL2().getTransactionReceipt(hash);
            const messages = Array.from(receipt.l2ToL1Logs.entries()).filter(([_, log]) => log.sender == utils_1.L1_MESSENGER_ADDRESS);
            const [l2ToL1LogIndex, l2ToL1Log] = messages[index];
            return {
                l2ToL1LogIndex,
                l2ToL1Log
            };
        }
        async finalizeWithdrawalParams(withdrawalHash, index = 0) {
            const { log, l1BatchTxId } = await this._getWithdrawalLog(withdrawalHash, index);
            const { l2ToL1LogIndex } = await this._getWithdrawalL2ToL1Log(withdrawalHash, index);
            const sender = ethers_1.ethers.utils.hexDataSlice(log.topics[1], 12);
            const proof = await this._providerL2().getLogProof(withdrawalHash, l2ToL1LogIndex);
            const message = ethers_1.ethers.utils.defaultAbiCoder.decode(['bytes'], log.data)[0];
            return {
                l1BatchNumber: log.l1BatchNumber,
                l2MessageIndex: proof.id,
                l2TxNumberInBlock: l1BatchTxId,
                message,
                sender,
                proof: proof.proof
            };
        }
        async finalizeWithdrawal(withdrawalHash, index = 0, overrides) {
            const { l1BatchNumber, l2MessageIndex, l2TxNumberInBlock, message, sender, proof } = await this.finalizeWithdrawalParams(withdrawalHash, index);
            if ((0, utils_1.isETH)(sender)) {
                const withdrawTo = ethers_1.ethers.utils.hexDataSlice(message, 4, 24);
                const l1Bridges = await this.getL1BridgeContracts();
                // If the destination address matches the address of the L1 WETH contract,
                // the withdrawal request is processed through the WETH bridge.
                if (withdrawTo.toLowerCase() == l1Bridges.weth.address.toLowerCase()) {
                    return await l1Bridges.weth.finalizeWithdrawal(l1BatchNumber, l2MessageIndex, l2TxNumberInBlock, message, proof, overrides !== null && overrides !== void 0 ? overrides : {});
                }
                const contractAddress = await this._providerL2().getMainContractAddress();
                const zksync = typechain_1.IZkSyncFactory.connect(contractAddress, this._signerL1());
                return await zksync.finalizeEthWithdrawal(l1BatchNumber, l2MessageIndex, l2TxNumberInBlock, message, proof, overrides !== null && overrides !== void 0 ? overrides : {});
            }
            const l2Bridge = typechain_1.IL2BridgeFactory.connect(sender, this._providerL2());
            const l1Bridge = typechain_1.IL1BridgeFactory.connect(await l2Bridge.l1Bridge(), this._signerL1());
            return await l1Bridge.finalizeWithdrawal(l1BatchNumber, l2MessageIndex, l2TxNumberInBlock, message, proof, overrides !== null && overrides !== void 0 ? overrides : {});
        }
        async isWithdrawalFinalized(withdrawalHash, index = 0) {
            const { log } = await this._getWithdrawalLog(withdrawalHash, index);
            const { l2ToL1LogIndex } = await this._getWithdrawalL2ToL1Log(withdrawalHash, index);
            const sender = ethers_1.ethers.utils.hexDataSlice(log.topics[1], 12);
            // `getLogProof` is called not to get proof but
            // to get the index of the corresponding L2->L1 log,
            // which is returned as `proof.id`.
            const proof = await this._providerL2().getLogProof(withdrawalHash, l2ToL1LogIndex);
            if ((0, utils_1.isETH)(sender)) {
                const contractAddress = await this._providerL2().getMainContractAddress();
                const zksync = typechain_1.IZkSyncFactory.connect(contractAddress, this._signerL1());
                return await zksync.isEthWithdrawalFinalized(log.l1BatchNumber, proof.id);
            }
            const l2Bridge = typechain_1.IL2BridgeFactory.connect(sender, this._providerL2());
            const l1Bridge = typechain_1.IL1BridgeFactory.connect(await l2Bridge.l1Bridge(), this._providerL1());
            return await l1Bridge.isWithdrawalFinalized(log.l1BatchNumber, proof.id);
        }
        async claimFailedDeposit(depositHash, overrides) {
            const receipt = await this._providerL2().getTransactionReceipt(ethers_1.ethers.utils.hexlify(depositHash));
            const successL2ToL1LogIndex = receipt.l2ToL1Logs.findIndex((l2ToL1log) => l2ToL1log.sender == utils_1.BOOTLOADER_FORMAL_ADDRESS && l2ToL1log.key == depositHash);
            const successL2ToL1Log = receipt.l2ToL1Logs[successL2ToL1LogIndex];
            if (successL2ToL1Log.value != ethers_1.ethers.constants.HashZero) {
                throw new Error('Cannot claim successful deposit');
            }
            const tx = await this._providerL2().getTransaction(ethers_1.ethers.utils.hexlify(depositHash));
            // Undo the aliasing, since the Mailbox contract set it as for contract address.
            const l1BridgeAddress = (0, utils_1.undoL1ToL2Alias)(receipt.from);
            const l2BridgeAddress = receipt.to;
            const l1Bridge = typechain_1.IL1BridgeFactory.connect(l1BridgeAddress, this._signerL1());
            const l2Bridge = typechain_1.IL2BridgeFactory.connect(l2BridgeAddress, this._providerL2());
            const calldata = l2Bridge.interface.decodeFunctionData('finalizeDeposit', tx.data);
            const proof = await this._providerL2().getLogProof(depositHash, successL2ToL1LogIndex);
            return await l1Bridge.claimFailedDeposit(calldata['_l1Sender'], calldata['_l1Token'], depositHash, receipt.l1BatchNumber, proof.id, receipt.l1BatchTxIndex, proof.proof, overrides !== null && overrides !== void 0 ? overrides : {});
        }
        async requestExecute(transaction) {
            const requestExecuteTx = await this.getRequestExecuteTx(transaction);
            return this._providerL2().getPriorityOpResponse(await this._signerL1().sendTransaction(requestExecuteTx));
        }
        async estimateGasRequestExecute(transaction) {
            const requestExecuteTx = await this.getRequestExecuteTx(transaction);
            delete requestExecuteTx.gasPrice;
            delete requestExecuteTx.maxFeePerGas;
            delete requestExecuteTx.maxPriorityFeePerGas;
            return this._providerL1().estimateGas(requestExecuteTx);
        }
        async getRequestExecuteTx(transaction) {
            var _a, _b, _c, _d, _e, _f, _g, _h;
            const zksyncContract = await this.getMainContract();
            const { ...tx } = transaction;
            (_a = tx.l2Value) !== null && _a !== void 0 ? _a : (tx.l2Value = ethers_1.BigNumber.from(0));
            (_b = tx.operatorTip) !== null && _b !== void 0 ? _b : (tx.operatorTip = ethers_1.BigNumber.from(0));
            (_c = tx.factoryDeps) !== null && _c !== void 0 ? _c : (tx.factoryDeps = []);
            (_d = tx.overrides) !== null && _d !== void 0 ? _d : (tx.overrides = {});
            (_e = tx.gasPerPubdataByte) !== null && _e !== void 0 ? _e : (tx.gasPerPubdataByte = utils_1.REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_LIMIT);
            (_f = tx.refundRecipient) !== null && _f !== void 0 ? _f : (tx.refundRecipient = await this.getAddress());
            (_g = tx.l2GasLimit) !== null && _g !== void 0 ? _g : (tx.l2GasLimit = await this._providerL2().estimateL1ToL2Execute(transaction));
            const { contractAddress, l2Value, calldata, l2GasLimit, factoryDeps, operatorTip, overrides, gasPerPubdataByte, refundRecipient } = tx;
            await insertGasPrice(this._providerL1(), overrides);
            const gasPriceForEstimation = overrides.maxFeePerGas || overrides.gasPrice;
            const baseCost = await this.getBaseCost({
                gasPrice: await gasPriceForEstimation,
                gasPerPubdataByte,
                gasLimit: l2GasLimit
            });
            (_h = overrides.value) !== null && _h !== void 0 ? _h : (overrides.value = baseCost.add(operatorTip).add(l2Value));
            await (0, utils_1.checkBaseCost)(baseCost, overrides.value);
            return await zksyncContract.populateTransaction.requestL2Transaction(contractAddress, l2Value, calldata, l2GasLimit, utils_1.REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_LIMIT, factoryDeps, refundRecipient, overrides);
        }
    };
}
exports.AdapterL1 = AdapterL1;
function AdapterL2(Base) {
    return class Adapter extends Base {
        _providerL2() {
            throw new Error('Must be implemented by the derived class!');
        }
        _signerL2() {
            throw new Error('Must be implemented by the derived class!');
        }
        async getBalance(token, blockTag = 'committed') {
            return await this._providerL2().getBalance(await this.getAddress(), blockTag, token);
        }
        async getAllBalances() {
            return await this._providerL2().getAllAccountBalances(await this.getAddress());
        }
        async getL2BridgeContracts() {
            const addresses = await this._providerL2().getDefaultBridgeAddresses();
            return {
                erc20: typechain_1.IL2BridgeFactory.connect(addresses.erc20L2, this._signerL2()),
                weth: typechain_1.IL2BridgeFactory.connect(addresses.wethL2, this._signerL2())
            };
        }
        _fillCustomData(data) {
            var _a, _b;
            const customData = { ...data };
            (_a = customData.gasPerPubdata) !== null && _a !== void 0 ? _a : (customData.gasPerPubdata = utils_1.DEFAULT_GAS_PER_PUBDATA_LIMIT);
            (_b = customData.factoryDeps) !== null && _b !== void 0 ? _b : (customData.factoryDeps = []);
            return customData;
        }
        async withdraw(transaction) {
            const withdrawTx = await this._providerL2().getWithdrawTx({
                from: await this.getAddress(),
                ...transaction
            });
            const txResponse = await this.sendTransaction(withdrawTx);
            return this._providerL2()._wrapTransaction(txResponse);
        }
        async transfer(transaction) {
            const transferTx = await this._providerL2().getTransferTx({
                from: await this.getAddress(),
                ...transaction
            });
            const txResponse = await this.sendTransaction(transferTx);
            return this._providerL2()._wrapTransaction(txResponse);
        }
    };
}
exports.AdapterL2 = AdapterL2;
/// @dev This method checks if the overrides contain a gasPrice (or maxFeePerGas), if not it will insert
/// the maxFeePerGas
async function insertGasPrice(l1Provider, overrides) {
    if (!overrides.gasPrice && !overrides.maxFeePerGas) {
        const l1FeeData = await l1Provider.getFeeData();
        // Sometimes baseFeePerGas is not available, so we use gasPrice instead.
        const baseFee = l1FeeData.lastBaseFeePerGas || l1FeeData.gasPrice;
        // ethers.js by default uses multiplcation by 2, but since the price for the L2 part
        // will depend on the L1 part, doubling base fee is typically too much.
        const maxFeePerGas = baseFee.mul(3).div(2).add(l1FeeData.maxPriorityFeePerGas);
        overrides.maxFeePerGas = maxFeePerGas;
        overrides.maxPriorityFeePerGas = l1FeeData.maxPriorityFeePerGas;
    }
}
