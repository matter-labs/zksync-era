# Account abstraction

One of the other important features of ZKsync is the support of account abstraction. It is highly recommended to read
the documentation on our AA protocol here:
[https://docs.zksync.io/build/developer-reference/account-abstraction](https://docs.zksync.io/build/developer-reference/account-abstraction)

### Account versioning

Each account can also specify which version of the account abstraction protocol do they support. This is needed to allow
breaking changes of the protocol in the future.

Currently, two versions are supported: `None` (i.e. it is a simple contract and it should never be used as `from` field
of a transaction), and `Version1`.

### Nonce ordering

Accounts can also signal to the operator which nonce ordering it should expect from these accounts: `Sequential` or
`Arbitrary`.

`Sequential` means that the nonces should be ordered in the same way as in EOAs. This means, that, for instance, the
operator will always wait for a transaction with nonce `X` before processing a transaction with nonce `X+1`.

`Arbitrary` means that the nonces can be ordered in arbitrary order. It is supported by the server right now, i.e. if
there is a contract with arbitrary nonce ordering, its transactions will likely either be rejected or get stuck in the
mempool due to nonce mismatch.

Note, that this is not enforced by system contracts in any way. Some sanity checks may be present, but the accounts are
allowed to do however they like. It is more of a suggestion to the operator on how to manage the mempool.

### Returned magic value

Now, both accounts and paymasters are required to return a certain magic value upon validation. This magic value will be
enforced to be correct on the mainnet, but will be ignored during fee estimation. Unlike Ethereum, the signature
verification + fee charging/nonce increment are not included as part of the intrinsic costs of the transaction. These
are paid as part of the execution and so they need to be estimated as part of the estimation for the transaction’s
costs.

Generally, the accounts are recommended to perform as many operations as during normal validation, but only return the
invalid magic in the end of the validation. This will allow to correctly (or at least as correctly as possible) estimate
the price for the validation of the account.
