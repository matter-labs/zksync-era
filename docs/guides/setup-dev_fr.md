# Installation des dépendances

## En bref

Si vous utilisez Debian 'propre' sur GCP :

```bash
# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
# NVM
curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.39.5/install.sh | bash
# Tout le nécessaire
sudo apt-get install build-essential pkg-config cmake clang lldb lld libssl-dev postgresql
# Docker
sudo usermod -aG docker VOTRE_UTILISATEUR

## Vous pourriez avoir besoin de vous reconnecter (à cause du changement usermod).

# Node & yarn
nvm install 18
npm install -g yarn
yarn set version 1.22.19

# Outils SQL
cargo install sqlx-cli --version 0.7.3
# Arrêtez postgres par défaut (car nous utiliserons celui de docker)
sudo systemctl stop postgresql
# Démarrer docker.
sudo systemctl start docker
```

## Systèmes d'exploitation pris en charge

zkSync peut actuellement être lancé sur n'importe quel système d'exploitation \*nix (par exemple, toute distribution linux ou MacOS).

Si vous utilisez Windows, assurez-vous d'utiliser WSL 2, car WSL 1 est connu pour causer des problèmes.

De plus, si vous allez utiliser WSL 2, assurez-vous que votre projet se trouve dans le _système de fichiers linux_, car
l'accès aux partitions NTFS depuis WSL est très lent.

