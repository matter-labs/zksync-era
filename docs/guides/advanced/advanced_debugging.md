# Advanced debugging

## Debugging backend in vscode

Our backend takes configuration from environment variables, so before starting the debugging, we must make sure that
they are properly set.

You should create the following file in your `$workspaceFolder/.vscode/` called `prelaunch.py`:

```python
import os
import lldb

# Read the .env file and store the key-value pairs in a array with format ["key=value"]
env_array = []
with open(os.path.join("etc/env/l2-inits/dev.init.env")) as f:
    for line in f:
        if line.strip() and line.strip()[0] != "#":
            env_array.append(line.strip())

with open(os.path.join("etc/env/targets/dev.env")) as f:
    for line in f:
        if line.strip() and line.strip()[0] != "#":
            env_array.append(line.strip())

target = lldb.debugger.GetSelectedTarget()

launch_info = target.GetLaunchInfo()
launch_info.SetEnvironmentEntries(env_array, True)
target.SetLaunchInfo(launch_info)
```

This file will load environment variables from `dev.init.env` and `dev.env` before starting the binary (notice that we
do this in a particular order, as values in dev.env should be overwriting the ones in dev.init.env).

Afterwards you need to add something like this to your launch.json:

```
 "configurations": [
        {
            "type": "lldb",
            "request": "launch",
            "name": "Debug executable 'zksync_server' DEV ENV",
            "cargo": {
                "args": [
                    "build",
                    "--bin=zksync_server",
                    "--package=zksync_core"
                ],
                "filter": {
                    "name": "zksync_server",
                    "kind": "bin"
                }
            },
            "args": [],
            "cwd": "${workspaceFolder}",
            "preRunCommands": [
                "command script import ${workspaceFolder}/.vscode/prelaunch.py"
            ]
        },
        ...
    ]
```

## Debugging contracts in vscode (using hardhat)

Assuming that you created project in hardhat, that you'd normally test with `hardhat test` - you also also test it with
vscode (which is super powerful - especially as you can have both binaries' debug sessions running in VSCode at the same
time).

in package.json, make sure to have:

```json
"scripts": {
        //...
        "test": "hardhat test",
        //...
    }
```

and then in VSCode's launch.json:

```json
{
  "version": "0.2.0",
  "configurations": [
    {
      "type": "node",
      "request": "launch",
      "name": "Launch Program",
      "console": "integratedTerminal",
      "runtimeExecutable": "yarn",
      "runtimeArgs": ["test", "${workspaceFolder}/test/try.js"]
    }
  ]
}
```

where `test/try.js` is your test code.

## Performance analysis of rust binaries (flame graphs)

If you'd like to analyze the CPU performance of your rust binary, you can use 'perf' to compute the flame graphs.

First - run the binary with perf enabled (this will make the binary a little bit slower):

```
sudo perf record -F500 --call-graph=dwarf,65528  /path/to/binary --other --flags
```

(you can also connect to already running binary - by providing its PID with `-p` option)

When you're done collecting records, you have to convert them into flame-graph friendly format, by running:

```
sudo perf script -F +pid > perfbench.script
```

This will create the perfbench.script file, that you can later upload to <https://profiler.firefox.com/> and see the
detailed flame graph.

## Debugging/understanding/tracing zkEVM assembly

Currently this is quite a complex process, but we're working on making it a little bit smoother.

You start by installing the 'compiler-tester' repo (see its README.md instructions for details) - it is quite heavy as
it needs the LLVM etc etc.

Afterwards, you can look at one of the tests (for example
[tests/solidity/simple/default.sol](https://github.com/matter-labs/era-compiler-tests/blob/main/solidity/simple/default.sol)).

```solidity
//! { "cases": [ {
//!     "name": "first",
//!     "inputs": [
//!         {
//!             "method": "first",
//!             "calldata": [
//!             ]
//!         }
//!     ],
//!     "expected": [
//!         "42"
//!     ]
//! }, ] }

// SPDX-License-Identifier: MIT

pragma solidity >=0.4.16;

contract Test {
  function first() public pure returns (uint64) {
    uint64 result = 42;
    return result;
  }
}

```

As you can see - it is self-contained - it has the solidity code at the bottom, and the top comments are used to define
the test case - and expected result.

You can run it by calling:

```shell
cargo run --release --bin compiler-tester -- -DT \
        --path='tests/solidity/simple/default.sol' \
        --mode='Y+M3B3 0.8.19'
```

And then collect the detailed tracing information from trace directory. You'll notice that you have 2 files for each
test - one covering the deployment, and one covering the test run.

You can take test run one and upload it to [our debugger](https://explorer.zksync.io/tools/debugger) to see detailed
zkAssembler and state of memory, heap, stack and registers at each execution step.
