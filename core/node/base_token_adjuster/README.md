# Base Token Adjuster

This crate contains an implementation of the base token adjuster

## Overview

The Base Token Adjuster has 3 roles:

1. Update the exchange-rate between the base token against ETH in DB
2. Update the exchange-rate between the base token against ETH in L1 for the L1->L2 transactions
3. Provide an interface for the server to poll the latest exchange-rate
