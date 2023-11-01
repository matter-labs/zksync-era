variable "hcloud_token" {

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

variable "instances" {
    default = "1"
}

variable "server_type" {
    default = "cpx31"
}

variable "os_type" {
    default = "debian-12"
}

variable "disk_size" {
    default = "60"
}

variable "ip_range" {
    default = "10.0.1.0/24"
}
