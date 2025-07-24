# Local upgrade testing

While it is theoretically possible to do it in CI-like style, it generally leads to needless recompilations, esp of rust
programs.

Here we contain the files/instructions needed to test an upgrade locally. The approach is to have two clones of the
repo, and to copy the config files between them. We could save target-etc in a different directory, but this process is
simpler and more robust.

We clone the two repos. We switch between them by copying them into zksync-working.

## Disclaimer on running the scripts locally

Currently, the state of commands is WIP. They may not work as-is and serve more as demonstration of how to test an upgrade locally than a command that fully works out of the box.

The commands contain comments with the instructions and tips on running those locally.

## Setup

`cp infrastructure/local-upgrade-testing/era-cacher/prepare.sh <YOUR_PATH>` into your appropriate folder where you want
to the upgrade testing folder to be created.

`run prepare.sh` . This creates upgrade testing folder, clones two zksync-era s into it. Initialiaes both. Copies
era-cacher into upgrade-testing.

`sh ./era-cacher/do-upgrade.sh`
