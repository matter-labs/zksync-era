Terraform Provider for the Hetzner Cloud
==================
[![GitHub release](https://img.shields.io/github/tag/hetznercloud/terraform-provider-hcloud.svg?label=release)](https://github.com/hetznercloud/terraform-provider-hcloud/releases/latest) [![Actions Status](https://github.com/hetznercloud/terraform-provider-hcloud/workflows/test/badge.svg)](https://github.com/hetznercloud/terraform-provider-hcloud/actions)[![Actions Status](https://github.com/hetznercloud/terraform-provider-hcloud/workflows/release/badge.svg)](https://github.com/hetznercloud/terraform-provider-hcloud/actions)

- Website: https://www.terraform.io
- Documentation: https://www.terraform.io/docs/providers/hcloud/index.html

Requirements
------------

-	[Terraform](https://www.terraform.io/downloads.html) 1.2.x
-	[Go](https://golang.org/doc/install) 1.19.x (to build the provider plugin)

API Stability
-------------

This Go module implements a Terraform Provider for Hetzner Cloud
Services. We thus guarantee backwards compatibility only for use through
Terraform HCL. The actual *Go code* in this repository *may change
without a major version increase*.

Currently the code is mostly located in the `hcloud` package. In the
long term we want to move most of the `hcloud` package into individual
sub-packages located in the `internal` directory. The goal is a
structure similar to HashiCorp's [Terraform Provider
Scaffolding](https://github.com/hashicorp/terraform-provider-scaffolding)

Building the provider
---------------------

Clone repository to: `$GOPATH/src/github.com/hetznercloud/terraform-provider-hcloud`

```sh
$ mkdir -p $GOPATH/src/github.com/hetznercloud; cd $GOPATH/src/github.com/hetznercloud
$ git clone https://github.com/hetznercloud/terraform-provider-hcloud.git
```

Enter the provider directory and build the provider

```sh
$ cd $GOPATH/src/github.com/hetznercloud/terraform-provider-hcloud
$ make build
```

Using the provider
----------------------

if you are building the provider, follow the instructions to [install it as a plugin](https://www.terraform.io/docs/plugins/basics.html#installing-a-plugin). After placing it into your plugins directory, run `terraform init` to initialize it.

Developing the provider
---------------------------

If you wish to work on the provider, you'll first need [Go](http://www.golang.org) installed on your machine (version 1.14+ is *required*). You'll also need to correctly setup a [GOPATH](http://golang.org/doc/code.html#GOPATH), as well as adding `$GOPATH/bin` to your `$PATH`.

To compile the provider, run `make build`. This will build the provider and put the provider binary in the `$GOPATH/bin` directory.

```sh
$ make build
...
$ ./bin/terraform-provider-hcloud
...
```

In order to test the provider, you can simply run `make test`.

```sh
$ make test
```

In order to run the full suite of Acceptance tests run `make testacc`.

*Note:* Acceptance tests create real resources, and often cost money to run.

```
$ make testacc
```
