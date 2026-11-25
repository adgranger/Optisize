# Benchmark du scan

Protocole, commandes, interprétation et résultats de référence de la mesure du scan d'Optisize
(carte F05). Tout est en **lecture seule** : le script lance des exécutables existants et lit des
dossiers ; il n'écrit que dans `target\bench` et dans son arbre synthétique (dossier neuf sous
`%TEMP%`, supprimé à la fin). Aucune installation, aucun téléchargement, aucune élévation, aucun
réglage Windows, aucun vidage de cache.

## Commandes

```powershell
cargo build --release
pwsh bench\bench.ps1                       # Optisize + TreeSize/WizTree s'ils sont présents, 7 tours, cache chaud
pwsh bench\bench.ps1 -SelfTest             # test du script (petit arbre, 1 tour) : code 0 attendu
pwsh bench\bench.ps1 -OptisizeOnly -Threads 4,8,16,20,32,48      # balayage du nombre de workers
# A/B : copier le binaire de base AVANT la modification, puis
Copy-Item target\release\optisize.exe $env:TEMP\base.exe
pwsh bench\bench.ps1 -OptisizeOnly -Optisize "$env:TEMP\base.exe=base","target\release\optisize.exe=new"
```

Depuis une console PowerShell 7 déjà ouverte, préférer `& .\bench\bench.ps1 …` (les listes
`-Optisize a,b` passent mal par `pwsh -File`).

