# Changelog

## [1.42.1](https://github.com/hetznercloud/terraform-provider-hcloud/compare/v1.42.0...v1.42.1) (2023-08-14)


### Bug Fixes

* **primary_ip:** list data source only returned first 25 IPs ([#729](https://github.com/hetznercloud/terraform-provider-hcloud/issues/729)) ([62e9781](https://github.com/hetznercloud/terraform-provider-hcloud/commit/62e97810df58d2eccaaed2e81d7833fff4e5d6ae))
* **server:** panic when passing empty string as ssh key ([#736](https://github.com/hetznercloud/terraform-provider-hcloud/issues/736)) ([d57b386](https://github.com/hetznercloud/terraform-provider-hcloud/commit/d57b38606c4b052b7d8181074d0860bd35935145))
* **server:** Return `nil` instead of  `"&lt;nil&gt;"` with IPv4/IPv6 disabled ([#723](https://github.com/hetznercloud/terraform-provider-hcloud/issues/723)) ([6cd2a37](https://github.com/hetznercloud/terraform-provider-hcloud/commit/6cd2a3753df03ebb6f3ebdb46899f2ff167d04ad))
* use exponential backoff when retrying actions ([#735](https://github.com/hetznercloud/terraform-provider-hcloud/issues/735)) ([d51ee4a](https://github.com/hetznercloud/terraform-provider-hcloud/commit/d51ee4a46dd869320b90413d8e7806fab21dc419))

## [1.42.0](https://github.com/hetznercloud/terraform-provider-hcloud/compare/v1.41.0...v1.42.0) (2023-07-13)


### Features

* **image:** add filter include_deprecated to datasources ([#685](https://github.com/hetznercloud/terraform-provider-hcloud/issues/685)) ([#717](https://github.com/hetznercloud/terraform-provider-hcloud/issues/717)) ([9f039ba](https://github.com/hetznercloud/terraform-provider-hcloud/commit/9f039ba35b9b0e94f4f5581099031e11f001a6d8))


### Bug Fixes

* **lb:** early validation for lb_target arguments ([#721](https://github.com/hetznercloud/terraform-provider-hcloud/issues/721)) ([10928d1](https://github.com/hetznercloud/terraform-provider-hcloud/commit/10928d1389f4f7e08f042c33101af03a4e78d155))
* **rdns:** crash when resource was deleted outside of terraform ([#720](https://github.com/hetznercloud/terraform-provider-hcloud/issues/720)) ([aad0614](https://github.com/hetznercloud/terraform-provider-hcloud/commit/aad0614d4abbe2dfbed53630b2e29380e6b087c5)), closes [#710](https://github.com/hetznercloud/terraform-provider-hcloud/issues/710)

## [1.41.0](https://github.com/hetznercloud/terraform-provider-hcloud/compare/v1.40.0...v1.41.0) (2023-06-22)


### Features

* **network:** add support for exposing routes to vswitch connection ([#703](https://github.com/hetznercloud/terraform-provider-hcloud/issues/703)) ([f213550](https://github.com/hetznercloud/terraform-provider-hcloud/commit/f2135509328ff2418ddc5f5224872ccb68821f6c))

## [1.40.0](https://github.com/hetznercloud/terraform-provider-hcloud/compare/v1.39.0...v1.40.0) (2023-06-13)


### Features

* deprecation info for server types ([#691](https://github.com/hetznercloud/terraform-provider-hcloud/issues/691)) ([9e6a22c](https://github.com/hetznercloud/terraform-provider-hcloud/commit/9e6a22cf2d5cc1e1859ec622c649978b83207938))


### Bug Fixes

* **server:** invalid ipv6_address nil1 when no IPv6 is used ([#689](https://github.com/hetznercloud/terraform-provider-hcloud/issues/689)) ([2912f45](https://github.com/hetznercloud/terraform-provider-hcloud/commit/2912f459bbf47b2d9f90325056713a4eb9d99d1d))

## v1.39.0

### What's Changed
* feat(rdns): support setting RDNS for Primary IPs by @apricote in https://github.com/hetznercloud/terraform-provider-hcloud/pull/669
* feat(server_type): return included traffic by @apricote in https://github.com/hetznercloud/terraform-provider-hcloud/pull/680


**Full Changelog**: https://github.com/hetznercloud/terraform-provider-hcloud/compare/v1.38.2...v1.39.0

## v1.38.2

### What's Changed
* ci: run e2etests in parallel by @apricote in https://github.com/hetznercloud/terraform-provider-hcloud/pull/660
* fix(server): avoid recreate when using official image by ID by @apricote in https://github.com/hetznercloud/terraform-provider-hcloud/pull/661


**Full Changelog**: https://github.com/hetznercloud/terraform-provider-hcloud/compare/v1.38.1...v1.38.2

## v1.38.1

Affordable, sustainable & powerful! rocketYou can now get one of our Arm64 CAX servers to optimize your operations while minimizing your costs!
Discover Ampere’s efficient and robust Arm64 architecture and be ready to get blown away with its performance. sunglasses

Learn more: https://www.hetzner.com/news/arm64-cloud

### What's Changed
* fix(server): crash when non-existent server type is used by @apricote in https://github.com/hetznercloud/terraform-provider-hcloud/pull/659
* fix(server): unable to create server from image id by @apricote in https://github.com/hetznercloud/terraform-provider-hcloud/pull/658
* fix(deps): update module golang.org/x/crypto to v0.8.0 by @renovate in https://github.com/hetznercloud/terraform-provider-hcloud/pull/652


**Full Changelog**: https://github.com/hetznercloud/terraform-provider-hcloud/compare/v1.38.0...v1.38.1

## v1.38.0

Affordable, sustainable & powerful! 🚀You can now get one of our Arm64 CAX servers to optimize your operations while minimizing your costs!
Discover Ampere’s efficient and robust Arm64 architecture and be ready to get blown away with its performance. 😎

Learn more: https://www.hetzner.com/news/arm64-cloud

### What's Changed
* fix(deps): update github.com/hashicorp/go-cty digest to 8598007 by @renovate in https://github.com/hetznercloud/terraform-provider-hcloud/pull/633
* fix(deps): update module golang.org/x/net to v0.9.0 by @renovate in https://github.com/hetznercloud/terraform-provider-hcloud/pull/651
* feat: add support for ARM APIs by @apricote in https://github.com/hetznercloud/terraform-provider-hcloud/pull/654


**Full Changelog**: https://github.com/hetznercloud/terraform-provider-hcloud/compare/v1.37.0...v1.38.0

## v1.37.0

### What's Changed
* docs: Add missing location (hil) by @akirak in https://github.com/hetznercloud/terraform-provider-hcloud/pull/606
* docs: replace outdated example OS image by @apricote in https://github.com/hetznercloud/terraform-provider-hcloud/pull/615
* docs: list available datacenters by @apricote in https://github.com/hetznercloud/terraform-provider-hcloud/pull/613
* docs: explain deprecated attributes by @apricote in https://github.com/hetznercloud/terraform-provider-hcloud/pull/614
* feat(primaryip): return IPv6 subnet #600 by @apricote in https://github.com/hetznercloud/terraform-provider-hcloud/pull/620
* fix: state is missing resources when rate limit is reached #604 by @apricote in https://github.com/hetznercloud/terraform-provider-hcloud/pull/621
* chore(ci): run e2e on public workers by @samcday in https://github.com/hetznercloud/terraform-provider-hcloud/pull/631
* Configure Renovate by @renovate in https://github.com/hetznercloud/terraform-provider-hcloud/pull/629
* chore: Test against Terraform v1.4 by @apricote in https://github.com/hetznercloud/terraform-provider-hcloud/pull/638
* fix(deps): update module github.com/hashicorp/terraform-plugin-log to v0.8.0 by @renovate in https://github.com/hetznercloud/terraform-provider-hcloud/pull/639
* fix(deps): update module github.com/hetznercloud/hcloud-go to v1.41.0 by @renovate in https://github.com/hetznercloud/terraform-provider-hcloud/pull/637
* fix(deps): update module github.com/hashicorp/terraform-plugin-sdk/v2 to v2.25.0 by @renovate in https://github.com/hetznercloud/terraform-provider-hcloud/pull/640
* chore(deps): update goreleaser/goreleaser-action action to v4 by @renovate in https://github.com/hetznercloud/terraform-provider-hcloud/pull/642
* fix: self-reported version not correct by @apricote in https://github.com/hetznercloud/terraform-provider-hcloud/pull/630
* chore(deps): update actions/setup-go action to v4 by @renovate in https://github.com/hetznercloud/terraform-provider-hcloud/pull/643
* fix(deps): update module golang.org/x/crypto to v0.7.0 by @renovate in https://github.com/hetznercloud/terraform-provider-hcloud/pull/641
* docs: freebsd64 is no longer available as rescue image by @apricote in https://github.com/hetznercloud/terraform-provider-hcloud/pull/645
* refactor: Make CI Happy Again by @apricote in https://github.com/hetznercloud/terraform-provider-hcloud/pull/646
* fix(deps): update module github.com/hashicorp/terraform-plugin-sdk/v2 to v2.26.1 by @renovate in https://github.com/hetznercloud/terraform-provider-hcloud/pull/644

### New Contributors
* @akirak made their first contribution in https://github.com/hetznercloud/terraform-provider-hcloud/pull/606
* @samcday made their first contribution in https://github.com/hetznercloud/terraform-provider-hcloud/pull/631

**Full Changelog**: https://github.com/hetznercloud/terraform-provider-hcloud/compare/v1.36.2...v1.37.0

## v1.36.2

### What's Changed
* test: fix acceptence tests for new location Hillsboro by @apricote in https://github.com/hetznercloud/terraform-provider-hcloud/pull/598
* fix(server): unhandled errors from API calls by @apricote in https://github.com/hetznercloud/terraform-provider-hcloud/pull/602
* fix(lb): lb_target breaks when load-balancer is deleted in API by @apricote in https://github.com/hetznercloud/terraform-provider-hcloud/pull/603
* fix(lb): add missing fields to data source by @apricote in https://github.com/hetznercloud/terraform-provider-hcloud/pull/605


**Full Changelog**: https://github.com/hetznercloud/terraform-provider-hcloud/compare/v1.36.1...v1.36.2

## v1.36.1

### What's Changed
* chore: update hcloud-go to v1.37.0 by @apricote in https://github.com/hetznercloud/terraform-provider-hcloud/pull/591
* fix(server): make sure that each network block is unique by @apricote in https://github.com/hetznercloud/terraform-provider-hcloud/pull/594
* docs: mention that we only accept the location name as attribute by @apricote in https://github.com/hetznercloud/terraform-provider-hcloud/pull/595
* fix(server): unnecessary updates when using network #556 by @apricote in https://github.com/hetznercloud/terraform-provider-hcloud/pull/593
* fix: multiple resources break when parent resource is recreated by @apricote in https://github.com/hetznercloud/terraform-provider-hcloud/pull/596


**Full Changelog**: https://github.com/hetznercloud/terraform-provider-hcloud/compare/v1.36.0...v1.36.1

## v1.36.0

### What's Changed
* Update auto delete on primary IP resource change by @4ND3R50N in https://github.com/hetznercloud/terraform-provider-hcloud/pull/573
* Update Dependencies by @LKaemmerling in https://github.com/hetznercloud/terraform-provider-hcloud/pull/575
* Add tests for Terraform 1.3 by @LKaemmerling in https://github.com/hetznercloud/terraform-provider-hcloud/pull/576
* docs: explain how to create a server from snapshot by @apricote in https://github.com/hetznercloud/terraform-provider-hcloud/pull/582
* docs: clarify arguments of hcloud_primary_ip resource by @apricote in https://github.com/hetznercloud/terraform-provider-hcloud/pull/584
* fix: error when deleting hcloud_primary_ip with auto_delete by @apricote in https://github.com/hetznercloud/terraform-provider-hcloud/pull/585
* test: fix flaky test TestServerResource_PrimaryIPNetworkTests by @apricote in https://github.com/hetznercloud/terraform-provider-hcloud/pull/587
* feat: import hcloud_load_balancer_target by @apricote in https://github.com/hetznercloud/terraform-provider-hcloud/pull/589
* fix: race-condition when re-creating server with external primary ip by @apricote in https://github.com/hetznercloud/terraform-provider-hcloud/pull/590

### New Contributors
* @apricote made their first contribution in https://github.com/hetznercloud/terraform-provider-hcloud/pull/582

**Full Changelog**: https://github.com/hetznercloud/terraform-provider-hcloud/compare/v1.35.2...v1.36.0

## v1.35.2

### What's Changed
* bug: add missing datacenter option at primary_ip & fix file naming by @komandar in https://github.com/hetznercloud/terraform-provider-hcloud/pull/559
* Fix private only server (attached to network) creation by @4ND3R50N in https://github.com/hetznercloud/terraform-provider-hcloud/pull/562
* feature: update workflow & golang to newest stable release 1.19 by @komandar in https://github.com/hetznercloud/terraform-provider-hcloud/pull/560
* ci: fix not available gpg_private_key in workflow by @komandar in https://github.com/hetznercloud/terraform-provider-hcloud/pull/563
* Remove < and > signs in import examples by @ekeih in https://github.com/hetznercloud/terraform-provider-hcloud/pull/564
* fix: wrong required statement by @komandar in https://github.com/hetznercloud/terraform-provider-hcloud/pull/567
* style: unify the bool and boolean type in the docs by @komandar in https://github.com/hetznercloud/terraform-provider-hcloud/pull/568

### New Contributors
* @komandar made their first contribution in https://github.com/hetznercloud/terraform-provider-hcloud/pull/559
* @ekeih made their first contribution in https://github.com/hetznercloud/terraform-provider-hcloud/pull/564

**Full Changelog**: https://github.com/hetznercloud/terraform-provider-hcloud/compare/v1.35.1...v1.35.2

## v1.35.1

### What's Changed
* Add workaround "fix" for network interface issue by @4ND3R50N in https://github.com/hetznercloud/terraform-provider-hcloud/pull/552
* Update hcloud-go to v1.35.2 by @4ND3R50N in https://github.com/hetznercloud/terraform-provider-hcloud/pull/554
* Prevent segfault when image nonexistent by @acuteaura in https://github.com/hetznercloud/terraform-provider-hcloud/pull/553

### New Contributors
* @acuteaura made their first contribution in https://github.com/hetznercloud/terraform-provider-hcloud/pull/553

**Full Changelog**: https://github.com/hetznercloud/terraform-provider-hcloud/compare/v1.35.0...v1.35.1

## v1.35.0

### What's Changed
* Implement Server Create Without primary ip on update behavior by @4ND3R50N in https://github.com/hetznercloud/terraform-provider-hcloud/pull/548
* Add support of using deprecated images by @LKaemmerling in https://github.com/hetznercloud/terraform-provider-hcloud/pull/549


**Full Changelog**: https://github.com/hetznercloud/terraform-provider-hcloud/compare/v1.34.3...v1.35.0

## v1.34.3

### What's Changed
* Create server without primary ips: Fix edge case bug + add test by @4ND3R50N in https://github.com/hetznercloud/terraform-provider-hcloud/pull/546


**Full Changelog**: https://github.com/hetznercloud/terraform-provider-hcloud/compare/v1.34.2...v1.34.3

## v1.34.2

### What's Changed
* Server Create without primary IPs via public_net by @4ND3R50N in https://github.com/hetznercloud/terraform-provider-hcloud/pull/544


**Full Changelog**: https://github.com/hetznercloud/terraform-provider-hcloud/compare/v1.34.1...v1.34.2

## v1.34.1

### What's Changed
* Add primary ip documentation by @4ND3R50N in https://github.com/hetznercloud/terraform-provider-hcloud/pull/540


**Full Changelog**: https://github.com/hetznercloud/terraform-provider-hcloud/compare/v1.34.0...v1.34.1

## v1.34.0

### What's Changed
* Update Dependencies (TF SDK 2.7.1 -> 2.14) by @LKaemmerling in https://github.com/hetznercloud/terraform-provider-hcloud/pull/524
* DataSource Network `id` should be an integer by @guineveresaenger in https://github.com/hetznercloud/terraform-provider-hcloud/pull/525
* Improve documentation by @02bensch in https://github.com/hetznercloud/terraform-provider-hcloud/pull/536
* Add support for primary IPs by @4ND3R50N in https://github.com/hetznercloud/terraform-provider-hcloud/pull/538

### New Contributors
* @guineveresaenger made their first contribution in https://github.com/hetznercloud/terraform-provider-hcloud/pull/525
* @02bensch made their first contribution in https://github.com/hetznercloud/terraform-provider-hcloud/pull/536

**Full Changelog**: https://github.com/hetznercloud/terraform-provider-hcloud/compare/v1.33.2...v1.34.0

## v1.33.2

### What's Changed
* Implement validation on labels as per spec by @4ND3R50N in https://github.com/hetznercloud/terraform-provider-hcloud/pull/522
* Add resourceFloatingIPAssignmentUpdate by @CyberShadow in https://github.com/hetznercloud/terraform-provider-hcloud/pull/501
* Use Go 1.18 for building by @LKaemmerling in https://github.com/hetznercloud/terraform-provider-hcloud/pull/523

### New Contributors
* @4ND3R50N made their first contribution in https://github.com/hetznercloud/terraform-provider-hcloud/pull/522
* @CyberShadow made their first contribution in https://github.com/hetznercloud/terraform-provider-hcloud/pull/501

**Full Changelog**: https://github.com/hetznercloud/terraform-provider-hcloud/compare/v1.33.1...v1.33.2

## v1.33.1

### What's Changed
* Datasource hcloud_location & hcloud_locations: Add network_zone by @LKaemmerling in https://github.com/hetznercloud/terraform-provider-hcloud/pull/508
* hcloud_servcer resource: Retry on enabling rescue (reset call) by @LKaemmerling in https://github.com/hetznercloud/terraform-provider-hcloud/pull/511


**Full Changelog**: https://github.com/hetznercloud/terraform-provider-hcloud/compare/v1.33.0...v1.33.1

## v1.33.0

### What's Changed
* Update image.html.md by @FloMaetschke in https://github.com/hetznercloud/terraform-provider-hcloud/pull/494
* docs: Add missing location (ash) by @dhoppe in https://github.com/hetznercloud/terraform-provider-hcloud/pull/496
* Add missing argument for resource hcloud_ssh_key by @dhoppe in https://github.com/hetznercloud/terraform-provider-hcloud/pull/498
* Make the image property of hcloud_server optional by @fhofherr in https://github.com/hetznercloud/terraform-provider-hcloud/pull/499
* Implement hcloud_firewall_attachment resource by @fhofherr in https://github.com/hetznercloud/terraform-provider-hcloud/pull/500

### New Contributors
* @FloMaetschke made their first contribution in https://github.com/hetznercloud/terraform-provider-hcloud/pull/494
* @dhoppe made their first contribution in https://github.com/hetznercloud/terraform-provider-hcloud/pull/496

**Full Changelog**: https://github.com/hetznercloud/terraform-provider-hcloud/compare/v1.32.2...v1.33.0

## v1.32.2

### What's Changed
* server: resource: fix spelling by @xdevs23 in https://github.com/hetznercloud/terraform-provider-hcloud/pull/480
* Use our own E2E Test runner by @LKaemmerling in https://github.com/hetznercloud/terraform-provider-hcloud/pull/481
* Mark the hcloud_token sensitive by @fhofherr in https://github.com/hetznercloud/terraform-provider-hcloud/pull/479
* Fix nil check for RNDSupporter by @fhofherr in https://github.com/hetznercloud/terraform-provider-hcloud/pull/485
* fix: typo in docs by @RobertHeim in https://github.com/hetznercloud/terraform-provider-hcloud/pull/486
* Adjust tests by @fhofherr in https://github.com/hetznercloud/terraform-provider-hcloud/pull/489
* fix: typo by @RobertHeim in https://github.com/hetznercloud/terraform-provider-hcloud/pull/488

### New Contributors
* @xdevs23 made their first contribution in https://github.com/hetznercloud/terraform-provider-hcloud/pull/480
* @RobertHeim made their first contribution in https://github.com/hetznercloud/terraform-provider-hcloud/pull/486

**Full Changelog**: https://github.com/hetznercloud/terraform-provider-hcloud/compare/v1.32.1...v1.32.2

## v1.32.1



- ec487e93 Fix failing tests
- d10e9f0d Fix firewall deletion

## v1.32.0



- af8300cf Case-insensitive comparison of IPv6 addresses
- 4adfcfa9 Update hcloud-go to v1.33.0
- 9e08f76f hcloud_firewall resource: Remove all resources before deleting the firewall

## v1.31.1



- 545375f9 hcloud_server resource: Add computed to firewall_ids

## v1.31.0

### What's Changed

- 3a6384cc Add delete and rebuild protection (#432)
- 6eaecafa Add list data source for certificates
- 07b1b53e Add list data source for firewall (#445)
- 45843b99 Add list data source for floating ips (#448)
- 9914e064 Add list data source for images
- 21267191 Add list data source for load balancers
- eb3bcc0c Add list data source for networks (#452)
- a0045237 Add list data source for placement groups (#456)
- 333e38ca Add list data source for server (#434)
- 3a33c154 Add list data source for volumes
- 8de96d9d Allow updating of hcloud_load_balancer_network resource
- 14c10006 Deprecate hcloud_load_balancer resource `target` property
- 5a7feae8 Do not fail fast on e2e tests
- a3023bfc Fix datasources of server and volume (#431)
- b02858af Fix firewall apply_to if assigned with server resource (#455)
- b595efa6 Fix race condition on server data source test (#446)
- ff44ac8e Improve handling of "Error: cannot remove subnet because servers are attached to it"
- 86f5c990 Refactor list data source for datacenter (#444)
- 984c2e3b Refactor list data source for locations
- 32000fcb Refactor list data source for server types
- 2c8ad4f4 Refactor list data source for ssh keys (#438)
- b5f5073e Remove testing & official support of Terraform < 1.0
- 7e4746f1 hcloud_firewall resource & datasource: Add apply_to possibility
- 54394aea hcloud_rdns: Fix nil pointer when resource does not exist

## v1.30.0

### What's Changed

- 985c1db9 Add dns ptr support for load balancer
- 13421a6a Update docs
- ce6982e0 Use go 1.17 for tests & builds

## v1.29.0

### What's Changed

- f0e2e3c1 Fix date format for certificate states
- e19f2d76 Placement groups (#426)
- 151a0b6a Update hcloud_firewall documentation on how to define port range

## v1.28.1

### What's Changed

- 93059571 Add missing firewall rule description docs
- 9abc5d7e Fix firewall rule description

## v1.28.0

### What's Changed

- 92a07cd0 Add description field to firewall rules
- a0a90b8f Increase amount of retries on firewall deletion

## v1.27.2

### What's Changed

- f397c38d Add a feature request template
- b4619110 Fix hcloud_snapshot resource documentation
- 8d204641 Fix spelling and grammar mistakes
- 219a6355 Update hcloud-go to v1.28.0

## v1.27.1

### What's Changed

- 71f995bf Add issue template
- ad88a85f Add missing docs about the network attribute/argument on hcloud_server and implement the datatransformation of the network for the attribute
- 72a5fb48 Add testcase
- 449710e9 Add tests for terraform 1.0
- 0cfa7c88 Docs: Add missing firewall_ids to hcloud_server resource and datasource
- 80ee6fab Docs: Improve documentation for hcloud_firewall resource to include information about port ranges and the `any` keyword (#381)
- 8f1c5c16 Docs: Mark example "hcloud_token" variable as sensitive
- b941c699 Fix non iso8601 timestamp in hcloud_image datasource
- 993b3cd2 Fix tests
- e775c362 Generate Changelog with goreleaser
- 1795d377 Improve error messages from hcloud-go
- 895813eb Improve hcloud_rdns resource documentation and validation of fields `server_id` and `floating_ip_id` that should be mutually exclusive
- 3cdd11b4 Increase create timeout for servers and snapshots
- 68a3d6a6 Move logic around to make it more readable
- faa15532 Network Attachments: Retry on ServiceError and NoSubnetAvailable Error
- edbddcfe Update dependencies
- 4027dd6f Update terraform SDK to v2.7.0


## 1.27.0 (June 17, 2021)

FEATURES:
* `hcloud_firewall` resource & datasource: Support GRE & ESP protocol in firewall rules

## 1.26.2 (May 28, 2021)

BUG FIXES:

* Fix invalid checksum for release 1.26.1

## 1.26.1 (May 28, 2021)

BUG FIXES:
* `hcloud_firewall` datasource: `destination_ips` missed in definition
* `hcloud_certificate` resource: panic when parsing certificate chains
  (#359)

## 1.26.0 (March 30, 2021)

* **New Resource** `hcloud_managed_certificate`
* **New Resource** `hcloud_uploaded_certificate`
* **Deprecated Resource** `hcloud_certificate`

## 1.25.2 (March 16, 2021)

BUG FIXES:
* `hcloud_firewall` resource: plugin normalized CIDRs silently.

## 1.25.1 (March 10, 2021)

BUG FIXES:
* `hcloud_firewall` documentation: fix name of `firewall_ids` property.

## 1.25.0 (March 10, 2021)

FEATURES:
* **New Resource**: `hcloud_snapshot`
* **New Resource**: `hcloud_firewall`
* **New Data Source**: `hcloud_firewall`

BUG FIXES:
* `hcloud_server` resource: image had a wrong type (int instead of string) when a server was created from a snapshot
* `hcloud_load_balancer_target` resource: force recreation when changing a target attribute (server_id, ip or label_selector)

NOTES:
* The provider is now built with Go 1.16

## 1.24.1 (February 04, 2021)

BUG FIXES:
* `hcloud_volume` datasource: id is now marked as computed to allow more setups where the id is unknown
* `hcloud_ssh_key` datasource: id is now marked as computed to allow more setups where the id is unknown
* `hcloud_network` datasource: id is now marked as computed to allow more setups where the id is unknown
* `hcloud_image` datasource: id is now marked as computed to allow more setups where the id is unknown
* `hcloud_certificate` datasource: id is now marked as computed to allow more setups where the id is unknown
* `hcloud_volume` resource: Automount is now working when you attach an already existing volume to a server.

## 1.24.0 (January 12, 2021)

FEATURES:
* **New Datasource**: `hcloud_server_type`
* **New Datasource**: `hcloud_server_types`
* New `network` property for `hcloud_server` resource.

BUG FIXES:
* `hcloud_volume` resource: A race condition was fixed, that was called when you tried to create multiple volumes for a single server
* `hcloud_locations` datasource: Use a stable value as IDs instead of a timestamp. We now use a hash of the concatenation of all location IDs as ID
* `hcloud_datacenters` datasource: Use a stable value as IDs instead of a timestamp. We now use a hash of the concatenation of all datacenters IDs as ID

Notes:
* This release is tested against Terraform 0.13.x and 0.14.x. Testing on 0.12.x was removed, therefore Terraform 0.12.x is no longer officially supported

## 1.23.0 (November 03, 2020)

FEATURES:
* `hcloud_network_subnet` supports vSwitch Subnets

Notes:
* The provider was updated to use the Terraform Plugin SDK v2.

## 1.22.0 (October 05, 2020)

FEATURES:

* All `hcloud_*` resources are now importable.

BUG FIXES:
* `hcloud_rdns` resource: It is now possible to import the resource as documented.

## 1.21.0 (September 09, 2020)

CHANGED:

* Un-deprecate `network_id` property of `hcloud_load_balancer_network` and
  `hcloud_server_network` resources.
* Change module path from
  `github.com/terraform-providers/terraform-provider-hcloud` to
  `github.com/hetznercloud/terraform-provider-hcloud`

## 1.20.1 (August 18, 2020)
BUG FIXES:

* `hcloud_certificate` resource: Updating the certificate needs to recreate the certificate.

NOTES:
* The provider is now build with Go 1.15
* We overhauled parts of the underlying test suite

## 1.20.0 (August 10, 2020)

FEATURES:

* Allow updating/resizing a Load Balancer through the
  `load_balancer_type` of `hcloud_load_balancer` resource
* Add support for Load Balancer Label Selector and IP targets.

## 1.19.2 (July 28, 2020)

CHANGED:

* Deprecate `network_id` property of `hcloud_server_network` and
  `hcloud_load_balancer_network` resources. Introduce a `subnet_id`
  property as replacement.

  Both resources require a subnet to be created. Since `network_id`
  references the network and not the subnet there is no explicit
  dependency between those resources. This leads to Terraform creating
  those resources in parallel, which creates a race condition. Users
  stuck with the `network_id` property can create an explicit dependency
  on the subnet using `depends_on` to work around this issue.

BUG FIXES:
* Enable and Disable `proxyprotocol` on a Load Balancer didn't work after creation
* Deleted all Load Balancer services when you changed the `listen_port` of one service
* `hcloud_load_balancer_target` was not idempotent when you add a target that was already defined

NOTES:
* Update to hcloud-go v1.19.0 to fix the bad request issue

## 1.19.1 (July 16, 2020)

NOTES:

* First release under new terraform registry
* Provider was moved to https://github.com/hetznercloud/terraform-provider-hcloud

## 1.19.0 (July 10, 2020)

BUG FIXES:

* Update to hcloud-go v1.18.2 to fix a conflict issue
* Ensure `alias_ip` retain the same order.

NOTES:

* This release uses Terraform Plugin SDK v1.15.0.

## 1.18.1 (July 02, 2020)

BUG FIXES

* Set correct defaults for `cookie_name` and `cookie_lifetime`
  properties of `hcloud_load_balancer_service`.
* Remove unsupported `https` protocol from health check documentation.
* Force recreate of `hcloud_network` if `ip_range` changes.

## 1.18.0 (June 30, 2020)

FEATURES:

* **New Resource**: `hcloud_load_balancer_target` which allows to add a
  target to a load balancer. This resource extends the `target` property
  of the `hcloud_load_balancer` resource.  `hcloud_load_balancer_target`
  should be preferred over the `target` property of
  `hcloud_load_balancer`.

## 1.17.0 (June 22, 2020)

FEATURES:

* **New Datasource**: `hcloud_load_balancer`
* **New Resource**: `hcloud_load_balancer`
* **New Resource**: `hcloud_load_balancer_service`
* **New Resource**: `hcloud_load_balancer_network`

BUG FIXES:

* resources/hcloud_network_route: Fix panic when trying to lookup an already deleted Network route

## 1.16.0 (March 24, 2020)

BUG FIXES:
* resource/hcloud_ssh_key: Fix panic when we update labels in SSH keys
* resource/hcloud_server_network: Fix alias ips ignored on creation of server network
* resource/hcloud_server: Use first assigned `ipv6_address` as value instead of the network address. **Attention: This can be a breaking change**

NOTES:
* This release uses Terraform Plugin SDK v1.8.0.

## 1.15.0 (November 11, 2019)

IMPROVEMENTS:

* resources/hcloud_server: Add retry mechanism for enabling the rescue mode.

NOTES:
* This release uses Terraform Plugin SDK v1.3.0.

## 1.14.0 (October 01, 2019)

NOTES:
* This release uses the Terraform Plugin SDK v1.1.0.

## 1.13.0 (September 19, 2019)

IMPROVEMENTS:

* resources/hcloud_floating_ip: Add `name` attribute to get or set the name of a Floating IP.
* datasource/hcloud_floating_ip: Add `name` attribute to get Floating IPs by their name.

NOTES:

* This release is Terraform 0.12.9+ compatible.
* Updated hcloud-go to `v1.16.0`
* The provider is now tested and build with  Go `1.13`

## 1.12.0 (July 29, 2019)

FEATURES:

* **New Datasource**: `hcloud_ssh_keys` Lookup all SSH keys.

IMPROVEMENTS:

* resources/hcloud_server_network: Add `mac_address` attribute to get the mac address of the Network interface.

BUG FIXES:

* Fix an error on server creation, when an iso id was given instead of an iso name.

NOTES:

* This release is Terraform 0.12.5+ compatible.
* Updated hcloud-go to `v1.15.1`
* Added hcloud-go request debugging when using `TF_LOG`.

## 1.11.0 (July 10, 2019)

FEATURES:

* **New Resource**: `hcloud_network` Manage Networks.
* **New Resource**: `hcloud_network_subnet` Manage Networks Subnets.
* **New Resource**: `hcloud_network_route` Manage Networks Routes.
* **New Resource**: `hcloud_server_network` Manage attachment between servers and Networks.
* **New Datasource**: `hcloud_network` Lookup Networks.

## 1.10.0 (May 14, 2019)

NOTES:
* This release is Terraform 0.12-RC1+ compatible.

## 1.9.0 (March 15, 2019)

IMPROVEMENTS:

* datasource/hcloud_server: Add `with_status` attribute to get images by their status.
* datasource/hcloud_image: Add `with_status` attribute to get servers by their status.
* datasource/hcloud_volume: Add `with_status` attribute to get volumes by their status.

* Added `with_selector` to all datasources that support label selectors.

NOTES:

* **Deprecation**: datasource/hcloud_server: `selector`, will be removed in the near future.
* **Deprecation**: datasource/hcloud_floating_ip: `selector`, will be removed in the near future.
* **Deprecation**: datasource/hcloud_image: `selector`, will be removed in the near future.
* **Deprecation**: datasource/hcloud_ssh_key: `selector`, will be removed in the near future.
* **Deprecation**: datasource/hcloud_volume: `selector`, will be removed in the near future.

## 1.8.1 (March 12, 2019)

BUG FIXES:
* Fix an error on server creation, when a image id was given instead of a image name.
* Fix an missing error on `terraform plan`, when using an image name which does not exists.

## 1.8.0 (February 06, 2019)

FEATURES:
* **New Datasource**: `hcloud_server` Lookup a server.

IMPROVEMENTS:
* Add API token length validation

## 1.7.0 (December 18, 2018)

FEATURES:
* **New Datasource**: `hcloud_location` Lookup a location.
* **New Datasource**: `hcloud_locations` Lookup all locations.
* **New Datasource**: `hcloud_datacenter` Lookup a datacenter.
* **New Datasource**: `hcloud_datacenters` Lookup all datacenters.
* Volume Automounting is now available for `hcloud_volume` and `hcloud_volume_attachment`

## 1.6.0 (December 03, 2018)

IMPROVEMENTS:
* datasource/hcloud_image: Add `most_recent` attribute to get the latest image when multiple images has the same label.

BUG FIXES:
* Fix an error on volume_attachment creation, when server was locked.

## 1.5.0 (November 16, 2018)

FEATURES:
* **New Resource**: `hcloud_volume_attachment` Manage the attachment between volumes and servers.

IMPROVEMENTS:
* resources/hcloud_server: Add `backups` attribute to enable or disable backups.

NOTES:
* **Read Only**: resources/hcloud_server: `backup_window`, removed the ability to set the attribute. This attribute is now read only.
* Updated hcloud-go to `v1.11.0`

## 1.4.0 (October 18, 2018)

FEATURES:

* **New Resource**: `hcloud_volume` Manage volumes.
* **New Datasource**: `hcloud_volume` Lookup volumes.

NOTES:

* **Deprecation**: resource/hcloud_server: `backup_window`, will be removed in the near future.

## 1.3.0 (September 12, 2018)

FEATURES:

- **New Resource**: `hcloud_rnds` Manage reverse DNS entries for servers and Floating IPs.
* **New Resource**: `hcloud_floating_ip_assignment` Manage the association between Floating IPs and servers.
- **New Datasource**: `hcloud_floating_ip` Lookup Floating ips.
- **New Datasource**: `hcloud_image` Lookup images.
- **New Datasource**: `hcloud_ssh_key` Lookup SSH Keys.
- **New Provider Config**: `poll_interval`  Configures the interval in which actions are polled by the client. Default `500ms`. Increase this interval if you run into rate limiting errors.

IMPROVEMENTS:

* resource/hcloud_server: Add `ipv6_network` attribute.

NOTES:

* Updated hcloud-go to `v1.9.0`

## 1.2.0 (June 07, 2018)

NOTES:

* Switched from MIT licence to MPL2
* removed `reverse_dns` property of `hcloud_floating_ip`, because it was not useable, see https://github.com/hetznercloud/terraform-provider-hcloud/issues/32
* improved test coverage
* updated terraform to `v0.11.7`
* updated hcloud-go to `v1.6.0`
* added log when waiting for an action to complete

BUG FIXES:

* delete records from state that are invalid or are not found by the server
* resource update methods return the result of the read method

## 1.1.0 (March 2, 2018)

* Save hashsum of `user_data`, existing state is migrated
* update hcloud-go to v1.4.0
* update terraform from v0.11.2 to v0.11.3

## 1.0.0 (January 30, 2018)

* Initial release
