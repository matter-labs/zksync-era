## Prerequisites
- Terraform
- AWS CLI

## Steps
1. export AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY with corresponding values
1. export HCLOUD_TOKEN with Hetzner token
1. In /user_data/, modify any SSH key as needed
1. `terraform init`
1. `terraform apply`
