# Base Token Adjuster

This crate contains all the logic to handle ZK Chain with custom base tokens.

## Overview

### The Base Token Ratio Persister

Contains the building block for the `BaseTokenRatioPersisterLayer`.

- Connects with external APIs to get the current price of the base token and of ETH.
- Persists the ETH<->BaseToken ratio in the database.
- Upon certain configured threshold, update the L1 ETH<->BaseToken conversion ratio.

### The Base Token Ratio Provider

Contains the building block for the `BaseTokenRatioProviderLayer`.

- Periodically fetches from the DB and caches the latest ETH<->BaseToken conversion ratio.
- Exposes this ratio upon request.
