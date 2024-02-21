# Architecture du projet zkSync v2

Ce document vous aidera à répondre à la question : _où puis-je trouver la logique pour x ?_ en donnant une structure de type arborescence de répertoires de l'architecture physique du projet zkSync Era.

## Vue d'ensemble de haut niveau

Le dépôt zksync-2-dev comprend les principales unités suivantes :

<ins>**Contrats Intelligents (Smart Contracts):**</ins> Tous les contrats intelligents en charge des protocoles sur L1 & L2. Certains contrats principaux :

- Contrats de pont L1 & L2.
- Le contrat de rollup zkSync sur Ethereum.
- Le contrat de vérificateur de preuve L1.

**<ins>Application Centrale (Core App):**</ins> La couche d'exécution. Un nœud exécutant le réseau zkSync en charge des composants suivants :

- Surveillance du contrat intelligent L1 pour les dépôts ou opérations prioritaires.
- Maintien d'un mempool qui reçoit les transactions.
- Sélection des transactions du mempool, leur exécution dans une VM et modification de l'état en conséquence.
- Génération de blocs de la chaîne zkSync.
- Préparation des circuits pour les blocs exécutés à prouver.
- Soumission des blocs et des preuves au contrat intelligent L1.
- Exposition de l'API web3 compatible avec Ethereum.

**<ins>Application de Preuve (Prover App):**</ins> L'application de preuve prend les blocs et les métadonnées générés par le serveur et construit une preuve de validité zk pour eux.

**<ins>Couche de Stockage (Storage Layer):**</ins> Les différents composants et sous-composants ne communiquent pas directement entre eux via
des API, mais via la seule source de vérité -- la couche de stockage db.

## Vue d'ensemble détaillée

Cette section fournit une carte physique des dossiers & fichiers dans ce dépôt.

- `/contracts`

  - `/ethereum` : Contrats intelligents déployés sur l'Ethereum L1.
  - `/zksync` : Contrats intelligents déployés sur le zkSync L2.

- `/core`

  - `/bin` : Exécutables pour les composants de microservices composant le nœud central zkSync.

    - `/admin-tools` : Outils CLI pour les opérations d'administration (par exemple, redémarrage des travaux de preuve).
    - `/external_node` : Une réplique en lecture qui peut se synchroniser avec le nœud principal.

  - `/lib` : Toutes les crates de bibliothèque utilisées comme dépendances des crates binaires ci-dessus.

    - `/basic_types` : Crate avec les types primitifs essentiels de zkSync.
    - `/config` : Toutes les valeurs configurées utilisées par les différentes applications zkSync.
    - `/contracts` : Contient les définitions des contrats intelligents couramment utilisés.
    - `/crypto` : Primitives cryptographiques utilisées par les différentes crates zkSync.
    - `/dal` : Couche de disponibilité des données
      - `/migrations` : Toutes les migrations db appliquées pour créer la couche de stockage.
      - `/src` : Fonctionnalité pour interagir avec les différentes tables db.
    - `/eth_client` : Module fournissant une interface pour interagir avec un nœud Ethereum.
    - `/eth_signer` : Module pour signer des messages et des txs.
    - `/mempool` : Implémentation du pool de transactions zkSync.
    - `/merkle_tree` : Implémentation d'un arbre de Merkle sparse.
    - `/mini_merkle_tree` : Implémentation en mémoire d'un arbre de Merkle sparse.
    - `/multivm` : Un wrapper sur plusieurs versions de VM qui ont été utilisées par le nœud principal.
    - `/object_store` : Abstraction pour stocker des blobs en dehors du magasin de données principal.
    - `/prometheus_exporter` : Exportateur de données Prometheus.
    - `/queued_job_processor` : Une abstraction pour le traitement asynchrone des travaux
    - `/state` : Un gardien d'état responsable de la gestion de l'exécution des transactions et de la création de miniblocs et de lots L1.
    - `/storage` :

 Une interface de base de données encapsulée.
    - `/test_account` : Une représentation du compte zkSync.
    - `/types` : Opérations, transactions et types communs du réseau zkSync.
    - `/utils` : Aides diverses pour les crates zkSync.
    - `/vlog` : Utilitaire de journalisation zkSync.
    - `/vm` : Interface VM légère hors circuit.
    - `/web3_decl` : Déclaration de l'API Web3.
    - `zksync_core/src`
      - `/api_server` APIs orientées vers l'extérieur.
        - `/web3` : Implémentation zkSync de l'API Web3.
        - `/tx_sender` : Module d'encapsulation de la logique de traitement des transactions.
      - `/bin` : Le point d'entrée principal exécutable pour le serveur zkSync.
      - `/consistency_checker` : Chien de garde zkSync.
      - `/eth_sender` : Soumet des transactions au contrat intelligent zkSync.
      - `/eth_watch` : Récupère des données du L1 pour la résistance à la censure du L2.
      - `/fee_monitor` : Surveille le ratio des frais collectés par l'exécution de txs par rapport aux coûts d'interaction avec Ethereum.
      - `/fee_ticker` : Module pour définir les composants de prix des transactions L2.
      - `/gas_adjuster` : Module pour déterminer les frais à payer dans les txs contenant des blocs soumis au L1.
      - `/gas_tracker` : Module pour prédire le coût du gaz L1 pour les opérations Commit/PublishProof/Execute.
      - `/metadata_calculator` : Module pour maintenir l'arbre d'état zkSync.
      - `/state_keeper` : Le séquenceur. En charge de collecter les txs en attente du mempool, de les exécuter dans la VM et de les sceller dans des blocs.
      - `/witness_generator` : Prend les blocs scellés et génère un _Témoin_, l'entrée pour le prouveur contenant les circuits à prouver.

  - `/tests` : Infrastructure de test pour le réseau zkSync.
    - `/cross_external_nodes_checker` : Un outil pour vérifier la cohérence des nœuds externes par rapport au nœud principal.
    - `/loadnext` : Une application pour les tests de charge du serveur zkSync.
    - `/ts-integration` : Ensemble de tests d'intégration implémentés en TypeScript.

- `/prover` : Application d'orchestrateur de preuve zkSync.

- `/docker` : Fichiers docker du projet.

- `/bin` & `/infrastructure` : Scripts d'infrastructure qui aident à travailler avec les applications zkSync.

- `/etc` : Fichiers de configuration.

  - `/env` : Fichiers `.env` contenant les variables d'environnement pour différentes configurations du serveur / preuveur zkSync.

- `/keys` : Clés de vérification pour le module `circuit`.

- `/sdk` : Implémentation des bibliothèques clientes pour le réseau zkSync dans différentes langues de programmation.
  - `/zksync-rs` : Bibliothèque cliente Rust pour zkSync."
