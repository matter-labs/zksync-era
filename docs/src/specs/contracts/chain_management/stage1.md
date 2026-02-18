# 

The goal is to achieve a simple, efficient, and secure solution that ensures users can withdraw their funds via L1, even if issues arise on L2 due to operator censorship, technical failures, or other disruptions.

## Requirements for a stage1 chain

1. Users are able to exit without the help of the permissioned operators.
2. In case of an unwanted upgrade by actors more centralized than a [Security Council](https://l2beat.com/glossary#security-council), users have at least 7d to exit.
3. The only way (other than implementation bug) for a rollup to indefinitely block an L2→L1 message (e.g. a withdrawal) or push an invalid L2→L1 message (e.g. an invalid withdrawal) is by compromising ≥75% of the Security Council.

Some clarification of the requirements:

1. The loss of key access by a minority of Security Council (4/12) members should **NOT** block permissionless withdrawals procedure. 
2. Users must have at least 7 days to exit during the emergency state, even if **Normal Protocol Upgrade** is executed on L2.
3. 9/12 Security Council members may still have a lot of power, including selecting new validator, freezing or upgrading contracts at any moment.

## Proposed Solution

Some terminology I will be using:

- **Escape Hatch:** A general term for the L2 mechanism that enables users to withdraw their funds via L1 in the event of L2 disruptions.
- **Priority Mode:** ZKsync’s escape hatch. It is a special state of the system in which censorship resistance is enforced by Ethereum. In other words, valid transactions can be executed on the ZK chain as long as Ethereum remains censorship-resistant.

Usually, we want only the permissioned validators to have the ability to submit new batches for the best UX. And only if censorship actually happens - allow users to permissionlesly withdraw. This approach allows minimal code changes and also stays efficient in the day-to-day operations. 

Under normal operation, user may send signed transactions to operator RPC or request L1 -> L2 transactions on `L1BridgeHub` contract. The requested transactions will be eventually executed on L2. However, if an operator starts censoring for whatever reason - the special mode should be activated (Priority Mode). In the Priority Mode, the system should allow users to request transactions on L1, and only those transactions should be included in the consequent batches. After some time of operating the escape hatch mechanism, the ZK Governance should be able to restore normal mode operation.

### 🚫 Limitations

- Only chains that settle to L1 have the support for "Stage 1".
- Actions that can only be created by executing L2 transactions and will not be possible in Priority Mode:
    - Some Account Abstraction wallets on Era
    - Deployment transactions from EOA
    - Keyless transactions

## Normal operation

Operator runs the node in the normal mode.

- Transactions that were accepted through the RPC are executed.
- Transactions that have been sent to the priority queue (priority transactions or L1 → L2 transactions) are executed during 4 **days** window.

## Criteria to activate priority mode

The only ‘objective’ way to prove that operator is censoring is L1. If there are priority transactions on L1 that were requested 4 days before and not yet executed - Priority Mode can be activated. 

The priority mode can be activated immediately permissionless by any account on L1 via `AdminFacet.activatePriorityMode`.

## Priority mode

From now, not only the whitelisted operator can commit/verify/execute batches. Instead anyone can do it, with some differences:

1. Everyone can commit/verify/execute batches in one function though **`PermissionlessValidator`** smart contract. 
2. Only priority transactions can be executed in the batch. 

### PermissionlessValidator

`PermissionlessValidator` is a simple smart contract that implements one function `settleBatchesSharedBridge`. The function is not restricted and anyone can call it. It simply forward the `commit` / `prove` / `execute` to the chain contract. It is expected that the chain contract knows about permissionless validator contract and allow it to settle batches when and only when the Priority Mode is activated.

### Only L1 → L2 transactions can be executed in Priority Mode

The goal of Priority Mode is to solve chain censorship, and it is sufficient to allow processing only L1 → L2 transactions in Priority Mode batches.

At the same time, allowing L2 transactions complicates the process and increases the attack surface. For example, if someone finds a way to produce batches faster than everyone else, they could become the sole entity processing batches in Priority Mode and effectively impose censorship. It is just harder to reason about the system’s properties when L2 transactions are allowed.

Please note that when the system is fully operational in normal mode, all types of transactions are possible. If L2 transactions are not allowed in Priority Mode, the system does not behave the same way as in normal mode, and some actions that are possible with normal L2 transactions may not be possible via L1 → L2 transactions. 

On ZKsync OS, we added a new field `numberOfLayer2Txs` to `CommitBatchInfoZKsyncOS`. ZKsync OS maintains a counter for L2 transactions and propagates it to the batch commitment.

On ZKsync Era, we reused the existing L2 → L1 log that records the number of L1 transactions executed in a batch. Previously, this log contained a single uint256 field representing the number of L1 transactions executed. Now, we use the lower half of the word to store the number of L1 transactions and the upper half to store the number of L2 transactions executed.

## Back to normal mode

We want to allow chains that have activated Priority Mode to restore normal operation (e.g. in case it was activated because of a bug). Only ZK Governance can do this, and the assumption is that ZK Governance can already perform more severe actions (e.g., changing the implementation of L1 smart contracts). Therefore, granting ZK Governance the ability to restore normal operation does not change the security assumptions. It just provides a convenient interface for Priority Mode deactivation.

```solidity
    function deactivatePriorityMode() external onlyPriorityMode onlyChainTypeManager onlySettlementLayer onlyL1 {
        s.priorityModeInfo.activated = false;
        emit PriorityModeDeactivated();
    }
```

## Exit window

The exit window is an important property for users. It is the time period during which a user may submit a transaction for execution, and during which it is expected to be processed regardless of whether a centralized party censors it or not and it is guaranteed the system properties won’t be changed during this time. 

In our case, system properties can be changed through a protocol upgrade. We assume that an Emergency Protocol Upgrade will not be used to exploit the system, and we calculate the exit window by considering a Normal Protocol Upgrade and system freezing. This is consistent with the L2Beat `Stage 1` requirements.

We really want to account for anything that can disturb the users guarantee to get their transaction processed on L2. 

1. **`HardFreeze`** – Only 9/12 Security Council members can hard-freeze the chain. We consider this action out of scope, in the same way that an Emergency Protocol Upgrade is considered out of scope.
2. **`SoftFreeze`** – It can happen only once for a duration of 12 hours before 9/12 Security Council members are able to trigger it again. We always assume the worst-case impact, namely that a soft freeze has happened.
3. `Normal Protocol Upgrade` - It can change the implementation of the chain. We always assume the worst-case impact, namely that the chain’s security properties may be broken after the upgrade. Therefore, the exit window is calculated from the moment a malicious protocol upgrade is initiated, giving users time up to the point when the upgrade is executed. The start of the upgrade is assumed to be the point at which the governance proposal passes voting and is submitted to L1.

To make the exit window 7 days, we will:

1. Increase the Normal Protocol Upgrade notice period to 12 days. 
2. Allow Priority Mode activation after 4 days of the unprocessed Priority Transaction.

This way, after the malicious upgrade is submitted to L1, users have 7 days to request a transaction, and it will be either processed by the centralized operator or the chain will enter Priority Mode. If the chain enters Priority Mode, then there are at least 1 day to process all transactions for anyone. If the minority of the Security Council performs the soft freeze at any moment, then there are 12 hours for anyone to settle batches.

## Economic attacks and fee configuration

### BaseToken

Firstly, it is important to clarify that in order to perform an L1->L2 one needs the base token of the chain. It implies that the token should be easy enough to get hold off. This will have to be checked onchain before determining wether a chain is "truly" a stage1.

### Fee accounting and base token moves price modifications

One of the ways a malicious admin could prevent deposits is by setting an absurdly high fee for each L1->L2 making it impossible for most users to leave the system. In order to prevent this, there is a speed limit on how fast the price can change (at most 30% in a day).

Note, that even after the priority mode is activated, the chain admin is the only entity that can continue updating the fees for priority transactions. It means that after the stage1 is activated, the fee can in theory become very small and uneconomical for a permissionless validator service to process the priority transactions. It is assumed that in case this event happens, the community will provide fees for their priority transactions in some way offchain or build batches themselves.

Also, note that before the chain is allowed to become stage1, the chain admin has more freedom at setting the fee params for the chain. So before treating a chain as "stage1" it is important to double check that no tampering has been done with the fee params before the transition. 

## What to pay attention to as an auditor

1. Admin is a privileged role in a normal chain mode, however it shouldn’t be able to censor L1 → L2 transactions in the Priority Mode. Examples
    1. Transaction Filtering
    2. Fee configuration
2. Is there any other way a chain admin can damage a chain *after* the priority mode has been activated?
