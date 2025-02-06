# Test VM dumps

This directory contains VM dumps for regression testing. 

## Overview

- [`estimate_fee_for_transfer_to_self`](estimate_fee_for_transfer_to_self.json): Fee estimation step for a 0 base token transfer to self
  with a low gas limit. The legacy VM previously returned the incorrect revert reason.
