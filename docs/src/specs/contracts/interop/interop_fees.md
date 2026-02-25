# Interop fees

When a chain settles on top of ZK Gateway, it will have to pay `GWAssetTracker.gatewaySettlementFee` ZK tokens for each [interop call](./interop_center/interop_center.md) used. The amount is set by the decentralized governance and the funds can be also withdrawn only by it.

The above ensures that every chain pays for the protection offered by the GWAssetTracker. Obviously the price for the interop calls should be paid by the users too. 

For better UX, we provide the ability to pay the funds in the base token of the chain. The operator has the ability to provide an arbitrary `InteropCenter.interopProtocolFee`, though usually it should be the value that can cover the `GWAssetTracker.gatewaySettlementFee` according to the current exchange rate. We allow arbitrary values (including zero) to ensure that free interop could also be supported for chains that wish to subsidize the cost.

While [stage1](../chain_management/stage1.md) is not yet supported for chains that settle on ZK Gateway (and thus for interop transactions too), for future compatibility we introduced the ability to pay a fixed fee in ZK for the interop (`useFixedFee = true`). This is a fixed amount that is defined to be noticeably higher than the typical value of `GWAssetTracker.gatewaySettlementFee` as it should be a last resort field to use and should also leave some room for governance to change `GWAssetTracker.gatewaySettlementFee` without necessitating a protocol upgrade.
