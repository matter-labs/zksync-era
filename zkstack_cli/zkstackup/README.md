# zkstackup - ZK Stack CLI Installer

`zkstackup` is a script designed to simplify the installation of
[ZK Stack CLI](https://github.com/matter-labs/zksync-era/tree/main/zkstack_cli). It allows you to install the tool from
a local directory or directly from a GitHub repository.

## Getting Started

To install `zkstackup`, run the following command:

```bash
curl -L https://raw.githubusercontent.com/matter-labs/zksync-era/main/zkstack_cli/zkstackup/install | bash
```

After installing `zkstackup`, you can use it to install `zkstack_cli` with:

```bash
zkstackup
```

## Usage

The `zkstackup` script provides various options for installing ZK Stack CLI:

### Options

- `-p, --path <path>`  
  Specify a local path to install ZK Stack CLI from. This option is ignored if `--repo` is provided.

- `-r, --repo <repo>`  
  GitHub repository to install from (e.g., "matter-labs/zksync-era"). Defaults to "matter-labs/zksync-era".

- `-b, --branch <branch>`  
  Git branch to use when installing from a repository. Ignored if `--commit` or `--version` is provided.

- `-c, --commit <commit>`  
  Git commit hash to use when installing from a repository. Ignored if `--branch` or `--version` is provided.

- `-v, --version <version>`  
  Git tag to use when installing from a repository. Ignored if `--branch` or `--commit` is provided.

### Local Installation

If you provide a local path using the `-p` or `--path` option, `zkstackup` will install ZK Stack CLI from that
directory. Note that repository-specific arguments (`--repo`, `--branch`, `--commit`, `--version`) will be ignored in
this case to preserve git state.

### Repository Installation

By default, `zkstackup` installs ZK Stack CLI from the "matter-labs/zksync-era" GitHub repository. You can specify a
different repository, branch, commit, or version using the respective options. If multiple arguments are provided,
`zkstackup` will prioritize them as follows:

- `--version`
- `--commit`
- `--branch`

### Examples

**Install from a GitHub repository with a specific version:**

```bash
zkstackup --repo matter-labs/zksync-era --version 0.1.1
```

**Install from a local path:**

```bash
zkstackup --path /path/to/local/zkstack_cli
```
