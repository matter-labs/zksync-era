# Creating a GCP VM

In this section we will cover the creation of a VM suitable for prover development. We assume that you already have
access to the GCP cluster.

## When you need a VM

Generally, you don't always need a VM to work on prover. You typically need it to either modify the code under
`cfg(feature = "gpu")` flag, or when you need to run some tests. Moreover, VMs are shared, e.g. many people have access
to them, and you can't store sensitive data (like SSH keys) there, so they can't be used as primary workstations.
Finally, the VMs with GPU aren't cheap, so we expect you to use them when you really need them.

A typical workflow so far is to instantiate a new VM when you need it, and remove once you're done. Remember: even if
the VM is stopped, the SSD is persisted, so it's not free.

## Create a VM

Open [Google cloud console](https://console.cloud.google.com/) and choose "Compute Engine".

On the "Compute Engine" page choose the cluster suitable for creating VMs with GPU, and then click on "Create instance".

We will need an GPU **L4** instance, so find the zone that is close to you geographically and has such instances. At the
time of writing, `europe-west2` is one of the possible options. L4 is recommended as the cheapest option, but you may
use a beefier machine if you need it.

When you choose the region, set the following options:

- Name: A descriptive name that contains your name, e.g. `john-doe-prover-dev-machine`.
- Region and zone: Values you've found above.
- Machine configuration: "GPUs", then:
  - GPU Type: NVIDIA L4
  - Number of GPUs: 1
  - Machine type: Preset, `g2-standard-16`
- Availability policies: Choose standard provisioning. Spot instances can be preempted while you work on them, which
  will disrupt your flow.
- Then click on "VM provisioning model advanced settings" and
  - Click on "Set a time limit for the VM"
  - Set the limit to 8 hours
- On VM termination: Stop
- Boot disk: Click on "Change", then:
  - Operating system: Ubuntu
  - Version: Ubuntu 22.04 LTS (x86/64)
  - Boot disk type: SSD persistent disk
  - Size: 300GB

Leave the remaining options as is and click on "Create".

You will have to wait a bit and then your instance will be created. Once you see that the machine is running, click on
an arrow near "SSH" in the list of options, and choose "Open in browser window".

You should successfully connect to your machine now.

```admonish warning
Don't forget to remove the VM once you've finished your scope of work. It's OK to keep the machine if you expect to
work with it on the next working day, but otherwise it's better to remove and create a new one when needed.
```

## Adding your own ssh key (on local machine)

Using browser to connect to the machine may not be the most convenient option. Instead, we can add an SSH key to be able
to connect there.

It is highly recommended to generate a new SSH key specifically for this VM, for example:

```
ssh-keygen -t rsa -f ~/.ssh/gcp_vm -C <YOUR WORK EMAIL> -b 2048
```

...where "your work email" is the same email you use to access GCP.

Check the contents of the public key:

```
cat ~/.ssh/gcp_vm.pub
```

Click on your machine name, then click on "Edit". Scroll down until you see "SSH Keys" section and add the generated
public key there. Then save.

Get back to the list of VMs and find the external IP of your VM. Now you should be able to connect to the VM via ssh.
Assuming that your work email is `abc@example.com` and the external IP is 35.35.35.35:

```
ssh -i ~/.ssh/gcp_vm abc@35.35.35.35
```

## Make the VM cozy

If you intend to use the VM somewhat regularly, install all the tools you would normally install on your own machine,
like `zsh` and `nvim`.

It is also _highly recommended_ to install `tmux`, as you will have to run multiple binaries and observe their output.
If you don't know what is it or why should you care, watch [this video](https://www.youtube.com/watch?v=DzNmUNvnB04).

Native `tmux` may be hard to use, so you may also want to install some configuration for it, e.g.

- [oh-my-tmux](https://github.com/gpakosz/.tmux) or
- [tmux-sensible](https://github.com/tmux-plugins/tmux-sensible).

Finally, it is recommended to choose a different terminal theme or prompt than what you use locally, so that you can
easily see whether you're running in the VM or locally.

## Connecting via VS Code

VS Code can connect to VMs via SSH, so you can have the comfort of using your own IDE while still running everything on
a remote machine.

If you're using WSL, note that VS Code will have to look up the keys in Windows, so you will have to copy your keys
there as well, e.g.:

```
cp ~/.ssh/gcp_vm* /mnt/c/Users/User/.ssh
```

Then, when you open a fresh VS Code window, in the "Start" section:

- Choose "Connect to Host"
- Click on "Configure Hosts"
- Create a host entry.

Host entry looks as follows:

```
Host <host_name>
  HostName <external IP>
  IdentityFile <path to private SSH key>
  User <your user name in VM>
```

E.g. for the command we've used as an example before: `ssh -i ~/.ssh/gcp_vm abc@35.35.35.35`, the file will be:

```
Host gcp_vm
  HostName 35.35.35.35
  IdentityFile ~/.ssh/gcp_vm
  User abc
```

Once you've configured the host, you can click on "Connect to" again, then "Connect to Host", and your VM should be
listed there. On the first connect you'll have to confirm that you want to connect to it, and then choose the operating
system (Linux).

## On security

Do not store SSH keys, tokens, or other private information on GCP VMs. Do not use SSH keys forwarding either. These VMs
are shared, and every person has root access to all the VMs by default.

You may, however, use tools like `rsync` or `sshfs`.
