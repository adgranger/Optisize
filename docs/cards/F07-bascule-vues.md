---
id: F07
title: Bascule des vues Détails / Carte / Types (F6–F8)
model: sonnet        # modèle qui implémente : sonnet | opus (voir la règle dans ROADMAP.md)
model_reason: Câblage d'interface borné (un enum de vue, trois branches d'affichage, menu, onglets, raccourcis) sur des widgets qui existent déjà ; rien ne touche au scan ni aux données.
depends_on: [F01]
---

## Contexte

Aujourd'hui la fenêtre a une seule disposition (`App::ui`, `src/app.rs`) :

- **panneau central** : puces de lecteurs (`drives::show`) puis tableau arborescent (`TreeView::show`, `src/ui/tree_view.rs`) ;
- **colonne droite** (`Panel::right("right")`, redimensionnable 360–900 px, 480 par défaut) : treemap « Carte de l'espace » (`TreemapView::show`, `src/ui/treemap.rs`, hauteur 45 % de la fenêtre bornée 260–620, bascule interne Dossiers / Types = `Mode`), barres « Par type de fichier » (`ui::types::show`), carte « Nettoyage suggéré », panneau de sélection.

Le menu **Affichage** (`src/ui/menu.rs`) montre déjà « Détails (arborescence) » F6, « Carte (treemap) » F7, « Types de fichiers » F8, grisés (`action = None`). Aucune touche F6–F8 n'est gérée. Le treemap, coincé dans la colonne droite, est trop petit pour être lu sur un gros disque ; c'est le besoin principal (TreeSize : onglets Détails / Carte / Types sous le ruban).

Raccourcis existants à ne pas casser (`App::shortcuts`) : F5 (Scanner, hors scan), Échap (Arrêter), Ctrl+O, Ctrl+T, Ctrl+R, Alt+Haut, Espace (pause), Entrée (Explorateur). F9 est réservé à F09 (« 100 plus gros fichiers », entrée déjà grisée dans le menu), F1 à F18 (aide).

## Comportement attendu

### Modèle : `View`
Dans `src/ui/mod.rs` :
```rust
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum View { Details, Map, Types }

impl View {
    /// Ordre des onglets et du menu. F09/F10 ajouteront leurs variantes ici.
    pub const ALL: [View; 3] = [View::Details, View::Map, View::Types];
    pub fn label(self) -> &'static str      // menu : « Détails (arborescence) », « Carte (treemap) », « Types de fichiers »
    pub fn tab(self) -> &'static str        // onglet : « Détails », « Carte », « Types »
    pub fn key(self) -> egui::Key           // F6, F7, F8
    pub fn key_text(self) -> &'static str   // « F6 », « F7 », « F8 »
    pub fn for_key(k: egui::Key) -> Option<View>  // inverse de key() sur ALL ; None pour toute autre touche
    pub fn side_treemap(self) -> bool       // colonne droite montre le treemap : vrai sauf Map
    pub fn side_types(self) -> bool         // colonne droite montre les barres Types : vrai sauf Types
}
```
- Nouvelle `Action::SetView(View)` (`Action` reste `Copy`). `Flags` gagne `view: View`.
- `App` gagne `view: View`, initialisé à `View::Details`. **Non persisté** entre deux lancements (F17), **non modifié** par un nouveau scan, Rafraîchir, Arrêter ou Dossier parent.

### Ce que montre chaque vue
La barre de menus, la barre d'outils, la barre d'état et la colonne droite restent dans toutes les vues. Seul le contenu du panneau central change ; la colonne droite masque la carte qui est passée au centre, pour ne jamais dessiner le même composant deux fois.