Paramètres : `-Optisize <exe[=étiquette]>[]`, `-Paths <dossiers>[]` (défaut `C:\Program Files`,
`C:\Windows` + l'arbre synthétique), `-Rounds 7`, `-Warmup 1`, `-Threads <n>[]`
(variable `OPTISIZE_THREADS`), `-TreeSize <exe>`, `-WizTree <exe>`, `-OptisizeOnly` (pas d'outil
tiers), `-Cold` + `-Order <étiquettes>`, `-NoSynth`, `-KeepSynth`, `-SelfTest`, `-Out target\bench`.

Sortie directe d'Optisize :

```powershell
optisize --bench <dossier> [--json] [--profile]
```

- sans option : ligne humaine, suivie de `, n dossiers ignorés (accès refusé)` s'il y en a ;
- `--json` : une seule ligne JSON (`version, path, files, dirs, logical, on_disk, secs, errors,
  root_denied, mem, threads`), `--profile` y ajoute `phases` ;
- racine illisible : message « Accès refusé à la racine … », code de sortie **3** ; option
  inconnue : code **2**.

L'exécutable est une application « fenêtre » : PowerShell ne l'attend pas quand on capture sa
sortie (`$x = optisize --bench …` ne marche pas). Le script passe par `System.Diagnostics.Process`
avec sortie redirigée ; à la main, `optisize --bench C:\Windows --json | Out-Host` dans une console
affiche bien la ligne, et `Start-Process -Wait -RedirectStandardOutput f.json` la capture.

`OPTISIZE_THREADS` (1 à 64) fixe le nombre de workers : réglage de mesure, pas une option
d'interface. Hors de cette plage ou absent : `2 × cœurs logiques`, borné à 4..32.

## Protocole

**Cache chaud** (mesure par défaut, celle des budgets) :

1. pour chaque dossier, `Warmup` exécutions de chaque outil, jetées ;
2. `Rounds` tours ; à chaque tour chaque outil (et chaque valeur de `-Threads`) passe une fois, dans
   un ordre qui tourne d'un cran à chaque tour (A B C, puis B C A…) pour répartir les dérives
   (turbo, thermique, tâches de fond) ;
3. une ligne CSV par exécution dans `target\bench\<aaaammjj-hhmmss>-<machine>.csv` (UTF-8),
   précédée d'une ligne `# cpu=… ; ram=… ; os=… ; alimentation=…` ;
4. résumé par (étiquette, threads, dossier) : médiane, min, max, écart interquartile en %, mémoire
   « arbre », et rapport des médianes à la première étiquette (`÷ base`).

Mesure **invalide** (signalée, exclue des médianes) : code de sortie ≠ 0, racine refusée, ou nombre
de fichiers différent de la valeur la plus fréquente du dossier (arbre modifié pendant la mesure).

**Arbre synthétique** (seule référence stable d'une machine à l'autre) :
`%TEMP%\optisize-bench-synth-v1-<pid>`, 40 × 10 × 10 dossiers (4 440 dossiers), 30 fichiers par
dossier feuille (120 000 fichiers) ; le fichier `j` du dossier feuille `k` s'appelle `fjj.dat` et
contient `(k × 31 + j × 17) mod 4096` octets nuls (≈ 245 Mo). Contrôle `files = 120000`,
`dirs = 4440` avant toute mesure ; supprimé en fin de script (sauf `-KeepSynth`).

**Cache froid** (`-Cold`, piloté par l'utilisateur, étape manuelle) : le script ne vide aucun
cache. Redémarrez la machine, attendez 2 minutes, puis lancez
`pwsh bench\bench.ps1 -Cold -Order optisize,treesize` : une exécution par (outil, dossier), sans
chauffe, dans l'ordre `-Order`. L'outil mesuré en premier a l'avantage du disque froid (le script
l'affiche). Une mesure froide = un redémarrage : c'est une indication, pas une base de budget.
L'arbre synthétique vient d'être écrit, il est donc toujours chaud.

Conditions : machine sur secteur, applications lourdes fermées. Le script ne règle rien dans
Windows ; il note le plan d'alimentation actif dans le CSV.

## Outils tiers

Rien n'est installé ni téléchargé. Un outil absent donne une ligne `TreeSize : absent (voir
docs/bench.md)` et la mesure continue sans lui. Un outil **déjà ouvert** par l'utilisateur n'est
pas mesuré (et jamais fermé) : le script le signale.

- **TreeSize Free** : pas d'export en ligne de commande. Méthode `cpu-idle` : lancement avec le
  dossier en argument, temps CPU du processus (et de ses enfants) lu toutes les 50 ms, fin du scan
  = dernier instant où il a augmenté, une fois stable 3 s ; puis fermeture de **ce seul PID**
  (`CloseMainWindow`, puis `Stop-Process` après 5 s). Cette méthode compte le démarrage de
  l'interface : Optisize est donc aussi mesuré en interface (étiquette `optisize-gui`, même méthode).
  Si l'outil affiche une boîte de dialogue au lancement, la mesure est invalide : réglez l'outil
  vous-même et relancez.
- **WizTree** : méthode `export` (`WizTree64.exe "<dossier>" /export="<csv>" /admin=0`, temps mur
  jusqu'à la fin du processus ; `/admin=0` = mode non élevé, énumération classique, à armes égales).
  Options **non vérifiées** : WizTree est absent de la machine de dev. À valider avec la page
  « Command Line » de l'éditeur avant de se fier aux chiffres.
- Les modes élevés (WizTree par la MFT, TreeSize élevé) ne sont pas mesurés : ils relèvent de F30.

**Étape utilisateur pour WizTree** : « Pour inclure WizTree, téléchargez vous-même la version
portable depuis le site officiel de l'éditeur, décompressez-la dans
`%LOCALAPPDATA%\Programs\WizTree\` et relancez `bench\bench.ps1` (ou passez `-WizTree <chemin>`).
Le script ne télécharge rien. »

**Étape utilisateur pour TreeSize Free** (s'il est absent) : « Pour inclure TreeSize Free,
téléchargez-le vous-même depuis le site officiel de l'éditeur, installez-le (installation par
utilisateur) et relancez `bench\bench.ps1` (ou passez `-TreeSize <chemin>`). Le script ne
télécharge rien. »

## Profilage

1. **Phases intégrées** (`--profile`), toujours mesurées (≈ 25 ns par mesure, ≈ 15 µs par
   dossier) : somme sur les workers du temps d'**ouverture** (`CreateFileW`), de **lecture**
   (`GetFileInformationByHandleEx`), de **décodage** (appels WOF compris) et d'**envoi** (lot +
   mise en file), nombre d'appels WOF et de dossiers lus ; côté consommateur (thread de `--bench`),
   temps dans `Tree::ingest` et bloqué dans `recv`.

   Interprétation :
   - `attente` ≈ durée totale → le consommateur n'est pas limitant ;
   - `ingestion` ≈ durée totale → le consommateur est limitant ;
   - somme des phases workers ≪ `threads × durée` → workers inactifs (file vide, mauvaise
     répartition) ;
   - `ouverture` ≫ nombre de dossiers × coût mono-thread → contention noyau à l'ouverture.

2. **WPR + WPA** (étape utilisateur, exige l'élévation) : dans un terminal **administrateur**,
   `wpr -start CPU -filemode`, puis `target\release\optisize.exe --bench C:\Windows`, puis
   `wpr -stop "$env:TEMP\optisize-f05.etl"`. Ouvrir le fichier avec `wpa.exe`, ou en extraire un
   résumé avec `xperf -i <etl> -o <txt> -a profile` (symboles : `target\release\optisize.pdb`).
   Utile seulement pour décomposer le temps noyau de l'ouverture (pile, minifiltres comme
   l'antivirus). Non fait pendant F05.

3. Écartés : `cargo flamegraph`, `samply` (installation ; ETW donc élévation), Superluminal
   (commercial), `windows-perf` (ARM64), profileur Visual Studio (non pilotable sans interface).

## Résultats de référence

Machine de dev : i7-13700H (20 threads logiques), 63,8 Go, SSD NVMe, Windows 11 Famille 26300,
sur secteur, plan « Utilisation normale », non élevé, cache chaud. TreeSize Free 4.8.1.609,
WizTree absent. Base = `main` avant F05 (`47fd6e7`). Fichiers CSV dans `target\bench` (non versionnés).

### Avant / après F05 (A/B final, 7 tours entrelacés, 1 chauffe)

| Dossier | Fichiers | Dossiers | Base (médiane) | F05 (médiane) | Rapport | Arbre (base = F05) |
|---|---|---|---|---|---|---|
| `C:\Program Files` | 166 341 | 23 773 | 0,340 s | 0,278 s | **0,818** | 22,0 Mo |
| `C:\Windows` | 244 168 | 216 278 | 4,477 s | 3,514 s | **0,785** | 83,6 Mo |
| arbre synthétique | 120 000 | 4 440 | 0,071 s | 0,057 s | **0,803** | 9,7 Mo |

Budget de la carte : `C:\Windows` ≤ 0,80 × (atteint, de justesse), les deux autres ≤ 1,03 ×
(objectif 0,95 × dépassé), mémoire inchangée. Comptes identiques à la base sur les trois dossiers ;
tailles logique et sur disque exactes identiques (octet près) entre le binaire instrumenté (chemin
chaud de la base) et le binaire final sur `C:\Program Files` et `C:\Windows`, mesurés à la suite.

Bruit observé : la même série de mesures varie de 10 à 25 % d'une heure à l'autre sur
`C:\Windows` (3,3 à 4,5 s pour le même binaire) ; seuls les rapports A/B d'une même série font foi.

### Phases (`--profile`, `C:\Windows`)

| Binaire | Durée | Ouverture | Lecture | Décodage | Envoi | Ingestion | Attente |
|---|---|---|---|---|---|---|---|
| avant P1/P4 (32 workers) | 4,27 s | 119,7 s (88 %) | 10,8 s (8 %) | 0,26 s | 0,51 s | 0,16 s | 4,05 s |
| F05 (8 workers) | 3,32 s | 14,0 s (53 %) | 8,4 s (32 %) | 0,20 s | 0,49 s | 0,13 s | 3,13 s |

(pourcentages du temps total des workers, `threads × durée`.) Lecture : le consommateur n'est
jamais limitant (attente ≈ durée) ; à 32 workers, chaque ouverture de dossier coûte ≈ 550 µs
cumulés pour ≈ 19 µs de temps mur : les ouvertures se **sérialisent dans le noyau** (le débit
d'ouvertures plafonne quel que soit le nombre de threads, 4 workers font presque aussi bien que
32). Ajouter des threads ne fait qu'allonger la file d'attente noyau. Avec 8 workers, ouverture +
lecture = 85 % du temps des workers : le reste du gain passe par la lecture de la MFT (F30).

### Pistes

| Piste | Décision | Program Files | Windows | Synth | Remarque |
|---|---|---|---|---|---|
| Instrumentation (phases toujours actives) | gardée | 1,056 (IQR 18 %) | 0,989 | 0,945 | dans le bruit, ≤ 2 % hors bruit |
| P1 – workers : 8 au lieu de 32 | **gardée** | 0,816 | 0,952 | 0,767 | balayage 5 tours : 4 / 8 / 16 / 20 / 32 / 48 → 8 gagne partout ; défaut = `cœurs.clamp(4, 8)` |
| P2 – travail du consommateur | non essayée | | | | disculpée : ingestion 4 % de la durée |
| P3 – allocations par entrée | non essayée | | | | disculpée : décodage + envoi ≈ 3 % du temps workers |
| P4 – le worker garde un sous-dossier (profondeur) | **gardée** | 1,100 puis 1,009 (11 tours) | 0,810 | 0,988 | 1re série bruitée (IQR 12 %), remesurée seule |
| P5 – tampon 256 Kio | rejetée | 0,947 | 0,995 | 1,074 | perd 7 % sur l'arbre synthétique |
| P6 – `NtOpenFile` relatif au parent | rejetée | 1,053 | 0,960 | 0,984 | < 10 % sur `C:\Windows` ; ouverture cumulée inchangée (≈ 14,5 s) ; ≤ 1 020 handles |

Rapports = médiane de la piste ÷ médiane du binaire d'avant la piste (7 tours sauf mention).

### Outils tiers (méthode `cpu-idle`, informatif)

| Dossier | TreeSize Free 4.8.1 | `optisize-gui` | `--bench` F05 |
|---|---|---|---|
| `C:\Program Files` | 3,34 s (IQR 37 %) | 0,42 s (IQR 19 %) | 0,28 s |
| `C:\Windows` | 14,74 s (IQR 23 %) | 3,38 s (IQR 17 %) | 3,51 s |
| arbre synthétique | 14,59 s (IQR 80 %) | 2,32 s (IQR 102 %) | 0,06 s |

Objectif « `optisize-gui` ≤ TreeSize Free » tenu sur les trois dossiers. La méthode `cpu-idle` est
**imprécise** (bien au-delà des ±0,1 s espérés) : une reprise d'activité CPU après la fin du scan
(rafraîchissement d'interface, tâches de fond de l'outil) repousse la fin mesurée, d'où les
maxima (Optisize : 3,4 s sur `C:\Program Files` pour une médiane de 0,42 s) et l'arbre
synthétique, trop petit pour cette méthode. La validation au chronomètre (étape de la carte)
n'a pas été faite : la session n'a pas d'écran. Étape utilisateur : lancer TreeSize Free sur
`C:\Program Files`, chronométrer jusqu'à la fin du scan affichée, comparer à la médiane ci-dessus.

Non mesurés : WizTree (absent, étape utilisateur ci-dessus), cache froid (étape utilisateur après
redémarrage), profil WPR (étape utilisateur en administrateur).

## Limite : `C:\Program Files\WindowsApps`

Sans élévation, la racine de `WindowsApps` est refusée à tout processus **non empaqueté** (ACL
Windows : la lecture est accordée aux applications empaquetées MSIX, pas aux utilisateurs).
Optisize, et les processus qu'il lance, ne sont pas empaquetés : `--bench` affiche « Accès refusé à
la racine … » (code 3), l'interface un toast « Accès refusé : … ». Les sous-dossiers de paquets
restent analysables un par un (droit « traverser » implicite). Ce n'est pas un défaut de
l'énumération : depuis un processus empaqueté, le même appel réussit.

Attention aux comparaisons « à la main » : le PowerShell 7 du Microsoft Store est empaqueté ; il
liste `WindowsApps` alors qu'Optisize ne le peut pas. Les processus qu'il lance ne sont, eux, pas
empaquetés (les mesures du script ne sont pas faussées).
