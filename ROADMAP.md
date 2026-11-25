# Roadmap Optisize

Source de vérité du travail restant. Le skill `/next-task` (`.claude/skills/next-task/SKILL.md`) lit ce fichier pour choisir la tâche suivante.

## Cycle de vie d'une fonctionnalité

| Statut | Signification | Prochaine tâche | Modèle de la sous-session |
|---|---|---|---|
| `todo` | pas de carte écrite | **écrire la carte** (`docs/cards/<ID>-<slug>.md`, gabarit `docs/cards/TEMPLATE.md`) | **toujours Opus** |
| `ready` | carte écrite | **implémenter** la carte | le modèle de la colonne « Modèle dev » (la carte le confirme ou le corrige) |
| `doing` | implémentation en cours | reprendre ou terminer | idem |
| `done` | livré et vérifié (build, clippy, tests) | — | — |

Une carte écrite en Opus fixe le modèle d'implémentation dans son en-tête ; la colonne « Modèle dev » est la valeur initiale, la carte fait foi une fois écrite.

## Règle de choix du modèle de développement

- **Opus** : sécurité des données (suppression), formats persistants, API Windows bas niveau, performance mesurée, algorithmes non triviaux, tout ce dont une erreur est coûteuse ou silencieuse.
- **Sonnet** : câblage d'interface, menus, vues, export, configuration, CI, packaging, tâches bien bornées par la carte.

## Livré (scaffold initial)

Scan parallèle en arrière-plan avec Pause / Reprendre / Arrêter / Rafraîchir · arbre compact en arène · tableau virtualisé triable avec navigation clavier · treemap imbriqué (infobulle, clic synchronisé) · répartition par type · puces de lecteurs · filtre par nom · Ouvrir dans l'Explorateur · Copier le chemin · barre d'état · mode `--bench` · carte « Nettoyage suggéré » (détection seule).

## Fonctionnalités

Dépend de = identifiants qui doivent être `done` avant de commencer.

### Phase 0 — Stabilisation

| ID | Fonctionnalité | Dépend de | Modèle dev | Carte | Statut |
|---|---|---|---|---|---|
| F01 | Tester et corriger « Choisir un dossier… » (Ctrl+O) et le clic sur une puce de lecteur | — | sonnet | — | todo |
| F02 | Dates affichées en heure locale (pas en UTC) | — | sonnet | — | todo |
| F03 | Embarquer la police JetBrains Mono pour les tailles | — | sonnet | — | todo |
| F04 | Taille allouée (clusters) en plus de la taille logique, avec bascule ; liens physiques, fichiers creux, compressés | — | opus | — | todo |
| F05 | Benchmark reproductible contre TreeSize et WizTree, profilage et optimisation du scan | — | opus | — | todo |
| F06 | CI GitHub Actions (build, clippy, test) | — | sonnet | — | todo |

### Phase 1 — Vues et navigation

| ID | Fonctionnalité | Dépend de | Modèle dev | Carte | Statut |
|---|---|---|---|---|---|
| F07 | Bascule des vues Détails / Carte / Types (F6–F8) | F01 | sonnet | — | todo |
| F08 | Zoom du treemap : double-clic pour entrer dans un dossier, fil d'Ariane, retour | — | sonnet | — | todo |
| F09 | Vue « 100 plus gros fichiers » (F9) | — | sonnet | — | todo |
| F10 | Vue « Fichiers les plus anciens » / répartition par âge | F02 | sonnet | — | todo |
| F11 | Recherche avancée (Ctrl+F) : `*.mp4`, `>1Go`, dates, combinaisons | — | sonnet | — | todo |
| F12 | Sélection multiple et Tout sélectionner (Ctrl+A) | — | sonnet | — | todo |
| F13 | Menu contextuel (clic droit) sur l'arbre et le treemap | F12 | sonnet | — | todo |
| F14 | Fenêtre Propriétés (chemin, tailles, dates, attributs) | — | sonnet | — | todo |
| F15 | Dossiers récents et choix de lecteur depuis le menu Fichier | F01 | sonnet | — | todo |
| F16 | Unités Go / Gio et thème clair / sombre | — | sonnet | — | todo |
| F17 | Options persistantes (fichier de configuration, fenêtre Options, Ctrl+,) | F16 | sonnet | — | todo |
| F18 | Aide : raccourcis clavier (F1), documentation, À propos | — | sonnet | — | todo |

### Phase 2 — Actions sur les fichiers

| ID | Fonctionnalité | Dépend de | Modèle dev | Carte | Statut |
|---|---|---|---|---|---|
| F19 | Exclusions : motifs, jonctions et points d'analyse, dossiers système | F17 | sonnet | — | todo |
| F20 | Corbeille réelle avec confirmation, mise à jour de l'arbre sans rescanner | F12 | opus | — | todo |
| F21 | Suppression définitive avec garde-fous (chemins protégés, double confirmation) | F20 | opus | — | todo |
| F22 | Nettoyage : règles (temp, caches, mises à jour), aperçu, exécution, journal | F20 | opus | — | todo |
| F23 | Vue dédiée « Fichiers temporaires et caches » | F22 | sonnet | — | todo |

### Phase 3 — Export et persistance

| ID | Fonctionnalité | Dépend de | Modèle dev | Carte | Statut |
|---|---|---|---|---|---|
| F24 | Export CSV | — | sonnet | — | todo |
| F25 | Export Excel (.xlsx) | F24 | sonnet | — | todo |
| F26 | Export HTML | F24 | sonnet | — | todo |
| F27 | Export PDF et impression | F26 | sonnet | — | todo |
| F28 | Enregistrer / ouvrir un scan (`.optisize`, format binaire versionné) | — | opus | — | todo |
| F29 | Comparer deux scans (croissance, nouveaux et supprimés) | F28 | opus | — | todo |

### Phase 4 — Avancé

| ID | Fonctionnalité | Dépend de | Modèle dev | Carte | Statut |
|---|---|---|---|---|---|
| F30 | Scan NTFS via la table MFT (élévation administrateur, repli sur le scan classique) | F05 | opus | — | todo |
| F31 | Surveillance en direct des changements (USN ou `ReadDirectoryChangesW`) | F30 | opus | — | todo |
| F32 | Doublons : regroupement par taille, hash partiel, puis BLAKE3 | F12 | opus | — | todo |
| F33 | Mode ligne de commande : scan headless et rapports | F24 | sonnet | — | todo |
| F34 | Planification de scan (tâche planifiée Windows) | F33 | sonnet | — | todo |
| F35 | Très gros arbres (>10 M de nœuds) : dédoublonnage des noms, réduction mémoire | F05 | opus | — | todo |

### Phase 5 — Qualité et distribution

| ID | Fonctionnalité | Dépend de | Modèle dev | Carte | Statut |
|---|---|---|---|---|---|
| F36 | Icône et métadonnées de l'exécutable | — | sonnet | — | todo |
| F37 | Installeur et version portable, signature du binaire | F06, F36 | sonnet | — | todo |
| F38 | Internationalisation FR / EN | F17 | sonnet | — | todo |
| F39 | Accessibilité (clavier, contrastes, AccessKit) | — | sonnet | — | todo |
| F40 | Captures de régression de l'interface | F06 | sonnet | — | todo |
