# Base Token Adjuster

This crate contains all the logic to handle ZK Chain with custom base tokens. It is used by other node layers to adjust
the fees to be denominated in the chain's base token.

## Overview

The Base Token Adjuster:

- Connects with external APIs to get the current price of the base token and of ETH.
- Persists the ETH<->BaseToken ratio in the database.
- Upon certain configured threshold, update the L1 ETH<->BaseToken conversion ratio.

The Base Token Fetcher:

- Periodically fetches from the DB and caches the latest ETH<->BaseToken conversion ratio.
- Exposes this ratio upon request.
