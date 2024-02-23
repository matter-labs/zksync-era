# Guide de développement

Ce document couvre les actions liées au développement dans zkSync.

## Initialisation du projet

Pour configurer le principal outil, `zk`, exécutez simplement :

```
zk
```

Vous pouvez également configurer l'autocomplétion pour votre shell via :

```
zk completion install
```

Une fois toutes les dépendances installées, le projet peut être initialisé :

```
zk init
```

Cette commande effectuera les actions suivantes :

- Générer le fichier `$ZKSYNC_HOME/etc/env/dev.env` avec les paramètres pour les applications.
- Initialiser les conteneurs Docker avec le nœud Ethereum `geth` pour le développement local.
- Télécharger et décompresser les fichiers pour le backend cryptographique.
- Générer les contrats intelligents requis.
- Compiler tous les contrats intelligents.
- Déployer les contrats intelligents sur le réseau Ethereum local.
- Créer le « bloc de genèse » pour le serveur.

L'initialisation peut prendre assez longtemps, mais de nombreuses étapes (telles que le téléchargement et la décompression des clés et l'initialisation des conteneurs) ne sont nécessaires qu'une seule fois.

Généralement, il est judicieux de faire `zk init` une fois après chaque fusion vers la branche `main` (car la configuration de l'application peut changer).

De plus, il existe une sous-commande `zk clean` pour supprimer les données précédemment générées. Exemples :

```
zk clean --all # Supprimer les configurations générées, la base de données et les sauvegardes.
zk clean --config # Supprimer uniquement les configurations.
zk clean --database # Supprimer la base de données.
zk clean --backups # Supprimer les sauvegardes.
zk clean --database --backups # Supprimer la base de données *et* les sauvegardes, mais pas les configurations.
```

**Quand en avez-vous besoin ?**

1. Si vous avez une base de données initialisée et souhaitez exécuter `zk init`, vous devez d'abord supprimer la base de données.
2. Si, après avoir obtenu de nouvelles fonctionnalités de la branche `main`, votre code cesse de fonctionner et `zk init` n'aide pas, vous pouvez essayer de supprimer `$ZKSYNC_HOME/etc/env/dev.env` et d'exécuter à nouveau `zk init`. Cela peut aider si la configuration de l'application a changé.

Si vous n'avez pas besoin de toute la fonctionnalité de `zk init`, mais avez juste besoin de démarrer/arrêter des conteneurs, utilisez les commandes suivantes :

```
zk up   # Configurer le conteneur `geth`
zk down # Arrêter le conteneur `geth`
```

## Réinitialisation

Lorsque vous changez activement quelque chose qui affecte l'infrastructure (par exemple, le code des contrats), vous n'avez normalement pas besoin de toute la fonctionnalité `init`, car elle contient de nombreuses étapes externes (par exemple, le déploiement de tokens ERC20) qui ne doivent pas être refaites.

Pour ce cas, il existe une commande supplémentaire :

```
zk reinit
```

Cette commande effectue le sous-ensemble minimal des actions de `zk init` nécessaires pour « réinitialiser » le réseau. Elle suppose que `zk init` a été appelé dans l'environnement actuel auparavant. Si `zk reinit` ne fonctionne pas pour vous, vous voudrez peut-être exécuter `zk init` à la place.

## Validation des changements

`zksync` utilise des hooks git pre-commit et pre-push pour des vérifications de base de l'intégrité du code. Les hooks sont configurés automatiquement lors du processus d'initialisation de l'espace de travail. Ces hooks ne permettront pas de valider le code qui ne passe pas plusieurs vérifications.

Actuellement, les critères suivants sont vérifiés :

- Le code Rust doit toujours être formaté via `cargo fmt`.
- Les autres codes doivent toujours être formatés via `zk fmt`.
- Le Prover Dummy ne doit pas être préparé pour la validation (voir ci-dessous pour l'explication).

## Vérification orthographique

Dans notre flux de travail de développement, nous utilisons un processus de vérification orthographique pour assurer la qualité et la précision de notre documentation et des commentaires dans le code. Ceci est réalisé en utilisant deux outils princip

aux : `cspell` et `cargo-spellcheck`. Cette section décrit comment utiliser ces outils et les configurer selon vos besoins.

### Utilisation de la commande de vérification orthographique

La commande de vérification orthographique `zk spellcheck` est conçue pour vérifier les erreurs d'orthographe dans notre documentation et notre code. Pour exécuter la vérification orthographique, utilisez la commande suivante :

```
zk spellcheck
Options :
--pattern <pattern> : Spécifie le modèle glob pour les fichiers à vérifier. Par défaut, c'est docs/**/*.
--use-cargo : Utiliser cargo spellcheck.
--use-cspell : Utiliser cspell.
```

## Vérification des liens

Pour maintenir l'intégrité et la fiabilité de notre documentation, nous utilisons un processus de vérification des liens en utilisant l'outil `markdown-link-check`. Cela garantit que tous les liens dans nos fichiers markdown sont valides et accessibles. La section suivante décrit comment utiliser cet outil et le configurer pour des besoins spécifiques.

### Utilisation de la commande de vérification des liens

La commande de vérification des liens `zk linkcheck` est conçue pour vérifier l'intégrité des liens dans nos fichiers markdown. Pour exécuter la vérification des liens, utilisez la commande suivante :

```
zk linkcheck
Options :
--config <config> : Chemin vers le fichier de configuration de markdown-link-check. Par défaut, c'est './checks-config/links.json'.
```

### Règles générales

**Références de code dans les commentaires** : Lorsque vous faites référence à des éléments de code dans les commentaires de développement, ils doivent être entourés de backticks. Par exemple, référencez une variable comme `block_number`.

**Blocs de code dans les commentaires** : Pour des blocs plus importants de pseudocode ou de code commenté, utilisez des blocs de code formatés comme suit :

````
// ```
// let overhead_for_pubdata = {
//     let numerator: U256 = overhead_for_block_gas * total_gas_limit
//         + gas_per_pubdata_byte_limit * U256::from(MAX_PUBDATA_PER_BLOCK);
//     let denominator =
//         gas_per_pubdata_byte_limit * U256::from(MAX_PUBDATA_PER_BLOCK) + overhead_for_block_gas;
// ```
````

**Paramètres de langue** : Nous utilisons le paramètre de langue Hunspell `en_US`.

**Utilisation de CSpell** : Pour la vérification orthographique dans le répertoire `docs/`, nous utilisons `cspell`. La configuration pour cet outil se trouve dans `cspell.json`. Elle est adaptée pour vérifier notre documentation pour les erreurs d'orthographe.

**Cargo-Spellcheck pour le code Rust et les commentaires de développement** : Pour le code Rust et les commentaires de développement, `cargo-spellcheck` est utilisé. Sa configuration est maintenue dans `era.cfg`.

### Ajout de mots au dictionnaire

Pour ajouter un nouveau mot au dictionnaire du vérificateur orthographique, naviguez vers `/spellcheck/era.dic` et incluez le mot. Assurez-vous que le mot est pertinent et nécessaire pour être inclus dans le dictionnaire afin de maintenir l'intégrité de notre documentation.

## Utilisation du Prover Dummy

Par défaut, le prover choisi est un « dummy », ce qui signifie qu'il ne calcule pas réellement les preuves mais utilise plutôt des mocks pour éviter les calculs coûteux dans l'environnement de développement.

Pour passer du prover dummy au vrai prover, il faut changer `dummy_verifier` en `false` dans `contracts.toml` pour votre environnement (probablement, `etc/env/dev/contracts.toml`) et exécuter `zk init` pour redéployer les contrats intelligents.

## Tests

- Exécution des tests unitaires `rust` :

  ```
  zk test rust
  ```

- Exécution d'un test unitaire `rust` spécifique :

  ```
  zk test rust --package <nom_du_package> --lib <mod>::tests::<nom_du_test_fn> -- --exact
  # e.g. zk test rust --package zksync_core --lib eth_sender::tests::resend_each_block -- --exact
  ```

- Exécution du test d'intégration :

  ```
  zk server           # Doit être exécuté dans le 1 er terminal
  zk test i server    # Doit être exécuté dans le 2ème terminal
  ```

- Exécution des benchmarks :

  ```
  zk f cargo bench
  ```

- Exécution du test de charge :

  ```
  zk server # Doit être exécuté dans le 1er terminal
  zk prover # Doit être exécuté dans le 2ème terminal si vous souhaitez utiliser le vrai prover, sinon ce n'est pas nécessaire.
  zk run loadtest # Doit être exécuté dans le 3ème terminal
  ```

## Contrats

### Reconstruire les contrats

```
zk contract build
```

### Publier le code source sur etherscan

```
zk contract publish
```
