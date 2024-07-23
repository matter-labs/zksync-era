# Creating a GCP VM

In this section we will cover the creation of a VM suitable for prover development. We assume that you already have
access to the GCP cluster.

## Create a VM

Open [Google cloud console](https://console.cloud.google.com/) and choose "Compute Engine".

On the "Compute Engine" page choose the cluster suitable for creating VMs with GPU, and then click on "Create instance".

We will need an GPU **L4** instance, so find the zone that is close to you geographically and has such instances. At the
time of writing, `europe-west2` is one of the possible options.

When you choose the region, set the following options:

- Name: A descriptive name that contains your name, e.g. `john-doe-prover-dev-machine`.
- Region and zone: Values you've found above.
- Machine configuration: "GPUs", then:
  - GPU Type: NVIDIA L4
  - Number of GPUs: 1
  - Machine type: Preset, `g2-standard-32`
- Availability policies: Spot ⚠️ Make sure to not skip that, standard provisioning is much more expensive.
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

## Adding your own ssh key

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

Finally, it is recommended to choose a different terminal theme than what you use locally, so that you can easily see
whether you're running in the VM or locally.

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

## Generating the SSH key on VM

Last but not least, you probably want to make sure that you can access GitHub from your VM, so that you can push and
access private repositories.

The simplest option would be to generate a new key on the VM and add it to GitHub. You probably know how to do it, but
if not, here's a guide on
[generating a key](https://docs.github.com/en/authentication/connecting-to-github-with-ssh/generating-a-new-ssh-key-and-adding-it-to-the-ssh-agent)
and
[adding it to github](https://docs.github.com/en/authentication/connecting-to-github-with-ssh/adding-a-new-ssh-key-to-your-github-account?platform=linux).

If you're signing your commits, don't forget to setup signing key as well.

If you want to use another approach, e.g. use key forwarding, feel free to do so. However, using `https` to interact
with GitHub is not recommended.
