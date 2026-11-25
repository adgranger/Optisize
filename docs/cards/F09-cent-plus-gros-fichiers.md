---
id: F09
title: Vue « 100 plus gros fichiers » (F9)
model: sonnet        # modèle qui implémente : sonnet | opus (voir la règle dans ROADMAP.md)
model_reason: Une nouvelle vue dans le cadre déjà posé par F07 (variante de `View`, branche du panneau central) plus une seule fonction pure simple (tas borné en un passage linéaire) ; le coût, les règles de cache et la garde contre les ids périmés sont fixés ici, rien ne touche au scan.
depends_on: []
---

## Contexte

Sur un gros disque, l'arbre (Détails) et le treemap montrent les **dossiers** lourds, mais pas directement les **fichiers** les plus gros, dispersés à toutes les profondeurs (ISO, vidéos, images de VM, `hiberfil.sys`…). TreeSize a un onglet « Top 100 » ; WinDirStat et WizTree une liste équivalente. C'est le moyen le plus rapide de trouver quoi libérer.

État actuel utile (vérifié dans le code) :

- `enum View { Details, Map, Types }` dans `src/ui/mod.rs`, `impl View` dans `src/ui/views.rs` (`ALL`, `label`, `tab`, `key`, `key_text`, `for_key`, `side_treemap`, `side_types`) ; F07 a prévu l'ajout d'une variante : entrée dans `View::ALL` + une branche dans le `match self.view` du panneau central (`App::ui`, `src/app.rs`).
- Menu **Affichage** (`src/ui/menu.rs`) : les vues sont générées par `for v in View::ALL { view_item(…) }`, puis une entrée grisée `item(ui, o, "100 plus gros fichiers", "F9", None, true)`.
- `App::shortcuts` : premier test = touches de `View::ALL` sans modificateur (actif même si un champ texte a le focus). F9 n'est aujourd'hui intercepté par rien.
- Arbre (`src/scan/tree.rs`) : arène `nodes: Vec<Node>` indexée par `u32`. Un fichier a `is_dir == false` ; un emplacement réservé non reçu (`Node::HOLE`) a `parent == NONE`, `is_dir == false`, tailles 0. `tree.size(id)` rend la taille du mode courant (`alloc` sur disque par défaut, `size` logique après Ctrl+T). `tree.version` est incrémenté à chaque lot ingéré **et** à chaque bascule Ctrl+T. Les ids sont stables tant que l'arbre n'est pas remplacé ; seul `App::start_scan` le remplace (et y appelle déjà `tv.reset()` / `tm.reset()`).
- Liens physiques (F04) : en taille sur disque, le premier nom reçu d'un `file_id` porte `alloc`, les autres noms ont `alloc = 0` ; en taille logique, chaque nom garde `size`.
- Un fichier ingéré ne change plus jamais de taille ensuite (seuls les dossiers agrègent) : la liste des plus gros fichiers ne fait que s'enrichir pendant un scan.
- Réutilisables tels quels : `fmt::size`, `fmt::percent`, `fmt::date(mtime, now)`, `theme::{kind_color, mono, TEXT, MUTED, SEL, BAR, BORDER, ACCENT}`, `Kind::label`, `tree.name`, `tree.path`, l'en-tête cliquable et le rendu de ligne de `TreeView` (modèle à suivre, pas à factoriser).

## Comportement attendu

### Modèle : `View::Top`
- `View` gagne une 4ᵉ variante `Top`. `View::ALL = [Details, Map, Types, Top]` (ordre des onglets et du menu).
- `label` = « 100 plus gros fichiers », `tab` = « Gros fichiers », `key` = `Key::F9`, `key_text` = « F9 ».
- `side_treemap()` et `side_types()` : **vrais** pour `Top` (la colonne droite garde treemap, Types, Nettoyage, Sélection). Le code actuel (`self != View::Map`, `self != View::Types`) donne déjà ce résultat : ne rien changer, seulement le tester.
- Le commentaire « F09/F10 ajouteront leurs variantes ici » de `View::ALL` devient « F10 ajoutera… ».
- Rien d'autre ne change dans `set_view`, `Flags`, `Action` : F9, l'onglet et le menu passent par `Action::SetView(View::Top)` existant.

