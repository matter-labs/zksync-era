"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.AccountNonceOrdering = exports.AccountAbstractionVersion = exports.TransactionStatus = exports.PriorityOpTree = exports.PriorityQueueType = exports.Network = void 0;
// Ethereum network
var Network;
(function (Network) {
    Network[Network["Mainnet"] = 1] = "Mainnet";
    Network[Network["Ropsten"] = 3] = "Ropsten";
    Network[Network["Rinkeby"] = 4] = "Rinkeby";
    Network[Network["Goerli"] = 5] = "Goerli";
    Network[Network["Localhost"] = 9] = "Localhost";
})(Network = exports.Network || (exports.Network = {}));
var PriorityQueueType;
(function (PriorityQueueType) {
    PriorityQueueType[PriorityQueueType["Deque"] = 0] = "Deque";
    PriorityQueueType[PriorityQueueType["HeapBuffer"] = 1] = "HeapBuffer";
    PriorityQueueType[PriorityQueueType["Heap"] = 2] = "Heap";
})(PriorityQueueType = exports.PriorityQueueType || (exports.PriorityQueueType = {}));
var PriorityOpTree;
(function (PriorityOpTree) {
    PriorityOpTree[PriorityOpTree["Full"] = 0] = "Full";
    PriorityOpTree[PriorityOpTree["Rollup"] = 1] = "Rollup";
})(PriorityOpTree = exports.PriorityOpTree || (exports.PriorityOpTree = {}));
var TransactionStatus;
(function (TransactionStatus) {
    TransactionStatus["NotFound"] = "not-found";
    TransactionStatus["Processing"] = "processing";
    TransactionStatus["Committed"] = "committed";
    TransactionStatus["Finalized"] = "finalized";
})(TransactionStatus = exports.TransactionStatus || (exports.TransactionStatus = {}));
var AccountAbstractionVersion;
(function (AccountAbstractionVersion) {
    AccountAbstractionVersion[AccountAbstractionVersion["None"] = 0] = "None";
    AccountAbstractionVersion[AccountAbstractionVersion["Version1"] = 1] = "Version1";
})(AccountAbstractionVersion = exports.AccountAbstractionVersion || (exports.AccountAbstractionVersion = {}));
var AccountNonceOrdering;
(function (AccountNonceOrdering) {
    AccountNonceOrdering[AccountNonceOrdering["Sequential"] = 0] = "Sequential";
    AccountNonceOrdering[AccountNonceOrdering["Arbitrary"] = 1] = "Arbitrary";
})(AccountNonceOrdering = exports.AccountNonceOrdering || (exports.AccountNonceOrdering = {}));
