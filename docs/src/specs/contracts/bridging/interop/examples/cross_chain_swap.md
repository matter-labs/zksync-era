## Simple Example

Imagine you have contracts on chains B, C, and D, and you’d like them to send "reports" to the Headquarters (HQ)
contract on chain A every time a customer makes a purchase.

```solidity
// Deployed on chains B, C, D.
contract Shop {
 /// Called by the customers when they buy something.
 function buy(uint256 itemPrice) {
   // handle payment etc.
   ...
   // report to HQ
   InteropCenter(INTEROP_ADDRESS).sendCall(
    324,       // chain id of chain A,
    0xc425..,  // HQ contract on chain A,
    createCalldata("reportSales(uint256)", itemPrice), // calldata
    0,         // no value
  );
 }
}

// Deployed on chain A
contract HQ {
  // List of shops
  mapping (address => bool) shops;
  mapping (address => uint256) sales;
  function addShop(address addressOnChain, uint256 chainId) onlyOwner {
    // Adding aliased accounts.
   shops[address(keccak(addressOnChain || chainId))] = true;
  }

  function reportSales(uint256 itemPrice) {
    // only allow calls from our shops (their aliased accounts).
   require(shops[msg.sender]);
   sales[msg.sender] += itemPrice;
  }
}
```

### Cross Chain Swap Example

Imagine you want to perform a swap on chain B, exchanging USDC for PEPE, but all your assets are currently on chain A.

This process would typically involve four steps:

1. Transfer USDC from chain A to chain B.
2. Set allowance for the swap.
3. Execute the swap.
4. Transfer PEPE back to chain A.

Each of these steps is a separate "call," but you need them to execute in exactly this order and, ideally, atomically.
If the swap fails, you wouldn’t want the allowance to remain set on the destination chain.

Below is an example of how this process could look (note that the code is pseudocode; we’ll explain the helper methods
required to make it work in a later section).

```solidity
bundleId = InteropCenter(INTEROP_CENTER).startBundle(chainD);
// This will 'burn' the 1k USDC, create the special interopCall
// when this call is executed on chainD, it will mint 1k USDC there.
// BUT - this interopCall is tied to this bundle id.
USDCBridge.transferWithBundle(
  bundleId,
  chainD,
  aliasedAccount(this(account), block.chain_id),
  1000);


// This will create interopCall to set allowance.
InteropCenter.addToBundle(bundleId,
            USDCOnDestinationChain,
            createCalldata("approve", 1000, poolOnDestinationChain),
            0);
// This will create interopCall to do the swap.
InteropCenter.addToBundle(bundleId,
            poolOnDestinationChain,
            createCalldata("swap", "USDC_PEPE", 1000, ...),
            0)
// And this will be the interopcall to transfer all the assets back.
InteropCenter.addToBundle(bundleId,
            pepeBridgeOnDestinationChain,
            createCalldata("transferAll", block.chain_id, this(account)),
            0)


bundleHash = interopCenter.finishAndSendBundle(bundleId);
```

In the code above, we created a bundle that anyone can execute on the destination chain. This bundle will handle the
entire process: minting, approving, swapping, and transferring back.