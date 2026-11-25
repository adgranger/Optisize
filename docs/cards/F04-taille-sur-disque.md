---
id: F04
title: Taille sur disque (clusters) en plus de la taille logique, avec bascule ; liens physiques, fichiers creux, compressés
model: opus        # modèle qui implémente : sonnet | opus (voir la règle dans ROADMAP.md)
model_reason: Réécriture de l'énumération du scan sur une API Windows bas niveau (structure binaire lue à la main, chemins verbatim), règles de taille dont les erreurs sont silencieuses (WOF, liens physiques, fichiers résidents) et budget de débit à mesurer.
depends_on: []
---

## Contexte

Aujourd'hui toutes les tailles affichées sont **logiques** : `src/scan/mod.rs` (`scan_dir`) lit `std::fs::read_dir` puis `DirEntry::metadata().len()` (taille de fin de fichier, issue des données d'énumération, sans ouvrir le fichier). Un `// TODO: taille allouée (clusters)` y est posé, et le `README.md` (« État ») le liste en TODO.

Conséquences visibles : un fichier creux de 10 Go qui n'occupe que 64 Kio compte 10 Go ; un dossier compressé NTFS ou CompactOS compte sa taille décompressée ; `C:\Windows` compte chaque lien physique de WinSxS autant de fois qu'il a de noms (mesuré : 244 167 fichiers pour 172 241 identifiants de fichier distincts) ; mille fichiers de 100 octets comptent 100 Ko alors qu'ils tiennent dans la MFT. Le total ne correspond donc pas à l'espace qu'on libère en supprimant. TreeSize et l'Explorateur (« Taille sur disque ») affichent la taille allouée.

Chaîne actuelle des tailles : `Entry::size` (worker) → `Tree::ingest` → `Node::size` (fichier) et somme remontée jusqu'à la racine, `Tree::type_sizes` → lus directement (`n.size`, `tree.nodes[x].size`, `tree.type_sizes`) par l'arbre (`src/ui/tree_view.rs`), le treemap (`src/ui/treemap.rs`, `TreeSrc` + mode Types), le panneau Types (`src/ui/types.rs`), la carte Nettoyage (`App::compute_cleanup`) et `--bench` (`src/main.rs`).

## Mesures préalables (faites pour cette carte, machine de dev, Windows 11 26300)

Programme d'essai jetable (non versionné), fichiers créés dans des dossiers temporaires neufs, sur C: (NTFS, cluster 4 Kio) et D: (ReFS, cluster 4 Kio). « dir » = `GetFileInformationByHandleEx(FileIdBothDirectoryInfo)` sur le handle du dossier ; « std » = `FileStandardInfo` sur un handle ouvert par fichier ; « GCFS » = `GetCompressedFileSizeW`.

| Fichier (NTFS) | logique | dir.alloc | std.alloc | GCFS | liens |
|---|---|---|---|---|---|
| ordinaire 10 000 o | 10 000 | 12 288 | 12 288 | 10 000 | 1 |
| ordinaire 1 000 000 o | 1 000 000 | 1 003 520 | 1 003 520 | 1 000 000 | 1 |
| 100 o (résident MFT) | 100 | 104 | 104 | 100 | 1 |
| creux 10 Mio, 64 Kio écrits | 10 485 760 | 65 536 | 65 536 | 65 536 | 1 |
| LZNT1 (`compact /c`) 1 Mio | 1 048 576 | 65 536 | 65 536 | 65 536 | 1 |
| WOF (`compact /c /exe:xpress4k`) 1 Mio | 1 048 576 | **0** | 69 632 | 69 632 | 1 |
| deux noms d'un lien physique | 10 000 ×2 | 12 288 ×2, **même FileId** | 12 288 | 10 000 | 2 |
| jonction | — | 0, attributs 0x410, **EaSize = 0xA0000003** | | | |