### Calcul du top (fonction pure, sans egui)
Dans un nouveau fichier `src/ui/top.rs` :
```rust
/// Les `n` plus gros fichiers de tout l'arbre dans le mode de taille courant,
/// triés par taille décroissante puis id croissant. Exclut dossiers, emplacements non reçus et tailles 0.
pub fn top_files(tree: &Tree, n: usize) -> Vec<u32>
```
- Un passage sur `1..tree.nodes.len()`, garde : `!node.is_dir && node.parent != NONE && tree.size(id) > 0`.
- **Tas borné** : `BinaryHeap<Reverse<(u64, Reverse<u32>)>>` de capacité `n` (tas-min sur la clé `(taille, Reverse(id))`). Tant que le tas a moins de `n` éléments : `push`. Sinon, si la clé est **strictement** supérieure au minimum (`peek`) : `pop` puis `push`. À la fin : `into_vec`, tri décroissant par clé, extraction des ids.
- Clé `(taille, Reverse(id))` = règle d'égalité : à taille égale, le plus petit id gagne (et passe devant). Le résultat est déterministe pour un arbre donné.
- `n == 0` → vide. Moins de `n` fichiers éligibles → tous, dans l'ordre.
- Coût : O(N log n) au pire, O(N) en pratique (la plupart des fichiers sont rejetés par une comparaison avec le minimum), sans allocation hors du tas de 100 éléments. Ordre de grandeur attendu : 1,3 M de nœuds × 48 octets ≈ 62 Mo lus séquentiellement, **< 30 ms en release** (à mesurer, voir Critères de fin). Pas de tri complet, pas de `Vec` de tous les fichiers.
- Pourquoi un recalcul et pas un tas tenu à jour pendant le scan : le recalcul ne touche ni `src/scan/` ni `Tree::ingest`, se calcule seulement quand la vue est affichée, et gère gratuitement la bascule Ctrl+T (l'ordre des fichiers change selon le mode) et les liens physiques. Le coût ci-dessus, limité à 2 fois par seconde pendant le scan, est négligeable.

### Liens physiques
- Conséquence directe de `tree.size` (aucun code spécial) : **taille sur disque** → un fichier physique apparaît **une seule fois**, sous le nom qui porte l'allocation (les autres noms ont 0 et sont exclus) ; **taille logique** → chaque nom apparaît (un même fichier peut figurer deux fois, dans deux dossiers, avec la même taille). C'est cohérent avec les totaux de l'arbre et du pied de liste dans chaque mode. Le documenter dans l'infobulle de l'en-tête de la colonne Taille (voir plus bas).

### État de la vue : `TopView`
Dans `src/ui/top.rs` :
```rust
pub const TOP_N: usize = 100;
const STALE_MS: u64 = 500;

struct Row { id: u32, rank: u32, dir: String }   // dir = chemin du dossier parent, relatif à la racine du scan

pub struct TopView {
    rows: Vec<Row>,          // dans l'ordre d'affichage courant
    built_version: u64,      // sentinelle u64::MAX
    built: Instant,
    sort: Col, desc: bool,   // Col::{Rank, Name, Dir, Size, Pct, Date, Kind} ; défaut Size, desc
    scroll_to: Option<u32>,
}
impl TopView {
    pub fn new() -> Self
    pub fn reset(&mut self)  // vide rows, built_version = u64::MAX, scroll_to = None ; garde sort/desc
    pub fn show(&mut self, ui: &mut Ui, tree: &Tree, selected: &mut Option<u32>, scanning: bool, now: u64) -> Option<u32>
}
```
- Champ `top: TopView` dans `App`, `TopView::new()` dans `App::new`. **`App::start_scan` appelle `self.top.reset()`** à côté de `tv.reset()` / `tm.reset()` (sans ça, des ids de l'ancien arbre seraient indexés dans le nouveau : classe du plantage F01 ; `Tree::new` repart de `version` 0, la clé seule ne suffit pas).
- Reconstruction en tête de `show`, règle en fonction pure testable :
  ```rust
  /// Vrai si `rows` doit être recalculé. Pendant le scan, au plus toutes les 500 ms.
  fn needs_rebuild(built_version: u64, version: u64, scanning: bool, since: Duration) -> bool
  // = built_version != version && (!scanning || since >= 500 ms)
  ```
  plus une **garde défensive** : si une ligne a `id as usize >= tree.nodes.len()`, reconstruire tout de suite (ne doit jamais arriver si `reset()` est appelé ; ne jamais indexer `tree.nodes` avec un id de `rows` avant cette garde).
- Reconstruire = `top_files(tree, TOP_N)`, puis pour chaque id : `rank` = position + 1 (rang par taille, **fixe** quel que soit le tri d'affichage), `dir` = `rel_dir(tree, id)`, puis appliquer le tri courant. `built_version = tree.version`, `built = Instant::now()`.
- La liste n'est calculée **que dans `show`**, donc seulement quand la vue Top est affichée. Revenir sur la vue après un changement de version reconstruit tout de suite hors scan, sous 500 ms pendant le scan.
- `rel_dir(tree, id) -> String` (fonction pure) : chemin du parent relatif à `tree.root_path`, composants joints par `\` (`tree.path(parent).strip_prefix(&tree.root_path)`, ou remontée des `parent` comme `Tree::path`) ; parent = racine → chaîne vide (affichée « (racine du scan) » en `MUTED`).

### Mise en page (panneau central)
Dans `App::ui`, nouvelle branche : `View::Top => { let r = self.top.show(ui, &self.tree, &mut self.selected, scanning, now); … }`, dans `Frame::new().inner_margin(Margin { left: 16, right: 16, top: 0, bottom: 12 })` comme la vue Carte.

1. **Ligne de titre** (`ui.horizontal`) : `RichText::new(format!("Plus gros fichiers ({n})")).strong()` où `n = rows.len()` ; à droite (ou à la suite), en `MUTED` 12 px : « sur disque » ou « taille logique » selon `tree.on_disk()`, et pendant `Scanning` (paramètre `scanning` de `show` ; pas de nouveau paramètre pour `Paused`) « — analyse en cours, liste provisoire ».
2. **En-tête de colonnes** (30 px, fond `BAR`, filet `BORDER`, textes 12 px, comme `TreeView::header`), cliquables pour trier :

   | Colonne | Contenu de la ligne | Alignement / largeur | Tri |
   |---|---|---|---|
   | « # » | `rank` | droite, 40 px, `mono(12.0)` `MUTED` | par rang (= par taille) |
   | « Nom » | pastille de fichier `kind_color(kind)` (rectangle 10×12 dessiné au `painter`, comme l'arbre) + `tree.name(id)` 13 px `TEXT` | gauche, 40 % de la largeur flexible, rogné (`with_clip_rect`) | `to_lowercase` |
   | « Dossier » | `dir` (ou « (racine du scan) ») 12 px `MUTED` | gauche, 60 % de la largeur flexible, rogné | `to_lowercase` |
   | « Sur disque » / « Taille » | `fmt::size(tree.size(id))`, `mono(12.0)` `TEXT` | droite, 110 px | taille |
   | « % » | `fmt::percent(100 × size / tree.size(0).max(1))`, `mono(12.0)` `MUTED` | droite, 64 px | taille |
   | « Modifié » | `fmt::date(mtime, now)` 12 px `MUTED` | droite, 104 px | `mtime` |
   | « Type » | `kind.label()` 12 px `MUTED` | droite, 96 px | ordre de `KINDS` |

   - Largeur flexible = largeur totale − colonnes fixes − marges (16) − espaces (8 entre colonnes), minimum 120 px pour Nom ; en dessous, Dossier rétrécit jusqu'à 0 avant Nom (même principe que `Layout` de l'arbre).
   - Marqueur de tri en **texte** comme l'arbre : « (desc.) » / « (asc.) » accolé au libellé. Clic sur la colonne courante → inverse le sens ; sur une autre → cette colonne, dans son sens initial : **#, Nom, Dossier, Type → ascendant ; Taille, %, Modifié → descendant**. Égalités : ordre par rang (stable). Le tri ne touche que les ≤ 100 lignes déjà calculées (pas de recalcul du top) ; il est conservé entre deux reconstructions et entre deux vues, réinitialisé seulement par le redémarrage (pas par `reset()`).
   - Infobulle de l'en-tête Taille : sur disque → « Espace occupé sur le disque. Un fichier à plusieurs liens physiques n'apparaît qu'une fois. Ctrl+T : taille logique » ; logique → « Taille logique. Un fichier à plusieurs liens physiques apparaît sous chacun de ses noms. Ctrl+T : taille sur disque ».
3. **Lignes** : `ScrollArea::vertical().id_salt("top_rows").auto_shrink([false, false]).show_rows(ui, 28.0, rows.len(), …)`, une ligne `allocate_exact_size(…, Sense::click())`, fond `SEL` si `Some(id) == *selected`, survol `Color32::from_white_alpha(8)`. Infobulle de ligne = chemin complet `tree.path(id)`.
4. **Pied** (une ligne `MUTED` 12 px sous la liste, hors `ScrollArea`) : « Total de ces {n} fichiers : {fmt::size(somme)} ({fmt::percent(somme / tree.size(0))} de l'analyse) ». Absent si `n == 0`.

Polices et glyphes : n'utiliser que des caractères latins déjà affichés dans l'application (lettres accentuées, « », …, —, #, %). **Aucune flèche ni emoji** (« ▲ ▼ ⬆ → 📄 » peuvent manquer dans les polices chargées : F08 a dû remplacer « ⬆ » par du texte). Les pastilles sont dessinées au `painter`. Tout glyphe hors de cette liste doit être vérifié à l'écran avant d'être gardé.

### Sélection, clic, double-clic, clavier
- **Clic** sur une ligne : `*selected = Some(id)` (sélection unique d'`App`, partagée). Le panneau Sélection de la colonne droite, Entrée (Ouvrir dans l'Explorateur, `/select,` sur le fichier), Copier le chemin (menu Édition) agissent sur ce fichier : code existant, rien à ajouter. Le clic ne déplie **pas** l'arbre (parcourir la liste ne doit pas ouvrir des dizaines de dossiers dans Détails).
- **Double-clic** : « révéler dans l'arbre ». `show` retourne `Some(id)` ; `App` fait alors `self.selected = Some(id)`, `self.tv.reveal(&self.tree, id)` et `action = Some(Action::SetView(View::Details))` (la vue Détails s'ouvre, ancêtres dépliés, ligne défilée et surlignée). Le double-clic n'ouvre **jamais** le fichier.
- La ligne sélectionnée ailleurs (arbre, treemap) est surlignée ici si elle est dans la liste ; sinon rien n'est surligné.
- **Clavier**, si aucun champ texte n'a le focus (`ui.memory(|m| m.focused().is_none())`) : Flèche Haut / Bas déplacent la sélection d'une ligne dans l'ordre d'affichage (sélection hors liste + Bas ou Haut → première ligne ; bornes : sans effet) et font défiler jusqu'à elle (`scroll_to`, même calcul que `TreeView`). Gauche / Droite : rien. Entrée : `OpenExplorer` existant. Pas de raccourci clavier pour « révéler » (hors périmètre).
- **Treemap** : sélectionner un fichier ici laisse le cadre `ACCENT` du treemap à sa logique existante (cadre si la cellule est dans la mise en page).

### Raccourci F9, onglet, menu
- F9 (sans modificateur, tous états de scan, même avec un champ focalisé) → vue Top : obtenu par l'ajout à `View::ALL`, aucun code spécifique dans `shortcuts`. Maj+F9 / Ctrl+F9 : rien.
- Onglet « Gros fichiers » en 4ᵉ position (généré par `views::tabs` depuis `ALL`), infobulle « 100 plus gros fichiers (F9) ».
- Menu Affichage : **supprimer** la ligne grisée `item(… "100 plus gros fichiers", "F9", None, true)` ; l'entrée est désormais produite par la boucle `view_item` (active, surlignée quand courante), à la même place (après Types, avant « Fichiers les plus anciens »).
- Filtre par nom : règle F07 inchangée (modifier le filtre hors Détails ramène en Détails). Le filtre ne s'applique **pas** à la liste Top.

### Pendant un scan, sans données, moins de 100 fichiers
- `Scanning` / `Paused` : la liste se remplit en direct (reconstruction au plus toutes les 500 ms pendant `Scanning` ; en `Paused`, `scanning == false` donc une reconstruction dès que la version change, ce qui n'arrive plus). Le scan n'est jamais touché.
- `Arrêter` : la liste reflète ce qui a été reçu.
- Avant tout scan ou racine de taille 0 (`tree.size(0) == 0`) : texte centré `MUTED` « Aucune donnée — lancez un scan » (même libellé que Carte et Types), ni en-tête ni pied.
- Scan avec des données mais aucun fichier de taille > 0 dans le mode courant (ex. : dossier de petits fichiers résidents MFT en taille sur disque) : en-tête de colonnes affiché, puis « Aucun fichier de taille non nulle dans ce mode d'affichage » centré `MUTED`.
- Moins de 100 fichiers éligibles : tous sont listés, le titre montre le nombre réel (« Plus gros fichiers (37) »).
- Nouveau scan, Rafraîchir, Dossier parent, puce, Ctrl+O : passent par `start_scan` → `top.reset()` → liste vide puis remplie avec le nouvel arbre. La vue reste `Top` (F07 : la vue n'est pas modifiée par un scan).
- Ctrl+T : `version` change → liste recalculée dans le nouveau mode (contenu et ordre peuvent changer, rangs recalculés), tri d'affichage conservé.

### Portée
- **Toute l'analyse** (racine du scan, id 0), toujours. Le zoom du treemap (F08) et la sélection ne restreignent pas la liste. Le titre ne mentionne pas de dossier.

## Hors périmètre

- Top limité au dossier zoomé ou sélectionné, nombre réglable (50 / 500), top des dossiers.
- Filtre par nom ou recherche dans la liste (F11), filtre par type.
- Sélection multiple (F12), menu contextuel (F13), Corbeille / suppression (F20, F21), Propriétés (F14).
- Export de la liste (F24+), persistance du tri (F17).
- Tas tenu à jour pendant l'ingestion, toute modification de `src/scan/` ou de `src/treemap_layout.rs`.
- Ouvrir le fichier avec son application associée (jamais, ni au double-clic ni à Entrée).

## Fichiers concernés

- `src/ui/mod.rs` : `pub mod top;`, variante `View::Top`.
- `src/ui/views.rs` : `ALL` à 4 éléments, branches `Top` de `label` / `tab` / `key` / `key_text` ; tests mis à jour (voir plus bas).
- `src/ui/top.rs` (nouveau) : `TOP_N`, `top_files`, `rel_dir`, `needs_rebuild`, `TopView` (`new`, `reset`, `show`, tri, en-tête, lignes, pied) ; tests.
- `src/ui/menu.rs` : suppression de l'entrée grisée « 100 plus gros fichiers ».
- `src/app.rs` : champ `top: TopView` ; `App::new` ; `start_scan` appelle `self.top.reset()` ; branche `View::Top` du panneau central ; double-clic → `selected`, `tv.reveal`, `Action::SetView(View::Details)`.
- `README.md`, `ROADMAP.md` (voir Critères de fin).

## Risques

- **Ids périmés (classe F01)** : `rows` contient des ids de l'arbre courant. `reset()` dans `start_scan` + garde `id < nodes.len()` en tête de `show`. Si F28 (ouvrir un scan) remplace l'arbre par une autre voie, il devra appeler `top.reset()` : le noter en commentaire d'une ligne à côté de l'appel dans `start_scan`.
- **Coût du recalcul** : mesurer avec le test ignoré `top_files_bench_1_3m` (voir plus bas). Au-delà de 30 ms en release, le noter dans « Limites » du README et passer `STALE_MS` à 1000 ; ne pas réécrire en incrémental ici.
- **`tree.path` par ligne** : n'est appelé qu'au survol (infobulle) et une fois par ligne à la reconstruction (`rel_dir`) ; ne pas l'appeler pour chaque ligne à chaque frame.
- **Égalités et rangs pendant le scan** : un fichier arrivé plus tard peut s'insérer et décaler les rangs ; normal, la liste est annoncée provisoire.
- **Ordre `KINDS` pour le tri Type** : utiliser `kind as u8` (l'enum est `#[repr(u8)]` dans l'ordre de `KINDS`).
- **Double-clic = deux clics** : la sélection change au premier clic (voulu), puis le second bascule en Détails ; vérifier qu'il n'y a pas de clignotement ni de double bascule.
- **Glyphes** : voir la contrainte de la section Mise en page ; vérifier à l'écran tout caractère non latin.
- **Touche F9 dans Windows** : pas de rôle système dans une fenêtre egui (F10 en a un, pas F9) ; vérifier avec le champ « Dossier » focalisé.

## Critères de fin

- `cargo build --release`, `cargo clippy --all-targets -- -D warnings` et `cargo test` passent sans avertissement.
- Tests unitaires, sans egui (arbre construit à la main avec `Tree::new` + `Tree::ingest`, comme dans `src/scan/tree.rs`) :
  - `src/ui/views.rs` :
    - `view_for_key_maps_f6_f7_f8` devient `view_for_key_maps_f6_to_f9` — F9 → `Some(View::Top)` en plus.
    - `view_for_key_ignores_other_keys` — remplacer `F9` par `F10` dans la liste (`F5`, `F10`, `F1`, `A` → `None`).
    - `view_keys_round_trip` — inchangé (couvre la 4ᵉ variante).
    - `side_cards_hide_the_central_one` — ajouter : `Top` → treemap et types à droite.
  - `src/ui/top.rs` `mod tests` :
    - `top_files_sorted_desc_files_only` — racine → `[A(dir), f1 300, f2 100]`, A → `[f3 200, f4 0]` → `top_files(t, 100)` = ids de `[f1, f3, f2]` (pas de dossier, pas de taille 0).
    - `top_files_keeps_n_largest` — 10 fichiers de tailles 1..=10 → `top_files(t, 3)` = ceux de 10, 9, 8 ; `top_files(t, 0)` vide.
    - `top_files_ties_by_smaller_id` — 4 fichiers de même taille (ids 1..4) → `top_files(t, 2) == [1, 2]`.
    - `top_files_matches_full_sort` — 10 000 fichiers répartis sur 100 dossiers, tailles pseudo-aléatoires (LCG déterministe, beaucoup d'égalités) → égal au tri complet `(Reverse(size), id)` tronqué à 100.
    - `top_files_skips_holes` — lot ingéré avec `first` au-delà de la fin (crée des `HOLE`) → aucun id de trou dans le résultat, pas de panique.
    - `top_files_follows_on_disk` — logique ≠ allouée (ex. a : 100 / 8192, b : 5000 / 4096) : sur disque → `[a, b]`, `set_on_disk(false)` → `[b, a]`.
    - `top_files_hard_links_once_on_disk` — deux noms de même `file_id` dans deux dossiers : sur disque → un seul des deux ; logique → les deux.
    - `rel_dir_relative_to_root` — fichier à la racine → `""` ; fichier sous `A\B` → `"A\\B"`.
    - `needs_rebuild_rules` — même version → faux ; version différente hors scan → vrai ; pendant le scan à 100 ms → faux, à 600 ms → vrai.
    - `reset_clears_rows_keeps_sort` — après une reconstruction et un tri par Nom, `reset()` → `rows` vide, `built_version == u64::MAX`, tri toujours Nom.
    - `top_files_bench_1_3m` — `#[ignore]` : arbre de 1 300 000 fichiers (13 000 dossiers × 100), mesure `top_files(t, 100)` avec `Instant`, affiche la durée (`println!`) ; assertion seulement sur la longueur (100). À lancer une fois : `cargo test --release top_files_bench -- --ignored --nocapture` ; reporter la durée dans le message de commit.
- Vérification manuelle (`cargo run --release -- C:\Users`, scans **en lecture seule**, ne jamais cliquer Corbeille / Nettoyer ; aucun fichier créé ni modifié hors d'un dossier neuf sous `%TEMP%`, supprimé à la fin) :
  1. Avant tout scan (lancer sans argument) : F9 → « Aucune donnée — lancez un scan ».
  2. Pendant le scan : F9 → la liste se remplit, titre « Plus gros fichiers (n) », mention « analyse en cours, liste provisoire » ; l'interface reste fluide ; le scan n'est ni interrompu ni mis en pause. La colonne droite garde treemap et Types.
  3. Fin du scan : 100 lignes, rangs 1 à 100, tailles décroissantes, % cohérents avec la racine, pied « Total de ces 100 fichiers : … ». Comparer les 3 premiers avec l'arbre (Détails) : mêmes fichiers, mêmes tailles.
  4. Tri : cliquer Nom, Dossier, Modifié, Type, # (deux fois chacun) → ordre et marqueur « (asc.) » / « (desc.) » corrects, rangs inchangés. F6 puis F9 → tri conservé.
  5. Clic sur une ligne → surlignée, panneau Sélection mis à jour ; Entrée → l'Explorateur s'ouvre avec le fichier sélectionné ; menu Édition > Copier le chemin → chemin complet dans le presse-papiers. F6 : l'arbre n'a pas été déplié par le clic.
  6. Flèches Haut / Bas → la sélection suit, la liste défile. Focus dans « Filtrer… » : les flèches ne bougent pas la sélection ; taper une lettre → retour en Détails (règle F07).
  7. Double-clic sur une ligne profonde → vue Détails, ancêtres dépliés, ligne visible et surlignée. Le fichier n'est pas ouvert.
  8. Ctrl+T → en-tête « Taille » / « Sur disque », valeurs et ordre recalculés, mention du mode dans le titre.
  9. Menu Affichage : « 100 plus gros fichiers » actif (F9), surligné dans la vue Top ; plus d'entrée grisée en double. Onglet « Gros fichiers » en 4ᵉ position.
  10. Focus dans « Dossier » : F9 change de vue sans modifier le texte.
  11. Vue Top affichée, puis Ctrl+R (scan terminé), une puce de lecteur, Ctrl+O : chaque nouveau scan vide puis remplit la liste, sans panique.
  12. Aucun carré vide (glyphe manquant) dans l'en-tête, les lignes ou le pied.
- Lignes à mettre à jour : statut F09 → `done` dans `ROADMAP.md` ; section « État » du `README.md`, « Fonctionnel » : ajouter « vue 100 plus gros fichiers (F9 : tri par colonne, sélection partagée, double-clic = révéler dans l'arbre, suit Ctrl+T) » ; « Limites » : tout point de Risques constaté (durée du recalcul au-delà de 30 ms, F9 avec un champ focalisé), ou « vue F09 : pas encore vérifiée à l'écran » si la vérification manuelle n'a pas pu être faite.
