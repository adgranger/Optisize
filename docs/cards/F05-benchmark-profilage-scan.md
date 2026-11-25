---
id: F05
title: Benchmark reproductible contre TreeSize et WizTree, profilage et optimisation du scan
model: opus        # modèle qui implémente : sonnet | opus (voir la règle dans ROADMAP.md)
model_reason: Performance mesurée sur le chemin chaud du scan (threads, énumération Windows bas niveau, éventuellement ntdll), où une optimisation fausse se traduit par des comptes ou des tailles faux sans aucune erreur visible.
depends_on: []
---

## Contexte

`optisize --bench <dossier>` (`src/main.rs`) mesure un scan sans interface et affiche une ligne lisible par un humain. Il n'existe ni protocole écrit, ni script, ni sortie machine-lisible, ni comparaison avec TreeSize ou WizTree : chaque carte (F04) refait sa mesure à la main, avec un bruit de 10–20 % entre deux exécutions. On ne sait pas non plus **où** passe le temps.

### Mesures préalables (faites pour cette carte, machine de dev : i7-13700H 20 threads logiques, SSD NVMe, Windows 11 26300, binaire `main` après F04, cache chaud, non élevé)

| Dossier | Fichiers | Dossiers | `--bench` (3 exécutions) | Arbre |
|---|---|---|---|---|
| `C:\Program Files` | 166 341 | 23 773 | 0,34 / 0,37 / 0,38 s | 22,0 Mo |
| `C:\Windows` | 244 167 | 216 278 | 4,11 / 4,25 / 4,88 s | 83,6 Mo |
| profil utilisateur | 1 348 207 | 265 057 | **10,51** (1re) / 5,80 / 5,87 s | 149,8 Mo |

Constats :
- **Passage à l'échelle faible** : F04 a mesuré un parcours mono-thread de `C:\Program Files` à 0,95 s ; le scan à 32 workers (`available_parallelism * 2`, borné à 4..32) le fait en 0,36 s, soit **×2,6 seulement** sur 20 threads logiques. Premier suspect : une sérialisation (thread consommateur qui ingère, file unique FIFO, contention noyau à l'ouverture des dossiers), pas le coût d'un appel.
- `C:\Windows` coûte ≈ 19 µs mur par dossier contre ≈ 15 µs pour `C:\Program Files` : c'est un arbre de petits dossiers (WinSxS), le coût est dominé par l'**ouverture** de chaque dossier (`CreateFileW`), pas par le nombre de fichiers.
- Le 1er scan du profil utilisateur prend presque le double des suivants : l'effet cache froid / chaud est du même ordre que les gains recherchés. Le protocole doit donc séparer les deux et jeter une exécution de chauffe.
- Outils présents sur la machine de dev : **TreeSize Free 4.8.1** (installé par utilisateur, `%LOCALAPPDATA%\Programs\JAM Software\TreeSize Free\TreeSizeFree.exe`) ; **WizTree absent** ; `wpr.exe` (Windows) et le Windows Performance Toolkit (`wpa.exe`, `xperf.exe`) présents ; Visual Studio Community 2026 présent ; ni `cargo-flamegraph`, ni `samply`, ni `hyperfine`.

### Défaut « WindowsApps : 0 fichier » (diagnostiqué pour cette carte)

Symptôme rapporté : sans élévation, `optisize --bench "C:\Program Files\WindowsApps"` affiche `0 fichiers, 0 dossiers`, alors qu'un PowerShell non élevé liste 399 entrées et 67 549 fichiers.

