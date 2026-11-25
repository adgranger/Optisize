---
id: F01
title: Fiabiliser « Choisir un dossier… » (Ctrl+O) et le clic sur une puce de lecteur
model: sonnet        # modèle qui implémente : sonnet | opus (voir la règle dans ROADMAP.md)
model_reason: Câblage d'interface borné à app.rs, drives.rs et main.rs, avec un seul appel Windows (SetErrorMode) dont la constante est donnée ici ; rien ne touche aux données.
depends_on: []
---

## Contexte

Deux entrées démarrent un scan sur une nouvelle racine :

- **« Choisir un dossier… »** : bouton de la barre d'outils (`src/ui/toolbar.rs`), menu Fichier (`src/ui/menu.rs`) et raccourci Ctrl+O (`App::shortcuts`, `src/app.rs`). Tous émettent `Action::PickFolder`. `App::run` lance `rfd::FileDialog::pick_folder()` dans un thread et garde un `Receiver<Option<PathBuf>>` dans `App::picker` ; `App::pump` lit la réponse et appelle `start_scan()`.
- **Puce de lecteur** : `drives::show` (`src/ui/drives.rs`) renvoie la racine cliquée (`"C:\\"`), `App::ui` l'applique en fin de frame.

Lecture du code (aucun test n'existe pour ces chemins) — défauts constatés :

1. **Puce ignorée pendant un scan.** `App::ui` ne fait rien si l'état est `Scanning` ou `Paused` : aucun retour visuel, alors que l'infobulle dit « Scanner C:\ ». À l'inverse, un dossier choisi par Ctrl+O pendant un scan annule le scan en cours et démarre le nouveau. Les deux entrées se comportent différemment.
2. **Sélecteur bloqué pour toujours si le thread meurt.** `pump` ne traite que `Ok(..)` de `try_recv()`. Si le thread du dialogue panique (échec COM, etc.), l'émetteur est détruit, `try_recv` renvoie `Err(Disconnected)` en boucle, `picker` reste `Some` : Ctrl+O et le bouton ne font plus rien, et l'UI se redessine toutes les 100 ms indéfiniment.
3. **Dialogue sans fenêtre parente.** Le dialogue n'est pas modal : il peut s'ouvrir derrière la fenêtre Optisize, et l'utilisateur peut continuer à cliquer dans l'application, qui semble alors ne pas réagir à Ctrl+O (garde `picker.is_none()`).
4. **Dossier de départ du dialogue non défini** : il s'ouvre sur le dernier dossier mémorisé par Windows, pas sur le dossier courant d'Optisize.
5. **`"D:"` sans barre oblique** : `PathBuf::from("D:")` est le *répertoire courant* du lecteur D (chemin relatif au lecteur), pas sa racine. Saisi dans le champ « Dossier » ou reçu en argument, il scanne le mauvais dossier.
6. **Lecteur absent ou non prêt.** La liste des lecteurs n'est énumérée qu'une fois au démarrage. Une clé USB retirée garde sa puce ; le clic affiche « Dossier introuvable » sans dire lequel, et la puce reste. Une clé branchée après le lancement n'apparaît jamais. De plus, sans `SetErrorMode`, interroger un lecteur amovible vide (lecteur de cartes, DVD) peut afficher la boîte système « Il n'y a pas de disque dans le lecteur » et bloquer le thread d'énumération ou un worker de scan.

## Comportement attendu

Une seule fonction d'entrée, `App::open(&mut self, input: &str)`, est utilisée par le dialogue, la puce, le champ « Dossier » (Entrée / Scanner / F5 hors pause) et l'argument de ligne de commande. `start_scan()` devient l'implémentation interne appelée par `open` et par Rafraîchir.

### Normalisation et validation (`scan_root`)
- `scan_root(input: &str) -> Result<PathBuf, String>` : fonction pure (hors `impl App`, dans `src/app.rs`).
  - Retire les espaces en tête et en fin.
  - Chaîne vide → `Err("Aucun dossier indiqué")`.
  - Lettre de lecteur seule (`"d:"`, `"D:"`) → `D:\` (ajoute `\`, met la lettre en majuscule).
  - Chemin qui n'est pas un dossier existant → `Err(format!("Dossier introuvable : {chemin}"))` avec le chemin normalisé.
  - Les chemins UNC (`\\serveur\partage`) et les chemins longs `\\?\` sont acceptés tels quels s'ils sont des dossiers.
- En cas d'`Err`, `open` affiche le message en toast (barre d'état), ne touche ni à l'arbre, ni au scan en cours, ni à `self.path`, puis relance `drives::enumerate_async` (la puce d'un lecteur disparu s'efface).
- En cas d'`Ok`, `self.path` reçoit le chemin normalisé (le champ « Dossier » affiche `D:\`, pas `D:`).

### Scan en cours
- Choisir un dossier (dialogue) ou cliquer une puce **pendant** `Scanning` ou `Paused` : le scan en cours est annulé (le `Drop` de `Scan` suffit, les workers en pause vérifient `cancel` avant `paused`) et le nouveau démarre. Même règle pour les deux entrées.
- L'infobulle de la puce reste « Scanner C:\ ».
- Si la validation échoue pendant un scan, le scan en cours **continue** (rien n'est annulé avant que la nouvelle racine soit validée).

### Dialogue « Choisir un dossier… »
- Construit dans le thread UI : `rfd::FileDialog::new().set_title("Choisir un dossier").set_parent(frame)`, plus `.set_directory(&self.path)` seulement si `self.path` est un dossier existant. `FileDialog` est `Send` (rfd 0.18), il est ensuite déplacé dans le thread qui appelle `pick_folder()`. `eframe::Frame` implémente `HasWindowHandle + HasDisplayHandle` : `App::run` reçoit donc `frame: &eframe::Frame` en plus de `ctx` (le passer depuis `App::ui`, aujourd'hui `_frame`).
- Avec une fenêtre parente, le dialogue est modal : il s'ouvre devant Optisize, et la fenêtre principale ne reçoit plus d'entrées tant qu'il est ouvert.
- Ctrl+O, le bouton ou le menu, alors que le dialogue est déjà ouvert : aucun second dialogue (garde `picker.is_none()` conservée).
- Annulation (Échap, bouton Annuler, fermeture) → `None` : rien ne change, aucun toast, le scan en cours (s'il y en a un) continue.
- Le thread du dialogue meurt sans répondre (`TryRecvError::Disconnected`) → `picker = None` et toast « Impossible d'ouvrir le sélecteur de dossier ». Ctrl+O fonctionne de nouveau ensuite.
- La lecture du canal passe par une fonction pure testable :
  ```rust
  enum Pick { Pending, Cancelled, Chosen(PathBuf), Failed }
  fn poll_pick(rx: &Receiver<Option<PathBuf>>) -> Pick
  ```
- Un dossier choisi passe par `App::open` (donc par `scan_root`).

### Puces de lecteur
- La liste est ré-énumérée (`drives::enumerate_async`) : au démarrage (existant), à la fin de chaque scan (`Msg::Done` ou Arrêter — les tailles utilisées changent), et après un échec de `open`. Pas de minuterie.
- La puce active se calcule par une fonction pure `drives::is_active(current: &str, d: &Drive) -> bool` (comparaison insensible à la casse de la lettre, comme aujourd'hui), pour pouvoir la tester.
- Le processus ne montre jamais de boîte système « pas de disque » : `main()` appelle, avant tout, `SetErrorMode(SEM_FAILCRITICALERRORS | SEM_NOOPENFILEERRORBOX)` (`0x0001 | 0x8000`) via un `extern "system"` dans `kernel32`, sur le modèle de `attach_console` (`#[cfg(windows)]`). Un lecteur vide est alors simplement absent de la liste (l'appel `GetDiskFreeSpaceExW` échoue déjà proprement et filtre la puce).

### Chemins réseau
- Un lecteur réseau mappé apparaît comme une puce s'il répond à `GetDiskFreeSpaceExW` (inchangé) ; l'énumération reste dans son thread (un partage déconnecté peut bloquer plusieurs secondes : l'UI ne doit pas geler).
- Un partage UNC choisi par le dialogue ou saisi est scanné comme un dossier local. `scan_root` appelle `is_dir()` dans le thread UI : sur un partage injoignable, cet appel peut bloquer quelques secondes. **Accepté pour F01** (voir Risques) ; ne pas ajouter de thread de validation.

## Hors périmètre

- Sous-menus « Choisir un lecteur » et « Dossiers récents » du menu Fichier (F15) : ils restent grisés.
- Détection en direct de l'insertion de clés USB (`WM_DEVICECHANGE`) : seule la ré-énumération ci-dessus.
- Annulation demandée par confirmation (« Un scan est en cours, l'interrompre ? ») : non, on annule directement.
- Validation asynchrone des chemins réseau, glisser-déposer d'un dossier sur la fenêtre.
- Toute modification de `src/scan/` : le démarrage et l'annulation du scan fonctionnent déjà (`Scan::start`, `Drop`).

## Fichiers concernés

- `src/app.rs`
  - `fn scan_root(input: &str) -> Result<PathBuf, String>` (libre, pure).
  - `enum Pick` + `fn poll_pick(rx) -> Pick` (libres, pures).
  - `App::open(&mut self, input: &str)` : seule porte d'entrée vers un nouveau scan ; `App::new` l'utilise pour l'argument de démarrage (le chemin par défaut `USERPROFILE` ne lance pas de scan, inchangé).
  - `App::run(&mut self, a, ctx, frame)` : `PickFolder` construit le dialogue avec parent et dossier de départ ; `Scan` (hors pause) et `Refresh` passent par `open(&self.path.clone())` ; `Parent` aussi.
  - `App::pump` : utilise `poll_pick` ; ré-énumère les lecteurs sur `Done`. `Action::Stop` ré-énumère aussi.
  - `App::ui` : la puce cliquée appelle `self.open(&root)` sans condition d'état ; transmettre `frame` à `run`.
- `src/ui/drives.rs` : `pub fn is_active(current: &str, d: &Drive) -> bool`, utilisé par `show`.
- `src/main.rs` : appel `SetErrorMode` en tête de `main()` (`#[cfg(windows)]`, fonction `quiet_drive_errors()` à côté de `attach_console`).
- Aucun changement dans `src/ui/menu.rs`, `src/ui/toolbar.rs`, `src/scan/`.

## Risques

- **`set_parent` depuis un autre thread** : le dialogue tourne dans un thread secondaire avec pour propriétaire la fenêtre du thread UI. Windows l'autorise (fenêtre propriétaire désactivée pendant `Show`). À vérifier manuellement : l'UI d'egui ne doit pas geler pendant que le dialogue est ouvert (la barre d'état continue d'avancer si un scan tourne). Si elle gèle ou si le dialogue ne s'affiche pas, retirer `set_parent` et le noter dans le README ; ne pas passer le dialogue dans le thread UI (bloquerait la boucle d'événements et le scan).
- **`is_dir()` bloquant sur UNC** : accepté, documenté ci-dessus. Ne pas l'appeler à chaque frame (seulement dans `open`).
- **Énumérations concurrentes** : plusieurs `enumerate_async` peuvent se chevaucher ; le dernier écrit gagne, c'est sans conséquence (même source). Ne pas ajouter de verrou supplémentaire.
- **`SetErrorMode` est global au processus** : c'est la valeur recommandée par Microsoft pour une application ; il masque aussi ces boîtes pour rfd, ce qui est voulu.
- **Ordre en fin de frame** : la puce et l'action sont appliquées après le dessin ; si les deux arrivent dans la même frame (improbable), la puce passe d'abord puis l'action. Ne pas compliquer.

## Critères de fin

- `cargo build --release`, `cargo clippy --all-targets` et `cargo test` passent sans avertissement.
- Tests unitaires (dans `src/app.rs` `mod tests` et `src/ui/drives.rs` `mod tests`) :
  - `scan_root_trims_spaces` — `"  <tempdir>  "` → `Ok(<tempdir>)`.
  - `scan_root_empty_is_error` — `""` et `"   "` → `Err("Aucun dossier indiqué")`.
  - `scan_root_missing_dir_names_path` — chemin inexistant sous `std::env::temp_dir()` → `Err` contenant ce chemin.
  - `scan_root_file_is_error` — un fichier existant (créé dans `temp_dir`) → `Err`.
  - `scan_root_bare_drive_gets_root` — `#[cfg(windows)]`, `"c:"` → `Ok(PathBuf::from("C:\\"))`.
  - `poll_pick_pending` — canal ouvert et vide → `Pick::Pending`.
  - `poll_pick_cancelled` — `None` envoyé → `Pick::Cancelled`.
  - `poll_pick_chosen` — `Some(p)` envoyé → `Pick::Chosen(p)`.
  - `poll_pick_disconnected_is_failed` — émetteur détruit sans envoi → `Pick::Failed`.
  - `is_active_ignores_case` — `"c:\\Users"` et puce `C:` → vrai ; puce `D:` → faux.
- Vérification manuelle (`cargo run --release`) :
  1. Ctrl+O depuis l'arbre, puis depuis le champ « Filtrer » ayant le focus : le dialogue s'ouvre **devant** la fenêtre, sur le dossier courant ; la fenêtre principale ne réagit pas aux clics tant qu'il est ouvert.
  2. Annuler le dialogue : rien ne change, pas de toast.
  3. Choisir un dossier : le champ « Dossier » l'affiche, le scan démarre.
  4. Pendant un scan, puis pendant une pause : clic sur une autre puce → le scan repart sur ce lecteur ; idem avec Ctrl+O + choix d'un dossier. Ctrl+O + Annuler → le scan d'origine continue.
  5. Saisir `d:` (ou la lettre d'un lecteur présent) dans « Dossier » + Entrée : le champ devient `D:\` et la racine est scannée.
  6. Avec une clé USB : lancer Optisize, retirer la clé, cliquer sa puce → toast « Dossier introuvable : E:\ », la puce disparaît. Rebrancher, terminer un scan quelconque → la puce réapparaît.
  7. Si un lecteur de cartes ou DVD vide est présent : aucune boîte « pas de disque » au démarrage ni à la fin d'un scan.
  8. Un partage `\\serveur\partage` (si disponible) via le dialogue : le scan démarre.
- Lignes à mettre à jour : statut F01 → `done` dans `ROADMAP.md` ; section « État » du `README.md` : retirer toute mention d'incertitude sur Ctrl+O / puces, et noter la limite « validation d'un chemin réseau injoignable bloquante quelques secondes ».
