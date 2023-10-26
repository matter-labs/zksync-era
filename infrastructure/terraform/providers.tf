terraform {
  required_providers {
    hcloud = {
      source = "hetznercloud/hcloud"
      version = "1.42.1"
    }
  }
  backend "s3" {
    bucket = "terraform-lambdaclass"
    key = "zksync/terraform.tfstate" # TODO: this should be a variable probably
    region = "us-west-2"
  }
}

provider "hcloud" {
}
