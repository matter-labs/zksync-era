// SPDX-License-Identifier: MIT

pragma solidity ^0.8.13;

import {Base} from "./Base.sol";
import {COMMIT_TIMESTAMP_NOT_OLDER, COMMIT_TIMESTAMP_APPROXIMATION_DELTA, EMPTY_STRING_KECCAK, L2_TO_L1_LOG_SERIALIZE_SIZE, MAX_INITIAL_STORAGE_CHANGES_COMMITMENT_BYTES, MAX_REPEATED_STORAGE_CHANGES_COMMITMENT_BYTES, MAX_L2_TO_L1_LOGS_COMMITMENT_BYTES, PACKED_L2_BLOCK_TIMESTAMP_MASK, PUBLIC_INPUT_SHIFT} from "../Config.sol";
import {IExecutor, L2_LOG_ADDRESS_OFFSET, L2_LOG_KEY_OFFSET, L2_LOG_VALUE_OFFSET, SystemLogKey} from "../interfaces/IExecutor.sol";
import {PriorityQueue, PriorityOperation} from "../libraries/PriorityQueue.sol";
import {UncheckedMath} from "../../common/libraries/UncheckedMath.sol";
import {UnsafeBytes} from "../../common/libraries/UnsafeBytes.sol";
import {L2ContractHelper} from "../../common/libraries/L2ContractHelper.sol";
import {VerifierParams} from "../Storage.sol";
import {L2_BOOTLOADER_ADDRESS, L2_TO_L1_MESSENGER_SYSTEM_CONTRACT_ADDR, L2_SYSTEM_CONTEXT_SYSTEM_CONTRACT_ADDR, L2_KNOWN_CODE_STORAGE_SYSTEM_CONTRACT_ADDR} from "../../common/L2ContractAddresses.sol";

