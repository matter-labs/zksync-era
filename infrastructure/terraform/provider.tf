# hetzner cloud provider
provider "hcloud" {
    token = "${var.hcloud_token}"
}

terraform {
  required_providers {
    hcloud = {
      source = "hetznercloud/hcloud"
    }
  }
  required_version = ">= 0.13"
}
