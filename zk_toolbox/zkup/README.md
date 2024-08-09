# zkup - zk_toolbox Installer

`zkup` is a script designed to simplify the installation of
[zk_toolbox](https://github.com/matter-labs/zksync-era/tree/main/zk_toolbox). It allows you to install the tool from a
local directory or directly from a GitHub repository.

## Getting Started

To install `zkup`, run the following command:

```bash
curl -L https://raw.githubusercontent.com/matter-labs/zksync-era/main/zk_toolbox/zkup/install | bash
```

After installing `zkup`, you can use it to install `zk_toolbox` with:

```bash
zkup
```

## Usage

The `zkup` script provides various options for installing `zk_toolbox`:

### Options

- `-p, --path <path>`  
  Specify a local path to install `zk_toolbox` from. This option is ignored if `--repo` is provided.

- `-r, --repo <repo>`  
  GitHub repository to install from (e.g., "matter-labs/zksync-era"). Defaults to "matter-labs/zksync-era".

- `-b, --branch <branch>`  
  Git branch to use when installing from a repository. Ignored if `--commit` or `--version` is provided.

- `-c, --commit <commit>`  
  Git commit hash to use when installing from a repository. Ignored if `--branch` or `--version` is provided.

- `-v, --version <version>`  
  Git tag to use when installing from a repository. Ignored if `--branch` or `--commit` is provided.

- `--skip-zk-supervisor`  
  Skip the installation of the `zk_supervisor` binary.

### Local Installation

If you provide a local path using the `-p` or `--path` option, `zkup` will install `zk_toolbox` from that directory.
Note that repository-specific arguments (`--repo`, `--branch`, `--commit`, `--version`) will be ignored in this case to
preserve git state.

### Repository Installation

By default, `zkup` installs `zk_toolbox` from the "matter-labs/zksync-era" GitHub repository. You can specify a
different repository, branch, commit, or version using the respective options. If multiple arguments are provided,
`zkup` will prioritize them as follows:

- `--version`
- `--commit`
- `--branch`

### Examples

**Install from a GitHub repository with a specific version:**

```bash
zkup --repo matter-labs/zksync-era --version 0.1.1
```

**Install from a local path, skipping `zk_supervisor`:**

```bash
zkup --path /path/to/local/zk_toolbox --skip-zk-supervisor
```