Cause (vérifiée) : **ce n'est pas un défaut de l'énumération d'Optisize, c'est l'ACL du dossier, et le PowerShell de comparaison n'est pas un processus ordinaire.**
- Le PowerShell 7.7 de la machine est la version Microsoft Store : `$PSHOME` = `C:\Program Files\WindowsApps\Microsoft.PowerShell_7.7.0.0_x64__8wekyb3d8bbwe`. C'est un processus **empaqueté** (MSIX) ; l'ACL de `WindowsApps` accorde la lecture aux applications empaquetées, pas aux utilisateurs.
- Depuis un processus non empaqueté, la racine est refusée : `powershell.exe` 5.1 → « L'accès au chemin d'accès 'C:\Program Files\WindowsApps' est refusé » ; `cmd /c dir` → « Fichier introuvable » ; `icacls` → « Accès refusé ». Optisize est dans ce cas, et les processus qu'il lance aussi.
- Depuis le PowerShell empaqueté, l'appel exact du scan (`CreateFileW(\\?\C:\Program Files\WindowsApps, FILE_LIST_DIRECTORY, partage 7, OPEN_EXISTING, FILE_FLAG_BACKUP_SEMANTICS)` puis `GetFileInformationByHandleEx(FileIdBothDirectoryInfo)`) **réussit** : les droits demandés, la classe d'information et le chemin verbatim sont corrects. Les 399 entrées sont des dossiers ordinaires (attribut 0x10, pas de point d'analyse).
- Les sous-dossiers de paquets restent lisibles sans élévation (droit « traverser » implicite) : `optisize --bench "C:\Program Files\WindowsApps\4DF9E0F8.Netflix_7.0.8.0_neutral__mcm4njqhnhss8"` → 84 fichiers, 2 dossiers. Défaut donc présent avant F04 (même ACL avec `read_dir`).
- Ce qui **est** un défaut d'Optisize : l'échec est **silencieux** en `--bench` (`Scan::errors()` vaut 1 mais n'est pas affiché ; la barre d'état de l'interface affiche bien « 1 dossiers ignorés (accès refusé) », sans dire que c'est la racine).

Décision : **traité dans F05**, pas de ligne dédiée dans `ROADMAP.md`. Raisons : (1) la justesse du benchmark l'exige — un dossier de référence à racine refusée donne un « temps » de 0,02 s pour 0 fichier, qu'un script prendrait pour une mesure ; (2) la correction côté code se limite au signalement (quelques lignes dans `--bench` et un toast), il n'y a rien à corriger dans l'énumération ; (3) lister la racine sans élévation exigerait de passer par les API de paquets (liste des paquets installés puis scan de chaque `InstallLocation`), une fonctionnalité à part au bénéfice faible, et l'élévation relève de F30. La limite est documentée dans le `README.md`.

## Comportement attendu

### 1. `--bench` : erreurs, racine refusée, sortie machine-lisible, phases

Syntaxe : `optisize --bench <dossier> [--json] [--profile]` (options **après** le dossier, dans n'importe quel ordre ; toute autre option → `option inconnue : {x}` sur la sortie standard, code de sortie 2 via `std::process::exit`).

- **Ligne humaine** (sans `--json`) : la ligne actuelle, suivie de `, {n} dossiers ignorés (accès refusé)` si `errors > 0`.
- **Racine refusée** : `Scan` expose `pub fn root_denied(&self) -> bool` (nouvel `AtomicBool` de `Shared`, mis à `true` dans `scan_dir` quand `id == 0` et que la lecture a échoué, `entries.is_none()`). En `--bench`, si vrai : afficher `Accès refusé à la racine : {dossier} (dossier protégé ; élévation ou application empaquetée requise)`, ne pas afficher de débit, code de sortie **3**. En `--json`, la ligne JSON est quand même émise (`"root_denied":true`) avant de sortir avec le code 3.
- **`--json`** : une seule ligne JSON sur la sortie standard, rien d'autre, champs dans cet ordre :
  `{"version":"0.1.0","path":"C:\\Windows","files":244167,"dirs":216278,"logical":71081234567,"on_disk":56012345678,"secs":4.1103,"errors":12,"root_denied":false,"mem":87654321,"threads":32}`
  - `version` = `env!("CARGO_PKG_VERSION")` ; `path` = argument tel que reçu, échappé ; nombres entiers bruts (pas de séparateur) ; `secs` avec 4 décimales ; `mem` en octets, mesuré avant `end_scan()` comme aujourd'hui ; `threads` = nombre de workers réellement lancés (`Scan::threads()`).
  - Écrit à la main (pas de `serde`) : fonction pure `fn json_str(s: &str) -> String` qui échappe `\` → `\\`, `"` → `\"`, et les caractères < 0x20 en `\u00XX`.
  - Avec `--profile`, ajouter avant `}` : `,"phases":{"open_us":…,"query_us":…,"parse_us":…,"send_us":…,"wof_calls":…,"dirs_read":…,"ingest_us":…,"wait_us":…}`.
- **`--profile`** (phases, sans outil externe) : chaque worker cumule dans une structure locale `Phases` (des `u64`) le temps passé dans : l'ouverture du dossier (`CreateFileW`), les appels `GetFileInformationByHandleEx`, le décodage (`parse`, appels WOF compris), l'envoi du lot et la mise en file des sous-dossiers ; plus `wof_calls` (appels `GetCompressedFileSizeW`) et `dirs_read`. Mesure par `Instant::now()` autour de chaque bloc (≈ 25 ns, négligeable devant ≈ 15 µs par dossier) ; **toujours active**, cumulée en fin de worker dans `Shared` (un `Mutex<Phases>` ou des `AtomicU64` ajoutés une seule fois par worker, jamais par dossier). Le thread de `--bench` mesure `ingest_us` (temps dans `Tree::ingest`) et `wait_us` (temps bloqué dans `recv`). Sans `--json`, `--profile` ajoute une seconde ligne humaine :
  `phases (somme des workers) : ouverture {a} s · lecture {b} s · décodage {c} s · envoi {d} s · {w} appels WOF · consommateur : ingestion {i} s, attente {x} s`.
  Interprétation à écrire dans `docs/bench.md` : `wait` ≈ durée totale → consommateur non limitant ; `ingest` ≈ durée totale → consommateur limitant ; somme des phases workers ≪ `threads × durée` → workers inactifs (file vide, mauvaise répartition).

### 2. Nombre de workers réglable (pour la mesure)

Fonction pure `fn worker_count(env: Option<&str>, cpus: usize) -> usize` : si `env` se lit comme un entier dans `1..=64`, il est pris ; sinon la formule actuelle `(cpus * 2).clamp(4, 32)`. `Scan::start` l'appelle avec `std::env::var("OPTISIZE_THREADS").ok().as_deref()`. Documenté dans `docs/bench.md` comme réglage de mesure (pas d'option d'interface). Si la mesure montre une meilleure valeur par défaut, changer la formule (et le test).

### 3. Script de benchmark versionné : `bench/bench.ps1`

PowerShell 7 (`#Requires -Version 7`), aucun module externe, aucune installation, aucun téléchargement, aucun réglage Windows modifié, aucune élévation demandée, aucun vidage de cache. Il ne fait que **lancer** des exécutables existants et **lire** les dossiers de référence ; il n'écrit que dans le dossier de résultats et dans son arbre synthétique.

Paramètres :
- `-Optisize <chemin[=étiquette]>[]` (défaut : `target\release\optisize.exe=optisize`) : un ou plusieurs binaires Optisize (A/B : `-Optisize $env:TEMP\base.exe=base, target\release\optisize.exe=new`).
- `-Paths <dossiers>[]` (défaut : `C:\Program Files`, `C:\Windows`, et l'arbre synthétique).
- `-Rounds <n>` (défaut 7), `-Warmup <n>` (défaut 1).
- `-Threads <n>[]` (facultatif) : balaye `OPTISIZE_THREADS` (ex. `8,16,20,32`) pour chaque binaire Optisize.
- `-TreeSize <exe>` / `-WizTree <exe>` (facultatifs, voir §4 ; détection automatique sinon).
- `-Cold` : mode cache froid (voir protocole).
- `-NoSynth`, `-KeepSynth`, `-SelfTest` (voir plus bas).
- `-Out <dossier>` (défaut `target\bench`, ignoré par git).

**Arbre synthétique** (reproductible d'une machine à l'autre) : créé dans un dossier **neuf** `$env:TEMP\optisize-bench-synth-v1-<pid>` (`New-Item` qui échoue si le dossier existe), puis supprimé en fin de script (bloc `finally`, `Remove-Item -Recurse` sur ce seul dossier) sauf `-KeepSynth`. Contenu déterministe : 40 dossiers de 1er niveau × 10 de 2e niveau × 10 de 3e niveau (4 000 dossiers feuilles + 440 intermédiaires), 30 fichiers par dossier feuille = **120 000 fichiers** ; le fichier `j` du dossier feuille d'indice `k` s'appelle `f{j:D2}.dat` et contient `((k * 31 + j * 17) % 4096)` octets de zéros (≈ 245 Mo au total). Création par `[IO.File]::WriteAllBytes` avec un tableau partagé tranché (pas de `New-Item` par fichier, trop lent). Le script vérifie après création, avec `optisize --bench --json`, `files == 120000` et `dirs == 4440`, sinon il s'arrête.

**Protocole cache chaud** (mesure par défaut, celle des budgets) :
1. Pour chaque dossier : `Warmup` exécutions de chaque outil, jetées.
2. Puis `Rounds` tours ; à chaque tour, chaque outil (et chaque valeur de `-Threads`) est exécuté une fois, dans un ordre **qui tourne d'un cran à chaque tour** (tour 1 : A B C, tour 2 : B C A…) pour répartir les dérives (turbo, thermique, tâches de fond).
3. Une ligne CSV par exécution : `horodatage,machine,outil,etiquette,version,threads,cache,dossier,tour,secondes,fichiers,dossiers,logique,sur_disque,erreurs,memoire,methode` (`methode` = `bench` pour Optisize `--bench`, `export` ou `cpu-idle` pour les outils tiers, voir §4). Fichier `target\bench\<aaaammjj-hhmmss>-<machine>.csv`, encodage UTF-8.
4. Résumé affiché par (outil, threads, dossier) : médiane, min, max, écart interquartile en %, et pour un A/B le rapport des médianes `new / base`. **Mesure invalide** (signalée, exclue des médianes) : code de sortie ≠ 0, `root_denied`, ou nombre de fichiers différent d'une exécution à l'autre du même dossier (arbre modifié pendant la mesure).
5. En tête de fichier, une ligne de commentaire `# cpu=… ; ram=… ; os=… ; alimentation=…` (lecture seule : `Get-CimInstance Win32_Processor`, `Win32_OperatingSystem`, `powercfg /getactivescheme`), pour qu'un résultat soit rattachable à sa machine.

**Protocole cache froid** (`-Cold`, facultatif, piloté par l'utilisateur) : le script ne vide **aucun** cache. L'utilisateur redémarre la machine, attend 2 minutes, puis lance `bench.ps1 -Cold` : une seule exécution par (outil, dossier), sans chauffe, ordre des outils fixé par `-Order` (l'outil mesuré en premier a l'avantage du disque froid : le script affiche cet avertissement). Les lignes sont marquées `cache=froid`. Une mesure froide = un redémarrage ; c'est une indication, pas une base de budget.

**`-SelfTest`** : arbre synthétique réduit (2 × 2 × 2 dossiers, 3 fichiers par feuille), 1 tour, 0 chauffe, Optisize seul ; vérifie que le CSV a l'en-tête attendu et 1 ligne valide par dossier, que l'arbre a été supprimé, code de sortie 0. C'est le test du script.

### 4. TreeSize et WizTree

Rien n'est installé ni téléchargé par le script ni par la session d'implémentation. Un outil absent → une ligne de résumé `TreeSize : absent (voir docs/bench.md)` et la mesure continue sans lui.

- **Détection** (dans cet ordre, premier trouvé) : paramètre explicite ; TreeSize Free `$env:LOCALAPPDATA\Programs\JAM Software\TreeSize Free\TreeSizeFree.exe`, `$env:ProgramFiles\JAM Software\TreeSize Free\TreeSizeFree.exe` ; WizTree `$env:ProgramFiles\WizTree\WizTree64.exe`, `$env:LOCALAPPDATA\Programs\WizTree\WizTree64.exe`. Version : `(Get-Item $exe).VersionInfo.FileVersion` dans la colonne `version`.
- **WizTree** a une ligne de commande d'export qui termine le processus (`WizTree64.exe "<dossier>" /export="<fichier.csv>" /admin=0`). `/admin=0` force le mode **non élevé** (énumération classique, pas de lecture de la MFT) : c'est la comparaison à armes égales avec Optisize aujourd'hui. Méthode `export` : temps mur entre le lancement et la fin du processus (`Start-Process -PassThru -Wait`), export écrit dans `target\bench\tmp\` puis supprimé. Le nombre de lignes de l'export (moins l'en-tête) sert de contrôle du nombre d'entrées. **Avant de s'en servir**, l'implémenteur vérifie ces options dans l'aide de la version installée (page « Command Line » du site officiel, ou fenêtre `/?`) ; si elles diffèrent ou si le processus ne se termine pas seul, WizTree passe en méthode `cpu-idle` ci-dessous.
- **TreeSize Free** n'a pas d'export en ligne de commande (c'est une fonction de TreeSize Professional) : il accepte seulement un chemin à analyser au lancement. Méthode `cpu-idle`, utilisée pour tout outil graphique sans export :
  1. `Start-Process <exe> "<dossier>" -PassThru` ;
  2. toutes les 50 ms, lire `TotalProcessorTime` du processus (et de ses enfants s'il en lance) ; fin du scan = dernier instant où ce temps a augmenté, **une fois qu'il est resté stable 3 s** ; durée = fin − lancement ;
  3. fermer le processus lancé par le script (`CloseMainWindow()`, puis `Stop-Process` sur **ce seul PID** après 5 s) ; ne jamais toucher à une instance déjà ouverte par l'utilisateur (si l'outil est déjà en cours au démarrage du script, le signaler et l'ignorer).
  Cette méthode inclut le démarrage de l'interface : pour comparer à armes égales, le script mesure **aussi** Optisize en interface (`optisize.exe "<dossier>"`, même méthode, étiquette `optisize-gui`), à côté de `--bench`. Précision attendue ≈ ±0,1 s ; l'implémenteur la valide **une fois** au chronomètre sur `C:\Program Files` et note l'écart dans `docs/bench.md`. Si TreeSize affiche une boîte de dialogue au lancement (élévation, nouveautés), la mesure est invalide : le documenter, et l'utilisateur règle l'outil lui-même avant de relancer.
- **Étape utilisateur pour WizTree** (à écrire telle quelle dans `docs/bench.md`) : « Pour inclure WizTree, téléchargez vous-même la version portable depuis le site officiel de l'éditeur, décompressez-la dans `%LOCALAPPDATA%\Programs\WizTree\` et relancez `bench\bench.ps1` (ou passez `-WizTree <chemin>`). Le script ne télécharge rien. » Même formulation pour TreeSize Free s'il est absent.
- Les modes élevés (WizTree par la MFT, TreeSize élevé) ne sont **pas** mesurés : ils relèvent de F30. `docs/bench.md` le dit.

### 5. Profilage

Choix, sans dépendance ni installation :
1. **Phases intégrées** (`--profile`, §1) : outil principal. Elles répondent à la question qui compte d'abord (consommateur, workers inactifs, ouverture ou lecture).
2. **WPR + WPA** (déjà présents : `wpr.exe` est livré avec Windows, WPA avec le Windows Performance Toolkit installé) : seulement si les phases montrent un temps noyau dominant à décomposer (pile d'appels, minifiltres comme l'antivirus). `wpr` exige l'élévation : **c'est une étape utilisateur**, décrite dans `docs/bench.md` : dans un terminal administrateur, `wpr -start CPU -filemode`, puis `target\release\optisize.exe --bench C:\Windows`, puis `wpr -stop "$env:TEMP\optisize-f05.etl"` ; l'implémenteur ouvre ensuite le fichier avec `wpa.exe` ou en extrait un résumé (`xperf -i <etl> -o <txt> -a profile`, symboles de `target\release\optisize.pdb`). La session d'implémentation ne lance pas `wpr` elle-même et n'a pas besoin du résultat pour finir.
3. Écartés : `cargo flamegraph` et `samply` (installation d'un outil ; `flamegraph` passe de toute façon par ETW, donc par l'élévation, sous Windows) ; Superluminal (logiciel commercial à installer) ; `windows-perf` (compteurs matériels sur Windows ARM64, machine x64) ; profileur de Visual Studio (possible pour l'utilisateur, mais non pilotable depuis une session sans interface).

### 6. Pistes d'optimisation (à mesurer une par une, garder seulement ce qui gagne)

Mesure de chaque piste : binaire de base copié **avant** la piste, A/B par `bench.ps1` (7 tours entrelacés, cache chaud) sur les trois dossiers de référence. Une piste est **gardée** si elle gagne au moins **3 %** de médiane sur au moins un dossier sans en perdre plus de 2 % sur un autre ni augmenter la mémoire « arbre » de plus de 2 % ; sinon elle est retirée du code et notée « essayée, rejetée, chiffres » dans `docs/bench.md`. Ordre conseillé (dicté par les phases ; sauter ce que les phases disculpent) :

- **P1 – Nombre de workers** : balayer `-Threads 4,8,16,20,32,48`. La formule par défaut change si une valeur gagne sur les trois dossiers.
- **P2 – Consommateur** : si `ingest` approche la durée totale, réduire le travail par entrée dans `Tree::ingest` (ex. `Kind::from_name` calculé dans le worker et transporté dans `Entry`), sans changer l'arbre produit.
- **P3 – Allocations par entrée** : aujourd'hui une `String` par nom (`from_utf16_lossy`) et un `PathBuf` par sous-dossier (`path.join`), réencodé en UTF-16 par `verbatim` à l'ouverture. Pistes : un seul tampon de noms par lot (`Batch { names: String, entries }` avec décalages) ; transporter dans la file le chemin déjà en UTF-16 verbatim (`Vec<u16>`) pour éviter `join` + réencodage. Doit garder `scan_long_path` et les noms non ASCII intacts.
- **P4 – Ordonnancement** : le worker garde pour lui un sous-dossier (le dernier) au lieu de tout remettre dans la file FIFO commune (parcours plus en profondeur, moins de trafic sur le canal, meilleure localité des métadonnées NTFS). **Invariant à garder** : le lot du parent est envoyé avant que ses sous-dossiers soient traités. Le compteur `pending` doit rester exact (test de fin de scan).
- **P5 – Tampon d'énumération** : 64 Kio → 128 et 256 Kio (moins d'appels sur les gros dossiers). Coût mémoire : un tampon par worker.
- **P6 – Ouverture relative au parent** (seulement si les phases montrent `open` dominant après P1–P4) : `NtOpenFile` / `NtCreateFile` (ntdll, `OBJECT_ATTRIBUTES.RootDirectory` = handle du dossier parent, nom relatif) évite au noyau de réanalyser tout le chemin à chaque dossier. Impose de garder le handle du parent ouvert jusqu'à l'ouverture de ses enfants (durée de vie des handles, nombre de handles ouverts borné) : ne la faire que si le gain mesuré dépasse 10 % sur `C:\Windows`, sinon la rejeter. Repli sur l'ouverture par chemin complet si l'appel échoue, pour rester juste.

Interdit pour gagner du temps : exclusions de l'antivirus, changement de plan d'alimentation, désactivation de l'indexation ou tout autre réglage Windows ; sauter des dossiers ; perdre l'exactitude des tailles (F04).

### 7. Budget de réussite (machine de dev, cache chaud, non élevé, médianes de 7 tours entrelacés, base = `main` avant F05)

- `C:\Windows` : médiane ≤ **0,80 ×** la base (au moins 20 % plus rapide).
- `C:\Program Files` et arbre synthétique : médiane ≤ **1,03 ×** la base (aucune régression), objectif ≤ 0,95 ×.
- Mémoire « arbre » ≤ base + 5 % sur les trois dossiers.
- Comptes (fichiers, dossiers, taille logique, taille sur disque) **identiques** à la base sur les trois dossiers (à la même seconde, l'arbre ne bouge pas ; un écart de quelques fichiers sur `C:\Windows` dû à une activité système doit se retrouver aussi entre deux exécutions de la base, sinon c'est un bogue).
- Si le seuil de 0,80 × n'est pas atteint après P1–P6 : livraison acceptée **seulement** si `docs/bench.md` montre, chiffres des phases à l'appui, que plus de 85 % du temps des workers est passé dans `open` + `query` (appels noyau) avec le nombre de workers retenu, et conclut que le gain restant passe par la MFT (F30).
- Comparaison aux outils tiers : **informative**, non bloquante (méthodes différentes, outils parfois absents). Objectif affiché : `optisize-gui` ≤ TreeSize Free sur les trois dossiers ; écrire le rapport mesuré dans `docs/bench.md` même s'il est défavorable.

### 8. Interface : racine refusée

À `Msg::Done`, si `scan.root_denied()` : toast `Accès refusé : {dossier}` (texte du chemin de la racine). La barre d'état garde son message actuel. Rien d'autre ne change dans l'interface.

### 9. Documentation : `docs/bench.md` (nouveau)

Contient : le protocole (résumé de §3), les commandes exactes (`cargo build --release`, `pwsh bench\bench.ps1`, A/B), l'interprétation des phases, les étapes utilisateur (WizTree / TreeSize à installer soi-même, cache froid après redémarrage, WPR en administrateur), le tableau de référence (machine, versions des outils, médianes avant/après F05 par dossier, mémoire), le tableau des pistes (gardée / rejetée, gain en %), et la limite `WindowsApps` avec sa cause.

## Hors périmètre

- Lister `C:\Program Files\WindowsApps` sans élévation (API de paquets) ; élévation, lecture de la MFT (F30).
- Mesurer les modes élevés de WizTree ou TreeSize.
- Installer, télécharger ou mettre à jour un outil ; modifier un réglage Windows ; vider un cache ; écrire en dehors de `target\bench` et de l'arbre synthétique.
- Nouvelle dépendance Rust (`serde`, `crossbeam-deque`, `windows`…) : tout se fait avec `std`, `crossbeam-channel` et des blocs `extern` locaux.
- Réduction mémoire des très gros arbres (F35), CI (F06), mode ligne de commande complet (F33) : `--json` reste propre à `--bench`.
- Changer les règles de taille de F04.

## Fichiers concernés

- `src/main.rs` : analyse des options de `--bench`, `--json`, `--profile`, ligne humaine avec erreurs, racine refusée (code 3), `json_str`, mesure `ingest_us` / `wait_us`. Si `bench` dépasse ~80 lignes, la déplacer dans `src/bench.rs` (pas plus de découpage).
- `src/scan/mod.rs` : `worker_count`, `Shared::{root_denied, phases}`, `Scan::{root_denied, threads, phases}`, struct `Phases` (pub, `Default`, `+=`), mesures dans `worker` / `scan_dir` ; pistes P1, P3, P4 retenues.
- `src/scan/win.rs` : mesure `open` / `query` / `parse` / `wof_calls` dans `read_dir` ; pistes P5, P6 retenues.
- `src/scan/tree.rs` : seulement si P2 ou P3 est retenue.
- `src/app.rs` : toast racine refusée.
- `bench/bench.ps1` (nouveau), `docs/bench.md` (nouveau).
- `README.md` : section « Lancer » (`--bench <dossier> [--json] [--profile]`, `pwsh bench\bench.ps1`, renvoi vers `docs/bench.md`) ; « Limites » : « `C:\Program Files\WindowsApps` : racine refusée sans élévation aux applications non empaquetées (ACL Windows) ; ses sous-dossiers restent analysables un par un ».
- `Cargo.toml`, `.gitignore` : inchangés (`target/` couvre `target\bench`).

## Risques

- **Comptes faux silencieux** (P3, P4, P6) : un sous-dossier oublié ou compté deux fois ne fait aucune erreur. Les tests `scan_matches_std_counts`, `scan_long_path`, `scan_hard_link_counted_once` et le contrôle « comptes identiques à la base » du budget sont obligatoires après chaque piste gardée.
- **Fin de scan** : P4 change qui décrémente `pending` ; une erreur bloque le scan (jamais de `Done`) ou le termine trop tôt. Test dédié ci-dessous ; `--bench` sur les trois dossiers doit se terminer.
- **Handles** (P6) : fuite ou épuisement si les handles parents restent ouverts sans borne ; garder la garde `Handle` (`Drop`) et mesurer le nombre de handles du processus pendant un scan de `C:\Windows` (Gestionnaire des tâches ou `(Get-Process optisize).HandleCount` à intervalle ; rester sous 10 000).
- **Bruit de mesure** (10–20 % entre deux exécutions) : ne conclure que sur des médianes A/B entrelacées ; machine sur secteur, autres applications lourdes fermées (le noter dans le rapport, ne rien régler dans Windows).
- **Arbres de référence vivants** : `C:\Windows` change avec les mises à jour ; la base et le nouveau binaire doivent être mesurés dans la même série. L'arbre synthétique est la seule référence stable d'une machine à l'autre.
- **Outils tiers** : méthode `cpu-idle` approximative ; une instance déjà ouverte par l'utilisateur ne doit jamais être fermée par le script.
- **Processus empaquetés** : mesurer ou comparer depuis le PowerShell du Store ne change rien pour les processus lancés (ils ne sont pas empaquetés : vérifié, `optisize --bench` lancé depuis ce PowerShell voit la racine de `WindowsApps` refusée), mais une comparaison « à la main » faite **dans** ce PowerShell voit plus de dossiers qu'Optisize. `docs/bench.md` le signale.

## Critères de fin

- `cargo build --release`, `cargo clippy --all-targets` et `cargo test` passent sans avertissement.
- **Dossiers de test** : comme F04, chaque test de fichiers crée **son propre** dossier neuf sous `temp_dir()` (préfixe `optisize-f05-`), n'écrit que dedans et ne supprime que lui. Aucune élévation, aucun réglage Windows.
- Tests unitaires purs :
  - `worker_count_env` — (`None`, 20) → 32 ; (`None`, 1) → 4 ; (`Some("8")`, 20) → 8 ; (`Some("0")`, 20) → 32 ; (`Some("65")`, 20) → 32 ; (`Some("x")`, 20) → 32 ; (`Some("64")`, 2) → 64.
  - `json_str_escapes` — `C:\a"b` → `"C:\\a\"b"` ; tabulation → `\u0009` ; `é` inchangé.
  - `bench_args` — `[dossier]` ; `[dossier, --json]` ; `[dossier, --profile, --json]` ; `[dossier, --foo]` → erreur `option inconnue : --foo`. (Fonction pure `fn bench_args(args: &[String]) -> Result<BenchOpts, String>`.)
  - `bench_json_line` — `fn bench_json(...) -> String` sur des valeurs fixes : ligne exacte attendue (avec et sans phases), une seule ligne, analysable par `ConvertFrom-Json` (vérifié à la main une fois).
- Tests d'intégration (`cfg(windows)`, `src/scan/mod.rs`, helper `scan_tree` existant) :
  - `scan_reports_root_denied` — dossier neuf, un fichier dedans, puis `icacls <dossier> /deny *S-1-1-0:(RD)` (refus de lister à « Tout le monde », sur **ce** dossier créé par le test) → `root_denied()` vrai, `errors() >= 1`, racine sans enfant ; nettoyage : `icacls <dossier> /remove:d *S-1-1-0` **avant** la suppression (garde `Drop` qui fait les deux dans cet ordre). Si `icacls` échoue : `eprintln!("ignoré : …")` et retour.
  - `scan_root_ok_not_denied` — dossier lisible → `root_denied()` faux, `errors() == 0`.
  - `phases_are_counted` — 20 dossiers × 5 fichiers : `dirs_read == 21`, `open_us + query_us > 0`, `wof_calls == 0`.
  - `scan_terminates_many_threads_few_dirs` — `OPTISIZE_THREADS` non utilisé ; appeler une variante `Scan::start_with(root, threads)` (ou équivalent interne) avec 1, 2 et 48 workers sur un arbre de 3 niveaux : `Done` reçu en moins de 5 s, mêmes comptes qu'un parcours `std::fs` (couvre P1 et P4).
  - tests F04 existants inchangés et verts (en particulier `scan_matches_std_counts`, `scan_long_path`, `scan_hard_link_counted_once`, `scan_does_not_follow_junction`).
- Script : `pwsh bench\bench.ps1 -SelfTest` termine avec le code 0 et ne laisse aucun dossier `optisize-bench-synth-*` dans `%TEMP%`.
- **Mesures (obligatoires, à reporter dans le message de commit et dans `docs/bench.md`)** :
  1. Avant toute modification du code : `cargo build --release`, copier `target\release\optisize.exe` vers `$env:TEMP\optisize-f05-base.exe`.
  2. Implémenter §1–§3 et §8 sans toucher au chemin chaud, puis : `pwsh bench\bench.ps1 -Optisize "$env:TEMP\optisize-f05-base.exe=base","target\release\optisize.exe=instr"` → l'instrumentation coûte ≤ 2 % (sinon la rendre conditionnelle).
  3. `--bench C:\Windows --profile` et `-Threads 4,8,16,20,32,48` : consigner les phases et choisir les pistes.
  4. Chaque piste : A/B contre le binaire d'avant la piste, décision gardée / rejetée selon §6.
  5. A/B final `base` contre le binaire final, 7 tours, trois dossiers : budget §7.
  6. TreeSize Free (présent) mesuré par `cpu-idle` contre `optisize-gui` ; WizTree « absent » si l'utilisateur ne l'a pas installé (le noter).
  7. Supprimer ensuite le seul fichier `$env:TEMP\optisize-f05-base.exe` et les autres binaires de pistes copiés par la session.
- **Vérification manuelle** (lecture seule, jamais Corbeille / Nettoyer) :
  1. `optisize --bench "C:\Program Files\WindowsApps"` (non élevé) → message « Accès refusé à la racine… », code de sortie 3 (`$LASTEXITCODE`).
  2. Même dossier dans l'interface → toast « Accès refusé : C:\Program Files\WindowsApps », barre d'état « 1 dossiers ignorés (accès refusé) ».
  3. `optisize --bench C:\Windows --json | ConvertFrom-Json` → objet avec tous les champs ; `--profile` ajoute `phases`.
  4. `pwsh bench\bench.ps1` complet : CSV dans `target\bench`, résumé lisible, arbre synthétique supprimé.
- Lignes à mettre à jour : statut F05 → `done` dans `ROADMAP.md` ; `README.md` comme indiqué dans « Fichiers concernés ».
