# InteropHandler

The interop handler processes interop bundles and interop transactions. The processing of interop transactions happens using our native account abstraction framework. The trigger is verified by the Default Account, while the bundles and the contained calls are executed by the interop handler. 

## Account Aliasing

The interop handler can execute calls either via the the [ERC-7786](https://github.com/ethereum/ERCs/pull/673/files) or via Account Aliasing, i.e. [implicit](https://github.com/ethereum/L2-interop/pull/26/files) calls. The benefit of implicit calls is that the called contract does not have to support interop. To make the implicit call secure, the call is executed by an intermediate Aliased Contract, which can only be triggered by interop transactions. 

## Standard Trigger Account
 
For interop transactions using interop triggers, the source chain sends the bundles and the triggers. There are security considerations to be made:

1. The account triggering (validating and executing) the transaction has to exist before the interop transaction is received. In particular, it cannot always be one of the aliased accounts, since these might not exist before the interop transaction is received. So we have to provide a predeployed contract that can do the basic features, this is the StandardTriggerAccount. However, alternative triggering accounts can also be used.
1. The triggering account is the account where gas fees need to be paid. The feePaymentBundle should specify the trigger accounts address on the source chain.
1. The executionBundle should be easy to execute because of retries. The feePaymentBundle should only be executed with the corresponding execution bundle, otherwise the gas funds might be stolen. We cannot link the feePayment and executionBundles directly, only through the trigger and trigger account. We can enforce the bundles are executed by the trigger account, and the tigger account enforces the link between the bundles by checking that the trigger specifies the bundles, and that the trigger's sender is the same as the feePaymentBundle's sender ( this additional check is required, as a malicious trigger could point to the feePaymentBundle). Note: it makes sense to have the trigger still point at the feePaymentBundle, since normally if a txs fails it will be due to the gas params in the trigger, and not due to the execution of the feePaymentBundle. So the trigger and the feePaymentBundle should be replaced together.
1. The gas remaining in the trigger account after the call should be sent to the refundRecipient. If a paymaster was used, and an alternative token was also bridged, it also has to be refunded.

