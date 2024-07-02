# Price API Client

This crate provides a simple trait to be implemented by clients interacting with external price APIs to fetch
ETH<->BaseToken ratio.

All clients should be implemented here and used by the node framework layer, which will be agnostic to the number of
clients available.