/// @title zkSync Executor contract capable of processing events emitted in the zkSync protocol.
/// @author Matter Labs
/// @custom:security-contact security@matterlabs.dev
contract ExecutorFacet is Base, IExecutor {
    using UncheckedMath for uint256;
    using PriorityQueue for PriorityQueue.Queue;

    string public constant override getName = "ExecutorFacet";

    /// @dev Process one batch commit using the previous batch StoredBatchInfo
    /// @dev returns new batch StoredBatchInfo
    /// @notice Does not change storage
    function _commitOneBatch(
        StoredBatchInfo memory _previousBatch,
        CommitBatchInfo calldata _newBatch,
        bytes32 _expectedSystemContractUpgradeTxHash
    ) internal view returns (StoredBatchInfo memory) {
        require(_newBatch.batchNumber == _previousBatch.batchNumber + 1, "f"); // only commit next batch

        // Check that batch contain all meta information for L2 logs.
        // Get the chained hash of priority transaction hashes.
        (
            uint256 expectedNumberOfLayer1Txs,
            bytes32 expectedPriorityOperationsHash,
            bytes32 previousBatchHash,
            bytes32 stateDiffHash,
            bytes32 l2LogsTreeRoot,
            uint256 packedBatchAndL2BlockTimestamp
        ) = _processL2Logs(_newBatch, _expectedSystemContractUpgradeTxHash);

        require(_previousBatch.batchHash == previousBatchHash, "l");
        // Check that the priority operation hash in the L2 logs is as expected
        require(expectedPriorityOperationsHash == _newBatch.priorityOperationsHash, "t");
        // Check that the number of processed priority operations is as expected
        require(expectedNumberOfLayer1Txs == _newBatch.numberOfLayer1Txs, "ta");

        // Check the timestamp of the new batch
        _verifyBatchTimestamp(packedBatchAndL2BlockTimestamp, _newBatch.timestamp, _previousBatch.timestamp);

        // Create batch commitment for the proof verification
        bytes32 commitment = _createBatchCommitment(_newBatch, stateDiffHash);

        return
            StoredBatchInfo(
                _newBatch.batchNumber,
                _newBatch.newStateRoot,
                _newBatch.indexRepeatedStorageChanges,
                _newBatch.numberOfLayer1Txs,
                _newBatch.priorityOperationsHash,
                l2LogsTreeRoot,
                _newBatch.timestamp,
                commitment
            );
    }

    /// @notice checks that the timestamps of both the new batch and the new L2 block are correct.
    /// @param _packedBatchAndL2BlockTimestamp - packed batch and L2 block timestamp in a format of batchTimestamp * 2**128 + l2BatchTimestamp
    /// @param _expectedBatchTimestamp - expected batch timestamp
    /// @param _previousBatchTimestamp - the timestamp of the previous batch
    function _verifyBatchTimestamp(
        uint256 _packedBatchAndL2BlockTimestamp,
        uint256 _expectedBatchTimestamp,
        uint256 _previousBatchTimestamp
    ) internal view {
        // Check that the timestamp that came from the system context is expected
        uint256 batchTimestamp = _packedBatchAndL2BlockTimestamp >> 128;
        require(batchTimestamp == _expectedBatchTimestamp, "tb");

        // While the fact that _previousBatchTimestamp < batchTimestamp is already checked on L2,
        // we double check it here for clarity
        require(_previousBatchTimestamp < batchTimestamp, "h3");

        uint256 lastL2BlockTimestamp = _packedBatchAndL2BlockTimestamp & PACKED_L2_BLOCK_TIMESTAMP_MASK;

        // All L2 blocks have timestamps within the range of [batchTimestamp, lastL2BatchTimestamp].
        // So here we need to only double check that:
        // - The timestamp of the batch is not too small.
        // - The timestamp of the last L2 block is not too big.
        require(block.timestamp - COMMIT_TIMESTAMP_NOT_OLDER <= batchTimestamp, "h1"); // New batch timestamp is too small
        require(lastL2BlockTimestamp <= block.timestamp + COMMIT_TIMESTAMP_APPROXIMATION_DELTA, "h2"); // The last L2 block timestamp is too big
    }

    /// @dev Check that L2 logs are proper and batch contain all meta information for them
    /// @dev The logs processed here should line up such that only one log for each key from the
    ///      SystemLogKey enum in Constants.sol is processed per new batch.
    /// @dev Data returned from here will be used to form the batch commitment.
    function _processL2Logs(CommitBatchInfo calldata _newBatch, bytes32 _expectedSystemContractUpgradeTxHash)
        internal
        pure
        returns (
            uint256 numberOfLayer1Txs,
            bytes32 chainedPriorityTxsHash,
            bytes32 previousBatchHash,
            bytes32 stateDiffHash,
            bytes32 l2LogsTreeRoot,
            uint256 packedBatchAndL2BlockTimestamp
        )
    {
        // Copy L2 to L1 logs into memory.
        bytes memory emittedL2Logs = _newBatch.systemLogs;

        // Used as bitmap to set/check log processing happens exactly once.
        // See SystemLogKey enum in Constants.sol for ordering.
        uint256 processedLogs;

        // bytes32 providedL2ToL1PubdataHash = _newBatch.totalL2ToL1PubdataHash;

        // linear traversal of the logs
        for (uint256 i = 0; i < emittedL2Logs.length; i = i.uncheckedAdd(L2_TO_L1_LOG_SERIALIZE_SIZE)) {
            // Extract the values to be compared to/used such as the log sender, key, and value
            (address logSender, ) = UnsafeBytes.readAddress(emittedL2Logs, i + L2_LOG_ADDRESS_OFFSET);
            (uint256 logKey, ) = UnsafeBytes.readUint256(emittedL2Logs, i + L2_LOG_KEY_OFFSET);
            (bytes32 logValue, ) = UnsafeBytes.readBytes32(emittedL2Logs, i + L2_LOG_VALUE_OFFSET);

            // Ensure that the log hasn't been processed already
            require(!_checkBit(processedLogs, uint8(logKey)), "kp");
            processedLogs = _setBit(processedLogs, uint8(logKey));

            // Need to check that each log was sent by the correct address.
            if (logKey == uint256(SystemLogKey.L2_TO_L1_LOGS_TREE_ROOT_KEY)) {
                require(logSender == L2_TO_L1_MESSENGER_SYSTEM_CONTRACT_ADDR, "lm");
                l2LogsTreeRoot = logValue;
            } else if (logKey == uint256(SystemLogKey.TOTAL_L2_TO_L1_PUBDATA_KEY)) {
                // require(logSender == L2_TO_L1_MESSENGER_SYSTEM_CONTRACT_ADDR, "ln");
                // require(providedL2ToL1PubdataHash == logValue, "wp");
            } else if (logKey == uint256(SystemLogKey.STATE_DIFF_HASH_KEY)) {
                require(logSender == L2_TO_L1_MESSENGER_SYSTEM_CONTRACT_ADDR, "lb");
                stateDiffHash = logValue;
            } else if (logKey == uint256(SystemLogKey.PACKED_BATCH_AND_L2_BLOCK_TIMESTAMP_KEY)) {
                require(logSender == L2_SYSTEM_CONTEXT_SYSTEM_CONTRACT_ADDR, "sc");
                packedBatchAndL2BlockTimestamp = uint256(logValue);
            } else if (logKey == uint256(SystemLogKey.PREV_BATCH_HASH_KEY)) {
                require(logSender == L2_SYSTEM_CONTEXT_SYSTEM_CONTRACT_ADDR, "sv");
                previousBatchHash = logValue;
            } else if (logKey == uint256(SystemLogKey.CHAINED_PRIORITY_TXN_HASH_KEY)) {
                require(logSender == L2_BOOTLOADER_ADDRESS, "bl");
                chainedPriorityTxsHash = logValue;
            } else if (logKey == uint256(SystemLogKey.NUMBER_OF_LAYER_1_TXS_KEY)) {
                require(logSender == L2_BOOTLOADER_ADDRESS, "bk");
                numberOfLayer1Txs = uint256(logValue);
            } else if (logKey == uint256(SystemLogKey.EXPECTED_SYSTEM_CONTRACT_UPGRADE_TX_HASH_KEY)) {
                require(logSender == L2_BOOTLOADER_ADDRESS, "bu");
                require(_expectedSystemContractUpgradeTxHash == logValue, "ut");
            } else {
                revert("ul");
            }
        }

        // We only require 7 logs to be checked, the 8th is if we are expecting a protocol upgrade
        // Without the protocol upgrade we expect 7 logs: 2^7 - 1 = 127
        // With the protocol upgrade we expect 8 logs: 2^8 - 1 = 255
        if (_expectedSystemContractUpgradeTxHash == bytes32(0)) {
            require(processedLogs == 127, "b7");
        } else {
            require(processedLogs == 255, "b8");
        }
    }

    /// @notice Commit batch
    /// @notice 1. Checks timestamp.
    /// @notice 2. Process L2 logs.
    /// @notice 3. Store batch commitments.
    function commitBatches(StoredBatchInfo memory _lastCommittedBatchData, CommitBatchInfo[] calldata _newBatchesData)
        external
        override
        nonReentrant
        onlyValidator
    {
        // Check that we commit batches after last committed batch
        require(s.storedBatchHashes[s.totalBatchesCommitted] == _hashStoredBatchInfo(_lastCommittedBatchData), "i"); // incorrect previous batch data
        require(_newBatchesData.length > 0, "No batches to commit");

        bytes32 systemContractsUpgradeTxHash = s.l2SystemContractsUpgradeTxHash;
        // Upgrades are rarely done so we optimize a case with no active system contracts upgrade.
        if (systemContractsUpgradeTxHash == bytes32(0) || s.l2SystemContractsUpgradeBatchNumber != 0) {
            _commitBatchesWithoutSystemContractsUpgrade(_lastCommittedBatchData, _newBatchesData);
        } else {
            _commitBatchesWithSystemContractsUpgrade(
                _lastCommittedBatchData,
                _newBatchesData,
                systemContractsUpgradeTxHash
            );
        }

        s.totalBatchesCommitted = s.totalBatchesCommitted + _newBatchesData.length;
    }

    /// @dev Commits new batches without any system contracts upgrade.
    /// @param _lastCommittedBatchData The data of the last committed batch.
    /// @param _newBatchesData An array of batch data that needs to be committed.
    function _commitBatchesWithoutSystemContractsUpgrade(
        StoredBatchInfo memory _lastCommittedBatchData,
        CommitBatchInfo[] calldata _newBatchesData
    ) internal {
        for (uint256 i = 0; i < _newBatchesData.length; i = i.uncheckedInc()) {
            _lastCommittedBatchData = _commitOneBatch(_lastCommittedBatchData, _newBatchesData[i], bytes32(0));

            s.storedBatchHashes[_lastCommittedBatchData.batchNumber] = _hashStoredBatchInfo(_lastCommittedBatchData);
            emit BlockCommit(
                _lastCommittedBatchData.batchNumber,
                _lastCommittedBatchData.batchHash,
                _lastCommittedBatchData.commitment
            );
        }
    }

    /// @dev Commits new batches with a system contracts upgrade transaction.
    /// @param _lastCommittedBatchData The data of the last committed batch.
    /// @param _newBatchesData An array of batch data that needs to be committed.
    /// @param _systemContractUpgradeTxHash The transaction hash of the system contract upgrade.
    function _commitBatchesWithSystemContractsUpgrade(
        StoredBatchInfo memory _lastCommittedBatchData,
        CommitBatchInfo[] calldata _newBatchesData,
        bytes32 _systemContractUpgradeTxHash
    ) internal {
        // The system contract upgrade is designed to be executed atomically with the new bootloader, a default account,
        // ZKP verifier, and other system parameters. Hence, we ensure that the upgrade transaction is
        // carried out within the first batch committed after the upgrade.

        // While the logic of the contract ensures that the s.l2SystemContractsUpgradeBatchNumber is 0 when this function is called,
        // this check is added just in case. Since it is a hot read, it does not encure noticable gas cost.
        require(s.l2SystemContractsUpgradeBatchNumber == 0, "ik");

        // Save the batch number where the upgrade transaction was executed.
        s.l2SystemContractsUpgradeBatchNumber = _newBatchesData[0].batchNumber;

        for (uint256 i = 0; i < _newBatchesData.length; i = i.uncheckedInc()) {
            // The upgrade transaction must only be included in the first batch.
            bytes32 expectedUpgradeTxHash = i == 0 ? _systemContractUpgradeTxHash : bytes32(0);
            _lastCommittedBatchData = _commitOneBatch(
                _lastCommittedBatchData,
                _newBatchesData[i],
                expectedUpgradeTxHash
            );

            s.storedBatchHashes[_lastCommittedBatchData.batchNumber] = _hashStoredBatchInfo(_lastCommittedBatchData);
            emit BlockCommit(
                _lastCommittedBatchData.batchNumber,
                _lastCommittedBatchData.batchHash,
                _lastCommittedBatchData.commitment
            );
        }
    }

    /// @dev Pops the priority operations from the priority queue and returns a rolling hash of operations
    function _collectOperationsFromPriorityQueue(uint256 _nPriorityOps) internal returns (bytes32 concatHash) {
        concatHash = EMPTY_STRING_KECCAK;

        for (uint256 i = 0; i < _nPriorityOps; i = i.uncheckedInc()) {
            PriorityOperation memory priorityOp = s.priorityQueue.popFront();
            concatHash = keccak256(abi.encode(concatHash, priorityOp.canonicalTxHash));
        }
    }

    /// @dev Executes one batch
    /// @dev 1. Processes all pending operations (Complete priority requests)
    /// @dev 2. Finalizes batch on Ethereum
    /// @dev _executedBatchIdx is an index in the array of the batches that we want to execute together
    function _executeOneBatch(StoredBatchInfo memory _storedBatch, uint256 _executedBatchIdx) internal {
        uint256 currentBatchNumber = _storedBatch.batchNumber;
        require(currentBatchNumber == s.totalBatchesExecuted + _executedBatchIdx + 1, "k"); // Execute batches in order
        require(
            _hashStoredBatchInfo(_storedBatch) == s.storedBatchHashes[currentBatchNumber],
            "exe10" // executing batch should be committed
        );

        bytes32 priorityOperationsHash = _collectOperationsFromPriorityQueue(_storedBatch.numberOfLayer1Txs);
        require(priorityOperationsHash == _storedBatch.priorityOperationsHash, "x"); // priority operations hash does not match to expected

        // Save root hash of L2 -> L1 logs tree
        s.l2LogsRootHashes[currentBatchNumber] = _storedBatch.l2LogsTreeRoot;
    }

    /// @notice Execute batches, complete priority operations and process withdrawals.
    /// @notice 1. Processes all pending operations (Complete priority requests)
    /// @notice 2. Finalizes batch on Ethereum
    function executeBatches(StoredBatchInfo[] calldata _batchesData) external nonReentrant onlyValidator {
        uint256 nBatches = _batchesData.length;
        for (uint256 i = 0; i < nBatches; i = i.uncheckedInc()) {
            _executeOneBatch(_batchesData[i], i);
            emit BlockExecution(_batchesData[i].batchNumber, _batchesData[i].batchHash, _batchesData[i].commitment);
        }

        uint256 newTotalBatchesExecuted = s.totalBatchesExecuted + nBatches;
        s.totalBatchesExecuted = newTotalBatchesExecuted;
        require(newTotalBatchesExecuted <= s.totalBatchesVerified, "n"); // Can't execute batches more than committed and proven currently.

        uint256 batchWhenUpgradeHappened = s.l2SystemContractsUpgradeBatchNumber;
        if (batchWhenUpgradeHappened != 0 && batchWhenUpgradeHappened <= newTotalBatchesExecuted) {
            delete s.l2SystemContractsUpgradeTxHash;
            delete s.l2SystemContractsUpgradeBatchNumber;
        }
    }

    /// @notice Batches commitment verification.
    /// @notice Only verifies batch commitments without any other processing
    function proveBatches(
        StoredBatchInfo calldata _prevBatch,
        StoredBatchInfo[] calldata _committedBatches,
        ProofInput calldata _proof
    ) external nonReentrant onlyValidator {
        // Save the variables into the stack to save gas on reading them later
        uint256 currentTotalBatchesVerified = s.totalBatchesVerified;
        uint256 committedBatchesLength = _committedBatches.length;

        // Save the variable from the storage to memory to save gas
        VerifierParams memory verifierParams = s.verifierParams;

        // Initialize the array, that will be used as public input to the ZKP
        uint256[] memory proofPublicInput = new uint256[](committedBatchesLength);

        // Check that the batch passed by the validator is indeed the first unverified batch
        require(_hashStoredBatchInfo(_prevBatch) == s.storedBatchHashes[currentTotalBatchesVerified], "t1");

        bytes32 prevBatchCommitment = _prevBatch.commitment;
        for (uint256 i = 0; i < committedBatchesLength; i = i.uncheckedInc()) {
            currentTotalBatchesVerified = currentTotalBatchesVerified.uncheckedInc();
            require(
                _hashStoredBatchInfo(_committedBatches[i]) == s.storedBatchHashes[currentTotalBatchesVerified],
                "o1"
            );

            bytes32 currentBatchCommitment = _committedBatches[i].commitment;
            proofPublicInput[i] = _getBatchProofPublicInput(
                prevBatchCommitment,
                currentBatchCommitment,
                verifierParams
            );

            prevBatchCommitment = currentBatchCommitment;
        }
        require(currentTotalBatchesVerified <= s.totalBatchesCommitted, "q");

        // #if DUMMY_VERIFIER

        // Additional level of protection for the mainnet
        assert(block.chainid != 1);
        // We allow skipping the zkp verification for the test(net) environment
        // If the proof is not empty, verify it, otherwise, skip the verification
        if (_proof.serializedProof.length > 0) {
            bool successVerifyProof = s.verifier.verify(
                proofPublicInput,
                _proof.serializedProof,
                _proof.recursiveAggregationInput
            );
            require(successVerifyProof, "p"); // Proof verification fail
        }
        // #else
        bool successVerifyProof = s.verifier.verify(
            proofPublicInput,
            _proof.serializedProof,
            _proof.recursiveAggregationInput
        );
        require(successVerifyProof, "p"); // Proof verification fail
        // #endif

        emit BlocksVerification(s.totalBatchesVerified, currentTotalBatchesVerified);
        s.totalBatchesVerified = currentTotalBatchesVerified;
    }

    /// @dev Gets zk proof public input
    function _getBatchProofPublicInput(
        bytes32 _prevBatchCommitment,
        bytes32 _currentBatchCommitment,
        VerifierParams memory _verifierParams
    ) internal pure returns (uint256) {
        return
            uint256(
                keccak256(
                    abi.encodePacked(
                        _prevBatchCommitment,
                        _currentBatchCommitment,
                        _verifierParams.recursionNodeLevelVkHash,
                        _verifierParams.recursionLeafLevelVkHash
                    )
                )
            ) >> PUBLIC_INPUT_SHIFT;
    }

    /// @notice Reverts unexecuted batches
    /// @param _newLastBatch batch number after which batches should be reverted
    /// NOTE: Doesn't delete the stored data about batches, but only decreases
    /// counters that are responsible for the number of batches
    function revertBatches(uint256 _newLastBatch) external nonReentrant onlyValidator {
        require(s.totalBatchesCommitted > _newLastBatch, "v1"); // The last committed batch is less than new last batch
        require(_newLastBatch >= s.totalBatchesExecuted, "v2"); // Already executed batches cannot be reverted

        if (_newLastBatch < s.totalBatchesVerified) {
            s.totalBatchesVerified = _newLastBatch;
        }
        s.totalBatchesCommitted = _newLastBatch;

        // Reset the batch number of the executed system contracts upgrade transaction if the batch
        // where the system contracts upgrade was committed is among the reverted batches.
        if (s.l2SystemContractsUpgradeBatchNumber > _newLastBatch) {
            delete s.l2SystemContractsUpgradeBatchNumber;
        }

        emit BlocksRevert(s.totalBatchesCommitted, s.totalBatchesVerified, s.totalBatchesExecuted);
    }

    /// @notice Returns larger of two values
    function _maxU256(uint256 a, uint256 b) internal pure returns (uint256) {
        return a < b ? b : a;
    }

    /// @dev Creates batch commitment from its data
    function _createBatchCommitment(CommitBatchInfo calldata _newBatchData, bytes32 _stateDiffHash)
        internal
        view
        returns (bytes32)
    {
        bytes32 passThroughDataHash = keccak256(_batchPassThroughData(_newBatchData));
        bytes32 metadataHash = keccak256(_batchMetaParameters());
        bytes32 auxiliaryOutputHash = keccak256(_batchAuxiliaryOutput(_newBatchData, _stateDiffHash));

        return keccak256(abi.encode(passThroughDataHash, metadataHash, auxiliaryOutputHash));
    }

    function _batchPassThroughData(CommitBatchInfo calldata _batch) internal pure returns (bytes memory) {
        return
            abi.encodePacked(
                _batch.indexRepeatedStorageChanges,
                _batch.newStateRoot,
                uint64(0), // index repeated storage changes in zkPorter
                bytes32(0) // zkPorter batch hash
            );
    }

    function _batchMetaParameters() internal view returns (bytes memory) {
        return abi.encodePacked(s.zkPorterIsAvailable, s.l2BootloaderBytecodeHash, s.l2DefaultAccountBytecodeHash);
    }

    function _batchAuxiliaryOutput(CommitBatchInfo calldata _batch, bytes32 _stateDiffHash)
        internal
        pure
        returns (bytes memory)
    {
        require(_batch.systemLogs.length <= MAX_L2_TO_L1_LOGS_COMMITMENT_BYTES, "pu");

        bytes32 l2ToL1LogsHash = keccak256(_batch.systemLogs);

        return
            abi.encode(
                l2ToL1LogsHash,
                _stateDiffHash,
                _batch.bootloaderHeapInitialContentsHash,
                _batch.eventsQueueStateHash
            );
    }

    /// @notice Returns the keccak hash of the ABI-encoded StoredBatchInfo
    function _hashStoredBatchInfo(StoredBatchInfo memory _storedBatchInfo) internal pure returns (bytes32) {
        return keccak256(abi.encode(_storedBatchInfo));
    }

    /// @notice Returns true if the bit at index {_index} is 1
    function _checkBit(uint256 _bitMap, uint8 _index) internal pure returns (bool) {
        return (_bitMap & (1 << _index)) > 0;
    }

    /// @notice Sets the given bit in {_num} at index {_index} to 1.
    function _setBit(uint256 _bitMap, uint8 _index) internal pure returns (uint256) {
        return _bitMap | (1 << _index);
    }
}
