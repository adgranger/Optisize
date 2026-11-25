---
id: F06
title: CI GitHub Actions (build, clippy, test)
model: sonnet        # modèle qui implémente : sonnet | opus (voir la règle dans ROADMAP.md)
model_reason: Un fichier YAML entièrement spécifié ici et une garde de trois lignes dans un test, sans logique de scan ni API Windows à écrire : de la CI bien bornée, cas Sonnet de la règle.
depends_on: []
---

## Contexte

Aucune intégration continue : la seule vérification est `cargo build --release`, `cargo clippy --all-targets` et `cargo test` lancés à la main par chaque session (64 tests, tous verts sur `main` au commit `3337e92`). Le dépôt a un remote `origin` (`git@github.com:adgranger/Optisize.git`) mais aucun dossier `.github/`. F37 (installeur) et F40 (captures de régression) dépendent de F06.

### État constaté pour cette carte (machine de dev, `main` à `3337e92`, rustc 1.100.0 du 2026-11-09)

- `cargo clippy --all-targets -- -D warnings` : **0 avertissement**, code 0.
- `cargo test` : **64 passed**, ≈ 4 s avec la compilation incrémentale chaude.
- `cargo fmt --check` : **échoue** (≈ 125 blocs de diff sur tous les fichiers de `src/`, lignes longues voulues, pas de `rustfmt.toml`). Le dépôt n'est **pas** formaté par rustfmt → **pas de `cargo fmt --check` en CI** (voir Hors périmètre).
- `pwsh bench\bench.ps1 -SelfTest -Out <dossier>` : `SelfTest : OK`, code 0, ≈ 2 s (exige `target\release\optisize.exe` ; arbre synthétique 2×2×2×3 sous `%TEMP%`, supprimé à la fin ; n'écrit que dans `-Out` et son arbre).
- Pas de `rust-toolchain.toml`, pas de `rust-version` dans `Cargo.toml`, édition 2021. Dépendances : `crossbeam-channel`, `eframe` 0.37 (glow), `rfd` 0.18. Aucune dépendance système hors MSVC (présent sur les runners Windows).
- `actionlint` **absent** de la machine ; `gh` présent et connecté.
- Versions des actions (relevées le 2026-11-25 par `gh api repos/<action>/releases/latest` puis `gh api repos/<action>/commits/<tag> --jq .sha`) :
  - `actions/checkout` **v7.0.1** = `3d3c42e5aac5ba805825da76410c181273ba90b1`
  - `Swatinem/rust-cache` **v2.9.3** = `d1527c8f67ef9af357290475d4af4b0fab8f12e0`
- Runners : `windows-latest` désigne aujourd'hui `windows-2025` (Windows Server 2025, image avec Rust/rustup, MSVC et PowerShell 7).

### Tests qui dépendent de la machine (tous dans `src/scan/mod.rs`, module `fs_tests`, et `src/app.rs`)

Aucun test ne lit `C:\Windows` ni `C:\Program Files` (ces chemins n'apparaissent que comme chaînes dans `src/bench.rs`, sans accès disque). Les tests sur fichiers réels créent un dossier neuf sous `std::env::temp_dir()` et le suppriment (`Drop`). Sur le runner, `%TEMP%` est sur `C:` (NTFS), en général sous la forme courte `C:\Users\RUNNER~1\AppData\Local\Temp` ; le compte `runneradmin` est administrateur, UAC désactivé.

| Test | Dépendance | Attendu sur le runner |
|---|---|---|
| `volume_info_of_temp`, `scan_plain_file_rounds_to_cluster`, `scan_hard_link_counted_once`, `scan_long_path`, `scan_matches_std_counts` | volume de `%TEMP%` | passe (NTFS, cluster 4 Kio) |
| `scan_tiny_file_zero_on_disk` | NTFS/ReFS, garde existante | passe (fichier résident MFT) |
| `scan_sparse_file` | `FSCTL_SET_SPARSE` | passe (NTFS) |
| `scan_compressed_file` | NTFS + `compact /c`, garde NTFS existante | passe |
| `scan_wof_file` | `compact /exe:xpress4k`, garde existante | passe ou « ignoré : compact /exe refusé » (accepté) |
| `scan_does_not_follow_junction` | `cmd /c mklink /J` | passe |
| `scan_reports_root_denied` | `icacls /deny *S-1-1-0:(RD)` sur un dossier du test, garde existante si `icacls` échoue | **risque** : le refus s'applique aux administrateurs, donc attendu vert, mais si le jeton du processus a `SeBackupPrivilege` **activé**, `FILE_FLAG_BACKUP_SEMANTICS` contourne l'ACL et `root_denied()` vaut `false` → test rouge alors que le scan est juste. Garde à ajouter (§ 3). |
| `scan_root_ok_not_denied`, `phases_are_counted`, `scan_terminates_many_threads_few_dirs` | dossier temporaire ; délai < 5 s | passe (runner plus lent mais ≪ 5 s) |
| `app::tests::scan_root_*` | `temp_dir()` existe, `C:` existe | passe (chemin court comparé à lui-même) |

Décision : **aucun `#[ignore]`**, aucune variable d'environnement « CI » qui changerait le comportement des tests. La seule modification de test est la garde du § 3, qui ne saute le test que si sa prémisse (le refus empêche ce processus de lister) est fausse ; en local la prémisse est vraie et le test s'exécute comme aujourd'hui.

## Comportement attendu

### 1. Fichier `.github/workflows/ci.yml` (nouveau, contenu exact)

```yaml
name: CI

on:
  push:
    branches: [main]
    paths-ignore: ['docs/**', '**.md']
  pull_request:
    paths-ignore: ['docs/**', '**.md']
  workflow_dispatch:

permissions:
  contents: read

concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: ${{ github.event_name == 'pull_request' }}

env:
  CARGO_TERM_COLOR: always
  RUST_TOOLCHAIN: 1.100.0

jobs:
  windows:
    name: Windows (build, clippy, test)
    runs-on: windows-2025
    timeout-minutes: 30
    steps:
      - name: Récupérer le code
        uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1
        with:
          persist-credentials: false

      - name: Toolchain Rust ${{ env.RUST_TOOLCHAIN }}
        run: |
          rustup toolchain install $env:RUST_TOOLCHAIN --profile minimal --component clippy --no-self-update
          rustup default $env:RUST_TOOLCHAIN
          rustc --version
          cargo clippy --version

      - name: Cache Cargo
        uses: Swatinem/rust-cache@d1527c8f67ef9af357290475d4af4b0fab8f12e0 # v2.9.3
        with:
          save-if: ${{ github.ref == 'refs/heads/main' }}

      - name: Clippy
        run: cargo clippy --all-targets --locked -- -D warnings

      - name: Tests
        run: cargo test --locked -- --show-output

      - name: Build release
        run: cargo build --release --locked

      - name: Bench (auto-test)
        run: pwsh -NoProfile -File bench/bench.ps1 -SelfTest
```

Choix, à ne pas changer sans raison écrite dans le commit :
- **Déclencheurs** : push sur `main`, toute pull request, lancement manuel. Les changements purement documentaires (`docs/**`, `*.md`, donc les cartes et `ROADMAP.md`) ne déclenchent rien. Pas de push sur les autres branches (doublon avec la PR).
- **Runner** `windows-2025` écrit en dur plutôt que `windows-latest` : une migration d'image devient un changement visible dans un commit, pas une CI rouge un matin. Un seul job, pas de matrice (application Windows uniquement).
- **Toolchain épinglée dans le workflow, pas de `rust-toolchain.toml`** : `1.100.0` = version locale actuelle, sur laquelle clippy est propre. Une nouvelle version stable n'introduit donc pas de lint inattendu en CI. Un `rust-toolchain.toml` forcerait aussi la version en local (téléchargement silencieux par rustup chez l'utilisateur) : refusé. Monter la version = changer `RUST_TOOLCHAIN` dans un commit dédié après `cargo clippy` propre en local. Shell par défaut des runners Windows = `pwsh`, d'où `$env:RUST_TOOLCHAIN`.
- **Cache** `Swatinem/rust-cache` (clé par défaut : version de rustc + hash de `Cargo.lock` + nom du job ; nettoie `target` des artefacts du crate lui-même). Sauvegardé seulement sur `main` (`save-if`) : les PR lisent le cache de `main` sans le polluer. Pas de `actions/cache` à la main.
- **Ordre** : clippy (le plus rapide à échouer) → tests → release → auto-test du bench, qui a besoin de `target\release\optisize.exe`. `--locked` partout : la CI échoue si `Cargo.lock` n'est pas à jour au lieu de le réécrire.
- **`--show-output`** : affiche dans le journal les `eprintln!("ignoré : …")` des tests gardés, pour voir lesquels ont sauté sur le runner.
- **`bench.ps1 -SelfTest` inclus** : ≈ 2 s, vérifie le script, le mode `--bench --json` du binaire release et le nettoyage de l'arbre synthétique. Il écrit dans `target\bench` du runner (sans conséquence). Le script mesure aussi TreeSize/WizTree s'ils existent : absents du runner, et `-SelfTest` force `-OptisizeOnly`.
- **Permissions** : `contents: read` au niveau du workflow, rien d'autre ; `persist-credentials: false` pour que le jeton ne reste pas dans `.git/config` du runner.
- **Actions épinglées par SHA complet**, tag en commentaire. Aucune autre action tierce (toolchain installée par `rustup`, déjà sur l'image).
- **Pas d'artefact** du binaire : la distribution est l'objet de F37 ; un artefact non signé, sans icône ni version, n'aurait pas d'usage.

### 2. Durée attendue (estimations, à confirmer au premier run)

- Premier run (cache vide) : ≈ 10–15 min (eframe compilé trois fois : métadonnées clippy, debug des tests, release avec `lto = "thin"`).
- Runs suivants sur `main` ou PR avec cache : ≈ 3–6 min (la release avec LTO thin reste la plus longue).
- `timeout-minutes: 30` laisse une marge de ×2 sur le premier run.

### 3. Garde de `scan_reports_root_denied` (`src/scan/mod.rs`)

Juste après `let d = Denied(d); if !ok { … }`, ajouter :

```rust
        if std::fs::read_dir(&d.0 .0).is_ok() {
            eprintln!("ignoré : le refus n'empêche pas ce processus de lister (privilège de sauvegarde actif ?)");
            return;
        }
```

Rien d'autre ne change dans le test (même `Drop`, mêmes assertions). Vérifier en local qu'il **s'exécute toujours** (pas de ligne « ignoré » dans `cargo test scan_reports_root_denied -- --show-output`).

### 4. `README.md`

- Section « Lancer » : remplacer `cargo test; cargo clippy --all-targets` par `cargo test; cargo clippy --all-targets -- -D warnings   # mêmes commandes que la CI (.github/workflows/ci.yml)`.
- Section « État » : ajouter à la phrase « Fonctionnel : … » : `CI GitHub Actions sous Windows Server 2025 (clippy sans avertissement, tests, build release, auto-test du bench) à chaque push sur main et chaque pull request`. Ajouter aux limites : `premier run de la CI à faire par l'utilisateur (rien n'est poussé par les sessions)` — à retirer par l'utilisateur ou une session ultérieure une fois le run vert.
- Pas de badge (le dépôt peut être privé ; un badge cassé n'apporte rien).

## Hors périmètre

- `cargo fmt --check` et le reformatage du dépôt : il échoue aujourd'hui sur tous les fichiers ; reformater toucherait tout `src/` et créerait des conflits avec les branches en cours. Si l'utilisateur le veut, ce sera une ligne de `ROADMAP.md` à part (formatage + `rustfmt.toml` + étape CI).
- Artefacts, releases, signature, cache de `sccache`, matrice de toolchains ou d'OS, `cargo audit`/`cargo deny`, Dependabot, protection de branche, badge.
- Pousser quoi que ce soit, créer une PR, lancer un workflow, créer un secret ou toute ressource GitHub : **l'utilisateur le fait lui-même**.
- Installer `actionlint` ou tout autre outil.
- Toute autre modification de test ou de code que la garde du § 3.

## Fichiers concernés

- `.github/workflows/ci.yml` : nouveau, contenu exact du § 1.
- `src/scan/mod.rs` : garde du § 3 dans `fs_tests::scan_reports_root_denied`, rien d'autre.
- `README.md` : § 4.
- `ROADMAP.md` : statut F06.

## Risques

- **Rust absent ou différent sur l'image** : l'étape toolchain installe la version exacte avec `rustup` ; si `rustup` lui-même manquait (retrait d'outil de l'image), l'étape échoue avec « rustup n'est pas reconnu » : ajouter alors une étape d'installation de rustup (pas une action tierce non épinglée). Non vérifiable sans run.
- **`scan_reports_root_denied`** : voir la garde du § 3. Si le test échoue malgré la garde (par exemple `read_dir` refusé mais `root_denied()` faux), c'est un vrai défaut du scan, à ne pas masquer.
- **`scan_wof_file`** peut sauter sur Windows Server (WOF) : accepté, visible grâce à `--show-output`.
- **Chemin court `RUNNER~1`** dans `%TEMP%` : le scan passe par `\\?\` ; NTFS résout les noms 8.3 même sans normalisation. Attendu sans effet ; à surveiller au premier run (`scan_long_path`, `scan_plain_file_rounds_to_cluster`).
- **Durées** : estimations, non mesurées sur runner.
- **SHA périmés** : les SHA ci-dessus sont ceux du 2026-11-25 ; l'implémenteur les revérifie (§ Critères) et, s'il prend une version plus récente, met SHA et commentaire à jour ensemble.
- **Données** : le workflow ne lit aucun secret et n'écrit que dans l'espace de travail et `%TEMP%` du runner jetable.

## Critères de fin

- En local, dans cet ordre, codes 0 : `cargo clippy --all-targets --locked -- -D warnings` (0 avertissement), `cargo test --locked -- --show-output` (64 tests, aucune ligne « ignoré » pour `scan_reports_root_denied` sur la machine de dev), `cargo build --release --locked`, `pwsh -NoProfile -File bench/bench.ps1 -SelfTest -Out <dossier neuf sous %TEMP%>` (`SelfTest : OK` ; dossier supprimé ensuite).
- Pas de nouveau test (la garde ne change pas le comportement local ; le workflow n'est pas testable en local).
- Validation du workflow **sans pousser** :
  - `Get-Command actionlint` : s'il existe, `actionlint .github/workflows/ci.yml` doit être silencieux ; sinon ne rien installer et faire la relecture ci-dessous.
  - Relecture : le fichier est identique au § 1 (indentation à deux espaces, pas de tabulation : `Select-String -Path .github/workflows/ci.yml -Pattern "\t"` ne renvoie rien) ; chaque `uses:` a un SHA de 40 caractères et un commentaire de tag ; `permissions` ne contient que `contents: read`.
  - SHA : `gh api repos/actions/checkout/commits/v7.0.1 --jq .sha` et `gh api repos/Swatinem/rust-cache/commits/v2.9.3 --jq .sha` (lecture seule) redonnent les SHA du fichier.
- Ce que **l'utilisateur** fera lui-même (à rappeler dans le rapport final de la session) : pousser la branche, ouvrir une PR (ou fusionner dans `main` puis pousser) ; regarder le premier run dans l'onglet Actions (`gh run watch` ou `gh run list --workflow CI`) ; vérifier dans le journal de « Tests » les lignes « ignoré » éventuelles et la durée ; si un test échoue sur le runner, ouvrir une session avec le journal plutôt que de le marquer `#[ignore]`.
- Vérification manuelle dans l'application : aucune (pas de changement d'interface).
- Lignes à mettre à jour : statut F06 dans `ROADMAP.md` (`done` quand le code est fusionné et vérifié en local ; le premier run vert reste une étape utilisateur, notée dans le `README.md`), sections « Lancer » et « État » du `README.md` (§ 4).
