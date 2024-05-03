# Note on virtual block timestamp fixed upgrade

After the release of the previous `1693318788-virtual-block-timestamp` upgrade, there was an issue discovered that
affected the first transaction right after the ending of the upgrade. With this upgrade, this issue has been fixed.

Note, that while on stage/testnet only `Bootloader` and `SystemContext` are updated (to patch the bug), on mainnet we
need to do the full upgrade (since the previous version had not been rolled out on mainnet yet).
