resource "hcloud_volume" "web_server_volume" {
  count    = var.instances
  name     = "zk-server-volume-${count.index}"
  size     = var.disk_size
  location = var.location
  format   = "xfs"
}

resource "hcloud_volume_attachment" "web_vol_attachment" {
  count     = var.instances
  volume_id = hcloud_volume.web_server_volume[count.index].id
  server_id = hcloud_server.web[count.index].id
  automount = true
}
