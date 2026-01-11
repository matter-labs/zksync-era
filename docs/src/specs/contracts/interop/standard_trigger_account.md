# Standard Trigger Account

[Interop transactions](./interop_center/interop_trigger.md) are a framework that enable automatic execution of interop bundles from the source chain. The processing of interop transactions happens using our native [account abstraction](../../l2_system_contracts/account_abstraction.md) framework. 

The account should:

- validate the trigger in the `validateTransaction` step
- it should execute the feePaymentBundle by calling the [interop handler](./interop_handler.md) in the `payForTransaction` step,
- and finally it should execute the interop bundle in the `executeTransaction` step.

Any custom account can execute interop transactions using this method. However not all users will have their accounts deployed on all chains, for this use case a [Standard Trigger Account](./standard_trigger_account.md) is provided.

Security considerations:

- The executionBundle should be easy to execute because of retries. If the interop transaction fails, the execution bundle can be reexecuted.
- If the interop transaction fails, the feePaymentBundle will still be executed since it is in the `payForTransaction` step and is used to pay for gas costs.
- The feePaymentBundle should only be executed with the corresponding execution bundle, otherwise the gas funds might be stolen. We cannot link the feePayment and executionBundles directly, only through the execution address of the bundles and so the trigger. We can enforce the bundles are executed by the trigger account, and the tigger account enforces the link between the bundles. The trigger account enforces that the trigger's sender is the same as the feePaymentBundle's sender (this additional check is required, as a malicious trigger could point to the feePaymentBundle $^1$). The trigger account also makes sure that the trigger points to the bundles.

1: Note: it makes sense to have the trigger still point at the feePaymentBundle, since normally if a txs fails it will be due to the gas params in the trigger, and not due to the execution of the feePaymentBundle. So the trigger and the feePaymentBundle should be replaced together.

This design has the following considerations:

1. The account triggering the interop transaction has to exist before the interop transaction is received. In particular, it cannot always be one of the Interop Accounts, since these are deployed during the execution of the interop bundle.
1. The triggering account is the account where gas fees need to be paid. The feePaymentBundle should specify the trigger account's address on the source chain when bridging funds.
1. The gas remaining in the trigger account after the call should be sent to the refundRecipient (for the baseToken this will require small bootloader modifications). If a paymaster was used, and an alternative token was also bridged, it also has to be refunded.

