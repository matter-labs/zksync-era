# Exécution de l'application

Ce document couvre les scénarios courants pour le lancement des ensembles d'applications zkSync localement.

## Prérequis

Préparez les prérequis de l'environnement de développement : voir

[Installation des dépendances](./setup-dev_fr.md)

## Configuration de l'environnement de développement local

Configuration :

```
zk # installe et construit zk lui-même
zk init
```

Si vous rencontrez d'autres problèmes avec la commande `zk init`, allez à la section
[Dépannage](https://github.com/matter-labs/zksync-era/blob/main/docs/guides/launch_fr.md#troubleshooting) à la fin de ce fichier. Il y a des solutions pour certains cas d'erreur courants.

Pour réinitialiser complètement l'environnement de développement :

- Arrêtez les services :

  ```
  zk down
  ```

- Répétez la procédure de configuration ci-dessus

Si `zk init` a déjà été exécuté, et maintenant vous avez seulement besoin de démarrer les conteneurs Docker (par exemple, après un redémarrage), lancez simplement :

```
zk up
```

## (Re)déployer la base de données et les contrats

```
zk contract redeploy
```

## Configurations de l'environnement

Les fichiers de configuration de l'environnement sont situés dans `etc/env/`

Listez les configurations :

```
zk env
```

Changez entre les configurations :

```
zk env <NOM_ENV>
```

La configuration par défaut est `dev.env`, qui est générée automatiquement à partir de `dev.env.example` lors de l'exécution de la commande `zk init`.

## Construire et exécuter le serveur

Exécutez le serveur :

```
zk server
```

Le serveur est configuré en utilisant les fichiers env dans le répertoire `./etc/env`. Après la première initialisation, le fichier `./etc/env/dev.env` sera créé. Par défaut, ce fichier est copié à partir du modèle `./etc/env/dev.env.example`.

Assurez-vous que les variables d'environnement sont correctement définies, vous pouvez le vérifier en exécutant : `zk env`. Vous devriez voir `* dev` dans la sortie.

## Exécuter le serveur en utilisant le stockage d'objets de Google Cloud Storage au lieu du stockage par défaut en mémoire

Obtenez le fichier service_account.json contenant les identifiants GCP à partir du secret Kubernetes pour l'environnement pertinent (stage2/testnet2) ajoutez ce fichier à l'emplacement par défaut ~/gcloud/service_account.json ou mettez à jour object_store.toml avec l'emplacement du fichier

```
zk server
```

## Exécuter le serveur de preuve

Exécution sur une machine sans GPU

```shell
zk f cargo +nightly run --release --bin zksync_prover
```

Exécution sur une machine avec GPU

```shell
zk f cargo +nightly run --features gpu --release --bin zksync_prover
```

## Exécuter le générateur de clés de vérification

```shell
# assurez-vous que le fichier setup_2^26.key est dans le répertoire courant, le fichier peut être téléchargé depuis https://storage.googleapis.com/matterlabs-setup-keys-us/setup-keys/setup_2\^26.key

# Pour générer toutes les clés de vérification
cargo run --release --bin zksync_verification_key_generator


```

## Générer des clés de vérification binaires pour les clés de vérification json existantes

```shell
cargo run --release --bin zksync_json_to_binary_vk_converter -- -o /chemin/vers/output-binary-vk
```

## Générer l'engagement pour les clés de vérification existantes

```shell
cargo run --release --bin zksync_commitment_generator
```

## Exécuter le vérificateur de contrat

```shell
# Pour traiter un nombre fixe de tâches
cargo run --release --bin zksync_contract_verifier -- --jobs-number X

# Pour exécuter jusqu'à la sortie manuelle
zk contract_verifier
```

## Dépannage

### Erreur SSL : échec de la vérification du certificat

**Problème**. `zk init` échoue avec l'erreur suivante :

```
Initializing download: https://storage.googleapis.com/matterlabs-setup-keys-us/setup-keys/setup_2%5E20.key
SSL error: certificate verify failed
```

**Solution**. Assurez-vous que la version de `axel` sur votre ordinateur est `2.17.10` ou supérieure.

### rmSync n'est pas une fonction

**Problème**. `zk init` échoue avec l'erreur suivante :

```
fs_1.default.rmSync n'est pas une fonction
```

**Solution**. Assurez-vous que la version de `node.js` installée sur votre ordinateur est `14.14.0` ou supérieure.

### Bytecode invalide : ()

**Problème**. `zk init` échoue avec une erreur similaire à :

```
Running `target/release/zksync_server --genesis`
2023-04-05T14:23:40.291277Z  INFO zksync_core::genesis: running regenesis
thread 'main' panicked at 'Invalid bytecode: ()', core/lib/utils/src/bytecode.rs:159:10
stack backtrace:
   0:        0x104551410 - std::backtrace_rs::backtrace::libunwind::trace::hf9c5171f212b04e2
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/../../backtrace/src/backtrace/libunwind.rs:93:5
   1:        0x104551410 - std::backtrace_rs::backtrace::trace_unsynchronized::h179003f6ec753118
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/../../backtrace/src/backtrace/mod.rs:66:5
   2:        0x104551410 - std::sys_common::backtrace::_print_fmt::h92d38f701cf42b17
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/sys_common/backtrace.rs:65:5
   3:        0x104551410 - <std::sys_common::backtrace::_print::DisplayBacktrace as core::fmt::Display>::fmt::hb33e6e8152f78c95
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/sys_common/backtrace.rs:44:22
   4:        0x10456cdb0 - core::fmt::write::hd33da007f7a27e39
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/core/src/fmt/mod.rs:1208:17
   5:        0x10454b41c - std::io::Write::write_fmt::h7edc10723862001e
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/io/mod.rs:1682:15
   6:        0x104551224 - std::sys_common::backtrace::_print::h5e00f05f436af01f
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/sys_common/backtrace.rs:47:5
   7:        0x104551224 - std::sys_common::backtrace::print::h895ee35b3f17b334
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/sys_common/backtrace.rs:34:9
   8:        0x104552d84 - std::panicking::default_hook::{{closure}}::h3b7ee083edc2ea3e
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/panicking.rs:267:22
   9:        0x104552adc - std::panicking::default_hook::h4e7c2c28eba716f5
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/panicking.rs:286:9
  10:        0x1045533a8 - std::panicking::rust_panic_with_hook::h1672176227032c45
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/panicking.rs:688:13
  11:        0x1045531c8 - std::panicking::begin_panic_handler::{{closure}}::h0b2d072f9624d32e
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/panicking.rs:579:13
  12:        0x104551878 - std::sys_common::backtrace::__rust_end_short_backtrace::he9abda779115b93c
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/sys_common/backtrace.rs:137:18
  13:        0x104552f24 - rust_begin_unwind
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/panicking.rs:575:5
  14:        0x1045f89c0 - core::panicking::panic_fmt::h23ae44661fec0889
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/core/src/panicking.rs:64:14
  15:        0x1045f8ce0 - core::result::unwrap_failed::h414a6cbb12b1e143
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/core/src/result.rs:1791:5
  16:        0x103f79a30 - zksync_utils::bytecode::hash_bytecode::h397dd7c5b6202bf4
  17:        0x103e47e78 - zksync_contracts::BaseSystemContracts::load_from_disk::h0e2da8f63292ac46
  18:        0x102d885a0 - zksync_core::genesis::ensure_genesis_state::{{closure}}::h5143873f2c337e11
  19:        0x102d7dee0 - zksync_core::genesis_init::{{closure}}::h4e94f3d4ad984788
  20:        0x102d9c048 - zksync_server::main::{{closure}}::h3fe943a3627d31e1
  21:        0x102d966f8 - tokio::runtime::park::CachedParkThread::block_on::h2f2fdf7edaf08470
  22:        0x102df0dd4 - tokio::runtime::runtime::Runtime::block_on::h1fd1d83272a23194
  23:        0x102e21470 - zksync_server::main::h500621fd4d160768
  24:        0x102d328f0 - std::sys_common::backtrace::__rust_begin_short_backtrace::h52973e519e2e8a0d
  25:        0x102e08ea8 - std::rt::lang_start::{{closure}}::hbd395afe0ab3b799
  26:        0x10454508c - core::ops::function::impls::<impl core::ops::function::FnOnce<A> for &F>::call_once::ha1c2447b9b665e13
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/core/src/ops/function.rs:606:13
  27:        0x10454508c - std::panicking::try::do_call::ha57d6d1e9532dc1f
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/panicking.rs:483:40
  28:        0x10454508c - std::panicking::try::hca0526f287961ecd
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/panicking.rs:447:19
  29:        0x10454508c - std::panic::catch_unwind::hdcaa7fa896e0496a
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/panic.rs:137:14
  30:        0x10454508c - std::rt::lang_start_internal::{{closure}}::h142ec071d3766871
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/rt.rs:148:48
  31:        0x10454508c - std::panicking::try::do_call::h95f5e55d6f048978
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/panicking.rs:483:40
  32:        0x10454508c - std::panicking::try::h0fa00e2f7b4a5c64
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/panicking.rs:447:19
  33:        0x10454508c - std::panic::catch_unwind::h1765f149814d4d3e
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/panic.rs:137:14
  34:        0x10454508c - std::rt::lang_start_internal::h00a235e820a7f01c
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/rt.rs:148:20
  35:        0x102e21578 - _main
Error: Genesis is not needed (either Postgres DB or tree's Rocks DB is not empty)
```

**Description**. Cela signifie que votre fichier de configuration bytecode contient une entrée vide : `"bytecode" : "0x"`. Cela se produit parce que votre dépendance dans `zksync-2-dev/etc/system-contracts/package.json` sur `"@matterlabs/hardhat-zksync-solc"` est obsolète. Nous ne nous attendons pas à ce que cette erreur se produise car nous avons mis à jour la dernière version qui corrige le problème.

**Solution**. Mettez à jour votre dépendance et réinitialisez :

```
yarn add -D @matterlabs/hardhat-zksync-solc # dans le dossier des contrats système
zk clean --all && zk init
```

Au cours de l'exécution, cela a changé de :

```
    "@matterlabs/hardhat-zksync-solc": "^0.3.14-beta.3",
```

à :

```
    "@matterlabs/hardhat-zksync-solc": "^0.3.15",
```

### Erreur : La longueur du bytecode en mots de 32 octets doit être impaire

**Problème**. `zk init` échoue avec une erreur similaire à :

```
Successfully generated Typechain artifacts!
Error: Error: Bytecode length in 32-byte words must be odd
    at hashL2Bytecode (/Users/emilluta/code/zksync-2-dev/contracts/zksync/src/utils.ts:29:15)
    at computeL2Create2Address (/Users/emilluta/code/zksync-2-dev/contracts/zksync/src/utils.ts:53:26)
    at /Users/emilluta/code/zksync-2-dev/contracts/zksync/src/compileAndDeployLibs.ts:50:63
    at step (/Users/emilluta/code/zksync-2-dev/contracts/zksync/src/compileAndDeployLibs.ts:33:23)
    at Object.next (/Users/emilluta/code/zksync-2-dev/contracts/zksync/src/compileAndDeployLibs.ts:14:53)
    at fulfilled (/Users/emilluta/code/zksync-2-dev/contracts/zksync/src/compileAndDeployLibs.ts:5:58)
error Command failed with exit code 1.
info Visit https://yarnpkg.com/en/docs/cli/run for documentation about this command.
error Command failed.
Exit code: 1
Command: /Users/emilluta/.nvm/versions/node/v16.19.1/bin/node
Arguments: /opt/homebrew/Cellar/yarn/1.22.19/libexec/lib/cli.js compile-and-deploy-libs
Directory: /Users/emilluta/code/zksync-2-dev/contracts/zksync
Output:

info Visit https://yarnpkg.com/en/docs/cli/workspace for documentation about this command.
error Command failed with exit code 1.
info Visit https://yarnpkg.com/en/docs/cli/run for documentation about this command.
Error: Child process exited with code 1
```

**Description**. Cela signifie que votre fichier de configuration bytecode contient une entrée vide : `"bytecode" : "0x"`. Cela se produit parce que votre dépendance dans `zksync-2-dev/contracts/zksync/package.json` sur `"@matterlabs/hardhat-zksync-solc"` est obsolète. Nous ne nous attendons pas à ce que cette erreur se produise car nous avons mis à jour la dernière version qui corrige le problème.

**Solution**. Mettez à jour votre dépendance et réinitialisez :

```
yarn add -D @matterlabs/hardhat-zksync-solc # dans le dossier des contrats système
zk clean --all && zk init
```

Au cours de l'exécution, cela a changé de :

```
    "@matterlabs/hardhat-zksync-solc": "^0.3.14-beta.3",
```

à :

```
    "@matterlabs/hardhat-zksync-solc": "^0.3.15",
```
