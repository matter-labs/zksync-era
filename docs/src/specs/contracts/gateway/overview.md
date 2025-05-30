# Gateway

Gateway is a proof aggregation layer, created to solve the following problems:

- Fast interop would require quick proof generation and verification. The latter can be very expensive on L1. Gateway provides an L1-like interface for chains, while giving a stable price for compute.
- Generally proof aggregation can reduce costs for users, if there are multiple chains settling on top of the same layer. It can reduce the costs of running a Validium even further.

In this release, Gateway is basically a fork of Era, that will be deployed within the same CTM as other ZK Chains. This allows us to reuse most of the existing code for Gateway.

> In some places in code you can meet words such as “settlement layer” or the abbreviation “sl”. “Settlement layer” is a general term that describes a chain that other chains can settle to. Right now, the list of settlement layers is whitelisted and only Gateway will be allowed to be a settlement layer (along with L1).

## Read more

- [Chain migration](chain_migration.md)
- [L1->L2 messaging via gateway](messaging_via_gateway.md)
- [L2->L1 messaging via gateway](nested_l3_l1_messaging.md)
- [Gateway protocol versioning](gateway_protocol_upgrades.md)
- [DA handling on Gateway](gateway_da.md)

## High level gateway architecture

![image.png](./img/ecosystem_architecture.png)
