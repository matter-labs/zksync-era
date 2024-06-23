# Base Token Adjuster

This crate contains all the logic to handle ZK Chain with custom base tokens. It is used by other node layers to adjust
the fees to be denominated in the chain's base token.

## Overview

The Base Token Adjuster:

- Connect with external APIs to get the current price of the base token.
- Persist the price of the base token in the database.
- Upon request, adjust the fees to be denominated in the base token.
- Upon certain configured threshold, update the L1 ETH<->BaseToken conversion ratio.