Constats :
- L'énumération par handle de dossier donne **déjà** la taille allouée arrondie au cluster, exacte pour les fichiers ordinaires, creux et compressés NTFS, et l'identifiant de fichier 64 bits (identique pour tous les noms d'un lien physique, sur NTFS et ReFS). Seuls les fichiers **WOF** (CompactOS, `compact /exe`) y apparaissent avec une allocation **0** (le filtre WOF masque le flux `WofCompressedData` et l'attribut de point d'analyse) ; GCFS et `FileStandardInfo` donnent alors la bonne valeur.
- GCFS **n'arrondit pas** au cluster pour un fichier ordinaire (renvoie la taille logique) : inutilisable seul.
- Le nombre de liens n'est disponible que par fichier (handle, ou `GetFileInformationByName`, présent sur cette machine mais seulement depuis Windows 11 24H2).
- Coût, parcours mono-thread de `C:\Program Files` (166 341 fichiers, cache chaud) : std::fs 0,90–1,27 s ; énumération par handle de dossier 0,95–0,98 s (**même coût**, c'est le même appel noyau que `FindFirstFileExW`) ; avec une requête par fichier : `GetFileInformationByName` 7,1 s, GCFS 8,0 s, handle + `FileStandardInfo` 7,9 s (≈ 40 µs/fichier, **7 à 8 fois plus lent**). Ensemble de 172 241 identifiants sur `C:\Windows` : aucun surcoût mesurable.
- Délais d'écriture : sur NTFS, la compression d'un fichier écrit *après* `FSCTL_SET_COMPRESSION` n'apparaît qu'après quelques secondes ; `compact /c` sur un fichier fermé est immédiat. Sur ReFS, l'allocation d'un fichier creux tout juste écrit était périmée (4 096) à la première lecture, juste ensuite. Les tests doivent donc utiliser `compact /c` sur un fichier fermé et des bornes larges.
- Référence `--bench` actuelle (binaire `main`, F03) : `C:\Program Files` 166 341 fichiers, 0,34–0,40 s, arbre 18,0 Mo ; `C:\Windows` 244 167 fichiers / 216 278 dossiers, 4,2–5,1 s, arbre 74,7 Mo. Le bruit entre deux exécutions atteint 10–20 % : d'où le protocole A/B entrelacé plus bas.

## Comportement attendu

### Définitions
- **Taille logique** (inchangée) : somme des tailles de fin de fichier, **chaque nom compté** (un fichier à deux liens physiques compte deux fois, comme la somme des tailles de l'Explorateur).
- **Taille sur disque** : somme des tailles allouées, chaque fichier physique (identifiant de fichier) compté **une seule fois** par scan. C'est l'espace effectivement occupé par les données des fichiers ; métadonnées des dossiers, flux de données alternatifs et `$MFT` exclus (comme TreeSize par défaut).
- Les dossiers n'ont pas de taille propre dans les deux modes (comme aujourd'hui) : leur taille est la somme de leur contenu.

### Taille allouée d'un fichier (worker)
Règle appliquée dans cet ordre par une fonction pure `fn on_disk(eof: u64, alloc: u64, attr: u32, cluster: u64) -> Option<u64>` (`None` = il faut interroger le fichier) :
1. **WOF / allocation masquée** : `eof > 0 && alloc == 0 && attr & (SPARSE 0x200 | REPARSE_POINT 0x400 | OFFLINE 0x1000 | RECALL_ON_OPEN 0x40000 | RECALL_ON_DATA_ACCESS 0x400000) == 0` → `None`. Le worker appelle alors `GetCompressedFileSizeW(chemin)` et arrondit le résultat au cluster supérieur (`round_up(n, cluster)`, identité si `cluster == 0`). Si l'appel échoue (`0xFFFF_FFFF` pour la partie basse et `GetLastError() != 0`), repli `round_up(eof, cluster)`. Ce cas ne concerne que les fichiers dont l'énumération ne dit rien (WOF, ou allocation pas encore écrite sur ReFS) : coût ≈ 40 µs chacun, réparti sur les workers ; mesuré à 1 fichier sur 244 167 dans `C:\Windows` sur une machine sans CompactOS.
2. **Fichier résident (données dans l'enregistrement MFT, ou intégrées sur ReFS)** : `cluster > 0 && alloc < cluster` → `Some(0)`. Couvre aussi `alloc == 0` (fichier vide, creux entièrement vide, espace réservé OneDrive non téléchargé `RECALL_ON_DATA_ACCESS` : 0 octet sur disque, c'est exact). Comme l'Explorateur, qui affiche 0 octet sur disque pour ces fichiers.
3. Sinon `Some(alloc)` tel que rapporté par le système de fichiers (déjà en multiples de cluster sur NTFS, ReFS, FAT32, exFAT ; valeur du serveur sur un partage). **Pas de réarrondi**.

Conséquences : creux → taille des plages écrites ; compressé NTFS (attribut 0x800) → taille compressée ; WOF → taille compressée via GCFS.

### Liens physiques (comptés une fois)
- Le worker remplit `Entry::file_id` avec l'identifiant 64 bits de l'énumération, **ou 0** si le système de fichiers du volume n'est ni `NTFS` ni `ReFS`, si l'identifiant vaut 0 ou `u64::MAX`, ou si la taille allouée retenue vaut 0 (inutile de dédoublonner 0).
- `Tree::ingest` tient un `links: HashSet<u64>` : pour une entrée fichier avec `file_id != 0`, si `links.insert(file_id)` renvoie `false` (déjà vu dans ce scan), sa taille allouée devient **0** ; sa taille logique est gardée. Le premier nom reçu porte donc l'allocation. L'ordre de réception dépendant des workers, le dossier qui porte l'allocation peut changer d'un scan à l'autre ; **le total de la racine, lui, est déterministe**. Limite acceptée, à écrire en commentaire `shortcut:` (passer à une attribution par nombre de liens si un utilisateur s'en plaint).
- Pas d'appel par fichier pour connaître le nombre de liens (7 à 8 fois plus lent, voir mesures) : on insère tous les identifiants à allocation non nulle.
- Coût mémoire : ≈ 9 octets par case du `HashSet` (8 + 1 octet de contrôle), soit 10 à 19 octets par fichier selon le facteur de remplissage, **seulement pendant le scan**. `Tree::end_scan()` libère l'ensemble (`self.links = HashSet::new()`) ; il est appelé par `App` à `Msg::Done` et à `Action::Stop`, et par `--bench` après la mesure. `Tree::mem_bytes()` ajoute `self.links.capacity() * 9`.
- Un scan ne traverse jamais deux volumes (les points de montage sont des points d'analyse, non suivis) : l'identifiant 64 bits suffit comme clé.
- Limites connues, à documenter dans la doc de `Tree::ingest` : les autres noms d'un lien physique peuvent avoir une taille périmée dans l'entrée de répertoire (NTFS ne met à jour que le nom utilisé pour écrire, comme aujourd'hui avec `FindFirstFile`) ; les fichiers clonés par blocs sur ReFS (Dev Drive) partagent des extents mais chacun rapporte sa propre allocation : comptés en entier, comme l'Explorateur.

### Points d'analyse, jonctions, liens symboliques (ne pas suivre)
Même filtre qu'aujourd'hui, réécrit sur les données d'énumération (la balise de point d'analyse est dans le champ `EaSize` quand `attr & 0x400 != 0`, vérifié : jonction = `0xA0000003`). Fonction pure `fn skipped(attr: u32, tag: u32) -> bool` :
- dossier (`attr & 0x10`) avec `attr & 0x400` → ignoré (jonctions, points de montage, liens symboliques de dossier) ;
- toute entrée avec `attr & 0x400` et `tag & 0x2000_0000 != 0` (bit « name surrogate » : liens symboliques, jonctions) → ignorée ;
- les autres fichiers à point d'analyse (OneDrive `0x9000_xxxx`, déduplication `0x8000_0013`, `AppExecLink` `0x8000_001B`) sont **comptés**.
- `.` et `..` ignorés.

### Volume : système de fichiers et taille de cluster
Une fois par scan, dans `Scan::start` : `GetVolumePathNameW(racine)` → racine du volume (`C:\`, `\\serveur\partage\`, ou dossier de montage) ; `GetVolumeInformationW(racine_volume)` → nom du système de fichiers ; `GetDiskFreeSpaceW(racine_volume)` → `cluster = secteurs_par_cluster * octets_par_secteur`. En cas d'échec : nom vide et `cluster = 0` (règles 1 et 2 sans arrondi ni seuil, pas de dédoublonnage). Stocké dans `Shared` (`fs_dedupe: bool`, `cluster: u64`).

### Systèmes de fichiers non NTFS
- **ReFS** : même traitement que NTFS (vérifié : identifiants partagés entre liens, allocation correcte, pas de compression NTFS ni WOF, `compact /exe` échoue).
- **FAT32 / exFAT** : pas de liens physiques, pas de creux ni de compression ; l'allocation énumérée est arrondie au cluster par le pilote. Pas de dédoublonnage (`file_id = 0`). Non vérifié sur matériel (pas de clé USB) : à vérifier si disponible, sinon noter dans le `README.md`.
- **Partage réseau (SMB)** : on prend l'allocation renvoyée par le serveur ; dédoublonnage seulement si `GetVolumeInformationW` y annonce `NTFS`/`ReFS`. Si la classe `FileIdBothDirectoryInfo` est refusée (voir repli ci-dessous), tailles allouées = logiques arrondies au cluster.

### Énumération (remplace `read_dir` sous Windows)
- Ouvrir chaque dossier avec `CreateFileW(chemin, FILE_LIST_DIRECTORY 0x0001, FILE_SHARE_READ|WRITE|DELETE 0x7, null, OPEN_EXISTING 3, FILE_FLAG_BACKUP_SEMANTICS 0x0200_0000, null)` puis boucler sur `GetFileInformationByHandleEx(h, FileIdBothDirectoryInfo = 10, buf, 65_536)` jusqu'à l'échec avec `GetLastError() == ERROR_NO_MORE_FILES (18)` ; fermer le handle (`CloseHandle`) dans tous les cas. Tampon de 64 Kio aligné sur 8 (`Vec<u64>` de 8 192), un par thread worker, réutilisé.
- Échec de `CreateFileW` → `errors += 1`, dossier ignoré (comme l'échec de `read_dir` aujourd'hui).
- Si le **premier** appel `GetFileInformationByHandleEx` échoue avec une autre erreur que 18 (classe non prise en charge par un pilote tiers ou un serveur SMB, typiquement 1 `ERROR_INVALID_FUNCTION`, 87 `ERROR_INVALID_PARAMETER` ou 124 `ERROR_INVALID_LEVEL`) → repli sur l'ancien code (`read_dir`, conservé sous le nom `scan_dir_std`) pour ce dossier, avec `alloc = round_up(len, cluster)` et `file_id = 0`. Une erreur sur un appel suivant → `errors += 1`, on garde ce qui a été lu.
- Structure lue (vérifiée sur NTFS et ReFS, 104 octets avant le nom) :
  ```rust
  #[repr(C)]
  struct FileIdBothDirInfo {          // FILE_ID_BOTH_DIR_INFO
      next: u32, index: u32,          // next = 0 : dernière entrée du tampon
      creation: i64, access: i64, write: i64, change: i64,
      eof: i64, alloc: i64,
      attr: u32,
      name_len: u32,                  // en octets
      ea_size: u32,                   // = balise de point d'analyse si attr & 0x400
      short_len: i8, short_name: [u16; 12],
      file_id: i64,                   // offset 96
  }                                   // le nom UTF-16 commence à l'offset 104 (= size_of)
  ```
  Lire chaque en-tête avec `std::ptr::read_unaligned`, le nom avec `std::slice::from_raw_parts(ptr.add(104) as *const u16, name_len / 2)`, avancer de `next` octets.
- `mtime` : `write` est un FILETIME UTC ; `secs = write / 10_000_000 - 11_644_473_600`, 0 si négatif, borné à `u32::MAX` (même valeur qu'aujourd'hui, compatible F02).
- Nom : `String::from_utf16_lossy` (équivalent du `to_string_lossy` actuel).
- **Chemins longs** : `CreateFileW` et `GetCompressedFileSizeW` n'ont pas la gestion automatique de `std`. `Scan::start` rend la racine absolue une fois (`std::path::absolute`, repli sur le chemin donné) pour les workers (le `root_path` du `Tree` ne change pas). Une fonction `fn verbatim(p: &Path) -> Vec<u16>` (terminée par 0) préfixe toujours : inchangé si le chemin commence par `\\?\` ou `\\.\` ; `\\?\UNC\` + reste si `\\serveur\…` ; sinon `\\?\` + chemin.
- Hors Windows (`#[cfg(not(windows))]`) : code `read_dir` actuel, `alloc = len`, `file_id = 0`.

### Bascule dans l'interface
- Mode par défaut : **taille sur disque** (le TODO du `README.md` demandait la taille allouée « au lieu de » la logique). Le mode est gardé par `App` (`on_disk: bool`, `true` au démarrage) et réappliqué à chaque nouvel arbre (`start_scan`). Non persistant entre deux lancements (F17).
- Bascule : **Ctrl+T** (actif aussi pendant un scan, comme Ctrl+O) et entrée de menu **Affichage** placée juste avant « Unités : Go / Gio », libellé selon l'état : « Afficher la taille logique » quand le mode sur disque est actif, « Afficher la taille sur disque » sinon, raccourci affiché « Ctrl+T ». Nouvelle `Action::ToggleOnDisk` ; `Flags` gagne `on_disk: bool`. À chaque bascule, toast : « Tailles sur disque (espace alloué) » ou « Tailles logiques ».
- Le passage de mode appelle `Tree::set_on_disk(b)`, qui incrémente `tree.version` : l'arbre et le treemap se reconstruisent par leur invalidation existante, sans code de cache supplémentaire. La sélection, les dossiers dépliés et le tri sont conservés.
- **Vues qui suivent le mode** (toutes via `tree.size(id)` / `tree.type_size(k)`, plus aucun accès direct à `.size` ou `type_sizes` hors de `tree.rs`) :
  - arbre : colonne taille, barre et colonne « % » (part du parent dans le même mode), tri par taille et par %, tri des résultats du filtre ;
  - en-tête de la colonne : « Sur disque » ou « Taille », avec infobulle « Espace réellement occupé sur le disque (clusters ; liens physiques comptés une fois). Ctrl+T : taille logique » ou « Taille logique des fichiers. Ctrl+T : taille sur disque » ;
  - treemap Dossiers (poids, filtre des tailles nulles, libellés) et Types ; message « Aucune donnée » si `tree.size(0) == 0` ;
  - panneau « Par type de fichier » ;
  - infobulle du treemap : toujours **les deux** tailles : `"{chemin}\n{sur disque} sur disque · {logique} logique · {n} fichiers"`.
- **Ne suivent pas** : carte « Nettoyage suggéré » (`compute_cleanup`) : utilise toujours `n.alloc` (ce qu'on récupère, c'est de l'espace disque) ; puces de lecteurs (espace du volume, inchangé) ; barre d'état (inchangée).
- `--bench` affiche les deux : `"{f} fichiers, {d} dossiers, {logique} logique, {sur disque} sur disque en {s} s => … arbre {mem}"`, avec `mem` mesuré **avant** `end_scan()` (pic, ensemble des liens compris).

## Choix de l'API (et alternatives écartées)

| Option | Coût mesuré | Verdict |
|---|---|---|
| `GetFileInformationByHandleEx(FileIdBothDirectoryInfo)` sur le handle du dossier | = `read_dir` actuel | **retenu** : allocation + identifiant sans appel par fichier, API Win32 documentée (Vista+), kernel32 |
| `FindFirstFileExW` (ce que fait `read_dir`) | référence | ni allocation ni identifiant dans `WIN32_FIND_DATAW` |
| `GetCompressedFileSizeW` pour chaque fichier | ×8 | pas d'arrondi au cluster ; gardé **uniquement** en repli WOF |
| handle + `GetFileInformationByHandleEx(FileStandardInfo)` / `GetFileInformationByHandle` | ×8 | donnerait le nombre de liens, trop cher |
| `GetFileInformationByName(FileStatByNameInfo)` | ×7 | Windows 11 24H2+ seulement, trop cher |
| `NtQueryDirectoryFile(Ex)` (ntdll) | ≈ retenu | non documenté côté Win32, aucun gain ici |
| `FileIdExtdDirectoryInfo` (19, identifiant 128 bits) | ≈ retenu | inutile : un scan reste sur un volume et l'identifiant 64 bits suffit (vérifié sur ReFS) ; à reconsidérer si un ReFS renvoie `u64::MAX` |

Pas de nouvelle dépendance (`windows`, `windows-sys` absentes) : blocs `extern "system"` locaux sous `#[link(name = "kernel32")]`, comme `src/main.rs`, `src/ui/drives.rs` et `src/fmt.rs`.

```rust
fn CreateFileW(name: *const u16, access: u32, share: u32, sa: *mut c_void, disp: u32, flags: u32, tmpl: *mut c_void) -> *mut c_void; // INVALID_HANDLE_VALUE = -1
fn CloseHandle(h: *mut c_void) -> i32;
fn GetFileInformationByHandleEx(h: *mut c_void, class: u32, buf: *mut c_void, len: u32) -> i32;
fn GetCompressedFileSizeW(name: *const u16, high: *mut u32) -> u32;
fn GetLastError() -> u32;
fn GetVolumePathNameW(name: *const u16, out: *mut u16, len: u32) -> i32;
fn GetVolumeInformationW(root: *const u16, vol: *mut u16, vol_len: u32, serial: *mut u32, max_comp: *mut u32, flags: *mut u32, fs: *mut u16, fs_len: u32) -> i32;
fn GetDiskFreeSpaceW(root: *const u16, spc: *mut u32, bps: *mut u32, free: *mut u32, total: *mut u32) -> i32;
```

## Stockage et mémoire

- `Node` gagne `pub alloc: u64` (taille sur disque ; agrégée comme `size`). `size` garde son sens actuel (logique). Disposition : 40 → **48 octets** (`u64` + `u64` + 7 × `u32` + `u16` + 2 × `u8`, alignement 8). Pas de `u32` en clusters : la structure passerait quand même à 48 octets avec le remplissage, et un total de racine en clusters 4 Kio déborderait au-delà de 16 To.
- Impact attendu : +8 octets par nœud (`C:\Windows` : 460 445 nœuds → +3,7 Mo sur 74,7 Mo de capacité, ≈ +5 %), plus l'ensemble des liens pendant le scan (≈ 10–19 o par fichier à allocation non nulle, libéré à la fin). À mesurer avec `--bench` (colonne « arbre ») avant/après et à reporter dans le message de commit.
- `Tree` gagne : `on_disk: bool` (`true` par défaut), `type_alloc: [u64; 8]`, `links: HashSet<u64>` ; méthodes `pub fn size(&self, id: u32) -> u64` (selon `on_disk`), `pub fn type_size(&self, k: Kind) -> u64`, `pub fn set_on_disk(&mut self, b: bool)` (ne fait rien si inchangé, sinon `version += 1`), `pub fn end_scan(&mut self)`.
- `Entry` gagne `pub alloc: u64` et `pub file_id: u64` (transitoires).

## Hors périmètre

- Flux de données alternatifs, taille des métadonnées de dossiers (`$I30`), `$MFT`, fichiers système NTFS.
- Lire la MFT (F30), afficher le nombre de liens ou la liste des autres noms d'un fichier (F14).
- Option persistante du mode par défaut, unités Go/Gio (F16/F17).
- Afficher les deux tailles en colonnes simultanées dans l'arbre ; colonne « taux de compression ».
- Optimiser le scan au-delà du budget (F05).
- Modifier `ScanState`, la barre d'état, les puces de lecteurs.

## Fichiers concernés

- `src/scan/mod.rs` : énumération Windows décrite plus haut (`scan_dir` Windows, `scan_dir_std` = code actuel en repli et hors Windows), `fn on_disk`, `fn skipped`, `fn round_up`, `fn verbatim`, infos de volume dans `Scan::start` / `Shared`, structure `FileIdBothDirInfo`, bloc `extern`. Supprimer le `// TODO` et `is_reparse_dir` (remplacé par `skipped`, garder la version `std` pour le repli). Si le fichier dépasse ~350 lignes, déplacer la partie Windows dans `src/scan/win.rs` (pas plus de découpage).
- `src/scan/tree.rs` : `Node::alloc`, `Entry::{alloc, file_id}`, champs et méthodes de `Tree` ci-dessus, agrégation et dédoublonnage dans `ingest`, `mem_bytes`.
- `src/ui/tree_view.rs` : `tree.size(id)` partout où `.size` est lu ; `header(ui, on_disk)` pour le libellé et l'infobulle de la colonne.
- `src/ui/treemap.rs` : `TreeSrc` et libellés via `tree.size`, mode Types via `tree.type_size`, infobulle à deux tailles.
- `src/ui/types.rs` : `tree.type_size(k)`.
- `src/ui/mod.rs` : `Action::ToggleOnDisk`, `Flags::on_disk`.
- `src/ui/menu.rs` : entrée Affichage.
- `src/app.rs` : champ `on_disk`, raccourci Ctrl+T, action, `set_on_disk` dans `start_scan`, `end_scan()` à `Done` et `Stop`, `compute_cleanup` sur `alloc`.
- `src/main.rs` : `--bench` (deux tailles, mémoire avant `end_scan`).
- `README.md` : « État » — retirer le TODO ; ajouter dans « Fonctionnel » : « taille sur disque par défaut (clusters, fichiers creux, compressés NTFS et WOF, liens physiques comptés une fois), bascule taille logique (Ctrl+T, menu Affichage) » ; dans « Limites » : FAT32/exFAT non vérifiés sur matériel (si c'est le cas), attribution d'un lien physique au premier nom rencontré.
- `Cargo.toml` : inchangé.

## Risques

- **Disposition de `FileIdBothDirInfo`** : une erreur donne des tailles fausses sans panique. Test d'offsets obligatoire (`offset_of!`) et tests d'intégration sur fichiers réels.
- **Lecture hors tampon** : vérifier `off + 104 + name_len <= octets valides` avant de lire un nom ; sinon arrêter le dossier avec `errors += 1`. Jamais d'`unwrap` sur une donnée noyau.
- **Fuite de handles** : `CloseHandle` sur tous les chemins de sortie (petite garde `struct Handle(*mut c_void)` avec `Drop`).
- **Régression du filtre des liens** : une erreur de `skipped` peut faire suivre une jonction (boucle, double comptage) ou masquer des fichiers OneDrive. Tests dédiés.
- **Débit** : la règle 1 (GCFS) sur un volume CompactOS touche des dizaines de milliers de fichiers de `C:\Windows`. Si le budget est dépassé sur une machine CompactOS (`compact /compactos:query`, lecture seule), le signaler dans le rapport plutôt que d'abandonner la règle (sinon `C:\Windows` serait sous-évalué de plusieurs Go).
- **Différences attendues avec l'Explorateur** : l'Explorateur compte les liens physiques autant de fois qu'ils ont de noms et inclut parfois les flux alternatifs ; comparer sur un dossier sans liens physiques.
- **Tests dépendant du système de fichiers du dossier temporaire** : compression et WOF n'existent que sur NTFS. Ces tests lisent le système de fichiers du dossier temporaire et, s'il n'est pas NTFS (ou si `compact` renvoie un code non nul pour WOF), affichent `eprintln!("ignoré : …")` et retournent sans échec.

## Critères de fin

- `cargo build --release`, `cargo clippy --all-targets` et `cargo test` passent sans avertissement.
- **Dossiers de test** : chaque test de fichiers crée **son propre** dossier neuf `std::env::temp_dir().join(format!("optisize-f04-{nom}-{pid}-{nanos}"))` avec `create_dir` (échec si existant), n'écrit que dedans et le supprime à la fin avec `remove_dir_all` sur ce seul dossier (jonction retirée d'abord par `remove_dir`). Aucun autre fichier touché, aucun réglage Windows modifié, pas d'élévation.
- Tests unitaires purs (toutes plateformes sauf mention) :
  - `node_is_48_bytes` — `size_of::<Node>() == 48`.
  - `dir_info_layout` (`cfg(windows)`) — `offset_of!(FileIdBothDirInfo, file_id) == 96`, `size_of::<FileIdBothDirInfo>() == 104`.
  - `on_disk_rules` — cluster 4096 : (10 000, 12 288, 0x20) → 12 288 ; (100, 104, 0x20) → 0 ; (0, 0, 0x20) → 0 ; (10 Mio, 65 536, 0x220) → 65 536 ; (10 Mio, 0, 0x220) → 0 ; (1 Mio, 65 536, 0x820) → 65 536 ; (1 Mio, 0, 0x20) → `None` ; (1 Mio, 0, 0x400420) → 0 ; cluster 0 : (100, 104, 0x20) → 104.
  - `round_up_cluster` — (10 000, 4096) → 12 288 ; (8192, 4096) → 8192 ; (0, 4096) → 0 ; (69 000, 0) → 69 000.
  - `skipped_rules` — dossier 0x410 tag 0xA0000003 → oui ; fichier 0x420 tag 0xA000000C → oui ; fichier 0x420 tag 0x9000001A → non ; fichier 0x20 → non ; dossier 0x10 → non.
  - `verbatim_paths` (`cfg(windows)`) — `C:\a` → `\\?\C:\a` ; `\\srv\sh\x` → `\\?\UNC\srv\sh\x` ; `\\?\C:\a` inchangé ; zéro final présent.
  - `filetime_to_unix` — 0 → 0 ; `116_444_736_000_000_000` → 0 ; `+ 10_000_000 * 1_704_067_200` → 1 704 067 200.
- Tests de `Tree` (`src/scan/tree.rs`, entrées construites à la main) :
  - `aggregates_alloc_up_to_root` — comme le test existant, avec des `alloc` distincts : sommes `size` et `alloc` exactes à chaque niveau.
  - `hard_link_alloc_counted_once` — deux entrées dans deux dossiers, même `file_id`, `size` 10 000, `alloc` 12 288 : racine `size` 20 000, `alloc` 12 288 ; second fichier `alloc` 0.
  - `file_id_zero_never_deduped` — deux entrées `file_id = 0` : allocation comptée deux fois.
  - `toggle_switches_sizes_and_bumps_version` — `size(id)` et `type_size(k)` changent avec `set_on_disk`, `version` +1, `set_on_disk` avec la même valeur ne change pas `version`.
  - `end_scan_frees_link_set` — `mem_bytes()` diminue après `end_scan()`, totaux inchangés.
  - tests existants adaptés (`e()` avec `alloc`, `file_id`).
- Tests d'intégration (`cfg(windows)`, dans `src/scan/mod.rs`, via un helper `scan_tree(dir) -> Tree` qui fait comme `--bench`) ; `cluster` lu par la même fonction de volume que le scan :
  - `volume_info_of_temp` — nom de système de fichiers non vide, `cluster` puissance de deux ≥ 512.
  - `scan_plain_file_rounds_to_cluster` — 10 000 octets → `size` 10 000, `alloc` `round_up(10 000, cluster)`.
  - `scan_tiny_file_zero_on_disk` — 100 octets → `alloc` 0 (NTFS ou ReFS, sinon ignoré).
  - `scan_sparse_file` — `FSCTL_SET_SPARSE` (`DeviceIoControl`, code `0x0009_00C4`, sans élévation), 64 Kio écrits, `set_len(10 Mio)`, `sync_all`, fermé → `size` 10 Mio, `alloc < 1 Mio`.
  - `scan_compressed_file` (NTFS) — 1 Mio de `"abcdefgh"` répétés, fichier fermé puis `compact /c <fichier>` (code 0 attendu) → `alloc < 256 Kio`.
  - `scan_wof_file` (NTFS ; ignoré si `compact /c /exe:xpress4k` ≠ 0) — même contenu → `0 < alloc < 256 Kio` (mesuré : 69 632).
  - `scan_hard_link_counted_once` — `a\f.bin` 10 000 octets + `std::fs::hard_link` vers `b\g.bin` → racine `files` 2, `size` 20 000, `alloc` `round_up(10 000, cluster)`.
  - `scan_does_not_follow_junction` — `cible\x.bin` + jonction `lien → cible` créée par `cmd /c mklink /J` (sans élévation) → `x.bin` compté une fois, `lien` absent de l'arbre ; nettoyage : `remove_dir(lien)` puis `remove_dir_all`.
  - `scan_long_path` — dossiers imbriqués jusqu'à un chemin de plus de 300 caractères, un fichier au fond (nom accentué `é.txt`) → trouvé, tailles exactes.
  - `scan_matches_std_counts` — 50 dossiers × 4 fichiers de tailles variées : nombres de fichiers, de dossiers et taille logique identiques à un parcours `std::fs` récursif.
- **Mesure du débit (obligatoire, à reporter dans le message de commit)** :
  1. Avant toute modification : `cargo build --release`, copier `target\release\optisize.exe` vers `$env:TEMP\optisize-f04-base.exe`.
  2. Après : 7 paires entrelacées (base, nouveau, base, nouveau…) de `--bench "C:\Program Files"` puis de `--bench "C:\Windows"` (lecture seule ; dossiers refusés ignorés comme aujourd'hui).
  3. Budget : **médiane du nouveau ≤ médiane de la base + 5 %** sur chaque dossier, et mémoire « arbre » ≤ +25 %. Au-delà : profiler et corriger avant de livrer ; si c'est dû au repli WOF sur une machine CompactOS, le dire avec les chiffres.
  4. Supprimer ensuite le seul fichier `$env:TEMP\optisize-f04-base.exe`.
- **Vérification manuelle** (`cargo run --release`, scans en lecture seule, jamais Corbeille / Nettoyer) :
  1. Scanner `C:\Program Files` : l'en-tête affiche « Sur disque » ; la taille racine est à ±1 % de « Taille sur disque » des Propriétés de ce dossier dans l'Explorateur (dossier sans liens physiques ; ouvrir les Propriétés ne modifie rien).
  2. Ctrl+T : en-tête « Taille », toast « Tailles logiques », valeurs ≈ « Taille » de l'Explorateur ; arbre, %, tri, treemap et panneau Types changent ; sélection et dossiers dépliés conservés. Ctrl+T de nouveau : retour. Même chose par le menu Affichage (libellé qui change).
  3. Scanner `C:\Windows` : en mode sur disque, total nettement inférieur au logique (liens WinSxS) ; l'infobulle du treemap montre les deux tailles.
  4. Basculer pendant un scan : pas de blocage, le scan continue.
  5. Dans un dossier neuf `$d = New-Item -ItemType Directory "$env:TEMP\optisize-f04-$(Get-Random)"` : y créer un fichier de 100 octets, un fichier de 1 Mio compressé par `compact /c`, un lien physique (`New-Item -ItemType HardLink`), scanner `$d`, comparer chaque fichier avec « Taille sur disque » de ses Propriétés (le lien physique : compté une fois dans le total). Supprimer ensuite le seul dossier `$d`.
  6. Si une clé USB FAT32/exFAT est disponible : scanner, tailles sur disque multiples du cluster, aucune erreur. Sinon, le noter dans le `README.md`.
- Lignes à mettre à jour : statut F04 → `done` dans `ROADMAP.md` ; section « État » du `README.md` comme indiqué dans « Fichiers concernés ».
