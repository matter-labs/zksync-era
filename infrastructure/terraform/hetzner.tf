# Variables
variable "instances" {
  default = "1"
}

variable "location" {
  default = "nbg1"	
}

variable "http_protocol" {
  default = "http"	
}

variable "http_port" {
  default = "80"
}

# Servers
resource "hcloud_server" "web" {
  count         = var.instances
  name          = "zksync-${count.index}"
  image         = "debian-12"
  server_type   = "ccx23"
  location      = var.location
  #ssh_keys    	= ["bastion_key"] TODO: Restore
  firewall_ids	= [hcloud_firewall.webservers_fw.id]
  depends_on = [
    hcloud_network_subnet.private_subnet
  ]
  labels = {
    type = "zksync-${count.index}"
  }
  user_data = file("user_data/operator.yml")
}

# Bastion
resource "hcloud_server" "bastion" {
  name         = "zksync-bastion"
  image        = "debian-12"
  server_type  = "cpx21"
  location     = var.location
  #ssh_keys     = ["zksync_ssh"] TODO: Restore
  firewall_ids = [hcloud_firewall.bastion_fw.id]

  user_data = file("user_data/bastion.yml")
}

# Firewall
resource "hcloud_firewall" "webservers_fw" {
  name = "webservers-firewall"

  # HTTP connections from load balancer
  rule {
    direction  = "in"
    protocol   = "tcp"
    port       = "4000"
    source_ips = ["10.0.1.10/32"] # Load balancer's IP.
  }

  # Add HTTPS

  # SSH connection from bastion host
  rule {
    direction  = "in"
    protocol   = "tcp"
    port       = "22"
    source_ips = ["10.0.1.2/32"] # Bastion's IP.
  }
}

resource "hcloud_firewall" "bastion_fw" {
  name = "bastion-firewall"

  rule {
    direction  = "in"
    protocol   = "tcp"
    port       = "22"
    source_ips = [
      "0.0.0.0/0",
      "::/0"
    ]
  }
}

# Networking
resource "hcloud_network" "private_network" {
  name     = "private_network"
  ip_range = "10.0.1.0/24"
}

resource "hcloud_network_subnet" "private_subnet" {
  network_id   = hcloud_network.private_network.id
  type         = "cloud"
  network_zone = "eu-central"
  ip_range     = "10.0.1.0/24"
  vswitch_id   = "43436"
}

resource "hcloud_server_network" "webservers_network" {
  count      = var.instances
  server_id  = hcloud_server.web[count.index].id
  ip		 = "10.0.1.${count.index + var.instances}" # 10.0.1.[0-2]
  subnet_id  = hcloud_network_subnet.private_subnet.id
}

resource "hcloud_server_network" "bastion_network" {
  server_id = hcloud_server.bastion.id
  ip		= "10.0.1.2"
  subnet_id = hcloud_network_subnet.private_subnet.id
}

# Load balancer
# resource "hcloud_load_balancer" "load_balancer" {
#   name               = "load-balancer"
#   load_balancer_type = "lb11"
#   location           = var.location
#   algorithm {
#     type = "round_robin"
#   }
# }

# resource "hcloud_load_balancer_network" "load_balancer_network" {
#   load_balancer_id        = hcloud_load_balancer.load_balancer.id
#   subnet_id               = hcloud_network_subnet.private_subnet.id
#   ip					  = "10.0.1.10"
#   enable_public_interface = "true"
# }

# resource "hcloud_load_balancer_target" "load_balancer_target" {
#   count			   = var.instances
#   type             = "server"
#   load_balancer_id = hcloud_load_balancer.load_balancer.id
#   server_id        = hcloud_server.web[count.index].id
#   use_private_ip   = "true"
#   depends_on = [
#       hcloud_server_network.webservers_network
#   ]
# }

# resource "hcloud_load_balancer_service" "load_balancer_service" {
#   load_balancer_id = hcloud_load_balancer.load_balancer.id
#   protocol         = "https"
#   destination_port = "4000"
#   http {
# 	sticky_sessions = "true"
# 	cookie_lifetime = "36000"
# 	redirect_http   = "true"
#   }

#   health_check {
#     protocol = "http"
#     port     = "4000"
#     interval = "10"
#     timeout  = "5"
# 	retries  = "3"
#     http {
#       path         = "/"
#       status_codes = ["2??", "3??"]
#     }
#   }
# }