| Vue | Panneau central (sous les onglets et les puces) | Colonne droite |
|---|---|---|
| Détails (F6) | `TreeView::show`, inchangé | treemap + Types + Nettoyage + Sélection (exactement l'existant) |
| Carte (F7) | `TreemapView::show` sur toute la place restante | Types + Nettoyage + Sélection |
| Types (F8) | `ui::types::show` en version complète (voir plus bas) | treemap + Nettoyage + Sélection |

- La vue Détails doit être **pixel pour pixel** la disposition actuelle (aucune régression visuelle).
- Carte : hauteur passée à `TreemapView::show` = `(ui.available_height() - 32.0).max(200.0)` (32 = en-tête « Carte de l'espace » + espacement), après une marge `Margin::symmetric(16, 0)` à gauche/droite et 12 en bas, comme le tableau. Le titre « Carte de l'espace » et la bascule Dossiers / Types (`Mode`) restent dans l'en-tête du treemap.
- Types (vue complète) : `ui::types::show(ui, tree, full: bool)`.
  - `full = false` (colonne droite) : rendu actuel, inchangé.
  - `full = true` (centre, marges 16) : titre « Par type de fichier », une ligne de 28 px par type non vide (pastille, libellé, barre, taille, puis pourcentage du total en `fmt::percent`, colonne de 64 px alignée à droite en `mono(12.0)`, couleur `MUTED`), puis une ligne « Total » (taille totale en `mono(12.0)`, `TEXT`). Le total = somme des tailles des types affichés (suit Ctrl+T comme `type_size`).
  - Aucune donnée (liste vide) : en `full = true`, texte centré `MUTED` « Aucune donnée — lancez un scan » (même libellé que le treemap) ; en `full = false`, titre seul comme aujourd'hui.
  - Le calcul de la liste passe par une fonction pure `pub fn kind_rows(tree: &Tree) -> Vec<(Kind, u64)>` (types de taille > 0, triés par taille décroissante, à égalité dans l'ordre de `KINDS`), utilisée par `types::show` **et** par `TreemapView::types` (qui duplique aujourd'hui ce calcul).
  - Pas de compteur de fichiers par type : l'arbre n'en tient pas (hors périmètre, toucherait `src/scan/`).

### Onglets
- En tête du panneau central, dans le `Frame` qui contient déjà les puces (`inner_margin(16, 12)`), **au-dessus** des puces : une ligne `ui.horizontal` de trois `ui.selectable_label(self.view == v, v.tab())`, dans l'ordre de `View::ALL`, chacune avec l'infobulle `format!("{} ({})", v.label(), v.key_text())`. Un clic émet `Action::SetView(v)` (appliqué en fin de frame comme les autres actions).
- Les onglets sont dessinés par une fonction `ui::views::tabs(ui, current: View) -> Option<Action>` dans un nouveau fichier `src/ui/views.rs` (qui porte aussi `impl View` si l'implémenteur préfère ; `View` reste exporté par `ui::View`).

### Menu Affichage
- Les trois entrées existantes deviennent actives : action `Some(Action::SetView(v))`, toujours activées (même pendant un scan), raccourci affiché `v.key_text()`, et la vue courante est marquée par `Button::selected(true)` (egui 0.37). Ajouter à `menu.rs` une variante de `item` (`view_item(ui, o, f, v)`) qui construit le bouton avec `.selected(f.view == v)` ; ne pas changer la signature de `item`.
- Libellés et ordre inchangés, générés depuis `View::ALL` (`v.label()`). « 100 plus gros fichiers » (F9), « Fichiers les plus anciens », « Doublons » restent grisés.

### Raccourcis F6 / F7 / F8
- Dans `App::shortcuts`, **en premier test** : `View::ALL` parcouru, `i.key_pressed(v.key())` sans modificateur (`i.modifiers.is_none()`) → `Some(Action::SetView(v))`.
- Actifs dans tous les états (`Idle`, `Scanning`, `Paused`, `Done`, `Stopped`) et **même si un champ texte a le focus** (les touches F ne tapent rien ; ne pas retirer le focus du champ).
- Touche de la vue déjà affichée : sans effet (pas de toast, pas de défilement).
- F5 et F9 : comportement inchangé (F5 = Scanner hors scan ; F9 ne fait rien jusqu'à F09). Maj+F6 / Ctrl+F6 : rien.
- Répétition de touche maintenue : idempotent.

### Changement de vue (`App::set_view(v)`, appelée par `run(Action::SetView(v))`)
- Si `v == self.view` : rien.
- Sinon `self.view = v`. Si `v == View::Details` et `self.selected == Some(id)` : `self.tv.scroll_to_row(id)` — nouvelle méthode publique de `TreeView` qui pose seulement `scroll_to = Some(id)` (**ne déplie rien** : un dossier replié par l'utilisateur reste replié ; si la ligne n'est pas visible, rien ne défile).
- Aucun toast.

### Sélection partagée
- `App::selected` reste l'unique sélection, commune à toutes les vues ; le panneau de sélection et les actions Entrée / Ctrl+C / Ouvrir dans l'Explorateur fonctionnent dans les trois vues.
- Clic dans le treemap (au centre ou à droite) : code existant inchangé (`selected = id`, `tv.reveal(id)`) ; le retour en Détails montre donc la ligne dépliée et défilée.
- Le cadre `ACCENT` du treemap entoure la sélection dans les deux emplacements (existant).
- La vue Types ne change pas la sélection (les lignes ne sont pas cliquables).

### État conservé
- `TreeView` (dossiers dépliés, colonne et sens de tri) et `TreemapView` (`mode` Dossiers / Types) sont des champs d'`App` qui ne sont ni recréés ni `reset()` au changement de vue. **Un seul** `TreemapView` : celui du centre et celui de droite sont le même objet (jamais affichés ensemble) ; son cache `cells` se recalcule de lui-même quand la taille change (clé `(w, h, version)`).
- Défilement de l'arbre : donner au `ScrollArea` de `TreeView::show` un identifiant stable `.id_salt("tree_rows")` ; egui conserve son état (`insert_persisted`) même quand il n'est pas dessiné pendant plusieurs frames. Retour en Détails = même position (sauf `scroll_to_row` ci-dessus, qui ne défile que si la sélection est hors champ).
- Colonne droite : même `Panel::right("right")` dans toutes les vues → largeur choisie par l'utilisateur conservée.
- Filtre par nom : il ne s'applique qu'à la vue Détails. Si le texte du filtre **change** (`response.changed()`) alors que la vue n'est pas Détails, la barre d'outils émet `Action::SetView(View::Details)` (elle lit `f.view`) ; le focus reste dans le champ. Basculer vers Carte avec un filtre non vide le laisse tel quel ; il s'applique de nouveau au retour.

### Pendant un scan et sans données
- Les trois vues se remplissent en direct, avec les limitations existantes (arbre reconstruit au plus toutes les 150 ms, treemap au plus toutes les 250 ms pendant `Scanning`).
- Changer de vue pendant `Scanning` ou `Paused` ne touche pas au scan.
- Avant tout scan (`tree.size(0) == 0`) : Détails montre la racine seule (existant), Carte affiche « Aucune donnée — lancez un scan » (existant), Types affiche le même message (nouveau, voir plus haut).

### Clavier et focus
- Flèches Haut / Bas / Gauche / Droite : navigation de l'arbre **uniquement en vue Détails** (elle vit dans `TreeView::show`, qui n'est pas appelé dans les autres vues). En Carte et Types, les flèches ne font rien (F08 pourra les utiliser).
- Entrée, Ctrl+C (via menu), Alt+Haut, Ctrl+O, Ctrl+T, Ctrl+R, F5, Échap, Espace : inchangés et actifs dans toutes les vues.
- « Tout développer » / « Tout réduire » restent actifs dans toutes les vues ; ils modifient l'état de l'arbre, visible au retour en Détails.

## Hors périmètre

- Vue « 100 plus gros fichiers » / F9 (F09), vue par âge (F10), doublons : ne rien ajouter à `View` pour elles. La conception doit seulement les accueillir : nouvelle variante + entrée dans `View::ALL` + une branche dans le `match self.view` du panneau central.
- Zoom du treemap, fil d'Ariane, double-clic (F08) : la vue Carte affiche la racine comme aujourd'hui. F08 zoomera le `TreemapView` unique ; ne pas introduire de second treemap.
- Persistance de la vue choisie (F17), plein écran sans colonne droite, colonne droite repliable, réorganisation des onglets.
- Compteurs de fichiers par type, clic sur un type pour filtrer (F11).
- Raccourci « Tout développer » au clavier (Ctrl+Maj+Droite est affiché dans le menu mais non câblé : non traité ici).
- Toute modification de `src/scan/` ou `src/treemap_layout.rs`.

## Fichiers concernés

- `src/ui/mod.rs` : `pub mod views;`, `enum View` (+ `impl View`, ici ou dans `views.rs`), `Action::SetView(View)`, `Flags::view`.
- `src/ui/views.rs` (nouveau) : `pub fn tabs(ui: &mut Ui, current: View) -> Option<Action>` ; tests de `View`.
- `src/ui/menu.rs` : `view_item` ; les trois entrées F6–F8 générées depuis `View::ALL`.
- `src/ui/toolbar.rs` : filtre modifié hors Détails → `Action::SetView(View::Details)`.
- `src/ui/types.rs` : `pub fn kind_rows(tree) -> Vec<(Kind, u64)>` ; `show(ui, tree, full: bool)`.
- `src/ui/treemap.rs` : `TreemapView::types` utilise `types::kind_rows` ; sinon inchangé.
- `src/ui/tree_view.rs` : `pub fn scroll_to_row(&mut self, id: u32)` ; `.id_salt("tree_rows")` sur le `ScrollArea`.
- `src/app.rs` :
  - champ `view: View` ; `Flags { …, view: self.view }` ;
  - `shortcuts` : F6–F8 en premier ;
  - `run` : `Action::SetView(v) => self.set_view(v)` ;
  - `ui` : colonne droite conditionnée par `self.view.side_treemap()` / `side_types()` ; panneau central = onglets + puces, puis `match self.view { Details => tv.show, Map => tm.show(plein), Types => types::show(full) }`. `clicked` (treemap) est traité de la même façon quel que soit l'emplacement.
- `README.md`, `ROADMAP.md` (voir Critères de fin).

## Risques

- **Régression de la vue Détails** : la vue par défaut doit rester identique ; comparer une capture avant / après (même dossier, même taille de fenêtre).
- **Identifiants egui** : ajouter les onglets avant les puces décale les identifiants automatiques des widgets suivants du panneau central. Sans incidence connue (aucun état n'y est attaché hormis le `ScrollArea`, d'où `id_salt`), mais vérifier que le tri et la position de défilement survivent à F7 puis F6.
- **Treemap grand format** : à 1920×1080 la carte centrale fait ~1300×800 px ; `layout_nested` est déjà en O(n) sur les nœuds visibles et limité à 250 ms pendant le scan. Vérifier à l'œil sur `C:\Users` pendant un scan que l'UI reste fluide ; si elle saccade, le noter dans le README, ne pas optimiser ici (F08 / F35).
- **F6 dans Windows** : F6 n'a pas de rôle système dans une fenêtre winit/egui (il ne déplace pas le focus) ; à vérifier manuellement avec le champ « Dossier » focalisé.
- **Touches F et `TextEdit`** : `key_pressed(F6)` reste vu par `ctx.input` même quand un `TextEdit` a le focus (egui ne consomme pas les touches F). À vérifier ; si ce n'est pas le cas, le noter et limiter F6–F8 à « aucun champ focalisé » plutôt que de contourner.

## Critères de fin

- `cargo build --release`, `cargo clippy --all-targets -- -D warnings` et `cargo test` passent sans avertissement.
- Tests unitaires :
  - `src/ui/views.rs` `mod tests` :
    - `view_for_key_maps_f6_f7_f8` — `for_key(F6/F7/F8)` → `Details/Map/Types`.
    - `view_for_key_ignores_other_keys` — `F5`, `F9`, `F1`, `A` → `None`.
    - `view_keys_round_trip` — pour chaque `v` de `ALL`, `for_key(v.key()) == Some(v)` ; clés, `label` et `tab` deux à deux distincts.
    - `side_cards_hide_the_central_one` — Details : treemap et types à droite ; Map : pas de treemap, types oui ; Types : treemap oui, pas de types.
  - `src/ui/types.rs` `mod tests` (arbre construit à la main avec `Tree::ingest`, comme dans `src/scan/tree.rs`) :
    - `kind_rows_sorted_desc_without_empty` — fichiers `.mp4` (gros) et `.zip` (petit) → `[(Kind::Video, …), (Kind::Archive, …)]` dans cet ordre, aucun type à 0.
    - `kind_rows_empty_tree` — `Tree::new` sans ingestion → vide.
    - `kind_rows_follows_on_disk` — tailles logique ≠ allouée ; `set_on_disk(false)` puis `true` change les valeurs comme `type_size`.
  - `src/ui/tree_view.rs` `mod tests` : `scroll_to_row_keeps_expanded` — `TreeView::new()`, `scroll_to_row(5)` → `scroll_to == Some(5)` et `expanded` toujours `{0}`.
- Vérification manuelle (`cargo run --release -- C:\Users`, scans en lecture seule, ne jamais cliquer Corbeille / Nettoyer) :
  1. Au lancement : onglet « Détails » actif, disposition identique à avant F07.
  2. Pendant le scan : F7 → treemap au centre qui se remplit, plus de treemap à droite (barres Types, Nettoyage, Sélection restent). F8 → liste Types au centre avec % et Total, treemap revenu à droite. F6 → retour à l'arbre. Le scan n'est ni interrompu ni mis en pause.
  3. Même chose par les onglets et par le menu Affichage (l'entrée de la vue courante est surlignée, F9 toujours grisé).
  4. En Détails : trier par Fichiers, déplier deux niveaux, défiler vers le bas, sélectionner une ligne. F7 puis F6 : tri, dépliage, position de défilement et sélection identiques.
  5. En Carte : basculer le treemap en mode « Types », F6 puis F7 → toujours en mode Types. Repasser en Dossiers, cliquer un rectangle → panneau de sélection mis à jour ; F6 → la ligne est dépliée, visible et surlignée.
  6. Focus dans « Filtrer… » : F7 → la vue change, le curseur reste dans le champ. Taper une lettre → retour automatique en Détails avec le filtre appliqué.
  7. Focus dans « Dossier » : F6/F7/F8 changent de vue sans modifier le texte. F5 (hors scan) relance toujours le scan ; F9 ne fait rien.
  8. Avant tout scan (lancer sans argument) : F7 et F8 affichent « Aucune donnée — lancez un scan ».
  9. Redimensionner la colonne droite en Détails, passer en Carte puis revenir : largeur conservée.
  10. Ctrl+T dans chaque vue : arbre, treemap central et liste Types changent de valeurs.
- Lignes à mettre à jour : statut F07 → `done` dans `ROADMAP.md` ; section « État » du `README.md`, « Fonctionnel » : ajouter « vues Détails / Carte / Types (F6–F8, onglets, menu Affichage ; sélection, tri et défilement conservés) » ; « Limites » : tout point de Risques constaté (fluidité du grand treemap, touches F dans un champ).