Si vous utilisez MacOS avec un processeur ARM (par exemple, M1/M2), assurez-vous que vous travaillez dans l'environnement _natif_
(par exemple, votre terminal et votre IDE ne fonctionnent pas sous Rosetta, et votre chaîne d'outils est native). Essayer de travailler avec le code zkSync via
Rosetta peut causer des problèmes difficiles à détecter et à déboguer, alors assurez-vous de tout vérifier avant de commencer.

Si vous êtes un utilisateur de NixOS ou souhaitez avoir un environnement reproductible, passez à la section sur `nix`.

## `Docker`

Installez `docker`. Il est recommandé de suivre les instructions du
[site officiel](https://docs.docker.com/install/).

Note : actuellement le site officiel propose d'utiliser Docker Desktop pour Linux, qui est un outil GUI avec de nombreux caprices. Si vous
voulez seulement avoir l'outil CLI, vous avez besoin du package `docker-ce` et vous pouvez suivre
[ce guide](https://www.digitalocean.com/community/tutorials/how-to-install-and-use-docker-on-ubuntu-20-04) pour Ubuntu.

Installer `docker` via `snap` ou depuis le dépôt par défaut peut causer des problèmes.

Vous devez installer à la fois `docker` et `docker compose`.

**Note :** `docker compose` est installé automatiquement avec `Docker Desktop`.

**Note :** Sur Linux, vous pourriez rencontrer l'erreur suivante lorsque vous essayez de travailler avec `zksync` :

```
ERROR: Couldn't connect to Docker daemon - you might need to run `docker-machine start default`.
```

Si c'est le cas, vous **n'avez pas besoin** d'installer `docker-machine`. Très probablement, cela signifie que votre utilisateur n'est pas ajouté au
groupe `docker`. Vous pouvez le vérifier comme suit :

```bash
docker-compose up # Devrait soulever la même erreur.
sudo docker-compose up # Devrait commencer à fonctionner.
```

Si la première commande échoue, mais que la seconde réussit, alors vous devez ajouter votre utilisateur au groupe `docker` :

```bash
sudo usermod -a -G docker votre_nom_utilisateur
```

Après cela, vous devriez vous déconnecter et vous reconnecter (les groupes d'utilisateurs sont actualisés après la connexion). Le problème devrait être résolu à cette étape.

Si se déconnecter ne résout pas le problème, redémarrer l'ordinateur le devrait.

## `Node` & `Yarn`

1. Installez `Node` (nécessite la version `v18.18.0`). Comme notre équipe essaie toujours d'utiliser la dernière version LTS de `Node.js`, nous vous suggérons d'installer [nvm](https://github.com/nvm-sh/nvm). Cela vous permettra de mettre à jour facilement la version de `Node.js` à l'avenir (en exécutant `nvm use` à la racine du dépôt)
2. Installez `yarn` (assurez-vous d'obtenir la version 1.22.19 - vous pouvez changer la version en exécutant `yarn set version 1.22.19`). Les instructions se trouvent sur le [site officiel](https://classic.yarnpkg.com/en/docs/install/). Vérifiez si `yarn` est installé en exécutant `yarn -v`. Si vous rencontrez des problèmes lors de l'installation de `yarn`, il se peut que votre gestionnaire de paquets ait installé le mauvais paquet. Assurez-vous de suivre attentivement les instructions ci-dessus sur le site officiel. Il contient de nombreux guides de dépannage.

## `Axel`

Installez `axel` pour télécharger les clés :

Sur mac :

```bash
brew install axel
```

Sur Linux basé sur Debian :

```bash
sudo apt-get install axel
```

Vérifiez la version de `axel` avec la commande suivante :

```
axel --version
```

Assurez-vous que la version est supérieure à `2.17.10`.

## `clang`

Pour compiler RocksDB, vous devez avoir LLVM disponible. Sur Linux basé sur Debian, il peut être installé comme suit :

Sur Linux basé sur Debian :

```bash
sudo apt-get install build-essential pkg-config cmake clang lldb lld
```

Sur mac :

Vous devez avoir un `Xcode` à jour. Vous pouvez l'installer directement depuis l'`App Store`. Avec les outils de ligne de commande Xcode, vous
obtenez le compilateur Clang installé par défaut. Ainsi, ayant XCode, vous n'avez pas besoin d'installer `clang`.

## `OpenSSL`

Installez OpenSSL :

Sur mac :

```bash
brew install openssl
```

Sur Linux basé sur Debian :

```bash
sudo apt-get install libssl-dev
```

## `Rust`

Installez la dernière version de `rust`.

Les instructions se trouvent sur le [site officiel](https://www.rust-lang.org/tools/install).

Vérifiez l'installation de `rust` :

```bash
rustc --version
rustc 1.xx.y (xxxxxx 20xx-yy-zz) # Le résultat peut varier selon la version actuelle de rust
```

Si vous utilisez MacOS avec un processeur ARM (par exemple, M1/M2), assurez-vous d'utiliser une chaîne d'outils `aarch64`. Par exemple, lorsque
vous exécutez `rustup show`, vous devriez voir une entrée similaire :

```bash
rustup show
Default host: aarch64-apple-darwin
rustup home:  /Users/user/.rustup

installed toolchains
--------------------

...

active toolchain
----------------

1.67.1-aarch64-apple-darwin (overridden by '/Users/user/workspace/zksync-era/rust-toolchain')
```

Si vous voyez `x86_64` mentionné dans la sortie, probablement vous exécutez (ou avez l'habitude d'exécuter) votre IDE/terminal sous Rosetta. Si
c'est le cas, vous devriez probablement changer la façon dont vous exécutez le terminal, et/ou réinstaller votre IDE, puis réinstaller la
chaîne d'outils Rust.

## Postgres

Installez la dernière version de postgres :

Sur mac :

```
brew install postgresql@14
```

Sur Linux basé sur Debian :

```
sudo apt-get install postgresql
```

### Cargo nextest

[cargo-nextest](https://nexte.st/) est le coureur de test de nouvelle génération pour les projets Rust. `zk test rust` utilise
`cargo nextest` par défaut.

```bash
cargo install cargo-nextest
```

### CLI SQLx

SQLx est une bibliothèque Rust que nous utilisons pour interagir avec Postgres, et sa CLI est utilisée pour gérer les migrations de DB et prendre en charge plusieurs
fonctionnalités de la bibliothèque.

```bash
cargo install sqlx-cli --version 0.7.3
```

## Compilateur Solidity `solc`

Installez le dernier compilateur solidity.

Sur mac :

```bash
brew install solidity
```

Sur Linux basé sur Debian :

```bash
sudo add-apt-repository ppa:ethereum/ethereum
sudo apt-get update
sudo apt-get install solc
```

Alternativement, téléchargez une [version précompilée](https://github.com/ethereum/solc-bin) et ajoutez-la à votre PATH.

## Python

La plupart des environnements auront cela préinstallé mais sinon, installez Python.

## Méthode plus simple en utilisant `nix`

Nix est un outil qui peut récupérer _exactement_ les bonnes dépendances spécifiées via des hachages. La configuration actuelle est uniquement pour Linux mais
il est probable qu'elle puisse être adaptée pour Mac.

Installez `nix`. Activez la commande nix et les flocons.

Installez docker, rustup et utilisez rust pour installer SQLx CLI comme décrit ci-dessus. Si vous êtes sur NixOS, vous devez également
activer nix-ld.

Allez dans le dossier zksync et exécutez `nix develop --impure`. Après cela, vous êtes dans un shell qui a toutes les
dépendances.

## Environnement

Modifiez les lignes ci-dessous et ajoutez-les à votre fichier de profil shell (par exemple, `~/.bash_profile`, `~/.zshrc`):

```bash
# Ajoutez le chemin ici :
export ZKSYNC_HOME=/chemin/vers/zksync

export PATH=$ZKSYNC_HOME/bin:$PATH

# Si vous êtes comme moi, décommentez :
# cd $ZKSYNC_HOME
```

### Astuce : `mold`

Optionnellement, vous voudrez peut-être optimiser le temps de compilation avec l'éditeur de liens moderne, [`mold`](https://github.com/rui314/mold).

Cet éditeur de liens accélérera les temps de compilation, qui peuvent être assez importants pour les binaires Rust.

Suivez les instructions dans le dépôt pour l'installer et l'activer pour Rust.

## Astuce : Accélérer la compilation de `RocksDB`

Par défaut, chaque fois que vous compilez la crate `rocksdb`, elle compilera les sources C++ requises à partir de zéro. Cela peut être évité
en utilisant des versions précompilées de la bibliothèque, et cela améliorera considérablement vos temps de compilation.

Pour ce faire, vous pouvez mettre les bibliothèques compilées dans un emplacement persistant, et ajouter ce qui suit à votre fichier de configuration shell (par exemple, `.zshrc` ou `.bashrc`):

```
export ROCKSDB_LIB_DIR=<emplacement de la bibliothèque>
export SNAPPY_LIB_DIR=<emplacement de la bibliothèque>
```

Assurez-vous que les bibliothèques compilées correspondent à la version actuelle de RocksDB. Une façon de les obtenir est de compiler le
projet de la manière habituelle une fois, puis de prendre les bibliothèques construites de
`target/{debug,release}/build/librocksdb-sys-{valeur aléatoire}/out`.
