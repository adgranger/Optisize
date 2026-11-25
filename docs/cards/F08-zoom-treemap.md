---
id: F08
title: Zoom du treemap — double-clic pour entrer dans un dossier, fil d'Ariane, retour
model: sonnet        # modèle qui implémente : sonnet | opus (voir la règle dans ROADMAP.md)
model_reason: Câblage d'interface borné dans un seul widget (`TreemapView`) plus quelques fonctions pures testables ; `layout_nested` accepte déjà une racine quelconque et les règles de cache qui protègent du plantage d'index sont écrites ici noir sur blanc.
depends_on: []
---

## Contexte

Le treemap (`TreemapView`, `src/ui/treemap.rs`) dessine toujours l'arbre depuis la racine du scan (`tl::layout_nested(&TreeSrc(tree), 0, …)`). Sur un gros disque, un dossier profond n'occupe que quelques pixels : impossible de le lire. TreeSize et WinDirStat permettent d'« entrer » dans un dossier de la carte, avec un chemin cliquable pour remonter.

État actuel utile :

- **Un seul `TreemapView`** (`App::tm`), affiché soit dans la colonne droite (vues Détails et Types), soit au centre (vue Carte, F7) — jamais deux fois dans la même frame (F07).
- Cache : `cells: Vec<Cell>` + `key: (w, h, tree.version)` + `built: Instant`. Pendant `Scanning`, un changement de `version` seul n'est reconstruit qu'au plus toutes les 250 ms ; un changement de taille l'est tout de suite. `reset()` vide `cells` et remet `key` à la sentinelle ; `App::start_scan` l'appelle à chaque nouvel arbre (le plantage d'index corrigé en F01 venait de `cells` d'un ancien arbre utilisés sur le nouveau).
- **Défaut existant** : `mode` (Dossiers / Types) ne fait pas partie de la clé. En mode Types, `cells` est vidé ; revenir en Dossiers sans changer la taille ni la version (scan terminé) laisse `cells` vide → carte noire jusqu'au prochain redimensionnement. F08 réécrit la clé et corrige ce défaut au passage.
- Clic simple : sélectionne le dossier survolé (ou le dossier parent d'un fichier), `App` fait `selected = id` puis `tv.reveal(id)`. Survol = cellule la plus profonde (`rposition`), infobulle avec chemin et tailles.
- `layout_nested(src, root, area, opts)` accepte déjà n'importe quel id comme racine (testé avec 0 seulement).
- **Identifiants stables** (vérifié dans `src/scan/tree.rs`) : les workers réservent les ids ; `Tree::ingest(parent, first, entries)` écrit les nœuds **à leur place** (`nodes[first..]`, `resize` avec `Node::HOLE` si besoin) et ne déplace ni ne supprime jamais un nœud. Un id valide le reste pendant tout le scan, après `Arrêter`, après la fin du scan et après Ctrl+T. Seul `App::start_scan` remplace l'arbre (nouveau scan, Rafraîchir, Dossier parent, puce de lecteur, Ctrl+O : tous passent par `open` → `start_scan`). Un emplacement réservé mais pas encore reçu (`HOLE`) a `parent == NONE`, `is_dir == false`, taille 0 ; il n'apparaît jamais dans `cells` (les enfants de taille 0 sont filtrés).

## Comportement attendu

### Racine de zoom
- `TreemapView` gagne un champ `root: u32` (id du dossier affiché en racine de la carte), `0` au départ.
- La mise en page utilise `layout_nested(&TreeSrc(tree), self.root, area, &Opts::default())`. La cellule de `root` couvre toute la zone (cadre avec en-tête « nom (taille) », comme la racine aujourd'hui ; libellé de l'id 0 inchangé : « Racine »).
- Le zoom est **un état du treemap**, partagé par ses deux emplacements : passer de Détails (treemap à droite) à Carte (treemap au centre) garde le zoom, et inversement. Il n'est pas persisté entre deux lancements.
- Cible valide de zoom (`fn zoomable(tree, id) -> bool`) : `id < tree.nodes.len()`, `nodes[id].is_dir`, et `id == 0 || nodes[id].parent != NONE`. Toute demande de zoom vers un id non valide est ignorée sans panique.
- Garde défensive en tête de `show` : si `!zoomable(tree, self.root)`, `self.root = 0` (ne doit jamais arriver si `reset()` est bien appelé ; protège de la classe de bug F01).

### Cache : clé et reconstruction
- La clé devient `key: (i32, i32, u64, u32, Mode)` = (largeur, hauteur, `tree.version`, `root`, `mode`). `Mode` dérive en plus `Debug`. Sentinelle de `new()` / `reset()` : `(0, 0, u64::MAX, 0, Mode::Folders)` (la `version` `u64::MAX` suffit à forcer la reconstruction).
- Règle de reconstruction, en fonction pure (testable) :
  ```rust
  type Key = (i32, i32, u64, u32, Mode);
  /// Vrai si `cells` doit être recalculé. Seul un changement de `version` pendant le scan est limité à 250 ms.
  fn needs_rebuild(old: Key, new: Key, scanning: bool, since_build: Duration) -> bool
  ```
  - `old == new` → `false` ;
  - largeur, hauteur, `root` ou `mode` différent → `true` immédiatement (le zoom doit répondre tout de suite, même pendant le scan) ;
  - seule la `version` diffère → `!scanning || since_build > 250 ms`.
- `show` appelle `needs_rebuild(self.key, key, scanning, self.built.elapsed())` à la place de la condition actuelle. Comportement inchangé pour la taille et la version ; corrige le défaut Types → Dossiers.
- `reset()` remet aussi `root = 0` (en plus de vider `cells` et `key`). Aucun autre appel à `reset()` n'est ajouté : changer de vue, Ctrl+T, Arrêter, Pause ne réinitialisent **pas** le zoom.

### Double-clic : entrer
- Cible commune au clic et au double-clic, extraite en fonction pure : `fn click_target(tree, id) -> u32` = `id` si dossier ou racine (`parent == NONE`), sinon `nodes[id].parent` (règle actuelle du clic simple, à réutiliser par le clic).
- `resp.double_clicked()` sur une cellule survolée, en mode Dossiers : `t = click_target(tree, c.id)` ; si `t != self.root` et `zoomable(tree, t)` → `self.root = t` et `ui.ctx().request_repaint()` (la nouvelle mise en page est calculée à la frame suivante ; les `cells` de cette frame restent valides).
  - Double-clic sur un **fichier** : entre dans le dossier qui le contient (si ce n'est pas déjà la racine affichée ; sinon rien).
  - Double-clic sur le **cadre / en-tête de la racine affichée** : rien.
  - Double-clic sur un **dossier dessiné plein** (trop petit pour être imbriqué, ou sans enfant de taille > 0) : on y entre ; s'il n'a aucun enfant de taille > 0, la carte montre une seule cellule pleine (comportement normal de `layout_nested`).
- Le clic simple reste la sélection, sans changement : un double-clic est aussi deux clics, donc la cible est sélectionnée et révélée dans l'arbre (comportement voulu).
- Valeur de retour de `show` inchangée (`Option<u32>`, dossier cliqué) : `App` n'a rien à changer pour la sélection.

### Fil d'Ariane (en-tête du treemap)
- Emplacement : la ligne d'en-tête existante de `TreemapView::show` (où sont « Carte de l'espace » et la bascule Dossiers / Types). Pas de ligne supplémentaire : la hauteur du composant ne change pas.
- `root == 0` ou mode **Types** : en-tête **identique à aujourd'hui** (titre « Carte de l'espace », pas de bouton). Aucune régression visuelle hors zoom.
- `root != 0` et mode Dossiers : à la place du titre, de gauche à droite :
  1. bouton retour `ui.small_button("⬆")` (si le glyphe ne s'affiche pas avec les polices embarquées, utiliser le texte « Remonter »), infobulle « Remonter d'un niveau (Retour arrière) » ;
  2. les segments du chemin, de la racine du scan jusqu'à `root`, séparés par « › » (`MUTED`). Les ancêtres sont des `ui.link(label)` cliquables (clic → zoom sur cet id) avec infobulle = chemin complet (`tree.path(id)`) ; le dernier segment (`root`) est un `RichText::strong()` non cliquable, même infobulle.
- Libellés (fonction pure `fn crumb_label(tree, id) -> String`) :
  - id 0 : dernier composant de `tree.root_path` (`file_name()`), ou le chemin complet s'il n'en a pas (racine de lecteur `C:\`) ;
  - autres : `tree.name(id)` ;
  - puis raccourci à 24 caractères (`fn short(s: &str, max: usize) -> String` : si `s.chars().count() > max`, les `max - 1` premiers caractères + « … » ; découpe **par caractères**, jamais par octets — noms accentués, CJK, emoji).
- Ids du chemin (fonction pure `fn crumbs(tree, root) -> Vec<u32>`) : ancêtres de `root` en remontant `parent` jusqu'à `NONE`, inversés → `[0, …, root]`. `root == 0` → `[0]`. Garde : arrêt si la chaîne dépasse `tree.nodes.len()` maillons.
- Troncature si le chemin ne tient pas (fonction pure, sans egui) :
  ```rust
  /// Segments à afficher : `Some(i)` = segment i, `None` = « … ». Toujours le premier et le dernier.
  fn fit_crumbs(widths: &[f32], sep: f32, ellipsis: f32, avail: f32) -> Vec<Option<usize>>
  ```
  - tout tient (somme des largeurs + séparateurs ≤ `avail`) → tous les segments ;
  - sinon on retire les segments à partir de l'index 1 (les plus proches de la racine) et on insère un seul `None` après le premier, jusqu'à ce que ça tienne ;
  - si même `[0, None, dernier]` ne tient pas → `[None, dernier]` (le dernier sera rogné par le clip) ;
  - un seul segment → `[Some(0)]` quel que soit `avail`.
  - Le « … » est un `ui.label` `MUTED` non cliquable, infobulle = chemin complet de `root`.
- Mesures : largeur d'un segment = `ui.painter().layout_no_wrap(label, FontId::proportional(13.0)/police du link, color).size().x` ; `sep` = largeur de « › » + espacements ; `avail` = `ui.available_width()` en début de ligne − largeur du bouton retour − 140 px réservés à la bascule Dossiers / Types (constante nommée `TOGGLE_W`).

### Remonter / retour
- Sémantique unique : **remonter d'un niveau** (`root = parent(root)`), pas d'historique. `TreemapView::zoom_out(&mut self, tree: &Tree) -> bool` : si `root != 0`, `root = nodes[root].parent` (ou 0 si `NONE` ou non `zoomable`), retourne `true` ; sinon `false`. `TreemapView::zoom_to(&mut self, tree, id) -> bool` : applique si `zoomable` et différent, sinon `false`.
- Déclencheurs :
  - bouton « ⬆ » de l'en-tête ;
  - clic sur un segment du fil d'Ariane (saut direct, y compris vers la racine) ;
  - touche **Retour arrière** : nouvelle `Action::ZoomOut` ; dans `App::shortcuts`, juste après le test F6–F8 : `i.key_pressed(Key::Backspace) && i.modifiers.is_none() && !typing` → `Some(Action::ZoomOut)` ; `run` : `Action::ZoomOut => { self.tm.zoom_out(&self.tree); }`. Actif dans toutes les vues (le treemap est visible dans les trois) et dans tous les états de scan. Ne fait rien en mode Types ni à la racine. Jamais quand un champ texte a le focus (il efface du texte).
  - **bouton souris « précédent » (bouton 4, `PointerButton::Extra1`)** quand le pointeur survole la zone du treemap : `resp.hovered() && ui.input(|i| i.pointer.button_clicked(PointerButton::Extra1))` → `zoom_out`. En mode Dossiers seulement.
- Non utilisés : **clic droit** (réservé au menu contextuel F13), **Échap** (Arrêter le scan), **Alt+Haut** (reste « Dossier parent » = nouveau scan du dossier parent ; ne pas le détourner).
- Remonter ou sauter via le fil d'Ariane **ne change pas la sélection**. Le cadre `ACCENT` de la sélection n'est dessiné que si sa cellule est dans la mise en page courante (code existant).

### Pendant un scan
- Zoomer, remonter et le fil d'Ariane fonctionnent pendant `Scanning` et `Paused` : ids stables, le dossier zoomé grossit en direct (reconstruction limitée à 250 ms comme aujourd'hui, sauf changement de racine qui est immédiat).
- Un dossier dont le contenu n'est pas encore reçu (taille 0) n'est pas dans `cells`, donc pas zoomable par double-clic.
- `Arrêter` et la fin du scan gardent le zoom.

### Nouveau scan, Rafraîchir, Dossier parent, lecteur, Ctrl+O
- Tous remplacent l'arbre via `App::start_scan`, qui appelle déjà `self.tm.reset()` : le zoom revient à la racine du nouveau scan. Ne pas dupliquer cet appel ailleurs ; vérifier seulement qu'aucune autre voie ne remplace `self.tree`.

### Mode Types et Ctrl+T
- Mode **Types** : le treemap par type reste global (pas de répartition par type d'un sous-dossier : l'arbre ne la tient pas). `root` est conservé mais ignoré ; en-tête sans fil d'Ariane ; double-clic, Retour arrière et bouton 4 sans effet. Revenir en Dossiers retrouve le zoom (et la carte se reconstruit grâce à `mode` dans la clé).
- **Ctrl+T** : `version` change → reconstruction ; le zoom est gardé. Si le dossier zoomé a une taille 0 dans le nouveau mode (fichiers résidents MFT en taille sur disque, par exemple), la zone affiche au centre, en `MUTED`, « Ce dossier est vide dans ce mode d'affichage » ; le fil d'Ariane reste utilisable pour remonter.
- Racine du scan de taille 0 : message existant « Aucune donnée — lancez un scan » (test sur `tree.size(0)`, inchangé, avant tout le reste).

### Vue Carte (F07) et colonne droite
- Rien à changer dans `App::ui` pour l'affichage : les deux appels à `self.tm.show(…)` restent tels quels. Le zoom posé au centre est visible à droite après F6/F8 et inversement.
- La vue Détails (arbre) n'est pas filtrée par le zoom.

## Hors périmètre

- Historique avant / arrière (bouton 5 / `Extra2`, Alt+Gauche/Droite) : seul « remonter d'un niveau » est fait.
- Zoom animé, zoom à la molette, déplacement à la souris.
- Synchroniser le zoom avec la sélection de l'arbre (sélectionner une ligne ne zoome pas ; zoomer ne change pas le dossier scanné).
- Répartition par type d'un sous-dossier (mode Types zoomé) : demanderait des compteurs par type par nœud dans `src/scan/`.
- Menu contextuel « Zoomer ici » (F13), entrée de menu Affichage pour remonter.
- Persistance du zoom (F17) ; toute modification de `src/scan/` ou de `src/treemap_layout.rs`.
- Flèches du clavier dans le treemap (F07 les a laissées libres ; non traité ici).

## Fichiers concernés

- `src/ui/treemap.rs` :
  - `Mode` : ajouter `Debug` ;
  - `TreemapView` : champ `root: u32` ; `key` à 5 éléments ; `reset()` remet `root = 0` ;
  - `pub fn zoom_out(&mut self, tree: &Tree) -> bool`, `fn zoom_to(&mut self, tree: &Tree, id: u32) -> bool` ;
  - fonctions pures privées (ou `pub(crate)`) : `zoomable`, `click_target`, `crumbs`, `crumb_label`, `short`, `fit_crumbs`, `needs_rebuild` ;
  - `show` : garde `root`, `needs_rebuild`, `layout_nested(…, self.root, …)`, message « dossier vide », en-tête avec bouton + fil d'Ariane, double-clic, bouton souris 4 ;
  - tests (voir Critères de fin).
- `src/ui/mod.rs` : `Action::ZoomOut`.
- `src/app.rs` : `shortcuts` (Retour arrière), `run` (`Action::ZoomOut`).
- `README.md`, `ROADMAP.md` (voir Critères de fin).

## Risques

- **Index périmés (classe F01)** : `cells` et `root` sont des ids de l'arbre courant. Seul `start_scan` remplace l'arbre et il appelle `reset()`, qui remet `root` à 0 et invalide la clé. La garde `zoomable(root)` en tête de `show` empêche une panique si un futur chemin oublie `reset()` (F28 « ouvrir un scan » devra l'appeler). Ne jamais indexer `tree.nodes` avec `root` avant cette garde.
- **Double-clic = deux clics** : la sélection change au premier clic (voulu). Vérifier que le double-clic sur un fichier sélectionne et zoome bien sur le même dossier, sans clignotement.
- **Zoom pendant le scan** : la règle « `root` change → reconstruction immédiate » garantit une réponse instantanée ; le coût d'une reconstruction est borné comme aujourd'hui (`max_depth` 10, `max_children` 200), et un sous-arbre coûte moins que la racine.
- **Glyphe « ⬆ »** : les polices embarquées (egui par défaut + JetBrains Mono) peuvent ne pas l'avoir ; vérifier à l'écran, sinon texte « Remonter ».
- **Bouton souris 4** : winit le transmet comme `PointerButton::Extra1` sous Windows ; à vérifier avec une souris qui l'a. S'il n'arrive pas, le noter dans « Limites » du README, ne pas contourner.
- **Retour arrière** : ne doit jamais partir quand un champ texte a le focus (`typing`) ; vérifier dans « Dossier » et « Filtrer… ».
- **Largeur du fil d'Ariane** : `TOGGLE_W = 140` est une estimation ; si la bascule Dossiers / Types passe à la ligne ou chevauche le chemin dans une colonne droite à 360 px, ajuster la constante (pas de calcul dynamique).

## Critères de fin

- `cargo build --release`, `cargo clippy --all-targets -- -D warnings` et `cargo test` passent sans avertissement.
- Tests unitaires dans `src/ui/treemap.rs` `mod tests`, sans egui (arbre construit à la main avec `Tree::new` + `Tree::ingest`, comme dans `src/scan/tree.rs` / `src/ui/types.rs` : par exemple racine 0 → `[A(dir)=1, f.txt=2]`, A → `[B(dir)=3, g.mp4=4]`, B → `[h.zip=5]`) :
  - `reset_clears_cells_key_and_root` (remplace `reset_clears_cells_and_key`) — après `root = 3`, cellules et clé modifiées, `reset()` → `cells` vide, `key` = sentinelle, `root == 0`.
  - `zoomable_accepts_dirs_only` — 0, 1, 3 → vrai ; 2, 4, 5 (fichiers) → faux ; `nodes.len()` et `u32::MAX` → faux ; un emplacement `HOLE` (ingérer un lot avec `first` au-delà de la fin pour créer un trou) → faux.
  - `click_target_file_goes_to_parent` — 4 → 1 ; 3 → 3 ; 0 → 0.
  - `crumbs_from_root_to_node` — `crumbs(t, 3) == [0, 1, 3]` ; `crumbs(t, 0) == [0]`.
  - `crumb_label_root_and_children` — racine `C:\` → `"C:\"` (cette assertion sous `#[cfg(windows)]`) ; racine `…\Users` → `"Users"` ; id 1 → `"A"`.
  - `short_cuts_on_chars_not_bytes` — `short("éééééé", 4) == "ééé…"` ; chaîne de 24 caractères inchangée ; 25 → 23 + « … ».
  - `fit_crumbs_all_fit` — largeurs `[50, 50, 50]`, sep 10, avail 500 → `[Some(0), Some(1), Some(2)]`.
  - `fit_crumbs_drops_from_the_root_side` — `[50; 6]`, sep 10, ellipsis 20, avail 220 → commence par `Some(0), None`, finit par `Some(5)`, et la largeur totale ≤ 220.
  - `fit_crumbs_too_narrow_keeps_last` — avail 10 → `[None, Some(n-1)]` ; un seul segment → `[Some(0)]`.
  - `needs_rebuild_rules` — clé égale → faux ; `root` ou `mode` différent pendant le scan à 0 ms → vrai ; taille différente → vrai ; version seule pendant le scan à 100 ms → faux, à 300 ms → vrai ; version seule hors scan → vrai.
  - `zoom_out_climbs_to_root` — `root = 3` → `zoom_out` → 1 → 0 → `false` et reste 0.
  - `zoom_to_ignores_files_and_invalid` — `zoom_to(4)`, `zoom_to(99)` → `false`, `root` inchangé ; `zoom_to(1)` → `true` ; `zoom_to(1)` à nouveau → `false`.
  - `ids_survive_later_batches` — `root = 3`, puis ingérer un nouveau lot sous 1 (nouveaux ids en fin) → `zoomable(t, 3)`, `crumbs(t, 3)` inchangés.
- Vérification manuelle (`cargo run --release -- C:\Users`, scans **en lecture seule**, ne jamais cliquer Corbeille / Nettoyer ; aucun fichier créé ni modifié hors d'un dossier neuf sous `%TEMP%`) :
  1. Au lancement et sans zoom : en-tête « Carte de l'espace » identique à avant F08, dans la colonne droite et en vue Carte (F7).
  2. Pendant le scan, F7, double-clic sur un gros dossier → il remplit la carte tout de suite, fil d'Ariane « Users › nom », bouton ⬆ ; la carte continue de se remplir. Le scan n'est ni interrompu ni mis en pause.
  3. Double-clic sur un fichier → entre dans son dossier ; double-clic sur l'en-tête de la racine affichée → rien.
  4. Descendre de 5–6 niveaux dans la colonne droite étroite (360 px) : le fil d'Ariane se tronque en « Users › … › dernier », la bascule Dossiers / Types reste visible et à sa place ; infobulles = chemins complets.
  5. Cliquer un segment intermédiaire → saut direct ; ⬆ → un niveau ; Retour arrière (aucun champ focalisé) → un niveau ; bouton 4 de la souris sur la carte → un niveau. À la racine, ⬆ disparaît et Retour arrière ne fait rien.
  6. Focus dans « Filtrer… » puis Retour arrière → efface une lettre, le zoom ne bouge pas.
  7. Zoomé : F6 puis F7 → même zoom au centre ; en Détails, la carte de droite est zoomée aussi.
  8. Basculer en mode Types → carte par type globale, en-tête « Carte de l'espace » ; revenir en Dossiers → zoom retrouvé, carte dessinée (plus de carte noire, scan terminé, sans redimensionner).
  9. Ctrl+T zoomé → valeurs changées, zoom gardé.
  10. Zoomé, puis F5 (scan terminé), Ctrl+R, Alt+Haut, une puce de lecteur, Ctrl+O : chaque nouveau scan repart à la racine sans panique.
  11. Clic simple : sélection et révélation dans l'arbre comme avant ; double-clic : la cible est aussi sélectionnée.
- Lignes à mettre à jour : statut F08 → `done` dans `ROADMAP.md` ; section « État » du `README.md`, « Fonctionnel » : ajouter « zoom du treemap (double-clic, fil d'Ariane cliquable, remonter par ⬆ / Retour arrière / bouton souris 4) » ; « Limites » : tout point de Risques constaté (bouton 4, glyphe, largeur du fil d'Ariane).
